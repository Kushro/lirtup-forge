//! «Señalar ventana»: crosshair cursor, click, window under it (`PickWindow`).

use crate::{Result, WindowInfo, X11};

impl X11 {
    /// Grab the pointer with a crosshair, wait for a click and return the
    /// top-level window under it. `Escape` or a right click cancel (`None`).
    /// Blocks; run it on its own thread.
    pub fn pick_window(&self) -> Result<Option<WindowInfo>> {
        todo!("front X")
    }
}
