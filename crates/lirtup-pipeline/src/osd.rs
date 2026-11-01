//! OSD messages and their timing (`osd.py`); drawing is
//! `lirtup_effects::ui_draw`. Front S, round 2; restyled in round 4 (front
//! T) after the «Superposición» artboard of the design.
//!
//! The session thread calls [`Osd::show`] for what 1.x showed («Modelo:
//! 2x16», «Zoom 150 %», «Captura guardada», «Generación: ×3», ...) and
//! [`Osd::update`] once per frame; the presenter records [`Osd::pass`] after
//! the effect chain. Texts come from the session thread already translated
//! (Spanish/English by `LANG`, as 1.x).
//!
//! A message is a `primary-container` pill (48 px high, 20 px of padding,
//! Google Sans Flex 16/600 in `on-primary-container`), centered at 72 % of
//! the height. It comes in with the `fast-spatial` spring (rising 16 px)
//! and leaves with the `effects` fade once `osd_duration` is over.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lirtup_config::Config;
use lirtup_effects::Pass;
use lirtup_effects::ui_draw::{Shape, UiDraw, UiFont};
use lirtup_gpu::{Device, Texture};

use crate::Result;
use crate::tokens::{self, Scheme, color, with_alpha};
use crate::toolbar::{ui_fonts, ui_scale};

/// CSS px of the pill.
const PILL_H: f32 = 48.0;
const PILL_PAD: f32 = 20.0;
const TEXT: f32 = 16.0;
/// How far it rises while it comes in.
const RISE: f32 = 16.0;
/// Its center, as a fraction of the screen height (`bottom: 28%`).
const CENTER_Y: f32 = 0.72;
/// The widest message, CSS px (longer ones are cropped).
const MAX_W: f32 = 1200.0;

/// The OSD of one session.
pub struct Osd {
    draw: UiDraw,
    screen: (u32, u32),
    scale: f32,
    enabled: bool,
    duration: Duration,
    /// The message shown, when it came and when it starts to leave.
    active: Option<Message>,
    /// The image must be drawn again.
    dirty: bool,
}

#[derive(Debug, Clone)]
struct Message {
    text: String,
    shown: Instant,
    until: Instant,
}

impl Osd {
    /// Create it over `screen` (the presenter's final texture), with
    /// `show_osd` and `osd_duration` of `config`.
    pub fn new(device: &Arc<Device>, screen: &Texture, config: &Config) -> Result<Osd> {
        let size = (screen.width(), screen.height());
        let scale = ui_scale(size);
        Ok(Osd {
            draw: UiDraw::new(device, screen, ui_fonts()?, max_image(scale), "osd")?,
            screen: size,
            scale,
            enabled: config.show_osd,
            duration: seconds(config.osd_duration),
            active: None,
            dirty: false,
        })
    }

