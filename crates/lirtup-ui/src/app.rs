//! The app: the store and the link as contexts, the effect runner, and the
//! common frame of every screen (6.1 «Marco común»): the frameless window's
//! title bar, the drawer, the page the route names, the save bar, the
//! snackbar, the dialogs and the app shortcuts.
//!
//! The reviewer owns this file. Screens get what they need with
//! [`use_store`] and [`use_dispatch`]; nothing else writes the store.
//!
//! With `LIRTUP_GUI_GALLERY=1` the app shows only the component gallery,
//! without the store, the link or the screens (front A's bench).
//!
//! Test aids (screenshots, the reviewer's checks; input injection does not
//! reach XWayland windows under KWin):
//!
//! - `LIRTUP_GUI_ROUTE`: the first page (`home`, `modes`, `modes/<id>`,
//!   `profile/default/<tab>`, `profile/<name>/<tab>`, `settings/<tab>`,
//!   `about`, `gallery`; tabs as `scaling`, `presentation`, `display`,
//!   `extras`, `advanced`, `general`, `hotkeys`, `style`);
//! - `LIRTUP_GUI_THEME`: `dark` or `light`, over `gui-config.yaml`;
//! - `LIRTUP_GUI_EVAL`: a JavaScript file run in the page 2 s after start
//!   (clicks and key events on the DOM reach the Dioxus handlers).

use std::time::Duration;

use dioxus::desktop::tao::event::{Event as TaoEvent, WindowEvent};
use dioxus::desktop::{use_window, use_wry_event_handler};
use dioxus::prelude::*;
use lirtup_config::{ConfigFile, GuiConfig, ModeStore};

use crate::components::RowLayer;
use crate::components::{
    Button, ButtonVariant, FloatingToolbar, Gallery, IconButton, MenuItem, NavDrawer, NavHeader, NavIcon,
    NavItem, Snackbar, SplitButton,
};
use crate::i18n::{LANG, Lang};
use crate::link::{self, CoreLink};
use crate::route::{ProfileRef, ProfileTab, Route, SettingsTab};
use crate::screens;
use crate::state::{Action, Dialog, Effect, LinkState, Store, Toast, Written, update};
use crate::theme::{self, SCHEME};
use crate::{thumbs, windows};

/// The snackbar goes away after this long.
const TOAST_TIME: Duration = Duration::from_secs(6);
/// Below this window width the drawer becomes a rail.
const RAIL_BELOW: f64 = 1100.0;

/// Bumped by F2: the profile page opens «Editar perfil».
pub static EDIT_PROFILE: GlobalSignal<u64> = Signal::global(|| 0);

/// Bumped by Ctrl+F: Inicio's filter field takes the focus when it changes.
pub static FOCUS_FILTER: GlobalSignal<u64> = Signal::global(|| 0);

/// The store (a context of the app).
pub fn use_store() -> Signal<Store> {
    use_context::<Signal<Store>>()
}

/// The dispatcher: runs `update` and the effect it returns (a context of
/// the app). `Callback` is `Copy`; call it as `dispatch(action)`.
pub fn use_dispatch() -> Callback<Action> {
    use_context::<Callback<Action>>()
}

/// The root component.
#[component]
pub fn App() -> Element {
    if std::env::var_os("LIRTUP_GUI_GALLERY").is_some() {
        return rsx! { GalleryRoot {} };
    }
    rsx! { Root {} }
}

/// Only the gallery, in the frame's theme.
#[component]
fn GalleryRoot() -> Element {
    let style = theme::root_style("#2F5F9E", *SCHEME.read());
    rsx! {
        Stylesheets {}
        div { class: "lf-root h-screen overflow-auto bg-surface text-on-surface font-brand", "data-theme": scheme_name(), style, Gallery {} }
    }
}

/// The stylesheet (`tailwind.css` compiled to `assets/tailwind.css`) and
/// the fonts it names.
#[component]
fn Stylesheets() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/tailwind.css") }
    }
}

