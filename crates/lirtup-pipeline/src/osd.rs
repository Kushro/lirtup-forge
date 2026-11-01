//! OSD messages and their timing (`osd.py`); drawing is
//! `lirtup_effects::osd`. Front S, round 2.
//!
//! The session thread calls [`Osd::show`] for what 1.x showed («Modelo:
//! 2x16», «Zoom 150 %», «Captura guardada», «Generación: ×3», ...) and
//! [`Osd::update`] once per frame; the presenter records [`Osd::pass`] after
//! the effect chain. Texts come from the session thread already translated
//! (Spanish/English by `LANG`, as 1.x).

use std::sync::Arc;
use std::time::Instant;

use lirtup_config::Config;
use lirtup_effects::Pass;
use lirtup_gpu::{Device, Texture};

use crate::Result;

/// The OSD of one session.
pub struct Osd {
    _private: (),
}

impl Osd {
    /// Create it over `screen` (the presenter's final texture), with
    /// `show_osd`, `osd_duration` and the font settings of `config`.
    pub fn new(device: &Arc<Device>, screen: &Texture, config: &Config) -> Result<Osd> {
        todo!("front S")
    }

    /// Show a message (replaces the current one and restarts its timer).
    /// Does nothing with `show_osd: false`.
    pub fn show(&mut self, text: &str) {
        todo!("front S")
    }

    /// Advance the timer (fade, expiry). `true` when the image changed this
    /// frame (the frame must be presented even if the window did not
    /// change).
    pub fn update(&mut self, now: Instant) -> Result<bool> {
        todo!("front S")
    }

    /// New `show_osd` / `osd_duration` / font settings.
    pub fn update_config(&mut self, config: &Config) {
        todo!("front S")
    }

    /// The screen texture was recreated (resize): blend onto the new one.
    pub fn rebind(&mut self, device: &Arc<Device>, screen: &Texture) -> Result<()> {
        todo!("front S")
    }

    /// The blend pass, while a message is visible.
    pub fn pass(&self) -> Option<&dyn Pass> {
        todo!("front S")
    }
}
