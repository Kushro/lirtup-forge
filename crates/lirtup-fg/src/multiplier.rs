//! Automatic multiplier with hysteresis (`ideal_multiplier`, `AutoMultiplier`
//! of `native_framegen.py`).

use std::time::Instant;

/// The multiplier that brings `game_fps` closest to `refresh_hz` without
/// going over, 2 to `max`.
pub fn ideal_multiplier(game_fps: f32, refresh_hz: f32, max: u8) -> u8 {
    todo!("front C")
}

/// Picks the multiplier while running; changes only after the target has been
/// stable for a while (the 1.x hysteresis).
#[derive(Debug, Clone)]
pub struct AutoMultiplier {
    _private: (),
}

impl AutoMultiplier {
    /// For a display refresh, starting at `current` (default ×2).
    pub fn new(refresh_hz: f32, current: Option<u8>) -> AutoMultiplier {
        todo!("front C")
    }

    /// The display refresh changed.
    pub fn set_refresh(&mut self, refresh_hz: f32) {
        todo!("front C")
    }

    /// Target for this frame rate, without hysteresis.
    pub fn target(&self, game_fps: f32) -> u8 {
        todo!("front C")
    }

    /// Feed the measured game frame rate; returns the multiplier to use now.
    pub fn update(&mut self, game_fps: f32, now: Instant) -> u8 {
        todo!("front C")
    }

    /// Current multiplier.
    pub fn current(&self) -> u8 {
        todo!("front C")
    }
}
