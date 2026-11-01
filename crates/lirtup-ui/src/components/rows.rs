//! Settings rows: the inheritance of a value made visible (decision 4 of
//! the design).

use dioxus::prelude::*;

use super::{Badge, Icon, IconButton};
use crate::state::Origin;

/// What the layer below a row is, for its origin note. `app.rs` provides it
/// as a context (`Memo<RowLayer>`, from the route); without one (the
/// gallery) rows read as a profile's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowLayer {
    /// A named profile, over *Predeterminado*.
    #[default]
    Profile,
    /// *Predeterminado* (and the hotkeys of Ajustes), over the system values.
    Default,
    /// A scaling mode, over the system values.
    Mode,
}

/// One setting: icon, title, subtitle and the control (`children`).
///
/// - `Inherited`: grey subtitle «Hereda de Predeterminado» (or `subtitle`).
/// - `Overridden`: primary stripe, «Cambiado en este perfil» and a
///   «Volver a heredar» icon button (`on_inherit`).
/// - `Dirty`: amber stripe and «Sin guardar · antes: …».
#[component]
pub fn SettingRow(
    /// Material Symbols name.
    icon: &'static str,
    title: String,
    /// Help text; replaces the origin note when the origin is `Inherited`.
    subtitle: Option<String>,
    origin: Origin,
    /// «Volver a heredar» (shown with `Overridden`; `None` on
    /// *Predeterminado* and on global settings).
    on_inherit: Option<EventHandler<()>>,
    /// The row says the setting is global (frame generation): a «Global»
    /// badge, and no inheritance stripe.
    #[props(default)]
    global: bool,
    /// The control (switch, select, slider…), and any expanded detail
    /// (`Disclosure` content goes under the row).
    children: Element,
    /// Content under the row, full width (a `Disclosure`'s body, an
    /// effect's parameters).
    detail: Option<Element>,
) -> Element {
    let layer = try_use_context::<Memo<RowLayer>>()
        .map(|m| m())
        .unwrap_or_default();
    // A global setting has no layer to inherit from: only «unsaved» shows.
    let origin = if global && origin == Origin::Overridden {
        Origin::Inherited
    } else {
        origin
    };
    let inherited = || match layer {
        RowLayer::Profile => t!("cmp-row-inherited"),
        RowLayer::Default | RowLayer::Mode => t!("cmp-row-inherited-system"),
    };
    let overridden = match layer {
        RowLayer::Profile => t!("cmp-row-overridden"),
        RowLayer::Default => t!("cmp-row-overridden-default"),
        RowLayer::Mode => t!("cmp-row-overridden-mode"),
    };
    let (class, note) = match &origin {
        Origin::Inherited => ("", subtitle.clone().or_else(|| (!global).then(inherited))),
        Origin::Overridden => ("ovr", Some(overridden)),
        Origin::Dirty { before } => ("dirty", Some(t!("cmp-row-dirty", before = before.clone()))),
    };
    // With a change, the help text still shows, over the origin note.
    let help = match origin {
        Origin::Inherited => None,
        _ => subtitle,
    };
    rsx! {
        div { class: "lf-row {class}",
            span { class: "lead", Icon { name: icon } }
            div { class: "grow",
                span { class: "t-bl", "{title}" }
                if let Some(help) = help {
                    span { class: "t-bs text-on-surface-variant", "{help}" }
                }
                if let Some(note) = note {
                    span { class: "note", "{note}" }
                }
            }
            div { class: "ctl",
                if global {
                    Badge { label: t!("cmp-row-global"), icon: "public" }
                }
                if let (Origin::Overridden, Some(on_inherit)) = (&origin, on_inherit) {
                    IconButton { icon: "layers_clear", label: t!("cmp-row-inherit"), size: 32, onclick: move |_| on_inherit(()) }
                }
                {children}
            }
            if let Some(detail) = detail {
                div { class: "detail", {detail} }
            }
        }
    }
}

/// A group of rows (`.group`): 2 px gaps, the first and last rows rounded.
#[component]
pub fn SettingGroup(
    /// Title over the group (`t-ts`), optional.
    title: Option<String>,
    /// Collapsible (Avanzado's groups); starts collapsed when `collapsed`.
    #[props(default)]
    collapsible: bool,
    #[props(default)] collapsed: bool,
    children: Element,
) -> Element {
    let mut shut = use_signal(|| collapsible && collapsed);
    let rows_id = super::overlays::use_dom_id();
    rsx! {
        section { class: "lf-group",
            match (title, collapsible) {
                (Some(title), true) => rsx! {
                    button {
                        r#type: "button",
                        class: "lf-group-h lf-state",
                        "aria-expanded": "{!shut()}",
                        "aria-controls": "{rows_id}",
                        onclick: move |_| shut.toggle(),
                        "{title}"
                        Icon { name: "expand_more", size: 20 }
                    }
                },
                (Some(title), false) => rsx! {
                    h3 { class: "lf-group-h", "{title}" }
                },
                (None, _) => rsx! {},
            }
            if !shut() {
                div { id: "{rows_id}", class: "lf-group-rows", {children} }
            }
        }
    }
}
