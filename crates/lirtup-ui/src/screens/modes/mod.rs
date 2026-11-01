//! Modos de escalado (front D): the mode list with total cost and use, and
//! the editor (model, double upscale, sampling, engine, the effect chain
//! with drag and drop, MagpieFX parameters, the shared mode banner, final
//! touches). Also the effect picker and deleting a mode.
//!
//! | Module | What |
//! | --- | --- |
//! | [`editor`] | One mode's editor |
//! | [`chain`] | The MagpieFX chain and its cards |
//! | [`picker`] | «Agregar efecto» |
//! | [`costs`] | ms per pass from `bench.json` and the last measurement |
//! | [`widgets`] | Rows bound to the store (also used by Ajustes) |

mod chain;
mod costs;
mod editor;
mod picker;
pub(crate) mod widgets;

use dioxus::prelude::*;
use lirtup_config::{EffectsEngine, Mode};

pub use picker::EffectPickerDialog;

use self::chain::chain_names;
use self::costs::{Costs, native_passes, use_costs};
use self::editor::ModeEditor;
use self::widgets::CostBadge;
use crate::app::{use_dispatch, use_store};
use crate::components::{Button, ButtonVariant, Dialog as DialogBox};
use crate::route::{ModeId, Route};
use crate::state::{Action, Scope};

/// The list (`id: None`) or one mode's editor (`Route::Modes`).
#[component]
pub fn ModesPage(id: Option<ModeId>) -> Element {
    let costs = use_costs();
    rsx! {
        div { class: "flex flex-wrap items-start gap-6 px-10 pt-8 pb-28 max-w-[1440px]",
            ModeList { selected: id.clone(), costs: costs() }
            match id {
                Some(id) => rsx! { ModeEditor { key: "{id}", id } },
                None => rsx! { NoModeSelected {} },
            }
        }
    }
}

/// The modes, with model, effects, profiles that use them and total cost.
#[component]
fn ModeList(selected: Option<ModeId>, costs: Costs) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let s = store.read();
    let rows: Vec<(ModeId, String, String, Option<f32>, bool)> = s
        .modes
        .store
        .modes
        .iter()
        .map(|(id, mode)| {
            let users = s.modes.users.get(id).map_or(0, Vec::len);
            let chain = chain_names(&mode.settings.magpie_effects);
            let summary = summary(mode, &chain, users);
            let cost = costs.mode(&mode.settings, &chain);
            let dirty = s.dirty_count(&Scope::Mode(id.clone())) > 0;
            (id.clone(), mode.name.clone(), summary, cost, dirty)
        })
        .collect();
    let new_name = free_name(&s.modes.store.modes);
    let new_id = s.modes.store.new_id(&new_name);
    drop(s);
    rsx! {
        section { class: "flex-[1_1_280px] max-w-[340px] min-w-0 flex flex-col gap-3",
            h1 { class: "t-hl m-0", {t!("modes-title")} }
            p { class: "t-bm m-0 text-on-surface-variant", {t!("modes-intro")} }
            nav { class: "flex flex-col gap-0.5 mt-2", "aria-label": t!("modes-list-label"),
                if rows.is_empty() {
                    p { class: "t-bm text-on-surface-variant m-0 px-5 py-4 rounded-lg-inc bg-surface-container", {t!("modes-none")} }
                }
                for (id, name, summary, cost, dirty) in rows {
                    ModeListItem {
                        key: "{id}",
                        selected: selected.as_deref() == Some(id.as_str()),
                        id,
                        name,
                        summary,
                        cost,
                        dirty,
                    }
                }
            }
            div {
                Button {
                    label: t!("modes-new"),
                    variant: ButtonVariant::Tonal,
                    icon: "add",
                    onclick: move |_| {
                        dispatch(Action::CreateMode { name: new_name.clone(), copy_from: None });
                        dispatch(Action::Navigate(Route::Modes { id: Some(new_id.clone()) }));
                    },
                }
            }
        }
    }
}

/// One entry of the list: a button that opens the editor.
#[component]
fn ModeListItem(
    id: ModeId,
    name: String,
    summary: String,
    cost: Option<f32>,
    dirty: bool,
    selected: bool,
) -> Element {
    let dispatch = use_dispatch();
    let tone = if selected {
        "bg-secondary-container text-on-secondary-container"
    } else {
        "bg-surface-container hover:bg-surface-container-high"
    };
    rsx! {
        button {
            class: "flex items-center gap-3 min-h-18 px-5 py-3 text-left rounded-xs first:rounded-t-lg-inc last:rounded-b-lg-inc {tone}",
            r#type: "button",
            "aria-current": if selected { "page" } else { "false" },
            onclick: move |_| dispatch(Action::Navigate(Route::Modes { id: Some(id.clone()) })),
            div { class: "flex-1 min-w-0 flex flex-col",
                span { class: "t-ts truncate flex items-center gap-2",
                    "{name}"
                    if dirty {
                        span { class: "dot", title: t!("nav-unsaved") }
                    }
                }
                span { class: "t-bs opacity-80 truncate", "{summary}" }
            }
            CostBadge { ms: cost, strong: selected }
        }
    }
}

/// «fast · Anime4K + LumaSharpen · 3 perfiles».
fn summary(mode: &Mode, chain: &[String], users: usize) -> String {
    let settings = &mode.settings;
    let effects: Vec<String> = if settings.effects_engine == EffectsEngine::Magpiefx {
        chain.to_vec()
    } else {
        native_passes(settings)
            .into_iter()
            .map(|label| crate::i18n::tr(&format!("modes-pass-{label}")))
            .collect()
    };
    let effects = match effects.len() {
        0 => t!("modes-no-effects"),
        1 | 2 => effects.join(" + "),
        n => t!(
            "modes-effects-more",
            first = effects[..2].join(" + "),
            count = n - 2
        ),
    };
    let users = t!("modes-profiles", count = users);
    format!("{} · {effects} · {users}", settings.model.as_str())
}

