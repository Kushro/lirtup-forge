//! Input forwarding from the overlay to the target window
//! (`overlay/forwarder.py`, `X11EventForwarder` and `CoordinateMapper`).

use lirtup_config::{Hotkey, HotkeyAction};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ButtonPressEvent, ConnectionExt as _, ENTER_NOTIFY_EVENT,
    EnterNotifyEvent, EventMask, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, KeyButMask, KeyPressEvent,
    LEAVE_NOTIFY_EVENT, MOTION_NOTIFY_EVENT, Motion, MotionNotifyEvent, NotifyDetail, NotifyMode,
};
use x11rb::rust_connection::RustConnection;

use crate::error::gone_to_none;
use crate::overlay::{OverlayEvent, PointerKind};
use crate::{Rect, Result, X11Error};

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
    ///
    /// The point is scaled from `content` to `source_size` (rounded down,
    /// clamped to the last pixel) and shifted by `crop_offset`, like
    /// `CoordinateMapper.map` of 1.x.
    pub fn to_target(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let c = self.content;
        let (sw, sh) = self.source_size;
        if c.width == 0 || c.height == 0 || sw == 0 || sh == 0 {
            return None;
        }
        let (dx, dy) = (i64::from(x) - i64::from(c.x), i64::from(y) - i64::from(c.y));
        if dx < 0 || dy < 0 || dx >= i64::from(c.width) || dy >= i64::from(c.height) {
            return None;
        }
        let sx = (dx * i64::from(sw) / i64::from(c.width)).min(i64::from(sw) - 1);
        let sy = (dy * i64::from(sh) / i64::from(c.height)).min(i64::from(sh) - 1);
        Some((
            sx as i32 + self.crop_offset.0 as i32,
            sy as i32 + self.crop_offset.1 as i32,
        ))
    }
}

/// Replays overlay events on the target with `SendEvent`. `Send`; its own
/// connection.
///
/// Synthetic events only, as 1.x. Programs that drop `send_event` events
/// (some SDL and Wine builds) would need XTest, which moves the real
/// pointer; round 2 decided to add it only if a game in the tests needs it.
pub struct InputForwarder {
    conn: RustConnection,
    root: u32,
    target: u32,
    mapper: CoordinateMapper,
    /// Target origin in root coordinates (for `root_x`/`root_y`); re-read
    /// on every enter and after `set_mapper`.
    origin: Option<(i32, i32)>,
    /// The pointer is inside the target (an `EnterNotify` was sent).
    inside: bool,
    /// Last target position, for key events.
    last: (i32, i32),
}

impl InputForwarder {
    /// Forward to `target`.
    pub fn new(target: u32) -> Result<InputForwarder> {
        let (conn, screen) = RustConnection::connect(None)?;
        let root = conn.setup().roots[screen].root;
        Ok(InputForwarder {
            conn,
            root,
            target,
            mapper: CoordinateMapper::default(),
            origin: None,
            inside: false,
            last: (0, 0),
        })
    }

    /// Forward to another window from now on (the pointer leaves the old
    /// one first).
    pub fn set_target(&mut self, target: u32) -> Result<()> {
        if target == self.target {
            return Ok(());
        }
        let left = self.leave(0, x11rb::CURRENT_TIME);
        self.target = target;
        self.origin = None;
        self.inside = false;
        // The old target may be gone already.
        match left {
            Err(X11Error::NoWindow(_)) => Ok(()),
            other => other,
        }
    }

    /// Update the mapping (geometry, zoom or offset changed).
    pub fn set_mapper(&mut self, mapper: CoordinateMapper) {
        self.mapper = mapper;
        self.origin = None;
    }

    /// The hotkeys among `bindings`, matched against key presses on the
    /// overlay (which may hold the keyboard focus where the global grab does
    /// not see them: a binding another program grabbed, for one).
    pub fn hotkey_matcher(&self, bindings: &[(HotkeyAction, Hotkey)]) -> Result<HotkeyMatcher> {
        let mut keys = Vec::new();
        for (action, hotkey) in bindings {
            let Some(keysym) = crate::hotkeys::keysym_for(&hotkey.key) else {
                continue;
            };
            for keycode in crate::hotkeys::keycodes_for(&self.conn, keysym)? {
                keys.push(HotkeyKey {
                    modifiers: hotkey.modifiers.0 & MODIFIER_BITS & !LOCK_BITS,
                    keycode,
                    action: *action,
                });
            }
        }
        Ok(HotkeyMatcher { keys })
    }

