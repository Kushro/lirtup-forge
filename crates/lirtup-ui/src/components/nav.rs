//! The drawer and the tabs.

use dioxus::prelude::*;

use super::{Icon, Tooltip};

/// Whether the drawer around a [`NavItem`] is a rail (labels become
/// tooltips).
#[derive(Clone, Copy)]
struct Rail(bool);

/// The navigation drawer: 288 px, or an 80 px rail below 1100 px of window
/// width (labels become tooltips).
#[component]
pub fn NavDrawer(
    /// Rail mode.
    #[props(default)]
    rail: bool,
    /// The top part (scrolls): Inicio, Modos, the profiles.
    children: Element,
    /// Pinned at the bottom: Acerca de, Ajustes.
    footer: Element,
) -> Element {
    let mut ctx = use_context_provider(|| Signal::new(Rail(rail)));
    if ctx.peek().0 != rail {
        ctx.set(Rail(rail));
    }
    rsx! {
        nav { class: "lf-nav", class: if rail { "rail" }, "aria-label": t!("cmp-nav-label"),
            div { class: "top", {children} }
            div { class: "bottom", {footer} }
        }
    }
}

/// A section title in the drawer («Perfiles»).
#[component]
pub fn NavHeader(label: String) -> Element {
    rsx! {
        h2 { class: "lf-nav-h", "{label}" }
    }
}

/// What a [`NavItem`] shows before its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavIcon {
    /// A Material Symbols name.
    Symbol(&'static str),
    /// A profile: its icon (`/icon/profile/<name>` asset) or, without one,
    /// its initials on a color derived from the name.
    Profile {
        /// Initials («BS»).
        initials: String,
        /// Background when there is no image (`#3D6FB6`).
        color: String,
        /// The icon's URL, if the profile has one.
        image: Option<String>,
    },
}

/// A profile's icon: its image, or its initials on its color (`.pico`).
#[component]
pub fn ProfileIcon(icon: NavIcon, #[props(default)] fill: bool) -> Element {
    match icon {
        NavIcon::Symbol(name) => rsx! {
            Icon { name, fill }
        },
        NavIcon::Profile { image: Some(src), .. } => rsx! {
            img { class: "lf-pico", src, alt: "" }
        },
        NavIcon::Profile { initials, color, .. } => rsx! {
            span { class: "lf-pico", style: "background-color: {color}", "aria-hidden": "true", "{initials}" }
        },
    }
}

/// One entry of the drawer (`h-13 rounded-full`).
#[component]
pub fn NavItem(
    label: String,
    icon: NavIcon,
    #[props(default)] selected: bool,
    /// Amber dot: unsaved changes.
    #[props(default)]
    dirty: bool,
    /// Amber ▶: this profile is scaling now.
    #[props(default)]
    running: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let rail = try_use_context::<Signal<Rail>>().is_some_and(|r| r.read().0);
    // What the dot and the ▶ say, for screen readers and the rail tooltip.
    let mut status = Vec::new();
    if running {
        status.push(t!("nav-running"));
    }
    if dirty {
        status.push(t!("nav-unsaved"));
    }
    let full = if status.is_empty() { label.clone() } else { format!("{label} · {}", status.join(" · ")) };
    let item = rsx! {
        button {
            r#type: "button",
            class: "lf-nav-i lf-state",
            class: if selected { "on" },
            "aria-current": selected.then_some("page"),
            "aria-label": full.clone(),
            onclick,
            ProfileIcon { icon, fill: selected }
            span { class: "lbl", "{label}" }
            span { class: "tail",
                if running {
                    span { class: "lf-ms fill run", "aria-hidden": "true", "play_arrow" }
                }
                if dirty {
                    span { class: "dot" }
                }
            }
            if rail && (dirty || running) {
                span { class: "rail-mark dot", style: if running && !dirty { "background: var(--tertiary)" } }
            }
        }
    };
    if rail {
        rsx! {
            Tooltip { text: full, place: "side", {item} }
        }
    } else {
        item
    }
}

/// One tab of [`Tabs`].
#[derive(Debug, Clone, PartialEq)]
pub struct TabItem<T> {
    /// Returned by `on_select`.
    pub value: T,
    /// Text (translated).
    pub label: String,
    /// Material Symbols name.
    pub icon: &'static str,
    /// Amber dot: unsaved changes in the tab.
    pub dot: bool,
}

/// Tabs (`.tabs`; `role="tablist"`, arrows move between them).
///
/// Arrows, Home and End select the tab they reach (automatic activation)
/// and move the focus there; only the selected tab is in the Tab order.
#[component]
pub fn Tabs<T: Clone + PartialEq + 'static>(tabs: Vec<TabItem<T>>, selected: T, on_select: EventHandler<T>) -> Element {
    let id = super::overlays::use_dom_id();
    let count = tabs.len();
    let current = tabs.iter().position(|t| t.value == selected).unwrap_or(0);
    let values: Vec<T> = tabs.iter().map(|t| t.value.clone()).collect();
    rsx! {
        div {
            id: "{id}",
            class: "lf-tabs",
            role: "tablist",
            onkeydown: move |e| {
                if count == 0 {
                    return;
                }
                let next = match e.key() {
                    Key::ArrowRight => (current + 1) % count,
                    Key::ArrowLeft => (current + count - 1) % count,
                    Key::Home => 0,
                    Key::End => count - 1,
                    _ => return,
                };
                e.prevent_default();
                on_select(values[next].clone());
                let id = id.clone();
                spawn(async move {
                    let _ = document::eval(&format!(
                        "requestAnimationFrame(() => document.getElementById('{id}')?.querySelectorAll('[role=tab]')[{next}]?.focus());"
                    ))
                    .await;
                });
            },
            for (i, tab) in tabs.into_iter().enumerate() {
                {
                    let on = i == current;
                    let value = tab.value.clone();
                    let label = if tab.dot { format!("{} · {}", tab.label, t!("nav-unsaved")) } else { tab.label.clone() };
                    rsx! {
                        button {
                            key: "{i}",
                            r#type: "button",
                            class: "lf-tab lf-state",
                            class: if on { "on" },
                            role: "tab",
                            "aria-selected": "{on}",
                            "aria-label": label,
                            tabindex: if on { "0" } else { "-1" },
                            onclick: move |_| on_select(value.clone()),
                            Icon { name: tab.icon, size: 20, fill: on }
                            "{tab.label}"
                            if tab.dot {
                                span { class: "dot" }
                            }
                        }
                    }
                }
            }
        }
    }
}
