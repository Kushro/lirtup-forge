//! Window thumbnails for the GUI (6.4): XComposite `NameWindowPixmap` +
//! `GetImage`, scaled down on the CPU. The GUI's `thumbs.rs` runs this on its
//! own thread and encodes the result.

use crate::{Result, RgbaImage, X11};

impl X11 {
    /// Redirect windows for thumbnails (`CompositeRedirectWindow`, automatic
    /// mode). Call once per connection.
    pub fn enable_thumbnails(&self) -> Result<()> {
        todo!("front X")
    }

    /// A thumbnail of `xid` at most `max_width` pixels wide (aspect kept).
    /// `None` if the window is unmapped or gone.
    pub fn thumbnail(&self, xid: u32, max_width: u32) -> Result<Option<RgbaImage>> {
        todo!("front X")
    }
}
