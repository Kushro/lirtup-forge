//! Wine menu bar detection (`menubar.py`). Front S, round 2.
//!
//! With `auto_crop_menu_bar`, the session thread hands captured frames to a
//! [`MenuBarDetector`] until it decides. When it finds a bar, the session
//! thread recreates the capture with the crop and sends
//! `Event::MenuBarDetected` (the GUI saves it in the profile: the scaler
//! never writes YAML).

use std::time::Instant;

use lirtup_config::Crop;

/// The crop of a detected menu bar (all zeros: none).
pub fn detect_menu_bar(pixels: &[u8], width: u32, height: u32, stride: u32) -> Crop {
    todo!("front S")
}

/// What the detector decided so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuBarCheck {
    /// Not this frame (between samples) or not sure yet.
    Pending,
    /// Found, the same in enough samples: crop this (added to the
    /// configured crop).
    Found(Crop),
    /// No menu bar; stop checking for this session.
    NotFound,
}

/// Samples frames over the first seconds of a session, like
/// `_check_menu_bar` of 1.x (interval, agreement of samples, give-up).
#[derive(Debug, Clone)]
pub struct MenuBarDetector {
    _private: (),
}

impl MenuBarDetector {
    /// Start detecting (call only with `auto_crop_menu_bar`).
    pub fn new(now: Instant) -> MenuBarDetector {
        todo!("front S")
    }

    /// Look at one captured frame (BGRA, `stride` bytes per row).
    pub fn check(
        &mut self,
        now: Instant,
        pixels: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) -> MenuBarCheck {
        todo!("front S")
    }
}
