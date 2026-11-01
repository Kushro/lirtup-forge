//! «Ventanas abiertas» (6.1): the header with the count and the quick
//! filter, the search field (Ctrl+F), the grid of `WindowTile`s and the tile
//! menu. Arrow keys, Home and End move between tiles; Enter and the Menu key
//! are the tile's own (`WindowTile`).

use dioxus::prelude::*;
use lirtup_config::gui::TileAspect;
use lirtup_proto::{SessionKind, Target};
use lirtup_x11::WindowInfo;

use crate::app::{FOCUS_FILTER, use_dispatch, use_store};
use crate::components::{Connected, Menu, MenuItem, TextField, TileState, WindowTile};
use crate::route::ProfileRef;
use crate::state::{Action, Dialog, SessionState};
use crate::thumbs::Thumbs;

/// Smallest tile width with automatic columns (the design's
/// `minmax(240px, 1fr)`).
const TILE_MIN: f64 = 240.0;
/// Gap between tiles.
const GAP: f64 = 16.0;

/// The last Ctrl+F the search field took (so a new one focuses it again,
/// also when Ctrl+F brought the person here from another page).
static FOCUS_DONE: GlobalSignal<u64> = Signal::global(|| 0);

/// The quick filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuickFilter {
    /// Todas.
    #[default]
    All,
    /// Con perfil.
    WithProfile,
    /// Sin perfil.
    WithoutProfile,
}

/// One tile, ready to draw.
#[derive(Debug, Clone, PartialEq)]
struct TileData {
    window: WindowInfo,
    profile: Option<String>,
    running: bool,
}

