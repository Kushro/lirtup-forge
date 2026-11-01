//! Automatic multiplier with hysteresis (`ideal_multiplier`, `AutoMultiplier`
//! of `native_framegen.py`).

use std::time::{Duration, Instant};

/// Lowest multiplier `auto` picks.
pub const AUTO_MIN_MULTIPLIER: u8 = 2;
/// Highest multiplier `auto` picks.
pub const AUTO_MAX_MULTIPLIER: u8 = 5;
/// What `auto` starts with, and what the layer runs at when asked for it.
pub const AUTO_START_MULTIPLIER: u8 = 2;

/// Output may exceed the display refresh by this share (measurement noise).
const HEADROOM: f32 = 1.02;
/// Going up, the higher output must fit within this share of the refresh;
/// with [`HEADROOM`] this leaves a band where neither change happens.
const UP_FIT: f32 = 0.97;
/// A new choice must last this long before switching to it.
const HOLD: Duration = Duration::from_secs(2);
/// Minimum time between two switches (each one restarts lsfg).
const MIN_GAP: Duration = Duration::from_secs(5);
/// Below this frame rate (a still image) the choice is kept.
const MIN_FPS: f32 = 5.0;

/// The multiplier that brings `game_fps` closest to `refresh_hz` without
/// going over, 2 to `max`.
///
/// The largest one whose output fits the refresh (with 2 % of headroom for
/// measurement noise): at 144 Hz, ×5 for 24 fps, ×4 for 30 fps, ×2 for 60
/// fps.
pub fn ideal_multiplier(game_fps: f32, refresh_hz: f32, max: u8) -> u8 {
    let max = max.max(AUTO_MIN_MULTIPLIER);
    if !(game_fps > 0.0 && refresh_hz > 0.0) {
        return AUTO_MIN_MULTIPLIER;
    }
    let m = (refresh_hz * HEADROOM / game_fps).floor();
    (m.min(f32::from(max)) as u8).clamp(AUTO_MIN_MULTIPLIER, max)
}

/// Picks the multiplier while running; changes only after the target has been
/// stable for a while (the 1.x hysteresis).
///
/// - It goes down as soon as the output exceeds the refresh (by more than 2 %),
///   to [`ideal_multiplier`].
/// - It goes up only while the higher output fits within 97 % of the refresh.
/// - A new choice must hold for 2 s, and switches are at least 5 s apart.
#[derive(Debug, Clone)]
pub struct AutoMultiplier {
    refresh_hz: f32,
    current: Option<u8>,
    candidate: Option<u8>,
    since: Option<Instant>,
    last_switch: Option<Instant>,
}

impl AutoMultiplier {
    /// For a display refresh, starting at `current` (default ×2).
    ///
    /// Without `current`, the first [`AutoMultiplier::update`] takes its
    /// choice right away (there is nothing to restart yet).
    pub fn new(refresh_hz: f32, current: Option<u8>) -> AutoMultiplier {
        AutoMultiplier {
            refresh_hz,
            current: current.map(|c| c.clamp(AUTO_MIN_MULTIPLIER, AUTO_MAX_MULTIPLIER)),
            candidate: None,
            since: None,
            last_switch: None,
        }
    }

    /// The display refresh changed.
    pub fn set_refresh(&mut self, refresh_hz: f32) {
        if refresh_hz != self.refresh_hz {
            self.refresh_hz = refresh_hz;
            self.candidate = None;
        }
    }

    /// Target for this frame rate, without hysteresis.
    pub fn target(&self, game_fps: f32) -> u8 {
        let ideal = ideal_multiplier(game_fps, self.refresh_hz, AUTO_MAX_MULTIPLIER);
        let Some(c) = self.current else {
            return ideal;
        };
        if game_fps * f32::from(c) > self.refresh_hz * HEADROOM {
            return c.min(ideal);
        }
        let mut up = c;
        while up < AUTO_MAX_MULTIPLIER && game_fps * f32::from(up + 1) <= self.refresh_hz * UP_FIT {
            up += 1;
        }
        up
    }

