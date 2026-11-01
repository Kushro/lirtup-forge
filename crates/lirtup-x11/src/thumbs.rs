//! Window thumbnails for the GUI (6.4): XComposite `NameWindowPixmap` +
//! `GetImage`, scaled down on the CPU. The GUI's `thumbs.rs` runs this on its
//! own thread and encodes the result.

use x11rb::connection::{Connection as _, RequestConnection as _};
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat, MapState, Window};

use crate::error::gone_to_none;
use crate::{Result, RgbaImage, X11, X11Error};

impl X11 {
    /// Redirect windows for thumbnails (`CompositeRedirectWindow`, automatic
    /// mode). Call once per connection.
    ///
    /// Redirects the top-level windows (`RedirectSubwindows` on the root):
    /// covered windows then still have their contents. With a compositing
    /// window manager they already are; the extra automatic redirect is
    /// harmless and ends with this connection.
    pub fn enable_thumbnails(&self) -> Result<()> {
        if self.thumbnails.get() {
            return Ok(());
        }
        if self.conn.extension_information(x11rb::protocol::composite::X11_EXTENSION_NAME)?.is_none() {
            return Err(X11Error::MissingExtension("Composite"));
        }
        self.conn.composite_query_version(0, 4)?.reply()?;
        self.conn.composite_redirect_subwindows(self.root, Redirect::AUTOMATIC)?.check()?;
        self.thumbnails.set(true);
        Ok(())
    }

    /// A thumbnail of `xid` at most `max_width` pixels wide (aspect kept).
    /// `None` if the window is unmapped or gone.
    pub fn thumbnail(&self, xid: u32, max_width: u32) -> Result<Option<RgbaImage>> {
        let c = &self.conn;
        let Some(attrs) = gone_to_none(c.get_window_attributes(xid)?.reply())? else { return Ok(None) };
        if attrs.map_state != MapState::VIEWABLE {
            return Ok(None);
        }
        let Some(geometry) = gone_to_none(c.get_geometry(xid)?.reply())? else { return Ok(None) };
        let (w, h) = (geometry.width, geometry.height);
        if w == 0 || h == 0 {
            return Ok(None);
        }

        let mut pixels = None;
        if self.thumbnails.get() {
            pixels = self.read_through_pixmap(xid, w, h)?;
        }
        if pixels.is_none() {
            pixels = match c.get_image(ImageFormat::Z_PIXMAP, xid, 0, 0, w, h, !0)?.reply() {
                Ok(reply) => Some(reply.data),
                // Unmapped meanwhile, or off screen without a compositor.
                Err(e) if crate::error::is_gone(&e) || is_match(&e) => return Ok(None),
                Err(e) => return Err(e.into()),
            };
        }
        let Some(data) = pixels else { return Ok(None) };
        let Some(full) = bgrx_to_rgba(&data, u32::from(w), u32::from(h)) else {
            tracing::debug!("thumbnail of 0x{xid:x}: not a 32-bpp window");
            return Ok(None);
        };
        if max_width == 0 || full.width <= max_width {
            return Ok(Some(full));
        }
        let (tw, th) = crate::window::fit_inside(full.width, full.height, max_width, u32::MAX);
        Ok(Some(crate::image::resize(&full, tw, th)))
    }

    /// Read the window from the pixmap of its redirected top-level ancestor
    /// (the window manager's frame). `None` when that fails.
    fn read_through_pixmap(&self, xid: Window, w: u16, h: u16) -> Result<Option<Vec<u8>>> {
        let c = &self.conn;
        let Some(top) = self.top_level(xid)? else { return Ok(None) };
        let Some(top_geometry) = gone_to_none(c.get_geometry(top)?.reply())? else { return Ok(None) };
        let Some(offset) = gone_to_none(c.translate_coordinates(xid, top, 0, 0)?.reply())? else {
            return Ok(None);
        };
        let pixmap = c.generate_id()?;
        if c.composite_name_window_pixmap(top, pixmap)?.check().is_err() {
            return Ok(None);
        }
        // The pixmap includes the frame's border; window coordinates start
        // inside it.
        let border = top_geometry.border_width as i16;
        let image =
            c.get_image(ImageFormat::Z_PIXMAP, pixmap, offset.dst_x + border, offset.dst_y + border, w, h, !0)?.reply();
        c.free_pixmap(pixmap)?;
        Ok(image.ok().map(|r| r.data))
    }

    /// The child of the root that contains `xid`.
    fn top_level(&self, mut xid: Window) -> Result<Option<Window>> {
        loop {
            let Some(tree) = gone_to_none(self.conn.query_tree(xid)?.reply())? else { return Ok(None) };
            if tree.parent == self.root || tree.parent == x11rb::NONE {
                return Ok(Some(xid));
            }
            xid = tree.parent;
        }
    }
}

fn is_match(e: &x11rb::errors::ReplyError) -> bool {
    matches!(e, x11rb::errors::ReplyError::X11Error(x) if x.error_kind == x11rb::protocol::ErrorKind::Match)
}

/// `GetImage` Z-pixmap data of a 24/32-bit window (B, G, R, X) to opaque
/// RGBA. `None` for other layouts.
pub(crate) fn bgrx_to_rgba(data: &[u8], w: u32, h: u32) -> Option<RgbaImage> {
    let len = w as usize * h as usize * 4;
    if data.len() != len {
        return None;
    }
    let mut out = Vec::with_capacity(len);
    for px in data.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    Some(RgbaImage { width: w, height: h, data: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgrx_swaps_and_fills_alpha() {
        let img = bgrx_to_rgba(&[1, 2, 3, 0, 4, 5, 6, 7], 2, 1).unwrap();
        assert_eq!(img.data, vec![3, 2, 1, 255, 6, 5, 4, 255]);
        assert!(bgrx_to_rgba(&[0; 6], 2, 1).is_none());
    }
}