/// The section.
#[component]
pub fn WindowsSection() -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let thumbs = use_context::<Thumbs>();
    let filter = use_signal(QuickFilter::default);
    let query = use_signal(String::new);
    let mut focused = use_signal(|| None::<usize>);
    let mut width = use_signal(|| 0.0f64);
    let mut menu_open = use_signal(|| false);
    let mut menu_at = use_signal(|| (0.0f64, 0.0f64));
    let mut menu_window = use_signal(|| None::<TileData>);

    // Every tile with its profile, in the list's order.
    let s = store.read();
    let running_xid = match &s.session {
        SessionState::Running(view) => Some(view.window.xid),
        SessionState::Starting { target } => Some(target.xid),
        _ => None,
    };
    let all: Vec<TileData> = s
        .windows
        .iter()
        .map(|w| TileData {
            profile: match s.matching_profile(w) {
                Some(ProfileRef::Named(name)) => Some(name),
                _ => None,
            },
            running: running_xid == Some(w.xid),
            window: w.clone(),
        })
        .collect();
    let fps = s.stats.as_ref().map(|st| st.present_fps);
    let aspect_setting = s.gui.tile_aspect;
    let columns_setting = s.gui.tile_columns;
    let interval = s.gui.tile_preview_interval_ms;
    drop(s);

    let total = all.len();
    let q = query.read().clone();
    let tiles: Vec<TileData> = all
        .into_iter()
        .filter(|t| matches(&t.window, t.profile.is_some(), filter(), &q))
        .collect();
    let len = tiles.len();
    let cols = if columns_setting > 0 {
        columns_setting as usize
    } else {
        auto_columns(width(), TILE_MIN, GAP)
    };
    let grid_style = if columns_setting > 0 {
        format!("grid-template-columns: repeat({columns_setting}, minmax(0, 1fr)); gap: {GAP}px")
    } else {
        format!("grid-template-columns: repeat(auto-fill, minmax({TILE_MIN}px, 1fr)); gap: {GAP}px")
    };

    // Ctrl+F: remount the field with autofocus.
    let focus_gen = FOCUS_FILTER();
    let autofocus = focus_gen > *FOCUS_DONE.peek();
    use_effect(move || {
        let generation = FOCUS_FILTER();
        if generation > *FOCUS_DONE.peek() {
            *FOCUS_DONE.write() = generation;
        }
    });

    let menu_items = vec![
        MenuItem::new("windowed", t!("home-tile-menu-windowed")).icon("select_window"),
        MenuItem::new("profile", t!("home-tile-menu-profile")).icon("person_add"),
        MenuItem::new("copy", t!("home-tile-menu-copy")).icon("content_copy"),
    ];
    let filters = vec![
        (QuickFilter::All, t!("home-filter-all")),
        (QuickFilter::WithProfile, t!("home-filter-with-profile")),
        (QuickFilter::WithoutProfile, t!("home-filter-without-profile")),
    ];

    rsx! {
        section { class: "flex flex-col gap-3",
            div { class: "flex flex-wrap items-center gap-3",
                h2 { class: "t-tm m-0 flex-[1_1_200px]",
                    {t!("home-windows-title")}
                    span { class: "font-normal text-on-surface-variant", " · {total}" }
                }
                Connected { options: filters, value: filter, label: t!("home-filter-label") }
            }
            TextField {
                key: "{focus_gen}",
                value: query,
                label: t!("home-search-label"),
                placeholder: t!("home-search-placeholder"),
                leading_icon: "search",
                shortcut: "Ctrl F".to_string(),
                autofocus,
            }
            if total == 0 {
                p { class: "t-bm m-0 py-8 text-center text-on-surface-variant", {t!("home-windows-empty")} }
            } else if len == 0 {
                p { class: "t-bm m-0 py-8 text-center text-on-surface-variant", {t!("home-windows-no-match")} }
            } else {
                div {
                    class: "grid",
                    style: "{grid_style}",
                    role: "list",
                    aria_label: t!("home-windows-title"),
                    onresize: move |e| {
                        if let Ok(size) = e.get_content_box_size() {
                            width.set(size.width);
                        }
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        let Some(key) = nav_key(&e.key()) else { return };
                        if let Some(next) = move_focus(*focused.peek(), key, len, cols) {
                            e.prevent_default();
                            focus_tile(next);
                        }
                    },
                    for (i, tile) in tiles.into_iter().enumerate() {
                        div {
                            key: "{tile.window.xid}",
                            role: "listitem",
                            tabindex: "-1",
                            "data-tile-index": "{i}",
                            onfocusin: move |_| focused.set(Some(i)),
                            onfocusout: move |_| {
                                if *focused.peek() == Some(i) {
                                    focused.set(None);
                                }
                            },
                            WindowTile {
                                window: tile.window.clone(),
                                profile: tile.profile.clone(),
                                state: if tile.running {
                                    TileState::Running
                                } else if focused() == Some(i) {
                                    TileState::Focus
                                } else {
                                    TileState::Idle
                                },
                                thumb: thumbs.url(tile.window.xid),
                                icon: thumbs.icon(tile.window.xid),
                                fps: if tile.running { fps } else { None },
                                aspect: aspect(aspect_setting, &tile.window),
                                on_activate: {
                                    let tile = tile.clone();
                                    move |_| {
                                        super::start(
                                            dispatch,
                                            Target::Window(tile.window.xid),
                                            SessionKind::Fullscreen,
                                            None,
                                            tile.profile.clone(),
                                        )
                                    }
                                },
                                on_menu: {
                                    let tile = tile.clone();
                                    move |at: (f64, f64)| {
                                        menu_window.set(Some(tile.clone()));
                                        menu_at.set(at);
                                        menu_open.set(true);
                                    }
                                },
                            }
                        }
                    }
                }
            }
            Menu {
                open: menu_open,
                items: menu_items,
                at: menu_at(),
                on_select: move |id: &'static str| {
                    menu_open.set(false);
                    let Some(tile) = menu_window.peek().clone() else { return };
                    match id {
                        "windowed" => super::start(
                            dispatch,
                            Target::Window(tile.window.xid),
                            SessionKind::Windowed,
                            None,
                            tile.profile.clone(),
                        ),
                        "profile" => dispatch(Action::OpenDialog(Dialog::NewProfile { from: Some(tile.window) })),
                        "copy" => super::copy_to_clipboard(&tile.window.title),
                        _ => {}
                    }
                },
            }
            p { class: "t-bs m-0 text-on-surface-variant", {t!("home-windows-hint", ms = interval)} }
        }
    }
}

/// Whether a window passes the quick filter and the search (title or
/// `WM_CLASS`, case-insensitive).
pub fn matches(window: &WindowInfo, has_profile: bool, filter: QuickFilter, query: &str) -> bool {
    let by_profile = match filter {
        QuickFilter::All => true,
        QuickFilter::WithProfile => has_profile,
        QuickFilter::WithoutProfile => !has_profile,
    };
    if !by_profile {
        return false;
    }
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    window.title.to_lowercase().contains(&query)
        || window.class.as_ref().is_some_and(|c| {
            c.class.to_lowercase().contains(&query) || c.instance.to_lowercase().contains(&query)
        })
}

/// A key that moves the grid's focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    /// ←.
    Left,
    /// →.
    Right,
    /// ↑.
    Up,
    /// ↓.
    Down,
    /// Inicio.
    Home,
    /// Fin.
    End,
}

fn nav_key(key: &Key) -> Option<NavKey> {
    Some(match key {
        Key::ArrowLeft => NavKey::Left,
        Key::ArrowRight => NavKey::Right,
        Key::ArrowUp => NavKey::Up,
        Key::ArrowDown => NavKey::Down,
        Key::Home => NavKey::Home,
        Key::End => NavKey::End,
        _ => return None,
    })
}

