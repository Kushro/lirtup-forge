//! OSD and stats panel drawing: text rasterized on the CPU with `ab_glyph`
//! into an RGBA image, uploaded, and blended over the output by
//! `misc/overlay_blender` (same shader as 1.x). Timing (what to show and for
//! how long) is `lirtup-pipeline::osd`. Since round 4 the OSD, the panel and
//! the bar over the game compose their images with [`crate::ui_draw`];
//! [`OverlayBlend`] is the pass of all three.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use lirtup_gpu::{Buffer, BufferDesc, CopyRegion, Device, Dispatch, Filter, Format, MemoryLocation, Texture};

use crate::util::{self, Bindings, CbWriter};
use crate::{EffectError, Pass, Result};

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
    font: FontVec,
}

impl TextRenderer {
    /// From font bytes (TTF/OTF). The default font is the bundled one of
    /// round 3 (`assets/fonts/`); until then 1.x's.
    pub fn new(font: Vec<u8>) -> Result<TextRenderer> {
        let font = FontVec::try_from_vec(font).map_err(|e| EffectError::Font(e.to_string()))?;
        Ok(TextRenderer { font })
    }

    /// Lay out and rasterize `text` (newlines allowed).
    pub fn render(&self, text: &str, style: &TextStyle) -> TextImage {
        let scale = PxScale::from(style.size_px.max(1.0));
        let font = self.font.as_scaled(scale);
        let line_height = font.height() + font.line_gap();
        let pad = style.padding as f32;

        // Layout: glyphs with their pen positions, line by line.
        let mut glyphs = Vec::new();
        let mut text_w: f32 = 0.0;
        let lines: Vec<&str> = text.split('\n').collect();
        for (row, line) in lines.iter().enumerate() {
            let baseline = pad + font.ascent() + row as f32 * line_height;
            let mut x = pad;
            let mut prev = None;
            for c in line.chars() {
                let id = font.glyph_id(c);
                if let Some(p) = prev {
                    x += font.kern(p, id);
                }
                glyphs.push(id.with_scale_and_position(scale, point(x, baseline)));
                x += font.h_advance(id);
                prev = Some(id);
            }
            text_w = text_w.max(x - pad);
        }
        let text_h = line_height * lines.len() as f32 - font.line_gap();
        let width = (text_w + 2.0 * pad).ceil().max(1.0) as u32;
        let height = (text_h + 2.0 * pad).ceil().max(1.0) as u32;

        // Straight-alpha float canvas, then premultiplied bytes.
        let mut canvas = vec![[0f32; 4]; (width * height) as usize];
        if let Some(bg) = style.background {
            fill_rounded_rect(&mut canvas, width, height, style.radius as f32, bg);
        }
        for glyph in glyphs {
            let Some(outlined) = self.font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let x = bounds.min.x as i32 + gx as i32;
                let y = bounds.min.y as i32 + gy as i32;
                if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                    return;
                }
                let px = &mut canvas[(y as u32 * width + x as u32) as usize];
                blend_over(px, style.color, coverage);
            });
        }

        let mut data = Vec::with_capacity(canvas.len() * 4);
        for [r, g, b, a] in canvas {
            let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            data.extend_from_slice(&[to8(r * a), to8(g * a), to8(b * a), to8(a)]);
        }
        TextImage { width, height, data }
    }
}

/// `src` with alpha scaled by `coverage` over `dst` (straight alpha).
fn blend_over(dst: &mut [f32; 4], src: [f32; 4], coverage: f32) {
    let sa = src[3] * coverage.clamp(0.0, 1.0);
    let da = dst[3];
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        *dst = [0.0; 4];
        return;
    }
    for c in 0..3 {
        dst[c] = (src[c] * sa + dst[c] * da * (1.0 - sa)) / out_a;
    }
    dst[3] = out_a;
}

/// Fill the whole canvas with a rounded rectangle (antialiased corners).
fn fill_rounded_rect(canvas: &mut [[f32; 4]], width: u32, height: u32, radius: f32, color: [f32; 4]) {
    let r = radius.min(width as f32 / 2.0).min(height as f32 / 2.0).max(0.0);
    for y in 0..height {
        for x in 0..width {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // distance outside the inner rectangle shrunk by r
            let cx = px.clamp(r, width as f32 - r);
            let cy = py.clamp(r, height as f32 - r);
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            let coverage = if r == 0.0 {
                1.0
            } else {
                (r - d + 0.5).clamp(0.0, 1.0)
            };
            blend_over(&mut canvas[(y * width + x) as usize], color, coverage);
        }
    }
}

