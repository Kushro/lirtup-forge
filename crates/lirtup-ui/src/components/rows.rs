//! Settings rows: the inheritance of a value made visible (decision 4 of
//! the design).
// Front A: remove when the bodies are written.
#![allow(unused_variables)]

use dioxus::prelude::*;

use crate::state::Origin;

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
) -> Element {
    todo!()
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
    todo!()
}
