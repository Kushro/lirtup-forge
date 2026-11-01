"""FP16 (//!CAPABILITY FP16): MF type generation, compiler flags and fallback."""

import pytest

from upscaler.magpiefx import compiler, effect
from upscaler.magpiefx.codegen import generate_pass_source, mf_type_macros
from upscaler.magpiefx.parser import MagpieFXError, parse_effect

SOURCE = """//!MAGPIE EFFECT
//!VERSION 4
{capability}

//!TEXTURE
Texture2D INPUT;

//!PASS 1
//!STYLE PS
//!IN INPUT
//!OUT OUTPUT

MF4 Pass1(float2 pos) {{
	return (MF4)INPUT.Load(int3(pos * GetInputSize(), 0));
}}
"""


def make_source(fp16: bool) -> str:
    return SOURCE.format(capability="//!CAPABILITY FP16" if fp16 else "")


# ----------------------------------------------------------------------
# Type macros
# ----------------------------------------------------------------------
def test_fp32_macros():
    lines = mf_type_macros(False)
    assert "#define MP_FP16" not in lines
    assert "#define MF float" in lines
    assert "#define MF4 float4" in lines
    assert "#define MF3x4 float3x4" in lines
    assert not any("min16float" in ln for ln in lines)


def test_fp16_macros():
    lines = mf_type_macros(True)
    assert lines[0] == "#define MP_FP16"
    assert "#define MF min16float" in lines
    assert "#define MF1 min16float1" in lines
    assert "#define MF4x4 min16float4x4" in lines
    # MF, MF1..MF4 and the 16 matrix types
    assert len([ln for ln in lines if ln.startswith("#define MF")]) == 1 + 4 + 16


def test_generated_source_follows_the_flag():
    desc = parse_effect(make_source(True))
    assert desc.supports_fp16
    fp32 = generate_pass_source(desc, 1)
    fp16 = generate_pass_source(desc, 1, fp16=True)
    assert "#define MF float" in fp32 and "MP_FP16" not in fp32
    assert "#define MF min16float" in fp16 and "#define MP_FP16" in fp16
    # Textures stay 32-bit: Vulkan cannot sample 16-bit texel types
    assert "Texture2D<float4> INPUT : register(t0);" in fp16
    assert "min16float4> " not in fp16
    # Only the MF macros change
    assert fp16.replace("min16float", "float").replace("#define MP_FP16\n", "") == fp32


# ----------------------------------------------------------------------
# Compiler command lines
# ----------------------------------------------------------------------
def test_dxc_flags():
    fp32 = compiler._command("dxc", "dxc", "in.hlsl", "out.spv", None)
    fp16 = compiler._command("dxc", "dxc", "in.hlsl", "out.spv", None, fp16=True)
    assert fp32[fp32.index("-T") + 1] == "cs_6_0"
    assert "-enable-16bit-types" not in fp32
    assert fp16[fp16.index("-T") + 1] == "cs_6_2"
    assert "-enable-16bit-types" in fp16


def test_glslang_flags():
    fp32 = compiler._command("glslang", "glslangValidator", "in.hlsl", "out.spv", "/inc")
    fp16 = compiler._command("glslang", "glslangValidator", "in.hlsl", "out.spv", "/inc", True)
    assert "--hlsl-enable-16bit-types" not in fp32
    assert "--hlsl-enable-16bit-types" in fp16
    # The flag goes before the output and input paths
    assert fp16[-3:] == ["-o", "out.spv", "in.hlsl"]
    assert fp16[1] == "-I/inc"


# ----------------------------------------------------------------------
# Choice and fallback in compile_effect
# ----------------------------------------------------------------------
@pytest.fixture
def fake_compile(monkeypatch):
    """Record the fp16 flag of every pass compilation instead of compiling."""
    calls = []
    fail_fp16 = {"value": False}

    def compile_pass(desc, p, name, include_dir, use_cache, sin, sout, fp16=False):
        calls.append(fp16)
        if fp16 and fail_fp16["value"]:
            raise MagpieFXError("dxc failed to compile: half precision error")
        return b"SPIRV16" if fp16 else b"SPIRV32"

    monkeypatch.setattr(effect, "_compile_pass", compile_pass)
    return calls, fail_fp16


def _write(tmp_path, fp16_capability):
    path = tmp_path / "Test.hlsl"
    path.write_text(make_source(fp16_capability))
    return str(path)


def test_fp16_is_used_when_requested_and_declared(tmp_path, fake_compile):
    calls, _ = fake_compile
    result = effect.compile_effect(_write(tmp_path, True), fp16=True)
    assert result.fp16 and result.spirv == [b"SPIRV16"]
    assert calls == [True]


def test_fp16_needs_the_capability(tmp_path, fake_compile):
    calls, _ = fake_compile
    result = effect.compile_effect(_write(tmp_path, False), fp16=True)
    assert not result.fp16 and result.spirv == [b"SPIRV32"]
    assert calls == [False]


def test_fp16_not_requested(tmp_path, fake_compile):
    calls, _ = fake_compile
    result = effect.compile_effect(_write(tmp_path, True), fp16=False)
    assert not result.fp16
    assert calls == [False]


def test_fp16_falls_back_to_fp32_when_it_does_not_compile(tmp_path, fake_compile, caplog):
    calls, fail = fake_compile
    fail["value"] = True
    result = effect.compile_effect(_write(tmp_path, True), fp16=True)
    assert not result.fp16 and result.spirv == [b"SPIRV32"]
    assert calls == [True, False]
    assert "using 32-bit floats" in caplog.text


# ----------------------------------------------------------------------
# GPU support and the environment switch
# ----------------------------------------------------------------------
class _Device:
    def __init__(self, fp16):
        self.supports_fp16 = fp16


@pytest.mark.parametrize("supported", [True, False])
def test_fp16_wanted_follows_the_device(monkeypatch, supported):
    monkeypatch.delenv("UPSCALER_MAGPIEFX_FP16", raising=False)
    monkeypatch.setattr(effect, "get_current_device", lambda: _Device(supported))
    assert effect.fp16_wanted() is supported


@pytest.mark.parametrize("value", ["0", "off", "false"])
def test_fp16_can_be_disabled(monkeypatch, value):
    monkeypatch.setenv("UPSCALER_MAGPIEFX_FP16", value)
    monkeypatch.setattr(effect, "get_current_device", lambda: _Device(True))
    assert effect.fp16_wanted() is False


def test_forced_fp16_still_needs_the_device(monkeypatch):
    monkeypatch.setenv("UPSCALER_MAGPIEFX_FP16", "1")
    monkeypatch.setattr(effect, "get_current_device", lambda: _Device(False))
    assert effect.fp16_wanted() is False


def test_no_device_means_fp32(monkeypatch):
    monkeypatch.delenv("UPSCALER_MAGPIEFX_FP16", raising=False)

    def no_device():
        raise RuntimeError("no Vulkan device")

    monkeypatch.setattr(effect, "get_current_device", no_device)
    assert effect.fp16_wanted() is False
