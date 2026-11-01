//! Pointer input for the bar over the game (round 4, front T).
//!
//! The overlay forwards the pointer to the game ([`crate::input`]). While
//! the bar is shown, events inside its rectangle are taken here instead and
//! handed to the session thread; the motion in the reveal band at the top
//! edge goes to both (the game keeps its hover). A press on the bar keeps
//! the pointer until its release, wherever that happens. In windowed mode a
//! press on the bar can start moving the window with `_NET_WM_MOVERESIZE`.
//!
//! [`ToolbarRouter`] keeps that state per overlay; it works on the
//! translated [`OverlayEvent`]s the overlay thread already reads, not on
//! raw X events.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask, Time};

use crate::overlay::{OverlayEvent, PointerKind};
use crate::{Rect, Result, X11};

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

/// What the overlay does with one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routed {
    /// Not the bar's: forward it to the game as before.
    Forward,
    /// The bar's: send it to the session thread, do not forward it.
    Toolbar(PointerEvent),
    /// Forward it and tell the bar too (motion in the reveal band, leaving).
    Both(PointerEvent),
}

/// Routes the overlay's pointer events between the game and the bar.
#[derive(Debug, Clone, Default)]
pub struct ToolbarRouter {
    zone: Option<ToolbarZone>,
    /// The button pressed on the bar, until its release.
    captured: Option<u8>,
    /// The pointer was over the bar or the band (a `Leave` is due).
    inside: bool,
}

impl ToolbarRouter {
    /// A router with no bar: everything is forwarded.
    pub fn new() -> ToolbarRouter {
        ToolbarRouter::default()
    }

    /// The bar moved, showed or hid (`None` = no bar this session).
    pub fn set_zone(&mut self, zone: Option<ToolbarZone>) {
        self.zone = zone;
    }

    /// The current zone.
    pub fn zone(&self) -> Option<ToolbarZone> {
        self.zone
    }

    /// Route one event of the overlay window.
    pub fn route(&mut self, event: &OverlayEvent) -> Routed {
        let Some(zone) = self.zone else {
            self.inside = false;
            return match (event, self.captured) {
                // A release after a press on a bar that has gone since.
                (
                    OverlayEvent::Pointer {
                        kind: PointerKind::Release(b),
                        ..
                    },
                    Some(c),
                ) if *b == c => {
                    self.captured = None;
                    Routed::Forward
                }
                _ => Routed::Forward,
            };
        };
        match *event {
            OverlayEvent::Pointer { kind, x, y, .. } => {
                let on_bar = zone.bar.is_some_and(|r| contains(r, x, y));
                let in_band = y >= 0 && y < zone.reveal as i32;
                match kind {
                    PointerKind::Motion => {
                        if self.captured.is_some() || on_bar {
                            self.inside = true;
                            Routed::Toolbar(PointerEvent::Motion { x, y })
                        } else if in_band {
                            self.inside = true;
                            Routed::Both(PointerEvent::Motion { x, y })
                        } else if std::mem::take(&mut self.inside) {
                            Routed::Both(PointerEvent::Leave)
                        } else {
                            Routed::Forward
                        }
                    }
                    PointerKind::Press(button) if on_bar && self.captured.is_none() => {
                        self.captured = Some(button);
                        self.inside = true;
                        Routed::Toolbar(PointerEvent::Press { button, x, y })
                    }
                    // Wheel and other buttons over the bar while one is held.
                    PointerKind::Press(button) if self.captured.is_some() || on_bar => {
                        Routed::Toolbar(PointerEvent::Press { button, x, y })
                    }
                    PointerKind::Release(button) if self.captured == Some(button) => {
                        self.captured = None;
                        Routed::Toolbar(PointerEvent::Release { button, x, y })
                    }
                    PointerKind::Release(_) if on_bar => {
                        Routed::Toolbar(PointerEvent::Release { button: 0, x, y })
                    }
                    _ => Routed::Forward,
                }
            }
            OverlayEvent::Leave if self.captured.is_none() && std::mem::take(&mut self.inside) => {
                Routed::Both(PointerEvent::Leave)
            }
            _ => Routed::Forward,
        }
    }

    /// A press on the bar was handed to the window manager
    /// ([`begin_move`]): its release will not come.
    pub fn release_capture(&mut self) {
        self.captured = None;
    }
}

impl X11 {
    /// Where the pointer is, root coordinates. The overlay polls it where
    /// it gets no pointer events (click-through `top-transparent`): the
    /// reveal band of the bar and the opacity of 1.x `opacity.py`.
    pub fn pointer_position(&self) -> Result<(i32, i32)> {
        let reply = self.conn.query_pointer(self.root)?.reply()?;
        Ok((i32::from(reply.root_x), i32::from(reply.root_y)))
    }
}

