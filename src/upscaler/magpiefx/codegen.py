"""
HLSL code generation for MagpieFX passes.

For every pass a self-contained compute shader is generated, mirroring
Magpie's ``GeneratePassSource``:

    * ``MF*`` type macros (always FP32 here) and ``MP_*`` macros,
    * the shared ``cbuffer`` with built-in sizes and the effect parameters,
    * SRV/UAV/sampler declarations with explicit registers,
    * the built-in helpers (``GetInputSize``, ``Rmp8x8``, ``MulAdd``...),
    * the COMMON blocks and the pass code,
    * a ``__M`` entry point with ``[numthreads]`` that calls ``PassN``.

Register layout: ``b0`` constants, ``t0..`` inputs, ``u0..`` outputs,
``s0..`` samplers. The compiler shifts them to the bindings used by the
Vulkan extension (0 / 1024 / 2048 / 3072).
"""

from __future__ import annotations

from typing import List

from .formats import FORMATS
from .parser import EffectDesc, EffectPass

_NUMBERS = ("1", "2", "3", "4")

_BUILTINS = """\
uint __Bfe(uint src, uint off, uint bits) { uint mask = (1u << bits) - 1; return (src >> off) & mask; }
uint __BfiM(uint src, uint ins, uint bits) { uint mask = (1u << bits) - 1; return (ins & mask) | (src & (~mask)); }
uint2 Rmp8x8(uint a) { return uint2(__Bfe(a, 1u, 3u), __BfiM(__Bfe(a, 3u, 3u), a, 1u)); }
uint2 GetInputSize() { return __inputSize; }
float2 GetInputPt() { return __inputPt; }
uint2 GetOutputSize() { return __outputSize; }
float2 GetOutputPt() { return __outputPt; }
float2 GetScale() { return __scale; }
"""


def _muladd_source() -> str:
    """``MulAdd`` overloads written with ``mad`` like Magpie does."""
    comps = "xyzw"
    out: List[str] = []
    for n in (2, 3, 4):
        for m in (2, 3, 4):
            out.append(f"MF{m} MulAdd(MF{n} x, MF{n}x{m} y, MF{m} a) {{")
            out.append(f"\tMF{m} result = a;")
            for r in range(n):
                swz = "_".join(f"m{r}{c}" for c in range(m))
                out.append(f"\tresult = mad(x.{comps[r]}, y._{swz}, result);")
            out.append("\treturn result;")
            out.append("}")
    return "\n".join(out) + "\n"


def constant_buffer_layout(desc: EffectDesc) -> List[tuple]:
    """
    Return the cbuffer members as ``(hlsl_type, name)`` in declaration order.

    All members are 2-component or scalar 32-bit values, so with DX packing
    they are tightly packed and the CPU side can write consecutive 4-byte
    words (see :meth:`MagpieEffect._pack_constants`).
    """
    members = [
        ("uint2", "__inputSize"),
        ("uint2", "__outputSize"),
        ("float2", "__inputPt"),
        ("float2", "__outputPt"),
        ("float2", "__scale"),
    ]
    for p in desc.passes[:-1]:
        if p.ps_style:
            members.append(("uint2", f"__pass{p.index}OutputSize"))
            members.append(("float2", f"__pass{p.index}OutputPt"))
    for param in desc.params:
        members.append((param.type, param.name))
    if desc.use_dynamic:
        members.append(("uint", "__frameCount"))
    return members


def _macros(desc: EffectDesc, p: EffectPass) -> List[str]:
    lines = [
        f"#define MP_BLOCK_WIDTH {p.block_size[0]}",
        f"#define MP_BLOCK_HEIGHT {p.block_size[1]}",
        f"#define MP_NUM_THREADS_X {p.num_threads[0]}",
        f"#define MP_NUM_THREADS_Y {p.num_threads[1]}",
        f"#define MP_NUM_THREADS_Z {p.num_threads[2]}",
    ]
    if p.ps_style:
        lines.append("#define MP_PS_STYLE")
    # Explicit storage image formats; glslang does not know the attribute
    # and infers the format from the texel type instead.
    lines += [
        "#ifdef __hlsl_dx_compiler",
        "#define MP_IMAGE_FORMAT(f) [[vk::image_format(f)]]",
        "#else",
        "#define MP_IMAGE_FORMAT(f)",
        "#endif",
    ]
    # FP16 is not implemented: MF types are always 32-bit floats
    lines.append("#define MF float")
    for i in _NUMBERS:
        lines.append(f"#define MF{i} float{i}")
        for j in _NUMBERS:
            lines.append(f"#define MF{i}x{j} float{i}x{j}")
    return lines


