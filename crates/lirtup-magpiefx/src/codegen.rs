//! HLSL generation, one compute shader per pass (`codegen.py`).
//!
//! Mirrors Magpie's `GeneratePassSource`: the `MF*` type macros
//! (`min16float` with FP16, `float` otherwise) and `MP_*` macros, the shared
//! `cbuffer` with the built-in sizes and the parameters, the SRV/UAV/sampler
//! declarations with explicit registers, the built-in helpers
//! (`GetInputSize`, `Rmp8x8`, `MulAdd`, ...), the COMMON blocks and the pass
//! code, and a `__M` entry point that calls `PassN`.
//!
//! Registers: `b0` constants, `t0..` inputs, `u0..` outputs, `s0..` samplers;
//! the compiler shifts them to the bindings of `lirtup-gpu` (0 / 1024 / 2048 /
//! 3072). The output is byte-for-byte the 1.x source, so the SPIR-V cache
//! keys of both versions describe the same shaders.

use crate::EffectDesc;
use crate::formats;
use crate::parser::EffectPass;

/// One entry of the constant buffer: name, HLSL type, byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CbEntry {
    /// Name.
    pub name: String,
    /// HLSL type (`float`, `int`, `uint2`, ...).
    pub ty: String,
    /// Byte offset.
    pub offset: u32,
}

const BUILTINS: &str = "\
uint __Bfe(uint src, uint off, uint bits) { uint mask = (1u << bits) - 1; return (src >> off) & mask; }
uint __BfiM(uint src, uint ins, uint bits) { uint mask = (1u << bits) - 1; return (ins & mask) | (src & (~mask)); }
uint2 Rmp8x8(uint a) { return uint2(__Bfe(a, 1u, 3u), __BfiM(__Bfe(a, 3u, 3u), a, 1u)); }
uint2 GetInputSize() { return __inputSize; }
float2 GetInputPt() { return __inputPt; }
uint2 GetOutputSize() { return __outputSize; }
float2 GetOutputPt() { return __outputPt; }
float2 GetScale() { return __scale; }
";

/// `MulAdd` overloads written with `mad`, like Magpie.
fn muladd_source() -> String {
    let comps = ['x', 'y', 'z', 'w'];
    let mut out = Vec::new();
    for n in 2..=4 {
        for m in 2..=4 {
            out.push(format!("MF{m} MulAdd(MF{n} x, MF{n}x{m} y, MF{m} a) {{"));
            out.push(format!("\tMF{m} result = a;"));
            for (r, comp) in comps.iter().enumerate().take(n) {
                let swz: Vec<String> = (0..m).map(|c| format!("m{r}{c}")).collect();
                out.push(format!(
                    "\tresult = mad(x.{}, y._{}, result);",
                    comp,
                    swz.join("_")
                ));
            }
            out.push("\treturn result;".to_owned());
            out.push("}".to_owned());
        }
    }
    out.join("\n") + "\n"
}

/// Layout of the generated constant buffer (sizes, `FrameCount`, then the
/// parameters).
///
/// Every member is a 2-component or scalar 32-bit value and the pairs come
/// first, so with DX packing they are tightly packed (no member crosses a
/// 16-byte boundary) and the CPU writes consecutive 4-byte words.
pub fn constant_buffer_layout(desc: &EffectDesc) -> Vec<CbEntry> {
    let mut members: Vec<(&str, String)> = vec![
        ("uint2", "__inputSize".to_owned()),
        ("uint2", "__outputSize".to_owned()),
        ("float2", "__inputPt".to_owned()),
        ("float2", "__outputPt".to_owned()),
        ("float2", "__scale".to_owned()),
    ];
    if let Some((_, rest)) = desc.passes.split_last() {
        for p in rest.iter().filter(|p| p.ps_style) {
            members.push(("uint2", format!("__pass{}OutputSize", p.index)));
            members.push(("float2", format!("__pass{}OutputPt", p.index)));
        }
    }
    for param in &desc.params {
        members.push((param.kind.hlsl(), param.name.clone()));
    }
    if desc.use_dynamic {
        members.push(("uint", "__frameCount".to_owned()));
    }
    let mut offset = 0;
    members
        .into_iter()
        .map(|(ty, name)| {
            let entry = CbEntry {
                name,
                ty: ty.to_owned(),
                offset,
            };
            offset += if ty.ends_with('2') { 8 } else { 4 };
            entry
        })
        .collect()
}

