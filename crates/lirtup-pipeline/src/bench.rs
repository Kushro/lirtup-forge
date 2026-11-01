//! «Medir en mi GPU» (`bench.py`). Front S, round 2.
//!
//! **It runs in a child process**, as in 1.x: `lirtup-core` answers
//! `Command::Bench` by starting `lirtup bench --json <request>` (the same
//! binary; the subcommand is front K's) and relaying what it prints. Each
//! line the child prints is an `Envelope<Event>`
//! (`lirtup_proto::encode_line`): `Event::BenchProgress` per case and one
//! `Event::BenchResult` at the end. `Command::CancelBench` is SIGTERM to the
//! child, which then prints a `BenchResult` with `cancelled: true`.
//!
//! Why a process: lsfg keeps global state and allows one engine per process
//! (`lirtup_fg::native::LsfgEngine`), so the frame generation cases cannot
//! run next to a session; and a driver crash while measuring does not take
//! the scaler down.
//!
//! The cases run headless on a synthetic test image, straight on
//! `lirtup-effects`, `lirtup-magpiefx` and `lirtup-fg` (not through the
//! presenter): native effects and MagpieFX with Vulkan timestamps, frame
//! generation timed on the CPU per cycle, like 1.x. Results are kept per GPU
//! in `bench.json` next to the configuration (same file and shape as 1.x).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use lirtup_gpu::Device;
use lirtup_proto::{BenchEntry, BenchKind, BenchReport, BenchRequest};

use crate::Result;

/// One measurement to make.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchCase {
    /// Native, MagpieFX or frame generation.
    pub kind: BenchKind,
    /// Effect name, or the frame generation key (`x2-flow1.0-quality`, as
    /// `framegen_key` of 1.x).
    pub name: String,
}

/// Limits of a run (`BenchSettings` of 1.x).
#[derive(Debug, Clone, PartialEq)]
pub struct BenchSettings {
    /// Warm-up frames per case, not measured.
    pub warmup: u32,
    /// Measured frames per case, at least.
    pub min_frames: u32,
    /// Measured frames per case, at most.
    pub max_frames: u32,
    /// Stop measuring a case after this (once `min_frames` are done).
    pub budget: Duration,
}

impl Default for BenchSettings {
    fn default() -> BenchSettings {
        BenchSettings {
            warmup: 5,
            min_frames: 10,
            max_frames: 100,
            budget: Duration::from_millis(1500),
        }
    }
}

/// Progress of a run.
#[derive(Debug, Clone, PartialEq)]
pub enum BenchProgress {
    /// A case is about to run (`index` from 0).
    Started {
        /// Index of the case.
        index: u32,
        /// Number of cases.
        total: u32,
        /// The case.
        case: BenchCase,
    },
    /// A case finished (measured or failed).
    Done {
        /// Cases done.
        done: u32,
        /// Number of cases.
        total: u32,
        /// Its result.
        entry: BenchEntry,
    },
}

/// The cases of a request, in the order they are measured (`build_cases`).
pub fn build_cases(request: &BenchRequest) -> Vec<BenchCase> {
    todo!("front S")
}

/// The synthetic test frame (`make_test_image`): RGBA8, gradients, fine
/// lines, flat areas and noise; `shift` moves the pattern to make a second,
/// different frame.
pub fn test_image(width: u32, height: u32, shift: u32) -> Vec<u8> {
    todo!("front S")
}

/// Run every case of `request` on `device`. Checks `cancel` between frames
/// and returns a report with `cancelled: true` when set. `progress` is
/// called from this thread.
pub fn run(
    device: &Arc<Device>,
    request: &BenchRequest,
    settings: &BenchSettings,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(BenchProgress),
) -> Result<BenchReport> {
    todo!("front S")
}

/// `bench.json` next to the configuration.
pub fn results_path() -> PathBuf {
    todo!("front S")
}

/// Merge a report into `bench.json` under its GPU (atomic write).
pub fn save_results(path: &Path, report: &BenchReport) -> Result<()> {
    todo!("front S")
}

/// The stored entries of a GPU (`results_for_gpu`), for the costs the GUI
/// shows before measuring again.
pub fn results_for_gpu(path: &Path, gpu: &str) -> Result<Vec<BenchEntry>> {
    todo!("front S")
}