def _ps_entry(desc: EffectDesc, p: EffectPass) -> str:
    last = p.index == len(desc.passes)
    size = "__outputSize" if last else f"__pass{p.index}OutputSize"
    pt = "__outputPt" if last else f"__pass{p.index}OutputPt"

    if len(p.outputs) <= 1:
        out_name = desc.textures[p.outputs[0]].name
        call = f"{out_name}[gxy] = Pass{p.index}(pos);"
    else:
        decls = "".join(
            f"\t{FORMATS[desc.textures[o].format].srv_type} c{i};\n"
            for i, o in enumerate(p.outputs)
        )
        args = ", ".join(f"c{i}" for i in range(len(p.outputs)))
        stores = " ".join(
            f"{desc.textures[o].name}[gxy] = c{i};" for i, o in enumerate(p.outputs)
        )
        call = f"{{\n{decls}\tPass{p.index}(pos, {args});\n\t{stores}\n\t}}"

    return f"""[numthreads(64, 1, 1)]
void __M(uint3 tid : SV_GroupThreadID, uint3 gid : SV_GroupID) {{
	uint2 gxy = (gid.xy << 4u) + Rmp8x8(tid.x);
	if (gxy.x >= {size}.x || gxy.y >= {size}.y) {{
		return;
	}}
	float2 pos = (gxy + 0.5f) * {pt};
	float2 step = 8 * {pt};

	{call}

	gxy.x += 8u;
	pos.x += step.x;
	if (gxy.x < {size}.x && gxy.y < {size}.y) {{
		{call}
	}}

	gxy.y += 8u;
	pos.y += step.y;
	if (gxy.x < {size}.x && gxy.y < {size}.y) {{
		{call}
	}}

	gxy.x -= 8u;
	pos.x -= step.x;
	if (gxy.x < {size}.x && gxy.y < {size}.y) {{
		{call}
	}}
}}
"""


def _cs_entry(p: EffectPass) -> str:
    bw, bh = p.block_size
    if bw == bh and bw & (bw - 1) == 0:
        block_start = f"(gid.xy << {bw.bit_length() - 1})"
    else:
        block_start = f"gid.xy * uint2({bw}, {bh})"
    nx, ny, nz = p.num_threads
    return f"""[numthreads({nx}, {ny}, {nz})]
void __M(uint3 tid : SV_GroupThreadID, uint3 gid : SV_GroupID) {{
	Pass{p.index}({block_start}, tid);
}}
"""


def generate_pass_source(desc: EffectDesc, pass_index: int) -> str:
    """Return the full HLSL source of pass *pass_index* (1-based)."""
    p = desc.passes[pass_index - 1]
    parts: List[str] = []

    parts.append("\n".join(_macros(desc, p)) + "\n")

    cb = ["cbuffer __CB1 : register(b0) {"]
    cb += [f"\t{t} {n};" for t, n in constant_buffer_layout(desc)]
    cb.append("};\n")
    parts.append("\n".join(cb))

    decls: List[str] = []
    for i, t in enumerate(p.inputs):
        tex = desc.textures[t]
        decls.append(
            f"Texture2D<{FORMATS[tex.format].srv_type}> {tex.name} : register(t{i});"
        )
    for i, t in enumerate(p.outputs):
        tex = desc.textures[t]
        fmt = FORMATS[tex.format]
        decls.append(
            f'MP_IMAGE_FORMAT("{fmt.spirv_format}") '
            f"RWTexture2D<{fmt.uav_type}> {tex.name} : register(u{i});"
        )
    for i, s in enumerate(desc.samplers):
        decls.append(f"SamplerState {s.name} : register(s{i});")
    parts.append("\n".join(decls) + "\n")

    parts.append(_BUILTINS)
    if desc.use_dynamic:
        parts.append("uint GetFrameCount() { return __frameCount; }\n")
    if desc.use_muladd:
        parts.append(_muladd_source())

    for common in desc.commons:
        parts.append(common.rstrip("\n") + "\n")

    parts.append(p.code.rstrip("\n") + "\n")
    parts.append(_ps_entry(desc, p) if p.ps_style else _cs_entry(p))
    return "\n".join(parts)
