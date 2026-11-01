//! Activación (6.1): the two activation hotkeys from the config, deferred
//! scaling («Empezar» sends `Start { target: Focused, delay }`) and the
//! daemon switch. The delay is `gui.start_delay_s` (round 4, front Y):
//! choosing one in the menu saves it.

use dioxus::prelude::*;
use lirtup_config::{ConfigFile, HotkeyAction, Value};
use lirtup_proto::{SessionKind, Target};

use crate::app::{use_dispatch, use_store};
use crate::components::{
    Badge, Button, ButtonVariant, IconButton, Keycap, Menu, MenuItem, SettingGroup, SettingRow, Switch,
};
use crate::route::{Route, SettingsTab};
use crate::screens::settings::write_gui;
use crate::state::{Action, Origin};

/// The delays «Escalado diferido» offers, in seconds; the first is the
/// default. Another value of `gui.start_delay_s` (0 to 60) is shown as one
/// more choice.
pub const DELAYS: [u32; 4] = [3, 5, 10, 15];

/// The group.
#[component]
pub fn ActivationGroup() -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let delay = use_memo(move || store.read().gui.start_delay_s);
    let mut delay_menu = use_signal(|| false);

    let (fullscreen, windowed) = {
        let s = store.read();
        (
            hotkey(&s.config.file, HotkeyAction::StartFullscreen),
            hotkey(&s.config.file, HotkeyAction::StartWindowed),
        )
    };
    let watching = store.read().daemon.watching;

    // The switch mirrors the core's state; a click asks, the core answers
    // with `DaemonState`.
    let mut daemon_on = use_signal(|| store.peek().daemon.on);
    use_effect(move || {
        let on = store.read().daemon.on;
        if *daemon_on.peek() != on {
            daemon_on.set(on);
        }
    });

    let edit_hotkeys = move |_| {
        dispatch(Action::Navigate(Route::Settings {
            tab: SettingsTab::Hotkeys,
        }))
    };
    let begin = move |kind: SessionKind| {
        move |_| {
            let seconds = *delay.peek();
            super::start(
                dispatch,
                Target::Focused,
                kind,
                Some(seconds).filter(|s| *s > 0.0),
                None,
            )
        }
    };
    let seconds = delay();
    let custom = (!DELAYS.iter().any(|&d| d as f32 == seconds)).then_some(seconds);
    let mut delay_items: Vec<MenuItem> = DELAYS
        .iter()
        .map(|&s| {
            let item = MenuItem::new(delay_id(s), t!("home-delay-option", seconds = s));
            if s as f32 == seconds {
                item.icon("check")
            } else {
                item
            }
        })
        .collect();
    if let Some(c) = custom {
        delay_items.push(
            MenuItem::new("custom", t!("home-delay-option", seconds = format_seconds(c))).icon("check"),
        );
    }
    let daemon_subtitle = if daemon_on() {
        t!("home-daemon-watching", count = watching)
    } else {
        t!("home-daemon-subtitle")
    };

    rsx! {
        section { class: "flex flex-col gap-2",
            h2 { class: "t-tm m-0", {t!("home-activation-title")} }
            p { class: "t-bm m-0 text-on-surface-variant", {t!("home-activation-help")} }
            div { class: "mt-2",
                SettingGroup {
                    SettingRow {
                        icon: "fullscreen",
                        title: t!("home-activation-fullscreen"),
                        subtitle: t!("home-activation-fullscreen-subtitle"),
                        origin: Origin::Inherited,
                        Keys { binding: fullscreen }
                        IconButton { icon: "edit", label: t!("home-activation-edit-hotkey"), size: 32, onclick: edit_hotkeys }
                    }
                    SettingRow {
                        icon: "select_window",
                        title: t!("home-activation-windowed"),
                        subtitle: t!("home-activation-windowed-subtitle"),
                        origin: Origin::Inherited,
                        Keys { binding: windowed }
                        IconButton { icon: "edit", label: t!("home-activation-edit-hotkey"), size: 32, onclick: edit_hotkeys }
                    }
                    SettingRow {
                        icon: "timer",
                        title: t!("home-delay-title"),
                        subtitle: t!("home-delay-subtitle", seconds = format_seconds(seconds)),
                        origin: Origin::Inherited,
                        div { class: "flex flex-wrap items-center gap-2",
                            Button {
                                label: t!("home-delay-start"),
                                variant: ButtonVariant::Outlined,
                                icon: "fullscreen",
                                title: t!("home-delay-start-fullscreen"),
                                onclick: begin(SessionKind::Fullscreen),
                            }
                            Button {
                                label: t!("home-delay-start"),
                                variant: ButtonVariant::Outlined,
                                icon: "select_window",
                                title: t!("home-delay-start-windowed"),
                                onclick: begin(SessionKind::Windowed),
                            }
                            div { class: "relative",
                                IconButton {
                                    icon: "expand_more",
                                    label: t!("home-delay-choose"),
                                    size: 32,
                                    selected: delay_menu(),
                                    onclick: move |_| delay_menu.toggle(),
                                }
                                Menu {
                                    open: delay_menu,
                                    items: delay_items,
                                    on_select: move |id: &'static str| {
                                        if let Some(s) = DELAYS.iter().copied().find(|&s| delay_id(s) == id) {
                                            write_gui(store, dispatch, |g| g.start_delay_s = s as f32);
                                        }
                                        delay_menu.set(false);
                                    },
                                }
                            }
                        }
                    }
                    SettingRow {
                        icon: "auto_mode",
                        title: t!("home-daemon-title"),
                        subtitle: daemon_subtitle,
                        origin: Origin::Inherited,
                        Switch {
                            value: daemon_on,
                            label: t!("home-daemon-title"),
                            on_change: move |on| dispatch(Action::SetDaemon { on }),
                        }
                    }
                }
            }
        }
    }
}

