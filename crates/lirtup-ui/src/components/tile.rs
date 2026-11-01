//! The window tile of Inicio's grid.

use std::rc::Rc;

use dioxus::prelude::*;
use lirtup_x11::WindowInfo;

use super::{Badge, BadgeVariant, Icon};

/// How a [`WindowTile`] looks. Hover and keyboard focus are also CSS states;
/// the prop lets the grid (which owns the arrow-key focus) and the gallery
/// force them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TileState {
    /// At rest.
    #[default]
    Idle,
    /// Pointer over it: the «Escalar» button over the preview.
    Hover,
    /// Keyboard focus of the grid.
    Focus,
    /// Its window is scaling: amber badge «Escalando · N fps».
    Running,
}

/// The CSS `aspect-ratio` of a preview: `4/3`, `16/9`, `video`, or `auto`
/// (the window's own).
fn aspect_ratio(aspect: &str, window: &WindowInfo) -> String {
    match aspect {
        "video" | "16/9" => "16 / 9".into(),
        "auto" if window.rect.width > 0 && window.rect.height > 0 => {
            format!("{} / {}", window.rect.width, window.rect.height)
        }
        "auto" | "" => "4 / 3".into(),
        other => other.replace('/', " / "),
    }
}

/// A window: live preview (`/thumb/{xid}?v={n}`), icon, title and its
/// profile's badge («sin perfil» when none).
///
/// The tile is one focusable element (`data-xid` on it, for the grid's
/// arrow keys): Enter and Space activate it, the Menu key and Shift+F10
/// open its menu at its center; the ⋮ button is for the pointer.
#[component]
pub fn WindowTile(
    window: WindowInfo,
    /// The matching profile's name.
    profile: Option<String>,
    #[props(default)] state: TileState,
    /// The preview's URL (front B's `thumbs.rs` builds it).
    thumb: Option<String>,
    /// The window icon's URL (`/icon/{xid}`).
    icon: Option<String>,
    /// Shown fps while running.
    fps: Option<f32>,
    /// Aspect of the preview (`aspect-4/3`, `aspect-video`; Ajustes ▸ Estilo).
    #[props(default = "4/3".to_string())]
    aspect: String,
    /// Click, Enter: scale it with its profile.
    on_activate: EventHandler<()>,
    /// Right click, ⋮, the Menu key: open the tile menu at the pointer.
    on_menu: EventHandler<(f64, f64)>,
) -> Element {
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);
    let class = match state {
        TileState::Idle => "",
        TileState::Hover => "hov",
        TileState::Focus => "foc",
        TileState::Running => "run",
    };
    let running = state == TileState::Running;
    let ratio = aspect_ratio(&aspect, &window);
    let title = if window.title.is_empty() { t!("cmp-tile-untitled") } else { window.title.clone() };
    let profile_text = profile.clone().unwrap_or_else(|| t!("cmp-tile-no-profile"));
    let running_text = match fps {
        Some(fps) => t!("cmp-tile-running-fps", fps = fps.round() as i64),
        None => t!("cmp-tile-running"),
    };
    let mut label = format!("{title} · {profile_text}");
    if running {
        label = format!("{label} · {running_text}");
    }
    let menu_at_center = move || {
        if let Some(m) = mounted.peek().clone() {
            spawn(async move {
                if let Ok(r) = m.get_client_rect().await {
                    on_menu((r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0));
                }
            });
        }
    };
    rsx! {
        div {
            class: "lf-tile",
            class: "{class}",
            role: "button",
            tabindex: "0",
            "data-xid": "{window.xid}",
            "aria-label": label,
            onmounted: move |e| mounted.set(Some(e.data())),
            onclick: move |_| on_activate(()),
            oncontextmenu: move |e| {
                e.prevent_default();
                let p = e.client_coordinates();
                on_menu((p.x, p.y));
            },
            onkeydown: move |e| match e.key() {
                Key::Enter => on_activate(()),
                Key::Character(c) if c == " " => {
                    e.prevent_default();
                    on_activate(());
                }
                Key::ContextMenu => {
                    e.prevent_default();
                    menu_at_center();
                }
                Key::F10 if e.modifiers().shift() => {
                    e.prevent_default();
                    menu_at_center();
                }
                _ => {}
            },
            div { class: "lf-shot", style: "aspect-ratio: {ratio}",
                if let Some(src) = thumb {
                    img { class: "prev", src, alt: "", draggable: "false" }
                } else {
                    Icon { name: "select_window", size: 32 }
                }
                if running {
                    span { class: "run-badge",
                        Badge { label: running_text, variant: BadgeVariant::Tertiary, icon: "play_arrow" }
                    }
                }
                div { class: "act", "aria-hidden": "true",
                    span { class: "lf-btn filled m",
                        Icon { name: "fullscreen", fill: true }
                        {t!("cmp-tile-scale")}
                    }
                }
            }
            div { class: "lf-tile-t",
                if let Some(src) = icon {
                    img { class: "ico", src, alt: "", draggable: "false" }
                } else {
                    Icon { name: "web_asset", size: 20 }
                }
                span { class: "ttl t-ts", title: "{window.title}", "{title}" }
                match profile {
                    Some(name) => rsx! { Badge { label: name, variant: BadgeVariant::Primary } },
                    None => rsx! { Badge { label: t!("cmp-tile-no-profile") } },
                }
                button {
                    r#type: "button",
                    class: "lf-ibtn s32 lf-state",
                    tabindex: "-1",
                    "aria-hidden": "true",
                    onclick: move |e| {
                        e.stop_propagation();
                        let p = e.client_coordinates();
                        on_menu((p.x, p.y));
                    },
                    Icon { name: "more_vert", size: 20 }
                }
            }
        }
    }
}
