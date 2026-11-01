//! One setting row: a [`SettingRow`] with its origin (inherited, changed,
//! unsaved) and the control for its key, wired to `SetField` / `Inherit`.
//!
//! Rows are described by [`RowSpec`] tables (the tabs, `tabs.rs`); the rows
//! whose options come from the store (mode, monitor, GPU) or that need more
//! than one control (geometry) are their own components.

use dioxus::prelude::*;
use lirtup_config::Value;

use crate::app::{use_dispatch, use_store};
use crate::components::{
    Badge, Button, ButtonVariant, ColorField, Connected, PathField, Select, SettingRow, Slider, Switch,
    TextField,
};
use crate::i18n::tr;
use crate::route::{ProfileRef, Route};
use crate::state::store::{format_value, is_global};
use crate::state::{Action, Origin, Scope};

/// How a row edits its key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    /// A switch.
    Switch,
    /// A dropdown of YAML strings and the Fluent keys of their labels.
    Select(&'static [(&'static str, &'static str)]),
    /// Connected buttons, same as `Select`.
    Connected(&'static [(&'static str, &'static str)]),
    /// A slider. The file keeps `value × scale` (milliseconds shown as
    /// seconds: `scale = 1000`).
    Number(Num),
    /// A switch and a slider for an optional number (`hide_cursor`,
    /// `max_fps`, `scale_factor`): off = no value.
    Optional(Num),
    /// Free text.
    Text,
    /// A path; `true` for a folder. Empty = no value.
    Path(bool),
    /// A `#RRGGBB(AA)` color.
    Color,
}

/// The range of a number row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Num {
    /// Shown minimum.
    pub min: f64,
    /// Shown maximum.
    pub max: f64,
    /// Shown step.
    pub step: f64,
    /// File value = shown value × `scale`.
    pub scale: f64,
    /// The file keeps an integer.
    pub int: bool,
    /// What switching an [`Control::Optional`] on sets (shown units).
    pub on: f64,
}

impl Num {
    /// An integer range.
    pub const fn int(min: f64, max: f64, step: f64) -> Num {
        Num {
            min,
            max,
            step,
            scale: 1.0,
            int: true,
            on: min,
        }
    }

    /// A float range.
    pub const fn float(min: f64, max: f64, step: f64) -> Num {
        Num {
            min,
            max,
            step,
            scale: 1.0,
            int: false,
            on: min,
        }
    }

    /// Shown as file / `scale`.
    pub const fn scaled(self, scale: f64) -> Num {
        Num { scale, ..self }
    }

    /// The value an optional row starts at.
    pub const fn starting(self, on: f64) -> Num {
        Num { on, ..self }
    }

    fn shown(&self, value: &Value) -> Option<f64> {
        value.as_f64().map(|v| v / self.scale)
    }

    fn to_value(self, shown: f64) -> Value {
        let raw = shown.clamp(self.min, self.max) * self.scale;
        if self.int {
            Value::from(raw.round() as i64)
        } else {
            // Keep a short decimal in the file (0.75, not 0.7500000001).
            Value::from((raw * 1e6).round() / 1e6)
        }
    }
}

/// One row of a tab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowSpec {
    /// YAML key.
    pub key: &'static str,
    /// Material Symbols name.
    pub icon: &'static str,
    /// Fluent key of the title.
    pub title: &'static str,
    /// Fluent key of the help text.
    pub subtitle: Option<&'static str>,
    /// The control.
    pub control: Control,
    /// An «Experimental» badge.
    pub experimental: bool,
}

impl RowSpec {
    /// A row without help text.
    pub const fn new(
        key: &'static str,
        icon: &'static str,
        title: &'static str,
        control: Control,
    ) -> RowSpec {
        RowSpec {
            key,
            icon,
            title,
            subtitle: None,
            control,
            experimental: false,
        }
    }

    /// With help text.
    pub const fn sub(self, subtitle: &'static str) -> RowSpec {
        RowSpec {
            subtitle: Some(subtitle),
            ..self
        }
    }

    /// With the «Experimental» badge.
    pub const fn experimental(self) -> RowSpec {
        RowSpec {
            experimental: true,
            ..self
        }
    }

