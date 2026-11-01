//! OSD messages and their timing (`osd.py`); drawing is
//! `lirtup_effects::osd`. Front S, round 2.
//!
//! The session thread calls [`Osd::show`] for what 1.x showed («Modelo:
//! 2x16», «Zoom 150 %», «Captura guardada», «Generación: ×3», ...) and
//! [`Osd::update`] once per frame; the presenter records [`Osd::pass`] after
//! the effect chain. Texts come from the session thread already translated
//! (Spanish/English by `LANG`, as 1.x).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lirtup_config::Config;
use lirtup_effects::Pass;
use lirtup_effects::osd::{OverlayBlend, TextImage, TextRenderer, TextStyle};
use lirtup_gpu::{Device, Texture};
use lirtup_proto::Stage;

use crate::{PipelineError, Result};

/// Fonts of the OSD, in order (1.x: `DejaVuSans.ttf`).
const OSD_FONTS: &[&str] = &[
    "DejaVuSans.ttf",
    "LiberationSans-Regular.ttf",
    "NotoSans-Regular.ttf",
];

/// Cached message images; more than this and the cache starts over.
const CACHE_LIMIT: usize = 64;

/// The OSD of one session.
pub struct Osd {
    renderer: TextRenderer,
    blend: OverlayBlend,
    screen: (u32, u32),
    font_size: f32,
    enabled: bool,
    duration: Duration,
    /// Rendered messages by text, for the current font size.
    cache: HashMap<String, TextImage>,
    /// The message shown and when it goes away.
    active: Option<(String, Instant)>,
    /// `show` or `rebind` changed the image since the last `update`.
    changed: bool,
}

impl Osd {
    /// Create it over `screen` (the presenter's final texture), with
    /// `show_osd`, `osd_duration` and the font settings of `config`.
    pub fn new(device: &Arc<Device>, screen: &Texture, config: &Config) -> Result<Osd> {
        let renderer = TextRenderer::new(load_font(OSD_FONTS)?)?;
        let size = (screen.width(), screen.height());
        Ok(Osd {
            renderer,
            blend: OverlayBlend::new(device, screen, size)?,
            screen: size,
            font_size: osd_font_size(size.1),
            enabled: config.show_osd,
            duration: seconds(config.osd_duration),
            cache: HashMap::new(),
            active: None,
            changed: false,
        })
    }

    /// Show a message (replaces the current one and restarts its timer).
    /// Does nothing with `show_osd: false`.
    pub fn show(&mut self, text: &str) {
        if !self.enabled {
            return;
        }
        self.active = Some((text.to_owned(), Instant::now() + self.duration));
        if let Err(e) = self.upload() {
            tracing::error!("OSD message '{text}' failed: {e}");
            self.active = None;
        }
        self.changed = true;
    }

    /// Advance the timer (fade, expiry). `true` when the image changed this
    /// frame (the frame must be presented even if the window did not
    /// change).
    pub fn update(&mut self, now: Instant) -> Result<bool> {
        if self.active.as_ref().is_some_and(|(_, until)| now >= *until) {
            self.active = None;
            self.blend.set_image(None, (0, 0))?;
            self.changed = true;
        }
        Ok(std::mem::take(&mut self.changed))
    }

    /// New `show_osd` / `osd_duration` / font settings.
    pub fn update_config(&mut self, config: &Config) {
        self.enabled = config.show_osd;
        self.duration = seconds(config.osd_duration);
        if !self.enabled && self.active.take().is_some() {
            let _ = self.blend.set_image(None, (0, 0));
            self.changed = true;
        }
    }

    /// The screen texture was recreated (resize): blend onto the new one.
    pub fn rebind(&mut self, device: &Arc<Device>, screen: &Texture) -> Result<()> {
        let size = (screen.width(), screen.height());
        self.blend = OverlayBlend::new(device, screen, size)?;
        self.screen = size;
        let font_size = osd_font_size(size.1);
        if font_size != self.font_size {
            self.font_size = font_size;
            self.cache.clear();
        }
        if self.active.is_some() {
            self.upload()?;
            self.changed = true;
        }
        Ok(())
    }

