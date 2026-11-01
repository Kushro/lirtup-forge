"""//!SOURCE textures: parsing, code generation and loading of the files."""

import os
import struct

import pytest

from upscaler.magpiefx.codegen import generate_pass_source
from upscaler.magpiefx.dds import DDPF_FOURCC, DDPF_LUMINANCE
from upscaler.magpiefx.effect import BUILTIN_EFFECTS_DIR, load_sources
from upscaler.magpiefx.lookup import upload_shader_source
from upscaler.magpiefx.parser import MagpieFXError, parse_effect

HEADER = "//!MAGPIE EFFECT\n//!VERSION 4\n\n"

PASS = """
//!PASS 1
//!STYLE PS
//!IN INPUT, lut
//!OUT OUTPUT

float4 Pass1(float2 pos) {
	return INPUT.SampleLevel(sam, pos, 0) * lut.SampleLevel(sam, pos, 0).x;
}
"""

SAMPLER = "//!SAMPLER\n//!FILTER LINEAR\nSamplerState sam;\n"


def effect(texture_block: str, passes: str = PASS) -> str:
    return (
        HEADER
        + "//!TEXTURE\nTexture2D INPUT;\n\n"
        + texture_block
        + "\n"
        + SAMPLER
        + passes
    )


def make_dds(width, height, data, flags, fourcc=0, bits=0, mask=0):
    header = struct.pack("<7I", 124, 0x100F, height, width, 0, 0, 1) + b"\0" * 44
    header += struct.pack("<8I", 32, flags, fourcc, bits, mask, 0, 0, 0)
    header += struct.pack("<4I", 0x1000, 0, 0, 0) + b"\0" * 4
    return b"DDS " + header + data


# ----------------------------------------------------------------------
# Parser
# ----------------------------------------------------------------------
def test_source_with_format():
    desc = parse_effect(
        effect("//!TEXTURE\n//!SOURCE lut.dds\n//!FORMAT R16G16B16A16_FLOAT\nTexture2D lut;\n")
    )
    lut = desc.textures[2]
    assert (lut.name, lut.source, lut.format) == ("lut", "lut.dds", "R16G16B16A16_FLOAT")
    assert desc.passes[0].inputs == [0, 2]


def test_source_without_format_takes_it_from_the_file():
    desc = parse_effect(effect("//!TEXTURE\n//!SOURCE lut.dds\nTexture2D lut;\n"))
    assert desc.textures[2].source == "lut.dds"
    assert desc.textures[2].format == ""


def test_source_excludes_size():
    with pytest.raises(MagpieFXError, match="SOURCE excludes WIDTH/HEIGHT"):
        parse_effect(
            effect(
                "//!TEXTURE\n//!SOURCE lut.dds\n//!WIDTH 4\n//!HEIGHT 4\nTexture2D lut;\n"
            )
        )


def test_texture_needs_format_or_source():
    with pytest.raises(MagpieFXError, match="missing '//!FORMAT'"):
        parse_effect(effect("//!TEXTURE\nTexture2D lut;\n"))


def test_source_texture_cannot_be_written():
    passes = """
//!PASS 1
//!STYLE PS
//!IN INPUT
//!OUT lut

float4 Pass1(float2 pos) { return 0; }

//!PASS 2
//!STYLE PS
//!IN lut
//!OUT OUTPUT

float4 Pass2(float2 pos) { return 0; }
"""
    with pytest.raises(MagpieFXError, match="invalid output 'lut'"):
        parse_effect(
            effect("//!TEXTURE\n//!SOURCE lut.dds\n//!FORMAT R8_UNORM\nTexture2D lut;\n", passes)
        )


def test_duplicated_source_directive():
    with pytest.raises(MagpieFXError, match="duplicated 'SOURCE'"):
        parse_effect(effect("//!TEXTURE\n//!SOURCE a.dds\n//!SOURCE b.dds\nTexture2D lut;\n"))


# ----------------------------------------------------------------------
# Code generation
# ----------------------------------------------------------------------
def test_declared_format_sets_the_texel_type():
    desc = parse_effect(
        effect("//!TEXTURE\n//!SOURCE lut.dds\n//!FORMAT R8_UNORM\nTexture2D lut;\n")
    )
    # MF is declared as float: Vulkan cannot sample 16-bit texel types
    for fp16 in (False, True):
        src = generate_pass_source(desc, 1, fp16=fp16)
        assert "Texture2D<float> lut : register(t1);" in src


def test_unknown_format_is_declared_as_float4():
    desc = parse_effect(effect("//!TEXTURE\n//!SOURCE lut.dds\nTexture2D lut;\n"))
    src = generate_pass_source(desc, 1)
    assert "Texture2D<float4> lut : register(t1);" in src


@pytest.mark.parametrize(
    "fmt, image_format, store",
    [
        ("R16G16B16A16_FLOAT", '"rgba16f"', "RWTexture2D<float4>"),
        ("R8G8B8A8_UNORM", '"rgba8"', "RWTexture2D<unorm float4>"),
        ("R8_UNORM", '"r8"', "RWTexture2D<unorm float>"),
        ("R16G16_SNORM", '"rg16snorm"', "RWTexture2D<snorm float2>"),
    ],
)
def test_upload_shader_writes_the_texture_format(fmt, image_format, store):
    src = upload_shader_source(fmt)
    assert f"MP_IMAGE_FORMAT({image_format}) {store} __dst : register(u1);" in src
    assert "RWByteAddressBuffer __src : register(u0);" in src
    assert "MF" not in src
    components = {"float4": "xyzw", "float2": "xy", "float": "x"}[store.split()[-1].rstrip(">")]
    assert f"__dst[id.xy] = v.{components};" in src


