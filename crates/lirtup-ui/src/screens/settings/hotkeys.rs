//! Atajos: the hotkeys of `lirtup serve`, in the groups of the design
//! (Activación, Sesión, Vista, Zoom y desplazamiento), each with a
//! `HotkeyCapture`.
//!
//! They are the `hotkeys.<action>` keys of *Predeterminado* in
//! `config.yaml`; an empty binding means unassigned. Ajustes has no save
//! bar, so each change is a `SetField` followed by `Save` (which also sends
//! `ReloadConfig`: the scaler grabs the new keys). «Restablecer» is
//! `Inherit`, back to the 1.x default.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lirtup_config::{Hotkey, HotkeyAction};

use super::SectionTitle;
use crate::app::{use_dispatch, use_store};
use crate::components::{Button, ButtonSize, ButtonVariant, HotkeyCapture, SettingGroup, SettingRow};
use crate::i18n::tr;
use crate::route::ProfileRef;
use crate::screens::modes::widgets::{use_control, use_origin, use_value, yaml};
use crate::state::{Action, Scope};

/// The groups and their actions, in display order.
pub(crate) const GROUPS: [(&str, &[HotkeyAction]); 4] = [
    (
        "settings-hotkeys-activation",
        &[HotkeyAction::StartFullscreen, HotkeyAction::StartWindowed],
    ),
    (
        "settings-hotkeys-session",
        &[
            HotkeyAction::ToggleScaling,
            HotkeyAction::ExitApp,
            HotkeyAction::Screenshot,
            HotkeyAction::ToggleStats,
        ],
    ),
    (
        "settings-hotkeys-view",
        &[
            HotkeyAction::CycleModel,
            HotkeyAction::CycleGeometry,
            HotkeyAction::RestoreView,
        ],
    ),
    (
        "settings-hotkeys-zoom",
        &[
            HotkeyAction::ZoomIn,
            HotkeyAction::ZoomOut,
            HotkeyAction::OffsetUp,
            HotkeyAction::OffsetDown,
            HotkeyAction::OffsetLeft,
            HotkeyAction::OffsetRight,
        ],
    ),
];

/// Material Symbols name of an action.
pub(crate) fn icon(action: HotkeyAction) -> &'static str {
    match action {
        HotkeyAction::StartFullscreen => "fullscreen",
        HotkeyAction::StartWindowed => "select_window",
        HotkeyAction::ToggleScaling => "visibility",
        HotkeyAction::ExitApp => "logout",
        HotkeyAction::Screenshot => "photo_camera",
        HotkeyAction::ToggleStats => "monitoring",
        HotkeyAction::CycleModel => "neurology",
        HotkeyAction::CycleGeometry => "aspect_ratio",
        HotkeyAction::RestoreView => "restart_alt",
        HotkeyAction::ZoomIn => "zoom_in",
        HotkeyAction::ZoomOut => "zoom_out",
        HotkeyAction::OffsetUp => "keyboard_arrow_up",
        HotkeyAction::OffsetDown => "keyboard_arrow_down",
        HotkeyAction::OffsetLeft => "keyboard_arrow_left",
        HotkeyAction::OffsetRight => "keyboard_arrow_right",
    }
}

/// Fluent key of an action's name (`settings-hotkey-start_fullscreen`).
pub(crate) fn label_key(action: HotkeyAction) -> String {
    format!("settings-hotkey-{}", action.as_str())
}

/// The store key of an action.
pub(crate) fn store_key(action: HotkeyAction) -> String {
    format!("hotkeys.{}", action.as_str())
}

/// A binding as written (`""` = unassigned).
pub(crate) fn parse(binding: &str) -> Option<Hotkey> {
    binding.parse().ok()
}

/// The same combination (key names compare without case: `Plus`, `plus`).
pub(crate) fn same(a: &Hotkey, b: &Hotkey) -> bool {
    a.modifiers == b.modifiers && a.key.eq_ignore_ascii_case(&b.key)
}

/// Other actions bound to the same combination (the scaler would grab only
/// one of them).
pub(crate) fn conflicts(
    bindings: &BTreeMap<HotkeyAction, Option<Hotkey>>,
    action: HotkeyAction,
) -> Vec<HotkeyAction> {
    let Some(Some(mine)) = bindings.get(&action) else {
        return Vec::new();
    };
    bindings
        .iter()
        .filter(|(other, hotkey)| **other != action && hotkey.as_ref().is_some_and(|h| same(h, mine)))
        .map(|(other, _)| *other)
        .collect()
}

