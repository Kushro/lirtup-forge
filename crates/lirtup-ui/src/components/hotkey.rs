//! Hotkeys: their keycaps and the capture row.

use std::rc::Rc;

use dioxus::prelude::*;
use lirtup_config::{Hotkey, Modifiers};

use super::{Icon, IconButton};

/// One key (`.kbd`, `h-9 rounded-md bg-primary`; `soft` for hints).
#[component]
pub fn Keycap(label: String, #[props(default)] soft: bool) -> Element {
    rsx! {
        kbd { class: "lf-kbd", class: if soft { "soft" }, "{label}" }
    }
}

/// The 1.x key name (`lirtup-x11`'s `keysym_for`) of a DOM key code
/// (`KeyA`, `Digit5`, `ArrowUp`…); `None` for a modifier or an unknown key.
pub fn key_name(code: &str) -> Option<String> {
    if let Some(letter) = code.strip_prefix("Key") {
        return Some(letter.to_string());
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return Some(digit.to_string());
    }
    if let Some(n) = code.strip_prefix('F').and_then(|n| n.parse::<u8>().ok())
        && (1..=12).contains(&n)
    {
        return Some(format!("F{n}"));
    }
    let name = match code {
        "ArrowLeft" => "Left",
        "ArrowUp" => "Up",
        "ArrowRight" => "Right",
        "ArrowDown" => "Down",
        "Insert" => "Insert",
        "Delete" => "Delete",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "PageUp",
        "PageDown" => "PageDown",
        "PrintScreen" => "Print",
        "Pause" => "Pause",
        "ScrollLock" => "ScrollLock",
        "BracketLeft" => "LeftBracket",
        "BracketRight" => "RightBracket",
        "Backslash" => "Backslash",
        "Semicolon" => "Semicolon",
        "Quote" => "Apostrophe",
        "Comma" => "Comma",
        "Period" => "Period",
        "Slash" => "Slash",
        "NumpadAdd" => "Plus",
        "Minus" | "NumpadSubtract" => "Minus",
        "Equal" => "Equal",
        "Backquote" => "Grave",
        "Space" => "Space",
        "Enter" | "NumpadEnter" => "Return",
        "Backspace" => "Backspace",
        "Tab" => "Tab",
        "Escape" => "Escape",
        _ => return None,
    };
    Some(name.to_string())
}

/// The keycap labels of a hotkey: `["Alt", "Shift", "A"]`.
pub fn hotkey_keys(hotkey: &Hotkey) -> Vec<String> {
    hotkey.to_string().split('+').map(str::to_string).collect()
}

fn modifiers_of(e: &KeyboardEvent) -> Modifiers {
    let m = e.modifiers();
    let mut out = Modifiers::default();
    for (on, bit) in [
        (m.ctrl(), Modifiers::CONTROL),
        (m.alt(), Modifiers::ALT),
        (m.shift(), Modifiers::SHIFT),
        (
            m.meta() || m.contains(dioxus::prelude::Modifiers::SUPER),
            Modifiers::SUPER,
        ),
    ] {
        if on {
            out = out | bit;
        }
    }
    out
}

fn modifier_names(m: Modifiers) -> Vec<String> {
    [
        (Modifiers::CONTROL, "Ctrl"),
        (Modifiers::ALT, "Alt"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::SUPER, "Super"),
    ]
    .into_iter()
    .filter(|(bit, _)| m.contains(*bit))
    .map(|(_, name)| name.to_string())
    .collect()
}

