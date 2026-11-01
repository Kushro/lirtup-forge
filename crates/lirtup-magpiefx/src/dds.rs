//! `//!SOURCE` lookup textures: `.dds` (and PNG/BMP/... through `image` in
//! front E's choice) (`dds.py`).

use crate::Result;

/// A decoded lookup texture.
#[derive(Debug, Clone, PartialEq)]
pub struct DdsImage {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// DXGI format name of the data (`R8G8B8A8_UNORM`, `R16G16B16A16_FLOAT`, ...).
    pub format: String,
    /// Texel bytes, top mip only.
    pub data: Vec<u8>,
}

/// Parse a `.dds` file.
pub fn parse_dds(data: &[u8]) -> Result<DdsImage> {
    todo!("front E")
}

/// Load a lookup texture from disk (by extension).
pub fn load_image(path: &std::path::Path) -> Result<DdsImage> {
    todo!("front E")
}

/// The format to create the texture with, from the declared `//!FORMAT` and
/// the file (`source_format`).
pub fn source_format(declared: &str, image: &DdsImage) -> Result<String> {
    todo!("front E")
}
