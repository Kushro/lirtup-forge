//! Pixel formats. The discriminants are the DXGI numbers of the 1.x format
//! table (`vulkan/__init__.py`), which MagpieFX's `//!FORMAT` names map to.

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

impl Format {
    /// The matching `VkFormat`.
    pub fn to_vk(self) -> ash::vk::Format {
        todo!("front G: port the format table of vk_*.cpp")
    }

    /// From a `VkFormat`, if it is in the table.
    pub fn from_vk(format: ash::vk::Format) -> Option<Format> {
        todo!("front G")
    }

    /// From the DXGI number.
    pub fn from_dxgi(n: u32) -> Option<Format> {
        todo!("front G")
    }

    /// Bytes per texel.
    pub fn bytes_per_texel(self) -> u32 {
        todo!("front G")
    }
}
