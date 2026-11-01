//! Pacing of generated frames between real ones (`_update_interval`,
//! `_show` and `_sleep_until` of 1.x `native_framegen.py`). Front F.
//!
//! Only [`crate::framegen`] uses it. Two halves, one per thread:
//!
//! - [`FrameInterval`], on the session thread: the game's frame interval,
//!   estimated from the new frames it presents (with frame generation on,
//!   only new frames reach [`crate::framegen::FrameGen::present`]).
//! - [`FramePacer`], on the `lsfg-present` thread: spreads the
//!   `generated + 1` presents of a cycle evenly over that interval, chained
//!   to the previous real present.
//!
//! The constants and the rules are the ones 1.x arrived at with video (see
//! `docs/developer/frame-generation-analysis.md`, «Fase 1: resultados»).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Shortest game frame interval used for pacing (1/240 s).
pub const MIN_INTERVAL: Duration = Duration::from_micros(4167);
/// Longest game frame interval used for pacing (1/15 s).
pub const MAX_INTERVAL: Duration = Duration::from_micros(66_667);
/// The guess before two frames have been seen (1/60 s, as 1.x).
pub const INITIAL_INTERVAL: Duration = Duration::from_micros(16_667);
/// Intervals the estimate spans.
const HISTORY: usize = 30;
/// An interval of at least this many medians lost frames, and counts as as
/// many frames as medians fit in it. (At 60 fps and 144 Hz the intervals
/// alternate 13.8 and 20.7 ms: a lower ratio counts the long ones as two.)
const LOST_FRAME_RATIO: f64 = 1.75;
/// A chain that restarts (its frame came late) starts this share of a step
/// later than needed, so the jitter of later frames (polling rounds them to
/// the display refresh) does not restart it again.
const CHAIN_MARGIN: f64 = 0.5;
/// [`sleep_until`] spins this last part instead of sleeping.
const SPIN: Duration = Duration::from_micros(200);

/// The game's frame interval, from the times new frames arrive.
///
/// The estimate is the time spanned by the last [`HISTORY`] intervals over
/// the game frames they hold. Polling rounds each interval to the display
/// refresh, so a median or an exponential average would be biased; the
/// long average is not. Intervals that lost frames count as several.
#[derive(Debug, Clone)]
pub struct FrameInterval {
    last: Option<Instant>,
    history: VecDeque<Duration>,
    interval: Duration,
}

impl Default for FrameInterval {
    fn default() -> Self {
        FrameInterval::new()
    }
}

impl FrameInterval {
    /// Start at [`INITIAL_INTERVAL`].
    pub fn new() -> FrameInterval {
        FrameInterval {
            last: None,
            history: VecDeque::with_capacity(HISTORY),
            interval: INITIAL_INTERVAL,
        }
    }

    /// A new game frame arrived at `now`: update the estimate.
    pub fn observe_frame(&mut self, now: Instant) {
        let Some(last) = self.last.replace(now) else {
            return;
        };
        let delta = now
            .saturating_duration_since(last)
            .clamp(MIN_INTERVAL, MAX_INTERVAL);
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(delta);
        self.interval = estimate(&self.history);
    }

    /// The estimated game frame interval.
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// The estimated game frame rate.
    pub fn fps(&self) -> f32 {
        (1.0 / self.interval.as_secs_f64()) as f32
    }
}

/// Total time over game frames (see [`FrameInterval`]).
fn estimate(history: &VecDeque<Duration>) -> Duration {
    let secs: Vec<f64> = history.iter().map(Duration::as_secs_f64).collect();
    let median = median(&secs);
    let frames: f64 = secs
        .iter()
        .map(|&v| {
            if v >= median * LOST_FRAME_RATIO {
                (v / median).round().max(2.0)
            } else {
                1.0
            }
        })
        .sum();
    Duration::from_secs_f64(secs.iter().sum::<f64>() / frames)
}

/// The median as Python's `statistics.median` (mean of the two middle ones).
fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// The chain of real presents on the `lsfg-present` thread.
///
/// Each cycle is chained to the previous real present, one step after it,
/// so the gaps do not follow the jitter of when a new frame is noticed
/// (polling) and generated. When the frame came too late for that, a new
/// chain starts with some margin. (It never comes too early: cycles are
/// shown one at a time, so the previous real frame is already on screen.)
#[derive(Debug, Clone, Default)]
pub struct FramePacer {
    last_real: Option<Instant>,
}

impl FramePacer {
    /// No chain yet.
    pub fn new() -> FramePacer {
        FramePacer::default()
    }

    /// Break the chain (the session thread presented a frame by itself, or
    /// lsfg restarted): the next cycle starts as soon as it is ready.
    pub fn reset(&mut self) {
        self.last_real = None;
    }

    /// When to present the `generated` frames and then the real one of a
    /// cycle whose first generated frame is ready at `ready`, for a game
    /// frame `interval`. Returns `generated + 1` instants; the last one (the
    /// real frame) is what the next cycle chains to.
    pub fn schedule(&mut self, generated: usize, interval: Duration, ready: Instant) -> Vec<Instant> {
        let step = interval / (generated as u32 + 1);
        let base = match self.last_real {
            Some(last) if last + step >= ready => last + step,
            Some(_) => ready + step.mul_f64(CHAIN_MARGIN),
            None => ready,
        };
        let times: Vec<Instant> = (0..=generated as u32).map(|i| base + step * i).collect();
        self.last_real = times.last().copied();
        times
    }