/// Blends an uploaded RGBA image over a texture at a position, in place.
pub struct OverlayBlend {
    target: Texture,
    image: Texture,
    staging: Buffer,
    cb: Buffer,
    pipeline: lirtup_gpu::ComputePipeline,
    max_size: (u32, u32),
    /// Name of the pass and of its timestamp.
    label: &'static str,
    /// `(x, y, w, h)` of the visible image.
    shown: Option<(i32, i32, u32, u32)>,
    pending_upload: AtomicBool,
}

impl OverlayBlend {
    /// Blend onto `target` (read-write), as the pass `osd`.
    pub fn new(device: &Arc<Device>, target: &Texture, max_size: (u32, u32)) -> Result<OverlayBlend> {
        OverlayBlend::with_label(device, target, max_size, "osd")
    }

    /// The same, named `label` in the GPU times (`toolbar` for the bar).
    pub fn with_label(
        device: &Arc<Device>,
        target: &Texture,
        max_size: (u32, u32),
        label: &'static str,
    ) -> Result<OverlayBlend> {
        let max_size = (max_size.0.max(1), max_size.1.max(1));
        let image = util::texture(device, max_size.0, max_size.1, Format::R8G8B8A8Unorm, "osd image")?;
        let staging = device.create_buffer(&BufferDesc {
            size: max_size.0 as u64 * max_size.1 as u64 * 4,
            stride: 0,
            format: None,
            location: MemoryLocation::Upload,
            label: "osd staging".to_owned(),
        })?;
        let cb = util::constant_buffer(device, 16, "osd cb")?;
        let point = util::sampler(device, Filter::Point)?;
        let spirv =
            crate::assets::native_spirv("misc/overlay_blender/overlay_blend.spv").ok_or_else(|| {
                EffectError::Asset {
                    name: "misc/overlay_blender/overlay_blend.spv".to_owned(),
                    message: "not embedded".to_owned(),
                }
            })?;
        let pipeline = util::compute(
            device,
            spirv,
            "main",
            &Bindings {
                cb: Some(&cb),
                srv: &[target, &image],
                uav: &[target],
                samplers: &[&point],
                push_size: 0,
            },
            "osd",
        )?;
        Ok(OverlayBlend {
            target: target.clone(),
            image,
            staging,
            cb,
            pipeline,
            max_size,
            label,
            shown: None,
            pending_upload: AtomicBool::new(false),
        })
    }

    /// Replace the image (uploads on the next record); `None` hides it.
    /// Images larger than `max_size` are cropped.
    pub fn set_image(&mut self, image: Option<&TextImage>, position: (i32, i32)) -> Result<()> {
        let Some(image) = image else {
            self.shown = None;
            return Ok(());
        };
        let w = image.width.min(self.max_size.0);
        let h = image.height.min(self.max_size.1);
        if w == 0 || h == 0 {
            self.shown = None;
            return Ok(());
        }
        let src_stride = image.width as usize * 4;
        let row = w as usize * 4;
        self.staging.write_with(|mapped| {
            for y in 0..h as usize {
                mapped[y * row..(y + 1) * row]
                    .copy_from_slice(&image.data[y * src_stride..y * src_stride + row]);
            }
        })?;
        let cb = CbWriter::new()
            .i32(position.0)
            .i32(position.1)
            .i32(w as i32)
            .i32(h as i32);
        self.cb.write(0, &cb.finish())?;
        self.shown = Some((position.0, position.1, w, h));
        self.pending_upload.store(true, Ordering::Release);
        Ok(())
    }
}

impl Pass for OverlayBlend {
    fn label(&self) -> &str {
        self.label
    }

    fn output(&self) -> &Texture {
        &self.target
    }

    fn record(&self, recorder: &mut lirtup_gpu::CommandRecorder) -> Result<()> {
        let Some((_, _, w, h)) = self.shown else {
            return Ok(());
        };
        if self.pending_upload.swap(false, Ordering::AcqRel) {
            recorder.copy_buffer_to_texture(
                &self.staging,
                &self.image,
                &[CopyRegion {
                    src_row_length: w,
                    width: w,
                    height: h,
                    ..CopyRegion::default()
                }],
            );
        }
        recorder.dispatch(&Dispatch {
            pipeline: &self.pipeline,
            groups: util::groups((w, h), (16, 16)),
            push: &[],
            timestamp: Some(self.label),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blending_is_premultiplied_ready() {
        let mut px = [0.0; 4];
        blend_over(&mut px, [1.0, 0.5, 0.0, 1.0], 0.5);
        assert_eq!(px, [1.0, 0.5, 0.0, 0.5]);
    }

    #[test]
    fn rounded_rect_corners_are_transparent() {
        let mut canvas = vec![[0f32; 4]; 100];
        fill_rounded_rect(&mut canvas, 10, 10, 4.0, [0.0, 0.0, 0.0, 1.0]);
        assert!(canvas[0][3] < 0.2);
        assert_eq!(canvas[55][3], 1.0);
    }
}