    /// The value as «antes: …» prints it.
    fn format(&self, value: &Value) -> String {
        match self.control {
            Control::Switch => tr(if value.as_bool().unwrap_or(false) {
                "profile-on"
            } else {
                "profile-off"
            }),
            Control::Select(options) | Control::Connected(options) => options
                .iter()
                .find(|(v, _)| value.as_str() == Some(*v))
                .map(|(_, label)| tr(label))
                .unwrap_or_else(|| format_value(value)),
            Control::Number(num) | Control::Optional(num) => match num.shown(value) {
                Some(v) => format_number(v, num.step),
                None => tr("profile-off"),
            },
            _ => format_value(value),
        }
    }
}

/// A number with the decimals of `step`, in the current language's decimal
/// separator.
pub fn format_number(value: f64, step: f64) -> String {
    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10()).ceil().clamp(0.0, 4.0) as usize
    };
    let text = format!("{value:.decimals$}");
    match *crate::i18n::LANG.read() {
        crate::i18n::Lang::Es => text.replace('.', ","),
        crate::i18n::Lang::En => text,
    }
}

/// A signal that follows `value` from the store (the components take
/// `Signal`s; the store is the truth). A change from the store overwrites
/// what the control holds; a change from the control goes out through
/// `on_change` and comes back as the same value.
pub fn use_synced<T: Clone + PartialEq + 'static>(value: T) -> Signal<T> {
    let mut signal = use_signal(|| value.clone());
    let mut last = use_signal(|| value.clone());
    if *last.peek() != value {
        last.set(value.clone());
        signal.set(value);
    }
    signal
}

/// What a row needs from the store, read once per render.
pub struct RowState {
    /// The origin for the stripe.
    pub origin: Origin,
    /// The value shown.
    pub value: Value,
    /// The key is global.
    pub global: bool,
    /// «Volver a heredar» applies (a named profile, not a global key).
    pub can_inherit: bool,
}

/// Read one row's state.
pub fn row_state(scope: &Scope, key: &str, format: impl Fn(&Value) -> String) -> RowState {
    let store = use_store();
    let s = store.read();
    let field = s.field::<Value>(scope, key);
    let global = is_global(key);
    RowState {
        origin: field.origin(format),
        value: field.value(),
        global,
        can_inherit: matches!(scope, Scope::Profile(ProfileRef::Named(_))) && !global,
    }
}

/// The «Volver a heredar» handler of a row.
pub fn inherit_handler(scope: &Scope, key: &'static str, can: bool) -> Option<EventHandler<()>> {
    let dispatch = use_dispatch();
    let scope = scope.clone();
    can.then(|| {
        EventHandler::new(move |_| {
            dispatch(Action::Inherit {
                scope: scope.clone(),
                key: key.into(),
            })
        })
    })
}

/// A row from a [`RowSpec`].
#[component]
pub fn Row(scope: Scope, spec: &'static RowSpec) -> Element {
    let dispatch = use_dispatch();
    let state = row_state(&scope, spec.key, |v| spec.format(v));
    let on_inherit = inherit_handler(&scope, spec.key, state.can_inherit);
    let title = tr(spec.title);
    let set = {
        let scope = scope.clone();
        move |value: Value| {
            dispatch(Action::SetField {
                scope: scope.clone(),
                key: spec.key.into(),
                value,
            })
        }
    };
    let inherit = {
        let scope = scope.clone();
        move || {
            dispatch(Action::Inherit {
                scope: scope.clone(),
                key: spec.key.into(),
            })
        }
    };
    let control = match spec.control {
        Control::Switch => rsx! {
            SwitchControl { value: state.value.as_bool().unwrap_or(false), label: title.clone(), on_change: move |v: bool| set(Value::Bool(v)) }
        },
        Control::Select(options) => rsx! {
            SelectControl {
                options: labels(options),
                value: state.value.as_str().unwrap_or_default().to_owned(),
                label: title.clone(),
                on_change: move |v: String| set(Value::String(v)),
            }
        },
        Control::Connected(options) => rsx! {
            ConnectedControl {
                options: labels(options),
                value: state.value.as_str().unwrap_or_default().to_owned(),
                label: title.clone(),
                on_change: move |v: String| set(Value::String(v)),
            }
        },
        Control::Number(num) => rsx! {
            NumberControl {
                num,
                value: num.shown(&state.value).unwrap_or(num.min),
                label: title.clone(),
                on_change: move |v: f64| set(num.to_value(v)),
            }
        },
        Control::Optional(num) => {
            let shown = num.shown(&state.value);
            let set_on = set.clone();
            rsx! {
                NumberControl {
                    num,
                    value: shown.unwrap_or(num.on),
                    label: title.clone(),
                    disabled: shown.is_none(),
                    on_change: move |v: f64| set(num.to_value(v)),
                }
                SwitchControl {
                    value: shown.is_some(),
                    label: title.clone(),
                    on_change: move |on: bool| if on { set_on(num.to_value(num.on)) } else { inherit() },
                }
            }
        }
        Control::Text => rsx! {
            TextControl {
                value: text_of(&state.value),
                label: title.clone(),
                on_change: move |v: String| if v.is_empty() { inherit() } else { set(Value::String(v)) },
            }
        },
        Control::Path(directory) => rsx! {
            PathControl {
                value: text_of(&state.value),
                label: title.clone(),
                directory,
                on_change: move |v: String| if v.trim().is_empty() { inherit() } else { set(Value::String(v)) },
            }
        },
        Control::Color => {
            let hex = serde_yaml_ng::from_value::<lirtup_config::BackgroundColor>(state.value.clone())
                .map(|c| c.to_hex())
                .unwrap_or_else(|_| text_of(&state.value));
            rsx! {
                ColorControl { value: hex, label: title.clone(), on_change: move |v: String| set(Value::String(v)) }
            }
        }
    };
    rsx! {
        SettingRow {
            icon: spec.icon,
            title,
            subtitle: spec.subtitle.map(tr),
            origin: state.origin,
            on_inherit,
            global: state.global,
            if spec.experimental {
                Badge { label: tr("profile-experimental") }
            }
            {control}
        }
    }
}

