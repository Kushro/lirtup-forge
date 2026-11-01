//! The overlay window thread and the swapchain on it (`swapchain.py` and the
//! Qt overlay of 1.x). Front K, round 2.
//!
//! Two halves, connected by an [`OverlaySurface`]:
//!
//! - [`OverlayThread`]: a `std::thread` that owns the
//!   [`lirtup_x11::OverlayWindow`] and the [`lirtup_x11::InputForwarder`]. It
//!   reads the window's events, forwards pointer and key events to the target
//!   window through the current [`CoordinateMapper`], and reports what the
//!   session thread needs ([`OverlayNotice`]). The session thread changes the
//!   window with [`OverlayCommand`]s (never touches it directly).
//! - [`PresentSurface`]: the `lirtup_gpu::Swapchain` on that window, plus the
//!   `OverlaySurface` that keeps the X connection alive while the swapchain
//!   lives. Shared by the session thread and the frame generation present
//!   thread as a [`SharedSurface`].
//!
//! [`PresentSurface::create`] is already written (reviewer, round 2
//! skeleton): it is the only `unsafe` of this crate.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::Receiver;
use lirtup_config::{HotkeyAction, OverlayMode};
use lirtup_gpu::{
    Device, Format, PresentMode, PresentOptions, PresentStatus, Swapchain, SwapchainDesc, Texture, XcbTarget,
};
use lirtup_x11::{CoordinateMapper, OverlayOptions, OverlaySurface, Rect};

use crate::Result;

/// What the session thread asks of the overlay window.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayCommand {
    /// Change the behavior and area (`CycleGeometry`, the windowed ↔
    /// fullscreen switch, a monitor change). Answered by
    /// `OverlayNotice::Configured` once the window manager placed it.
    SetMode {
        /// New behavior.
        mode: OverlayMode,
        /// New area, root coordinates.
        rect: Rect,
    },
    /// Move without resizing (windowed mode following the target).
    MoveTo {
        /// Left, root coordinates.
        x: i32,
        /// Top, root coordinates.
        y: i32,
    },
    /// `_NET_WM_WINDOW_OPACITY`, 0 to 1 (`overlay_opacity`, the fade of
    /// 1.x `opacity.py`).
    SetOpacity(f32),
    /// Show or hide the window (`ToggleScaling` pauses with it hidden).
    SetVisible(bool),
    /// Show or hide the cursor over the overlay (`hide_cursor`).
    SetCursorVisible(bool),
    /// Where the scaled content is now: input is mapped through it. Sent
    /// after every change of the content rect (zoom, offset, resize).
    SetMapper(CoordinateMapper),
    /// Input region: `None` = the whole window takes input, `Some(&[])` =
    /// click-through.
    SetInputRegion(Option<Vec<Rect>>),
    /// Destroy the window and end the thread.
    Close,
}

/// What the overlay thread reports to the session thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayNotice {
    /// The window has a new geometry (root coordinates): recreate the
    /// swapchain at this size.
    Configured(Rect),
    /// Needs a redraw (present even if nothing changed).
    Exposed,
    /// A key on the overlay matched a hotkey (the overlay has the keyboard
    /// focus in `fullscreen` and `windowed`, so the global grab may not see
    /// it). The session thread handles it like a global hotkey.
    Hotkey(HotkeyAction),
    /// The window manager closed the window: the session stops with
    /// `StopReason::User`.
    Closed,
    /// The thread failed (lost X connection); the session stops with
    /// `Stage::Overlay`.
    Failed,
}

/// The overlay window thread. Dropping it sends `Close` and joins.
pub struct OverlayThread {
    _private: (),
}

impl OverlayThread {
    /// Create the window (on the new thread, so its connection is never
    /// shared) and start forwarding input to `options.target`. Returns once
    /// the window exists, with its surface for [`PresentSurface::create`].
    ///
    /// `hotkeys` are the session's bindings, for [`OverlayNotice::Hotkey`].
    pub fn spawn(
        options: OverlayOptions,
        hotkeys: Vec<(HotkeyAction, lirtup_config::Hotkey)>,
    ) -> Result<OverlayThread> {
        todo!("front K")
    }

