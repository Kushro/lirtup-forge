//! Inputs. Each one edits a `Signal` the caller owns; the caller turns a
//! change into an `Action` (`use_effect` on the signal, or `on_change`).

use dioxus::prelude::*;

use super::overlays::{MenuEntry, MenuPopup, use_dom_id};
use super::Icon;
use crate::i18n::{LANG, Lang};

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
    let current = value.read().clone();
    rsx! {
        div { class: "lf-seg", role: "group", "aria-label": label,
            for (i, (option, text)) in options.into_iter().enumerate() {
                {
                    let on = option == current;
                    // The selected segment shows its icon, or a check.
                    let icon = icons.get(i).copied().flatten().or(on.then_some("check"));
                    rsx! {
                        button {
                            key: "{i}",
                            r#type: "button",
                            class: "lf-btn lf-state",
                            class: if on { "on" },
                            class: if on && primary { "pri" },
                            "aria-pressed": "{on}",
                            disabled,
                            onclick: move |_| {
                                if *value.peek() != option {
                                    value.set(option.clone());
                                    if let Some(on_change) = on_change {
                                        on_change(option.clone());
                                    }
                                }
                            },
                            if let Some(icon) = icon {
                                Icon { name: icon, size: 20 }
                            }
                            "{text}"
                        }
                    }
                }
            }
        }
    }
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
    let on = value();
    rsx! {
        button {
            r#type: "button",
            class: "lf-sw",
            class: if on { "on" },
            role: "switch",
            "aria-checked": "{on}",
            "aria-label": label,
            disabled,
            onclick: move |_| {
                let next = !value();
                value.set(next);
                if let Some(on_change) = on_change {
                    on_change(next);
                }
            },
        }
    }
}

/// A checkbox, only in multi-select lists.
#[component]
pub fn Checkbox(value: Signal<bool>, label: String, #[props(default)] disabled: bool) -> Element {
    let on = value();
    rsx! {
        button {
            r#type: "button",
            class: "lf-cb lf-state",
            class: if on { "on" },
            role: "checkbox",
            "aria-checked": "{on}",
            "aria-label": label,
            disabled,
            onclick: move |_| value.toggle(),
            span { class: "box",
                if on {
                    Icon { name: "check", size: 16 }
                }
            }
        }
    }
}

/// Decimals to show for a step (`0.05` → 2, `1` → 0).
fn decimals(step: f64) -> usize {
    let text = format!("{:.6}", step.abs());
    let frac = text.split('.').nth(1).unwrap_or("").trim_end_matches('0');
    frac.len()
}

/// `value` with `decimals`, with the decimal separator of the language.
pub fn format_number(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    match *LANG.read() {
        Lang::Es => text.replace('.', ","),
        Lang::En => text,
    }
}

/// A number typed by the person: `0,75`, `0.75`, `120 %`.
pub fn parse_number(text: &str) -> Option<f64> {
    let clean: String = text
        .trim()
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '-' | '.' | ','))
        .map(|c| if c == ',' { '.' } else { c })
        .collect();
    clean.parse().ok()
}

