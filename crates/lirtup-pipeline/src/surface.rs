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
//!
//! # The bar over the game (round 4, front T)
//!
//! Pointer events go through a [`ToolbarRouter`] with the zone of
//! [`OverlayCommand::SetToolbarZone`]: what is the bar's comes back as
//! [`OverlayNotice::Pointer`] instead of going to the game, and the cursor
//! shows over the bar even when `hide_cursor` hid it. Also here, from the
//! 1.x overlay:
//!
//! - in click-through `top-transparent`, where the window gets no pointer
//!   events, the input region is the bar while it shows, and the pointer is
//!   polled ([`POINTER_POLL`]) for the reveal band and for the opacity:
//!   [`OverlayCommand::SetDimmedOpacity`] outside the target, the full one
//!   inside (`opacity.py`);
//! - the last motion is sent again every [`HOVER_KEEPALIVE`] while the
//!   pointer rests on the content, so games keep their hover
//!   (`HOVER_KEEPALIVE_INTERVAL_MS`).

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
use lirtup_x11::overlay::PointerKind;
use lirtup_x11::overlay_input::{Routed, ToolbarRouter, ToolbarZone};
use lirtup_x11::{
    CoordinateMapper, InputForwarder, OverlayEvent, OverlayOptions, OverlaySurface, OverlayWindow, Rect, X11,
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

/// How often the pointer is polled in click-through mode (1.x
/// `opacity.py` checks at 10 Hz; the reveal band wants a little more).
pub const POINTER_POLL: Duration = Duration::from_millis(50);

/// The last motion over the content is sent again this often while the
/// pointer rests (1.x `HOVER_KEEPALIVE_INTERVAL_MS`).
pub const HOVER_KEEPALIVE: Duration = Duration::from_millis(100);

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
    /// Click-through mode: the opacity while the pointer is outside the
    /// target (`overlay_opacity_min`; `None` = always the full one). Inside
    /// it is the one of [`OverlayCommand::SetOpacity`].
    SetDimmedOpacity(Option<f32>),
    /// Show or hide the window (`ToggleScaling` pauses with it hidden).
    SetVisible(bool),
    /// Show or hide the cursor over the overlay. Hidden wins over
    /// [`OverlayCommand::HideCursorAfter`].
    SetCursorVisible(bool),
    /// `hide_cursor`: hide the cursor after this long without pointer
    /// motion over the overlay, show it again on the next motion (`None` =
    /// never hide it).
    HideCursorAfter(Option<Duration>),
    /// Forward input to another window (`SwitchWindow`, `follow_focus`)
    /// without making a new overlay.
    SetTarget(u32),
    /// Where the scaled content is now: input is mapped through it. Sent
    /// after every change of the content rect (zoom, offset, resize).
    SetMapper(CoordinateMapper),
    /// Input region: `None` = the whole window takes input, `Some(&[])` =
    /// click-through.
    SetInputRegion(Option<Vec<Rect>>),
    /// Where the bar over the game takes the pointer (`None` = no bar):
    /// events there come back as [`OverlayNotice::Pointer`] instead of going
    /// to the game (round 4, front T; `lirtup_x11::overlay_input`).
    SetToolbarZone(Option<lirtup_x11::overlay_input::ToolbarZone>),
    /// Start moving the window from the bar (windowed mode): root
    /// coordinates and the button held.
    BeginMove {
        /// Root X.
        x: i32,
        /// Root Y.
        y: i32,
        /// X button.
        button: u8,
    },
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
    /// The pointer over the bar over the game or its reveal band (see
    /// [`OverlayCommand::SetToolbarZone`]).
    Pointer(lirtup_x11::overlay_input::PointerEvent),
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
    /// `SetCursorVisible(false)`.
    cursor_forced_hidden: bool,
    /// `HideCursorAfter`.
    cursor_idle: Option<Duration>,
    /// The idle timer hid the cursor.
    cursor_idle_hidden: bool,
    last_motion: Instant,
    /// The bar's share of the pointer (`SetToolbarZone`).
    router: ToolbarRouter,
    /// The pointer is over the bar: the cursor shows.
    over_bar: bool,
    /// The window lets the pointer through (`top-transparent`).
    click_through: bool,
    /// Its own connection, to poll the pointer in click-through mode.
    probe: Option<X11>,
    next_poll: Instant,
    /// Last polled pointer, root coordinates.
    polled: Option<(i32, i32)>,
    /// The target, for the opacity (`SetTarget`).
    target: u32,
    opacity: f32,
    dimmed: Option<f32>,
    /// The opacity the window has now.
    applied_opacity: f32,
    /// The last motion forwarded to the content, sent again while the
    /// pointer rests there.
    hover: Option<OverlayEvent>,
    last_keepalive: Instant,
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
        let probe = if options.click_through {
            X11::connect()
                .inspect_err(|e| {
                    tracing::warn!("overlay: no pointer polling ({e}); the bar needs the pointer")
                })
                .ok()
        } else {
            None
        };
        let now = Instant::now();
        Ok(Overlay {
            window,
            forwarder,
            hotkeys,
            rect,
            swallowed: None,
            configure_due: None,
            target_gone: false,
            cursor_forced_hidden: false,
            cursor_idle: None,
            cursor_idle_hidden: false,
            last_motion: now,
            router: ToolbarRouter::new(),
            over_bar: false,
            click_through: options.click_through,
            probe,
            next_poll: now,
            polled: None,
            target: options.target,
            opacity: options.opacity,
            dimmed: None,
            applied_opacity: options.opacity,
            hover: None,
            last_keepalive: now,
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
            self.hide_idle_cursor();
            if let Some(notice) = self.poll_pointer()
                && notices.send(notice).is_err()
            {
                return;
            }
            self.keep_hover();
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
            OverlayCommand::SetOpacity(opacity) => {
                self.opacity = opacity;
                self.applied_opacity = opacity;
                self.window.set_opacity(opacity)
            }
            OverlayCommand::SetDimmedOpacity(dimmed) => {
                self.dimmed = dimmed;
                // The next poll applies it.
                self.polled = None;
                Ok(())
            }
            OverlayCommand::SetVisible(visible) => self.window.set_visible(visible),
            OverlayCommand::SetCursorVisible(visible) => {
                self.cursor_forced_hidden = !visible;
                self.cursor_idle_hidden = false;
                self.last_motion = Instant::now();
                self.window.set_cursor_visible(visible)
            }
            OverlayCommand::HideCursorAfter(idle) => {
                self.cursor_idle = idle;
                self.last_motion = Instant::now();
                self.show_idle_cursor()
            }
            OverlayCommand::SetTarget(target) => {
                self.target_gone = false;
                self.target = target;
                self.hover = None;
                self.window
                    .set_target(target)
                    .and_then(|()| self.forwarder.set_target(target))
            }
            OverlayCommand::SetMapper(mapper) => {
                self.forwarder.set_mapper(mapper);
                self.target_gone = false;
                Ok(())
            }
            OverlayCommand::SetInputRegion(region) => self.window.set_input_region(region.as_deref()),
            OverlayCommand::SetToolbarZone(zone) => self.set_toolbar_zone(zone),
            OverlayCommand::BeginMove { x, y, button } => {
                // The window manager takes the pointer: no release comes.
                self.router.release_capture();
                self.window.begin_move(x, y, button)
            }
            OverlayCommand::Close => Ok(()),
        };
        if let Err(e) = result {
            tracing::warn!("overlay: {e}");
        }
    }

    /// The bar showed, hid or moved. In click-through mode the input
    /// region follows it, so the bar can be clicked.
    fn set_toolbar_zone(&mut self, zone: Option<ToolbarZone>) -> lirtup_x11::Result<()> {
        let old_bar = self.router.zone().and_then(|z| z.bar);
        self.router.set_zone(zone);
        let bar = zone.and_then(|z| z.bar);
        if bar.is_none() && self.over_bar {
            self.set_over_bar(false)?;
        }
        if self.click_through && bar != old_bar {
            let region: Vec<Rect> = bar.into_iter().collect();
            self.window.set_input_region(Some(&region))?;
        }
        Ok(())
    }

    /// Show the cursor over the bar, whatever `hide_cursor` says; give it
    /// back to the rules when the pointer leaves.
    fn set_over_bar(&mut self, over: bool) -> lirtup_x11::Result<()> {
        if over == self.over_bar {
            return Ok(());
        }
        self.over_bar = over;
        if over {
            self.window.set_cursor_visible(true)
        } else {
            self.last_motion = Instant::now();
            self.window
                .set_cursor_visible(!self.cursor_forced_hidden && !self.cursor_idle_hidden)
        }
    }

    /// Click-through mode: poll the pointer for the reveal band (as motion
    /// for the bar) and the opacity of 1.x `opacity.py`.
    fn poll_pointer(&mut self) -> Option<OverlayNotice> {
        let probe = self.probe.as_ref()?;
        let now = Instant::now();
        if now < self.next_poll {
            return None;
        }
        self.next_poll = now + POINTER_POLL;
        let (x, y) = match probe.pointer_position() {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("overlay: pointer: {e}");
                return None;
            }
        };
        if self.polled == Some((x, y)) {
            return None;
        }
        self.polled = Some((x, y));
        if let Some(dimmed) = self.dimmed {
            let inside = probe
                .window_info(self.target)
                .ok()
                .flatten()
                .is_none_or(|w| lirtup_x11::overlay_input::contains(w.rect, x, y));
            let wanted = if inside { self.opacity } else { dimmed };
            if wanted != self.applied_opacity {
                self.applied_opacity = wanted;
                if let Err(e) = self.window.set_opacity(wanted) {
                    tracing::warn!("overlay: {e}");
                }
            }
        }
        // Motion for the bar only: the game gets the real events (the
        // window lets them through).
        let rect = self.window.rect();
        let motion = OverlayEvent::Pointer {
            kind: PointerKind::Motion,
            x: x - rect.x,
            y: y - rect.y,
            state: 0,
            time: 0,
        };
        match self.router.route(&motion) {
            Routed::Toolbar(e) | Routed::Both(e) => Some(OverlayNotice::Pointer(e)),
            Routed::Forward => None,
        }
    }

    /// Send the last motion again while the pointer rests on the content.
    fn keep_hover(&mut self) {
        let Some(motion) = self.hover else { return };
        if self.last_motion.elapsed() < HOVER_KEEPALIVE || self.last_keepalive.elapsed() < HOVER_KEEPALIVE {
            return;
        }
        self.last_keepalive = Instant::now();
        if let Err(e) = self.forwarder.forward(&motion) {
            tracing::debug!("overlay: hover keepalive: {e}");
            self.hover = None;
        }
    }

    /// Hide the cursor once the pointer has been still for `cursor_idle`.
    fn hide_idle_cursor(&mut self) {
        let Some(idle) = self.cursor_idle else { return };
        if self.over_bar
            || self.cursor_forced_hidden
            || self.cursor_idle_hidden
            || self.last_motion.elapsed() < idle
        {
            return;
        }
        self.cursor_idle_hidden = true;
        if let Err(e) = self.window.set_cursor_visible(false) {
            tracing::warn!("overlay: {e}");
        }
    }

    /// The pointer moved: show the cursor the idle timer hid.
    fn show_idle_cursor(&mut self) -> lirtup_x11::Result<()> {
        if !self.cursor_idle_hidden {
            return Ok(());
        }
        self.cursor_idle_hidden = false;
        self.window
            .set_cursor_visible(self.over_bar || !self.cursor_forced_hidden)
    }

    /// One event: forward it, or turn it into a notice.
    fn event(&mut self, event: OverlayEvent) -> Option<OverlayNotice> {
        if let OverlayEvent::Pointer { .. } = event {
            self.last_motion = Instant::now();
            if let Err(e) = self.show_idle_cursor() {
                tracing::warn!("overlay: {e}");
            }
        }
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
        let mut notice = None;
        if matches!(event, OverlayEvent::Pointer { .. } | OverlayEvent::Leave) {
            match self.router.route(&event) {
                Routed::Forward => {}
                Routed::Toolbar(e) => {
                    self.hover = None;
                    if let Err(e) = self.set_over_bar(true) {
                        tracing::warn!("overlay: {e}");
                    }
                    return Some(OverlayNotice::Pointer(e));
                }
                Routed::Both(e) => notice = Some(OverlayNotice::Pointer(e)),
            }
            if let Err(e) = self.set_over_bar(false) {
                tracing::warn!("overlay: {e}");
            }
            self.hover = match event {
                OverlayEvent::Pointer {
                    kind: PointerKind::Motion,
                    x,
                    y,
                    state,
                    ..
                } if !self.click_through => Some(OverlayEvent::Pointer {
                    kind: PointerKind::Motion,
                    x,
                    y,
                    state,
                    time: 0,
                }),
                OverlayEvent::Pointer { .. } => self.hover,
                _ => None,
            };
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
        notice
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
