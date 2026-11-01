//! Pixel formats. The discriminants are the DXGI numbers of the 1.x format
//! table (`vulkan/__init__.py`), which MagpieFX's `//!FORMAT` names map to.

use ash::vk;

/// Texel format of a texture or a typed buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
#[allow(missing_docs)] // the names say it all
pub enum Format {
    R32G32B32A32Float = 2,
    R32G32B32A32Uint = 3,
    R32G32B32A32Sint = 4,
    R32G32B32Float = 6,
    R32G32B32Uint = 7,
    R32G32B32Sint = 8,
    R16G16B16A16Float = 10,
    R16G16B16A16Unorm = 11,
    R16G16B16A16Uint = 12,
    R16G16B16A16Snorm = 13,
    R16G16B16A16Sint = 14,
    R32G32Float = 16,
    R32G32Uint = 17,
    R32G32Sint = 18,
    R10G10B10A2Unorm = 24,
    R10G10B10A2Uint = 25,
    R11G11B10Float = 26,
    R8G8B8A8Unorm = 28,
    R8G8B8A8UnormSrgb = 29,
    R8G8B8A8Uint = 30,
    R8G8B8A8Snorm = 31,
    R8G8B8A8Sint = 32,
    R16G16Float = 34,
    R16G16Unorm = 35,
    R16G16Uint = 36,
    R16G16Snorm = 37,
    R16G16Sint = 38,
    R32Float = 41,
    R32Uint = 42,
    R32Sint = 43,
    R8G8Unorm = 49,
    R8G8Uint = 50,
    R8G8Snorm = 51,
    R8G8Sint = 52,
    R16Float = 54,
    R16Unorm = 55,
    R16Uint = 57,
    R16Snorm = 58,
    R16Sint = 59,
    R8Unorm = 61,
    R8Uint = 62,
    R8Snorm = 63,
    R8Sint = 64,
    /// What X11 capture produces (alpha undefined).
    B8G8R8A8Unorm = 87,
    B8G8R8A8UnormSrgb = 91,
}

/// `(format, VkFormat, bytes per texel)`, as `vk_module.cpp` fills
/// `vk_format_map`.
const TABLE: [(Format, vk::Format, u32); 45] = [
    (Format::R32G32B32A32Float, vk::Format::R32G32B32A32_SFLOAT, 16),
    (Format::R32G32B32A32Uint, vk::Format::R32G32B32A32_UINT, 16),
    (Format::R32G32B32A32Sint, vk::Format::R32G32B32A32_SINT, 16),
    (Format::R32G32B32Float, vk::Format::R32G32B32_SFLOAT, 12),
    (Format::R32G32B32Uint, vk::Format::R32G32B32_UINT, 12),
    (Format::R32G32B32Sint, vk::Format::R32G32B32_SINT, 12),
    (Format::R16G16B16A16Float, vk::Format::R16G16B16A16_SFLOAT, 8),
    (Format::R16G16B16A16Unorm, vk::Format::R16G16B16A16_UNORM, 8),
    (Format::R16G16B16A16Uint, vk::Format::R16G16B16A16_UINT, 8),
    (Format::R16G16B16A16Snorm, vk::Format::R16G16B16A16_SNORM, 8),
    (Format::R16G16B16A16Sint, vk::Format::R16G16B16A16_SINT, 8),
    (Format::R32G32Float, vk::Format::R32G32_SFLOAT, 8),
    (Format::R32G32Uint, vk::Format::R32G32_UINT, 8),
    (Format::R32G32Sint, vk::Format::R32G32_SINT, 8),
    // DXGI R10G10B10A2 is Vulkan's A2B10G10R10 (same bit layout).
    (Format::R10G10B10A2Unorm, vk::Format::A2B10G10R10_UNORM_PACK32, 4),
    (Format::R10G10B10A2Uint, vk::Format::A2B10G10R10_UINT_PACK32, 4),
    // Packed float used by some MagpieFX effects.
    (Format::R11G11B10Float, vk::Format::B10G11R11_UFLOAT_PACK32, 4),
    (Format::R8G8B8A8Unorm, vk::Format::R8G8B8A8_UNORM, 4),
    (Format::R8G8B8A8UnormSrgb, vk::Format::R8G8B8A8_SRGB, 4),
    (Format::R8G8B8A8Uint, vk::Format::R8G8B8A8_UINT, 4),
    (Format::R8G8B8A8Snorm, vk::Format::R8G8B8A8_SNORM, 4),
    (Format::R8G8B8A8Sint, vk::Format::R8G8B8A8_SINT, 4),
    (Format::R16G16Float, vk::Format::R16G16_SFLOAT, 4),
    (Format::R16G16Unorm, vk::Format::R16G16_UNORM, 4),
    (Format::R16G16Uint, vk::Format::R16G16_UINT, 4),
    (Format::R16G16Snorm, vk::Format::R16G16_SNORM, 4),
    (Format::R16G16Sint, vk::Format::R16G16_SINT, 4),
    (Format::R32Float, vk::Format::R32_SFLOAT, 4),
    (Format::R32Uint, vk::Format::R32_UINT, 4),
    (Format::R32Sint, vk::Format::R32_SINT, 4),
    (Format::R8G8Unorm, vk::Format::R8G8_UNORM, 2),
    (Format::R8G8Uint, vk::Format::R8G8_UINT, 2),
    (Format::R8G8Snorm, vk::Format::R8G8_SNORM, 2),
    (Format::R8G8Sint, vk::Format::R8G8_SINT, 2),
    (Format::R16Float, vk::Format::R16_SFLOAT, 2),
    (Format::R16Unorm, vk::Format::R16_UNORM, 2),
    (Format::R16Uint, vk::Format::R16_UINT, 2),
    (Format::R16Snorm, vk::Format::R16_SNORM, 2),
    (Format::R16Sint, vk::Format::R16_SINT, 2),
    (Format::R8Unorm, vk::Format::R8_UNORM, 1),
    (Format::R8Uint, vk::Format::R8_UINT, 1),
    (Format::R8Snorm, vk::Format::R8_SNORM, 1),
    (Format::R8Sint, vk::Format::R8_SINT, 1),
    (Format::B8G8R8A8Unorm, vk::Format::B8G8R8A8_UNORM, 4),
    (Format::B8G8R8A8UnormSrgb, vk::Format::B8G8R8A8_SRGB, 4),
];