fn scheme_name() -> &'static str {
    match *SCHEME.read() {
        theme::Scheme::Dark => "dark",
        theme::Scheme::Light => "light",
    }
}

#[component]
fn Root() -> Element {
    let mut store = use_context_provider(|| Signal::new(Store::default()));
    let (core, rx) = use_hook(|| {
        let (core, rx) = CoreLink::new();
        (core, std::rc::Rc::new(std::cell::Cell::new(Some(rx))))
    });
    use_context_provider(|| core.clone());

    let effect_core = core.clone();
    let dispatch = use_callback(move |action: Action| {
        let effect = update(&mut store.write(), action);
        if let Some(effect) = effect {
            run_effect(effect, &effect_core, store);
        }
    });
    use_context_provider(|| dispatch);

    // The link task, once.
    use_hook(move || {
        if let Some(rx) = rx.take() {
            link::spawn(rx, move |action| dispatch.call(action));
        }
    });
    // The files, once (small and local: read on the UI thread).
    use_hook(move || dispatch(load_files()));
    use_hook(move || test_aids(dispatch));
    // Window list and previews (front B).
    windows::use_window_list(dispatch);
    let thumbs = thumbs::use_thumbnails();
    use_context_provider(|| thumbs);
    // Tray and notifications (round 4, front Y).
    crate::tray::use_tray(store, dispatch);
    crate::notify::use_notifications(store, dispatch);

    // The origin notes of the rows depend on the page's layer.
    let layer = use_memo(move || match &store.read().route {
        Route::Profile {
            id: ProfileRef::Default,
            ..
        }
        | Route::Settings { .. } => RowLayer::Default,
        Route::Modes { .. } => RowLayer::Mode,
        _ => RowLayer::Profile,
    });
    use_context_provider(|| layer);

    // Language and theme follow gui-config.yaml.
    use_effect(move || {
        let gui = store.read().gui.clone();
        *LANG.write() = Lang::from_config(gui.language);
        *SCHEME.write() = forced_scheme().unwrap_or_else(|| theme::resolve(gui.theme));
    });

    // Rail below 1100 px.
    let window = use_window();
    let mut rail = use_signal(|| false);
    use_wry_event_handler(move |event, _| {
        if let TaoEvent::WindowEvent {
            event: WindowEvent::Resized(size),
            ..
        } = event
        {
            let width = size.width as f64 / window.window.scale_factor();
            rail.set(width < RAIL_BELOW);
        }
    });

    let seed = store.read().gui.seed_color.clone();
    let style = theme::root_style(&seed, *SCHEME.read());
    rsx! {
        Stylesheets {}
        div {
            class: "lf-root flex flex-col h-screen overflow-hidden bg-surface text-on-surface font-brand select-none",
            "data-theme": scheme_name(),
            style,
            tabindex: "-1",
            onkeydown: move |e| app_shortcut(e, dispatch, store),
            TitleBar {}
            div { class: "flex flex-1 min-h-0",
                Drawer { rail: rail() }
                main { class: "relative flex-1 min-w-0 overflow-auto", Page {} }
            }
            SaveBar {}
            Toaster {}
            Dialogs {}
        }
    }
}

/// `LIRTUP_GUI_THEME` (module docs).
fn forced_scheme() -> Option<theme::Scheme> {
    match std::env::var("LIRTUP_GUI_THEME").ok()?.as_str() {
        "light" => Some(theme::Scheme::Light),
        "dark" => Some(theme::Scheme::Dark),
        _ => None,
    }
}

