//! «Señalar ventana»: crosshair cursor, click, window under it (`PickWindow`).

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, EventMask, GrabMode, GrabStatus, Window};
use x11rb::protocol::Event;
use x11rb::CURRENT_TIME;

use crate::error::gone_to_none;
use crate::{Result, WindowInfo, X11, X11Error};

impl X11 {
    /// Grab the pointer with a crosshair, wait for a click and return the
    /// top-level window under it. `Escape` or a right click cancel (`None`).
    /// Blocks; run it on its own thread.
    pub fn pick_window(&self) -> Result<Option<WindowInfo>> {
        let c = &self.conn;
        let cursor = crate::cursor::font_cursor(c, crate::cursor::XC_CROSSHAIR)?;
        let grab = c
            .grab_pointer(
                false,
                self.root,
                EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                x11rb::NONE,
                cursor,
                CURRENT_TIME,
            )?
            .reply()?;
        if grab.status != GrabStatus::SUCCESS {
            c.free_cursor(cursor)?;
            c.flush()?;
            return Err(X11Error::Request(format!("cannot grab the pointer ({:?})", grab.status)));
        }
        // The keyboard grab only serves Escape; failing it is fine.
        let _ = c.grab_keyboard(false, self.root, CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC)?.reply();
        let escape = crate::hotkeys::keycodes_for(c, crate::hotkeys::XK_ESCAPE).unwrap_or_default();

        let picked = loop {
            match c.wait_for_event() {
                Ok(Event::ButtonPress(e)) if e.detail == 1 => break Ok(e.child),
                Ok(Event::ButtonPress(_)) => break Ok(x11rb::NONE),
                Ok(Event::KeyPress(e)) if escape.contains(&e.detail) => break Ok(x11rb::NONE),
                Ok(_) => {}
                Err(e) => break Err(X11Error::from(e)),
            }
        };
        let _ = c.ungrab_pointer(CURRENT_TIME);
        let _ = c.ungrab_keyboard(CURRENT_TIME);
        let _ = c.free_cursor(cursor);
        c.flush()?;

        let frame = picked?;
        if frame == x11rb::NONE || frame == self.root {
            return Ok(None);
        }
        let client = self.client_window(frame)?;
        self.window_info(client)
    }

    /// The client window inside a window-manager frame: the frame itself or
    /// the first descendant with `WM_STATE` (`XmuClientWindow`).
    pub(crate) fn client_window(&self, frame: Window) -> Result<Window> {
        let mut queue = std::collections::VecDeque::from([frame]);
        while let Some(win) = queue.pop_front() {
            let state = self.conn.get_property(false, win, self.atoms.WM_STATE, AtomEnum::ANY, 0, 0)?.reply();
            if gone_to_none(state)?.is_some_and(|r| r.type_ != u32::from(AtomEnum::NONE)) {
                return Ok(win);
            }
            if let Some(tree) = gone_to_none(self.conn.query_tree(win)?.reply())? {
                queue.extend(tree.children);
            }
        }
        Ok(frame)
    }
}
