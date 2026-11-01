//! A profile page (front C): header with the rules as chips, the five tabs
//! (`tabs/` is C's to split as it likes), inheritance per row, Restablecer
//! and the save bar's content. Also the dialogs of profiles.
//!
//! - Every row reads its [`crate::state::Field`] from the store and shows
//!   its origin; edits go out as `SetField` / `Inherit` and stay unsaved
//!   until Guardar (the save bar of `app.rs`).
//! - «Editar perfil» opens the name and the rules in the header itself (the
//!   rename writes at once, like 1.x; the rules are an edit until Guardar).

pub mod framegen;
pub mod rows;
pub mod tabs;
pub mod unsaved;

pub use unsaved::UnsavedDialog;

use dioxus::prelude::*;
use lirtup_config::Match;

use crate::app::{use_dispatch, use_store};
use crate::components::{
    Button, ButtonVariant, Chip, Dialog, Icon, IconButton, IconButtonVariant, Menu, MenuItem, TabItem, Tabs,
};
use crate::i18n::{arg, tr, tr_args};
use crate::route::{ProfileName, ProfileRef, ProfileTab, Route};
use crate::screens::new_profile::{MatchPreview, Rule, RuleEditor, match_of, rules_of};
use crate::state::{Action, Dialog as StoreDialog, Scope};

/// The profile page (`Route::Profile`).
#[component]
pub fn ProfilePage(id: ProfileRef, tab: ProfileTab) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    // Monitors and GPUs for Pantalla, once.
    use_effect(move || {
        let s = store.peek();
        if s.monitors.is_empty() && s.gpus.is_empty() {
            drop(s);
            dispatch(Action::ListDevices);
        }
    });
    let tabs: Vec<TabItem<ProfileTab>> = {
        let s = store.read();
        ProfileTab::ALL
            .into_iter()
            .map(|t| TabItem {
                value: t,
                label: tr(t.label_key()),
                icon: t.icon(),
                dot: s.tab_dirty(&id, t),
            })
            .collect()
    };
    let scope = Scope::Profile(id.clone());
    let key = format!("{}-{tab:?}", page_key(&id));
    let go = id.clone();
    rsx! {
        div { class: "max-w-[960px] mx-auto px-6 pt-6 pb-32 flex flex-col gap-6",
            ProfileHeader { key: "{page_key(&id)}", id: id.clone() }
            Tabs {
                tabs,
                selected: tab,
                on_select: move |t| dispatch(Action::Navigate(Route::Profile { id: go.clone(), tab: t })),
            }
            div { class: "flex flex-col gap-6", role: "tabpanel",
                // rsx keeps a key only inside a list: a list of one, so each
                // page and tab mounts fresh.
                for k in [key] {
                    tabs::TabContent { key: "{k}", scope: scope.clone(), tab }
                }
            }
        }
    }
}

fn page_key(id: &ProfileRef) -> String {
    match id {
        ProfileRef::Default => "default".into(),
        ProfileRef::Named(name) => format!("p:{name}"),
    }
}

/// The text of one rule chip.
pub fn rule_chips(matcher: &Match) -> Vec<(&'static str, String)> {
    let mut chips = Vec::new();
    let text = |key: &'static str, value: &Option<String>, chips: &mut Vec<(&'static str, String)>| {
        if let Some(v) = value {
            chips.push(("title", tr_args(key, &[("value", arg(v.clone()))])));
        }
    };
    text("profile-rule-title", &matcher.title, &mut chips);
    text("profile-rule-title-contains", &matcher.title_contains, &mut chips);
    text("profile-rule-title-regex", &matcher.title_regex, &mut chips);
    text(
        "profile-rule-title-startswith",
        &matcher.title_startswith,
        &mut chips,
    );
    text("profile-rule-title-endswith", &matcher.title_endswith, &mut chips);
    let size = |i: &lirtup_config::Interval| match i {
        lirtup_config::Interval::Number(n) => format!("= {n}"),
        lirtup_config::Interval::Text(t) => t.clone(),
    };
    if let Some(w) = &matcher.width {
        chips.push(("width", tr_args("profile-rule-width", &[("value", arg(size(w)))])));
    }
    if let Some(h) = &matcher.height {
        chips.push((
            "height",
            tr_args("profile-rule-height", &[("value", arg(size(h)))]),
        ));
    }
    for key in matcher.extra.keys() {
        let name = key.as_str().unwrap_or_default().to_owned();
        chips.push(("help", tr_args("profile-rule-unknown", &[("value", arg(name))])));
    }
    chips
}

/// The profile's icon, as a `data:` URL (the `icon:` key of 1.x, a PNG next
/// to the config).
fn icon_url(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        crate::screens::new_profile::base64(&bytes)
    ))
}

/// Initials on a color from the name (as the drawer does).
pub fn initials(name: &str) -> (String, &'static str) {
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
    (initials, COLORS[hash as usize % COLORS.len()])
}