/// `r` holds `(x, y)`.
pub fn contains(r: Rect, x: i32, y: i32) -> bool {
    x >= r.x
        && y >= r.y
        && i64::from(x) < i64::from(r.x) + i64::from(r.width)
        && i64::from(y) < i64::from(r.y) + i64::from(r.height)
}

/// `_NET_WM_MOVERESIZE_MOVE`.
const MOVERESIZE_MOVE: u32 = 8;
/// Source indication: a normal application.
const SOURCE_APPLICATION: u32 = 1;

/// Start moving `window` with the window manager (`_NET_WM_MOVERESIZE`,
/// `_NET_WM_MOVERESIZE_MOVE`) from the pointer at root `(x, y)` with
/// `button` held: the bar's drag in windowed mode. The implicit grab of the
/// press is released first, as the EWMH asks; the window manager then
/// owns the pointer until the button goes up.
pub fn begin_move(conn: &impl Connection, root: u32, window: u32, x: i32, y: i32, button: u8) -> Result<()> {
    let atom = conn.intern_atom(false, b"_NET_WM_MOVERESIZE")?.reply()?.atom;
    conn.ungrab_pointer(Time::CURRENT_TIME)?;
    let event = ClientMessageEvent::new(
        32,
        window,
        atom,
        [
            x as u32,
            y as u32,
            MOVERESIZE_MOVE,
            u32::from(button),
            SOURCE_APPLICATION,
        ],
    );
    conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )?;
    conn.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pointer(kind: PointerKind, x: i32, y: i32) -> OverlayEvent {
        OverlayEvent::Pointer {
            kind,
            x,
            y,
            state: 0,
            time: 0,
        }
    }

    fn router(bar: Option<Rect>) -> ToolbarRouter {
        let mut r = ToolbarRouter::new();
        r.set_zone(Some(ToolbarZone { bar, reveal: 8 }));
        r
    }

    const BAR: Rect = Rect {
        x: 100,
        y: 16,
        width: 400,
        height: 64,
    };

    #[test]
    fn no_bar_forwards_everything() {
        let mut r = ToolbarRouter::new();
        assert_eq!(r.route(&pointer(PointerKind::Motion, 0, 0)), Routed::Forward);
        assert_eq!(r.route(&pointer(PointerKind::Press(1), 0, 0)), Routed::Forward);
    }

    #[test]
    fn the_band_reveals_and_still_forwards() {
        let mut r = router(None);
        assert_eq!(
            r.route(&pointer(PointerKind::Motion, 300, 3)),
            Routed::Both(PointerEvent::Motion { x: 300, y: 3 })
        );
        // Leaving the band: the game gets the motion, the bar a Leave.
        assert_eq!(
            r.route(&pointer(PointerKind::Motion, 300, 200)),
            Routed::Both(PointerEvent::Leave)
        );
        assert_eq!(r.route(&pointer(PointerKind::Motion, 300, 210)), Routed::Forward);
        // Clicks in the band are the game's.
        assert_eq!(r.route(&pointer(PointerKind::Press(1), 300, 3)), Routed::Forward);
    }

    #[test]
    fn the_bar_keeps_a_press_until_its_release() {
        let mut r = router(Some(BAR));
        assert_eq!(
            r.route(&pointer(PointerKind::Press(1), 120, 30)),
            Routed::Toolbar(PointerEvent::Press {
                button: 1,
                x: 120,
                y: 30
            })
        );
        // Dragged out of the bar: still the bar's.
        assert_eq!(
            r.route(&pointer(PointerKind::Motion, 900, 500)),
            Routed::Toolbar(PointerEvent::Motion { x: 900, y: 500 })
        );
        assert_eq!(
            r.route(&pointer(PointerKind::Release(1), 900, 500)),
            Routed::Toolbar(PointerEvent::Release {
                button: 1,
                x: 900,
                y: 500
            })
        );
        // Then the game's again, after one Leave.
        assert_eq!(
            r.route(&pointer(PointerKind::Motion, 900, 501)),
            Routed::Both(PointerEvent::Leave)
        );
        assert_eq!(
            r.route(&pointer(PointerKind::Press(1), 900, 501)),
            Routed::Forward
        );
    }

    #[test]
    fn leaving_the_window_tells_the_bar_once() {
        let mut r = router(Some(BAR));
        r.route(&pointer(PointerKind::Motion, 120, 30));
        assert_eq!(r.route(&OverlayEvent::Leave), Routed::Both(PointerEvent::Leave));
        assert_eq!(r.route(&OverlayEvent::Leave), Routed::Forward);
    }

    #[test]
    fn a_move_drops_the_capture() {
        let mut r = router(Some(BAR));
        r.route(&pointer(PointerKind::Press(1), 120, 30));
        r.release_capture();
        assert_eq!(
            r.route(&pointer(PointerKind::Press(1), 900, 500)),
            Routed::Forward
        );
    }
}