/// The tile focus goes to; `None` when the key does nothing (an edge).
/// Without a focused tile, any key goes to the first.
pub fn move_focus(current: Option<usize>, key: NavKey, len: usize, cols: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let cols = cols.max(1);
    let Some(i) = current.filter(|&i| i < len) else {
        return Some(0);
    };
    let next = match key {
        NavKey::Left => i.checked_sub(1)?,
        NavKey::Right => i + 1,
        NavKey::Up => i.checked_sub(cols)?,
        NavKey::Down => {
            // The last row may be short: go to its last tile.
            if i + cols < len {
                i + cols
            } else if i / cols < (len - 1) / cols {
                len - 1
            } else {
                return None;
            }
        }
        NavKey::Home => 0,
        NavKey::End => len - 1,
    };
    (next < len && next != i).then_some(next)
}

/// Columns of an `auto-fill` grid `width` wide.
pub fn auto_columns(width: f64, min: f64, gap: f64) -> usize {
    if width <= 0.0 {
        return 1;
    }
    (((width + gap) / (min + gap)).floor() as usize).max(1)
}

/// The preview's aspect (Ajustes ▸ Estilo): fixed, or the window's own.
fn aspect(setting: TileAspect, window: &WindowInfo) -> String {
    match setting {
        TileAspect::FourThree => "4/3".into(),
        TileAspect::SixteenNine => "16/9".into(),
        TileAspect::Auto if window.width() > 0 && window.height() > 0 => {
            format!("{}/{}", window.width(), window.height())
        }
        TileAspect::Auto => "4/3".into(),
    }
}

/// Move the DOM focus to tile `i` (its first focusable element, else its
/// cell).
fn focus_tile(i: usize) {
    document::eval(&format!(
        r#"
        const cell = document.querySelector('[data-tile-index="{i}"]');
        if (cell) {{
            const target = cell.querySelector('button, a[href], [tabindex]:not([tabindex="-1"])') || cell;
            target.focus();
            target.scrollIntoView({{ block: "nearest" }});
        }}
        "#
    ));
}

#[cfg(test)]
mod tests {
    use lirtup_x11::{Rect, WindowClass};

    use super::*;

    fn window(title: &str, class: &str) -> WindowInfo {
        WindowInfo {
            xid: 1,
            title: title.into(),
            rect: Rect {
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
            },
            pid: None,
            class: Some(WindowClass {
                instance: class.to_lowercase(),
                class: class.into(),
            }),
        }
    }

    #[test]
    fn filters_by_profile_and_text() {
        let w = window("BALDR SKY (18+ Patch)", "baldrsky.exe");
        assert!(matches(&w, true, QuickFilter::All, ""));
        assert!(matches(&w, true, QuickFilter::WithProfile, "baldr"));
        assert!(!matches(&w, true, QuickFilter::WithoutProfile, ""));
        assert!(matches(&w, false, QuickFilter::WithoutProfile, "  SKY "));
        assert!(matches(&w, false, QuickFilter::All, "BALDRSKY.EXE"));
        assert!(!matches(&w, false, QuickFilter::All, "firefox"));
    }

    #[test]
    fn arrows_move_in_the_grid() {
        // 7 tiles in 3 columns:
        // 0 1 2
        // 3 4 5
        // 6
        let m = |cur, key| move_focus(Some(cur), key, 7, 3);
        assert_eq!(move_focus(None, NavKey::Down, 7, 3), Some(0));
        assert_eq!(m(0, NavKey::Right), Some(1));
        assert_eq!(m(0, NavKey::Left), None);
        assert_eq!(m(1, NavKey::Down), Some(4));
        assert_eq!(m(4, NavKey::Down), Some(6), "short last row");
        assert_eq!(m(6, NavKey::Down), None);
        assert_eq!(m(4, NavKey::Up), Some(1));
        assert_eq!(m(1, NavKey::Up), None);
        assert_eq!(m(2, NavKey::Right), Some(3), "wraps to the next row");
        assert_eq!(m(6, NavKey::Right), None);
        assert_eq!(m(4, NavKey::Home), Some(0));
        assert_eq!(m(4, NavKey::End), Some(6));
        assert_eq!(m(6, NavKey::End), None);
        assert_eq!(move_focus(Some(0), NavKey::Down, 0, 3), None);
        assert_eq!(move_focus(Some(9), NavKey::Down, 3, 3), Some(0), "stale index");
    }

    #[test]
    fn automatic_columns() {
        assert_eq!(auto_columns(0.0, 240.0, 16.0), 1);
        assert_eq!(auto_columns(239.0, 240.0, 16.0), 1);
        assert_eq!(auto_columns(496.0, 240.0, 16.0), 2);
        assert_eq!(auto_columns(1040.0, 240.0, 16.0), 4);
    }

    #[test]
    fn preview_aspect() {
        let w = window("a", "b");
        assert_eq!(aspect(TileAspect::Auto, &w), "1280/720");
        assert_eq!(aspect(TileAspect::FourThree, &w), "4/3");
        assert_eq!(aspect(TileAspect::SixteenNine, &w), "16/9");
    }
}
