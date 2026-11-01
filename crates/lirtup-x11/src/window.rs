//! Window list and properties (`window/info.py`, `window/acquisition.py`).

use serde::{Deserialize, Serialize};

use x11rb::CURRENT_TIME;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask, InputFocus, MapState,
    StackMode, Window,
};

use crate::error::gone_to_none;
use crate::{Result, RgbaImage, X11};

/// A rectangle in root coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// `WM_CLASS`: instance and class names.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct WindowClass {
    /// Instance name (first string).
    pub instance: String,
    /// Class name (second string).
    pub class: String,
}

/// One top-level application window.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowInfo {
    /// XID.
    pub xid: u32,
    /// Title (`_NET_WM_NAME`, then `WM_NAME`), UTF-8.
    pub title: String,
    /// Geometry of the client area, in root coordinates.
    pub rect: Rect,
    /// `_NET_WM_PID`.
    pub pid: Option<u32>,
    /// `WM_CLASS`.
    pub class: Option<WindowClass>,
}

impl WindowInfo {
    /// Width.
    pub fn width(&self) -> u32 {
        self.rect.width
    }

    /// Height.
    pub fn height(&self) -> u32 {
        self.rect.height
    }

    /// A window of LirtUp Forge by its `WM_CLASS`: `lirtup` (the overlay,
    /// `lirtup`/`LirtUp`) or `lirtup-…` (the GUI, named after its binary).
    pub fn is_lirtup_class(&self) -> bool {
        let ours = |name: &str| {
            let name = name.to_ascii_lowercase();
            name == "lirtup" || name.starts_with("lirtup-") || name.starts_with("lirtup_")
        };
        self.class
            .as_ref()
            .is_some_and(|c| ours(&c.instance) || ours(&c.class))
    }
}