/// `LIRTUP_GUI_ROUTE` and `LIRTUP_GUI_EVAL` (module docs).
fn test_aids(dispatch: Callback<Action>) {
    if let Some(route) = std::env::var("LIRTUP_GUI_ROUTE")
        .ok()
        .and_then(|r| parse_route(&r))
    {
        dispatch(Action::Navigate(route));
    }
    if let Some(path) = std::env::var_os("LIRTUP_GUI_EVAL") {
        match std::fs::read_to_string(&path) {
            Ok(script) => {
                spawn(async move {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    if let Err(e) = document::eval(&script).await {
                        tracing::warn!("LIRTUP_GUI_EVAL: {e}");
                    }
                });
            }
            Err(e) => tracing::warn!("LIRTUP_GUI_EVAL {}: {e}", std::path::Path::new(&path).display()),
        }
    }
}

/// A route as `LIRTUP_GUI_ROUTE` spells it.
fn parse_route(text: &str) -> Option<Route> {
    let profile_tab = |t: &str| {
        ProfileTab::ALL
            .into_iter()
            .find(|tab| format!("{tab:?}").eq_ignore_ascii_case(t))
    };
    let settings_tab = |t: &str| {
        SettingsTab::ALL
            .into_iter()
            .find(|tab| format!("{tab:?}").eq_ignore_ascii_case(t))
    };
    let parts: Vec<&str> = text.splitn(3, '/').collect();
    Some(match parts.as_slice() {
        ["home"] => Route::Home,
        ["about"] => Route::About,
        ["gallery"] => Route::Gallery,
        ["modes"] => Route::Modes { id: None },
        ["modes", id] => Route::Modes {
            id: Some((*id).into()),
        },
        ["settings"] => Route::Settings {
            tab: SettingsTab::General,
        },
        ["settings", tab] => Route::Settings {
            tab: settings_tab(tab)?,
        },
        ["profile", name, rest @ ..] => Route::Profile {
            id: if *name == "default" {
                ProfileRef::Default
            } else {
                ProfileRef::Named((*name).into())
            },
            tab: match rest.first() {
                Some(tab) => profile_tab(tab)?,
                None => ProfileTab::default(),
            },
        },
        _ => return None,
    })
}

/// Read `config.yaml`, `modes/` and `gui-config.yaml`. A file that cannot be
/// read leaves its defaults and a log line (the scaler reports the same
/// warnings on its side).
fn load_files() -> Action {
    let config = ConfigFile::load(&crate::config_path())
        .map(|(f, _)| f)
        .unwrap_or_else(|e| {
            tracing::error!("config.yaml: {e}");
            ConfigFile::default()
        });
    let modes = ModeStore::load(&crate::modes_dir())
        .map(|(m, _)| m)
        .unwrap_or_else(|e| {
            tracing::error!("modes: {e}");
            ModeStore::default()
        });
    let gui = GuiConfig::load(None).unwrap_or_else(|e| {
        tracing::error!("gui-config.yaml: {e}");
        GuiConfig::default()
    });
    Action::Loaded { config, modes, gui }
}