    /// A cycle without generated frames (the first of an lsfg context) put
    /// its real frame on screen at `at`.
    pub fn real_presented(&mut self, at: Instant) {
        self.last_real = Some(at);
    }
}

/// Sleep until `target`: `thread::sleep` for most of it, then spin the last
/// fraction of a millisecond (the scheduler may wake a sleep late).
pub fn sleep_until(target: Instant) {
    let now = Instant::now();
    if target <= now {
        return;
    }
    let left = target - now;
    if left > SPIN {
        std::thread::sleep(left - SPIN);
    }
    while Instant::now() < target {
        std::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: f64) -> Duration {
        Duration::from_micros((v * 1000.0).round() as u64)
    }

    fn feed(intervals: &[f64]) -> FrameInterval {
        let mut fi = FrameInterval::new();
        let mut t = Instant::now();
        fi.observe_frame(t);
        for &v in intervals {
            t += ms(v);
            fi.observe_frame(t);
        }
        fi
    }

    fn close(a: Duration, b: Duration) -> bool {
        a.abs_diff(b) < Duration::from_micros(20)
    }

    #[test]
    fn starts_at_the_initial_guess() {
        let mut fi = FrameInterval::new();
        assert_eq!(fi.interval(), INITIAL_INTERVAL);
        fi.observe_frame(Instant::now());
        assert_eq!(fi.interval(), INITIAL_INTERVAL);
    }

    #[test]
    fn steady_rate() {
        let fi = feed(&[33.3; 10]);
        assert!(close(fi.interval(), ms(33.3)), "{:?}", fi.interval());
    }

    #[test]
    fn polling_jitter_averages_out() {
        // 60 fps polled at 144 Hz: 13.8 and 20.7 ms alternate (not lost
        // frames, below 1.75 medians).
        let fi = feed(&[13.8, 20.7, 13.8, 20.7, 13.8, 20.7]);
        assert!(close(fi.interval(), ms(17.25)), "{:?}", fi.interval());
    }

    #[test]
    fn lost_frames_count_as_several() {
        // One interval of 66.6 ms at 30 fps lost a frame: 4 intervals hold
        // 5 frames, (33.3 × 3 + 66.6) / 5 = 33.3.
        let fi = feed(&[33.3, 33.3, 66.6, 33.3]);
        assert!(close(fi.interval(), ms(33.3)), "{:?}", fi.interval());
    }

    #[test]
    fn intervals_are_clamped() {
        let fi = feed(&[1.0, 1.0]);
        assert!(close(fi.interval(), MIN_INTERVAL), "{:?}", fi.interval());
        let slow = feed(&[500.0]);
        assert!(close(slow.interval(), MAX_INTERVAL), "{:?}", slow.interval());
    }

    #[test]
    fn history_is_bounded() {
        let mut values = vec![50.0; 40];
        values.extend([20.0; HISTORY]);
        let fi = feed(&values);
        assert!(close(fi.interval(), ms(20.0)), "{:?}", fi.interval());
    }

    #[test]
    fn median_of_even_and_odd() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
    }

    #[test]
    fn first_cycle_starts_when_ready() {
        let t = Instant::now();
        let mut pacer = FramePacer::new();
        let times = pacer.schedule(3, ms(32.0), t);
        assert_eq!(times, vec![t, t + ms(8.0), t + ms(16.0), t + ms(24.0)]);
    }

    #[test]
    fn cycles_chain_to_the_previous_real_frame() {
        let t = Instant::now();
        let mut pacer = FramePacer::new();
        let first = pacer.schedule(1, ms(20.0), t);
        assert_eq!(first[1], t + ms(10.0));
        // The next frame is ready a bit early: it waits for its slot.
        let second = pacer.schedule(1, ms(20.0), t + ms(12.0));
        assert_eq!(second, vec![t + ms(20.0), t + ms(30.0)]);
    }

    #[test]
    fn a_late_frame_restarts_the_chain_with_a_margin() {
        let t = Instant::now();
        let mut pacer = FramePacer::new();
        pacer.schedule(1, ms(20.0), t);
        let late = t + ms(50.0);
        let times = pacer.schedule(1, ms(20.0), late);
        assert_eq!(times[0], late + ms(5.0));
        assert_eq!(times[1], late + ms(15.0));
    }

    #[test]
    fn reset_and_real_presented() {
        let t = Instant::now();
        let mut pacer = FramePacer::new();
        pacer.real_presented(t);
        assert_eq!(pacer.schedule(1, ms(20.0), t + ms(5.0))[0], t + ms(10.0));
        pacer.reset();
        assert_eq!(pacer.schedule(1, ms(20.0), t + ms(100.0))[0], t + ms(100.0));
    }

    #[test]
    fn sleep_until_reaches_the_target() {
        let target = Instant::now() + Duration::from_millis(3);
        sleep_until(target);
        assert!(Instant::now() >= target);
        // The past returns at once.
        sleep_until(Instant::now() - Duration::from_millis(1));
    }
}
