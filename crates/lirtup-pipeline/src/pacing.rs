//! Pacing of generated frames between real ones. Front F, round 2.
//!
//! Only [`crate::framegen`] uses it; front F may reshape it freely. The
//! skeleton fixes what 1.x does (`_update_interval`, `_show`,
//! `_sleep_until`): estimate the game's frame interval from new frames,
//! then spread `generated + 1` presents over it, chained to the previous
//! real present.

use std::time::{Duration, Instant};

/// Shortest game frame interval used for pacing (1/240 s in 1.x).
pub const MIN_INTERVAL: Duration = Duration::from_micros(4167);

/// Pacing state of one session.
#[derive(Debug, Clone)]
pub struct FramePacer {
    _private: (),
}

impl FramePacer {
    /// Start with the refresh interval as the first guess.
    pub fn new(refresh_hz: f32) -> FramePacer {
        todo!("front F")
    }

    /// A new game frame arrived at `now`: update the interval estimate.
    pub fn observe_frame(&mut self, now: Instant) {
        todo!("front F")
    }

    /// The estimated game frame interval.
    pub fn interval(&self) -> Duration {
        todo!("front F")
    }

    /// When to present the `generated` frames and then the real one of a
    /// cycle that is ready at `now`.
    pub fn schedule(&mut self, generated: usize, now: Instant) -> Vec<Instant> {
        todo!("front F")
    }

    /// The real frame of a cycle was presented at `at` (the next cycle
    /// chains to it).
    pub fn real_presented(&mut self, at: Instant) {
        todo!("front F")
    }
}

/// Sleep until `target`: `thread::sleep` for most of it, then spin the last
/// fraction of a millisecond, like `_sleep_until`.
pub fn sleep_until(target: Instant) {
    todo!("front F")
}