/// The `MF` type macros, as in Magpie: `min16float` when the effect runs in
/// FP16 (the compiler then gets 16-bit types, so they are real 16-bit
/// floats) and `float` otherwise.
pub fn mf_type_macros(fp16: bool) -> Vec<String> {
    let base = if fp16 { "min16float" } else { "float" };
    let mut lines = Vec::new();
    if fp16 {
        lines.push("#define MP_FP16".to_owned());
    }
    lines.push(format!("#define MF {base}"));
    for i in 1..=4 {
        lines.push(format!("#define MF{i} {base}{i}"));
        for j in 1..=4 {
            lines.push(format!("#define MF{i}x{j} {base}{i}x{j}"));
        }
    }
    lines
}

fn macros(p: &EffectPass, fp16: bool) -> Vec<String> {
    let mut lines = vec![
        format!("#define MP_BLOCK_WIDTH {}", p.block_size.0),
        format!("#define MP_BLOCK_HEIGHT {}", p.block_size.1),
        format!("#define MP_NUM_THREADS_X {}", p.num_threads.0),
        format!("#define MP_NUM_THREADS_Y {}", p.num_threads.1),
        format!("#define MP_NUM_THREADS_Z {}", p.num_threads.2),
    ];
    if p.ps_style {
        lines.push("#define MP_PS_STYLE".to_owned());
    }
    // Explicit storage image formats; glslang does not know the attribute
    // and infers the format from the texel type instead.
    for l in [
        "#ifdef __hlsl_dx_compiler",
        "#define MP_IMAGE_FORMAT(f) [[vk::image_format(f)]]",
        "#else",
        "#define MP_IMAGE_FORMAT(f)",
        "#endif",
    ] {
        lines.push(l.to_owned());
    }
    lines.extend(mf_type_macros(fp16));
    lines
}

/// Texel type of a `Texture2D`; `float4` when the format is unknown.
fn srv_type(format: &str) -> &'static str {
    formats::info(format).map(|f| f.srv_type).unwrap_or("float4")
}

/// Texel type of a declaration: always 32-bit floats, because SPIR-V for
/// Vulkan cannot sample or store 16-bit texels (dxc rejects
/// `Texture2D<min16float4>`).
fn resource_type(texel: &str) -> String {
    texel.replace("MF", "float")
}

fn ps_entry(desc: &EffectDesc, p: &EffectPass) -> String {
    let last = p.index as usize == desc.passes.len();
    let (size, pt) = if last {
        ("__outputSize".to_owned(), "__outputPt".to_owned())
    } else {
        (
            format!("__pass{}OutputSize", p.index),
            format!("__pass{}OutputPt", p.index),
        )
    };
    let call = if p.outputs.len() <= 1 {
        format!(
            "{}[gxy] = Pass{}(pos);",
            desc.textures[p.outputs[0] as usize].name, p.index
        )
    } else {
        let decls: String = p
            .outputs
            .iter()
            .enumerate()
            .map(|(i, o)| format!("\t{} c{i};\n", srv_type(&desc.textures[*o as usize].format)))
            .collect();
        let args: Vec<String> = (0..p.outputs.len()).map(|i| format!("c{i}")).collect();
        let stores: Vec<String> = p
            .outputs
            .iter()
            .enumerate()
            .map(|(i, o)| format!("{}[gxy] = c{i};", desc.textures[*o as usize].name))
            .collect();
        format!(
            "{{\n{decls}\tPass{}(pos, {});\n\t{}\n\t}}",
            p.index,
            args.join(", "),
            stores.join(" ")
        )
    };
    format!(
        "[numthreads(64, 1, 1)]
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
"
    )
}

