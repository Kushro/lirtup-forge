//! A connection for window queries (`window/connection.py`, `AtomCache`).

use std::cell::Cell;

use x11rb::connection::Connection;
use x11rb::rust_connection::RustConnection;

use crate::Result;

x11rb::atom_manager! {
    /// Atoms every connection of this crate uses, interned once at connect.
    pub(crate) Atoms: AtomsCookie {
        UTF8_STRING,
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        WM_STATE,
        _NET_SUPPORTED,
        _NET_CLIENT_LIST,
        _NET_CLIENT_LIST_STACKING,
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_WM_ICON,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NORMAL,
        _NET_WM_WINDOW_TYPE_DESKTOP,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_TOOLBAR,
        _NET_WM_WINDOW_TYPE_MENU,
        _NET_WM_WINDOW_TYPE_UTILITY,
        _NET_WM_WINDOW_TYPE_SPLASH,
        _NET_WM_WINDOW_TYPE_DIALOG,
        _NET_WM_WINDOW_TYPE_POPUP_MENU,
        _NET_WM_STATE,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_FULLSCREEN,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_SKIP_PAGER,
        _NET_WM_WINDOW_OPACITY,
        _NET_WM_BYPASS_COMPOSITOR,
        _MOTIF_WM_HINTS,
        _LIRTUP_OVERLAY,
    }
}

/// A connection to the X server with its atom cache.
///
/// `Send`; one per thread. Methods are in [`crate::window`],
/// [`crate::monitor`], [`crate::pick`], [`crate::acquire`].
pub struct X11 {
    pub(crate) conn: RustConnection,
    pub(crate) screen: usize,
    pub(crate) root: u32,
    pub(crate) atoms: Atoms,
    xwayland: bool,
    /// `enable_thumbnails` already ran on this connection.
    pub(crate) thumbnails: Cell<bool>,
}

impl X11 {
    /// Connect to `$DISPLAY`.
    pub fn connect() -> Result<X11> {
        let (conn, screen) = RustConnection::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let atoms = Atoms::new(&conn)?.reply()?;
        let xwayland = detect_xwayland(&conn);
        Ok(X11 { conn, screen, root, atoms, xwayland, thumbnails: Cell::new(false) })
    }

    /// Root window of the default screen.
    pub fn root(&self) -> u32 {
        self.root
    }

    /// Whether the session is XWayland (for Diagnostics and the overlay
    /// quirks).
    pub fn is_xwayland(&self) -> bool {
        self.xwayland
    }
}

/// XWayland advertises the `XWAYLAND` extension (Xwayland 21.1+); older ones
/// are recognized by their RandR output names (`XWAYLAND0`, ...).
pub(crate) fn detect_xwayland<C: Connection>(conn: &C) -> bool {
    if matches!(conn.extension_information("XWAYLAND"), Ok(Some(_))) {
        return true;
    }
    let root = conn.setup().roots[0].root;
    let Ok(cookie) = x11rb::protocol::randr::get_screen_resources_current(conn, root) else {
        return false;
    };
    let Ok(res) = cookie.reply() else { return false };
    res.outputs.iter().any(|&output| {
        x11rb::protocol::randr::get_output_info(conn, output, res.config_timestamp)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|info| info.name.starts_with(b"XWAYLAND"))
    })
}
