//! The editor of one mode: header (rename, duplicate, delete), the shared
//! mode banner, Modelo, Muestreo, the effects (engine, native passes or the
//! MagpieFX chain, the total and «Medir en mi GPU») and Retoque final.
//!
//! Every row edits `Scope::Mode(id)` with `SetField`, `Inherit` and
//! `Revert`; the save bar of the frame saves (front C's `save_bar_scope`).

use dioxus::prelude::*;
use lirtup_config::{Downsampler, EffectsEngine, MagpieEffectSpec, ModeSettings, Model, Upsampler};

use super::chain::{CHAIN_KEY, EffectChain, chain_names};
use super::costs::{Costs, use_costs};
use super::widgets::{
    CostBadge, Num, NumberRow, SwitchRow, fmt_ms, inherit, use_control, use_origin, use_value, yaml,
};
use crate::app::{use_dispatch, use_store};
use crate::components::{
    Button, ButtonVariant, Connected, IconButton, IconButtonVariant, Select, SettingGroup, SettingRow,
    Slider, Switch, TextField,
};
use crate::i18n::tr;
use crate::route::{ModeId, ProfileRef, Route};
use crate::state::{Action, Dialog, Scope};

/// `LUT_PRESETS` of 1.x (`shaders/effects/lut/presets.py`).
pub(crate) const LUT_PRESETS: [&str; 12] = [
    "identity", "warm", "cool", "split", "vivid", "pastel", "lofi", "bleach", "film", "noir", "sepia",
    "cyano",
];

/// An effect toggled by a switch, with its settings under it while on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Toggle {
    /// The `*_enabled` key.
    pub key: &'static str,
    pub icon: &'static str,
    /// Fluent key of the title.
    pub title: &'static str,
    /// Fluent key of the help text.
    pub subtitle: Option<&'static str>,
    /// Its label in the bench (`cas`, `anime4k`...).
    pub bench: &'static str,
    /// Its settings.
    pub params: &'static [Num],
}

/// The native passes of the «Nativo» engine.
pub(crate) const NATIVE: &[Toggle] = &[
    Toggle {
        key: "anime4k_enabled",
        icon: "auto_awesome",
        title: "modes-anime4k",
        subtitle: Some("modes-anime4k-help"),
        bench: "anime4k",
        params: &[],
    },
    Toggle {
        key: "luma_sharpen_enabled",
        icon: "deblur",
        title: "modes-luma-sharpen",
        subtitle: Some("modes-luma-sharpen-help"),
        bench: "luma_sharpen",
        params: &[
            Num::float(
                "luma_sharpen_strength",
                "tune",
                "modes-luma-sharpen-strength",
                0.0,
                3.0,
                0.05,
            ),
            Num::float(
                "luma_sharpen_clamp",
                "vertical_align_center",
                "modes-luma-sharpen-clamp",
                0.0,
                1.0,
                0.005,
            ),
            Num::int(
                "luma_sharpen_pattern",
                "grid_on",
                "modes-luma-sharpen-pattern",
                0.0,
                3.0,
            ),
            Num::float(
                "luma_sharpen_offset_bias",
                "open_with",
                "modes-luma-sharpen-offset-bias",
                0.0,
                6.0,
                0.1,
            ),
        ],
    },
    Toggle {
        key: "crt_enabled",
        icon: "tv",
        title: "modes-crt",
        subtitle: Some("modes-crt-help"),
        bench: "crt_easymode",
        params: &[
            Num::float(
                "crt_sharpness_h",
                "swap_horiz",
                "modes-crt-sharpness-h",
                0.0,
                1.0,
                0.05,
            ),
            Num::float(
                "crt_sharpness_v",
                "swap_vert",
                "modes-crt-sharpness-v",
                0.0,
                1.0,
                0.05,
            ),
            Num::float(
                "crt_mask_strength",
                "texture",
                "modes-crt-mask-strength",
                0.0,
                1.0,
                0.01,
            ),
            Num::int(
                "crt_mask_dot_width",
                "width",
                "modes-crt-mask-dot-width",
                1.0,
                100.0,
            ),
            Num::int(
                "crt_mask_dot_height",
                "height",
                "modes-crt-mask-dot-height",
                1.0,
                100.0,
            ),
            Num::int(
                "crt_mask_stagger",
                "view_week",
                "modes-crt-mask-stagger",
                0.0,
                100.0,
            ),
            Num::int("crt_mask_size", "zoom_out_map", "modes-crt-mask-size", 1.0, 100.0),
            Num::float(
                "crt_scanline_strength",
                "line_weight",
                "modes-crt-scanline-strength",
                0.0,
                1.0,
                0.05,
            ),
            Num::float(
                "crt_scanline_beam_width_min",
                "line_start",
                "modes-crt-beam-min",
                0.5,
                5.0,
                0.1,
            ),
            Num::float(
                "crt_scanline_beam_width_max",
                "line_end",
                "modes-crt-beam-max",
                0.5,
                5.0,
                0.1,
            ),
            Num::float(
                "crt_scanline_bright_min",
                "brightness_low",
                "modes-crt-bright-min",
                0.0,
                1.0,
                0.05,
            ),
            Num::float(
                "crt_scanline_bright_max",
                "brightness_high",
                "modes-crt-bright-max",
                0.0,
                1.0,
                0.05,
            ),
            Num::int(
                "crt_scanline_cutoff",
                "content_cut",
                "modes-crt-cutoff",
                1.0,
                1000.0,
            ),
            Num::float("crt_gamma_input", "input", "modes-crt-gamma-input", 0.1, 5.0, 0.1),
            Num::float(
                "crt_gamma_output",
                "output",
                "modes-crt-gamma-output",
                0.1,
                5.0,
                0.1,
            ),
            Num::float(
                "crt_bright_boost",
                "exposure_plus_1",
                "modes-crt-bright-boost",
                1.0,
                2.0,
                0.05,
            ),
            Num::int("crt_dilation", "blur_linear", "modes-crt-dilation", 0.0, 1.0),
        ],
    },
];

