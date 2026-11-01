//! Ajustes (front D): General (language, theme, seed, tray, daemon), Atajos
//! (groups with `HotkeyCapture`) and Estilo (zoom, font, grid, intervals).
//! Written at once (`Action::SetGui`), no save bar.
//!
//! The hotkeys live in `config.yaml` (*Predeterminado*, read by `lirtup
//! serve`, which grabs them), not in `gui-config.yaml`: Atajos edits them
//! with `SetField` and saves at once with `Save` (see [`hotkeys`]).

mod hotkeys;
mod style;

use dioxus::prelude::*;
use lirtup_config::GuiConfig;
use lirtup_config::gui::{Language, Theme};

use crate::app::{use_dispatch, use_store};
use crate::components::{ColorField, Connected, SettingGroup, SettingRow, Switch, TabItem, Tabs};
use crate::i18n::tr;
use crate::route::{Route, SettingsTab};
use crate::screens::modes::widgets::use_control;
use crate::state::{Action, Origin, Store};

/// Seeds offered next to the color picker: the default and the accent of
/// each 1.x palette (`gui/config/palette.py`, its `control` color).
pub(crate) const SEEDS: [&str; 21] = [
    "#2F5F9E", "#FFCC66", "#FA8D3E", "#CBA6F7", "#FFC600", "#FF00FF", "#BD93F9", "#A7C080", "#58A6FF",
    "#83A598", "#FFFF00", "#7FB4CA", "#C792EA", "#FC9867", "#88C0D0", "#61AFEF", "#0184BC", "#C4A7E7",
    "#268BD2", "#FF7EDB", "#7AA2F7",
];

/// The Ajustes page (`Route::Settings`).
#[component]
pub fn SettingsPage(tab: SettingsTab) -> Element {
    let dispatch = use_dispatch();
    let tabs: Vec<TabItem<SettingsTab>> = SettingsTab::ALL
        .iter()
        .map(|t| TabItem {
            value: *t,
            label: tr(t.label_key()),
            icon: t.icon(),
            dot: false,
        })
        .collect();
    rsx! {
        div { class: "flex flex-col gap-2.5 px-10 pt-8 pb-12 max-w-[980px]",
            h1 { class: "t-hl m-0 mb-2", {t!("settings-title")} }
            Tabs { tabs, selected: tab, on_select: move |t| dispatch(Action::Navigate(Route::Settings { tab: t })) }
            div { role: "tabpanel", class: "flex flex-col gap-2.5",
                match tab {
                    SettingsTab::General => rsx! { General {} },
                    SettingsTab::Hotkeys => rsx! { hotkeys::Hotkeys {} },
                    SettingsTab::Style => rsx! { style::Style {} },
                }
            }
        }
    }
}

/// A `gui-config.yaml` value for a control: follows the store.
pub(crate) fn use_gui<T: Clone + PartialEq + 'static>(get: fn(&GuiConfig) -> T) -> Signal<T> {
    let store = use_store();
    let value = use_memo(move || get(&store.read().gui));
    use_control(value, Clone::clone)
}

/// Change `gui-config.yaml` (written at once by `SetGui`), if it changes.
pub(crate) fn write_gui(store: Signal<Store>, dispatch: Callback<Action>, edit: impl FnOnce(&mut GuiConfig)) {
    let mut gui = store.peek().gui.clone();
    edit(&mut gui);
    if gui != store.peek().gui {
        dispatch(Action::SetGui(gui));
    }
}

/// A section title of Ajustes.
#[component]
pub(crate) fn SectionTitle(title: String) -> Element {
    rsx! { h2 { class: "t-ts text-on-surface-variant m-0 mt-3 px-1", "{title}" } }
}

/// A row of Ajustes: settings here have no layers, so no inheritance note.
#[component]
pub(crate) fn PlainRow(icon: &'static str, title: String, subtitle: String, children: Element) -> Element {
    rsx! {
        SettingRow { icon, title, subtitle, origin: Origin::Inherited, {children} }
    }
}

