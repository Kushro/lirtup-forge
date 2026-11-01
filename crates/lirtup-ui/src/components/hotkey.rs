//! Hotkeys: their keycaps and the capture row.
// Front A: remove when the bodies are written.
#![allow(unused_variables)]

use dioxus::prelude::*;
use lirtup_config::Hotkey;

/// One key (`.kbd`, `h-9 rounded-md bg-primary`; `soft` for hints).
#[component]
pub fn Keycap(label: String, #[props(default)] soft: bool) -> Element {
    todo!()
}

/// The keys of a hotkey and a «Cambiar atajo» button. Capturing: the row
/// turns primary-container, «Presione teclas… · Esc cancela», the keys held
/// show as soft keycaps; Esc cancels, Backspace clears. The captured combo
/// uses the names `lirtup-config` parses (`Hotkey::from_str`).
#[component]
pub fn HotkeyCapture(
    value: Signal<Option<Hotkey>>,
    /// The action's name (for the aria label).
    label: String,
    /// Refuse a combo without Ctrl, Alt, Shift or Super («Requiere un
    /// modificador» in error color).
    #[props(default)]
    require_modifier: bool,
    #[props(default)] disabled: bool,
    on_change: Option<EventHandler<Option<Hotkey>>>,
) -> Element {
    todo!()
}
