//! OSD and stats panel drawing: text rasterized on the CPU with `ab_glyph`
//! into an RGBA image, uploaded, and blended over the output by
//! `misc/overlay_blender` (same shader as 1.x). Timing (what to show and for
//! how long) is `lirtup-pipeline::osd`.

use std::sync::Arc;

use lirtup_gpu::{Device, Texture};

use crate::{Pass, Result};

/// A rasterized block of text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextImage {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Premultiplied RGBA8.
    pub data: Vec<u8>,
}

/// Text style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Size in pixels.
    pub size_px: f32,
    /// Text color, RGBA 0 to 1.
    pub color: [f32; 4],
    /// Background box color (with padding), RGBA; `None` = none.
    pub background: Option<[f32; 4]>,
    /// Padding in pixels.
    pub padding: u32,
    /// Corner radius of the box.
    pub radius: u32,
}

/// Rasterizes text with one font.
pub struct TextRenderer {
    _private: (),
}

impl TextRenderer {
    /// From font bytes (TTF/OTF). The default font is the bundled one of
    /// round 3 (`assets/fonts/`); until then 1.x's.
    pub fn new(font: Vec<u8>) -> Result<TextRenderer> {
        todo!("front E")
    }

    /// Lay out and rasterize `text` (newlines allowed).
    pub fn render(&self, text: &str, style: &TextStyle) -> TextImage {
        todo!("front E")
    }
}

/// Blends an uploaded RGBA image over a texture at a position, in place.
pub struct OverlayBlend {
    _private: (),
}

impl OverlayBlend {
    /// Blend onto `target` (read-write).
    pub fn new(device: &Arc<Device>, target: &Texture, max_size: (u32, u32)) -> Result<OverlayBlend> {
        todo!("front E")
    }

    /// Replace the image (uploads on the next record); `None` hides it.
    pub fn set_image(&mut self, image: Option<&TextImage>, position: (i32, i32)) -> Result<()> {
        todo!("front E")
    }
}

impl Pass for OverlayBlend {
    fn label(&self) -> &str {
        "osd"
    }

    fn output(&self) -> &Texture {
        todo!("front E")
    }

    fn record(&self, recorder: &mut lirtup_gpu::CommandRecorder) -> Result<()> {
        todo!("front E")
    }
}
