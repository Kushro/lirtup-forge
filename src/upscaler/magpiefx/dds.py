"""
Reader for the lookup textures of ``//!SOURCE`` (``.dds`` and common images).

Magpie loads these with DirectXTK's DDS loader (and WIC for other images).
Only what MagpieFX effects can use is supported here:

* uncompressed DDS files, with the legacy header (masks or a D3DFMT FourCC)
  or the ``DX10`` extension header, whose format is one of the intermediate
  formats in :mod:`.formats` (plus ``B8G8R8A8_UNORM``, read as RGBA);
* only the top mip level of a single 2D texture is used;
* ``.png``, ``.bmp``, ``.jpg`` and ``.tif`` images, loaded with Pillow as
  ``R8G8B8A8_UNORM``.

Block-compressed DDS files (BC1-BC7) are rejected with a clear error: none
of the effects shipped with Magpie use them (RAVU and NIS use
``R16G16B16A16_FLOAT``, SMAA ``R8G8B8A8_UNORM`` and ``R8_UNORM``).

This module does not touch the GPU: :func:`to_rgba_floats` expands the
texels to RGBA floats, which the effect runtime writes into a texture of the
right format with a small compute pass (see :mod:`.effect`).
"""

from __future__ import annotations

import os
import struct
import sys
from array import array
from typing import Dict, NamedTuple, Optional, Tuple

from .parser import MagpieFXError

DDS_MAGIC = b"DDS "
_HEADER_SIZE = 124
_DX10_SIZE = 20

# DDS_PIXELFORMAT flags
DDPF_ALPHAPIXELS = 0x1
DDPF_ALPHA = 0x2
DDPF_FOURCC = 0x4
DDPF_RGB = 0x40
DDPF_LUMINANCE = 0x20000
DDPF_BUMPDUDV = 0x80000

# DDS_HEADER caps2 flags
_DDSCAPS2_CUBEMAP = 0x200
_DDSCAPS2_VOLUME = 0x200000

# DX10 resource dimension
_DIMENSION_TEXTURE2D = 3

IMAGE_EXTENSIONS = (".png", ".bmp", ".jpg", ".jpeg", ".tif", ".tiff")


class Layout(NamedTuple):
    """How the texels of a format are stored."""

    components: int
    code: str  # struct code of one component ("" for packed formats)
    size: int  # bytes per texel
    kind: str  # "float", "unorm", "snorm" or "packed"


# DXGI format numbers of the formats a lookup texture may use. The numbers
# match ``DXGI_FORMAT`` (and the constants of :mod:`upscaler.vulkan`).
DXGI_FORMATS: Dict[int, str] = {
    2: "R32G32B32A32_FLOAT",
    10: "R16G16B16A16_FLOAT",
    11: "R16G16B16A16_UNORM",
    13: "R16G16B16A16_SNORM",
    16: "R32G32_FLOAT",
    24: "R10G10B10A2_UNORM",
    26: "R11G11B10_FLOAT",
    28: "R8G8B8A8_UNORM",
    31: "R8G8B8A8_SNORM",
    34: "R16G16_FLOAT",
    35: "R16G16_UNORM",
    37: "R16G16_SNORM",
    41: "R32_FLOAT",
    49: "R8G8_UNORM",
    51: "R8G8_SNORM",
    54: "R16_FLOAT",
    55: "R16_UNORM",
    58: "R16_SNORM",
    61: "R8_UNORM",
    63: "R8_SNORM",
    87: "B8G8R8A8_UNORM",
}

# Block-compressed DXGI formats (typeless, unorm and sRGB variants)
_BC_FORMATS: Dict[int, str] = {}
for _first, _name in ((70, "BC1"), (73, "BC2"), (76, "BC3"), (79, "BC4"), (82, "BC5")):
    for _k in range(3):
        _BC_FORMATS[_first + _k] = _name
for _first, _name in ((94, "BC6H"), (97, "BC7")):
    for _k in range(3):
        _BC_FORMATS[_first + _k] = _name

