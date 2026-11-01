//! Monitors (RandR), for the `monitor:` option and `ListDevices`.

use serde::{Deserialize, Serialize};

use crate::{Rect, Result, X11};

/// One monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// Index in the list (what `monitor: 1` means).
    pub index: u32,
    /// RandR output name (`DP-1`, ...).
    pub name: String,
    /// Geometry in root coordinates.
    pub rect: Rect,
    /// Primary output.
    pub primary: bool,
    /// Refresh rate of the current mode, if known (auto multiplier).
    pub refresh_hz: Option<f32>,
}

impl X11 {
    /// Active monitors, primary first.
    pub fn monitors(&self) -> Result<Vec<MonitorInfo>> {
        todo!("front X")
    }

    /// Resolve `monitor:` (`primary`, `all`, a name or an index) to the area
    /// to cover; `all` is the bounding box of every monitor.
    pub fn monitor_area(&self, selector: &lirtup_config::MonitorSelector) -> Result<(Rect, Option<f32>)> {
        todo!("front X")
    }
}
