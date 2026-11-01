//! Window list and properties (`window/info.py`, `window/acquisition.py`).

use serde::{Deserialize, Serialize};

use crate::{Result, RgbaImage, X11};

/// A rectangle in root coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// `WM_CLASS`: instance and class names.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct WindowClass {
    /// Instance name (first string).
    pub instance: String,
    /// Class name (second string).
    pub class: String,
}

/// One top-level application window.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowInfo {
    /// XID.
    pub xid: u32,
    /// Title (`_NET_WM_NAME`, then `WM_NAME`), UTF-8.
    pub title: String,
    /// Geometry of the client area, in root coordinates.
    pub rect: Rect,
    /// `_NET_WM_PID`.
    pub pid: Option<u32>,
    /// `WM_CLASS`.
    pub class: Option<WindowClass>,
}

impl WindowInfo {
    /// Width.
    pub fn width(&self) -> u32 {
        self.rect.width
    }

    /// Height.
    pub fn height(&self) -> u32 {
        self.rect.height
    }
}

impl X11 {
    /// Viewable application windows (the `is_application_window` filter of
    /// 1.x: normal type, not XWayland helpers, not our own overlay), in
    /// stacking order.
    pub fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        todo!("front X")
    }

    /// Info of one window; `None` if it is gone.
    pub fn window_info(&self, xid: u32) -> Result<Option<WindowInfo>> {
        todo!("front X")
    }

    /// The focused window (`_NET_ACTIVE_WINDOW`).
    pub fn active_window(&self) -> Result<Option<WindowInfo>> {
        todo!("front X")
    }

    /// `_NET_WM_ICON`, the entry closest to `size` pixels.
    pub fn window_icon(&self, xid: u32, size: u32) -> Result<Option<RgbaImage>> {
        todo!("front X")
    }

    /// Raise and focus a window (`_NET_ACTIVE_WINDOW` client message).
    pub fn activate_window(&self, xid: u32) -> Result<()> {
        todo!("front X")
    }

    /// Whether a window still exists and is viewable.
    pub fn is_viewable(&self, xid: u32) -> Result<bool> {
        todo!("front X")
    }
}