/// Run an effect. Writes happen on a blocking thread and come back as
/// [`Action::Written`] through the dispatcher (a context of `Root`, where
/// the tasks are spawned).
fn run_effect(effect: Effect, core: &CoreLink, store: Signal<Store>) {
    fn written(result: Result<Written, String>) {
        if let Some(dispatch) = try_consume_context::<Callback<Action>>() {
            dispatch(Action::Written(result));
        }
    }
    match effect {
        Effect::Send(command) => core.send(command),
        Effect::WriteConfig(file) => {
            spawn(async move {
                let result = tokio::task::spawn_blocking(move || file.save()).await;
                written(flatten(result).map(|()| Written::Config));
            });
        }
        Effect::WriteMode { id, raw, .. } => {
            let dir = store.peek().modes.store.dir.clone();
            spawn(async move {
                let saved = id.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let mut modes = ModeStore {
                        dir,
                        ..ModeStore::default()
                    };
                    modes.save_raw(&id, raw).map(|_| ())
                })
                .await;
                written(flatten(result).map(|()| Written::Mode(saved)));
            });
        }
        Effect::DeleteModeFile(id) => {
            let dir = store.peek().modes.store.dir.clone();
            spawn(async move {
                let deleted = id.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let mut modes = ModeStore {
                        dir,
                        ..ModeStore::default()
                    };
                    modes.delete(&id)
                })
                .await;
                written(flatten(result).map(|()| Written::ModeDeleted(deleted)));
            });
        }
        Effect::WriteGui(gui) => {
            spawn(async move {
                let result = tokio::task::spawn_blocking(move || gui.save(None)).await;
                written(flatten(result).map(|()| Written::Gui));
            });
        }
        Effect::Toast(toast) => dismiss_later(toast, store),
        Effect::WriteIcon { path, png } => {
            if !in_icons_dir(&path) {
                tracing::error!("refusing to touch {} (not in icons/)", path.display());
                return;
            }
            let result = match png {
                Some(bytes) => path
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&path, bytes)),
                None => match std::fs::remove_file(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                },
            };
            if let Err(e) = result {
                tracing::warn!("profile icon {}: {e}", path.display());
            }
        }
        Effect::MoveIcon { from, to } => {
            if in_icons_dir(&from) && in_icons_dir(&to) {
                if let Err(e) = std::fs::rename(&from, &to) {
                    tracing::warn!("profile icon {} → {}: {e}", from.display(), to.display());
                }
            }
        }
        Effect::Migrate(plan) => {
            spawn(async move {
                let result = tokio::task::spawn_blocking(move || {
                    let (mut file, _) = ConfigFile::load(&crate::config_path())?;
                    let (mut modes, _) = ModeStore::load(&crate::modes_dir())?;
                    lirtup_config::migrate::apply(&mut file, &mut modes, &plan)
                })
                .await;
                let result = match result {
                    Ok(Ok(backup)) => Ok(backup.display().to_string()),
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
                let ok = result.is_ok();
                if let Some(dispatch) = try_consume_context::<Callback<Action>>() {
                    dispatch(Action::Migrated(result));
                    if ok {
                        dispatch(load_files());
                    }
                }
            });
        }
        Effect::Many(effects) => {
            for effect in effects {
                run_effect(effect, core, store);
            }
        }
    }
}

/// Dismiss `toast` after [`TOAST_TIME`], unless another replaced it.
fn dismiss_later(toast: Toast, store: Signal<Store>) {
    spawn(async move {
        tokio::time::sleep(TOAST_TIME).await;
        if store.peek().toast.as_ref() == Some(&toast)
            && let Some(dispatch) = try_consume_context::<Callback<Action>>()
        {
            dispatch(Action::DismissToast);
        }
    });
}

/// A profile icon the GUI may write, move or remove: a `.png` right inside
/// the `icons/` folder next to `config.yaml`.
fn in_icons_dir(path: &std::path::Path) -> bool {
    let icons = crate::config_path().parent().map(|d| d.join("icons"));
    path.parent() == icons.as_deref() && path.extension().is_some_and(|e| e == "png")
}