/// Retoque final (and debanding, which runs before scaling).
pub(crate) const FINAL: &[Toggle] = &[
    Toggle {
        key: "cas_enabled",
        icon: "contrast",
        title: "modes-cas",
        subtitle: Some("modes-cas-help"),
        bench: "cas",
        params: &[Num::float(
            "cas_strength",
            "tune",
            "modes-strength",
            0.0,
            1.0,
            0.05,
        )],
    },
    Toggle {
        key: "deband_enabled",
        icon: "gradient",
        title: "modes-deband",
        subtitle: Some("modes-deband-help"),
        bench: "deband",
        params: &[Num::float(
            "deband_strength",
            "tune",
            "modes-strength",
            0.0,
            1.0,
            0.05,
        )],
    },
    Toggle {
        key: "bloom_enabled",
        icon: "flare",
        title: "modes-bloom",
        subtitle: None,
        bench: "bloom",
        params: &[
            Num::float("bloom_strength", "tune", "modes-strength", 0.0, 1.0, 0.01),
            Num::float(
                "bloom_threshold",
                "brightness_medium",
                "modes-bloom-threshold",
                0.0,
                1.0,
                0.05,
            ),
            Num::int("bloom_radius", "blur_circular", "modes-bloom-radius", 1.0, 16.0),
        ],
    },
    Toggle {
        key: "vignette_enabled",
        icon: "vignette",
        title: "modes-vignette",
        subtitle: None,
        bench: "vignette",
        params: &[
            Num::float("vignette_strength", "tune", "modes-strength", 0.0, 1.0, 0.05),
            Num::float(
                "vignette_radius",
                "radio_button_unchecked",
                "modes-vignette-radius",
                0.0,
                2.0,
                0.05,
            ),
            Num::float(
                "vignette_falloff",
                "blur_on",
                "modes-vignette-falloff",
                0.1,
                10.0,
                0.1,
            ),
        ],
    },
    Toggle {
        key: "grain_enabled",
        icon: "grain",
        title: "modes-grain",
        subtitle: None,
        bench: "grain",
        params: &[
            Num::float("grain_strength", "tune", "modes-strength", 0.0, 1.0, 0.05),
            Num::float(
                "grain_size",
                "photo_size_select_small",
                "modes-grain-size",
                1.0,
                10.0,
                0.1,
            ),
        ],
    },
    Toggle {
        key: "lut_enabled",
        icon: "palette",
        title: "modes-lut",
        subtitle: Some("modes-lut-help"),
        bench: "lut",
        params: &[Num::float(
            "lut_intensity",
            "tune",
            "modes-lut-intensity",
            0.0,
            1.0,
            0.05,
        )],
    },
];