    /// Forward one event; events that do not map are dropped.
    pub fn forward(&mut self, event: &OverlayEvent) -> Result<()> {
        match *event {
            OverlayEvent::Pointer {
                kind,
                x,
                y,
                state,
                time,
            } => {
                let Some((tx, ty)) = self.mapper.to_target(x, y) else {
                    // Over the letterbox: the target loses the pointer.
                    return self.leave(state, time);
                };
                if !self.inside {
                    self.origin = None;
                    self.enter(tx, ty, state, time)?;
                }
                self.last = (tx, ty);
                let (ox, oy) = self.origin()?;
                let (rx, ry) = (clamp_i16(ox + tx), clamp_i16(oy + ty));
                let (ex, ey) = (clamp_i16(tx), clamp_i16(ty));
                let state = KeyButMask::from(state);
                match kind {
                    PointerKind::Motion => {
                        let e = MotionNotifyEvent {
                            response_type: MOTION_NOTIFY_EVENT,
                            detail: Motion::NORMAL,
                            sequence: 0,
                            time,
                            root: self.root,
                            event: self.target,
                            child: x11rb::NONE,
                            root_x: rx,
                            root_y: ry,
                            event_x: ex,
                            event_y: ey,
                            state,
                            same_screen: true,
                        };
                        self.send(EventMask::POINTER_MOTION | EventMask::BUTTON_MOTION, e)?;
                    }
                    PointerKind::Press(button) | PointerKind::Release(button) => {
                        let press = matches!(kind, PointerKind::Press(_));
                        let e = ButtonPressEvent {
                            response_type: if press {
                                BUTTON_PRESS_EVENT
                            } else {
                                BUTTON_RELEASE_EVENT
                            },
                            detail: button,
                            sequence: 0,
                            time,
                            root: self.root,
                            event: self.target,
                            child: x11rb::NONE,
                            root_x: rx,
                            root_y: ry,
                            event_x: ex,
                            event_y: ey,
                            state,
                            same_screen: true,
                        };
                        let mask = if press {
                            EventMask::BUTTON_PRESS
                        } else {
                            EventMask::BUTTON_RELEASE
                        };
                        self.send(mask, e)?;
                    }
                }
            }
            OverlayEvent::Key {
                pressed,
                keycode,
                state,
                time,
            } => {
                let (ox, oy) = self.origin()?;
                let (tx, ty) = self.last;
                let e = KeyPressEvent {
                    response_type: if pressed {
                        KEY_PRESS_EVENT
                    } else {
                        KEY_RELEASE_EVENT
                    },
                    detail: keycode,
                    sequence: 0,
                    time,
                    root: self.root,
                    event: self.target,
                    child: x11rb::NONE,
                    root_x: clamp_i16(ox + tx),
                    root_y: clamp_i16(oy + ty),
                    event_x: clamp_i16(tx),
                    event_y: clamp_i16(ty),
                    state: KeyButMask::from(state),
                    same_screen: true,
                };
                let mask = if pressed {
                    EventMask::KEY_PRESS
                } else {
                    EventMask::KEY_RELEASE
                };
                self.send(mask, e)?;
            }
            OverlayEvent::Leave => self.leave(0, x11rb::CURRENT_TIME)?,
            OverlayEvent::Enter | OverlayEvent::Configure(_) | OverlayEvent::Expose | OverlayEvent::Close => {
            }
        }
        Ok(())
    }

    fn enter(&mut self, tx: i32, ty: i32, state: u16, time: u32) -> Result<()> {
        self.inside = true;
        let e = self.crossing(ENTER_NOTIFY_EVENT, tx, ty, state, time)?;
        self.send(EventMask::ENTER_WINDOW, e)
    }

    fn leave(&mut self, state: u16, time: u32) -> Result<()> {
        if !self.inside {
            return Ok(());
        }
        self.inside = false;
        let (tx, ty) = self.last;
        let e = self.crossing(LEAVE_NOTIFY_EVENT, tx, ty, state, time)?;
        self.send(EventMask::LEAVE_WINDOW, e)
    }

    fn crossing(&mut self, kind: u8, tx: i32, ty: i32, state: u16, time: u32) -> Result<EnterNotifyEvent> {
        let (ox, oy) = self.origin()?;
        Ok(EnterNotifyEvent {
            response_type: kind,
            detail: NotifyDetail::NONLINEAR,
            sequence: 0,
            time,
            root: self.root,
            event: self.target,
            child: x11rb::NONE,
            root_x: clamp_i16(ox + tx),
            root_y: clamp_i16(oy + ty),
            event_x: clamp_i16(tx),
            event_y: clamp_i16(ty),
            state: KeyButMask::from(state),
            mode: NotifyMode::NORMAL,
            // Bit 0: same screen; bit 1: focus.
            same_screen_focus: 0b11,
        })
    }

