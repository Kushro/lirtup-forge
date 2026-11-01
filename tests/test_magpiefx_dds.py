"""DDS reader for MagpieFX //!SOURCE lookup textures, with small synthetic files."""

import os
import struct

import pytest

from upscaler.magpiefx import dds
from upscaler.magpiefx.dds import (
    DDPF_ALPHAPIXELS,
    DDPF_BUMPDUDV,
    DDPF_FOURCC,
    DDPF_LUMINANCE,
    DDPF_RGB,
    DDSImage,
    load_image,
    parse_dds,
    resolve_source_path,
    source_format,
    to_rgba_floats,
)
from upscaler.magpiefx.effect import BUILTIN_EFFECTS_DIR
from upscaler.magpiefx.parser import MagpieFXError


def make_dds(
    width,
    height,
    data,
    flags=0,
    fourcc=0,
    bits=0,
    masks=(0, 0, 0, 0),
    dx10=None,
    caps2=0,
    mips=1,
):
    """A DDS file with the given pixel format; *dx10* is (dxgi, dim, misc, array)."""
    if isinstance(fourcc, bytes):
        fourcc = struct.unpack("<I", fourcc)[0]
    header = struct.pack("<7I", 124, 0x100F, height, width, 0, 0, mips)
    header += b"\0" * 44
    header += struct.pack("<8I", 32, flags, fourcc, bits, *masks)
    header += struct.pack("<4I", 0x1000, caps2, 0, 0) + b"\0" * 4
    assert len(header) == 124
    out = b"DDS " + header
    if dx10 is not None:
        out += struct.pack("<5I", *dx10, 0)
    return out + data


def halves(*values):
    return struct.pack(f"<{len(values)}e", *values)


# ----------------------------------------------------------------------
# Formats
# ----------------------------------------------------------------------
def test_fourcc_113_is_rgba16_float():
    data = halves(0.5, -1.0, 2.0, 1.0, 0.25, 0.0, 65504.0, -0.125)
    img = parse_dds(make_dds(2, 1, data, flags=DDPF_FOURCC, fourcc=113))
    assert (img.width, img.height, img.format) == (2, 1, "R16G16B16A16_FLOAT")
    assert img.data == data
    assert list(to_rgba_floats(img)) == [0.5, -1.0, 2.0, 1.0, 0.25, 0.0, 65504.0, -0.125]


def test_rgba8_masks():
    data = bytes([255, 0, 51, 255, 0, 255, 102, 0])
    img = parse_dds(
        make_dds(
            2, 1, data,
            flags=DDPF_RGB | DDPF_ALPHAPIXELS, bits=32,
            masks=(0xFF, 0xFF00, 0xFF0000, 0xFF000000),
        )
    )
    assert img.format == "R8G8B8A8_UNORM" and not img.swap_rb
    assert list(to_rgba_floats(img)) == pytest.approx(
        [1.0, 0.0, 0.2, 1.0, 0.0, 1.0, 0.4, 0.0]
    )


def test_bgra8_masks_are_read_as_rgba():
    data = bytes([10, 20, 30, 40])  # B, G, R, A
    img = parse_dds(
        make_dds(
            1, 1, data,
            flags=DDPF_RGB | DDPF_ALPHAPIXELS, bits=32,
            masks=(0xFF0000, 0xFF00, 0xFF, 0xFF000000),
        )
    )
    assert img.format == "R8G8B8A8_UNORM" and img.swap_rb
    assert list(to_rgba_floats(img)) == pytest.approx([30 / 255, 20 / 255, 10 / 255, 40 / 255])


def test_luminance8_is_r8_unorm():
    img = parse_dds(
        make_dds(3, 1, bytes([0, 128, 255]), flags=DDPF_LUMINANCE, bits=8,
                 masks=(0xFF, 0, 0, 0))
    )
    assert img.format == "R8_UNORM"
    assert list(to_rgba_floats(img)) == pytest.approx(
        [0, 0, 0, 1, 128 / 255, 0, 0, 1, 1, 0, 0, 1]
    )


def test_luminance_alpha_is_r8g8_unorm():
    img = parse_dds(
        make_dds(1, 1, bytes([255, 0]), flags=DDPF_LUMINANCE | DDPF_ALPHAPIXELS,
                 bits=16, masks=(0xFF, 0, 0, 0xFF00))
    )
    assert img.format == "R8G8_UNORM"
    assert list(to_rgba_floats(img)) == pytest.approx([1, 0, 0, 1])


def test_bumpdudv_is_snorm_and_clamps_minus_128():
    img = parse_dds(
        make_dds(1, 1, struct.pack("<2b", -128, 127), flags=DDPF_BUMPDUDV, bits=16,
                 masks=(0xFF, 0xFF00, 0, 0))
    )
    assert img.format == "R8G8_SNORM"
    assert list(to_rgba_floats(img)) == pytest.approx([-1.0, 1.0, 0.0, 1.0])