LAYOUTS: Dict[str, Layout] = {
    "R32G32B32A32_FLOAT": Layout(4, "f", 16, "float"),
    "R32G32_FLOAT": Layout(2, "f", 8, "float"),
    "R32_FLOAT": Layout(1, "f", 4, "float"),
    "R16G16B16A16_FLOAT": Layout(4, "e", 8, "float"),
    "R16G16_FLOAT": Layout(2, "e", 4, "float"),
    "R16_FLOAT": Layout(1, "e", 2, "float"),
    "R16G16B16A16_UNORM": Layout(4, "H", 8, "unorm"),
    "R16G16_UNORM": Layout(2, "H", 4, "unorm"),
    "R16_UNORM": Layout(1, "H", 2, "unorm"),
    "R16G16B16A16_SNORM": Layout(4, "h", 8, "snorm"),
    "R16G16_SNORM": Layout(2, "h", 4, "snorm"),
    "R16_SNORM": Layout(1, "h", 2, "snorm"),
    "R8G8B8A8_UNORM": Layout(4, "B", 4, "unorm"),
    "R8G8_UNORM": Layout(2, "B", 2, "unorm"),
    "R8_UNORM": Layout(1, "B", 1, "unorm"),
    "R8G8B8A8_SNORM": Layout(4, "b", 4, "snorm"),
    "R8G8_SNORM": Layout(2, "b", 2, "snorm"),
    "R8_SNORM": Layout(1, "b", 1, "snorm"),
    "R10G10B10A2_UNORM": Layout(4, "", 4, "packed"),
}

# D3DFMT values used as FourCC by D3DX/DirectXTex for float and 16-bit formats
_D3DFMT_FOURCC: Dict[int, str] = {
    36: "R16G16B16A16_UNORM",
    110: "R16G16B16A16_SNORM",
    111: "R16_FLOAT",
    112: "R16G16_FLOAT",
    113: "R16G16B16A16_FLOAT",
    114: "R32_FLOAT",
    115: "R32G32_FLOAT",
    116: "R32G32B32A32_FLOAT",
}

_COMPRESSED_FOURCC = {
    b"DXT1": "BC1",
    b"DXT2": "BC2",
    b"DXT3": "BC2",
    b"DXT4": "BC3",
    b"DXT5": "BC3",
    b"ATI1": "BC4",
    b"BC4U": "BC4",
    b"BC4S": "BC4",
    b"ATI2": "BC5",
    b"BC5U": "BC5",
    b"BC5S": "BC5",
}

# (flags, bit count, R, G, B, A masks) -> (format, stored as BGRA)
_MASKS: Dict[Tuple[int, int, int, int, int, int], Tuple[str, bool]] = {
    (DDPF_RGB, 32, 0xFF, 0xFF00, 0xFF0000, 0xFF000000): ("R8G8B8A8_UNORM", False),
    (DDPF_RGB, 32, 0xFF0000, 0xFF00, 0xFF, 0xFF000000): ("R8G8B8A8_UNORM", True),
    (DDPF_RGB, 32, 0xFFFF, 0xFFFF0000, 0, 0): ("R16G16_UNORM", False),
    (DDPF_RGB, 32, 0xFFFFFFFF, 0, 0, 0): ("R32_FLOAT", False),
    (DDPF_RGB, 32, 0x3FF, 0xFFC00, 0x3FF00000, 0xC0000000): ("R10G10B10A2_UNORM", False),
    # D3DX writes R10G10B10A2 with the red and blue masks swapped
    (DDPF_RGB, 32, 0x3FF00000, 0xFFC00, 0x3FF, 0xC0000000): ("R10G10B10A2_UNORM", False),
    (DDPF_LUMINANCE, 8, 0xFF, 0, 0, 0): ("R8_UNORM", False),
    (DDPF_LUMINANCE, 16, 0xFFFF, 0, 0, 0): ("R16_UNORM", False),
    (DDPF_LUMINANCE, 16, 0xFF, 0, 0, 0xFF00): ("R8G8_UNORM", False),
    (DDPF_BUMPDUDV, 16, 0xFF, 0xFF00, 0, 0): ("R8G8_SNORM", False),
    (DDPF_BUMPDUDV, 32, 0xFF, 0xFF00, 0xFF0000, 0xFF000000): ("R8G8B8A8_SNORM", False),
    (DDPF_BUMPDUDV, 32, 0xFFFF, 0xFFFF0000, 0, 0): ("R16G16_SNORM", False),
}


