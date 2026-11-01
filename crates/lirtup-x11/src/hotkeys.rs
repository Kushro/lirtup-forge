//! Global hotkeys with `GrabKey` (`window/hotkeys.py`), on their own thread
//! and connection. Grabs every lock-mask combination (NumLock, CapsLock).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use lirtup_config::{Hotkey, HotkeyAction, Modifiers};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, GrabMode, Keycode, ModMask, Window,
    WindowClass,
};
use x11rb::protocol::{ErrorKind, Event};
use x11rb::rust_connection::RustConnection;

use crate::{Result, X11Error};

/// `XK_Escape`.
pub(crate) const XK_ESCAPE: u32 = 0xFF1B;

/// Lock modifiers ignored when matching: CapsLock, NumLock (Mod2) and
/// ScrollLock (Mod5), as 1.x.
const LOCK_MASKS: [u16; 3] = [LOCK, MOD2, MOD5];
const LOCK: u16 = 0x0002;
const MOD2: u16 = 0x0010;
const MOD5: u16 = 0x0080;

/// Modifier bits that count (Shift, Lock, Control, Mod1–Mod5).
const MODIFIER_BITS: u16 = 0x00FF;

/// One grabbed combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Grab {
    modifiers: u16,
    keycode: Keycode,
    action: HotkeyAction,
}

/// What the listener thread and the owner share.
struct Shared {
    conn: RustConnection,
    root: Window,
    /// Unmapped window used to wake the listener on `stop`.
    wake: Window,
    grabs: Mutex<Vec<Grab>>,
    stop: AtomicBool,
}

/// Running hotkey grabs. Dropping it ungrabs and stops the thread.
pub struct Hotkeys {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Hotkeys {
    /// Grab `bindings` and start the listener thread. Bindings that cannot
    /// be grabbed (unknown key, taken by another client) are returned as
    /// errors next to the running grabs; the rest still work.
    pub fn start(
        bindings: &[(HotkeyAction, Hotkey)],
    ) -> Result<(Hotkeys, Receiver<HotkeyAction>, Vec<crate::X11Error>)> {
        let (conn, screen) = RustConnection::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let wake = conn.generate_id()?;
        conn.create_window(0, wake, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new())?;
        let shared = Arc::new(Shared { conn, root, wake, grabs: Mutex::new(Vec::new()), stop: AtomicBool::new(false) });
        let errors = shared.grab_all(bindings)?;

        let (tx, rx) = crossbeam_channel::unbounded();
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("lirtup-hotkeys".into())
            .spawn(move || listen(&thread_shared, &tx))
            .map_err(|e| X11Error::Connection(format!("cannot start the hotkey thread: {e}")))?;
        Ok((Hotkeys { shared, thread: Some(thread) }, rx, errors))
    }

    /// Replace the bindings (after `ReloadConfig`).
    pub fn rebind(&self, bindings: &[(HotkeyAction, Hotkey)]) -> Result<Vec<crate::X11Error>> {
        self.shared.ungrab_all()?;
        self.shared.grab_all(bindings)
    }

    /// Ungrab and join the thread.
    pub fn stop(self) {
        // `Drop` does the work.
    }