def test_dx10_r16_unorm():
    data = struct.pack("<2H", 0, 65535)
    img = parse_dds(make_dds(2, 1, data, flags=DDPF_FOURCC, fourcc=b"DX10", dx10=(55, 3, 0, 1)))
    assert img.format == "R16_UNORM"
    assert list(to_rgba_floats(img)) == pytest.approx([0, 0, 0, 1, 1, 0, 0, 1])


def test_dx10_r32_float_and_rg32_float():
    img = parse_dds(
        make_dds(1, 1, struct.pack("<f", 3.5), flags=DDPF_FOURCC, fourcc=b"DX10",
                 dx10=(41, 3, 0, 1))
    )
    assert img.format == "R32_FLOAT"
    assert list(to_rgba_floats(img)) == [3.5, 0.0, 0.0, 1.0]
    img = parse_dds(
        make_dds(1, 1, struct.pack("<2f", 1.5, -2.0), flags=DDPF_FOURCC, fourcc=115)
    )
    assert img.format == "R32G32_FLOAT"
    assert list(to_rgba_floats(img)) == [1.5, -2.0, 0.0, 1.0]


def test_dx10_bgra8_is_read_as_rgba():
    img = parse_dds(
        make_dds(1, 1, bytes([0, 0, 255, 255]), flags=DDPF_FOURCC, fourcc=b"DX10",
                 dx10=(87, 3, 0, 1))
    )
    assert img.format == "R8G8B8A8_UNORM" and img.swap_rb
    assert list(to_rgba_floats(img)) == pytest.approx([1, 0, 0, 1])


def test_r10g10b10a2_is_unpacked():
    word = 1023 | (0 << 10) | (512 << 20) | (3 << 30)
    img = parse_dds(
        make_dds(1, 1, struct.pack("<I", word), flags=DDPF_FOURCC, fourcc=b"DX10",
                 dx10=(24, 3, 0, 1))
    )
    assert img.format == "R10G10B10A2_UNORM"
    assert list(to_rgba_floats(img)) == pytest.approx([1.0, 0.0, 512 / 1023, 1.0])


def test_only_the_top_mip_is_kept():
    top = halves(*([1.0] * 16))  # 2x2 RGBA16F
    mip1 = halves(0.0, 0.0, 0.0, 0.0)
    img = parse_dds(make_dds(2, 2, top + mip1, flags=DDPF_FOURCC, fourcc=113, mips=2))
    assert img.data == top


# ----------------------------------------------------------------------
# Rejected files
# ----------------------------------------------------------------------
@pytest.mark.parametrize("fourcc", [b"DXT1", b"DXT5", b"ATI2", b"BC4U"])
def test_block_compressed_fourcc_is_rejected(fourcc):
    with pytest.raises(MagpieFXError, match="block-compressed"):
        parse_dds(make_dds(4, 4, b"\0" * 16, flags=DDPF_FOURCC, fourcc=fourcc))


@pytest.mark.parametrize("dxgi", [71, 77, 98])
def test_block_compressed_dx10_is_rejected(dxgi):
    with pytest.raises(MagpieFXError, match="block-compressed"):
        parse_dds(make_dds(4, 4, b"\0" * 16, flags=DDPF_FOURCC, fourcc=b"DX10",
                           dx10=(dxgi, 3, 0, 1)))


def test_bad_magic_and_short_files():
    with pytest.raises(MagpieFXError, match="not a DDS"):
        parse_dds(b"PNG " + b"\0" * 200)
    with pytest.raises(MagpieFXError, match="not a DDS"):
        parse_dds(b"DDS ")


def test_truncated_data():
    with pytest.raises(MagpieFXError, match="truncated"):
        parse_dds(make_dds(4, 4, halves(1.0), flags=DDPF_FOURCC, fourcc=113))


def test_cube_maps_volumes_and_arrays_are_rejected():
    data = halves(*([0.0] * 4))
    with pytest.raises(MagpieFXError, match="cube"):
        parse_dds(make_dds(1, 1, data, flags=DDPF_FOURCC, fourcc=113, caps2=0x200))
    with pytest.raises(MagpieFXError, match="volume"):
        parse_dds(make_dds(1, 1, data, flags=DDPF_FOURCC, fourcc=113, caps2=0x200000))
    with pytest.raises(MagpieFXError, match="arrays"):
        parse_dds(make_dds(1, 1, data * 2, flags=DDPF_FOURCC, fourcc=b"DX10",
                           dx10=(10, 3, 0, 2)))
    with pytest.raises(MagpieFXError, match="2D"):
        parse_dds(make_dds(1, 1, data, flags=DDPF_FOURCC, fourcc=b"DX10",
                           dx10=(10, 4, 0, 1)))