/// More settings than this fold under «Más opciones».
const FOLD_PARAMS: usize = 4;

/// The editor of mode `id` (keyed by id in the page, so its hooks start
/// over when the mode changes).
#[component]
pub(crate) fn ModeEditor(id: ModeId) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let costs = use_costs();
    // The catalog and the GPUs, once.
    use_hook(move || {
        let s = store.peek();
        let load_effects = s.effects.is_empty();
        let load_devices = s.gpus.is_empty();
        drop(s);
        if load_effects {
            dispatch(Action::ListEffects);
        }
        if load_devices {
            dispatch(Action::ListDevices);
        }
    });

    let scope = Scope::Mode(id.clone());
    let name = use_value::<String>(scope.clone(), "name".into());
    let engine = use_value::<EffectsEngine>(scope.clone(), "effects_engine".into());
    let s = store.read();
    let Some(saved) = s.modes.store.get(&id).cloned() else {
        drop(s);
        return rsx! { MissingMode {} };
    };
    let users = s.modes.users.get(&id).cloned().unwrap_or_default();
    drop(s);

    rsx! {
        section { class: "flex-[999_1_520px] min-w-0 flex flex-col gap-5", "aria-label": "{name}",
            ModeHeader { id: id.clone(), name: name(), users: users.clone() }
            if users.len() > 1 {
                SharedModeBanner { id: id.clone(), users: users.len() }
            }
            Section { title: t!("modes-section-model"),
                SettingGroup {
                    ModelRow { id: id.clone() }
                    SwitchRow {
                        scope: scope.clone(),
                        setting: "double_upscale",
                        icon: "filter_2",
                        title: "modes-double-upscale",
                        subtitle: "modes-double-upscale-help",
                    }
                }
            }
            Section { title: t!("modes-section-sampling"), Sampling { id: id.clone() } }
            EffectsSection { id: id.clone(), engine: engine(), costs: costs() }
            Section { title: t!("modes-section-final"),
                for toggle in FINAL {
                    ToggleGroup { key: "{toggle.key}", scope: scope.clone(), toggle: *toggle, cost: costs.read().native(toggle.bench) }
                }
            }
            if saved.description.is_some() {
                p { class: "t-bs text-on-surface-variant m-0", {saved.description.clone().unwrap_or_default()} }
            }
        }
    }
}

/// A titled block of the editor.
#[component]
fn Section(title: String, trailing: Option<Element>, children: Element) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2",
            div { class: "flex flex-wrap items-center gap-3",
                h3 { class: "t-ts text-on-surface-variant m-0 flex-[1_1_200px]", "{title}" }
                {trailing}
            }
            {children}
        }
    }
}

/// The mode was deleted (or renamed on disk) while open.
#[component]
fn MissingMode() -> Element {
    let dispatch = use_dispatch();
    rsx! {
        section { class: "flex-[999_1_520px] min-w-0 flex flex-col items-start gap-3",
            p { class: "t-bl m-0", {t!("modes-missing")} }
            Button {
                label: t!("modes-back-to-list"),
                variant: ButtonVariant::Tonal,
                icon: "arrow_back",
                onclick: move |_| dispatch(Action::Navigate(Route::Modes { id: None })),
            }
        }
    }
}

