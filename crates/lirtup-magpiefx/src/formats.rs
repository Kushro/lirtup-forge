//! MagpieFX format names (`formats.py`) → `lirtup_gpu::Format`, texel types.

use lirtup_gpu::Format;

/// What the codegen and the runtime need about a `//!FORMAT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatDesc {
    /// GPU format.
    pub format: Format,
    /// HLSL texel type (`float4`, `uint`, ...).
    pub texel_type: &'static str,
    /// Channels.
    pub channels: u32,
}

/// One row of the table: name, format, SRV type, UAV type, SPIR-V storage
/// image format (`[[vk::image_format]]`).
pub(crate) struct FormatInfo {
    pub name: &'static str,
    pub format: Format,
    pub srv_type: &'static str,
    pub uav_type: &'static str,
    pub spirv_format: &'static str,
}

macro_rules! row {
    ($name:literal, $fmt:ident, $srv:literal, $uav:literal, $spirv:literal) => {
        FormatInfo {
            name: $name,
            format: Format::$fmt,
            srv_type: $srv,
            uav_type: $uav,
            spirv_format: $spirv,
        }
    };
}

/// The intermediate formats of MagpieFX (Magpie's `EffectHelper.h`).
pub(crate) static FORMATS: &[FormatInfo] = &[
    row!(
        "R32G32B32A32_FLOAT",
        R32G32B32A32Float,
        "float4",
        "float4",
        "rgba32f"
    ),
    row!("R16G16B16A16_FLOAT", R16G16B16A16Float, "MF4", "MF4", "rgba16f"),
    row!(
        "R16G16B16A16_UNORM",
        R16G16B16A16Unorm,
        "MF4",
        "unorm MF4",
        "rgba16"
    ),
    row!(
        "R16G16B16A16_SNORM",
        R16G16B16A16Snorm,
        "MF4",
        "snorm MF4",
        "rgba16snorm"
    ),
    row!("R32G32_FLOAT", R32G32Float, "float2", "float2", "rg32f"),
    row!(
        "R10G10B10A2_UNORM",
        R10G10B10A2Unorm,
        "MF4",
        "unorm MF4",
        "rgb10a2"
    ),
    row!("R11G11B10_FLOAT", R11G11B10Float, "MF3", "MF3", "r11g11b10f"),
    row!("R8G8B8A8_UNORM", R8G8B8A8Unorm, "MF4", "unorm MF4", "rgba8"),
    row!("R8G8B8A8_SNORM", R8G8B8A8Snorm, "MF4", "snorm MF4", "rgba8snorm"),
    row!("R16G16_FLOAT", R16G16Float, "MF2", "MF2", "rg16f"),
    row!("R16G16_UNORM", R16G16Unorm, "MF2", "unorm MF2", "rg16"),
    row!("R16G16_SNORM", R16G16Snorm, "MF2", "snorm MF2", "rg16snorm"),
    row!("R32_FLOAT", R32Float, "float", "float", "r32f"),
    row!("R8G8_UNORM", R8G8Unorm, "MF2", "unorm MF2", "rg8"),
    row!("R8G8_SNORM", R8G8Snorm, "MF2", "snorm MF2", "rg8snorm"),
    row!("R16_FLOAT", R16Float, "MF", "MF", "r16f"),
    row!("R16_UNORM", R16Unorm, "MF", "unorm MF", "r16"),
    row!("R16_SNORM", R16Snorm, "MF", "snorm MF", "r16snorm"),
    row!("R8_UNORM", R8Unorm, "MF", "unorm MF", "r8"),
    row!("R8_SNORM", R8Snorm, "MF", "snorm MF", "r8snorm"),
];

/// The full row of a format name.
pub(crate) fn info(name: &str) -> Option<&'static FormatInfo> {
    FORMATS.iter().find(|f| f.name == name)
}

/// Look up a format name (`R8G8B8A8_UNORM`, ...).
pub fn lookup(name: &str) -> Option<FormatDesc> {
    info(name).map(|f| {
        let last = f.srv_type.chars().last().unwrap_or('1');
        FormatDesc {
            format: f.format,
            texel_type: f.srv_type,
            channels: last.to_digit(10).unwrap_or(1),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_gives_channels() {
        let d = lookup("R16G16B16A16_FLOAT").unwrap();
        assert_eq!(
            (d.format, d.texel_type, d.channels),
            (Format::R16G16B16A16Float, "MF4", 4)
        );
        assert_eq!(lookup("R32_FLOAT").unwrap().channels, 1);
        assert_eq!(lookup("R11G11B10_FLOAT").unwrap().channels, 3);
        assert!(lookup("B8G8R8A8_UNORM").is_none());
        assert_eq!(FORMATS.len(), 20);
    }
}
