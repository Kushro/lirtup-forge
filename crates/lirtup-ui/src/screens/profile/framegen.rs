//! Presentación ▸ Generación de cuadros (flow 5). Global: its keys are in
//! [`crate::state::store::global_keys`], so edits from any profile go to
//! *Predeterminado*. The card reads `FrameGenStatus` from the core and
//! translates `reason_code`; with `LSFGVK_*` variables the section is
//! disabled. The settings are read when a session starts, so with a
//! session running the section offers «Guardar y reiniciar».

use dioxus::prelude::*;
use lirtup_config::{FrameGenEngine, FrameGenMultiplier, Value};
use lirtup_proto::{FrameGenReason, FrameGenState, FrameGenStatus};

use super::rows::{ConnectedControl, GlobalNote, Num, NumberControl, inherit_handler, row_state};
use crate::app::{use_dispatch, use_store};
use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Icon, PathField, SettingRow};
use crate::i18n::{arg, tr, tr_args};
use crate::state::{Action, Dialog, Scope, SessionState};

/// The keys of the section (all global).
pub const KEYS: [&str; 5] = [
    "frame_gen_engine",
    "frame_gen_multiplier",
    "frame_gen_flow_scale",
    "frame_gen_performance",
    "frame_gen_dll",
];

/// The Fluent key of a reason code.
pub fn reason_key(code: FrameGenReason) -> &'static str {
    match code {
        FrameGenReason::LayerMissing => "fg-reason-layer-missing",
        FrameGenReason::LosslessDllMissing => "fg-reason-lossless-dll-missing",
        FrameGenReason::LsfgVkDllMissing => "fg-reason-lsfg-vk-dll-missing",
        FrameGenReason::EnvLocked => "fg-reason-env-locked",
        FrameGenReason::NoExternalFd => "fg-reason-no-external-fd",
        FrameGenReason::EngineFailed => "fg-reason-engine-failed",
    }
}

/// Why frame generation is not running, translated; `None` when off or
/// active. A core without `reason_code` (older) falls back on the state.
pub fn reason_text(status: &FrameGenStatus) -> Option<String> {
    let code = status.reason_code.or(match status.state {
        FrameGenState::NoLayer => Some(FrameGenReason::LayerMissing),
        FrameGenState::External => Some(FrameGenReason::EnvLocked),
        FrameGenState::NoDll => Some(match status.engine {
            FrameGenEngine::Layer => FrameGenReason::LsfgVkDllMissing,
            FrameGenEngine::Native => FrameGenReason::LosslessDllMissing,
        }),
        FrameGenState::NoNative => Some(FrameGenReason::NoExternalFd),
        FrameGenState::Off | FrameGenState::Active => None,
    })?;
    Some(tr(reason_key(code)))
}

/// Whether the configured engine can run.
fn engine_ready(status: &FrameGenStatus, engine: FrameGenEngine) -> bool {
    match engine {
        FrameGenEngine::Layer => status.layer_installed && status.dll_found,
        FrameGenEngine::Native => status.native_available,
    }
}

const ENGINES: [(&str, &str); 2] = [("layer", "fg-engine-layer"), ("native", "fg-engine-native")];

const FLOW: Num = Num::float(0.25, 1.0, 0.05);

