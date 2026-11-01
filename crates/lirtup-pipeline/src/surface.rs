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
//!
//! # The overlay thread
//!
//! It waits on the window's X events in short slices
//! ([`EVENT_SLICE`]) and applies the [`OverlayCommand`]s between them, so a
//! command waits at most one slice and the pointer never waits for a frame.
//! Key presses that match a session hotkey become [`OverlayNotice::Hotkey`]
//! and are not forwarded (nor their release). Everything else that maps to
//! the content goes to the target with `SendEvent`.
//!
//! The X connection is the window's own, shared with the swapchain (the
//! Vulkan WSI uses it from the presenting threads; libxcb is thread-safe).

use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use lirtup_config::{HotkeyAction, OverlayMode};
use lirtup_gpu::{
    Device, Format, PresentMode, PresentOptions, PresentStatus, Swapchain, SwapchainDesc, Texture, XcbTarget,
};
use lirtup_proto::Stage;
use lirtup_x11::input::HotkeyMatcher;
use lirtup_x11::{
    CoordinateMapper, InputForwarder, OverlayEvent, OverlayOptions, OverlaySurface, OverlayWindow, Rect,
    X11Error,
};

use crate::{PipelineError, Result};

/// The longest the overlay thread waits for X events before it looks at its
/// commands again.
pub const EVENT_SLICE: Duration = Duration::from_millis(4);

/// After `SetMode`, how long to wait for the window manager's
/// `ConfigureNotify` before reporting the geometry as it is.
const CONFIGURE_GRACE: Duration = Duration::from_millis(250);

