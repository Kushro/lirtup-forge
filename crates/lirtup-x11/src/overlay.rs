//! The overlay window (3.6): ARGB visual, borderless, above everything,
//! with XFixes input regions. Front X creates the window and reads its
//! events; front K (round 2) connects the swapchain to it.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::os::fd::AsRawFd;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lirtup_config::OverlayMode;
use x11rb::connection::{Connection, RequestConnection as _};
use x11rb::protocol::Event;
use x11rb::protocol::shape::SK;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _,
    CreateWindowAux, EventMask, PropMode, Rectangle, VisualClass, Window, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use crate::conn::Atoms;
use crate::{Rect, Result, X11Error};

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

/// The overlay window. `Send`; owns an `XCBConnection` (shared with the
/// [`OverlaySurface`]s made from it, so a swapchain keeps it alive).
///
/// `always-on-top` and `top-transparent` are override-redirect windows (the
/// 1.x `X11BypassWindowManagerHint`): the window manager neither decorates
/// nor moves them. `fullscreen` asks the window manager for
/// `_NET_WM_STATE_FULLSCREEN`; `windowed` is a normal window of fixed size.
pub struct OverlayWindow {
    conn: Arc<XCBConnection>,
    window: Window,
    root: Window,
    atoms: Atoms,
    target: Window,
    mode: OverlayMode,
    rect: Rect,
    /// `click_through` of the options (`top-transparent` adds it).
    base_click_through: bool,
    click_through: bool,
    visible: bool,
    blank_cursor: Option<u32>,
    pending: VecDeque<OverlayEvent>,
}

// The pointer of `xcb_connection` is derived from `conn` on each call; the
// struct holds no raw pointer, so `Send` comes from `XCBConnection`.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<OverlayWindow>();
};

/// The connection and window id of an [`OverlayWindow`], for a Vulkan
/// surface (`VK_KHR_xcb_surface`). `Send + Sync`; cheap to clone.
#[derive(Clone)]
pub struct OverlaySurface {
    conn: Arc<XCBConnection>,
    window: Window,
}

impl OverlaySurface {
    /// The `xcb_connection_t*`. Valid as long as this value (or a clone)
    /// lives.
    pub fn xcb_connection(&self) -> NonNull<c_void> {
        NonNull::new(self.conn.get_raw_xcb_connection()).expect("XCBConnection has a connection pointer")
    }

    /// The overlay window id.
    pub fn window(&self) -> u32 {
        self.window
    }
}

impl std::fmt::Debug for OverlaySurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlaySurface")
            .field("window", &self.window)
            .finish_non_exhaustive()
    }
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<OverlaySurface>();
};

/// Event mask of the overlay window.
fn overlay_events() -> EventMask {
    EventMask::EXPOSURE
        | EventMask::STRUCTURE_NOTIFY
        | EventMask::KEY_PRESS
        | EventMask::KEY_RELEASE
        | EventMask::BUTTON_PRESS
        | EventMask::BUTTON_RELEASE
        | EventMask::POINTER_MOTION
        | EventMask::ENTER_WINDOW
        | EventMask::LEAVE_WINDOW
}

fn is_override_redirect(mode: OverlayMode) -> bool {
    matches!(mode, OverlayMode::AlwaysOnTop | OverlayMode::TopTransparent)
}

impl OverlayWindow {
    /// Create and map the window.
    pub fn create(options: &OverlayOptions) -> Result<OverlayWindow> {
        let (conn, screen_num) = XCBConnection::connect(None)?;
        let conn = Arc::new(conn);
        let screen = conn.setup().roots[screen_num].clone();
        let atoms = Atoms::new(&conn)?.reply()?;

        // A 32-bit TrueColor visual for the transparency; the root visual if
        // there is none (no compositor-friendly alpha, still works).
        let argb = screen
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .find_map(|d| {
                d.visuals
                    .iter()
                    .find(|v| v.class == VisualClass::TRUE_COLOR)
                    .map(|v| (d.depth, v.visual_id))
            });
        let (depth, visual) = argb.unwrap_or_else(|| {
            tracing::warn!("no 32-bit ARGB visual; the overlay will not be translucent");
            (screen.root_depth, screen.root_visual)
        });
        let colormap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual)?;