/// How the texels of a format are read, for clear colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericKind {
    Float,
    Uint,
    Sint,
}

impl Format {
    fn entry(self) -> &'static (Format, vk::Format, u32) {
        // Every variant is in TABLE (checked by a test).
        TABLE
            .iter()
            .find(|e| e.0 == self)
            .expect("format table is complete")
    }

    /// The matching `VkFormat`.
    pub fn to_vk(self) -> vk::Format {
        self.entry().1
    }

    /// From a `VkFormat`, if it is in the table.
    pub fn from_vk(format: vk::Format) -> Option<Format> {
        TABLE.iter().find(|e| e.1 == format).map(|e| e.0)
    }

    /// From the DXGI number.
    pub fn from_dxgi(n: u32) -> Option<Format> {
        TABLE.iter().find(|e| e.0 as u32 == n).map(|e| e.0)
    }

    /// Bytes per texel.
    pub fn bytes_per_texel(self) -> u32 {
        self.entry().2
    }

    /// Whether texels are floats (float, unorm, snorm), unsigned or signed
    /// integers.
    pub(crate) fn numeric_kind(self) -> NumericKind {
        use Format::*;
        match self {
            R32G32B32A32Uint | R32G32B32Uint | R16G16B16A16Uint | R32G32Uint | R10G10B10A2Uint
            | R8G8B8A8Uint | R16G16Uint | R32Uint | R8G8Uint | R16Uint | R8Uint => NumericKind::Uint,
            R32G32B32A32Sint | R32G32B32Sint | R16G16B16A16Sint | R32G32Sint | R8G8B8A8Sint | R16G16Sint
            | R32Sint | R8G8Sint | R16Sint | R8Sint => NumericKind::Sint,
            _ => NumericKind::Float,
        }
    }

    /// Every format, in DXGI order.
    pub fn all() -> impl Iterator<Item = Format> {
        TABLE.iter().map(|e| e.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_round_trips() {
        for f in Format::all() {
            assert_eq!(Format::from_dxgi(f as u32), Some(f));
            assert_eq!(Format::from_vk(f.to_vk()), Some(f));
        }
    }

    #[test]
    fn dxgi_numbers_match_1x() {
        assert_eq!(Format::from_dxgi(28), Some(Format::R8G8B8A8Unorm));
        assert_eq!(Format::from_dxgi(87), Some(Format::B8G8R8A8Unorm));
        assert_eq!(Format::from_dxgi(10), Some(Format::R16G16B16A16Float));
        assert_eq!(Format::from_dxgi(1), None);
        assert_eq!(Format::from_dxgi(56), None);
    }

    #[test]
    fn texel_sizes() {
        assert_eq!(Format::R32G32B32A32Float.bytes_per_texel(), 16);
        assert_eq!(Format::R32G32B32Float.bytes_per_texel(), 12);
        assert_eq!(Format::R16G16B16A16Float.bytes_per_texel(), 8);
        assert_eq!(Format::B8G8R8A8Unorm.bytes_per_texel(), 4);
        assert_eq!(Format::R11G11B10Float.bytes_per_texel(), 4);
        assert_eq!(Format::R16Float.bytes_per_texel(), 2);
        assert_eq!(Format::R8Unorm.bytes_per_texel(), 1);
    }

    #[test]
    fn packed_formats() {
        assert_eq!(
            Format::R10G10B10A2Unorm.to_vk(),
            vk::Format::A2B10G10R10_UNORM_PACK32
        );
        assert_eq!(
            Format::R11G11B10Float.to_vk(),
            vk::Format::B10G11R11_UFLOAT_PACK32
        );
        assert_eq!(Format::R8G8B8A8UnormSrgb.to_vk(), vk::Format::R8G8B8A8_SRGB);
    }

    #[test]
    fn numeric_kinds() {
        assert_eq!(Format::R8G8B8A8Unorm.numeric_kind(), NumericKind::Float);
        assert_eq!(Format::R32Uint.numeric_kind(), NumericKind::Uint);
        assert_eq!(Format::R16G16Sint.numeric_kind(), NumericKind::Sint);
    }
}
