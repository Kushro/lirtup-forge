"""
Intermediate texture formats accepted by MagpieFX.

Each entry maps the DXGI name used in ``//!FORMAT`` to:
    * the format constant of the Vulkan extension (``upscaler.vulkan``),
    * the HLSL texel type for SRVs and UAVs (as in Magpie's EffectHelper.h),
    * the SPIR-V storage image format, emitted as ``[[vk::image_format]]`` so
      that UAV writes do not depend on ``shaderStorageImageWriteWithoutFormat``.
"""

from typing import Dict, NamedTuple

from .. import vulkan as vk


class FormatDesc(NamedTuple):
    vk_format: int
    srv_type: str
    uav_type: str
    spirv_format: str


FORMATS: Dict[str, FormatDesc] = {
    "R32G32B32A32_FLOAT": FormatDesc(vk.R32G32B32A32_FLOAT, "float4", "float4", "rgba32f"),
    "R16G16B16A16_FLOAT": FormatDesc(vk.R16G16B16A16_FLOAT, "MF4", "MF4", "rgba16f"),
    "R16G16B16A16_UNORM": FormatDesc(vk.R16G16B16A16_UNORM, "MF4", "unorm MF4", "rgba16"),
    "R16G16B16A16_SNORM": FormatDesc(vk.R16G16B16A16_SNORM, "MF4", "snorm MF4", "rgba16snorm"),
    "R32G32_FLOAT": FormatDesc(vk.R32G32_FLOAT, "float2", "float2", "rg32f"),
    "R10G10B10A2_UNORM": FormatDesc(vk.R10G10B10A2_UNORM, "MF4", "unorm MF4", "rgb10a2"),
    "R11G11B10_FLOAT": FormatDesc(vk.R11G11B10_FLOAT, "MF3", "MF3", "r11g11b10f"),
    "R8G8B8A8_UNORM": FormatDesc(vk.R8G8B8A8_UNORM, "MF4", "unorm MF4", "rgba8"),
    "R8G8B8A8_SNORM": FormatDesc(vk.R8G8B8A8_SNORM, "MF4", "snorm MF4", "rgba8snorm"),
    "R16G16_FLOAT": FormatDesc(vk.R16G16_FLOAT, "MF2", "MF2", "rg16f"),
    "R16G16_UNORM": FormatDesc(vk.R16G16_UNORM, "MF2", "unorm MF2", "rg16"),
    "R16G16_SNORM": FormatDesc(vk.R16G16_SNORM, "MF2", "snorm MF2", "rg16snorm"),
    "R32_FLOAT": FormatDesc(vk.R32_FLOAT, "float", "float", "r32f"),
    "R8G8_UNORM": FormatDesc(vk.R8G8_UNORM, "MF2", "unorm MF2", "rg8"),
    "R8G8_SNORM": FormatDesc(vk.R8G8_SNORM, "MF2", "snorm MF2", "rg8snorm"),
    "R16_FLOAT": FormatDesc(vk.R16_FLOAT, "MF", "MF", "r16f"),
    "R16_UNORM": FormatDesc(vk.R16_UNORM, "MF", "unorm MF", "r16"),
    "R16_SNORM": FormatDesc(vk.R16_SNORM, "MF", "snorm MF", "r16snorm"),
    "R8_UNORM": FormatDesc(vk.R8_UNORM, "MF", "unorm MF", "r8"),
    "R8_SNORM": FormatDesc(vk.R8_SNORM, "MF", "snorm MF", "r8snorm"),
}