/// The section.
#[component]
pub fn FrameGenSection(scope: Scope) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let s = store.read();
    let status = s.framegen.clone();
    let running = matches!(s.session, SessionState::Running(_));
    let engine: FrameGenEngine = s.field(&scope, "frame_gen_engine").value();
    let multiplier: FrameGenMultiplier = s.field(&scope, "frame_gen_multiplier").value();
    let flow: f32 = s.field(&scope, "frame_gen_flow_scale").value();
    let performance: bool = s.field(&scope, "frame_gen_performance").value();
    let dll: Option<std::path::PathBuf> = s.field(&scope, "frame_gen_dll").value();
    let dirty = KEYS.iter().any(|k| s.field::<Value>(&scope, k).is_dirty());
    drop(s);

    let locked = status.env_locked;
    let ready = engine_ready(&status, engine);
    let reason = reason_text(&status);
    let title = match (status.state, status.active, multiplier) {
        (FrameGenState::Active, _, FrameGenMultiplier::Auto) => tr("fg-active-auto"),
        (FrameGenState::Active, Some(n), _) => tr_args("fg-active", &[("n", arg(i64::from(n)))]),
        (_, _, FrameGenMultiplier::Fixed(1)) => tr("fg-off"),
        (_, _, m) => tr_args("fg-configured", &[("value", arg(multiplier_label(m)))]),
    };
    let subtitle = match (&reason, running) {
        (Some(reason), _) => reason.clone(),
        (None, false) if multiplier.is_on() => tr("fg-not-running"),
        _ => tr("fg-sub"),
    };

    let set = {
        let scope = scope.clone();
        move |key: &'static str, value: Value| {
            dispatch(Action::SetField {
                scope: scope.clone(),
                key: key.into(),
                value,
            })
        }
    };
    let (set1, set2, set3, set4, set5) = (set.clone(), set.clone(), set.clone(), set.clone(), set);

    let engine_state = row_state(&scope, "frame_gen_engine", |v| {
        v.as_str().unwrap_or_default().to_owned()
    });
    let mult_state = row_state(&scope, "frame_gen_multiplier", |v| match v {
        Value::String(s) => s.clone(),
        other => format!("×{}", other.as_u64().unwrap_or(1)),
    });
    let flow_state = row_state(&scope, "frame_gen_flow_scale", |v| {
        super::rows::format_number(v.as_f64().unwrap_or(1.0), 0.05)
    });
    let model_state = row_state(&scope, "frame_gen_performance", |v| {
        tr(if v.as_bool().unwrap_or(false) {
            "fg-model-performance"
        } else {
            "fg-model-quality"
        })
    });
    let dll_state = row_state(&scope, "frame_gen_dll", |v| {
        v.as_str().unwrap_or_default().to_owned()
    });
    // Global rows have no «volver a heredar» on a profile page.
    let no_inherit = inherit_handler(&scope, "frame_gen_engine", false);

    let multipliers = vec![
        ("1".to_owned(), tr("fg-multiplier-off")),
        ("2".to_owned(), "×2".to_owned()),
        ("3".to_owned(), "×3".to_owned()),
        ("4".to_owned(), "×4".to_owned()),
        ("auto".to_owned(), tr("fg-multiplier-auto")),
    ];
    let mult_value = match multiplier {
        FrameGenMultiplier::Auto => "auto".to_owned(),
        FrameGenMultiplier::Fixed(n) => n.to_string(),
    };
    let engine_value = match engine {
        FrameGenEngine::Layer => "layer",
        FrameGenEngine::Native => "native",
    };
    let model_value = if performance { "performance" } else { "quality" };
    let auto_on_layer = multiplier == FrameGenMultiplier::Auto && engine == FrameGenEngine::Layer;
    let dll_text = dll.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let dll_signal = super::rows::use_synced(dll_text);

    rsx! {
        section { class: "flex flex-col gap-2",
            div { class: "flex items-center gap-3 flex-wrap px-1",
                h2 { class: "t-ts", {tr("fg-title")} }
                GlobalNote {}
            }
            div { class: "flex flex-col gap-4 p-5 rounded-lg bg-surface-container",
                div { class: "flex items-center gap-4",
                    Icon { name: "animation", size: 32 }
                    div { class: "flex-1 min-w-0 flex flex-col",
                        span { class: "t-tm", "{title}" }
                        span { class: "t-bs text-on-surface-variant", "{subtitle}" }
                        if let Some(detail) = status.reason.clone().filter(|_| reason.is_some()) {
                            span { class: "t-bs mono text-on-surface-variant", "{detail}" }
                        }
                    }
                    if ready {
                        Badge { label: tr("fg-ready"), variant: BadgeVariant::Success, icon: "check" }
                    } else {
                        Badge { label: tr("fg-not-ready"), variant: BadgeVariant::Error, icon: "error" }
                    }
                }
                if locked {
                    div { class: "p-3 rounded-md bg-surface-container-highest t-bm", {tr("fg-env-locked")} }
                }
                div { class: "flex flex-col gap-0.5",
                    SettingRow {
                        icon: "memory",
                        title: tr("fg-engine"),
                        origin: engine_state.origin,
                        on_inherit: no_inherit,
                        global: true,
                        ConnectedControl {
                            options: ENGINES.iter().map(|(v, l)| ((*v).to_owned(), tr(l))).collect::<Vec<_>>(),
                            value: engine_value.to_owned(),
                            label: tr("fg-engine"),
                            disabled: locked,
                            on_change: move |v: String| set1("frame_gen_engine", Value::String(v)),
                        }
                    }
                    SettingRow {
                        icon: "animation",
                        title: tr("fg-multiplier"),
                        subtitle: auto_on_layer.then(|| tr("fg-auto-needs-native")),
                        origin: mult_state.origin,
                        global: true,
                        ConnectedControl {
                            options: multipliers,
                            value: mult_value,
                            label: tr("fg-multiplier"),
                            icons: vec![None, None, None, None, Some("auto_awesome")],
                            primary: multiplier == FrameGenMultiplier::Auto,
                            disabled: locked,
                            on_change: move |v: String| {
                                let value = match v.parse::<u64>() {
                                    Ok(n) => Value::from(n),
                                    Err(_) => Value::String(v),
                                };
                                set2("frame_gen_multiplier", value);
                            },
                        }
                    }
                    SettingRow {
                        icon: "blur_on",
                        title: tr("fg-flow-scale"),
                        subtitle: tr("fg-flow-scale-sub"),
                        origin: flow_state.origin,
                        global: true,
                        NumberControl {
                            num: FLOW,
                            value: f64::from(flow),
                            label: tr("fg-flow-scale"),
                            disabled: locked,
                            on_change: move |v: f64| set3("frame_gen_flow_scale", Value::from((v * 100.0).round() / 100.0)),
                        }
                    }
                    SettingRow {
                        icon: "model_training",
                        title: tr("fg-model"),
                        origin: model_state.origin,
                        global: true,
                        ConnectedControl {
                            options: vec![("quality".to_owned(), tr("fg-model-quality")), ("performance".to_owned(), tr("fg-model-performance"))],
                            value: model_value.to_owned(),
                            label: tr("fg-model"),
                            disabled: locked,
                            on_change: move |v: String| set4("frame_gen_performance", Value::Bool(v == "performance")),
                        }
                    }
                    SettingRow {
                        icon: "folder",
                        title: tr("fg-dll"),
                        subtitle: tr("fg-dll-sub"),
                        origin: dll_state.origin,
                        global: true,
                        PathField {
                            value: dll_signal,
                            label: tr("fg-dll"),
                            directory: true,
                            disabled: locked,
                            on_change: {
                                let scope = scope.clone();
                                move |v: String| {
                                    if v.trim().is_empty() {
                                        dispatch(Action::Inherit { scope: scope.clone(), key: "frame_gen_dll".into() });
                                    } else {
                                        set5("frame_gen_dll", Value::String(v));
                                    }
                                }
                            },
                        }
                    }
                }
                div { class: "flex items-center gap-3 flex-wrap",
                    span { class: "t-bs text-on-surface-variant flex-1", {tr("fg-restart-note")} }
                    Button {
                        label: tr("fg-measure"),
                        variant: ButtonVariant::Tonal,
                        icon: "speed",
                        onclick: move |_| dispatch(Action::OpenDialog(Dialog::Bench { mode: None })),
                    }
                    if running && dirty {
                        Button {
                            label: tr("fg-save-restart"),
                            icon: "restart_alt",
                            onclick: move |_| dispatch(Action::SaveAndRestart),
                        }
                    }
                }
            }
        }
    }
}

fn multiplier_label(m: FrameGenMultiplier) -> String {
    match m {
        FrameGenMultiplier::Auto => tr("fg-multiplier-auto"),
        FrameGenMultiplier::Fixed(n) => format!("×{n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_is_translated() {
        use FrameGenReason::*;
        for code in [
            LayerMissing,
            LosslessDllMissing,
            LsfgVkDllMissing,
            EnvLocked,
            NoExternalFd,
            EngineFailed,
        ] {
            for lang in [crate::i18n::Lang::Es, crate::i18n::Lang::En] {
                assert!(crate::i18n::has_key(lang, reason_key(code)), "{lang:?} {code:?}");
            }
        }
    }

    #[test]
    fn reason_falls_back_on_the_state() {
        // Without a Dioxus runtime `tr` cannot read the language; check the
        // codes instead of the text.
        let status = FrameGenStatus {
            state: FrameGenState::NoDll,
            engine: FrameGenEngine::Native,
            ..FrameGenStatus::default()
        };
        assert!(status.reason_code.is_none());
        assert!(engine_ready(
            &FrameGenStatus {
                native_available: true,
                ..status.clone()
            },
            FrameGenEngine::Native
        ));
        assert!(!engine_ready(&status, FrameGenEngine::Layer));
    }
}