fn labels(options: &'static [(&'static str, &'static str)]) -> Vec<(String, String)> {
    options.iter().map(|(v, l)| ((*v).to_owned(), tr(l))).collect()
}

fn text_of(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => format_value(other),
    }
}

/// A switch that follows the store.
#[component]
pub fn SwitchControl(
    value: bool,
    label: String,
    #[props(default)] disabled: bool,
    on_change: EventHandler<bool>,
) -> Element {
    let signal = use_synced(value);
    rsx! { Switch { value: signal, label, disabled, on_change } }
}

/// A dropdown that follows the store.
#[component]
pub fn SelectControl(
    options: Vec<(String, String)>,
    value: String,
    label: String,
    #[props(default)] disabled: bool,
    on_change: EventHandler<String>,
) -> Element {
    let signal = use_synced(value);
    rsx! { Select { options, value: signal, label, disabled, on_change } }
}

/// Connected buttons that follow the store.
#[component]
pub fn ConnectedControl(
    options: Vec<(String, String)>,
    value: String,
    label: String,
    #[props(default)] icons: Vec<Option<&'static str>>,
    #[props(default)] primary: bool,
    #[props(default)] disabled: bool,
    on_change: EventHandler<String>,
) -> Element {
    let signal = use_synced(value);
    rsx! { Connected { options, value: signal, label, icons, primary, disabled, on_change } }
}

/// A slider that follows the store.
#[component]
pub fn NumberControl(
    num: Num,
    value: f64,
    label: String,
    #[props(default)] disabled: bool,
    on_change: EventHandler<f64>,
) -> Element {
    let signal = use_synced(value);
    let step = num.step;
    rsx! {
        Slider {
            value: signal,
            min: num.min,
            max: num.max,
            step,
            label,
            format: Callback::new(move |v: f64| format_number(v, step)),
            disabled,
            on_change,
        }
    }
}

/// A text field that follows the store; edits go out on Enter and blur
/// (the component's `on_change`).
#[component]
pub fn TextControl(
    value: String,
    label: String,
    error: Option<String>,
    #[props(default)] disabled: bool,
    on_change: EventHandler<String>,
) -> Element {
    let signal = use_synced(value);
    rsx! { TextField { value: signal, label, error, disabled, on_change } }
}

/// A path field that follows the store.
#[component]
fn PathControl(value: String, label: String, directory: bool, on_change: EventHandler<String>) -> Element {
    let signal = use_synced(value);
    rsx! { PathField { value: signal, label, directory, on_change } }
}

/// A color field that follows the store.
#[component]
fn ColorControl(value: String, label: String, on_change: EventHandler<String>) -> Element {
    let signal = use_synced(value);
    rsx! { ColorField { value: signal, label, on_change } }
}