/// How long dropping an [`OverlayThread`] waits for it.
const DROP_TIMEOUT: Duration = Duration::from_secs(1);

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
    surface: OverlaySurface,
    rect: Arc<Mutex<Rect>>,
    commands: Sender<OverlayCommand>,
    notices: Receiver<OverlayNotice>,
    thread: Option<JoinHandle<()>>,
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
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let (notice_tx, notice_rx) = crossbeam_channel::unbounded();
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        let rect = Arc::new(Mutex::new(options.rect));
        let thread_rect = Arc::clone(&rect);
        let thread = std::thread::Builder::new()
            .name("lirtup-overlay".into())
            .spawn(move || {
                let mut overlay = match Overlay::new(&options, &hotkeys, thread_rect) {
                    Ok(overlay) => overlay,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok((overlay.window.surface(), overlay.window.rect())));
                overlay.run(&command_rx, &notice_tx);
            })
            .map_err(|e| {
                PipelineError::other(Stage::Overlay, format!("cannot start the overlay thread: {e}"))
            })?;
        let (surface, initial) = match ready_rx.recv() {
            Ok(Ok(ready)) => ready,
            Ok(Err(e)) => {
                let _ = thread.join();
                return Err(e);
            }
            Err(_) => {
                let _ = thread.join();
                return Err(PipelineError::other(Stage::Overlay, "the overlay thread ended"));
            }
        };
        *rect.lock().unwrap_or_else(PoisonError::into_inner) = initial;
        Ok(OverlayThread {
            surface,
            rect,
            commands: command_tx,
            notices: notice_rx,
            thread: Some(thread),
        })
    }

    /// The window's surface (connection and id).
    pub fn surface(&self) -> &OverlaySurface {
        &self.surface
    }

    /// The window's last known geometry.
    pub fn rect(&self) -> Rect {
        *self.rect.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Send a command. `false` if the thread has ended.
    pub fn send(&self, command: OverlayCommand) -> bool {
        self.commands.send(command).is_ok()
    }

    /// Notices, to poll between frames (`try_recv`), or to wait on with
    /// `select!` while the session is paused.
    pub fn notices(&self) -> &Receiver<OverlayNotice> {
        &self.notices
    }

    /// Close the window and wait for the thread at most `timeout`.
    pub fn close(mut self, timeout: Duration) {
        self.join(timeout);
    }

    fn join(&mut self, timeout: Duration) {
        let Some(thread) = self.thread.take() else { return };
        let _ = self.commands.send(OverlayCommand::Close);
        let deadline = Instant::now() + timeout;
        while !thread.is_finished() {
            if Instant::now() >= deadline {
                tracing::warn!("the overlay thread did not stop in {timeout:?}; leaving it");
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let _ = thread.join();
    }
}

impl Drop for OverlayThread {
    fn drop(&mut self) {
        self.join(DROP_TIMEOUT);
    }
}

/// What the overlay thread owns.
struct Overlay {
    window: OverlayWindow,
    forwarder: InputForwarder,
    hotkeys: HotkeyMatcher,
    rect: Arc<Mutex<Rect>>,
    /// Keycode of a hotkey press, whose release is not forwarded either.
    swallowed: Option<u8>,
    /// A `SetMode` waits for its `Configure` until then.
    configure_due: Option<Instant>,
    /// The target is gone (logged once).
    target_gone: bool,
}

/// An X11 error of the overlay, as the session reports it.
fn overlay_error(e: X11Error) -> PipelineError {
    PipelineError::other(Stage::Overlay, e.to_string())
}

impl Overlay {
    fn new(
        options: &OverlayOptions,
        hotkeys: &[(HotkeyAction, lirtup_config::Hotkey)],
        rect: Arc<Mutex<Rect>>,
    ) -> Result<Overlay> {
        let window = OverlayWindow::create(options).map_err(overlay_error)?;
        let forwarder = InputForwarder::new(options.target).map_err(overlay_error)?;
        let hotkeys = forwarder.hotkey_matcher(hotkeys).unwrap_or_else(|e| {
            tracing::warn!("overlay: no hotkeys over the overlay: {e}");
            HotkeyMatcher::default()
        });
        Ok(Overlay {
            window,
            forwarder,
            hotkeys,
            rect,
            swallowed: None,
            configure_due: None,
            target_gone: false,
        })
    }

    fn run(&mut self, commands: &Receiver<OverlayCommand>, notices: &Sender<OverlayNotice>) {
        loop {
            loop {
                match commands.try_recv() {
                    Ok(OverlayCommand::Close) | Err(TryRecvError::Disconnected) => return,
                    Ok(command) => self.apply(command),
                    Err(TryRecvError::Empty) => break,
                }
            }
            if let Some(due) = self.configure_due
                && Instant::now() >= due
            {
                // The window manager left it where it was: no event.
                self.configure_due = None;
                let rect = self.window.rect();
                self.set_rect(rect);
                let _ = notices.send(OverlayNotice::Configured(rect));
            }
            match self.window.wait_event(EVENT_SLICE) {
                Ok(Some(event)) => {
                    if let Some(notice) = self.event(event)
                        && notices.send(notice).is_err()
                    {
                        return;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::error!("overlay: {e}");
                    let _ = notices.send(OverlayNotice::Failed);
                    return;
                }
            }
        }
    }

    fn set_rect(&self, rect: Rect) {
        *self.rect.lock().unwrap_or_else(PoisonError::into_inner) = rect;
    }

    fn apply(&mut self, command: OverlayCommand) {
        let result = match command {
            OverlayCommand::SetMode { mode, rect } => {
                self.configure_due = Some(Instant::now() + CONFIGURE_GRACE);
                self.window.set_mode(mode, rect)
            }
            OverlayCommand::MoveTo { x, y } => {
                let moved = self.window.move_to(x, y);
                self.set_rect(self.window.rect());
                moved
            }
            OverlayCommand::SetOpacity(opacity) => self.window.set_opacity(opacity),
            OverlayCommand::SetVisible(visible) => self.window.set_visible(visible),
            OverlayCommand::SetCursorVisible(visible) => self.window.set_cursor_visible(visible),
            OverlayCommand::SetMapper(mapper) => {
                self.forwarder.set_mapper(mapper);
                self.target_gone = false;
                Ok(())
            }
            OverlayCommand::SetInputRegion(region) => self.window.set_input_region(region.as_deref()),
            OverlayCommand::Close => Ok(()),
        };
        if let Err(e) = result {
            tracing::warn!("overlay: {e}");
        }
    }

    /// One event: forward it, or turn it into a notice.
    fn event(&mut self, event: OverlayEvent) -> Option<OverlayNotice> {
        match event {
            OverlayEvent::Configure(rect) => {
                self.configure_due = None;
                self.set_rect(rect);
                return Some(OverlayNotice::Configured(rect));
            }
            OverlayEvent::Expose => return Some(OverlayNotice::Exposed),
            OverlayEvent::Close => return Some(OverlayNotice::Closed),
            OverlayEvent::Key {
                pressed: true,
                keycode,
                state,
                ..
            } => {
                if let Some(action) = self.hotkeys.matches(keycode, state) {
                    self.swallowed = Some(keycode);
                    return Some(OverlayNotice::Hotkey(action));
                }
            }
            OverlayEvent::Key {
                pressed: false,
                keycode,
                ..
            } if self.swallowed == Some(keycode) => {
                self.swallowed = None;
                return None;
            }
            _ => {}
        }
        match self.forwarder.forward(&event) {
            Ok(()) => {}
            Err(X11Error::NoWindow(xid)) => {
                if !self.target_gone {
                    self.target_gone = true;
                    tracing::debug!("overlay: the target 0x{xid:08x} is gone; not forwarding");
                }
            }
            Err(e) => tracing::debug!("overlay: forwarding: {e}"),
        }
        None
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