/// Clamp to `[min, max]` and snap to the step.
fn snap(value: f64, min: f64, max: f64, step: f64) -> f64 {
    let value = value.clamp(min, max);
    if step <= 0.0 {
        return value;
    }
    let snapped = min + ((value - min) / step).round() * step;
    let scale = 10f64.powi(decimals(step) as i32);
    ((snapped * scale).round() / scale).clamp(min, max)
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
    let named = !stops.is_empty();
    let (min, max, step) = if named { (0.0, (stops.len() - 1) as f64, 1.0) } else { (min, max, step) };
    let digits = decimals(step);
    // While the number box has the focus it shows the plain number.
    let mut draft = use_signal(|| None::<String>);
    let v = value();
    let shown = if named {
        stops.get(v.round().max(0.0) as usize).cloned().unwrap_or_default()
    } else if let Some(format) = format {
        format(v)
    } else {
        format_number(v, digits)
    };
    let frac = if max > min { ((v - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
    let mut commit = move |next: f64| {
        let next = snap(next, min, max, step);
        if (next - *value.peek()).abs() > f64::EPSILON {
            value.set(next);
            if let Some(on_change) = on_change {
                on_change(next);
            }
        }
    };
    let stop_count = stops.len();
    let range_label = label.clone();
    let value_text = shown.clone();
    rsx! {
        div { class: "lf-sl", class: if disabled { "off" },
            div { class: "lf-sl-track",
                span { class: "lf-sl-a", style: "width: calc((100% - 16px) * {frac})" }
                span { class: "lf-sl-h" }
                span { class: "lf-sl-i" }
                if named && stop_count > 1 {
                    span { class: "lf-sl-stops", "aria-hidden": "true",
                        for i in 0..stop_count {
                            i { key: "{i}" }
                        }
                    }
                }
                input {
                    class: "lf-sl-range",
                    r#type: "range",
                    min: "{min}",
                    max: "{max}",
                    step: "{step}",
                    value: "{v}",
                    disabled,
                    "aria-label": range_label,
                    "aria-valuetext": value_text,
                    oninput: move |e| {
                        if let Ok(next) = e.value().parse::<f64>() {
                            commit(next);
                        }
                    },
                }
            }
            if named {
                span { class: "lf-sl-v stop", "aria-hidden": "true", "{shown}" }
            } else {
                input {
                    class: "lf-sl-v",
                    r#type: "text",
                    inputmode: "decimal",
                    disabled,
                    "aria-label": label,
                    value: draft().unwrap_or(shown),
                    onfocus: move |_| draft.set(Some(format_number(*value.peek(), digits))),
                    oninput: move |e| draft.set(Some(e.value())),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            if let Some(next) = draft.peek().as_deref().and_then(parse_number) {
                                commit(next);
                            }
                            draft.set(Some(format_number(*value.peek(), digits)));
                        } else if e.key() == Key::Escape {
                            draft.set(Some(format_number(*value.peek(), digits)));
                        }
                    },
                    onblur: move |_| {
                        if let Some(next) = draft.peek().as_deref().and_then(parse_number) {
                            commit(next);
                        }
                        draft.set(None);
                    },
                }
            }
        }
    }
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
    let mut open = use_signal(|| false);
    let current = value.read().clone();
    let shown = options
        .iter()
        .find(|(o, _)| *o == current)
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    let entries: Vec<MenuEntry> = options
        .iter()
        .map(|(o, text)| MenuEntry {
            label: text.clone(),
            icon: None,
            danger: false,
            disabled: false,
            divider_before: false,
            selected: *o == current,
        })
        .collect();
    let values: Vec<T> = options.into_iter().map(|(o, _)| o).collect();
    rsx! {
        span { class: "lf-menu-anchor start",
            button {
                r#type: "button",
                class: "lf-dd lf-state",
                "aria-haspopup": "listbox",
                "aria-expanded": "{open}",
                "aria-label": "{label}: {shown}",
                disabled,
                onclick: move |_| open.toggle(),
                onkeydown: move |e| {
                    if matches!(e.key(), Key::ArrowDown | Key::ArrowUp) {
                        e.prevent_default();
                        open.set(true);
                    }
                },
                span { class: "val", "{shown}" }
                Icon { name: "arrow_drop_down" }
            }
            MenuPopup {
                open,
                entries,
                at: None,
                role: "option",
                on_pick: move |i: usize| {
                    let next = values[i].clone();
                    if *value.peek() != next {
                        value.set(next.clone());
                        if let Some(on_change) = on_change {
                            on_change(next);
                        }
                    }
                },
            }
        }
    }
}

/// A text field (`.fld`), with inline validation.
///
/// With a `placeholder` the field is compact (48 px, the search field of
/// Inicio) and `label` is only its `aria-label`; without one, `label`
/// floats inside the 56 px field.
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
    let id = use_dom_id();
    let msg_id = format!("{id}-m");
    let floating = placeholder.is_none();
    let has_error = error.is_some();
    let input = rsx! {
        input {
            id: "{id}",
            r#type: "text",
            value: "{value}",
            disabled,
            spellcheck: "false",
            // A space keeps `:placeholder-shown` working for the floating label.
            placeholder: placeholder.clone().unwrap_or_else(|| " ".into()),
            "aria-label": (!floating).then(|| label.clone()),
            "aria-invalid": has_error.then_some("true"),
            "aria-describedby": has_error.then(|| msg_id.clone()),
            onmounted: move |e| async move {
                if autofocus {
                    let _ = e.set_focus(true).await;
                }
            },
            oninput: move |e| {
                let text = e.value();
                value.set(text.clone());
                if let Some(on_change) = on_change {
                    on_change(text);
                }
            },
            onkeydown: move |e| {
                if e.key() == Key::Enter
                    && let Some(on_submit) = on_submit
                {
                    on_submit(value.peek().clone());
                }
            },
        }
    };
    rsx! {
        div { class: "flex flex-col min-w-0",
            label {
                class: "lf-fld",
                class: if floating { "lbl" },
                class: if has_error { "err" },
                class: if disabled { "off" },
                r#for: "{id}",
                if let Some(icon) = leading_icon {
                    Icon { name: icon }
                }
                if floating {
                    span { class: "lf-fld-in",
                        {input}
                        span { class: "lf-fld-label", "{label}" }
                    }
                } else {
                    {input}
                }
                if let Some(shortcut) = shortcut {
                    span { class: "lf-kbd soft hint", "aria-hidden": "true", "{shortcut}" }
                }
            }
            if let Some(error) = error {
                span { id: "{msg_id}", class: "lf-fld-msg err", role: "alert", "{error}" }
            }
        }
    }
}