/// The keycaps of a binding, or «Sin atajo».
#[component]
fn Keys(binding: String) -> Element {
    let keys = split_binding(&binding);
    if keys.is_empty() {
        return rsx! { Badge { label: t!("home-activation-no-hotkey") } };
    }
    rsx! {
        span { class: "flex items-center gap-1",
            for key in keys {
                Keycap { label: key }
            }
        }
    }
}

/// `Alt+Shift+A` → `["Alt", "Shift", "A"]`; empty or `none` → nothing.
pub fn split_binding(binding: &str) -> Vec<String> {
    let binding = binding.trim();
    if binding.is_empty() || binding.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    binding
        .split('+')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The binding of an action in *Predeterminado* (`hotkeys.<action>` of
/// `config.yaml`), else the system default. Activation hotkeys are grabbed
/// by `lirtup serve` with the top-level config only.
pub fn hotkey(file: &ConfigFile, action: HotkeyAction) -> String {
    file.general
        .get("hotkeys")
        .and_then(Value::as_mapping)
        .and_then(|m| m.get(action.as_str()))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            // `null` or `false`: unbound.
            _ => String::new(),
        })
        .unwrap_or_else(|| action.default_binding().to_owned())
}

/// `3` for 3.0, `2.5` for 2.5 (the delay as the subtitle says it).
fn format_seconds(seconds: f32) -> String {
    if seconds.fract() == 0.0 {
        format!("{seconds:.0}")
    } else {
        format!("{seconds:.1}")
    }
}

fn delay_id(seconds: u32) -> &'static str {
    match seconds {
        3 => "3",
        5 => "5",
        10 => "10",
        _ => "15",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_split_into_keys() {
        assert_eq!(split_binding("Alt+Shift+A"), vec!["Alt", "Shift", "A"]);
        assert_eq!(split_binding(" Ctrl + F10 "), vec!["Ctrl", "F10"]);
        assert!(split_binding("").is_empty());
        assert!(split_binding("none").is_empty());
    }

    #[test]
    fn hotkeys_from_config_or_default() {
        let mut file = ConfigFile::default();
        assert_eq!(hotkey(&file, HotkeyAction::StartFullscreen), "Alt+Shift+A");
        let yaml = "hotkeys:\n  start_windowed: Ctrl+Alt+W\n  start_fullscreen: null\n";
        file.general = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(hotkey(&file, HotkeyAction::StartWindowed), "Ctrl+Alt+W");
        assert_eq!(hotkey(&file, HotkeyAction::StartFullscreen), "");
    }

    #[test]
    fn seconds_read_as_written() {
        assert_eq!(format_seconds(3.0), "3");
        assert_eq!(format_seconds(2.5), "2.5");
    }

    #[test]
    fn the_default_delay_is_offered() {
        assert!(
            DELAYS
                .iter()
                .any(|&d| d as f32 == lirtup_config::GuiConfig::default().start_delay_s)
        );
    }

    #[test]
    fn every_delay_has_an_id() {
        for s in DELAYS {
            assert_eq!(delay_id(s), s.to_string());
        }
    }
}
