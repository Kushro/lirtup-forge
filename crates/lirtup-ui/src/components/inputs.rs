//! Inputs. Each one edits a `Signal` the caller owns; the caller turns a
//! change into an `Action` (`use_effect` on the signal, or `on_change`).
// Front A: remove when the bodies are written.
#![allow(unused_variables)]

use dioxus::prelude::*;

/// A connected button group (`.seg`): short choices such as the effects
/// engine (Nativo | MagpieFX) or the multiplier (Apagado ×2 ×3 ×4 Auto).
#[component]
pub fn Connected<T: Clone + PartialEq + 'static>(
    /// Value and label of each segment.
    options: Vec<(T, String)>,
    value: Signal<T>,
    /// `aria-label` of the group.
    label: String,
    /// Optional icon per segment, same order as `options`.
    #[props(default)]
    icons: Vec<Option<&'static str>>,
    /// The selected segment in primary (the multiplier's Auto).
    #[props(default)]
    primary: bool,
    #[props(default)] disabled: bool,
    on_change: Option<EventHandler<T>>,
) -> Element {
    todo!()
}

/// An on/off switch (`role="switch"`). In settings rows always a switch,
/// never a checkbox.
#[component]
pub fn Switch(
    value: Signal<bool>,
    /// `aria-label` (the row's title).
    label: String,
    #[props(default)] disabled: bool,
    on_change: Option<EventHandler<bool>>,
) -> Element {
    todo!()
}

/// A checkbox, only in multi-select lists.
#[component]
pub fn Checkbox(value: Signal<bool>, label: String, #[props(default)] disabled: bool) -> Element {
    todo!()
}

/// A slider with an editable number next to it.
#[component]
pub fn Slider(
    value: Signal<f64>,
    min: f64,
    max: f64,
    step: f64,
    /// `aria-label`.
    label: String,
    /// How the number shows (`0,75`, `120 %`); `None` = the value with the
    /// decimals of `step`, in the current language.
    format: Option<Callback<f64, String>>,
    /// Named stops instead of a number (`ModelSlider`: none, 2x8 … 8x32):
    /// the value is the index.
    #[props(default)]
    stops: Vec<String>,
    #[props(default)] disabled: bool,
    on_change: Option<EventHandler<f64>>,
) -> Element {
    todo!()
}

/// A dropdown (`.dd`).
#[component]
pub fn Select<T: Clone + PartialEq + 'static>(
    options: Vec<(T, String)>,
    value: Signal<T>,
    label: String,
    #[props(default)] disabled: bool,
    on_change: Option<EventHandler<T>>,
) -> Element {
    todo!()
}

/// A text field (`.fld`), with inline validation.
#[component]
pub fn TextField(
    value: Signal<String>,
    /// Floating label (translated).
    label: String,
    placeholder: Option<String>,
    /// Material Symbols name before the text (`search`).
    leading_icon: Option<&'static str>,
    /// A keycap hint at the end («Ctrl F»).
    shortcut: Option<String>,
    /// Shown under the field in error color; the field turns error.
    error: Option<String>,
    #[props(default)] disabled: bool,
    /// Focus it when mounted (Ctrl+F, the new profile name).
    #[props(default)]
    autofocus: bool,
    on_change: Option<EventHandler<String>>,
    /// Enter.
    on_submit: Option<EventHandler<String>>,
) -> Element {
    todo!()
}

/// A path with a «Elegir…» button. The webview cannot return paths, so the
/// picker is native (front A asks for `rfd` if it uses it) or the field is
/// text only.
#[component]
pub fn PathField(
    value: Signal<String>,
    label: String,
    /// Pick a folder instead of a file.
    #[props(default)]
    directory: bool,
    /// File filters for the picker (`("LUT", ["cube", "png"])`).
    #[props(default)]
    filters: Vec<(String, Vec<String>)>,
    #[props(default)] disabled: bool,
    on_change: Option<EventHandler<String>>,
) -> Element {
    todo!()
}

/// A color as `#RRGGBB` (background color, seed color).
#[component]
pub fn ColorField(
    value: Signal<String>,
    label: String,
    /// Swatches offered next to the picker (the 20 legacy palettes as seeds).
    #[props(default)]
    swatches: Vec<String>,
    #[props(default)] disabled: bool,
    on_change: Option<EventHandler<String>>,
) -> Element {
    todo!()
}

/// A font family; empty = the bundled Google Sans Flex.
#[component]
pub fn FontField(
    value: Signal<String>,
    label: String,
    /// Families to offer (`fc-list : family`, read once by the caller).
    families: Vec<String>,
    #[props(default)] disabled: bool,
    on_change: Option<EventHandler<String>>,
) -> Element {
    todo!()
}