        let rect = options.rect;
        let window = conn.generate_id()?;
        conn.create_window(
            depth,
            window,
            screen.root,
            clamp_i16(rect.x),
            clamp_i16(rect.y),
            clamp_side(rect.width),
            clamp_side(rect.height),
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .override_redirect(u32::from(is_override_redirect(options.mode)))
                .event_mask(overlay_events()),
        )?
        .check()?;
        // The window keeps the colormap alive.
        conn.free_colormap(colormap)?;

        // Fixed properties.
        let title = options.title.as_bytes();
        conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title,
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            window,
            atoms._NET_WM_NAME,
            atoms.UTF8_STRING,
            title,
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"lirtup\0LirtUp\0",
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms._NET_WM_PID,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms.WM_PROTOCOLS,
            AtomEnum::ATOM,
            &[atoms.WM_DELETE_WINDOW],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms._LIRTUP_OVERLAY,
            AtomEnum::CARDINAL,
            &[1],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms._NET_WM_WINDOW_TYPE,
            AtomEnum::ATOM,
            &[atoms._NET_WM_WINDOW_TYPE_NORMAL],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms._NET_WM_BYPASS_COMPOSITOR,
            AtomEnum::CARDINAL,
            &[u32::from(options.bypass_compositor)],
        )?;

        let mut overlay = OverlayWindow {
            conn,
            window,
            root: screen.root,
            atoms,
            target: options.target,
            mode: options.mode,
            rect,
            base_click_through: options.click_through,
            click_through: options.click_through || options.mode == OverlayMode::TopTransparent,
            visible: true,
            blank_cursor: None,
            pending: VecDeque::new(),
        };
        overlay.apply_mode_properties(rect)?;
        overlay.set_opacity(options.opacity)?;
        if overlay.click_through {
            overlay.set_input_region(Some(&[]))?;
        }
        overlay.conn.map_window(window)?;
        // Override-redirect windows are not stacked by the window manager.
        overlay.conn.configure_window(
            window,
            &ConfigureWindowAux::new().stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
        )?;
        overlay.conn.flush()?;
        Ok(overlay)
    }

    /// Window id.
    pub fn window(&self) -> u32 {
        self.window
    }

    /// The `xcb_connection_t*` for `lirtup_gpu::XcbTarget` (lives as long as
    /// this window). Prefer [`OverlayWindow::surface`], which keeps the
    /// connection alive by itself.
    pub fn xcb_connection(&self) -> NonNull<c_void> {
        NonNull::new(self.conn.get_raw_xcb_connection()).expect("XCBConnection has a connection pointer")
    }

    /// What a swapchain needs to present to this window. It holds a
    /// reference to the connection, so the pointer stays valid while the
    /// surface lives, even after the window is dropped (presenting then
    /// fails with `OutOfDate` or a surface-lost error, never a dangling
    /// pointer). Round 2: `lirtup_pipeline::surface` turns it into a
    /// `lirtup_gpu::XcbTarget`.
    pub fn surface(&self) -> OverlaySurface {
        OverlaySurface {
            conn: Arc::clone(&self.conn),
            window: self.window,
        }
    }

    /// Current geometry.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Change the behavior (fullscreen ↔ windowed, ...).
    ///
    /// The window is unmapped, reconfigured and mapped again (the window
    /// manager reads override-redirect and the initial state at map time).
    /// [`OverlayWindow::rect`] keeps the old geometry until the window
    /// manager places the window: the [`OverlayEvent::Configure`] that
    /// follows reports where it ended up (none if that is where it was).
    pub fn set_mode(&mut self, mode: OverlayMode, rect: Rect) -> Result<()> {
        if self.visible {
            self.conn.unmap_window(self.window)?;
        }
        self.mode = mode;
        self.conn.change_window_attributes(
            self.window,
            &ChangeWindowAttributesAux::new().override_redirect(u32::from(is_override_redirect(mode))),
        )?;
        self.conn.configure_window(
            self.window,
            &ConfigureWindowAux::new()
                .x(i32::from(clamp_i16(rect.x)))
                .y(i32::from(clamp_i16(rect.y)))
                .width(u32::from(clamp_side(rect.width)))
                .height(u32::from(clamp_side(rect.height))),
        )?;
        self.apply_mode_properties(rect)?;
        let click_through = self.base_click_through || mode == OverlayMode::TopTransparent;
        if click_through != self.click_through {
            self.click_through = click_through;
            self.set_input_region(if click_through { Some(&[]) } else { None })?;
        }
        if self.visible {
            self.conn.map_window(self.window)?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Opacity 0 to 1.
    pub fn set_opacity(&mut self, opacity: f32) -> Result<()> {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity >= 1.0 {
            self.conn
                .delete_property(self.window, self.atoms._NET_WM_WINDOW_OPACITY)?;
        } else {
            let value = (f64::from(opacity) * f64::from(u32::MAX)).round() as u32;
            self.conn.change_property32(
                PropMode::REPLACE,
                self.window,
                self.atoms._NET_WM_WINDOW_OPACITY,
                AtomEnum::CARDINAL,
                &[value],
            )?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Input region: `None` = whole window takes input, `Some(empty)` = all
    /// clicks pass through, otherwise only those rectangles take input (the
    /// in-game toolbar of round 4).
    pub fn set_input_region(&mut self, region: Option<&[Rect]>) -> Result<()> {
        if self
            .conn
            .extension_information(x11rb::protocol::xfixes::X11_EXTENSION_NAME)?
            .is_none()
        {
            return Err(X11Error::MissingExtension("XFIXES"));
        }
        self.conn.xfixes_query_version(5, 0)?.reply()?;
        match region {
            None => {
                self.conn
                    .xfixes_set_window_shape_region(self.window, SK::INPUT, 0, 0, x11rb::NONE)?;
            }
            Some(rects) => {
                let rects: Vec<Rectangle> = rects
                    .iter()
                    .map(|r| Rectangle {
                        x: clamp_i16(r.x),
                        y: clamp_i16(r.y),
                        width: r.width.min(u32::from(u16::MAX)) as u16,
                        height: r.height.min(u32::from(u16::MAX)) as u16,
                    })
                    .collect();
                let id = self.conn.generate_id()?;
                self.conn.xfixes_create_region(id, &rects)?;
                self.conn
                    .xfixes_set_window_shape_region(self.window, SK::INPUT, 0, 0, id)?;
                self.conn.xfixes_destroy_region(id)?;
            }
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Show or hide the pointer over the overlay (`hide_cursor`).
    pub fn set_cursor_visible(&mut self, visible: bool) -> Result<()> {
        let cursor = if visible {
            x11rb::NONE
        } else {
            match self.blank_cursor {
                Some(c) => c,
                None => {
                    let c = crate::cursor::blank_cursor(&self.conn, self.window)?;
                    self.blank_cursor = Some(c);
                    c
                }
            }
        };
        self.conn
            .change_window_attributes(self.window, &ChangeWindowAttributesAux::new().cursor(cursor))?;
        self.conn.flush()?;
        Ok(())
    }

    /// Map or unmap (`pause_on_focus_loss`).
    pub fn set_visible(&mut self, visible: bool) -> Result<()> {
        if visible == self.visible {
            return Ok(());
        }
        if visible {
            self.conn.map_window(self.window)?;
            self.conn.configure_window(
                self.window,
                &ConfigureWindowAux::new().stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
            )?;
        } else {
            self.conn.unmap_window(self.window)?;
        }
        self.visible = visible;
        self.conn.flush()?;
        Ok(())
    }

    /// Follow another target window (`WM_TRANSIENT_FOR` outside windowed
    /// mode); the window itself stays as it is.
    pub fn set_target(&mut self, target: u32) -> Result<()> {
        self.target = target;
        if self.mode == OverlayMode::Windowed || target == x11rb::NONE {
            self.conn
                .delete_property(self.window, AtomEnum::WM_TRANSIENT_FOR.into())?;
        } else {
            self.conn.change_property32(
                PropMode::REPLACE,
                self.window,
                AtomEnum::WM_TRANSIENT_FOR,
                AtomEnum::WINDOW,
                &[target],
            )?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Move the window (drag in windowed mode).
    pub fn move_to(&mut self, x: i32, y: i32) -> Result<()> {
        self.conn.configure_window(
            self.window,
            &ConfigureWindowAux::new()
                .x(i32::from(clamp_i16(x)))
                .y(i32::from(clamp_i16(y))),
        )?;
        self.rect.x = x;
        self.rect.y = y;
        self.conn.flush()?;
        Ok(())
    }

    /// Drain pending events into `out` without blocking.
    pub fn poll_events(&mut self, out: &mut Vec<OverlayEvent>) -> Result<()> {
        out.extend(self.pending.drain(..));
        while let Some(event) = self.conn.poll_for_event()? {
            if let Some(e) = self.translate(event)? {
                out.push(e);
            }
        }
        Ok(())
    }

    /// Block until an event arrives or `timeout` passes.
    pub fn wait_event(&mut self, timeout: std::time::Duration) -> Result<Option<OverlayEvent>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(e) = self.pending.pop_front() {
                return Ok(Some(e));
            }
            while let Some(event) = self.conn.poll_for_event()? {
                if let Some(e) = self.translate(event)? {
                    self.pending.push_back(e);
                }
            }
            if let Some(e) = self.pending.pop_front() {
                return Ok(Some(e));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            self.conn.flush()?;
            wait_readable(self.conn.as_raw_fd(), left)?;
        }
    }

    /// `_MOTIF_WM_HINTS`, `_NET_WM_STATE`, `WM_TRANSIENT_FOR` and
    /// `WM_NORMAL_HINTS` of the current mode at `rect` (read by the window
    /// manager when the window is mapped).
    fn apply_mode_properties(&mut self, rect: Rect) -> Result<()> {
        let (c, w, a) = (&self.conn, self.window, &self.atoms);
        let windowed = self.mode == OverlayMode::Windowed;
        // flags = MWM_HINTS_DECORATIONS, decorations = 0 (none) or 1 (all).
        c.change_property32(
            PropMode::REPLACE,
            w,
            a._MOTIF_WM_HINTS,
            a._MOTIF_WM_HINTS,
            &[2, 0, u32::from(windowed), 0, 0],
        )?;
        let mut state = Vec::new();
        match self.mode {
            OverlayMode::Fullscreen => state.extend([a._NET_WM_STATE_FULLSCREEN, a._NET_WM_STATE_ABOVE]),
            OverlayMode::AlwaysOnTop | OverlayMode::TopTransparent => state.extend([
                a._NET_WM_STATE_ABOVE,
                a._NET_WM_STATE_SKIP_TASKBAR,
                a._NET_WM_STATE_SKIP_PAGER,
            ]),
            OverlayMode::Windowed => {}
        }
        c.change_property32(PropMode::REPLACE, w, a._NET_WM_STATE, AtomEnum::ATOM, &state)?;
        if windowed || self.target == x11rb::NONE {
            c.delete_property(w, AtomEnum::WM_TRANSIENT_FOR.into())?;
        } else {
            c.change_property32(
                PropMode::REPLACE,
                w,
                AtomEnum::WM_TRANSIENT_FOR,
                AtomEnum::WINDOW,
                &[self.target],
            )?;
        }
        c.change_property32(
            PropMode::REPLACE,
            w,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            &size_hints(rect, windowed),
        )?;
        Ok(())
    }

    /// One X event → at most one overlay event.
    fn translate(&mut self, event: Event) -> Result<Option<OverlayEvent>> {
        use PointerKind::{Motion, Press, Release};
        let w = self.window;
        Ok(match event {
            Event::ButtonPress(e) if e.event == w => Some(pointer(
                Press(e.detail),
                e.event_x,
                e.event_y,
                e.state.into(),
                e.time,
            )),
            Event::ButtonRelease(e) if e.event == w => Some(pointer(
                Release(e.detail),
                e.event_x,
                e.event_y,
                e.state.into(),
                e.time,
            )),
            Event::MotionNotify(e) if e.event == w => {
                Some(pointer(Motion, e.event_x, e.event_y, e.state.into(), e.time))
            }
            Event::KeyPress(e) if e.event == w => Some(OverlayEvent::Key {
                pressed: true,
                keycode: e.detail,
                state: e.state.into(),
                time: e.time,
            }),
            Event::KeyRelease(e) if e.event == w => Some(OverlayEvent::Key {
                pressed: false,
                keycode: e.detail,
                state: e.state.into(),
                time: e.time,
            }),
            Event::EnterNotify(e) if e.event == w => Some(OverlayEvent::Enter),
            Event::LeaveNotify(e) if e.event == w => Some(OverlayEvent::Leave),
            Event::Expose(e) if e.window == w && e.count == 0 => Some(OverlayEvent::Expose),
            Event::ConfigureNotify(e) if e.window == w => {
                // Real events of a reparented window are relative to the
                // frame; synthetic ones (from the window manager) and those
                // of override-redirect windows are in root coordinates.
                let synthetic = e.response_type & 0x80 != 0;
                let (x, y) = if synthetic || e.override_redirect {
                    (i32::from(e.x), i32::from(e.y))
                } else {
                    let t = self.conn.translate_coordinates(w, self.root, 0, 0)?.reply()?;
                    (i32::from(t.dst_x), i32::from(t.dst_y))
                };
                let rect = Rect {
                    x,
                    y,
                    width: e.width.into(),
                    height: e.height.into(),
                };
                if rect == self.rect {
                    None
                } else {
                    self.rect = rect;
                    Some(OverlayEvent::Configure(rect))
                }
            }
            Event::ClientMessage(e)
                if e.window == w
                    && e.type_ == self.atoms.WM_PROTOCOLS
                    && e.data.as_data32()[0] == self.atoms.WM_DELETE_WINDOW =>
            {
                Some(OverlayEvent::Close)
            }
            Event::Error(e) => {
                tracing::debug!("overlay: X error {:?}", e.error_kind);
                None
            }
            _ => None,
        })
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        if let Some(c) = self.blank_cursor {
            let _ = self.conn.free_cursor(c);
        }
        let _ = self.conn.destroy_window(self.window);
        let _ = self.conn.flush();
    }
}

fn pointer(kind: PointerKind, x: i16, y: i16, state: u16, time: u32) -> OverlayEvent {
    OverlayEvent::Pointer {
        kind,
        x: x.into(),
        y: y.into(),
        state,
        time,
    }
}

/// `WM_SIZE_HINTS`: user position and size, and min = max when `fixed`.
fn size_hints(rect: Rect, fixed: bool) -> [u32; 18] {
    const US_POSITION: u32 = 1;
    const US_SIZE: u32 = 2;
    const P_MIN_SIZE: u32 = 16;
    const P_MAX_SIZE: u32 = 32;
    let mut h = [0u32; 18];
    h[0] = US_POSITION | US_SIZE | if fixed { P_MIN_SIZE | P_MAX_SIZE } else { 0 };
    h[1] = rect.x as u32;
    h[2] = rect.y as u32;
    h[3] = rect.width;
    h[4] = rect.height;
    if fixed {
        h[5] = rect.width;
        h[6] = rect.height;
        h[7] = rect.width;
        h[8] = rect.height;
    }
    h
}

fn clamp_i16(v: i32) -> i16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn clamp_side(v: u32) -> u16 {
    v.clamp(1, u32::from(u16::MAX)) as u16
}

/// `poll(2)` the connection for input, up to `timeout`.
fn wait_readable(fd: std::os::fd::RawFd, timeout: Duration) -> Result<()> {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = timeout.as_millis().clamp(1, i32::MAX as u128) as i32;
    // SAFETY: one valid `pollfd`, count 1.
    let r = unsafe { libc::poll(&mut pfd, 1, ms) };
    if r < 0 {
        let e = std::io::Error::last_os_error();
        if e.kind() != std::io::ErrorKind::Interrupted {
            return Err(X11Error::Connection(format!("poll: {e}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_fix_size_only_when_asked() {
        let r = Rect {
            x: 10,
            y: 20,
            width: 640,
            height: 480,
        };
        let h = size_hints(r, true);
        assert_eq!(h[0], 1 | 2 | 16 | 32);
        assert_eq!(&h[5..9], &[640, 480, 640, 480]);
        let h = size_hints(r, false);
        assert_eq!(h[0], 3);
        assert_eq!(&h[5..9], &[0, 0, 0, 0]);
    }

    #[test]
    fn clamps() {
        assert_eq!(clamp_i16(-100_000), i16::MIN);
        assert_eq!(clamp_side(0), 1);
        assert_eq!(clamp_side(100_000), u16::MAX);
    }

    #[test]
    fn override_redirect_modes() {
        assert!(is_override_redirect(OverlayMode::AlwaysOnTop));
        assert!(is_override_redirect(OverlayMode::TopTransparent));
        assert!(!is_override_redirect(OverlayMode::Fullscreen));
        assert!(!is_override_redirect(OverlayMode::Windowed));
    }
}
