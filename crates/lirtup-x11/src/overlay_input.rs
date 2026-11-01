//! Pointer input for the bar over the game (round 4, front T).
//!
//! The overlay forwards the pointer to the game ([`crate::input`]). While
//! the bar is shown, events inside its rectangle — and the motion in the
//! reveal band at the top edge — are taken here instead and handed to the
//! session thread. In windowed mode a press on the bar can start moving the
//! window with `_NET_WM_MOVERESIZE`.

use x11rb::protocol::Event;

use crate::{Rect, Result};

/// Where the bar takes the pointer, in overlay (window) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarZone {
    /// The bar while shown (`None` = hidden: only the reveal band counts).
    pub bar: Option<Rect>,
    /// Height of the band at the top edge that reveals the bar, px.
    pub reveal: u32,
}

/// A pointer event for the bar, in overlay coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvent {
    /// Motion over the bar or the reveal band.
    Motion {
        /// X.
        x: i32,
        /// Y.
        y: i32,
    },
    /// A button pressed over the bar.
    Press {
        /// X button (1 = left).
        button: u8,
        /// X.
        x: i32,
        /// Y.
        y: i32,
    },
    /// A button released (anywhere, after a press on the bar).
    Release {
        /// X button.
        button: u8,
        /// X.
        x: i32,
        /// Y.
        y: i32,
    },
    /// The pointer left the bar and the band.
    Leave,
}

/// What the overlay does with one X event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routed {
    /// Not the bar's: forward it to the game as before.
    Forward,
    /// The bar's: send it to the session thread.
    Toolbar(PointerEvent),
}

/// Route one X event of the overlay window.
pub fn route(zone: &ToolbarZone, event: &Event) -> Routed {
    let _ = (zone, event);
    todo!()
}

/// Start moving `window` with the window manager (`_NET_WM_MOVERESIZE`,
/// `_NET_WM_MOVERESIZE_MOVE`) from the pointer at root `(x, y)` with
/// `button` held: the bar's drag in windowed mode.
pub fn begin_move(
    conn: &impl x11rb::connection::Connection,
    root: u32,
    window: u32,
    x: i32,
    y: i32,
    button: u8,
) -> Result<()> {
    let _ = (conn, root, window, x, y, button);
    todo!()
}
