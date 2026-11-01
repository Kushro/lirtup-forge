//! Things over the page: the floating toolbar, the snackbar, dialogs, menus
//! and tooltips.
// Front A: remove when the bodies are written.
#![allow(unused_variables)]

use dioxus::prelude::*;

use super::MenuItem;

/// A floating toolbar (`.ftb`, `h-16 rounded-full`), at the bottom center
/// of the page (the save bar) — it animates in and out with `open`.
#[component]
pub fn FloatingToolbar(open: bool, children: Element) -> Element {
    todo!()
}

/// The snackbar (`.snack`): one at a time, bottom left, above the save bar.
#[component]
pub fn Snackbar(
    /// Text (translated).
    message: String,
    /// The action («Deshacer»).
    action: Option<String>,
    on_action: Option<EventHandler<()>>,
    on_close: EventHandler<()>,
) -> Element {
    todo!()
}

/// A modal dialog (`.dlg`, `rounded-xl`): scrim, focus trapped inside, Esc
/// and the scrim close it (`on_close`).
#[component]
pub fn Dialog(
    open: Signal<bool>,
    /// Material Symbols name over the title (`edit_note`).
    icon: Option<&'static str>,
    /// Color of the icon: `tertiary` for unsaved changes, `error` for
    /// deleting (a CSS variable name without `--`).
    icon_color: Option<&'static str>,
    title: String,
    /// Wider for «Nuevo perfil» (with its side panel).
    #[props(default)]
    wide: bool,
    children: Element,
    /// The buttons, right aligned.
    actions: Element,
    on_close: Option<EventHandler<()>>,
) -> Element {
    todo!()
}

/// A menu (`.menu`, `rounded-sm`) at a point or under its anchor; arrows,
/// Enter and Esc.
#[component]
pub fn Menu(
    open: Signal<bool>,
    items: Vec<MenuItem>,
    /// Page coordinates (a right click); `None` = under the previous
    /// sibling (a split button).
    at: Option<(f64, f64)>,
    on_select: EventHandler<&'static str>,
) -> Element {
    todo!()
}

/// A tooltip (`.tip`) on hover and focus of its child, after 500 ms.
#[component]
pub fn Tooltip(text: String, children: Element) -> Element {
    todo!()
}