/// Name, who uses it, and rename, duplicate and delete.
#[component]
fn ModeHeader(id: ModeId, name: String, users: Vec<ProfileRef>) -> Element {
    let dispatch = use_dispatch();
    let mut renaming = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let used_by = used_by_text(&users);
    let rename_id = id.clone();
    let commit = move |text: String| {
        let text = text.trim().to_owned();
        if !text.is_empty() {
            dispatch(Action::SetField {
                scope: Scope::Mode(rename_id.clone()),
                key: "name".into(),
                value: yaml(text),
            });
        }
        renaming.set(false);
    };
    let mut commit_button = commit.clone();
    let dup_id = id.clone();
    let start_name = name.clone();
    rsx! {
        div { class: "flex flex-wrap items-center gap-3",
            if renaming() {
                div { class: "flex-[1_1_240px] min-w-0 flex items-center gap-2",
                    div { class: "flex-1",
                        TextField {
                            value: draft,
                            label: t!("modes-name"),
                            autofocus: true,
                            error: draft.read().trim().is_empty().then(|| t!("modes-name-empty")),
                            on_submit: commit,
                        }
                    }
                    IconButton { icon: "check", label: t!("modes-rename-done"), onclick: move |_| commit_button(draft()) }
                    IconButton { icon: "close", label: t!("modes-rename-cancel"), onclick: move |_| renaming.set(false) }
                }
            } else {
                div { class: "flex-[1_1_240px] min-w-0 flex flex-col",
                    h2 { class: "t-hm m-0 truncate", "{name}" }
                    span { class: "t-bs text-on-surface-variant", "{used_by}" }
                }
                IconButton {
                    icon: "edit",
                    label: t!("modes-rename"),
                    variant: IconButtonVariant::Outlined,
                    onclick: move |_| {
                        draft.set(start_name.clone());
                        renaming.set(true);
                    },
                }
            }
            IconButton {
                icon: "content_copy",
                label: t!("modes-duplicate"),
                variant: IconButtonVariant::Outlined,
                onclick: move |_| dispatch(Action::DuplicateMode { id: dup_id.clone(), for_profile: None }),
            }
            IconButton {
                icon: "delete",
                label: t!("modes-delete"),
                variant: IconButtonVariant::Outlined,
                onclick: move |_| dispatch(Action::OpenDialog(Dialog::DeleteMode(id.clone()))),
            }
        }
    }
}

/// «Usado por BALDR SKY, Demons Roots y Utawarerumono» / «Sin uso».
pub(crate) fn used_by_text(users: &[ProfileRef]) -> String {
    let names: Vec<String> = users
        .iter()
        .map(|p| match p {
            ProfileRef::Default => t!("nav-default-profile"),
            ProfileRef::Named(name) => name.clone(),
        })
        .collect();
    match names.as_slice() {
        [] => t!("modes-unused"),
        [one] => t!("modes-used-by-one", name = one.clone()),
        [rest @ .., last] => t!("modes-used-by-many", names = rest.join(", "), last = last.clone()),
    }
}

/// «Este modo lo usan N perfiles»: offers a copy before editing.
#[component]
fn SharedModeBanner(id: ModeId, users: usize) -> Element {
    let dispatch = use_dispatch();
    let mut hidden = use_signal(|| false);
    if hidden() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "flex flex-wrap items-center gap-3 px-5 py-4 rounded-lg-inc bg-tertiary-container text-on-tertiary-container",
            role: "status",
            span { class: "flex-[1_1_260px] t-bm", {t!("modes-shared", count = users)} }
            Button {
                label: t!("modes-shared-duplicate"),
                variant: ButtonVariant::Tertiary,
                icon: "content_copy",
                onclick: move |_| dispatch(Action::DuplicateMode { id: id.clone(), for_profile: None }),
            }
            Button { label: t!("modes-shared-edit-anyway"), variant: ButtonVariant::Text, onclick: move |_| hidden.set(true) }
        }
    }
}

/// The SRCNN model as a slider with named stops.
#[component]
fn ModelRow(id: ModeId) -> Element {
    let dispatch = use_dispatch();
    let scope = Scope::Mode(id);
    let model = use_value::<Model>(scope.clone(), "model".into());
    let origin = use_origin(scope.clone(), "model".into());
    let control = use_control(model, |m| model_index(*m) as f64);
    let set_scope = scope.clone();
    let title = t!("modes-model");
    rsx! {
        SettingRow {
            icon: "neurology",
            title: title.clone(),
            subtitle: t!("modes-model-help"),
            origin: origin(),
            on_inherit: inherit(scope, "model"),
            div { class: "flex-[1_1_320px]",
                Slider {
                    value: control,
                    min: 0.0,
                    max: (Model::ALL.len() - 1) as f64,
                    step: 1.0,
                    label: title,
                    stops: Model::ALL.iter().map(|m| m.as_str().to_owned()).collect::<Vec<_>>(),
                    on_change: move |v: f64| {
                        let model = Model::ALL[(v.round() as usize).min(Model::ALL.len() - 1)];
                        dispatch(Action::SetField { scope: set_scope.clone(), key: "model".into(), value: yaml(model) })
                    },
                }
            }
        }
    }
}

