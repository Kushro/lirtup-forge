//! Buttons: round at rest, squared while pressed (the M3 Expressive shape
//! morph, `press-morph` in `tailwind.css`).
// Front A: remove when the bodies are written.
#![allow(unused_variables)]

use dioxus::prelude::*;

use super::MenuItem;

/// [`Button`] styles (`.btn-*` of `lf.css`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Primary action.
    #[default]
    Filled,
    /// Secondary container (Guardar in a dialog).
    Tonal,
    /// Outline (Cancelar).
    Outlined,
    /// Text only (Más tarde, Deshacer).
    Text,
    /// Tertiary container, the «live» amber (Detener).
    Tertiary,
    /// Error container (Eliminar).
    Danger,
}

/// [`Button`] heights: XS 32, S 40, M 56, L 96 px (L only for «Escalar»
/// on the empty session).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// 32 px.
    Xs,
    /// 40 px.
    #[default]
    S,
    /// 56 px.
    M,
    /// 96 px.
    L,
}

/// A button with an optional leading icon.
#[component]
pub fn Button(
    /// Text.
    label: String,
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    /// Material Symbols name.
    icon: Option<&'static str>,
    #[props(default)] disabled: bool,
    /// Tooltip / keyboard hint («Guardar (Ctrl+S)»).
    title: Option<String>,
    onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    todo!()
}

/// [`IconButton`] styles (`.ibtn.*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconButtonVariant {
    /// Transparent.
    #[default]
    Standard,
    /// Secondary container.
    Tonal,
    /// Outline.
    Outlined,
    /// Primary.
    Filled,
}

/// An icon-only button (40 px; 32 or 56 with `size`).
#[component]
pub fn IconButton(
    /// Material Symbols name.
    icon: &'static str,
    /// `aria-label` and tooltip (translated): required, the button has no
    /// text.
    label: String,
    #[props(default)] variant: IconButtonVariant,
    /// Toggle state (`aria-pressed`; filled icon, squared shape).
    #[props(default)]
    selected: bool,
    /// 32, 40 or 56 px.
    #[props(default = 40)]
    size: u8,
    #[props(default)] disabled: bool,
    onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    todo!()
}

/// A button with a menu half (Restablecer ▾ of the save bar).
#[component]
pub fn SplitButton(
    /// Text of the main half.
    label: String,
    #[props(default)] variant: ButtonVariant,
    /// `aria-label` of the menu half («Más opciones de restablecer»).
    menu_label: String,
    /// The main half.
    on_click: EventHandler<MouseEvent>,
    /// The menu.
    menu: Vec<MenuItem>,
    /// A menu entry, by [`MenuItem::id`].
    on_select: EventHandler<&'static str>,
) -> Element {
    todo!()
}
