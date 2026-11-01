//! Monitors (RandR), for the `monitor:` option and `ListDevices`.

use serde::{Deserialize, Serialize};

use x11rb::connection::Connection as _;
use x11rb::protocol::randr::{self, ConnectionExt as _, ModeFlag};
use x11rb::protocol::xproto::ConnectionExt as _;

use crate::{Rect, Result, X11};

/// One monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// Index in the list (what `monitor: 1` means).
    pub index: u32,
    /// RandR output name (`DP-1`, ...).
    pub name: String,
    /// Geometry in root coordinates.
    pub rect: Rect,
    /// Primary output.
    pub primary: bool,
    /// Refresh rate of the current mode, if known (auto multiplier).
    pub refresh_hz: Option<f32>,
}

impl X11 {
    /// Active monitors, primary first.
    ///
    /// RandR 1.5 `GetMonitors`; without it, one monitor covering the root
    /// window (named `default`).
    pub fn monitors(&self) -> Result<Vec<MonitorInfo>> {
        let c = &self.conn;
        let randr_ok = c
            .randr_query_version(1, 5)
            .ok()
            .and_then(|v| v.reply().ok())
            .is_some_and(|v| (v.major_version, v.minor_version) >= (1, 5));
        let mut out = Vec::new();
        if randr_ok {
            let resources = c.randr_get_screen_resources_current(self.root)?.reply()?;
            let monitors = c.randr_get_monitors(self.root, true)?.reply()?;
            for m in monitors.monitors {
                let name = c.get_atom_name(m.name)?.reply()?;
                let refresh_hz = m.outputs.iter().find_map(|&output| {
                    let info = c.randr_get_output_info(output, resources.config_timestamp).ok()?.reply().ok()?;
                    if info.crtc == 0 {
                        return None;
                    }
                    let crtc = c.randr_get_crtc_info(info.crtc, resources.config_timestamp).ok()?.reply().ok()?;
                    resources.modes.iter().find(|mode| mode.id == crtc.mode).and_then(refresh_rate)
                });
                out.push(MonitorInfo {
                    index: 0,
                    name: String::from_utf8_lossy(&name.name).into_owned(),
                    rect: Rect { x: m.x.into(), y: m.y.into(), width: m.width.into(), height: m.height.into() },
                    primary: m.primary,
                    refresh_hz,
                });
            }
        }
        if out.is_empty() {
            let screen = &c.setup().roots[self.screen];
            out.push(MonitorInfo {
                index: 0,
                name: "default".into(),
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: screen.width_in_pixels.into(),
                    height: screen.height_in_pixels.into(),
                },
                primary: true,
                refresh_hz: None,
            });
        }
        sort_monitors(&mut out);
        Ok(out)
    }

    /// Resolve `monitor:` (`primary`, `all`, a name or an index) to the area
    /// to cover; `all` is the bounding box of every monitor.
    ///
    /// The refresh rate of `all` is the lowest one (frame generation must not
    /// outrun any screen). An unknown name or index falls back to the primary
    /// monitor with a warning, as 1.x did after validation.
    pub fn monitor_area(&self, selector: &lirtup_config::MonitorSelector) -> Result<(Rect, Option<f32>)> {
        Ok(select_area(&self.monitors()?, selector))
    }
}

/// Primary first, then left to right, top to bottom; renumber.
fn sort_monitors(list: &mut [MonitorInfo]) {
    list.sort_by_key(|m| (!m.primary, m.rect.x, m.rect.y));
    for (i, m) in list.iter_mut().enumerate() {
        m.index = i as u32;
    }
}

/// `dot_clock / (htotal × vtotal)`, corrected for interlace and double scan.
fn refresh_rate(mode: &randr::ModeInfo) -> Option<f32> {
    if mode.htotal == 0 || mode.vtotal == 0 {
        return None;
    }
    let mut vtotal = f64::from(mode.vtotal);
    if mode.mode_flags & ModeFlag::DOUBLE_SCAN != ModeFlag::from(0u32) {
        vtotal *= 2.0;
    }
    if mode.mode_flags & ModeFlag::INTERLACE != ModeFlag::from(0u32) {
        vtotal /= 2.0;
    }
    Some((f64::from(mode.dot_clock) / (f64::from(mode.htotal) * vtotal)) as f32)
}