fn model_index(model: Model) -> usize {
    Model::ALL.iter().position(|m| *m == model).unwrap_or(3)
}

/// Upsampler, downsampler, blur and the folded Lanczos options.
#[component]
fn Sampling(id: ModeId) -> Element {
    let scope = Scope::Mode(id);
    let upsampler_options = vec![
        (Upsampler::Lanczos, t!("modes-upsampler-lanczos")),
        (Upsampler::Fsr, t!("modes-upsampler-fsr")),
        (Upsampler::Nis, t!("modes-upsampler-nis")),
    ];
    let downsampler_options = vec![
        (Downsampler::Catmull, t!("modes-downsampler-catmull")),
        (Downsampler::Lanczos, t!("modes-downsampler-lanczos")),
    ];
    rsx! {
        SettingGroup {
            SelectRow {
                scope: scope.clone(),
                setting: "upsampler",
                icon: "open_in_full",
                title: t!("modes-upsampler"),
                subtitle: t!("modes-upsampler-help"),
                options: upsampler_options,
            }
            SelectRow {
                scope: scope.clone(),
                setting: "downsampler",
                icon: "close_fullscreen",
                title: t!("modes-downsampler"),
                options: downsampler_options,
            }
            NumberRow { scope: scope.clone(), num: Num::float("blur", "blur_on", "modes-blur", 0.0, 2.0, 0.05) }
        }
        SettingGroup { title: t!("modes-lanczos-options"), collapsible: true, collapsed: true,
            NumberRow {
                scope: scope.clone(),
                num: Num::float("antiring_strength", "waves", "modes-antiring", 0.0, 1.0, 0.05),
            }
            SwitchRow {
                scope: scope.clone(),
                setting: "tight_antiring",
                icon: "compress",
                title: "modes-tight-antiring",
            }
            KernelRadiusRow { scope: scope.clone() }
        }
    }
}

/// A dropdown row of one enum setting of `scope`.
#[component]
fn SelectRow<T: Clone + PartialEq + serde::Serialize + serde::de::DeserializeOwned + 'static>(
    scope: Scope,
    /// YAML key.
    setting: &'static str,
    icon: &'static str,
    title: String,
    subtitle: Option<String>,
    options: Vec<(T, String)>,
) -> Element {
    let dispatch = use_dispatch();
    let value = use_value::<T>(scope.clone(), setting.into());
    let origin = use_origin(scope.clone(), setting.into());
    let control = use_control(value, Clone::clone);
    let set_scope = scope.clone();
    rsx! {
        SettingRow {
            icon,
            title: title.clone(),
            subtitle,
            origin: origin(),
            on_inherit: inherit(scope, setting),
            Select {
                options,
                value: control,
                label: title,
                on_change: move |v: T| dispatch(Action::SetField { scope: set_scope.clone(), key: setting.into(), value: yaml(v) }),
            }
        }
    }
}

