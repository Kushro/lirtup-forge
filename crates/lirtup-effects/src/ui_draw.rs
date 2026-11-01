//! Drawing for the bar over the game (round 4, front T), the stats panel and
//! the OSD: rounded rectangles, text and Material Symbols icons with
//! antialiased edges, blended onto the screen texture in one pass.
//!
//! The scene is rasterized on the CPU into one image that
//! covers the shapes' bounding box, uploaded only when it changes, and
//! blended by `misc/overlay_blender` (the shader of the OSD), so the GPU
//! work is one dispatch over that box. Glyph coverage is cached per font,
//! glyph and size, so a redraw (the fps readout, a frame of the show
//! animation) only composes cached masks. Icons are looked up by code point
//! (`ab_glyph` applies no ligatures) in two static instances of Material
//! Symbols Rounded (FILL 0 and 1, `scripts/gen-tokens-icons.py`).
//!
//! Sizes are CSS pixels: `size` is the em size, as `font-size`.
//!
//! Cost target (5.8): the whole bar ≤ 0.1 ms of GPU time at 4K.

use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::{Font, FontArc, FontRef, GlyphId, PxScale, ScaleFont, VariableFont, point};
use lirtup_gpu::{Device, Texture};

use crate::osd::{OverlayBlend, TextImage};
use crate::{EffectError, Pass, Result};

/// The fonts of the bar, parsed once and shared (`FontArc` clones are
/// cheap). Built from the `.ttf` files of `lirtup-ui/assets/fonts/`.
#[derive(Clone)]
pub struct UiFonts {
    /// Google Sans Flex at wght 500 (labels).
    pub text: FontArc,
    /// Google Sans Flex at wght 600 (OSD, emphasis).
    pub text_strong: FontArc,
    /// JetBrains Mono at wght 400 (the panel).
    pub mono: FontArc,
    /// JetBrains Mono at wght 600 (fps).
    pub mono_strong: FontArc,
    /// Material Symbols Rounded, FILL 0.
    pub icons: FontArc,
    /// Material Symbols Rounded, FILL 1 (selected states).
    pub icons_filled: FontArc,
}

impl UiFonts {
    /// Parse the font files (`'static`: embedded with `include_bytes!`, so
    /// nothing is copied). `text` and `mono` are variable fonts; each
    /// weight is an instance of them.
    pub fn from_static(
        text: &'static [u8],
        mono: &'static [u8],
        icons: &'static [u8],
        icons_filled: &'static [u8],
    ) -> Result<UiFonts> {
        let weighted = |bytes: &'static [u8], wght: f32| -> Result<FontArc> {
            let mut font = FontRef::try_from_slice(bytes).map_err(|e| EffectError::Font(e.to_string()))?;
            font.set_variation(b"wght", wght);
            Ok(FontArc::new(font))
        };
        let plain = |bytes: &'static [u8]| -> Result<FontArc> {
            FontRef::try_from_slice(bytes)
                .map(FontArc::new)
                .map_err(|e| EffectError::Font(e.to_string()))
        };
        Ok(UiFonts {
            text: weighted(text, 500.0)?,
            text_strong: weighted(text, 600.0)?,
            mono: weighted(mono, 400.0)?,
            mono_strong: weighted(mono, 600.0)?,
            icons: plain(icons)?,
            icons_filled: plain(icons_filled)?,
        })
    }

    fn get(&self, face: Face) -> &FontArc {
        match face {
            Face::Text(UiFont::Text) => &self.text,
            Face::Text(UiFont::TextStrong) => &self.text_strong,
            Face::Text(UiFont::Mono) => &self.mono,
            Face::Text(UiFont::MonoStrong) => &self.mono_strong,
            Face::Icons { filled: false } => &self.icons,
            Face::Icons { filled: true } => &self.icons_filled,
        }
    }
}

/// Which font a text uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiFont {
    /// Google Sans Flex, wght 500.
    Text,
    /// Google Sans Flex, wght 600.
    TextStrong,
    /// JetBrains Mono, wght 400.
    Mono,
    /// JetBrains Mono, wght 600.
    MonoStrong,
}

/// A font of [`UiFonts`], for the glyph cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Face {
    Text(UiFont),
    Icons { filled: bool },
}

/// One thing to draw, in screen pixels; colors are RGBA 0 to 1, straight
/// alpha, in the screen's encoding (`lirtup_pipeline::tokens::color`).
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
        /// Size (em), px.
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
        /// Size (em), px.
        size: f32,
        /// Code point (`tokens::icon`).
        codepoint: u32,
        /// FILL 1.
        filled: bool,
        /// Color.
        color: [f32; 4],
    },
}

/// Cached coverage of one glyph at one size.
struct Mask {
    /// Offset of the top-left pixel from the pen (baseline origin).
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    coverage: Vec<u8>,
}