/// A path with a «Elegir…» button. The webview cannot return paths, so the
/// picker is native (front A asks for `rfd` if it uses it) or the field is
/// text only.
///
/// The picker is the native one Dioxus desktop already opens for an
/// `<input type="file">` (it uses `rfd` inside and gives the full path), so
/// no new dependency is needed.
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
    let accept: String = filters
        .iter()
        .flat_map(|(_, exts)| exts.iter().map(|e| format!(".{}", e.trim_start_matches('.'))))
        .collect::<Vec<_>>()
        .join(",");
    let mut set = move |text: String| {
        value.set(text.clone());
        if let Some(on_change) = on_change {
            on_change(text);
        }
    };
    let (field_icon, pick_icon) = if directory { ("folder", "folder_open") } else { ("description", "upload_file") };
    rsx! {
        div { class: "flex items-center gap-2 min-w-0 flex-1",
            label { class: "lf-fld flex-1 min-w-[200px]", class: if disabled { "off" },
                Icon { name: field_icon }
                input {
                    r#type: "text",
                    class: "mono",
                    style: "font-size: 13px",
                    value: "{value}",
                    disabled,
                    spellcheck: "false",
                    "aria-label": label.clone(),
                    onchange: move |e| set(e.value()),
                }
            }
            span { class: "lf-btn tonal lf-state lf-file",
                Icon { name: pick_icon, size: 20 }
                {t!("cmp-path-choose")}
                input {
                    r#type: "file",
                    disabled,
                    accept: (!accept.is_empty()).then_some(accept.as_str()),
                    "webkitdirectory": directory.then_some("true"),
                    "aria-label": t!("cmp-path-choose-for", label = label.clone()),
                    onchange: move |e| {
                        if let Some(file) = e.files().first() {
                            set(file.path().to_string_lossy().into_owned());
                        }
                    },
                }
            }
            if !value.read().is_empty() {
                super::IconButton {
                    icon: "backspace",
                    label: t!("cmp-path-clear"),
                    size: 32,
                    disabled,
                    onclick: move |_| set(String::new()),
                }
            }
        }
    }
}

/// Whether `text` is `#RRGGBB`.
fn is_hex(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
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
    let mut draft = use_signal(|| None::<String>);
    let mut set = move |text: String| {
        let text = text.to_uppercase();
        if is_hex(&text) && *value.peek().to_uppercase() != text {
            value.set(text.clone());
            if let Some(on_change) = on_change {
                on_change(text);
            }
        }
    };
    let current = value.read().to_uppercase();
    rsx! {
        div { class: "flex items-center gap-3 flex-wrap", role: "group", "aria-label": label.clone(),
            if !swatches.is_empty() {
                div { class: "flex flex-wrap gap-1.5 max-w-[360px]",
                    for swatch in swatches {
                        {
                            let on = swatch.to_uppercase() == current;
                            let pick = swatch.clone();
                            rsx! {
                                button {
                                    key: "{swatch}",
                                    r#type: "button",
                                    class: "lf-swatch",
                                    class: if on { "on" },
                                    style: "background-color: {swatch}",
                                    "aria-label": "{swatch}",
                                    "aria-pressed": "{on}",
                                    disabled,
                                    onclick: move |_| set(pick.clone()),
                                }
                            }
                        }
                    }
                }
            }
            input {
                class: "lf-color-input",
                r#type: "color",
                value: if is_hex(&current) { current.to_lowercase() } else { "#000000".to_string() },
                disabled,
                "aria-label": t!("cmp-color-pick", label = label.clone()),
                onchange: move |e| set(e.value()),
            }
            input {
                class: "lf-sl-v mono",
                style: "width: 104px",
                r#type: "text",
                maxlength: "7",
                spellcheck: "false",
                disabled,
                "aria-label": t!("cmp-color-hex", label = label),
                "aria-invalid": draft().as_deref().is_some_and(|d| !is_hex(&d.to_uppercase())).then_some("true"),
                value: draft().unwrap_or(current),
                oninput: move |e| {
                    let mut text = e.value();
                    if !text.starts_with('#') {
                        text.insert(0, '#');
                    }
                    if is_hex(&text.to_uppercase()) {
                        set(text.clone());
                    }
                    draft.set(Some(text));
                },
                onblur: move |_| draft.set(None),
            }
        }
    }
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
    let mut options = vec![(String::new(), t!("cmp-font-bundled"))];
    options.extend(families.into_iter().filter(|f| !f.is_empty()).map(|f| (f.clone(), f)));
    let family = value.read().clone();
    let sample = if family.is_empty() { "var(--font-brand)".to_string() } else { format!("\"{family}\"") };
    rsx! {
        div { class: "flex items-center gap-3 flex-wrap",
            Select { options, value, label, disabled, on_change }
            span { class: "t-bl text-on-surface-variant", style: "font-family: {sample}", "aria-hidden": "true", "Aa Ñ 0123" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(decimals(0.05), 2);
        assert_eq!(decimals(1.0), 0);
        assert_eq!(decimals(0.1), 1);
        assert_eq!(parse_number("0,75"), Some(0.75));
        assert_eq!(parse_number(" 120 %"), Some(120.0));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(snap(0.333, 0.0, 1.0, 0.05), 0.35);
        assert_eq!(snap(7.0, 0.0, 2.0, 0.5), 2.0);
        assert_eq!(snap(-1.0, 0.0, 2.0, 0.5), 0.0);
        assert!(is_hex("#2F5F9E"));
        assert!(!is_hex("#2F5F9"));
    }
}
