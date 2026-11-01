//! A connection for window queries (`window/connection.py`, `AtomCache`).

use crate::Result;

/// A connection to the X server with its atom cache.
///
/// `Send`; one per thread. Methods are in [`crate::window`],
/// [`crate::monitor`], [`crate::pick`], [`crate::acquire`].
pub struct X11 {
    _private: (),
}

impl X11 {
    /// Connect to `$DISPLAY`.
    pub fn connect() -> Result<X11> {
        todo!("front X")
    }

    /// Root window of the default screen.
    pub fn root(&self) -> u32 {
        todo!("front X")
    }

    /// Whether the session is XWayland (for Diagnostics and the overlay
    /// quirks).
    pub fn is_xwayland(&self) -> bool {
        todo!("front X")
    }
}