fn select_area(monitors: &[MonitorInfo], selector: &lirtup_config::MonitorSelector) -> (Rect, Option<f32>) {
    use lirtup_config::MonitorSelector;
    let primary = || monitors.iter().find(|m| m.primary).or(monitors.first());
    let found = match selector {
        MonitorSelector::Index(i) => monitors.iter().find(|m| m.index == *i),
        MonitorSelector::Name(name) if name.eq_ignore_ascii_case("primary") => primary(),
        MonitorSelector::Name(name) if name.eq_ignore_ascii_case("all") => {
            let x0 = monitors.iter().map(|m| m.rect.x).min().unwrap_or(0);
            let y0 = monitors.iter().map(|m| m.rect.y).min().unwrap_or(0);
            let x1 = monitors.iter().map(|m| m.rect.x + m.rect.width as i32).max().unwrap_or(0);
            let y1 = monitors.iter().map(|m| m.rect.y + m.rect.height as i32).max().unwrap_or(0);
            let hz = monitors.iter().filter_map(|m| m.refresh_hz).reduce(f32::min);
            return (Rect { x: x0, y: y0, width: (x1 - x0) as u32, height: (y1 - y0) as u32 }, hz);
        }
        MonitorSelector::Name(name) => monitors
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
            .or_else(|| name.trim().parse::<u32>().ok().and_then(|i| monitors.iter().find(|m| m.index == i))),
    };
    let chosen = found.or_else(|| {
        tracing::warn!("monitor {selector:?} not found; using the primary one");
        primary()
    });
    chosen.map_or((Rect::default(), None), |m| (m.rect, m.refresh_hz))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lirtup_config::MonitorSelector;

    fn mon(name: &str, x: i32, primary: bool, hz: f32) -> MonitorInfo {
        MonitorInfo {
            index: 0,
            name: name.into(),
            rect: Rect { x, y: 0, width: 1920, height: 1080 },
            primary,
            refresh_hz: Some(hz),
        }
    }

    fn sample() -> Vec<MonitorInfo> {
        let mut list = vec![mon("HDMI-1", 0, false, 60.0), mon("DP-1", 1920, true, 144.0)];
        sort_monitors(&mut list);
        list
    }

    #[test]
    fn primary_comes_first() {
        let list = sample();
        assert_eq!(list[0].name, "DP-1");
        assert_eq!(list[0].index, 0);
        assert_eq!(list[1].index, 1);
    }

    #[test]
    fn selectors() {
        let list = sample();
        let dp = (Rect { x: 1920, y: 0, width: 1920, height: 1080 }, Some(144.0));
        assert_eq!(select_area(&list, &MonitorSelector::Name("primary".into())), dp);
        assert_eq!(select_area(&list, &MonitorSelector::Name("dp-1".into())), dp);
        assert_eq!(select_area(&list, &MonitorSelector::Index(0)), dp);
        assert_eq!(select_area(&list, &MonitorSelector::Name("1".into())).0.x, 0);
        assert_eq!(select_area(&list, &MonitorSelector::Name("nope".into())), dp);
        assert_eq!(
            select_area(&list, &MonitorSelector::Name("all".into())),
            (Rect { x: 0, y: 0, width: 3840, height: 1080 }, Some(60.0))
        );
    }

    #[test]
    fn refresh_from_mode_timings() {
        let mode = randr::ModeInfo {
            id: 1,
            width: 1920,
            height: 1080,
            dot_clock: 148_500_000,
            hsync_start: 0,
            hsync_end: 0,
            htotal: 2200,
            hskew: 0,
            vsync_start: 0,
            vsync_end: 0,
            vtotal: 1125,
            name_len: 0,
            mode_flags: ModeFlag::from(0u32),
        };
        assert!((refresh_rate(&mode).unwrap() - 60.0).abs() < 0.01);
    }
}