/// Glyph cache entries before it starts over (a few sizes of ~100 glyphs).
const GLYPH_CACHE_LIMIT: usize = 2048;

/// The drawing pass.
pub struct UiDraw {
    fonts: UiFonts,
    blend: OverlayBlend,
    max_size: (u32, u32),
    target_size: (u32, u32),
    label: &'static str,
    glyphs: HashMap<(Face, GlyphId, u32), Option<Mask>>,
    /// Something is drawn.
    shown: bool,
}

impl UiDraw {
    /// A pass that draws onto `target` (the screen texture). `max_size`
    /// bounds the image (and its upload buffer); what does not fit is
    /// cropped. `label` names the pass in the GPU times.
    pub fn new(
        device: &Arc<Device>,
        target: &Texture,
        fonts: UiFonts,
        max_size: (u32, u32),
        label: &'static str,
    ) -> Result<UiDraw> {
        let target_size = (target.width(), target.height());
        let max_size = clamp_size(max_size, target_size);
        Ok(UiDraw {
            fonts,
            blend: OverlayBlend::with_label(device, target, max_size, label)?,
            max_size,
            target_size,
            label,
            glyphs: HashMap::new(),
            shown: false,
        })
    }

    /// Replace what is drawn (empty = nothing; the pass then records no
    /// work). The shapes are drawn in order, each over the previous ones.
    pub fn set_scene(&mut self, shapes: &[Shape]) -> Result<()> {
        let Some((x, y, image)) = self.rasterize(shapes) else {
            self.shown = false;
            return self.blend.set_image(None, (0, 0));
        };
        self.shown = true;
        self.blend.set_image(Some(&image), (x, y))
    }

    /// The target changed (resize). The scene must be set again.
    pub fn rebind(&mut self, device: &Arc<Device>, target: &Texture, max_size: (u32, u32)) -> Result<()> {
        self.target_size = (target.width(), target.height());
        self.max_size = clamp_size(max_size, self.target_size);
        self.blend = OverlayBlend::with_label(device, target, self.max_size, self.label)?;
        self.shown = false;
        Ok(())
    }

    /// Something is drawn.
    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// The pass to record after the screen passes. It records nothing
    /// while the scene is empty.
    pub fn pass(&self) -> &dyn Pass {
        &self.blend
    }