fn scope() -> Scope {
    Scope::Profile(ProfileRef::Default)
}

/// The Atajos tab.
#[component]
pub(crate) fn Hotkeys() -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let bindings = use_memo(move || {
        let s = store.read();
        HotkeyAction::ALL
            .into_iter()
            .map(|a| (a, parse(&s.field::<String>(&scope(), &store_key(a)).value())))
            .collect::<BTreeMap<_, _>>()
    });
    let reset_all = move |_| {
        for action in HotkeyAction::ALL {
            dispatch(Action::Inherit {
                scope: scope(),
                key: store_key(action),
            });
        }
        dispatch(Action::Save);
    };
    rsx! {
        SectionTitle { title: t!("settings-tab-hotkeys") }
        div { class: "flex flex-wrap items-center gap-3 px-1 pb-1",
            p { class: "t-bs m-0 text-on-surface-variant flex-[1_1_320px]", {t!("settings-hotkeys-help")} }
            Button {
                label: t!("settings-hotkeys-reset-all"),
                variant: ButtonVariant::Text,
                size: ButtonSize::Xs,
                icon: "restart_alt",
                onclick: reset_all,
            }
        }
        for (group, actions) in GROUPS {
            div { key: "{group}", class: "flex flex-col gap-1.5 mb-2.5",
                h3 { class: "t-lm text-on-surface-variant m-0 pt-1.5 px-1 uppercase", {tr(group)} }
                SettingGroup {
                    for action in actions.iter().copied() {
                        HotkeyRow {
                            key: "{action.as_str()}",
                            action,
                            conflicts: conflicts(&bindings.read(), action),
                        }
                    }
                }
            }
        }
    }
}

/// One action: its keys and «Cambiar atajo».
#[component]
fn HotkeyRow(action: HotkeyAction, conflicts: Vec<HotkeyAction>) -> Element {
    let dispatch = use_dispatch();
    let binding = use_value::<String>(scope(), store_key(action));
    let origin = use_origin(scope(), store_key(action));
    let control = use_control(binding, |b| parse(b));
    let name = tr(&label_key(action));
    let subtitle = if conflicts.is_empty() {
        t!("settings-hotkey-default", keys = action.default_binding())
    } else {
        let names: Vec<String> = conflicts.iter().map(|a| tr(&label_key(*a))).collect();
        t!("settings-hotkey-conflict", actions = names.join(", "))
    };
    let on_change = move |hotkey: Option<Hotkey>| {
        dispatch(Action::SetField {
            scope: scope(),
            key: store_key(action),
            value: yaml(hotkey.map(|h| h.to_string()).unwrap_or_default()),
        });
        dispatch(Action::Save);
    };
    let on_inherit = move |()| {
        dispatch(Action::Inherit {
            scope: scope(),
            key: store_key(action),
        });
        dispatch(Action::Save);
    };
    rsx! {
        SettingRow { icon: icon(action), title: name.clone(), subtitle, origin: origin(), on_inherit,
            HotkeyCapture { value: control, label: name, require_modifier: true, on_change }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, has_key};

    #[test]
    fn every_action_is_in_one_group_with_its_text() {
        let mut seen: Vec<HotkeyAction> = GROUPS.iter().flat_map(|(_, a)| a.iter().copied()).collect();
        seen.sort();
        let mut all = HotkeyAction::ALL.to_vec();
        all.sort();
        assert_eq!(seen, all);
        for (group, _) in GROUPS {
            assert!(has_key(Lang::Es, group) && has_key(Lang::En, group), "{group}");
        }
        for action in HotkeyAction::ALL {
            let key = label_key(action);
            assert!(has_key(Lang::Es, &key) && has_key(Lang::En, &key), "{key}");
        }
    }

    #[test]
    fn conflicting_bindings() {
        let mut bindings = BTreeMap::new();
        bindings.insert(HotkeyAction::ZoomIn, parse("Alt+Shift+Plus"));
        bindings.insert(HotkeyAction::ZoomOut, parse("shift+alt+plus"));
        bindings.insert(HotkeyAction::Screenshot, parse(""));
        bindings.insert(HotkeyAction::ExitApp, parse(""));
        assert_eq!(
            conflicts(&bindings, HotkeyAction::ZoomIn),
            vec![HotkeyAction::ZoomOut]
        );
        // Two unassigned actions do not conflict.
        assert!(conflicts(&bindings, HotkeyAction::Screenshot).is_empty());
    }
}