    /// The blend pass, while a message is visible.
    pub fn pass(&self) -> Option<&dyn Pass> {
        self.active.as_ref().map(|_| &self.blend as &dyn Pass)
    }

    /// Render (or take from the cache) the active message and hand it to
    /// the blend, centered on the screen.
    fn upload(&mut self) -> Result<()> {
        let Some((text, _)) = &self.active else {
            return Ok(());
        };
        if !self.cache.contains_key(text) {
            if self.cache.len() >= CACHE_LIMIT {
                self.cache.clear();
            }
            let image = render_message(&self.renderer, text, self.font_size);
            self.cache.insert(text.clone(), image);
        }
        let image = &self.cache[text];
        let x = (self.screen.0 as i32 - image.width as i32) / 2;
        let y = (self.screen.1 as i32 - image.height as i32) / 2;
        self.blend.set_image(Some(image), (x.max(0), y.max(0)))?;
        Ok(())
    }
}

/// 1.x: 4 % of the screen height, at least 24 px.
fn osd_font_size(screen_height: u32) -> f32 {
    ((screen_height as f32 * 0.04) as u32).max(24) as f32
}

fn seconds(s: f32) -> Duration {
    Duration::from_secs_f32(if s.is_finite() { s.max(0.0) } else { 0.0 })
}

/// A message as 1.x draws it (`TextRenderer._render_to_cache`): white text
/// with a dark shadow on a translucent rounded box, padding half the font
/// size.
fn render_message(renderer: &TextRenderer, text: &str, size: f32) -> TextImage {
    let padding = (size * 0.5) as u32;
    let offset = ((size * 0.05) as u32).max(2);
    let plain = |color: [f32; 4]| TextStyle {
        size_px: size,
        color,
        background: None,
        padding: 0,
        radius: 0,
    };
    let shadow = renderer.render(text, &plain([0.0, 0.0, 0.0, 180.0 / 255.0]));
    let glyphs = renderer.render(text, &plain([1.0; 4]));
    let mut canvas = Canvas::new(
        glyphs.width + 2 * padding + offset,
        glyphs.height + 2 * padding + offset,
    );
    canvas.rounded_rect(padding as f32, [0.0, 0.0, 0.0, 180.0 / 255.0]);
    canvas.draw(&shadow, padding + offset, padding + offset);
    canvas.draw(&glyphs, padding, padding);
    canvas.finish()
}

/// A premultiplied RGBA canvas to compose text images on (the OSD shadow,
/// the two colored columns of the stats panel).
pub(crate) struct Canvas {
    width: u32,
    height: u32,
    /// Premultiplied RGBA, 0 to 1.
    pixels: Vec<[f32; 4]>,
}

impl Canvas {
    /// Transparent, at least 1×1.
    pub(crate) fn new(width: u32, height: u32) -> Canvas {
        let (width, height) = (width.max(1), height.max(1));
        Canvas {
            width,
            height,
            pixels: vec![[0.0; 4]; (width * height) as usize],
        }
    }

    /// Fill the whole canvas with a rounded rectangle (straight-alpha
    /// `color`, antialiased corners), under what is already drawn.
    pub(crate) fn rounded_rect(&mut self, radius: f32, color: [f32; 4]) {
        let (w, h) = (self.width as f32, self.height as f32);
        let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
        for y in 0..self.height {
            for x in 0..self.width {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let cx = px.clamp(r, w - r);
                let cy = py.clamp(r, h - r);
                let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let coverage = if r == 0.0 {
                    1.0
                } else {
                    (r - d + 0.5).clamp(0.0, 1.0)
                };
                let a = color[3] * coverage;
                let src = [color[0] * a, color[1] * a, color[2] * a, a];
                let dst = &mut self.pixels[(y * self.width + x) as usize];
                // Under: the new color goes behind the current pixel.
                let keep = 1.0 - dst[3];
                for c in 0..4 {
                    dst[c] += src[c] * keep;
                }
            }
        }
    }