/// Escalado ▸ the mode: a dropdown of the modes plus «Editar modo».
#[component]
pub fn ModeRow(scope: Scope) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let state = row_state(&scope, "mode", |v| {
        let id = v.as_str().unwrap_or_default();
        store
            .peek()
            .modes
            .store
            .get(id)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| tr("profile-mode-none"))
    });
    let on_inherit = inherit_handler(&scope, "mode", state.can_inherit);
    let s = store.read();
    let current = state.value.as_str().unwrap_or_default().to_owned();
    let missing = !current.is_empty() && s.modes.store.get(&current).is_none();
    // "" = no value in this layer.
    let mut options = vec![(
        String::new(),
        tr(if matches!(scope, Scope::Profile(ProfileRef::Named(_))) {
            "profile-mode-inherit"
        } else {
            "profile-mode-none"
        }),
    )];
    options.extend(
        s.modes
            .store
            .modes
            .iter()
            .map(|(id, m)| (id.clone(), m.name.clone())),
    );
    if missing {
        options.push((current.clone(), current.clone()));
    }
    // What the profile ends up with (its own or Predeterminado's).
    let used = match &scope {
        Scope::Profile(p) => s.mode_of(p),
        Scope::Mode(_) => None,
    };
    let own =
        s.field::<Value>(&scope, "mode").profile.is_some() || s.field::<Value>(&scope, "mode").edit.is_some();
    let loose = match &scope {
        Scope::Profile(ProfileRef::Named(name)) => s.config.file.profiles.get(name).map_or(0, |p| {
            p.options
                .keys()
                .filter_map(Value::as_str)
                .filter(|k| lirtup_config::MODE_KEYS.contains(k))
                .count()
        }),
        _ => s
            .config
            .file
            .general
            .keys()
            .filter_map(Value::as_str)
            .filter(|k| lirtup_config::MODE_KEYS.contains(k))
            .count(),
    };
    drop(s);
    let selected = if own { current.clone() } else { String::new() };
    let subtitle = if missing {
        Some(crate::i18n::tr_args(
            "profile-mode-missing",
            &[("id", crate::i18n::arg(current.clone()))],
        ))
    } else if loose > 0 {
        Some(crate::i18n::tr_args(
            "profile-loose-mode-fields",
            &[("count", crate::i18n::arg(loose as i64))],
        ))
    } else {
        None
    };
    let set_scope = scope.clone();
    rsx! {
        SettingRow {
            icon: "tune",
            title: tr("profile-mode"),
            subtitle,
            origin: state.origin,
            on_inherit,
            SelectControl {
                options,
                value: selected,
                label: tr("profile-mode"),
                on_change: move |id: String| {
                    let scope = set_scope.clone();
                    if id.is_empty() {
                        dispatch(Action::Inherit { scope, key: "mode".into() });
                    } else {
                        dispatch(Action::SetField { scope, key: "mode".into(), value: Value::String(id) });
                    }
                },
            }
            if let Some(id) = used {
                Button {
                    label: tr("profile-mode-edit"),
                    variant: ButtonVariant::Text,
                    onclick: move |_| dispatch(Action::Navigate(Route::Modes { id: Some(id.clone()) })),
                }
            }
        }
    }
}

/// Pantalla ▸ Monitor: `primary`, `all` and the monitors of `ListDevices`
/// (by RandR name; an index in the file shows as its monitor).
#[component]
pub fn MonitorRow(scope: Scope) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let monitors = store.read().monitors.clone();
    let name_of = {
        let monitors = monitors.clone();
        move |v: &Value| -> String {
            match v {
                Value::Number(n) => n
                    .as_u64()
                    .and_then(|i| monitors.iter().find(|m| u64::from(m.index) == i))
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| n.to_string()),
                Value::String(s) if s == "primary" => tr("profile-monitor-primary"),
                Value::String(s) if s == "all" => tr("profile-monitor-all"),
                other => text_of(other),
            }
        }
    };
    let state = row_state(&scope, "monitor", name_of);
    let on_inherit = inherit_handler(&scope, "monitor", state.can_inherit);
    let value = match &state.value {
        Value::Number(n) => n
            .as_u64()
            .and_then(|i| monitors.iter().find(|m| u64::from(m.index) == i))
            .map(|m| m.name.clone())
            .unwrap_or_else(|| n.to_string()),
        other => text_of(other),
    };
    let mut options = vec![
        ("primary".to_owned(), tr("profile-monitor-primary")),
        ("all".to_owned(), tr("profile-monitor-all")),
    ];
    options.extend(monitors.iter().map(|m| {
        let label = format!("{} · {}×{}", m.name, m.width, m.height);
        (m.name.clone(), label)
    }));
    if !options.iter().any(|(v, _)| *v == value) {
        options.push((value.clone(), value.clone()));
    }
    rsx! {
        SettingRow { icon: "monitor", title: tr("profile-monitor"), origin: state.origin, on_inherit,
            SelectControl {
                options,
                value,
                label: tr("profile-monitor"),
                on_change: move |v: String| dispatch(Action::SetField { scope: scope.clone(), key: "monitor".into(), value: Value::String(v) }),
            }
        }
    }
}

