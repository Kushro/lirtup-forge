//! A plain RGBA image (icons, thumbnails), so this crate needs no image crate.

/// 8-bit RGBA, rows tightly packed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RgbaImage {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// `width * height * 4` bytes, R, G, B, A.
    pub data: Vec<u8>,
}