fn cs_entry(p: &EffectPass) -> String {
    let (bw, bh) = p.block_size;
    let block_start = if bw == bh && bw.is_power_of_two() {
        format!("(gid.xy << {})", bw.trailing_zeros())
    } else {
        format!("gid.xy * uint2({bw}, {bh})")
    };
    let (nx, ny, nz) = p.num_threads;
    format!(
        "[numthreads({nx}, {ny}, {nz})]
void __M(uint3 tid : SV_GroupThreadID, uint3 gid : SV_GroupID) {{
	Pass{}({block_start}, tid);
}}
",
        p.index
    )
}

/// The HLSL of pass `pass_index` (0-based), FP16 variant or not.
///
/// # Panics
///
/// If `pass_index` is out of range or the description was not made by
/// [`crate::parser::parse_effect`] (unknown texture indices).
pub fn generate_pass_source(desc: &EffectDesc, pass_index: usize, fp16: bool) -> String {
    let p = &desc.passes[pass_index];
    let mut parts: Vec<String> = Vec::new();

    parts.push(macros(p, fp16).join("\n") + "\n");

    let mut cb = vec!["cbuffer __CB1 : register(b0) {".to_owned()];
    cb.extend(
        constant_buffer_layout(desc)
            .into_iter()
            .map(|e| format!("\t{} {};", e.ty, e.name)),
    );
    cb.push("};\n".to_owned());
    parts.push(cb.join("\n"));

    let mut decls = Vec::new();
    for (i, t) in p.inputs.iter().enumerate() {
        let tex = &desc.textures[*t as usize];
        decls.push(format!(
            "Texture2D<{}> {} : register(t{i});",
            resource_type(srv_type(&tex.format)),
            tex.name
        ));
    }
    for (i, t) in p.outputs.iter().enumerate() {
        let tex = &desc.textures[*t as usize];
        let fmt = formats::info(&tex.format).expect("pass outputs have a known format");
        decls.push(format!(
            "MP_IMAGE_FORMAT(\"{}\") RWTexture2D<{}> {} : register(u{i});",
            fmt.spirv_format,
            resource_type(fmt.uav_type),
            tex.name
        ));
    }
    for (i, s) in desc.samplers.iter().enumerate() {
        decls.push(format!("SamplerState {} : register(s{i});", s.name));
    }
    parts.push(decls.join("\n") + "\n");

    parts.push(BUILTINS.to_owned());
    if desc.use_dynamic {
        parts.push("uint GetFrameCount() { return __frameCount; }\n".to_owned());
    }
    if desc.use_muladd {
        parts.push(muladd_source());
    }
    for common in &desc.commons {
        parts.push(common.trim_end_matches('\n').to_owned() + "\n");
    }
    parts.push(p.code.trim_end_matches('\n').to_owned() + "\n");
    parts.push(if p.ps_style {
        ps_entry(desc, p)
    } else {
        cs_entry(p)
    });
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_effect;

    /// The FP16 test effect of 1.x `test_magpiefx_fp16.py`.
    fn source(fp16: bool) -> String {
        format!(
            "//!MAGPIE EFFECT\n//!VERSION 4\n{}\n\n//!TEXTURE\nTexture2D INPUT;\n\n//!PASS 1\n//!STYLE PS\n//!IN INPUT\n//!OUT OUTPUT\n\nMF4 Pass1(float2 pos) {{\n\treturn (MF4)INPUT.Load(int3(pos * GetInputSize(), 0));\n}}\n",
            if fp16 { "//!CAPABILITY FP16" } else { "" }
        )
    }

    #[test]
    fn fp32_macros() {
        let lines = mf_type_macros(false);
        assert!(!lines.contains(&"#define MP_FP16".to_owned()));
        assert!(lines.contains(&"#define MF float".to_owned()));
        assert!(lines.contains(&"#define MF4 float4".to_owned()));
        assert!(lines.contains(&"#define MF3x4 float3x4".to_owned()));
        assert!(!lines.iter().any(|l| l.contains("min16float")));
    }

    #[test]
    fn fp16_macros() {
        let lines = mf_type_macros(true);
        assert_eq!(lines[0], "#define MP_FP16");
        assert!(lines.contains(&"#define MF min16float".to_owned()));
        assert!(lines.contains(&"#define MF1 min16float1".to_owned()));
        assert!(lines.contains(&"#define MF4x4 min16float4x4".to_owned()));
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("#define MF")).count(),
            1 + 4 + 16
        );
    }

    #[test]
    fn generated_source_follows_the_fp16_flag() {
        let desc = parse_effect(&source(true), "Test").unwrap();
        assert!(desc.supports_fp16);
        let fp32 = generate_pass_source(&desc, 0, false);
        let fp16 = generate_pass_source(&desc, 0, true);
        assert!(fp32.contains("#define MF float") && !fp32.contains("MP_FP16"));
        assert!(fp16.contains("#define MF min16float") && fp16.contains("#define MP_FP16"));
        // textures stay 32-bit
        assert!(fp16.contains("Texture2D<float4> INPUT : register(t0);"));
        assert!(!fp16.contains("min16float4> "));
        assert_eq!(
            fp16.replace("min16float", "float")
                .replace("#define MP_FP16\n", ""),
            fp32
        );
    }

    #[test]
    fn layout_offsets_are_packed() {
        let src = "//!MAGPIE EFFECT\n//!VERSION 4\n//!USE _DYNAMIC\n\
            //!PARAMETER\n//!DEFAULT 1\n//!MIN 0\n//!MAX 2\n//!STEP 1\nint taps;\n\
            //!TEXTURE\n//!FORMAT R8G8B8A8_UNORM\nTexture2D t;\n\
            //!PASS 1\n//!STYLE PS\n//!IN INPUT\n//!OUT t\nMF4 Pass1(float2 pos) { return 0; }\n\
            //!PASS 2\n//!STYLE PS\n//!IN t\n//!OUT OUTPUT\nMF4 Pass2(float2 pos) { return 0; }\n";
        let desc = parse_effect(src, "t").unwrap();
        let layout = constant_buffer_layout(&desc);
        let names: Vec<_> = layout
            .iter()
            .map(|e| (e.name.as_str(), e.ty.as_str(), e.offset))
            .collect();
        assert_eq!(
            names,
            [
                ("__inputSize", "uint2", 0),
                ("__outputSize", "uint2", 8),
                ("__inputPt", "float2", 16),
                ("__outputPt", "float2", 24),
                ("__scale", "float2", 32),
                ("__pass1OutputSize", "uint2", 40),
                ("__pass1OutputPt", "float2", 48),
                ("taps", "int", 56),
                ("__frameCount", "uint", 60),
            ]
        );
        let src1 = generate_pass_source(&desc, 0, false);
        assert!(src1.contains("if (gxy.x >= __pass1OutputSize.x"));
        assert!(src1.contains("MP_IMAGE_FORMAT(\"rgba8\") RWTexture2D<unorm float4> t : register(u0);"));
        assert!(src1.contains("uint GetFrameCount()"));
        let src2 = generate_pass_source(&desc, 1, false);
        assert!(src2.contains("if (gxy.x >= __outputSize.x"));
        assert!(src2.contains("Texture2D<float4> t : register(t0);"));
    }

    #[test]
    fn cs_entry_block_start() {
        let mut p = EffectPass {
            index: 3,
            block_size: (16, 16),
            num_threads: (64, 1, 1),
            ..EffectPass::default()
        };
        assert!(cs_entry(&p).contains("Pass3((gid.xy << 4), tid);"));
        p.block_size = (16, 8);
        assert!(cs_entry(&p).contains("Pass3(gid.xy * uint2(16, 8), tid);"));
    }

    #[test]
    fn muladd_has_nine_overloads() {
        let src = muladd_source();
        assert_eq!(src.matches("MulAdd(").count(), 9);
        assert!(src.contains("MF3 MulAdd(MF2 x, MF2x3 y, MF3 a) {\n\tMF3 result = a;\n\tresult = mad(x.x, y._m00_m01_m02, result);"));
    }
}