    /// The window's surface (connection and id).
    pub fn surface(&self) -> &OverlaySurface {
        todo!("front K")
    }

    /// The window's last known geometry.
    pub fn rect(&self) -> Rect {
        todo!("front K")
    }

    /// Send a command. `false` if the thread has ended.
    pub fn send(&self, command: OverlayCommand) -> bool {
        todo!("front K")
    }

    /// Notices, to poll between frames (`try_recv`), or to wait on with
    /// `select!` while the session is paused.
    pub fn notices(&self) -> &Receiver<OverlayNotice> {
        todo!("front K")
    }

    /// Close the window and wait for the thread at most `timeout`.
    pub fn close(self, timeout: Duration) {
        todo!("front K")
    }
}

/// How to create a [`PresentSurface`]: a [`SwapchainDesc`] without the
/// target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceDesc {
    /// Preferred format (`B8G8R8A8_SRGB` in 1.x, falls back).
    pub format: Format,
    /// Images (2 or 3, `swapchain_buffers`).
    pub buffers: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `vulkan_present_mode`.
    pub present_mode: PresentMode,
}

/// A swapchain on the overlay window. `Send`, not `Sync`: share it as a
/// [`SharedSurface`].
pub struct PresentSurface {
    // Field order matters: the swapchain (and its VkSurfaceKHR) is dropped
    // before the connection reference.
    swapchain: Swapchain,
    _overlay: OverlaySurface,
}

/// The swapchain shared by the session thread and the frame generation
/// present thread. Lock it only around one present or recreate.
pub type SharedSurface = Arc<Mutex<PresentSurface>>;

impl PresentSurface {
    /// Create the swapchain on `overlay`'s window.
    #[allow(unsafe_code)]
    pub fn create(
        device: &Arc<Device>,
        overlay: &OverlaySurface,
        desc: &SurfaceDesc,
    ) -> Result<PresentSurface> {
        // SAFETY: the connection pointer comes from `overlay`, which holds a
        // reference to the connection; the clone stored in the returned value
        // outlives the swapchain (field order above), as `XcbTarget::new`
        // requires.
        let target = unsafe { XcbTarget::new(overlay.xcb_connection(), overlay.window()) };
        let swapchain = device.create_swapchain(&SwapchainDesc {
            target,
            format: desc.format,
            buffers: desc.buffers,
            width: desc.width,
            height: desc.height,
            present_mode: desc.present_mode,
        })?;
        Ok(PresentSurface {
            swapchain,
            _overlay: overlay.clone(),
        })
    }

    /// Wrap it for sharing.
    pub fn shared(self) -> SharedSurface {
        Arc::new(Mutex::new(self))
    }

    /// Present `texture` (see [`Swapchain::present`]).
    pub fn present(&mut self, texture: &Texture, options: &PresentOptions<'_>) -> Result<PresentStatus> {
        Ok(self.swapchain.present(texture, options)?)
    }

    /// Recreate at a new size (after `OverlayNotice::Configured` or an
    /// `OutOfDate` present).
    pub fn recreate(&mut self, width: u32, height: u32) -> Result<()> {
        Ok(self.swapchain.recreate(width, height)?)
    }

    /// The last present said `Suboptimal`, or the size changed.
    pub fn needs_recreation(&self) -> bool {
        self.swapchain.needs_recreation()
    }

    /// Current size.
    pub fn size(&self) -> (u32, u32) {
        (self.swapchain.width(), self.swapchain.height())
    }

    /// Format in use.
    pub fn format(&self) -> Format {
        self.swapchain.format()
    }

    /// Present mode in use.
    pub fn present_mode(&self) -> PresentMode {
        self.swapchain.present_mode()
    }
}
