//! Global hotkeys with `GrabKey` (`window/hotkeys.py`), on their own thread
//! and connection. Grabs every lock-mask combination (NumLock, CapsLock).

use crossbeam_channel::Receiver;
use lirtup_config::{Hotkey, HotkeyAction};

use crate::Result;

/// Running hotkey grabs. Dropping it ungrabs and stops the thread.
pub struct Hotkeys {
    _private: (),
}

impl Hotkeys {
    /// Grab `bindings` and start the listener thread. Bindings that cannot
    /// be grabbed (unknown key, taken by another client) are returned as
    /// errors next to the running grabs; the rest still work.
    pub fn start(
        bindings: &[(HotkeyAction, Hotkey)],
    ) -> Result<(Hotkeys, Receiver<HotkeyAction>, Vec<crate::X11Error>)> {
        todo!("front X")
    }

    /// Replace the bindings (after `ReloadConfig`).
    pub fn rebind(&self, bindings: &[(HotkeyAction, Hotkey)]) -> Result<Vec<crate::X11Error>> {
        todo!("front X")
    }

    /// Ungrab and join the thread.
    pub fn stop(self) {
        todo!("front X")
    }
}

/// The keysym of a 1.x key name (`S`, `Escape`, `Plus`, `F5`, `Up`, ...).
pub fn keysym_for(name: &str) -> Option<u32> {
    todo!("front X: port the keysym table of utils/hotkeys.py")
}