/// `kernel_radius`: automatic (unset) or forced to 2 to 10.
#[component]
fn KernelRadiusRow(scope: Scope) -> Element {
    let dispatch = use_dispatch();
    let value = use_value::<Option<u32>>(scope.clone(), "kernel_radius".into());
    let origin = use_origin(scope.clone(), "kernel_radius".into());
    let forced = use_control(value, Option::is_some);
    let radius = use_control(value, |r| r.unwrap_or(3) as f64);
    let switch_scope = scope.clone();
    let slider_scope = scope.clone();
    let title = t!("modes-kernel-radius");
    rsx! {
        SettingRow {
            icon: "radar",
            title: title.clone(),
            subtitle: t!("modes-kernel-radius-help"),
            origin: origin(),
            on_inherit: inherit(scope, "kernel_radius"),
            if forced() {
                div { class: "w-56",
                    Slider {
                        value: radius,
                        min: 2.0,
                        max: 10.0,
                        step: 1.0,
                        label: title.clone(),
                        on_change: move |v: f64| dispatch(Action::SetField {
                            scope: slider_scope.clone(),
                            key: "kernel_radius".into(),
                            value: yaml(v.round() as u32),
                        }),
                    }
                }
            }
            Switch {
                value: forced,
                label: t!("modes-kernel-radius-force"),
                on_change: move |on: bool| {
                    let scope = switch_scope.clone();
                    let key = "kernel_radius".to_owned();
                    let action = if on {
                        Action::SetField { scope, key, value: yaml(3) }
                    } else {
                        Action::Inherit { scope, key }
                    };
                    dispatch(action)
                },
            }
        }
    }
}

/// Engine choice, its explanation, then the native passes or the chain,
/// the total and «Medir en mi GPU».
#[component]
fn EffectsSection(id: ModeId, engine: EffectsEngine, costs: Costs) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let scope = Scope::Mode(id.clone());
    let engine_value = use_value::<EffectsEngine>(scope.clone(), "effects_engine".into());
    let control = use_control(engine_value, |e| *e);
    let settings = use_mode_settings(id.clone());
    let chain = chain_names(&settings.read().magpie_effects);
    let total = costs.mode(&settings.read(), &chain);
    let (catalog, verified) = {
        let s = store.read();
        (s.effects.len(), s.effects.iter().filter(|e| e.verified).count())
    };
    let set_scope = scope.clone();
    let bench_id = id.clone();
    let summary = {
        let mut parts = Vec::new();
        parts.push(match total {
            Some(ms) => t!("modes-total", ms = fmt_ms(ms)),
            None => t!("modes-total-unknown"),
        });
        if let Some(gpu) = &costs.gpu {
            parts.push(gpu.clone());
        }
        parts.join(" · ")
    };
    rsx! {
        Section {
            title: t!("modes-section-effects"),
            trailing: rsx! {
                Connected {
                    options: vec![
                        (EffectsEngine::Native, t!("modes-engine-native")),
                        (EffectsEngine::Magpiefx, t!("modes-engine-magpiefx")),
                    ],
                    value: control,
                    label: t!("modes-engine"),
                    on_change: move |e: EffectsEngine| dispatch(Action::SetField {
                        scope: set_scope.clone(),
                        key: "effects_engine".into(),
                        value: yaml(e),
                    }),
                }
            },
            div { class: "flex flex-wrap gap-x-6 gap-y-2 px-5 py-4 rounded-lg-inc bg-surface-container-high",
                p { class: "t-bs m-0 flex-[1_1_240px]",
                    b { {t!("modes-engine-magpiefx")} ": " }
                    if catalog > 0 {
                        {t!("modes-engine-magpiefx-help", count = catalog, verified = verified)}
                    } else {
                        {t!("modes-engine-magpiefx-help-unloaded")}
                    }
                }
                p { class: "t-bs m-0 flex-[1_1_240px] text-on-surface-variant",
                    b { {t!("modes-engine-native")} ": " }
                    {t!("modes-engine-native-help")}
                }
            }
            if engine == EffectsEngine::Magpiefx {
                EffectChain { id: id.clone(), costs: costs.clone() }
            } else {
                for toggle in NATIVE {
                    ToggleGroup { key: "{toggle.key}", scope: scope.clone(), toggle: *toggle, cost: costs.native(toggle.bench) }
                }
            }
            div { class: "flex flex-wrap items-center gap-2 justify-end",
                span { class: "t-bs mono text-on-surface-variant", "{summary}" }
                Button {
                    label: t!("modes-measure"),
                    variant: ButtonVariant::Tonal,
                    icon: "speed",
                    onclick: move |_| dispatch(Action::OpenDialog(Dialog::Bench { mode: Some(bench_id.clone()) })),
                }
            }
        }
    }
}

