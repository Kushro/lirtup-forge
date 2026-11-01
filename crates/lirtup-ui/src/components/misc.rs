//! Small pieces: badges, chips, icons and the loading indicator.

use dioxus::prelude::*;

/// [`Badge`] colors (`.badge.*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeVariant {
    /// Surface container («Hereda», «Global», «sin perfil»).
    #[default]
    Neutral,
    /// Primary container («×2 Auto»).
    Primary,
    /// Tertiary container, live («Escalando»).
    Tertiary,
    /// Success container (a measured cost: «0,9 ms»).
    Success,
    /// Error container («Necesita dxc»).
    Error,
}

/// A small label (`h-[22px] rounded-full`).
#[component]
pub fn Badge(label: String, #[props(default)] variant: BadgeVariant, icon: Option<&'static str>) -> Element {
    let class = match variant {
        BadgeVariant::Neutral => "",
        BadgeVariant::Primary => "pri",
        BadgeVariant::Tertiary => "ter",
        BadgeVariant::Success => "ok",
        BadgeVariant::Error => "err",
    };
    rsx! {
        span { class: "lf-badge {class}",
            if let Some(icon) = icon {
                Icon { name: icon, size: 14 }
            }
            "{label}"
        }
    }
}

/// A filter or rule chip (`.chip`); selectable when `onclick` is set.
#[component]
pub fn Chip(
    label: String,
    icon: Option<&'static str>,
    #[props(default)] selected: bool,
    /// A trailing × that removes it (rule chips of the profile header).
    on_remove: Option<EventHandler<()>>,
    onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    let on = if selected { "on" } else { "" };
    // A selected filter chip shows a check when it has no icon of its own.
    let icon = icon.or(selected.then_some("check"));
    let content = rsx! {
        if let Some(icon) = icon {
            Icon { name: icon, size: 18 }
        }
        "{label}"
    };
    let remove = on_remove.map(|on_remove| {
        rsx! {
            button {
                r#type: "button",
                class: "lf-chip-x lf-state",
                "aria-label": t!("cmp-chip-remove", label = label.clone()),
                onclick: move |e| {
                    e.stop_propagation();
                    on_remove(());
                },
                Icon { name: "close", size: 18 }
            }
        }
    });
    match onclick {
        Some(onclick) => rsx! {
            span { class: "lf-chip click {on}",
                button {
                    r#type: "button",
                    class: "lf-chip-main lf-state",
                    "aria-pressed": "{selected}",
                    onclick,
                    {content}
                }
                {remove}
            }
        },
        None => rsx! {
            span { class: "lf-chip {on}", {content} {remove} }
        },
    }
}

/// A Material Symbols Rounded icon (the bundled subset, by ligature name).
/// Decorative (`aria-hidden`) unless `label` is set.
#[component]
pub fn Icon(
    name: &'static str,
    /// FILL 1, weight 500 (selected states).
    #[props(default)]
    fill: bool,
    /// 18, 20, 24 (default) or 32 px.
    #[props(default = 24)]
    size: u8,
    label: Option<String>,
) -> Element {
    let fill = if fill { "fill" } else { "" };
    let style = format!("font-size: {size}px");
    match label {
        Some(label) => rsx! {
            span { class: "lf-ms {fill}", style, role: "img", "aria-label": label, translate: "no", {name} }
        },
        None => rsx! {
            span { class: "lf-ms {fill}", style, "aria-hidden": "true", translate: "no", {name} }
        },
    }
}

/// The M3 Expressive loading indicator (a morphing shape); a plain spinner
/// under `prefers-reduced-motion`.
#[component]
pub fn LoadingIndicator(
    /// 24, 38 (default) or 48 px.
    #[props(default = 38)]
    size: u8,
    /// `aria-label` (translated).
    label: String,
) -> Element {
    rsx! {
        span {
            class: "lf-load",
            style: "width: {size}px; height: {size}px",
            role: "progressbar",
            "aria-label": label,
            "aria-busy": "true",
            span { class: "shape" }
            span { class: "ring" }
        }
    }
}