/// Pantalla ▸ GPU: automatic or one adapter of `ListDevices`, by name.
#[component]
pub fn GpuRow(scope: Scope) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let gpus = store.read().gpus.clone();
    let state = row_state(&scope, "gpu", |v| match v {
        Value::Null => tr("profile-gpu-auto"),
        other => text_of(other),
    });
    let on_inherit = inherit_handler(&scope, "gpu", state.can_inherit);
    let value = match &state.value {
        Value::Number(n) => n
            .as_u64()
            .and_then(|i| gpus.iter().find(|g| u64::from(g.index) == i))
            .map(|g| g.name.clone())
            .unwrap_or_else(|| n.to_string()),
        other => text_of(other),
    };
    let mut options = vec![(String::new(), tr("profile-gpu-auto"))];
    options.extend(gpus.iter().map(|g| (g.name.clone(), g.name.clone())));
    if !options.iter().any(|(v, _)| *v == value) {
        options.push((value.clone(), value.clone()));
    }
    rsx! {
        SettingRow { icon: "memory", title: tr("profile-gpu"), origin: state.origin, on_inherit,
            SelectControl {
                options,
                value,
                label: tr("profile-gpu"),
                on_change: move |v: String| {
                    let scope = scope.clone();
                    if v.is_empty() {
                        dispatch(Action::Inherit { scope, key: "gpu".into() });
                    } else {
                        dispatch(Action::SetField { scope, key: "gpu".into(), value: Value::String(v) });
                    }
                },
            }
        }
    }
}

const GEOMETRIES: [(&str, &str); 4] = [
    ("fit", "profile-geometry-fit"),
    ("stretch", "profile-geometry-stretch"),
    ("cover", "profile-geometry-cover"),
    ("custom", "profile-geometry-custom"),
];

/// Presentación ▸ geometry: Ajustar / Estirar / Cubrir, or a custom string
/// checked with the scaler's own parser.
#[component]
pub fn GeometryRow(scope: Scope) -> Element {
    let dispatch = use_dispatch();
    let state = row_state(&scope, "output_geometry", |v| {
        let text = text_of(v);
        GEOMETRIES
            .iter()
            .find(|(g, _)| *g == text)
            .map(|(_, l)| tr(l))
            .unwrap_or(text)
    });
    let on_inherit = inherit_handler(&scope, "output_geometry", state.can_inherit);
    let text = text_of(&state.value);
    let preset = ["fit", "stretch", "cover"].contains(&text.as_str());
    let mut custom = use_signal(|| !preset);
    let shown = if preset && !custom() {
        text.clone()
    } else {
        "custom".to_owned()
    };
    let mut error = use_signal(|| None::<String>);
    let set = {
        let scope = scope.clone();
        move |v: String| {
            dispatch(Action::SetField {
                scope: scope.clone(),
                key: "output_geometry".into(),
                value: Value::String(v),
            })
        }
    };
    let set_preset = set.clone();
    rsx! {
        SettingRow { icon: "aspect_ratio", title: tr("profile-geometry"), origin: state.origin, on_inherit,
            if shown == "custom" {
                TextControl {
                    value: text.clone(),
                    label: tr("profile-geometry-custom-label"),
                    error: error(),
                    on_change: move |v: String| {
                        let v = v.trim().to_owned();
                        match lirtup_config::OutputGeometry(v.clone()).parse() {
                            Ok(_) => {
                                error.set(None);
                                set(v);
                            }
                            Err(_) => error.set(Some(tr("profile-geometry-invalid"))),
                        }
                    },
                }
            }
            ConnectedControl {
                options: labels(&GEOMETRIES),
                value: shown.clone(),
                label: tr("profile-geometry"),
                on_change: move |v: String| {
                    if v == "custom" {
                        custom.set(true);
                    } else {
                        custom.set(false);
                        error.set(None);
                        set_preset(v);
                    }
                },
            }
        }
    }
}

/// A small «Global» note for a group title.
#[component]
pub fn GlobalNote() -> Element {
    rsx! {
        span { class: "inline-flex items-center gap-2",
            Badge { label: tr("profile-global") }
            span { class: "t-bs text-on-surface-variant", {tr("profile-global-note")} }
        }
    }
}
