//! Input forwarding from the overlay to the target window
//! (`overlay/forwarder.py`, `X11EventForwarder` and `CoordinateMapper`).

use crate::overlay::OverlayEvent;
use crate::{Rect, Result};

/// Maps overlay coordinates to target window coordinates: the inverse of the
/// scaling (content rect inside the overlay → cropped source rect).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CoordinateMapper {
    /// Where the scaled content is drawn inside the overlay.
    pub content: Rect,
    /// Size of the captured (cropped) source.
    pub source_size: (u32, u32),
    /// Crop offset inside the target window.
    pub crop_offset: (u32, u32),
}

impl CoordinateMapper {
    /// Target window coordinates of an overlay point; `None` outside the
    /// content (letterbox).
    pub fn to_target(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        todo!("front X")
    }
}

/// Replays overlay events on the target with `SendEvent` (and XTest where
/// `SendEvent` is ignored). `Send`; its own connection.
pub struct InputForwarder {
    _private: (),
}

impl InputForwarder {
    /// Forward to `target`.
    pub fn new(target: u32) -> Result<InputForwarder> {
        todo!("front X")
    }

    /// Update the mapping (geometry, zoom or offset changed).
    pub fn set_mapper(&mut self, mapper: CoordinateMapper) {
        todo!("front X")
    }

    /// Forward one event; events that do not map are dropped.
    pub fn forward(&mut self, event: &OverlayEvent) -> Result<()> {
        todo!("front X")
    }
}