class DDSImage(NamedTuple):
    """
    The top mip level of a lookup texture.

    Attributes:
        width, height: Size in texels.
        format: Intermediate format name (a key of :data:`LAYOUTS`) the
            texture is created with.
        data: Texel rows, tightly packed (``width * texel size`` bytes each).
        swap_rb: The data is stored as BGRA and must be read as RGBA.
    """

    width: int
    height: int
    format: str
    data: bytes
    swap_rb: bool = False


def _legacy_format(pf: Tuple[int, ...]) -> Tuple[str, bool]:
    _size, flags, fourcc, bits, r, g, b, a = pf
    if flags & DDPF_FOURCC:
        code = struct.pack("<I", fourcc)
        if code in _COMPRESSED_FOURCC:
            raise MagpieFXError(
                f"block-compressed DDS ({_COMPRESSED_FOURCC[code]}) is not supported"
            )
        if fourcc in _D3DFMT_FOURCC:
            return _D3DFMT_FOURCC[fourcc], False
        shown = code.decode("latin-1") if code.isalnum() else str(fourcc)
        raise MagpieFXError(f"unsupported DDS FourCC '{shown}'")

    kind = flags & (DDPF_RGB | DDPF_LUMINANCE | DDPF_BUMPDUDV | DDPF_ALPHA)
    key = (kind, bits, r, g, b, a)
    if key in _MASKS:
        return _MASKS[key]
    raise MagpieFXError(
        f"unsupported DDS pixel format (flags 0x{flags:x}, {bits} bits, masks "
        f"0x{r:x}/0x{g:x}/0x{b:x}/0x{a:x})"
    )


def parse_dds(data: bytes) -> DDSImage:
    """Parse a DDS file held in memory; raises :class:`MagpieFXError`."""
    if len(data) < 4 + _HEADER_SIZE or data[:4] != DDS_MAGIC:
        raise MagpieFXError("not a DDS file")
    size, _flags, height, width, _pitch, depth, _mips = struct.unpack_from("<7I", data, 4)
    if size != _HEADER_SIZE:
        raise MagpieFXError(f"invalid DDS header size {size}")
    pf = struct.unpack_from("<8I", data, 76)
    if pf[0] != 32:
        raise MagpieFXError(f"invalid DDS pixel format size {pf[0]}")
    _caps, caps2 = struct.unpack_from("<2I", data, 108)
    if caps2 & _DDSCAPS2_CUBEMAP:
        raise MagpieFXError("cube map DDS textures are not supported")
    if caps2 & _DDSCAPS2_VOLUME or depth > 1:
        raise MagpieFXError("volume DDS textures are not supported")

    offset = 4 + _HEADER_SIZE
    swap_rb = False
    if pf[1] & DDPF_FOURCC and struct.pack("<I", pf[2]) == b"DX10":
        if len(data) < offset + _DX10_SIZE:
            raise MagpieFXError("truncated DDS DX10 header")
        dxgi, dimension, misc, array_size = struct.unpack_from("<4I", data, offset)
        offset += _DX10_SIZE
        if dimension != _DIMENSION_TEXTURE2D or misc & 0x4:  # 0x4: cube map
            raise MagpieFXError("only 2D DDS textures are supported")
        if array_size > 1:
            raise MagpieFXError("DDS texture arrays are not supported")
        if dxgi in _BC_FORMATS:
            raise MagpieFXError(
                f"block-compressed DDS ({_BC_FORMATS[dxgi]}) is not supported"
            )
        fmt = DXGI_FORMATS.get(dxgi)
        if fmt is None:
            raise MagpieFXError(f"unsupported DXGI format {dxgi} in DDS file")
        if fmt == "B8G8R8A8_UNORM":
            fmt, swap_rb = "R8G8B8A8_UNORM", True
    else:
        fmt, swap_rb = _legacy_format(pf)

    layout = LAYOUTS.get(fmt)
    if layout is None:
        raise MagpieFXError(f"DDS format {fmt} cannot be used as a lookup texture")
    if width == 0 or height == 0:
        raise MagpieFXError("DDS texture has a zero size")
    needed = width * height * layout.size
    if len(data) < offset + needed:
        raise MagpieFXError(
            f"truncated DDS data ({len(data) - offset} bytes, need {needed})"
        )
    return DDSImage(width, height, fmt, bytes(data[offset:offset + needed]), swap_rb)