/// The mode's settings with the unsaved edits (for the total cost).
fn use_mode_settings(id: ModeId) -> Memo<ModeSettings> {
    let store = use_store();
    use_memo(move || {
        let s = store.read();
        let scope = Scope::Mode(id.clone());
        let mut settings = s
            .modes
            .store
            .get(&id)
            .map(|m| m.settings.clone())
            .unwrap_or_default();
        settings.effects_engine = s.field::<EffectsEngine>(&scope, "effects_engine").value();
        settings.magpie_effects = s.field::<Vec<MagpieEffectSpec>>(&scope, CHAIN_KEY).value();
        for toggle in NATIVE.iter().chain(FINAL) {
            let on = s.field::<bool>(&scope, toggle.key).value();
            match toggle.key {
                "anime4k_enabled" => settings.anime4k_enabled = on,
                "luma_sharpen_enabled" => settings.luma_sharpen_enabled = on,
                "crt_enabled" => settings.crt_enabled = on,
                "cas_enabled" => settings.cas_enabled = on,
                "deband_enabled" => settings.deband_enabled = on,
                "bloom_enabled" => settings.bloom_enabled = on,
                "vignette_enabled" => settings.vignette_enabled = on,
                "grain_enabled" => settings.grain_enabled = on,
                "lut_enabled" => settings.lut_enabled = on,
                _ => {}
            }
        }
        settings
    })
}

/// A switch row and, while it is on, its settings (folded when many).
#[component]
fn ToggleGroup(scope: Scope, toggle: Toggle, cost: Option<f32>) -> Element {
    let on = use_value::<bool>(scope.clone(), toggle.key.into());
    let switch = rsx! {
        SwitchRow {
            scope: scope.clone(),
            setting: toggle.key,
            icon: toggle.icon,
            title: toggle.title,
            subtitle: toggle.subtitle,
            cost,
        }
    };
    if !on() {
        return rsx! { SettingGroup { {switch} } };
    }
    let params = rsx! {
        for num in toggle.params {
            NumberRow { key: "{num.key}", scope: scope.clone(), num: *num }
        }
        if toggle.key == "lut_enabled" {
            LutPresetRow { scope: scope.clone() }
        }
    };
    if toggle.params.len() > FOLD_PARAMS {
        rsx! {
            SettingGroup { {switch} }
            SettingGroup { title: t!("modes-more-options", name = tr(toggle.title)), collapsible: true, collapsed: true,
                {params}
            }
        }
    } else {
        rsx! { SettingGroup { {switch} {params} } }
    }
}

/// The built-in LUT.
#[component]
fn LutPresetRow(scope: Scope) -> Element {
    let options: Vec<(String, String)> = LUT_PRESETS
        .iter()
        .map(|p| (p.to_string(), tr(&format!("modes-lut-{p}"))))
        .collect();
    rsx! {
        SelectRow { scope, setting: "lut_preset", icon: "photo_filter", title: t!("modes-lut-preset"), options }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, has_key};

    #[test]
    fn every_setting_has_its_text() {
        let mut keys: Vec<&str> = Vec::new();
        for toggle in NATIVE.iter().chain(FINAL) {
            keys.push(toggle.title);
            keys.extend(toggle.subtitle);
            keys.extend(toggle.params.iter().map(|n| n.title));
        }
        let luts: Vec<String> = LUT_PRESETS.iter().map(|p| format!("modes-lut-{p}")).collect();
        keys.extend(luts.iter().map(String::as_str));
        for key in keys {
            assert!(has_key(Lang::Es, key), "es: {key}");
            assert!(has_key(Lang::En, key), "en: {key}");
        }
    }

    #[test]
    fn toggles_are_mode_keys() {
        for toggle in NATIVE.iter().chain(FINAL) {
            assert!(lirtup_config::MODE_KEYS.contains(&toggle.key), "{}", toggle.key);
            for num in toggle.params {
                assert!(lirtup_config::MODE_KEYS.contains(&num.key), "{}", num.key);
            }
        }
    }

    #[test]
    fn model_index_follows_the_slider() {
        assert_eq!(model_index(Model::None), 0);
        assert_eq!(model_index(Model::Fast), 3);
        assert_eq!(model_index(Model::M8x32), 9);
    }
}