fn flatten(
    result: Result<Result<(), lirtup_config::ConfigError>, tokio::task::JoinError>,
) -> Result<(), String> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Ctrl+S save, Ctrl+N new profile, Ctrl+D duplicate the profile shown,
/// Ctrl+F the window filter of Inicio, Ctrl+Shift+G the gallery; and from
/// 1.x Ctrl+Q quit, F2 edit and Supr delete the profile shown,
/// Ctrl+Shift+↑/↓ move it.
fn app_shortcut(event: KeyboardEvent, dispatch: Callback<Action>, store: Signal<Store>) {
    let mods = event.modifiers();
    let profile = match store.peek().route.profile() {
        Some(ProfileRef::Named(name)) => Some(name.clone()),
        _ => None,
    };
    // Without Ctrl (1.x): F2 edits the profile shown, Supr deletes it (not
    // while typing in a field).
    if !mods.ctrl() && !mods.alt() && !mods.shift() {
        match (event.key(), profile) {
            (Key::F2, Some(_)) => {
                event.prevent_default();
                *EDIT_PROFILE.write() += 1;
            }
            (Key::Delete, Some(name)) => {
                spawn(async move {
                    let typing = document::eval(
                        "return ['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName ?? '')",
                    )
                    .await
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                    if !typing {
                        dispatch(Action::OpenDialog(Dialog::DeleteProfile(name)));
                    }
                });
            }
            _ => {}
        }
        return;
    }
    if !mods.ctrl() {
        return;
    }
    // Ctrl+Shift+↑/↓: move the profile shown (the first match wins).
    if mods.shift()
        && let Some(name) = profile.clone()
    {
        let by = match event.key() {
            Key::ArrowUp => -1,
            Key::ArrowDown => 1,
            _ => 0,
        };
        if by != 0 {
            event.prevent_default();
            dispatch(Action::MoveProfile { profile: name, by });
            return;
        }
    }
    let key = event.key().to_string().to_lowercase();
    if key == "q" && !mods.shift() {
        // Ctrl+Q: «Salir», as the tray (asks about unsaved edits).
        event.prevent_default();
        if let Some(core) = try_consume_context::<CoreLink>() {
            crate::tray::request_quit(store, &dioxus::desktop::window(), &core, dispatch);
        }
        return;
    }
    let action = match (key.as_str(), mods.shift()) {
        ("s", false) => Some(Action::Save),
        ("n", false) => Some(Action::OpenDialog(Dialog::NewProfile { from: None })),
        ("d", false) => match store.peek().route.profile() {
            Some(ProfileRef::Named(name)) => Some(Action::DuplicateProfile(name.clone())),
            _ => None,
        },
        ("f", false) => {
            *FOCUS_FILTER.write() += 1;
            Some(Action::Navigate(Route::Home))
        }
        ("g", true) => Some(Action::Navigate(Route::Gallery)),
        _ => None,
    };
    if let Some(action) = action {
        event.prevent_default();
        dispatch(action);
    }
}

/// The 40 px title bar: drag to move, double click to maximize, minimize,
/// maximize and close. Minimize and close go to the tray when Ajustes says
/// so, and close quits otherwise, asking about unsaved edits (round 4,
/// front Y: `crate::tray`).
#[component]
fn TitleBar() -> Element {
    let window = use_window();
    let store = use_store();
    let dispatch = use_dispatch();
    let core = use_context::<CoreLink>();
    let drag = window.clone();
    let max = window.clone();
    let max2 = window.clone();
    let min = window.clone();
    let close = window.clone();
    let to_tray = {
        let gui = &store.read().gui;
        gui.show_tray_icon && gui.close_to_tray
    };
    rsx! {
        header {
            class: "flex items-center h-10 shrink-0 pl-4 pr-1 gap-2 bg-surface-container-low",
            onmousedown: move |_| drag.drag(),
            ondoubleclick: move |_| max.toggle_maximized(),
            span { class: "t-ts flex-1 truncate", {t!("app-title")} }
            div {
                class: "flex items-center gap-1",
                // Buttons must not start a drag.
                onmousedown: move |e| e.stop_propagation(),
                IconButton { icon: "remove", label: t!("titlebar-minimize"), size: 32, tip: "right", onclick: move |_| crate::tray::minimize_window(&min, store, dispatch) }
                IconButton { icon: "crop_square", label: t!("titlebar-maximize"), size: 32, tip: "right", onclick: move |_| max2.toggle_maximized() }
                IconButton {
                    icon: "close",
                    label: if to_tray { t!("titlebar-close-to-tray") } else { t!("titlebar-close") },
                    size: 32,
                    tip: "right",
                    onclick: move |_| crate::tray::close_window(&close, store, &core, dispatch, false),
                }
            }
        }
        crate::tray::QuitDialog {}
    }
}