/// «Modo nuevo», or «Modo nuevo 2»… when it is taken.
fn free_name(modes: &indexmap::IndexMap<String, Mode>) -> String {
    let base = t!("modes-new-name");
    let taken = |name: &str| modes.values().any(|m| m.name == name);
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken(name))
        .unwrap_or(base)
}

/// The right side without a mode.
#[component]
fn NoModeSelected() -> Element {
    rsx! {
        section { class: "flex-[999_1_520px] min-w-0 flex flex-col items-center justify-center gap-2 min-h-80 rounded-xl bg-surface-container-low text-on-surface-variant",
            crate::components::Icon { name: "tune", size: 32 }
            p { class: "t-bm m-0", {t!("modes-pick-one")} }
        }
    }
}

/// «Eliminar modo» confirmation (`Dialog::DeleteMode`).
#[component]
pub fn DeleteModeDialog(id: ModeId) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let open = use_signal(|| true);
    let s = store.read();
    let name = s
        .modes
        .store
        .get(&id)
        .map_or_else(|| id.clone(), |m| m.name.clone());
    let users = s.modes.users.get(&id).map_or(0, Vec::len);
    drop(s);
    let body = if users == 0 {
        t!("modes-delete-body", name = name.clone())
    } else {
        t!("modes-delete-body-used", name = name.clone(), count = users)
    };
    rsx! {
        DialogBox {
            open,
            icon: "delete",
            icon_color: "error",
            title: t!("modes-delete-title"),
            on_close: move |_| dispatch(Action::CloseDialog),
            actions: rsx! {
                Button { label: t!("modes-cancel"), variant: ButtonVariant::Outlined, onclick: move |_| dispatch(Action::CloseDialog) }
                Button {
                    label: t!("modes-delete-confirm"),
                    variant: ButtonVariant::Danger,
                    icon: "delete",
                    onclick: move |_| dispatch(Action::DeleteMode(id.clone())),
                }
            },
            p { class: "t-bm m-0", "{body}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, has_key};
    use lirtup_config::{MagpieEffectSpec, ModeSettings};

    /// Every literal key given to `t!` in front D's files is defined in
    /// Spanish and English (read from the files: `has_key` does not find
    /// messages that take arguments when it is given none).
    #[test]
    fn every_literal_key_is_translated() {
        let sources = [
            include_str!("mod.rs"),
            include_str!("editor.rs"),
            include_str!("chain.rs"),
            include_str!("picker.rs"),
            include_str!("widgets.rs"),
            include_str!("../bench.rs"),
            include_str!("../settings/mod.rs"),
            include_str!("../settings/hotkeys.rs"),
            include_str!("../settings/style.rs"),
        ];
        let ftl = [
            (
                "es",
                [
                    include_str!("../../../i18n/es/modes.ftl"),
                    include_str!("../../../i18n/es/settings.ftl"),
                    include_str!("../../../i18n/es/app.ftl"),
                ],
            ),
            (
                "en",
                [
                    include_str!("../../../i18n/en/modes.ftl"),
                    include_str!("../../../i18n/en/settings.ftl"),
                    include_str!("../../../i18n/en/app.ftl"),
                ],
            ),
        ];
        let mut missing = Vec::new();
        for source in sources {
            let mut rest = source;
            while let Some(at) = rest.find("t!(\"") {
                let before = rest[..at].chars().next_back();
                rest = &rest[at + 4..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let key = &rest[..rest.find('"').unwrap()];
                for (lang, files) in &ftl {
                    let defined = files.iter().any(|f| {
                        f.lines()
                            .any(|l| l.split_once(" =").is_some_and(|(k, _)| k == key))
                    });
                    if !defined {
                        missing.push(format!("{lang}: {key}"));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "missing: {missing:?}");
    }

    fn mode(name: &str, settings: ModeSettings) -> Mode {
        Mode {
            name: name.into(),
            description: None,
            settings,
            extra: Default::default(),
        }
    }

    #[test]
    fn summaries() {
        widgets::in_runtime(summaries_in_runtime);
    }

    fn summaries_in_runtime() {
        let plain = mode("Predeterminado", ModeSettings::default());
        assert_eq!(summary(&plain, &[], 2), "fast · sin efectos · 2 perfiles");
        let chain = ModeSettings {
            effects_engine: EffectsEngine::Magpiefx,
            magpie_effects: vec![
                MagpieEffectSpec::Short("A".into()),
                MagpieEffectSpec::Short("B".into()),
                MagpieEffectSpec::Short("C".into()),
            ],
            ..ModeSettings::default()
        };
        let names = chain_names(&chain.magpie_effects);
        assert_eq!(
            summary(&mode("x", chain), &names, 0),
            "fast · A + B y 1 más · sin uso"
        );
    }

    #[test]
    fn new_names_are_free() {
        widgets::in_runtime(new_names_in_runtime);
    }

    fn new_names_in_runtime() {
        let mut modes = indexmap::IndexMap::new();
        assert_eq!(free_name(&modes), "Modo nuevo");
        modes.insert("modo-nuevo".into(), mode("Modo nuevo", ModeSettings::default()));
        assert_eq!(free_name(&modes), "Modo nuevo 2");
    }
}