    /// Draw a premultiplied image over the canvas at `(x, y)` (clipped).
    pub(crate) fn draw(&mut self, image: &TextImage, x: u32, y: u32) {
        for iy in 0..image.height {
            let cy = y + iy;
            if cy >= self.height {
                break;
            }
            for ix in 0..image.width {
                let cx = x + ix;
                if cx >= self.width {
                    break;
                }
                let i = ((iy * image.width + ix) * 4) as usize;
                let src = [
                    image.data[i] as f32 / 255.0,
                    image.data[i + 1] as f32 / 255.0,
                    image.data[i + 2] as f32 / 255.0,
                    image.data[i + 3] as f32 / 255.0,
                ];
                let dst = &mut self.pixels[(cy * self.width + cx) as usize];
                for c in 0..4 {
                    dst[c] = src[c] + dst[c] * (1.0 - src[3]);
                }
            }
        }
    }

    /// The premultiplied RGBA8 image.
    /// The image, premultiplied, **in B, G, R, A byte order**: the screen
    /// texture holds the captured frame's BGRA bytes as they are (1.x does
    /// the same; the swapchain copy and `screenshot` reorder), so what is
    /// blended onto it must be BGRA too.
    pub(crate) fn finish(self) -> TextImage {
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let data = self
            .pixels
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]].map(to8))
            .collect();
        TextImage {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

/// The bytes of the first font of `names` found in the usual font folders
/// (1.x asked fontconfig through PIL for the same file names).
pub(crate) fn load_font(names: &[&str]) -> Result<Vec<u8>> {
    let dirs = font_dirs();
    for name in names {
        for dir in &dirs {
            if let Some(path) = find_file(dir, name, 4) {
                return Ok(std::fs::read(path)?);
            }
        }
    }
    Err(PipelineError::other(
        Stage::Effects,
        format!("no font found ({})", names.join(", ")),
    ))
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        dirs.push(data.join("fonts"));
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/share/fonts"));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".fonts"));
    }
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
    dirs.extend(
        data_dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(|d| Path::new(d).join("fonts")),
    );
    dirs
}

/// `name` under `dir`, at most `depth` folders down.
fn find_file(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let candidate = dir.join(name);
    if candidate.is_file() {
        return Some(candidate);
    }
    if depth == 0 {
        return None;
    }
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    subdirs.sort();
    subdirs.into_iter().find_map(|d| find_file(&d, name, depth - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_size_follows_the_screen() {
        assert_eq!(osd_font_size(1080), 43.0);
        assert_eq!(osd_font_size(480), 24.0);
    }

    #[test]
    fn canvas_draws_over_and_fills_under() {
        let mut canvas = Canvas::new(2, 1);
        let dot = TextImage {
            width: 1,
            height: 1,
            data: vec![255, 255, 255, 255],
        };
        canvas.draw(&dot, 0, 0);
        canvas.rounded_rect(0.0, [0.0, 0.0, 0.0, 0.5]);
        let image = canvas.finish();
        assert_eq!(&image.data[..4], &[255, 255, 255, 255]);
        assert_eq!(&image.data[4..], &[0, 0, 0, 128]);
    }

    #[test]
    fn clipped_draws_stay_inside() {
        let mut canvas = Canvas::new(1, 1);
        let image = TextImage {
            width: 2,
            height: 2,
            data: vec![255; 16],
        };
        canvas.draw(&image, 0, 0);
        canvas.draw(&image, 5, 5);
        assert_eq!(canvas.finish().data, vec![255; 4]);
    }
}
