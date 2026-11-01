"""
Upload of ``//!SOURCE`` lookup textures to the GPU.

``Texture2D.upload_subresources`` only copies 4-byte texels, while lookup
textures are mostly ``R16G16B16A16_FLOAT`` (RAVU, NIS) or ``R8_UNORM``
(SMAA). So the texels are expanded to RGBA floats on the CPU
(:func:`.dds.to_rgba_floats`), put in a storage buffer and written into a
texture of the right format by a one-off compute pass; the image store does
the conversion. Lookup textures are small (at most a few MB) and uploaded
once per effect instance.
"""

from __future__ import annotations

import struct
from typing import List

from ..vulkan import HEAP_UPLOAD, Buffer, Compute, Texture2D
from .compiler import compile_hlsl
from .dds import DDSImage, to_rgba_floats
from .formats import FORMATS

_SWIZZLES = {1: "x", 2: "xy", 3: "xyz", 4: "xyzw"}


def upload_shader_source(format_name: str) -> str:
    """HLSL of the compute pass that writes RGBA floats into *format_name*."""
    fmt = FORMATS[format_name]
    uav_type = fmt.uav_type.replace("MF", "float")
    last = uav_type[-1]
    components = int(last) if last.isdigit() else 1
    lines: List[str] = [
        "#ifdef __hlsl_dx_compiler",
        "#define MP_IMAGE_FORMAT(f) [[vk::image_format(f)]]",
        "#else",
        "#define MP_IMAGE_FORMAT(f)",
        "#endif",
        "",
        "cbuffer __CB : register(b0) { uint2 __size; };",
        "RWByteAddressBuffer __src : register(u0);",
        f'MP_IMAGE_FORMAT("{fmt.spirv_format}") RWTexture2D<{uav_type}> __dst : register(u1);',
        "",
        "[numthreads(8, 8, 1)]",
        "void __M(uint3 id : SV_DispatchThreadID) {",
        "\tif (id.x >= __size.x || id.y >= __size.y) {",
        "\t\treturn;",
        "\t}",
        "\tfloat4 v = asfloat(__src.Load4((id.y * __size.x + id.x) * 16));",
        f"\t__dst[id.xy] = v.{_SWIZZLES[components]};",
        "}",
    ]
    return "\n".join(lines) + "\n"


def upload_lookup_texture(image: DDSImage, use_cache: bool = True) -> Texture2D:
    """Create a texture with *image*'s format and contents."""
    fmt = FORMATS[image.format]
    texture = Texture2D(image.width, image.height, format=fmt.vk_format)

    data = to_rgba_floats(image).tobytes()
    src = Buffer(len(data), heap_type=HEAP_UPLOAD, format=0)
    src.upload(data)
    cb = Buffer(16)
    cb.upload(struct.pack("<IIII", image.width, image.height, 0, 0))

    spirv = compile_hlsl(
        upload_shader_source(image.format), f"UploadLookup_{image.format}", use_cache=use_cache
    )
    compute = Compute(spirv, cbv=[cb], uav=[src, texture])
    compute.dispatch((image.width + 7) // 8, (image.height + 7) // 8, 1)
    return texture