/// The drawer: Inicio, Modos, the profiles (dirty dot, running ▶), Nuevo
/// perfil; Acerca de and Ajustes at the bottom.
#[component]
fn Drawer(rail: bool) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let s = store.read();
    let route = s.route.clone();
    let running = s.running_profile();
    let profiles: Vec<(ProfileRef, bool)> = s
        .profiles()
        .into_iter()
        .map(|p| {
            let dirty = s.profile_dirty(&p);
            (p, dirty)
        })
        .collect();
    drop(s);
    let go = move |to: Route| move |_| dispatch(Action::Navigate(to.clone()));
    rsx! {
        NavDrawer {
            rail,
            footer: rsx! {
                NavItem { label: t!("nav-about"), icon: NavIcon::Symbol("info"), selected: route == Route::About, onclick: go(Route::About) }
                NavItem {
                    label: t!("nav-settings"),
                    icon: NavIcon::Symbol("settings"),
                    selected: matches!(route, Route::Settings { .. }),
                    onclick: go(Route::Settings { tab: SettingsTab::General }),
                }
            },
            NavItem { label: t!("nav-home"), icon: NavIcon::Symbol("home"), selected: route == Route::Home, onclick: go(Route::Home) }
            NavItem {
                label: t!("nav-modes"),
                icon: NavIcon::Symbol("tune"),
                selected: matches!(route, Route::Modes { .. }),
                onclick: go(Route::Modes { id: None }),
            }
            NavHeader { label: t!("nav-profiles") }
            for (profile, dirty) in profiles {
                NavItem {
                    key: "{profile_key(&profile)}",
                    label: profile_label(&profile),
                    icon: profile_icon(&profile),
                    selected: route.profile() == Some(&profile),
                    dirty,
                    running: running.as_ref() == Some(&profile),
                    onclick: go(Route::Profile { id: profile.clone(), tab: ProfileTab::default() }),
                }
            }
            NavItem {
                label: t!("nav-new-profile"),
                icon: NavIcon::Symbol("add"),
                onclick: move |_| dispatch(Action::OpenDialog(Dialog::NewProfile { from: None })),
            }
        }
    }
}

fn profile_key(profile: &ProfileRef) -> String {
    match profile {
        ProfileRef::Default => String::new(),
        ProfileRef::Named(name) => format!("p:{name}"),
    }
}

fn profile_label(profile: &ProfileRef) -> String {
    match profile {
        ProfileRef::Default => t!("nav-default-profile"),
        ProfileRef::Named(name) => name.clone(),
    }
}

/// Initials on a color from the name's hash (the profile's own icon is
/// front C's to add: `icon:` key, served as an asset).
fn profile_icon(profile: &ProfileRef) -> NavIcon {
    match profile {
        ProfileRef::Default => NavIcon::Symbol("star"),
        ProfileRef::Named(name) => {
            let initials: String = name
                .split_whitespace()
                .filter_map(|w| w.chars().next())
                .take(2)
                .flat_map(char::to_uppercase)
                .collect();
            const COLORS: [&str; 6] = ["#3D6FB6", "#8A4FA8", "#B3641E", "#2E7D6B", "#A8455A", "#5B6B2F"];
            let hash = name
                .bytes()
                .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
            NavIcon::Profile {
                initials,
                color: COLORS[hash as usize % COLORS.len()].into(),
                image: None,
            }
        }
    }
}