    /// Feed the measured game frame rate; returns the multiplier to use now.
    pub fn update(&mut self, game_fps: f32, now: Instant) -> u8 {
        let Some(current) = self.current else {
            // First choice: take it right away. A still image (a blinking
            // cursor) says nothing about the game's rate: start low and let
            // the rules raise it once it moves.
            let first = if game_fps < MIN_FPS {
                AUTO_MIN_MULTIPLIER
            } else {
                ideal_multiplier(game_fps, self.refresh_hz, AUTO_MAX_MULTIPLIER)
            };
            self.current = Some(first);
            self.last_switch = Some(now);
            return first;
        };
        if !(game_fps >= MIN_FPS) {
            self.candidate = None;
            return current;
        }
        let target = self.target(game_fps);
        if target == current {
            self.candidate = None;
        } else if self.candidate != Some(target) {
            self.candidate = Some(target);
            self.since = Some(now);
        } else {
            let held = self
                .since
                .is_some_and(|s| now.saturating_duration_since(s) >= HOLD);
            let spaced = self
                .last_switch
                .is_none_or(|t| now.saturating_duration_since(t) >= MIN_GAP);
            if held && spaced {
                tracing::info!(
                    "automatic frame generation: x{current} -> x{target} (game {game_fps:.1} fps, {:.0} Hz)",
                    self.refresh_hz
                );
                self.current = Some(target);
                self.candidate = None;
                self.last_switch = Some(now);
            }
        }
        self.current.unwrap_or(current)
    }

    /// Current multiplier.
    pub fn current(&self) -> u8 {
        self.current.unwrap_or(AUTO_START_MULTIPLIER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ideal_at_144_hz() {
        assert_eq!(ideal_multiplier(24.0, 144.0, 5), 5);
        assert_eq!(ideal_multiplier(30.0, 144.0, 5), 4);
        assert_eq!(ideal_multiplier(48.0, 144.0, 5), 3);
        assert_eq!(ideal_multiplier(60.0, 144.0, 5), 2);
        assert_eq!(ideal_multiplier(120.0, 144.0, 5), 2);
        assert_eq!(ideal_multiplier(10.0, 144.0, 4), 4);
        assert_eq!(ideal_multiplier(0.0, 144.0, 5), 2);
        assert_eq!(ideal_multiplier(f32::NAN, 60.0, 5), 2);
        // 72 fps × 2 = 144 fits only thanks to the headroom.
        assert_eq!(ideal_multiplier(72.5, 144.0, 5), 2);
    }

    #[test]
    fn first_update_is_immediate() {
        let t = Instant::now();
        let mut auto = AutoMultiplier::new(144.0, None);
        assert_eq!(auto.current(), AUTO_START_MULTIPLIER);
        assert_eq!(auto.update(30.0, t), 4);
        let mut still = AutoMultiplier::new(144.0, None);
        assert_eq!(still.update(1.0, t), AUTO_MIN_MULTIPLIER);
    }

    #[test]
    fn hysteresis_band() {
        let auto = AutoMultiplier::new(144.0, Some(2));
        // 2 × 70 = 140 fits, 3 × 70 = 210 does not: stays.
        assert_eq!(auto.target(70.0), 2);
        // 3 × 46 = 138 <= 0.97 × 144 = 139.7: goes up.
        assert_eq!(auto.target(46.0), 3);
        // 3 × 47 = 141 is inside the band: stays at 2.
        assert_eq!(auto.target(47.0), 2);
        let high = AutoMultiplier::new(144.0, Some(4));
        // 4 × 40 = 160 > 146.9: down to the ideal.
        assert_eq!(high.target(40.0), 3);
    }

    #[test]
    fn switches_after_hold_and_gap() {
        let t0 = Instant::now();
        let s = Duration::from_secs;
        let mut auto = AutoMultiplier::new(144.0, None);
        assert_eq!(auto.update(60.0, t0), 2);
        // The game slows down to 30 fps: x4 is the target.
        assert_eq!(auto.update(30.0, t0 + s(1)), 2);
        assert_eq!(auto.update(30.0, t0 + s(2)), 2);
        // Held for 2 s but only 3 s since the first choice: waits for the gap.
        assert_eq!(auto.update(30.0, t0 + s(3)), 2);
        assert_eq!(auto.update(30.0, t0 + s(5)), 4);
        // A still image keeps the choice.
        assert_eq!(auto.update(0.5, t0 + s(20)), 4);
        // A blip resets the candidate.
        assert_eq!(auto.update(60.0, t0 + s(21)), 4);
        assert_eq!(auto.update(30.0, t0 + s(22)), 4);
        assert_eq!(auto.update(60.0, t0 + s(23)), 4);
        assert_eq!(auto.update(60.0, t0 + s(24)), 4);
        assert_eq!(auto.update(60.0, t0 + s(25)), 2);
    }
}
