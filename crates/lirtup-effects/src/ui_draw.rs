//! Drawing for the bar over the game (round 4, front T): rounded
//! rectangles, text and Material Symbols icons blended onto the screen
//! texture in one pass, with antialiased edges. Fonts are rasterized with
//! `ab_glyph` into an atlas once per size; icons are looked up by code point
//! (`ab_glyph` applies no ligatures) in two static instances of Material
//! Symbols Rounded (FILL 0 and 1).
//!
//! Cost target (5.8): the whole bar ≤ 0.1 ms of GPU time at 4K.

use std::sync::Arc;

use lirtup_gpu::{Device, Texture};

use crate::{Pass, Result};

/// The fonts of the bar (bytes of `.ttf` files; `lirtup-ui/assets/fonts/`).
#[derive(Clone)]
pub struct UiFonts {
    /// Google Sans Flex (text).
    pub text: Arc<Vec<u8>>,
    /// JetBrains Mono (fps, ms).
    pub mono: Arc<Vec<u8>>,
    /// Material Symbols Rounded, FILL 0.
    pub icons: Arc<Vec<u8>>,
    /// Material Symbols Rounded, FILL 1 (selected states).
    pub icons_filled: Arc<Vec<u8>>,
}

/// Which font a text uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFont {
    /// Google Sans Flex.
    Text,
    /// JetBrains Mono.
    Mono,
}

/// One thing to draw, in screen pixels; colors are linear RGBA.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A filled rounded rectangle.
    RoundRect {
        /// Left, top, width, height.
        rect: [f32; 4],
        /// Corner radius.
        radius: f32,
        /// Fill.
        color: [f32; 4],
    },
    /// A line of text; `origin` is the left end of its baseline.
    Text {
        /// Left, baseline.
        origin: [f32; 2],
        /// Size, px.
        size: f32,
        /// Font.
        font: UiFont,
        /// The text.
        text: String,
        /// Color.
        color: [f32; 4],
    },
    /// An icon centered on `center`.
    Icon {
        /// Center.
        center: [f32; 2],
        /// Size, px.
        size: f32,
        /// Code point (`tokens::icon`).
        codepoint: u32,
        /// FILL 1.
        filled: bool,
        /// Color.
        color: [f32; 4],
    },
}

/// The drawing pass.
pub struct UiDraw {
    _device: Arc<Device>,
}

impl UiDraw {
    /// A pass that draws onto `target` (the screen texture).
    pub fn new(device: &Arc<Device>, target: &Texture, fonts: UiFonts) -> Result<UiDraw> {
        let _ = (device, target, fonts);
        todo!()
    }

    /// Replace what is drawn (empty = nothing; the pass then records no
    /// work).
    pub fn set_scene(&mut self, shapes: &[Shape]) -> Result<()> {
        let _ = shapes;
        todo!()
    }

    /// The target changed (resize).
    pub fn rebind(&mut self, device: &Arc<Device>, target: &Texture) -> Result<()> {
        let _ = (device, target);
        todo!()
    }

    /// The pass to record after the screen passes.
    pub fn pass(&self) -> &dyn Pass {
        todo!()
    }

    /// Width of `text` at `size`, px (layout of the bar).
    pub fn measure(&self, text: &str, size: f32, font: UiFont) -> f32 {
        let _ = (text, size, font);
        todo!()
    }
}