    /// Target origin in root coordinates, cached.
    fn origin(&mut self) -> Result<(i32, i32)> {
        if let Some(o) = self.origin {
            return Ok(o);
        }
        let reply = self
            .conn
            .translate_coordinates(self.target, self.root, 0, 0)?
            .reply();
        let Some(t) = gone_to_none(reply)? else {
            return Err(X11Error::NoWindow(self.target));
        };
        let o = (i32::from(t.dst_x), i32::from(t.dst_y));
        self.origin = Some(o);
        Ok(o)
    }

    fn send(&self, mask: EventMask, event: impl Into<[u8; 32]>) -> Result<()> {
        self.conn.send_event(false, self.target, mask, event)?;
        self.conn.flush()?;
        Ok(())
    }
}

/// Modifier bits that count (Shift, Lock, Control, Mod1–Mod5).
const MODIFIER_BITS: u16 = 0x00FF;
/// CapsLock, NumLock (Mod2) and ScrollLock (Mod5): ignored, as the global
/// hotkeys do.
const LOCK_BITS: u16 = 0x0002 | 0x0010 | 0x0080;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HotkeyKey {
    modifiers: u16,
    keycode: u8,
    action: HotkeyAction,
}

/// Key presses on the overlay → hotkey actions (from
/// [`InputForwarder::hotkey_matcher`]). The keyboard layout is read once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HotkeyMatcher {
    keys: Vec<HotkeyKey>,
}

impl HotkeyMatcher {
    /// The action of a key press with modifier `state`, if any: exact
    /// modifiers once the lock bits are off.
    pub fn matches(&self, keycode: u8, state: u16) -> Option<HotkeyAction> {
        let state = state & MODIFIER_BITS & !LOCK_BITS;
        self.keys
            .iter()
            .find(|k| k.keycode == keycode && k.modifiers == state)
            .map(|k| k.action)
    }

    /// No hotkey to match.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

fn clamp_i16(v: i32) -> i16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapper() -> CoordinateMapper {
        // 640x360 source shown at 2x inside a 1920x1080 overlay, letterboxed.
        CoordinateMapper {
            content: Rect {
                x: 320,
                y: 180,
                width: 1280,
                height: 720,
            },
            source_size: (640, 360),
            crop_offset: (0, 30),
        }
    }

    #[test]
    fn maps_inside_content() {
        let m = mapper();
        assert_eq!(m.to_target(320, 180), Some((0, 30)));
        assert_eq!(m.to_target(321, 181), Some((0, 30)));
        assert_eq!(m.to_target(322, 182), Some((1, 31)));
        assert_eq!(m.to_target(1599, 899), Some((639, 389)));
    }

    #[test]
    fn letterbox_does_not_map() {
        let m = mapper();
        assert_eq!(m.to_target(319, 500), None);
        assert_eq!(m.to_target(1600, 500), None);
        assert_eq!(m.to_target(800, 900), None);
        assert_eq!(CoordinateMapper::default().to_target(0, 0), None);
    }

    #[test]
    fn hotkeys_match_with_locks_ignored() {
        let m = HotkeyMatcher {
            keys: vec![HotkeyKey {
                // Alt (Mod1) + Shift.
                modifiers: 0x0008 | 0x0001,
                keycode: 39,
                action: HotkeyAction::Screenshot,
            }],
        };
        assert_eq!(m.matches(39, 0x0009), Some(HotkeyAction::Screenshot));
        // NumLock and CapsLock on.
        assert_eq!(
            m.matches(39, 0x0009 | 0x0010 | 0x0002),
            Some(HotkeyAction::Screenshot)
        );
        // A button held (bit 8 and up) does not count.
        assert_eq!(m.matches(39, 0x0009 | 0x0100), Some(HotkeyAction::Screenshot));
        // Missing or extra modifiers.
        assert_eq!(m.matches(39, 0x0008), None);
        assert_eq!(m.matches(39, 0x0009 | 0x0004), None);
        assert_eq!(m.matches(40, 0x0009), None);
        assert!(HotkeyMatcher::default().is_empty());
    }

    #[test]
    fn downscaled_content_maps_back() {
        let m = CoordinateMapper {
            content: Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            source_size: (1000, 1000),
            crop_offset: (5, 5),
        };
        assert_eq!(m.to_target(50, 99), Some((505, 995)));
    }
}