/// Apariencia, Bandeja del sistema, Escalado automático.
#[component]
fn General() -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let language = use_gui(|g| g.language);
    let theme = use_gui(|g| g.theme);
    let seed = use_gui(|g| g.seed_color.clone());
    let show_tray = use_gui(|g| g.show_tray_icon);
    let close_to_tray = use_gui(|g| g.close_to_tray);
    let minimize_to_tray = use_gui(|g| g.minimize_to_tray);
    let start_hidden = use_gui(|g| g.start_hidden);
    // Without the icon there is no tray to close, minimize or start to.
    let no_tray = !show_tray();
    let keep_running = use_gui(|g| g.keep_running_after_exit);
    let daemon_value = use_memo(move || store.read().daemon.on);
    let daemon = use_control(daemon_value, |on| *on);
    let watching = store.read().daemon.watching;
    rsx! {
        SectionTitle { title: t!("settings-appearance") }
        SettingGroup {
            PlainRow { icon: "translate", title: t!("settings-language"), subtitle: t!("settings-language-help"),
                Connected {
                    options: vec![
                        (Language::System, t!("settings-language-system")),
                        // Each language in its own name.
                        (Language::Es, "Español".to_owned()),
                        (Language::En, "English".to_owned()),
                    ],
                    value: language,
                    label: t!("settings-language"),
                    on_change: move |l: Language| write_gui(store, dispatch, |g| g.language = l),
                }
            }
            PlainRow { icon: "contrast", title: t!("settings-theme"), subtitle: t!("settings-theme-help"),
                Connected {
                    options: vec![
                        (Theme::System, t!("settings-theme-system")),
                        (Theme::Light, t!("settings-theme-light")),
                        (Theme::Dark, t!("settings-theme-dark")),
                    ],
                    value: theme,
                    label: t!("settings-theme"),
                    on_change: move |t: Theme| write_gui(store, dispatch, |g| g.theme = t),
                }
            }
            PlainRow { icon: "palette", title: t!("settings-seed"), subtitle: t!("settings-seed-help"),
                ColorField {
                    value: seed,
                    label: t!("settings-seed"),
                    swatches: SEEDS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                    on_change: move |c: String| write_gui(store, dispatch, |g| g.seed_color = c),
                }
            }
        }

        SectionTitle { title: t!("settings-tray") }
        SettingGroup {
            PlainRow { icon: "toolbar", title: t!("settings-show-tray-icon"), subtitle: t!("settings-show-tray-icon-help"),
                Switch {
                    value: show_tray,
                    label: t!("settings-show-tray-icon"),
                    on_change: move |on: bool| write_gui(store, dispatch, |g| g.show_tray_icon = on),
                }
            }
            PlainRow { icon: "close", title: t!("settings-close-to-tray"), subtitle: t!("settings-close-to-tray-help"),
                Switch {
                    value: close_to_tray,
                    label: t!("settings-close-to-tray"),
                    disabled: no_tray,
                    on_change: move |on: bool| write_gui(store, dispatch, |g| g.close_to_tray = on),
                }
            }
            PlainRow { icon: "remove", title: t!("settings-minimize-to-tray"), subtitle: t!("settings-minimize-to-tray-help"),
                Switch {
                    value: minimize_to_tray,
                    label: t!("settings-minimize-to-tray"),
                    disabled: no_tray,
                    on_change: move |on: bool| write_gui(store, dispatch, |g| g.minimize_to_tray = on),
                }
            }
            PlainRow { icon: "visibility_off", title: t!("settings-start-hidden"), subtitle: t!("settings-start-hidden-help"),
                Switch {
                    value: start_hidden,
                    label: t!("settings-start-hidden"),
                    disabled: no_tray,
                    on_change: move |on: bool| write_gui(store, dispatch, |g| g.start_hidden = on),
                }
            }
        }

        SectionTitle { title: t!("settings-auto") }
        SettingGroup {
            PlainRow {
                icon: "auto_mode",
                title: t!("settings-daemon"),
                subtitle: if daemon() { t!("settings-daemon-watching", count = watching) } else { t!("settings-daemon-help") },
                Switch { value: daemon, label: t!("settings-daemon"), on_change: move |on: bool| dispatch(Action::SetDaemon { on }) }
            }
            PlainRow { icon: "power_settings_new", title: t!("settings-keep-running"), subtitle: t!("settings-keep-running-help"),
                Switch {
                    value: keep_running,
                    label: t!("settings-keep-running"),
                    on_change: move |on: bool| write_gui(store, dispatch, |g| g.keep_running_after_exit = on),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_are_colors() {
        for seed in SEEDS {
            assert!(seed.len() == 7 && seed.starts_with('#'), "{seed}");
            assert!(u32::from_str_radix(&seed[1..], 16).is_ok(), "{seed}");
        }
        assert_eq!(SEEDS[0], GuiConfig::default().seed_color);
    }
}