def test_unknown_masks_and_fourcc():
    with pytest.raises(MagpieFXError, match="pixel format"):
        parse_dds(make_dds(1, 1, b"\0\0", flags=DDPF_RGB, bits=16,
                           masks=(0xF800, 0x7E0, 0x1F, 0)))
    with pytest.raises(MagpieFXError, match="FourCC"):
        parse_dds(make_dds(1, 1, b"\0" * 8, flags=DDPF_FOURCC, fourcc=b"ABCD"))


def test_unsupported_dxgi_format():
    # R11G11B10_FLOAT exists as an intermediate format but cannot be uploaded
    with pytest.raises(MagpieFXError, match="lookup texture"):
        parse_dds(make_dds(1, 1, b"\0" * 4, flags=DDPF_FOURCC, fourcc=b"DX10",
                           dx10=(26, 3, 0, 1)))
    with pytest.raises(MagpieFXError, match="DXGI format 1000"):
        parse_dds(make_dds(1, 1, b"\0" * 4, flags=DDPF_FOURCC, fourcc=b"DX10",
                           dx10=(1000, 3, 0, 1)))


# ----------------------------------------------------------------------
# Files
# ----------------------------------------------------------------------
def test_load_image_dds_and_png(tmp_path):
    path = tmp_path / "lut.dds"
    path.write_bytes(make_dds(1, 1, halves(1, 2, 3, 4), flags=DDPF_FOURCC, fourcc=113))
    assert load_image(str(path)).format == "R16G16B16A16_FLOAT"

    from PIL import Image

    png = tmp_path / "lut.png"
    Image.new("RGB", (2, 1), (255, 0, 0)).save(png)
    img = load_image(str(png))
    assert (img.width, img.height, img.format) == (2, 1, "R8G8B8A8_UNORM")
    assert img.data == bytes([255, 0, 0, 255] * 2)


def test_load_image_errors_name_the_file(tmp_path):
    bad = tmp_path / "broken.dds"
    bad.write_bytes(b"nope")
    with pytest.raises(MagpieFXError, match="broken.dds"):
        load_image(str(bad))
    other = tmp_path / "lut.raw"
    other.write_bytes(b"\0")
    with pytest.raises(MagpieFXError, match="unsupported lookup texture type"):
        load_image(str(other))


def test_resolve_source_path(tmp_path):
    effect_dir = tmp_path / "RAVU"
    effect_dir.mkdir()
    assert resolve_source_path(str(effect_dir), "lut.dds") == os.path.realpath(
        effect_dir / "lut.dds"
    )
    assert resolve_source_path(str(effect_dir), "sub\\lut.dds").endswith(
        os.path.join("sub", "lut.dds")
    )
    for bad in ("../lut.dds", "/etc/passwd", ""):
        with pytest.raises(MagpieFXError):
            resolve_source_path(str(effect_dir), bad)


def test_source_format_must_match_the_declared_format():
    img = DDSImage(1, 1, "R8_UNORM", b"\0")
    assert source_format("", img) == "R8_UNORM"
    assert source_format("R8_UNORM", img) == "R8_UNORM"
    with pytest.raises(MagpieFXError, match="does not match"):
        source_format("R16G16B16A16_FLOAT", img, "searchTex")


# ----------------------------------------------------------------------
# Bundled lookup textures
# ----------------------------------------------------------------------
def _bundled_dds():
    found = []
    for dirpath, _, files in os.walk(BUILTIN_EFFECTS_DIR):
        found += [os.path.join(dirpath, f) for f in files if f.endswith(".dds")]
    return sorted(found)


def test_bundled_dds_files_are_readable():
    files = _bundled_dds()
    assert len(files) == 17
    for path in files:
        img = load_image(path)
        name = os.path.basename(path)
        if name == "AreaTex.dds":
            assert (img.format, img.width, img.height) == ("R8G8B8A8_UNORM", 160, 560)
        elif name == "SearchTex.dds":
            assert (img.format, img.width, img.height) == ("R8_UNORM", 64, 16)
        else:
            assert img.format == "R16G16B16A16_FLOAT", name
        floats = to_rgba_floats(img)
        assert len(floats) == img.width * img.height * 4


def test_layouts_cover_only_known_formats():
    from upscaler.magpiefx.formats import FORMATS

    assert set(dds.LAYOUTS) <= set(FORMATS)
    for name in dds.DXGI_FORMATS.values():
        assert name in FORMATS or name == "B8G8R8A8_UNORM"