def load_image(path: str) -> DDSImage:
    """Load a ``.dds`` file or an image Pillow can read (as RGBA8)."""
    ext = os.path.splitext(path)[1].lower()
    try:
        if ext == ".dds":
            with open(path, "rb") as f:
                return parse_dds(f.read())
        if ext in IMAGE_EXTENSIONS:
            from PIL import Image

            with Image.open(path) as img:
                rgba = img.convert("RGBA")
                return DDSImage(rgba.width, rgba.height, "R8G8B8A8_UNORM", rgba.tobytes())
    except MagpieFXError as e:
        raise MagpieFXError(f"{os.path.basename(path)}: {e}") from None
    except OSError as e:
        raise MagpieFXError(f"cannot read '{path}': {e}") from None
    raise MagpieFXError(f"unsupported lookup texture type '{ext}' ({os.path.basename(path)})")


def to_rgba_floats(image: DDSImage) -> array:
    """
    Expand the texels to ``width * height * 4`` floats (RGBA, row-major).

    Missing channels read as 0 (green, blue) and 1 (alpha), like a texture
    load of that format would return. The values are exact: half floats and
    8/16-bit normalized values all fit in a 32-bit float.
    """
    layout = LAYOUTS[image.format]
    n = image.width * image.height
    count = n * layout.components
    if layout.kind == "packed":  # R10G10B10A2_UNORM
        words = struct.unpack(f"<{n}I", image.data[: n * 4])
        values = []
        for w in words:
            values += (
                (w & 0x3FF) / 1023.0,
                ((w >> 10) & 0x3FF) / 1023.0,
                ((w >> 20) & 0x3FF) / 1023.0,
                (w >> 30) / 3.0,
            )
    else:
        raw = struct.unpack(f"<{count}{layout.code}", image.data[: n * layout.size])
        if layout.kind == "unorm":
            scale = 1.0 / (255.0 if layout.code == "B" else 65535.0)
            values = [v * scale for v in raw]
        elif layout.kind == "snorm":
            top = 127.0 if layout.code == "b" else 32767.0
            values = [max(v / top, -1.0) for v in raw]
        else:
            values = list(raw)

    if layout.components == 4:
        rgba = values
    else:
        rgba = [0.0] * (n * 4)
        for c in range(layout.components):
            rgba[c::4] = values[c :: layout.components]
        rgba[3::4] = [1.0] * n
    if image.swap_rb:
        rgba[0::4], rgba[2::4] = rgba[2::4], rgba[0::4]

    out = array("f", rgba)
    if sys.byteorder == "big":
        out.byteswap()
    return out


def resolve_source_path(effect_dir: str, source: str) -> str:
    """
    Path of a ``//!SOURCE`` file, relative to the effect's directory.

    Absolute paths and paths that leave the effect's directory are rejected
    (Magpie also only looks next to the effect).
    """
    if os.path.isabs(source) or not source.strip():
        raise MagpieFXError(f"invalid //!SOURCE path '{source}'")
    base = os.path.realpath(effect_dir)
    path = os.path.realpath(os.path.join(base, source.replace("\\", "/")))
    if os.path.commonpath([base, path]) != base:
        raise MagpieFXError(f"//!SOURCE path '{source}' leaves the effect's directory")
    return path


def source_format(declared: str, image: DDSImage, name: Optional[str] = None) -> str:
    """
    Check that the format of a loaded texture matches the ``//!FORMAT`` of
    its ``//!SOURCE`` texture (when given) and return the format to use.
    """
    if declared and declared != image.format:
        what = f"texture '{name}'" if name else "texture"
        raise MagpieFXError(
            f"{what}: //!FORMAT {declared} does not match the file ({image.format})"
        )
    return image.format
