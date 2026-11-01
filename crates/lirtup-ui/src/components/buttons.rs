//! Buttons: round at rest, squared while pressed (the M3 Expressive shape
//! morph, `.lf-btn:active` in `main.css`).

use dioxus::prelude::*;

use super::{Icon, Menu, MenuItem, Tooltip};

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

impl ButtonVariant {
    fn class(self) -> &'static str {
        match self {
            ButtonVariant::Filled => "filled",
            ButtonVariant::Tonal => "tonal",
            ButtonVariant::Outlined => "outlined",
            ButtonVariant::Text => "text",
            ButtonVariant::Tertiary => "tertiary",
            ButtonVariant::Danger => "danger",
        }
    }
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

impl ButtonSize {
    fn class(self) -> &'static str {
        match self {
            ButtonSize::Xs => "xs",
            ButtonSize::S => "",
            ButtonSize::M => "m",
            ButtonSize::L => "l",
        }
    }
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
    let button = rsx! {
        button {
            r#type: "button",
            class: "lf-btn lf-state {variant.class()} {size.class()}",
            disabled,
            onclick: move |e| {
                if let Some(onclick) = onclick {
                    onclick(e);
                }
            },
            if let Some(icon) = icon {
                Icon { name: icon, fill: size == ButtonSize::L }
            }
            "{label}"
        }
    };
    match title {
        Some(text) => rsx! {
            Tooltip { text, {button} }
        },
        None => button,
    }
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
    /// Where the tooltip opens (see [`Tooltip`]); `"right"` for buttons at
    /// the right edge of the window.
    #[props(default)]
    tip: &'static str,
) -> Element {
    let variant = match variant {
        IconButtonVariant::Standard => "",
        IconButtonVariant::Tonal => "tonal",
        IconButtonVariant::Outlined => "outlined",
        IconButtonVariant::Filled => "filled",
    };
    let size_class = match size {
        0..=36 => "s32",
        37..=48 => "",
        _ => "s56",
    };
    let icon_size = if size <= 36 { 20 } else { 24 };
    rsx! {
        Tooltip { text: label.clone(), place: tip,
            button {
                r#type: "button",
                class: "lf-ibtn lf-state {variant} {size_class}",
                class: if selected { "sel" },
                "aria-label": label,
                "aria-pressed": selected.then_some("true"),
                disabled,
                onclick: move |e| {
                    if let Some(onclick) = onclick {
                        onclick(e);
                    }
                },
                Icon { name: icon, fill: selected, size: icon_size }
            }
        }
    }
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
    let mut open = use_signal(|| false);
    rsx! {
        span { class: "lf-split lf-menu-anchor",
            button {
                r#type: "button",
                class: "lf-btn lf-state {variant.class()}",
                onclick: on_click,
                "{label}"
            }
            button {
                r#type: "button",
                class: "lf-btn lf-state menu-half {variant.class()}",
                "aria-label": menu_label,
                "aria-haspopup": "menu",
                "aria-expanded": "{open}",
                onclick: move |_| open.toggle(),
                Icon { name: "keyboard_arrow_down", size: 20 }
            }
            Menu { open, items: menu, at: None, on_select }
        }
    }
}