    fn shutdown(&mut self) {
        let Some(thread) = self.thread.take() else { return };
        self.shared.stop.store(true, Ordering::SeqCst);
        // An event sent with an empty mask goes to the window's creator: us.
        let wake = ClientMessageEvent::new(32, self.shared.wake, x11rb::protocol::xproto::AtomEnum::NONE, [0u32; 5]);
        let _ = self.shared.conn.send_event(false, self.shared.wake, EventMask::NO_EVENT, wake);
        let _ = self.shared.conn.flush();
        let _ = thread.join();
        let _ = self.shared.ungrab_all();
        let _ = self.shared.conn.destroy_window(self.shared.wake);
        let _ = self.shared.conn.flush();
    }
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Shared {
    fn grab_all(&self, bindings: &[(HotkeyAction, Hotkey)]) -> Result<Vec<X11Error>> {
        let mut errors = Vec::new();
        let mut grabs = Vec::new();
        for (action, hotkey) in bindings {
            let label = hotkey_label(hotkey);
            let Some(keysym) = keysym_for(&hotkey.key) else {
                errors.push(X11Error::Hotkey(label, format!("unknown key name {:?}", hotkey.key)));
                continue;
            };
            let keycodes = keycodes_for(&self.conn, keysym)?;
            if keycodes.is_empty() {
                errors.push(X11Error::Hotkey(label, "no key on this keyboard layout produces it".into()));
                continue;
            }
            let modifiers = hotkey.modifiers.0 & MODIFIER_BITS;
            if grabs.iter().any(|g: &Grab| g.modifiers == modifiers && keycodes.contains(&g.keycode)) {
                errors.push(X11Error::Hotkey(label, "bound to two actions".into()));
                continue;
            }
            let mut taken = false;
            let mut done = Vec::new();
            'codes: for &keycode in &keycodes {
                for lock in lock_combinations() {
                    let cookie = self.conn.grab_key(
                        true,
                        self.root,
                        ModMask::from(modifiers | lock),
                        keycode,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    )?;
                    match cookie.check() {
                        Ok(()) => {}
                        Err(x11rb::errors::ReplyError::X11Error(e)) if e.error_kind == ErrorKind::Access => {
                            taken = true;
                            break 'codes;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                done.push(keycode);
            }
            if taken {
                // Leave nothing half-grabbed.
                for &keycode in keycodes.iter() {
                    for lock in lock_combinations() {
                        self.conn.ungrab_key(keycode, self.root, ModMask::from(modifiers | lock))?;
                    }
                }
                errors.push(X11Error::Hotkey(label, "already grabbed by another program".into()));
                continue;
            }
            tracing::debug!("grabbed {} for {}: keycodes {done:?}", label, action.as_str());
            grabs.extend(done.into_iter().map(|keycode| Grab { modifiers, keycode, action: *action }));
        }
        self.conn.flush()?;
        *self.grabs.lock().unwrap_or_else(PoisonError::into_inner) = grabs;
        Ok(errors)
    }

    fn ungrab_all(&self) -> Result<()> {
        let mut grabs = self.grabs.lock().unwrap_or_else(PoisonError::into_inner);
        for g in grabs.drain(..) {
            for lock in lock_combinations() {
                self.conn.ungrab_key(g.keycode, self.root, ModMask::from(g.modifiers | lock))?;
            }
        }
        self.conn.flush()?;
        Ok(())
    }
}

/// The listener thread: key presses → actions, until `stop`.
fn listen(shared: &Shared, tx: &Sender<HotkeyAction>) {
    loop {
        let event = match shared.conn.wait_for_event() {
            Ok(e) => e,
            Err(e) => {
                if !shared.stop.load(Ordering::SeqCst) {
                    tracing::error!("hotkey connection lost: {e}");
                }
                return;
            }
        };
        if shared.stop.load(Ordering::SeqCst) {
            return;
        }
        if let Event::KeyPress(e) = event {
            let grabs = shared.grabs.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(action) = match_grab(&grabs, e.detail, u16::from(e.state)) {
                tracing::debug!("hotkey {}", action.as_str());
                if tx.send(action).is_err() {
                    return;
                }
            }
        }
    }
}

/// The action of a key press: exact modifiers once the lock bits are off.
fn match_grab(grabs: &[Grab], keycode: Keycode, state: u16) -> Option<HotkeyAction> {
    let locks = LOCK_MASKS.iter().fold(0, |a, m| a | m);
    let state = state & MODIFIER_BITS & !locks;
    grabs.iter().find(|g| g.keycode == keycode && g.modifiers & !locks == state).map(|g| g.action)
}

/// Every OR of the lock masks (8 combinations, 0 included).
fn lock_combinations() -> impl Iterator<Item = u16> {
    (0u16..1 << LOCK_MASKS.len()).map(|bits| {
        LOCK_MASKS.iter().enumerate().filter(|(i, _)| bits & (1 << i) != 0).fold(0, |a, (_, m)| a | m)
    })
}

/// Keycodes whose keyboard mapping contains `keysym` (any level).
pub(crate) fn keycodes_for<C: Connection>(conn: &C, keysym: u32) -> Result<Vec<Keycode>> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let count = max - min + 1;
    let map = conn.get_keyboard_mapping(min, count)?.reply()?;
    let per = usize::from(map.keysyms_per_keycode.max(1));
    let mut out: Vec<Keycode> = map
        .keysyms
        .chunks(per)
        .enumerate()
        .filter(|(_, syms)| syms.contains(&keysym))
        .map(|(i, _)| min + i as u8)
        .collect();
    out.dedup();
    Ok(out)
}

/// `Ctrl+Alt+Shift+Super+Key`, built here so errors do not depend on the
/// `Display` of `Hotkey`.
fn hotkey_label(hotkey: &Hotkey) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for (bit, name) in
        [(Modifiers::CONTROL, "Ctrl"), (Modifiers::ALT, "Alt"), (Modifiers::SHIFT, "Shift"), (Modifiers::SUPER, "Super")]
    {
        if hotkey.modifiers.contains(bit) {
            parts.push(name);
        }
    }
    parts.push(&hotkey.key);
    parts.join("+")
}

/// The keysym of a 1.x key name (`S`, `Escape`, `Plus`, `F5`, `Up`, ...).
///
/// Letters are case-insensitive (`s` = `S`, keysym of the lowercase
/// letter); the rest are the exact names of `KEYSYM_MAP` in
/// `utils/hotkeys.py`.
pub fn keysym_for(name: &str) -> Option<u32> {
    let bytes = name.as_bytes();
    if bytes.len() == 1 {
        let c = bytes[0];
        if c.is_ascii_alphabetic() {
            return Some(u32::from(c.to_ascii_lowercase()));
        }
        if c.is_ascii_digit() {
            return Some(u32::from(c));
        }
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok())
        && (1..=12).contains(&n)
    {
        return Some(0xFFBE + n - 1);
    }
    Some(match name {
        "Left" => 0xFF51,
        "Up" => 0xFF52,
        "Right" => 0xFF53,
        "Down" => 0xFF54,
        "Insert" => 0xFF63,
        "Delete" => 0xFFFF,
        "Home" => 0xFF50,
        "End" => 0xFF57,
        "PageUp" => 0xFF55,
        "PageDown" => 0xFF56,
        "Print" => 0xFF61,
        "Pause" => 0xFF13,
        "ScrollLock" => 0xFF14,
        "LeftBracket" => 0x5B,
        "RightBracket" => 0x5D,
        "Backslash" => 0x5C,
        "Semicolon" => 0x3B,
        "Apostrophe" => 0x27,
        "Comma" => 0x2C,
        "Period" => 0x2E,
        "Slash" => 0x2F,
        "Plus" => 0x2B,
        "Minus" => 0x2D,
        "Equal" => 0x3D,
        "Grave" => 0x60,
        "Space" => 0x20,
        "Return" => 0xFF0D,
        "Backspace" => 0xFF08,
        "Tab" => 0xFF09,
        "Escape" => XK_ESCAPE,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysyms_of_the_1x_table() {
        assert_eq!(keysym_for("S"), Some(0x73));
        assert_eq!(keysym_for("s"), Some(0x73));
        assert_eq!(keysym_for("7"), Some(0x37));
        assert_eq!(keysym_for("F1"), Some(0xFFBE));
        assert_eq!(keysym_for("F12"), Some(0xFFC9));
        assert_eq!(keysym_for("F13"), None);
        assert_eq!(keysym_for("Plus"), Some(0x2B));
        assert_eq!(keysym_for("Escape"), Some(0xFF1B));
        assert_eq!(keysym_for("Up"), Some(0xFF52));
        assert_eq!(keysym_for("Nope"), None);
        assert_eq!(keysym_for(""), None);
    }

    #[test]
    fn eight_lock_combinations() {
        let mut all: Vec<u16> = lock_combinations().collect();
        all.sort_unstable();
        assert_eq!(all.len(), 8);
        assert_eq!(all[0], 0);
        assert!(all.contains(&(LOCK | MOD2 | MOD5)));
    }

    #[test]
    fn match_ignores_locks_but_not_extra_modifiers() {
        let alt_shift = Modifiers::ALT.0 | Modifiers::SHIFT.0;
        let grabs = [Grab { modifiers: alt_shift, keycode: 39, action: HotkeyAction::ToggleScaling }];
        assert_eq!(match_grab(&grabs, 39, alt_shift), Some(HotkeyAction::ToggleScaling));
        // NumLock + CapsLock on, and a mouse button held (bit 8).
        let noisy = alt_shift | MOD2 | LOCK | 0x100;
        assert_eq!(match_grab(&grabs, 39, noisy), Some(HotkeyAction::ToggleScaling));
        assert_eq!(match_grab(&grabs, 39, alt_shift | Modifiers::CONTROL.0), None);
        assert_eq!(match_grab(&grabs, 40, alt_shift), None);
    }

    #[test]
    fn label_in_1x_order() {
        let hk = Hotkey { modifiers: Modifiers::SHIFT | Modifiers::ALT | Modifiers::CONTROL, key: "Plus".into() };
        assert_eq!(hotkey_label(&hk), "Ctrl+Alt+Shift+Plus");
    }
}
