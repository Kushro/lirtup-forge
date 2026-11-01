//! The overlay window (3.6): ARGB visual, borderless, above everything,
//! with XFixes input regions. Front X creates the window and reads its
//! events; front K (round 2) connects the swapchain to it.

use std::ffi::c_void;
use std::ptr::NonNull;

use lirtup_config::OverlayMode;

use crate::{Rect, Result};

/// How to create the overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayOptions {
    /// Behavior.
    pub mode: OverlayMode,
    /// Area to cover (from `X11::monitor_area`, or the target's rect in
    /// windowed mode).
    pub rect: Rect,
    /// Window title (`lirtup — <target title>`).
    pub title: String,
    /// The target window: the overlay sets `WM_TRANSIENT_FOR`-like hints and
    /// never stacks below it.
    pub target: u32,
    /// Opacity 0 to 1 (`_NET_WM_WINDOW_OPACITY`).
    pub opacity: f32,
    /// Let the mouse through to what is below (`top-transparent`).
    pub click_through: bool,
    /// `_NET_WM_BYPASS_COMPOSITOR` (fullscreen).
    pub bypass_compositor: bool,
}

/// Pointer button or motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerKind {
    /// Motion.
    Motion,
    /// Button press (1 left, 2 middle, 3 right, 4/5 wheel).
    Press(u8),
    /// Button release.
    Release(u8),
}

/// An event of the overlay window, in overlay coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayEvent {
    /// Pointer event.
    Pointer {
        /// What happened.
        kind: PointerKind,
        /// X in the overlay.
        x: i32,
        /// Y in the overlay.
        y: i32,
        /// X11 modifier state.
        state: u16,
        /// Server time.
        time: u32,
    },
    /// Key press or release (keycode as is).
    Key {
        /// Pressed (else released).
        pressed: bool,
        /// Keycode.
        keycode: u8,
        /// X11 modifier state.
        state: u16,
        /// Server time.
        time: u32,
    },
    /// Pointer entered the overlay.
    Enter,
    /// Pointer left the overlay.
    Leave,
    /// Moved or resized (the swapchain follows).
    Configure(Rect),
    /// Needs a redraw.
    Expose,
    /// The window manager asked to close it (`WM_DELETE_WINDOW`).
    Close,
}

/// The overlay window. `Send`; owns an `XCBConnection`.
pub struct OverlayWindow {
    _private: (),
}

impl OverlayWindow {
    /// Create and map the window.
    pub fn create(options: &OverlayOptions) -> Result<OverlayWindow> {
        todo!("front X")
    }

    /// Window id.
    pub fn window(&self) -> u32 {
        todo!("front X")
    }

    /// The `xcb_connection_t*` for `lirtup_gpu::XcbTarget` (lives as long as
    /// this window).
    pub fn xcb_connection(&self) -> NonNull<c_void> {
        todo!("front X")
    }

    /// Current geometry.
    pub fn rect(&self) -> Rect {
        todo!("front X")
    }

    /// Change the behavior (fullscreen ↔ windowed, ...).
    pub fn set_mode(&mut self, mode: OverlayMode, rect: Rect) -> Result<()> {
        todo!("front X")
    }

    /// Opacity 0 to 1.
    pub fn set_opacity(&mut self, opacity: f32) -> Result<()> {
        todo!("front X")
    }

    /// Input region: `None` = whole window takes input, `Some(empty)` = all
    /// clicks pass through, otherwise only those rectangles take input (the
    /// in-game toolbar of round 4).
    pub fn set_input_region(&mut self, region: Option<&[Rect]>) -> Result<()> {
        todo!("front X")
    }

    /// Show or hide the pointer over the overlay (`hide_cursor`).
    pub fn set_cursor_visible(&mut self, visible: bool) -> Result<()> {
        todo!("front X")
    }

    /// Map or unmap (`pause_on_focus_loss`).
    pub fn set_visible(&mut self, visible: bool) -> Result<()> {
        todo!("front X")
    }

    /// Move the window (drag in windowed mode).
    pub fn move_to(&mut self, x: i32, y: i32) -> Result<()> {
        todo!("front X")
    }

    /// Drain pending events into `out` without blocking.
    pub fn poll_events(&mut self, out: &mut Vec<OverlayEvent>) -> Result<()> {
        todo!("front X")
    }

    /// Block until an event arrives or `timeout` passes.
    pub fn wait_event(&mut self, timeout: std::time::Duration) -> Result<Option<OverlayEvent>> {
        todo!("front X")
    }
}
