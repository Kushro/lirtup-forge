//! Cursors without libxcursor: glyphs of the core `cursor` font, and an
//! empty one to hide the pointer.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, Cursor, Window};

use crate::Result;

/// `XC_crosshair` of `<X11/cursorfont.h>`.
pub(crate) const XC_CROSSHAIR: u16 = 34;

/// A cursor from the core cursor font (`XCreateFontCursor`).
pub(crate) fn font_cursor<C: Connection>(conn: &C, glyph: u16) -> Result<Cursor> {
    let font = conn.generate_id()?;
    conn.open_font(font, b"cursor")?;
    let cursor = conn.generate_id()?;
    conn.create_glyph_cursor(cursor, font, font, glyph, glyph + 1, 0, 0, 0, 0xFFFF, 0xFFFF, 0xFFFF)?;
    conn.close_font(font)?;
    Ok(cursor)
}

/// A fully transparent 1×1 cursor.
pub(crate) fn blank_cursor<C: Connection>(conn: &C, window: Window) -> Result<Cursor> {
    let pixmap = conn.generate_id()?;
    conn.create_pixmap(1, pixmap, window, 1, 1)?;
    let cursor = conn.generate_id()?;
    // Source and mask both the (undefined, but fully masked) 1-bit pixmap:
    // clear it first so the mask is empty.
    let gc = conn.generate_id()?;
    conn.create_gc(gc, pixmap, &x11rb::protocol::xproto::CreateGCAux::new().foreground(0))?;
    conn.poly_fill_rectangle(pixmap, gc, &[x11rb::protocol::xproto::Rectangle { x: 0, y: 0, width: 1, height: 1 }])?;
    conn.free_gc(gc)?;
    conn.create_cursor(cursor, pixmap, pixmap, 0, 0, 0, 0, 0, 0, 0, 0)?;
    conn.free_pixmap(pixmap)?;
    Ok(cursor)
}
