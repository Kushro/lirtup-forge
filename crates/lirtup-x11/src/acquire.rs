//! Finding the target window: by title, by PID of a launched program (two
//! phases), or the active one after a delay (`window/acquisition.py`).

use std::time::Duration;

use crate::{Result, WindowInfo, X11};

/// How to wait for a window of a launched program.
#[derive(Debug, Clone, PartialEq)]
pub struct PidSearch {
    /// PID of the launched program (its descendants count too).
    pub pid: u32,
    /// Substring of `WM_CLASS` for the class phase.
    pub class_hint: Option<String>,
    /// Seconds of the PID (+class) phase.
    pub pid_timeout: f64,
    /// Seconds of the class-only phase.
    pub class_timeout: f64,
    /// Maximum seconds overall.
    pub total_timeout: f64,
    /// 1 = PID first, 2 = class first.
    pub starting_phase: u8,
}

impl X11 {
    /// First window whose title contains `needle` (case-insensitive) or
    /// matches `regex`.
    pub fn find_by_title(&self, needle: Option<&str>, regex: Option<&str>) -> Result<Option<WindowInfo>> {
        todo!("front X")
    }

    /// Poll until a window of `search.pid` appears (`find_window_by_pid`).
    pub fn wait_for_pid(&self, search: &PidSearch) -> Result<WindowInfo> {
        todo!("front X")
    }

    /// Sleep `delay`, then take the active window (`target_delay`).
    pub fn active_after(&self, delay: Duration) -> Result<Option<WindowInfo>> {
        todo!("front X")
    }
}