    /// Width of `text` at `size`, px (layout).
    pub fn measure(&self, text: &str, size: f32, font: UiFont) -> f32 {
        let font = self.fonts.get(Face::Text(font));
        let scaled = font.as_scaled(em_scale(font, size));
        let mut x = 0.0;
        let mut prev = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                x += scaled.kern(p, id);
            }
            x += scaled.h_advance(id);
            prev = Some(id);
        }
        x
    }

    /// Ascent and descent (negative) of `font` at `size`, px.
    pub fn metrics(&self, size: f32, font: UiFont) -> (f32, f32) {
        let font = self.fonts.get(Face::Text(font));
        let scaled = font.as_scaled(em_scale(font, size));
        (scaled.ascent(), scaled.descent())
    }

    /// The scene as one image and where it goes, clipped to the target and
    /// to `max_size` (`None` = nothing visible).
    fn rasterize(&mut self, shapes: &[Shape]) -> Option<(i32, i32, TextImage)> {
        let mut bounds: Option<[f32; 4]> = None;
        for shape in shapes {
            let b = self.bounds(shape);
            bounds = Some(match bounds {
                None => b,
                Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])],
            });
        }
        let [x0, y0, x1, y1] = bounds?;
        let left = (x0.floor() as i32).max(0);
        let top = (y0.floor() as i32).max(0);
        let right = (x1.ceil() as i32).min(self.target_size.0 as i32);
        let bottom = (y1.ceil() as i32).min(self.target_size.1 as i32);
        let width = (right - left).min(self.max_size.0 as i32);
        let height = (bottom - top).min(self.max_size.1 as i32);
        if width <= 0 || height <= 0 {
            return None;
        }
        let mut canvas = Canvas {
            left,
            top,
            width: width as u32,
            height: height as u32,
            pixels: vec![[0.0; 4]; (width * height) as usize],
        };
        for shape in shapes {
            match shape {
                Shape::RoundRect { rect, radius, color } => canvas.round_rect(*rect, *radius, *color),
                Shape::Text {
                    origin,
                    size,
                    font,
                    text,
                    color,
                } => self.draw_text(&mut canvas, *origin, *size, *font, text, *color),
                Shape::Icon {
                    center,
                    size,
                    codepoint,
                    filled,
                    color,
                } => self.draw_icon(&mut canvas, *center, *size, *codepoint, *filled, *color),
            }
        }
        Some((left, top, canvas.finish()))
    }

    /// Left, top, right, bottom of a shape (generous for glyphs).
    fn bounds(&self, shape: &Shape) -> [f32; 4] {
        match shape {
            Shape::RoundRect { rect, .. } => [rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]],
            Shape::Text {
                origin,
                size,
                font,
                text,
                ..
            } => {
                let (ascent, descent) = self.metrics(*size, *font);
                let w = self.measure(text, *size, *font);
                [
                    origin[0] - 1.0,
                    origin[1] - ascent - 1.0,
                    origin[0] + w + 1.0,
                    origin[1] - descent + 1.0,
                ]
            }
            Shape::Icon { center, size, .. } => {
                let h = size * 0.5 + 1.0;
                [center[0] - h, center[1] - h, center[0] + h, center[1] + h]
            }
        }
    }

    /// The cached mask of `glyph` in `face` at `size`.
    fn mask(&mut self, face: Face, glyph: GlyphId, size: f32) -> Option<&Mask> {
        let key = (face, glyph, (size * 4.0).round() as u32);
        if !self.glyphs.contains_key(&key) {
            if self.glyphs.len() >= GLYPH_CACHE_LIMIT {
                self.glyphs.clear();
            }
            let font = self.fonts.get(face);
            let scale = em_scale(font, size);
            let mask = font
                .outline_glyph(glyph.with_scale_and_position(scale, point(0.0, 0.0)))
                .map(|outlined| {
                    let b = outlined.px_bounds();
                    let width = b.width() as u32;
                    let height = b.height() as u32;
                    let mut coverage = vec![0u8; (width * height) as usize];
                    outlined.draw(|x, y, c| {
                        if x < width && y < height {
                            coverage[(y * width + x) as usize] = (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                        }
                    });
                    Mask {
                        left: b.min.x as i32,
                        top: b.min.y as i32,
                        width,
                        height,
                        coverage,
                    }
                });
            self.glyphs.insert(key, mask);
        }
        self.glyphs[&key].as_ref()
    }

    fn draw_text(
        &mut self,
        canvas: &mut Canvas,
        origin: [f32; 2],
        size: f32,
        font: UiFont,
        text: &str,
        color: [f32; 4],
    ) {
        let face = Face::Text(font);
        let f = self.fonts.get(face).clone();
        let scaled = f.as_scaled(em_scale(&f, size));
        let mut x = origin[0];
        let baseline = origin[1].round() as i32;
        let mut prev = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                x += scaled.kern(p, id);
            }
            let pen = x.round() as i32;
            if let Some(mask) = self.mask(face, id, size) {
                canvas.mask(mask, pen, baseline, color);
            }
            x += scaled.h_advance(id);
            prev = Some(id);
        }
    }

    fn draw_icon(
        &mut self,
        canvas: &mut Canvas,
        center: [f32; 2],
        size: f32,
        codepoint: u32,
        filled: bool,
        color: [f32; 4],
    ) {
        let Some(c) = char::from_u32(codepoint) else {
            return;
        };
        let face = Face::Icons { filled };
        let id = self.fonts.get(face).glyph_id(c);
        if id.0 == 0 {
            tracing::debug!("icon U+{codepoint:04X} is not in the bar's font");
            return;
        }
        let (cx, cy) = (center[0].round() as i32, center[1].round() as i32);
        if let Some(mask) = self.mask(face, id, size) {
            // Centered on its own box: Material Symbols are drawn in a
            // square em box, centered.
            let pen_x = cx - mask.left - mask.width as i32 / 2;
            let pen_y = cy - mask.top - mask.height as i32 / 2;
            canvas.mask(mask, pen_x, pen_y, color);
        }
    }
}

/// The `PxScale` that makes `size` the em size (`font-size` in CSS):
/// `ab_glyph` scales by ascent − descent.
fn em_scale(font: &impl Font, size: f32) -> PxScale {
    let upem = font.units_per_em().unwrap_or(1000.0);
    PxScale::from(size.max(1.0) * font.height_unscaled() / upem)
}

fn clamp_size(max: (u32, u32), target: (u32, u32)) -> (u32, u32) {
    (max.0.min(target.0).max(1), max.1.min(target.1).max(1))
}

/// A premultiplied RGBA canvas at `(left, top)` of the screen.
struct Canvas {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    pixels: Vec<[f32; 4]>,
}

impl Canvas {
    /// Blend a straight-alpha color with `coverage` over pixel `(x, y)` of
    /// the screen.
    fn over(&mut self, x: i32, y: i32, color: [f32; 4], coverage: f32) {
        let (cx, cy) = (x - self.left, y - self.top);
        if cx < 0 || cy < 0 || cx >= self.width as i32 || cy >= self.height as i32 {
            return;
        }
        let a = color[3] * coverage;
        if a <= 0.0 {
            return;
        }
        let dst = &mut self.pixels[(cy as u32 * self.width + cx as u32) as usize];
        let keep = 1.0 - a;
        for c in 0..3 {
            dst[c] = color[c] * a + dst[c] * keep;
        }
        dst[3] = a + dst[3] * keep;
    }