    /// Show a message (replaces the current one and restarts its timer).
    /// Does nothing with `show_osd: false`.
    pub fn show(&mut self, text: &str) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        // A message replacing another one does not come in again.
        let shown = match &self.active {
            Some(m) if now < m.until => {
                now - Duration::from_millis(u64::from(tokens::MOTION_FAST_SPATIAL.ms))
            }
            _ => now,
        };
        self.active = Some(Message {
            text: text.to_owned(),
            shown,
            until: now + self.duration,
        });
        self.dirty = true;
    }

    /// Advance the timer and the animations. `true` when the image changed
    /// this frame (the frame must be presented even if the window did not
    /// change).
    pub fn update(&mut self, now: Instant) -> Result<bool> {
        if let Some(m) = &self.active {
            let gone = m.until + Duration::from_millis(u64::from(tokens::MOTION_EFFECTS.ms));
            if now >= gone {
                self.active = None;
                self.dirty = true;
            } else if now < m.shown + Duration::from_millis(u64::from(tokens::MOTION_FAST_SPATIAL.ms))
                || now >= m.until
            {
                self.dirty = true;
            }
        }
        if !std::mem::take(&mut self.dirty) {
            return Ok(false);
        }
        let shapes = self.shapes(now);
        self.draw.set_scene(&shapes)?;
        Ok(true)
    }

    /// New `show_osd` / `osd_duration`.
    pub fn update_config(&mut self, config: &Config) {
        self.enabled = config.show_osd;
        self.duration = seconds(config.osd_duration);
        if !self.enabled && self.active.take().is_some() {
            self.dirty = true;
        }
    }

    /// The screen texture was recreated (resize): blend onto the new one.
    pub fn rebind(&mut self, device: &Arc<Device>, screen: &Texture) -> Result<()> {
        self.screen = (screen.width(), screen.height());
        self.scale = ui_scale(self.screen);
        self.draw.rebind(device, screen, max_image(self.scale))?;
        self.dirty = self.active.is_some();
        Ok(())
    }

    /// The blend pass, while a message is visible.
    pub fn pass(&self) -> Option<&dyn Pass> {
        self.draw.is_shown().then(|| self.draw.pass())
    }

    /// The pill at `now`: rising in, in place, or fading out.
    fn shapes(&self, now: Instant) -> Vec<Shape> {
        let Some(m) = &self.active else {
            return Vec::new();
        };
        let progress = |since: Instant, motion: &tokens::Motion| {
            now.saturating_duration_since(since).as_secs_f32() * 1000.0 / motion.ms.max(1) as f32
        };
        let rise = tokens::ease(
            &tokens::MOTION_FAST_SPATIAL,
            progress(m.shown, &tokens::MOTION_FAST_SPATIAL),
        );
        let fade = if now >= m.until {
            1.0 - tokens::ease(
                &tokens::MOTION_EFFECTS,
                progress(m.until, &tokens::MOTION_EFFECTS),
            )
        } else {
            1.0
        };
        let alpha = (rise.clamp(0.0, 1.0) * fade).clamp(0.0, 1.0);
        if alpha <= 0.001 {
            return Vec::new();
        }
        let s = self.scale;
        let size = TEXT * s;
        let font = UiFont::TextStrong;
        let text_w = self
            .draw
            .measure(&m.text, size, font)
            .min((MAX_W - 2.0 * PILL_PAD) * s);
        let (w, h) = (text_w + 2.0 * PILL_PAD * s, PILL_H * s);
        let x = ((self.screen.0 as f32 - w) / 2.0).round();
        let cy = self.screen.1 as f32 * CENTER_Y + (1.0 - rise) * RISE * s;
        let y = (cy - h / 2.0).round();
        let (ascent, descent) = self.draw.metrics(size, font);
        let scheme = Scheme::Dark;
        vec![
            Shape::RoundRect {
                rect: [x, y, w, h],
                radius: h / 2.0,
                color: with_alpha(color("primary-container", scheme), alpha),
            },
            Shape::Text {
                origin: [x + PILL_PAD * s, (y + h / 2.0 + (ascent + descent) / 2.0).round()],
                size,
                font,
                text: m.text.clone(),
                color: with_alpha(color("on-primary-container", scheme), alpha),
            },
        ]
    }
}

/// The largest image of a message.
fn max_image(scale: f32) -> (u32, u32) {
    (
        (MAX_W * scale).ceil() as u32 + 2,
        ((PILL_H + RISE) * scale).ceil() as u32 + 2,
    )
}

fn seconds(s: f32) -> Duration {
    Duration::from_secs_f32(if s.is_finite() { s.max(0.0) } else { 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_sane() {
        assert_eq!(seconds(1.5), Duration::from_millis(1500));
        assert_eq!(seconds(-1.0), Duration::ZERO);
        assert_eq!(seconds(f32::NAN), Duration::ZERO);
    }

    #[test]
    fn the_image_holds_the_pill_while_it_rises() {
        let (w, h) = max_image(1.5);
        assert!(w as f32 >= MAX_W * 1.5 && h as f32 >= (PILL_H + RISE) * 1.5);
    }
}