/// The keys of a hotkey and a «Cambiar atajo» button. Capturing: the row
/// turns primary-container, «Presione teclas… · Esc cancela», the keys held
/// show as soft keycaps; Esc cancels, Backspace clears. The captured combo
/// uses the names `lirtup-config` parses (`Hotkey::from_str`).
///
/// Combos that `lirtup serve` grabs on X11 (the activation hotkeys) never
/// reach the webview while it runs (front A's note): `on_capture` tells the
/// caller when capturing starts and ends, so it can release them
/// (`Command::PauseHotkeys`, round 4 front Y).
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
    /// Capturing started (`true`) or ended (`false`, also when the
    /// component goes away while capturing).
    on_capture: Option<EventHandler<bool>>,
) -> Element {
    let mut capturing = use_signal(|| false);
    let mut reported = use_signal(|| false);
    use_effect(move || {
        let now = capturing();
        if now != *reported.peek() {
            reported.set(now);
            if let Some(on_capture) = on_capture {
                on_capture(now);
            }
        }
    });
    use_drop(move || {
        if *reported.peek()
            && let Some(on_capture) = on_capture
        {
            on_capture(false);
        }
    });
    let mut held = use_signal(Modifiers::default);
    let mut error = use_signal(|| false);
    let mut target = use_signal(|| None::<Rc<MountedData>>);
    let mut set = move |next: Option<Hotkey>| {
        value.set(next.clone());
        if let Some(on_change) = on_change {
            on_change(next);
        }
    };
    let mut stop = move || {
        capturing.set(false);
        held.set(Modifiers::default());
    };
    let start = move |_| {
        error.set(false);
        held.set(Modifiers::default());
        capturing.set(true);
    };
    let keys = value.read().as_ref().map(hotkey_keys);
    rsx! {
        div { class: "lf-hk flex items-center gap-2 flex-wrap justify-end", class: if capturing() { "cap" },
            if capturing() {
                div {
                    class: "flex items-center gap-2 rounded-md outline-none",
                    tabindex: "0",
                    role: "textbox",
                    "aria-label": t!("cmp-hotkey-capturing-label", action = label.clone()),
                    onmounted: move |e| {
                        let data = e.data();
                        target.set(Some(data.clone()));
                        async move {
                            let _ = data.set_focus(true).await;
                        }
                    },
                    onblur: move |_| stop(),
                    onkeydown: move |e| {
                        e.prevent_default();
                        e.stop_propagation();
                        let mods = modifiers_of(&e);
                        let code = e.code().to_string();
                        let Some(key) = key_name(&code) else {
                            // A modifier alone: show it held.
                            held.set(mods);
                            return;
                        };
                        if mods.is_empty() && key == "Escape" {
                            stop();
                            return;
                        }
                        if mods.is_empty() && key == "Backspace" {
                            set(None);
                            stop();
                            return;
                        }
                        if require_modifier && mods.is_empty() {
                            error.set(true);
                            return;
                        }
                        let hotkey = Hotkey { modifiers: mods, key };
                        // Through the parser, so the value is what YAML will hold.
                        if let Ok(hotkey) = hotkey.to_string().parse::<Hotkey>() {
                            set(Some(hotkey));
                        }
                        error.set(false);
                        stop();
                    },
                    onkeyup: move |e| held.set(modifiers_of(&e)),
                    span { class: "t-bs", {t!("cmp-hotkey-press")} }
                    for name in modifier_names(held()) {
                        Keycap { key: "{name}", label: name, soft: true }
                    }
                    Keycap { label: "…", soft: true }
                }
                IconButton { icon: "close", label: t!("cmp-hotkey-cancel"), size: 32, onclick: move |_| stop() }
            } else {
                match keys {
                    Some(keys) => {
                        let joined = keys.join("+");
                        rsx! {
                        span { class: "flex items-center gap-1.5", "aria-label": t!("cmp-hotkey-current", action = label.clone(), keys = joined),
                            for (i, k) in keys.into_iter().enumerate() {
                                Keycap { key: "{i}", label: k }
                            }
                        }
                        }
                    }
                    None => rsx! {
                        span { class: "t-bm text-on-surface-variant", {t!("cmp-hotkey-unset")} }
                    },
                }
                IconButton {
                    icon: "edit",
                    label: t!("cmp-hotkey-change", action = label.clone()),
                    size: 32,
                    disabled,
                    onclick: start,
                }
            }
            if error() {
                span { class: "basis-full text-right t-bs text-error", role: "alert", {t!("cmp-hotkey-needs-modifier")} }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dom_codes_map_to_1x_names() {
        assert_eq!(key_name("KeyA").as_deref(), Some("A"));
        assert_eq!(key_name("Digit5").as_deref(), Some("5"));
        assert_eq!(key_name("F12").as_deref(), Some("F12"));
        assert_eq!(key_name("ArrowUp").as_deref(), Some("Up"));
        assert_eq!(key_name("NumpadAdd").as_deref(), Some("Plus"));
        assert_eq!(key_name("ShiftLeft"), None);
        let h: Hotkey = "Alt+Shift+A".parse().unwrap();
        assert_eq!(hotkey_keys(&h), ["Alt", "Shift", "A"]);
        // Every name produced is one lirtup-x11 maps to a keysym.
        for code in [
            "KeyQ",
            "Digit0",
            "F1",
            "BracketLeft",
            "Quote",
            "Backquote",
            "Enter",
            "PrintScreen",
            "ArrowLeft",
            "NumpadAdd",
            "Minus",
            "Equal",
            "Space",
            "Tab",
            "Delete",
            "PageDown",
        ] {
            let name = key_name(code).unwrap();
            assert!(
                lirtup_x11::hotkeys::keysym_for(&name).is_some(),
                "{code} -> {name}"
            );
        }
    }
}