/// Icon, name, rules as chips, «Editar perfil» and the ⋮ menu.
#[component]
fn ProfileHeader(id: ProfileRef) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let mut editing = use_signal(|| false);
    let mut menu = use_signal(|| false);
    // F2 (`app.rs`): open the editor. The first run only reads the counter.
    let mut seen_f2 = use_signal(|| *crate::app::EDIT_PROFILE.peek());
    use_effect(move || {
        let bumped = *crate::app::EDIT_PROFILE.read();
        if bumped != *seen_f2.peek() {
            seen_f2.set(bumped);
            editing.set(true);
        }
    });
    let ProfileRef::Named(name) = id.clone() else {
        return rsx! {
            header { class: "flex items-center gap-4",
                span { class: "size-16 rounded-lg bg-primary-container text-on-primary-container grid place-items-center",
                    Icon { name: "layers", size: 32 }
                }
                div { class: "flex flex-col gap-1",
                    h1 { class: "t-hl", {tr("profile-default-title")} }
                    span { class: "t-bm text-on-surface-variant", {tr("profile-default-subtitle")} }
                }
            }
        };
    };
    let s = store.read();
    let matcher: Option<Match> = s
        .field(&Scope::Profile(id.clone()), crate::state::store::MATCH_KEY)
        .value();
    let icon = s
        .config
        .file
        .profiles
        .get(&name)
        .and_then(|p| p.extra.get("icon"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    drop(s);
    let icon = use_memo(use_reactive((&icon,), |(icon,)| {
        icon.as_deref().and_then(icon_url)
    }));
    let (letters, color) = initials(&name);
    let chips = matcher.as_ref().map(rule_chips).unwrap_or_default();
    let items = vec![
        MenuItem::new("duplicate", tr("profile-duplicate")).icon("content_copy"),
        MenuItem {
            danger: true,
            divider_before: true,
            ..MenuItem::new("delete", tr("profile-delete")).icon("delete")
        },
    ];
    let menu_name = name.clone();
    rsx! {
        header { class: "flex items-start gap-4 flex-wrap",
            if let Some(url) = icon() {
                img { class: "size-16 rounded-lg object-cover", src: "{url}", alt: "" }
            } else {
                span {
                    class: "size-16 rounded-lg grid place-items-center t-tl text-white",
                    style: "background: {color}",
                    "aria-hidden": "true",
                    "{letters}"
                }
            }
            div { class: "flex-1 min-w-0 flex flex-col gap-2",
                h1 { class: "t-hl truncate", "{name}" }
                div { class: "flex flex-wrap gap-2",
                    if chips.is_empty() {
                        span { class: "t-bs text-on-surface-variant", {tr("profile-no-rules")} }
                    }
                    for (i, (icon, label)) in chips.into_iter().enumerate() {
                        Chip { key: "{i}", label, icon: chip_icon(icon) }
                    }
                }
            }
            Button {
                label: tr(if editing() { "profile-edit-done" } else { "profile-edit" }),
                variant: ButtonVariant::Outlined,
                icon: Some(if editing() { "check" } else { "edit" }),
                onclick: move |_| editing.toggle(),
            }
            div { class: "relative",
                IconButton {
                    icon: "more_vert",
                    label: tr("profile-more"),
                    variant: IconButtonVariant::Outlined,
                    onclick: move |_| menu.set(true),
                }
                Menu {
                    open: menu,
                    items,
                    on_select: move |id: &'static str| {
                        menu.set(false);
                        match id {
                            "duplicate" => dispatch(Action::DuplicateProfile(menu_name.clone())),
                            _ => dispatch(Action::OpenDialog(StoreDialog::DeleteProfile(menu_name.clone()))),
                        }
                    },
                }
            }
        }
        if editing() {
            ProfileEditor { name: name.clone(), matcher, on_done: move |_| editing.set(false) }
        }
    }
}

fn chip_icon(kind: &'static str) -> Option<&'static str> {
    Some(match kind {
        "title" => "title",
        "width" => "width",
        "height" => "height",
        _ => "help",
    })
}

/// The header's editor: the name (renamed on Listo) and the rules (an edit
/// until Guardar), with the match preview.
#[component]
fn ProfileEditor(name: ProfileName, matcher: Option<Match>, on_done: EventHandler<()>) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let mut new_name = use_signal(|| name.clone());
    let rules: Signal<Vec<Rule>> = use_signal(|| matcher.as_ref().map(rules_of).unwrap_or_default());
    let taken = {
        let s = store.read();
        let lower = new_name().trim().to_lowercase();
        lower != name.to_lowercase() && s.config.file.profiles.keys().any(|n| n.to_lowercase() == lower)
    };
    let error = if new_name().trim().is_empty() {
        Some(tr("new-profile-name-empty"))
    } else if taken {
        Some(tr("new-profile-name-taken"))
    } else {
        None
    };
    // The rules go to the store as they change (unsaved, counted by the bar).
    let rules_name = name.clone();
    use_effect(move || {
        let matcher = match_of(&rules.read());
        dispatch(Action::SetMatch {
            profile: rules_name.clone(),
            matcher,
        });
    });
    let current = match_of(&rules.read());
    let ok = error.is_none();
    rsx! {
        section { class: "flex flex-col gap-4 p-5 rounded-lg bg-surface-container",
            crate::components::TextField {
                value: new_name,
                label: tr("profile-name"),
                error,
                on_change: move |v: String| new_name.set(v),
            }
            ProfileIconRow { name: name.clone(), matcher: current.clone() }
            RuleEditor { rules }
            MatchPreview { matcher: current }
            div { class: "flex justify-end",
                Button {
                    label: tr("profile-edit-done"),
                    icon: "check",
                    disabled: !ok,
                    onclick: move |_| {
                        let to = new_name.peek().trim().to_owned();
                        if to != name {
                            dispatch(Action::RenameProfile { from: name.clone(), to });
                        }
                        on_done.call(());
                    },
                }
            }
        }
    }
}