/// The page of the route, with the link's error card over Inicio.
#[component]
fn Page() -> Element {
    let store = use_store();
    let route = store.read().route.clone();
    let link = store.read().link.clone();
    rsx! {
        if let LinkState::Down { reason } = link {
            div { class: "m-4 p-4 rounded-lg bg-error-container text-on-error-container t-bm",
                {t!("link-down", reason = reason)}
            }
        }
        match route {
            Route::Home => rsx! { screens::home::Home {} },
            Route::Modes { id } => rsx! { screens::modes::ModesPage { id } },
            Route::Profile { id, tab } => rsx! { screens::profile::ProfilePage { id, tab } },
            Route::Settings { tab } => rsx! { screens::settings::SettingsPage { tab } },
            Route::About => rsx! { screens::about::About {} },
            Route::Gallery => rsx! { Gallery {} },
        }
    }
}

/// The floating save bar: shown with edits on the page's scope.
#[component]
fn SaveBar() -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let scope = screens::profile::save_bar_scope(&store.read());
    let open = scope.is_some();
    let count = scope.as_ref().map_or(0, |(_, n)| *n);
    let route = store.read().route.clone();
    let menu = vec![
        MenuItem::new("tab", t!("savebar-reset-tab")).icon("undo"),
        MenuItem::new("overrides", t!("savebar-remove-overrides")).icon("layers_clear"),
        MenuItem {
            divider_before: true,
            ..MenuItem::new(
                "all",
                match route.profile() {
                    Some(ProfileRef::Named(_)) => t!("profile-reset-all-named"),
                    _ => t!("savebar-reset-all"),
                },
            )
            .icon("restart_alt")
        },
    ];
    let reset = move |id: &'static str| {
        let Route::Profile { id: profile, tab } = route.clone() else {
            return;
        };
        let action = match id {
            "tab" => Action::ResetTab { profile, tab },
            "overrides" => Action::RemoveProfileOverrides { profile, tab },
            _ => Action::ResetAll { profile },
        };
        dispatch(action);
    };
    let reset_main = reset.clone();
    rsx! {
        FloatingToolbar { open,
            span { class: "dot" }
            span { class: "t-ll mr-2", {t!("savebar-changes", count = count)} }
            SplitButton {
                label: t!("savebar-reset"),
                variant: ButtonVariant::Outlined,
                menu_label: t!("savebar-reset-more"),
                on_click: move |_| reset_main("tab"),
                menu,
                on_select: reset,
            }
            Button { label: t!("savebar-save"), title: "Ctrl+S".to_string(), onclick: move |_| dispatch(Action::Save) }
        }
    }
}

/// The snackbar.
#[component]
fn Toaster() -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let Some(toast) = store.read().toast.clone() else {
        return rsx! {};
    };
    let args: Vec<(
        &'static str,
        fluent_templates::fluent_bundle::FluentValue<'static>,
    )> = toast
        .args
        .iter()
        .map(|(k, v)| (*k, crate::i18n::arg(v.clone())))
        .collect();
    let message = crate::i18n::tr_args(toast.key, &args);
    rsx! {
        Snackbar {
            message,
            action: toast.undo.then(|| t!("toast-undo")),
            on_action: move |_| dispatch(Action::Undo),
            on_close: move |_| dispatch(Action::DismissToast),
        }
    }
}

/// The dialog of the store, if any.
#[component]
fn Dialogs() -> Element {
    let store = use_store();
    let dialog = store.read().dialog.clone();
    match dialog {
        None => rsx! {},
        Some(Dialog::Unsaved { to }) => rsx! { screens::profile::UnsavedDialog { to } },
        Some(Dialog::NewProfile { from }) => rsx! { screens::new_profile::NewProfileDialog { from } },
        Some(Dialog::Bench { mode }) => rsx! { screens::bench::BenchDialog { mode } },
        Some(Dialog::EffectPicker { mode }) => rsx! { screens::modes::EffectPickerDialog { mode } },
        Some(Dialog::DeleteProfile(name)) => rsx! { screens::profile::DeleteProfileDialog { name } },
        Some(Dialog::DeleteMode(id)) => rsx! { screens::modes::DeleteModeDialog { id } },
        Some(Dialog::Migrate(plan)) => rsx! { screens::migrate::MigrateDialog { plan } },
    }
}