    /// A rounded rectangle with antialiased edges (signed distance).
    fn round_rect(&mut self, rect: [f32; 4], radius: f32, color: [f32; 4]) {
        let [x, y, w, h] = rect;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
        let (x0, y0) = (x.floor() as i32, y.floor() as i32);
        let (x1, y1) = ((x + w).ceil() as i32, (y + h).ceil() as i32);
        let (hw, hh) = (w / 2.0, h / 2.0);
        let (mx, my) = (x + hw, y + hh);
        for py in y0.max(self.top)..y1.min(self.top + self.height as i32) {
            for px in x0.max(self.left)..x1.min(self.left + self.width as i32) {
                let qx = (px as f32 + 0.5 - mx).abs() - (hw - r);
                let qy = (py as f32 + 0.5 - my).abs() - (hh - r);
                let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
                let d = outside + qx.max(qy).min(0.0) - r;
                let coverage = (0.5 - d).clamp(0.0, 1.0);
                self.over(px, py, color, coverage);
            }
        }
    }

    /// A glyph mask with its pen at `(pen_x, baseline)`.
    fn mask(&mut self, mask: &Mask, pen_x: i32, baseline: i32, color: [f32; 4]) {
        let (x0, y0) = (pen_x + mask.left, baseline + mask.top);
        for gy in 0..mask.height {
            for gx in 0..mask.width {
                let c = mask.coverage[(gy * mask.width + gx) as usize];
                if c != 0 {
                    self.over(x0 + gx as i32, y0 + gy as i32, color, f32::from(c) / 255.0);
                }
            }
        }
    }

    /// **Straight** alpha, **B, G, R, A** bytes. The screen texture holds
    /// the captured BGRA bytes as they are, and `overlay_blend.hlsl`
    /// computes `rgb · a + screen · (1 − a)`: its header says
    /// premultiplied, but a premultiplied image would be darkened twice.
    fn finish(self) -> TextImage {
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let data = self
            .pixels
            .iter()
            .flat_map(|p| {
                let a = p[3];
                let un = |c: f32| if a > 0.0 { c / a } else { 0.0 };
                [un(p[2]), un(p[1]), un(p[0]), a].map(to8)
            })
            .collect();
        TextImage {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(w: u32, h: u32) -> Canvas {
        Canvas {
            left: 0,
            top: 0,
            width: w,
            height: h,
            pixels: vec![[0.0; 4]; (w * h) as usize],
        }
    }

    #[test]
    fn round_rect_is_antialiased_and_filled() {
        let mut c = canvas(20, 20);
        c.round_rect([0.0, 0.0, 20.0, 20.0], 8.0, [1.0, 0.0, 0.0, 1.0]);
        let at = |x: u32, y: u32| c.pixels[(y * 20 + x) as usize];
        assert_eq!(at(10, 10), [1.0, 0.0, 0.0, 1.0]);
        assert!(at(0, 0)[3] < 0.05, "corner {:?}", at(0, 0));
        assert_eq!(at(10, 0)[3], 1.0);
        // The edge of the arc is partly covered.
        let edge = (0..20).map(|x| at(x, 1)[3]).find(|a| *a > 0.0 && *a < 1.0);
        assert!(edge.is_some());
    }

    #[test]
    fn draws_over_in_order_and_finishes_bgra() {
        let mut c = canvas(1, 1);
        c.over(0, 0, [0.0, 0.0, 1.0, 1.0], 1.0);
        c.over(0, 0, [1.0, 0.0, 0.0, 0.5], 1.0);
        let image = c.finish();
        // Red at 0.5 over opaque blue: (0.5, 0, 0.5, 1), stored B, G, R, A.
        assert_eq!(image.data, vec![128, 0, 128, 255]);
        // A half-covered pixel keeps its color: straight alpha.
        let mut c = canvas(1, 1);
        c.over(0, 0, [1.0, 0.5, 0.0, 1.0], 0.5);
        assert_eq!(c.finish().data, vec![0, 128, 255, 128]);
    }

    #[test]
    fn clipped_pixels_are_ignored() {
        let mut c = canvas(2, 2);
        c.left = 10;
        c.top = 10;
        c.over(0, 0, [1.0; 4], 1.0);
        c.over(12, 12, [1.0; 4], 1.0);
        c.over(11, 11, [1.0; 4], 1.0);
        assert_eq!(c.pixels.iter().filter(|p| p[3] > 0.0).count(), 1);
    }
}