/// Change the icon after creating the profile (1.x `dialogs/profile.py`):
/// from the first open window the rules match, from a file, or remove it.
/// Saved at once (`Action::SetProfileIcon`).
#[component]
fn ProfileIconRow(name: ProfileName, matcher: Option<Match>) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let window = matcher
        .as_ref()
        .and_then(|m| store.read().windows_matching(m).first().map(|w| w.xid));
    let has_icon = store
        .read()
        .config
        .file
        .profiles
        .get(&name)
        .is_some_and(|p| p.extra.contains_key("icon"));
    let capture_name = name.clone();
    let file_name = name.clone();
    let remove_name = name.clone();
    rsx! {
        div { class: "flex items-center gap-2",
            span { class: "t-bm text-on-surface-variant flex-1", {tr("profile-icon")} }
            IconButton {
                icon: "screenshot_region",
                label: tr("new-profile-icon-capture"),
                size: 32,
                disabled: window.is_none(),
                onclick: move |_| {
                    if let Some(xid) = window {
                        let profile = capture_name.clone();
                        spawn(async move {
                            if let Some(png) = crate::screens::new_profile::capture_icon(xid).await {
                                dispatch(Action::SetProfileIcon { profile, png: Some(png) });
                            }
                        });
                    }
                },
            }
            label { class: "relative inline-grid", title: tr("new-profile-icon-file"),
                input {
                    r#type: "file",
                    accept: "image/*",
                    class: "absolute inset-0 opacity-0 cursor-pointer",
                    "aria-label": tr("new-profile-icon-file"),
                    onchange: move |e: FormEvent| {
                        if let Some(file) = e.files().into_iter().next() {
                            let profile = file_name.clone();
                            spawn(async move {
                                let Ok(bytes) = file.read_bytes().await else { return };
                                let png = image::load_from_memory(&bytes)
                                    .ok()
                                    .and_then(crate::screens::new_profile::png_icon);
                                if png.is_some() {
                                    dispatch(Action::SetProfileIcon { profile, png });
                                }
                            });
                        }
                    },
                }
                span { class: "size-8 grid place-items-center rounded-full", "aria-hidden": "true",
                    Icon { name: "upload_file", size: 20 }
                }
            }
            IconButton {
                icon: "delete",
                label: tr("new-profile-icon-remove"),
                size: 32,
                disabled: !has_icon,
                onclick: move |_| dispatch(Action::SetProfileIcon { profile: remove_name.clone(), png: None }),
            }
        }
    }
}

/// «Eliminar perfil» confirmation (`Dialog::DeleteProfile`).
#[component]
pub fn DeleteProfileDialog(name: ProfileName) -> Element {
    let dispatch = use_dispatch();
    let open = use_signal(|| true);
    let body = tr_args("profile-delete-body", &[("name", arg(name.clone()))]);
    rsx! {
        Dialog {
            open,
            icon: "delete",
            icon_color: "error",
            title: tr("profile-delete-title"),
            on_close: move |_| dispatch(Action::CloseDialog),
            actions: rsx! {
                Button { label: tr("profile-cancel"), variant: ButtonVariant::Text, onclick: move |_| dispatch(Action::CloseDialog) }
                Button {
                    label: tr("profile-delete-confirm"),
                    variant: ButtonVariant::Danger,
                    icon: "delete",
                    onclick: move |_| dispatch(Action::DeleteProfile(name.clone())),
                }
            },
            p { class: "t-bm", "{body}" }
        }
    }
}

/// What the save bar shows for the current page: the edits count and the
/// scope Restablecer acts on. `app.rs` draws the bar (it floats over every
/// page) and asks this; `None` hides it.
pub fn save_bar_scope(store: &crate::state::Store) -> Option<(crate::state::Scope, usize)> {
    let scope = match &store.route {
        Route::Profile { id, .. } => Scope::Profile(id.clone()),
        Route::Modes { id: Some(id) } => Scope::Mode(id.clone()),
        _ => return None,
    };
    let count = store.page_edits(&store.route);
    (count > 0).then_some((scope, count))
}