# ----------------------------------------------------------------------
# Loading the files
# ----------------------------------------------------------------------
def _write_effect(tmp_path, texture_block, files):
    folder = tmp_path / "fx"
    folder.mkdir()
    for name, data in files.items():
        (folder / name).write_bytes(data)
    desc = parse_effect(effect(texture_block), "Test")
    return desc, str(folder)


def test_load_sources_fills_the_format(tmp_path):
    lut = make_dds(4, 1, bytes([0, 85, 170, 255]), DDPF_LUMINANCE, bits=8, mask=0xFF)
    desc, folder = _write_effect(
        tmp_path, "//!TEXTURE\n//!SOURCE lut.dds\nTexture2D lut;\n", {"lut.dds": lut}
    )
    sources = load_sources(desc, folder)
    assert list(sources) == [2]
    assert (sources[2].format, sources[2].width, sources[2].height) == ("R8_UNORM", 4, 1)


def test_load_sources_checks_the_declared_format(tmp_path):
    lut = make_dds(1, 1, b"\0" * 8, DDPF_FOURCC, fourcc=113)
    desc, folder = _write_effect(
        tmp_path,
        "//!TEXTURE\n//!SOURCE lut.dds\n//!FORMAT R8_UNORM\nTexture2D lut;\n",
        {"lut.dds": lut},
    )
    with pytest.raises(MagpieFXError, match="does not match"):
        load_sources(desc, folder)


def test_load_sources_missing_file(tmp_path):
    desc, folder = _write_effect(
        tmp_path, "//!TEXTURE\n//!SOURCE lut.dds\nTexture2D lut;\n", {}
    )
    with pytest.raises(MagpieFXError, match="'lut.dds' of 'lut' not found"):
        load_sources(desc, folder)


def test_load_sources_rejects_paths_outside_the_effect(tmp_path):
    (tmp_path / "secret.dds").write_bytes(make_dds(1, 1, b"\0" * 8, DDPF_FOURCC, fourcc=113))
    desc, folder = _write_effect(
        tmp_path, "//!TEXTURE\n//!SOURCE ../secret.dds\nTexture2D lut;\n", {}
    )
    with pytest.raises(MagpieFXError, match="leaves the effect's directory"):
        load_sources(desc, folder)


def _bundled_with_source():
    found = []
    for dirpath, _, files in os.walk(BUILTIN_EFFECTS_DIR):
        for f in files:
            if not f.endswith(".hlsl"):
                continue
            path = os.path.join(dirpath, f)
            with open(path, encoding="utf-8-sig") as fh:
                if "//!SOURCE" in fh.read():
                    found.append(path)
    return sorted(found)


def test_all_bundled_source_effects_load_their_textures():
    paths = _bundled_with_source()
    names = [os.path.relpath(p, BUILTIN_EFFECTS_DIR) for p in paths]
    assert len(paths) == 31
    assert sum(n.startswith("RAVU") for n in names) == 26
    assert sum(n.startswith("SMAA") for n in names) == 4
    assert os.path.join("NIS", "NIS.hlsl") in names
    for path in paths:
        with open(path, encoding="utf-8-sig") as f:
            desc = parse_effect(f.read(), os.path.basename(path))
        sources = load_sources(desc, os.path.dirname(path))
        assert sources, path
        for i, image in sources.items():
            declared = desc.textures[i].format
            assert not declared or declared == image.format
        for p in desc.passes:
            generate_pass_source(desc, p.index)


def test_a_dxc_failure_without_glslang_says_to_install_it(monkeypatch):
    """Round 1.3, T5.5: the error only showed dxc's diagnostics."""
    from upscaler.magpiefx import compiler

    monkeypatch.setattr(compiler, "select_backends", lambda: [("dxc", "/usr/bin/dxc")])

    def fail(backend, *args, **kwargs):
        raise compiler.MagpieFXError(f"{backend} failed to compile X: boom")

    monkeypatch.setattr(compiler, "_compile_with", fail)
    with pytest.raises(compiler.MagpieFXError) as err:
        compiler.compile_hlsl("void main() {}", "X", use_cache=False)
    message = str(err.value)
    assert message.startswith("X needs glslangValidator")
    assert "dxc failed to compile X" in message


def test_no_install_hint_when_glslang_also_failed(monkeypatch):
    from upscaler.magpiefx import compiler

    monkeypatch.setattr(
        compiler,
        "select_backends",
        lambda: [("dxc", "/usr/bin/dxc"), ("glslang", "/usr/bin/glslangValidator")],
    )

    def fail(backend, *args, **kwargs):
        raise compiler.MagpieFXError(f"{backend} failed to compile X: boom")

    monkeypatch.setattr(compiler, "_compile_with", fail)
    with pytest.raises(compiler.MagpieFXError) as err:
        compiler.compile_hlsl("void main() {}", "X", use_cache=False)
    assert "needs glslangValidator" not in str(err.value)