impl X11 {
    /// Viewable application windows (the `is_application_window` filter of
    /// 1.x: normal type, not XWayland helpers, not our own overlay), in
    /// stacking order.
    pub fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        let candidates = match self.window_list_property(self.atoms._NET_CLIENT_LIST_STACKING)? {
            Some(list) if !list.is_empty() => list,
            _ => match self.window_list_property(self.atoms._NET_CLIENT_LIST)? {
                Some(list) if !list.is_empty() => list,
                _ => {
                    tracing::debug!("no _NET_CLIENT_LIST; walking the window tree");
                    self.all_windows()?
                }
            },
        };
        let mut out = Vec::new();
        for win in candidates {
            if !self.is_application_window(win)? {
                continue;
            }
            match self.window_info(win)? {
                Some(info) if !info.title.is_empty() => out.push(info),
                _ => {}
            }
        }
        Ok(out)
    }

    /// Info of one window; `None` if it is gone.
    pub fn window_info(&self, xid: u32) -> Result<Option<WindowInfo>> {
        // Send every request first, then collect: one round trip.
        let c = &self.conn;
        let geometry = c.get_geometry(xid)?;
        let origin = c.translate_coordinates(xid, self.root, 0, 0)?;
        let net_name = c.get_property(
            false,
            xid,
            self.atoms._NET_WM_NAME,
            self.atoms.UTF8_STRING,
            0,
            1024,
        )?;
        let wm_name = c.get_property(false, xid, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 1024)?;
        let class = c.get_property(false, xid, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)?;
        let pid = c.get_property(false, xid, self.atoms._NET_WM_PID, AtomEnum::CARDINAL, 0, 1)?;

        let Some(geometry) = gone_to_none(geometry.reply())? else {
            return Ok(None);
        };
        let Some(origin) = gone_to_none(origin.reply())? else {
            return Ok(None);
        };
        let net_name = gone_to_none(net_name.reply())?;
        let wm_name = gone_to_none(wm_name.reply())?;
        let class = gone_to_none(class.reply())?;
        let pid = gone_to_none(pid.reply())?;

        let title = net_name
            .filter(|r| !r.value.is_empty())
            .map(|r| String::from_utf8_lossy(&r.value).trim().to_owned())
            .or_else(|| {
                wm_name.filter(|r| !r.value.is_empty()).map(|r| {
                    if r.type_ == self.atoms.UTF8_STRING {
                        String::from_utf8_lossy(&r.value).trim().to_owned()
                    } else {
                        latin1(&r.value).trim().to_owned()
                    }
                })
            })
            .unwrap_or_default();
        let class = class.and_then(|r| parse_wm_class(&r.value));
        let pid = pid
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .filter(|&p| p != 0);

        Ok(Some(WindowInfo {
            xid,
            title,
            rect: Rect {
                x: i32::from(origin.dst_x),
                y: i32::from(origin.dst_y),
                width: u32::from(geometry.width),
                height: u32::from(geometry.height),
            },
            pid,
            class,
        }))
    }

    /// The focused window (`_NET_ACTIVE_WINDOW`).
    pub fn active_window(&self) -> Result<Option<WindowInfo>> {
        let reply = self
            .conn
            .get_property(
                false,
                self.root,
                self.atoms._NET_ACTIVE_WINDOW,
                AtomEnum::WINDOW,
                0,
                1,
            )?
            .reply()?;
        match reply.value32().and_then(|mut v| v.next()) {
            Some(win) if win != 0 && win != self.root => self.window_info(win),
            _ => Ok(None),
        }
    }

    /// `_NET_WM_ICON`, the entry closest to `size` pixels.
    pub fn window_icon(&self, xid: u32, size: u32) -> Result<Option<RgbaImage>> {
        let reply = self
            .conn
            .get_property(
                false,
                xid,
                self.atoms._NET_WM_ICON,
                AtomEnum::CARDINAL,
                0,
                1024 * 1024,
            )?
            .reply();
        let Some(reply) = gone_to_none(reply)? else {
            return Ok(None);
        };
        let Some(values) = reply.value32() else {
            return Ok(None);
        };
        let values: Vec<u32> = values.collect();
        let Some(icon) = pick_icon(&values, size) else {
            return Ok(None);
        };
        if size == 0
            || (icon.width <= size && icon.height <= size && (icon.width == size || icon.height == size))
        {
            return Ok(Some(icon));
        }
        let (w, h) = fit_inside(icon.width, icon.height, size, size);
        Ok(Some(crate::image::resize(&icon, w, h)))
    }

    /// Raise and focus a window (`_NET_ACTIVE_WINDOW` client message).
    pub fn activate_window(&self, xid: u32) -> Result<()> {
        if self.wm_supports(self.atoms._NET_ACTIVE_WINDOW)? {
            // Source indication 2 = pager: window managers honor it without
            // focus-stealing prevention.
            let event =
                ClientMessageEvent::new(32, xid, self.atoms._NET_ACTIVE_WINDOW, [2, CURRENT_TIME, 0, 0, 0]);
            self.conn.send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )?;
        } else {
            self.conn
                .configure_window(xid, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
            self.conn.set_input_focus(InputFocus::PARENT, xid, CURRENT_TIME)?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Whether a window still exists and is viewable.
    pub fn is_viewable(&self, xid: u32) -> Result<bool> {
        let attrs = gone_to_none(self.conn.get_window_attributes(xid)?.reply())?;
        Ok(attrs.is_some_and(|a| a.map_state == MapState::VIEWABLE))
    }
}

// -------------------------------------------------------------------------
// Helpers shared with acquire, pick and thumbs
// -------------------------------------------------------------------------

impl X11 {
    /// Whether `window` is LirtUp Forge itself (an overlay, marked with
    /// `_LIRTUP_OVERLAY`, or the GUI): never a target to scale.
    pub fn is_lirtup_window(&self, window: &WindowInfo) -> Result<bool> {
        if window.is_lirtup_class() {
            return Ok(true);
        }
        let marker = self
            .conn
            .get_property(false, window.xid, self.atoms._LIRTUP_OVERLAY, AtomEnum::ANY, 0, 1)?;
        Ok(gone_to_none(marker.reply())?.is_some_and(|m| m.type_ != u32::from(AtomEnum::NONE)))
    }

    /// A `WINDOW[]` property of the root (`_NET_CLIENT_LIST`, ...).
    fn window_list_property(&self, atom: u32) -> Result<Option<Vec<Window>>> {
        let reply = self
            .conn
            .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 16 * 1024)?
            .reply()?;
        Ok(reply.value32().map(|v| v.collect()))
    }

    /// Every window under the root, depth first (no window manager).
    pub(crate) fn all_windows(&self) -> Result<Vec<Window>> {
        let mut out = Vec::new();
        let mut stack = vec![self.root];
        while let Some(win) = stack.pop() {
            out.push(win);
            if let Some(tree) = gone_to_none(self.conn.query_tree(win)?.reply())? {
                stack.extend(tree.children.iter().rev());
            }
        }
        Ok(out)
    }

    /// `_NET_WM_PID` of a window.
    pub(crate) fn window_pid(&self, xid: u32) -> Result<Option<u32>> {
        let reply = self
            .conn
            .get_property(false, xid, self.atoms._NET_WM_PID, AtomEnum::CARDINAL, 0, 1)?
            .reply();
        Ok(gone_to_none(reply)?.and_then(|r| r.value32().and_then(|mut v| v.next())))
    }

    /// `WM_CLASS` of a window.
    pub(crate) fn window_class(&self, xid: u32) -> Result<Option<WindowClass>> {
        let reply = self
            .conn
            .get_property(false, xid, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)?
            .reply();
        Ok(gone_to_none(reply)?.and_then(|r| parse_wm_class(&r.value)))
    }

    /// Whether the window manager lists `atom` in `_NET_SUPPORTED`.
    pub(crate) fn wm_supports(&self, atom: u32) -> Result<bool> {
        let reply = self
            .conn
            .get_property(
                false,
                self.root,
                self.atoms._NET_SUPPORTED,
                AtomEnum::ATOM,
                0,
                4096,
            )?
            .reply()?;
        Ok(reply.value32().is_some_and(|mut v| v.any(|a| a == atom)))
    }

    /// The 1.x `is_application_window` heuristic, plus "not our overlay".
    fn is_application_window(&self, win: Window) -> Result<bool> {
        const MIN_SIDE: u16 = 200;
        let c = &self.conn;
        let attrs = c.get_window_attributes(win)?;
        let geometry = c.get_geometry(win)?;
        let types = c.get_property(false, win, self.atoms._NET_WM_WINDOW_TYPE, AtomEnum::ATOM, 0, 16)?;
        let marker = c.get_property(false, win, self.atoms._LIRTUP_OVERLAY, AtomEnum::ANY, 0, 1)?;

        let Some(attrs) = gone_to_none(attrs.reply())? else {
            return Ok(false);
        };
        let Some(geometry) = gone_to_none(geometry.reply())? else {
            return Ok(false);
        };
        let types = gone_to_none(types.reply())?;
        let marker = gone_to_none(marker.reply())?;

        if attrs.map_state != MapState::VIEWABLE || attrs.override_redirect {
            return Ok(false);
        }
        if marker.is_some_and(|m| m.type_ != u32::from(AtomEnum::NONE)) {
            return Ok(false);
        }
        let (w, h) = (geometry.width, geometry.height);
        if w < MIN_SIDE || h < MIN_SIDE {
            return Ok(false);
        }

        let a = &self.atoms;
        let excluded = [
            a._NET_WM_WINDOW_TYPE_DESKTOP,
            a._NET_WM_WINDOW_TYPE_DOCK,
            a._NET_WM_WINDOW_TYPE_TOOLBAR,
            a._NET_WM_WINDOW_TYPE_MENU,
            a._NET_WM_WINDOW_TYPE_UTILITY,
            a._NET_WM_WINDOW_TYPE_SPLASH,
            a._NET_WM_WINDOW_TYPE_DIALOG,
            a._NET_WM_WINDOW_TYPE_POPUP_MENU,
        ];
        let types: Vec<u32> = types
            .and_then(|t| t.value32().map(|v| v.collect()))
            .unwrap_or_default();
        if !types.is_empty() {
            if types.iter().any(|t| excluded.contains(t)) || !types.contains(&a._NET_WM_WINDOW_TYPE_NORMAL) {
                return Ok(false);
            }
        } else if let Some(class) = self.window_class(win)?
            && (class.instance.to_lowercase().contains("xwayland")
                || class.class.to_lowercase().contains("xwayland"))
        {
            return Ok(false);
        }

        // Small XWayland helper windows ("xwaylandvideobridge", ...).
        if w < 300
            && h < 300
            && let Some(info) = self.window_info(win)?
        {
            let title = info.title.to_lowercase();
            if title.contains("wayland") {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// `WM_CLASS` is two NUL-terminated Latin-1 strings: instance, class.
fn parse_wm_class(raw: &[u8]) -> Option<WindowClass> {
    let mut parts = raw.split(|&b| b == 0);
    let instance = parts.next()?;
    let class = parts.next()?;
    Some(WindowClass {
        instance: latin1(instance),
        class: latin1(class),
    })
}

fn latin1(raw: &[u8]) -> String {
    raw.iter().map(|&b| char::from(b)).collect()
}

/// Pick the `_NET_WM_ICON` entry for `size`: the smallest one at least
/// `size` on both sides, else the largest smaller one (the 1.x rule).
/// The property is `width, height, width*height ARGB pixels`, repeated.
pub(crate) fn pick_icon(values: &[u32], size: u32) -> Option<RgbaImage> {
    let mut best: Option<(usize, u32, u32)> = None; // (offset of pixels, w, h)
    let mut best_is_big = false;
    let mut i = 0usize;
    while i + 2 <= values.len() {
        let (w, h) = (values[i], values[i + 1]);
        let start = i + 2;
        let Some(len) = (w as usize).checked_mul(h as usize) else {
            break;
        };
        if start + len > values.len() {
            break;
        }
        i = start + len;
        if w == 0 || h == 0 {
            continue;
        }
        let area = u64::from(w) * u64::from(h);
        let best_area = best.map_or(0, |(_, bw, bh)| u64::from(bw) * u64::from(bh));
        if w >= size && h >= size {
            if !best_is_big || area < best_area {
                best = Some((start, w, h));
                best_is_big = true;
            }
        } else if !best_is_big && area > best_area {
            best = Some((start, w, h));
        }
    }
    let (start, w, h) = best?;
    let pixels = &values[start..start + (w as usize) * (h as usize)];
    let mut data = Vec::with_capacity(pixels.len() * 4);
    for &argb in pixels {
        let [b, g, r, a] = argb.to_le_bytes();
        data.extend_from_slice(&[r, g, b, a]);
    }
    Some(RgbaImage {
        width: w,
        height: h,
        data,
    })
}

/// Largest `(w, h)` with the aspect of `src_w × src_h` that fits in
/// `max_w × max_h` (at least 1×1).
pub(crate) fn fit_inside(src_w: u32, src_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if src_w == 0 || src_h == 0 {
        return (max_w.max(1), max_h.max(1));
    }
    let scale = f64::min(
        f64::from(max_w) / f64::from(src_w),
        f64::from(max_h) / f64::from(src_h),
    );
    let w = (f64::from(src_w) * scale).round().max(1.0) as u32;
    let h = (f64::from(src_h) * scale).round().max(1.0) as u32;
    (w.min(max_w.max(1)), h.min(max_h.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wm_class_splits_instance_and_class() {
        let c = parse_wm_class(b"steam_app_123\0Steam\0").unwrap();
        assert_eq!(c.instance, "steam_app_123");
        assert_eq!(c.class, "Steam");
        assert!(parse_wm_class(b"only").is_none());
    }

    #[test]
    fn icon_picks_smallest_big_enough() {
        // 2x2 red, then 4x4 green, then 1x1 blue.
        let mut v = vec![2, 2];
        v.extend([0xFFFF_0000; 4]);
        v.extend([4, 4]);
        v.extend([0xFF00_FF00; 16]);
        v.extend([1, 1, 0xFF00_00FF]);
        let icon = pick_icon(&v, 3).unwrap();
        assert_eq!((icon.width, icon.height), (4, 4));
        assert_eq!(&icon.data[..4], &[0, 255, 0, 255]);
        let icon = pick_icon(&v, 2).unwrap();
        assert_eq!((icon.width, icon.height), (2, 2));
        assert_eq!(&icon.data[..4], &[255, 0, 0, 255]);
        // Nothing big enough: the largest smaller one.
        let icon = pick_icon(&v, 64).unwrap();
        assert_eq!((icon.width, icon.height), (4, 4));
    }

    #[test]
    fn icon_stops_on_truncated_data() {
        assert!(pick_icon(&[16, 16, 1, 2, 3], 16).is_none());
        assert!(pick_icon(&[], 16).is_none());
    }

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(fit_inside(1920, 1080, 320, 320), (320, 180));
        assert_eq!(fit_inside(100, 400, 32, 32), (8, 32));
        assert_eq!(fit_inside(1, 1000, 32, 32), (1, 32));
    }
}
