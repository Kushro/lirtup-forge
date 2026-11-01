//! «Nuevo perfil» (front C): icon picker (capture from a window, a file,
//! none), name with inline validation, mode and «Copiar ajustes de», the
//! rule editor, the match preview («Coincide con N ventanas», computed here
//! from the window list) and the side panel *Capturar ventana* with
//! «Señalar ventana» (`Command::PickWindow`).
//!
//! The icon is the only file the dialog writes itself (`icons/<name>.png`,
//! as 1.x): `update` stays pure and only records the path in the profile.

use dioxus::prelude::*;
use lirtup_config::{Interval, Match};
use lirtup_x11::WindowInfo;

use crate::app::{use_dispatch, use_store};
use crate::components::{Button, ButtonVariant, Dialog, Icon, IconButton, Select, TextField};
use crate::i18n::{arg, tr, tr_args};
use crate::route::ProfileRef;
use crate::screens::profile::initials;
use crate::state::{Action, NewProfile};

/// The kinds of rule of [`Match`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    /// `title`.
    Title,
    /// `title_contains`.
    TitleContains,
    /// `title_regex`.
    TitleRegex,
    /// `title_startswith`.
    TitleStartswith,
    /// `title_endswith`.
    TitleEndswith,
    /// `width`.
    Width,
    /// `height`.
    Height,
}

impl RuleKind {
    /// In the order of the dropdown.
    pub const ALL: [RuleKind; 7] = [
        RuleKind::TitleContains,
        RuleKind::Title,
        RuleKind::TitleRegex,
        RuleKind::TitleStartswith,
        RuleKind::TitleEndswith,
        RuleKind::Width,
        RuleKind::Height,
    ];

    fn id(self) -> &'static str {
        match self {
            RuleKind::Title => "title",
            RuleKind::TitleContains => "title_contains",
            RuleKind::TitleRegex => "title_regex",
            RuleKind::TitleStartswith => "title_startswith",
            RuleKind::TitleEndswith => "title_endswith",
            RuleKind::Width => "width",
            RuleKind::Height => "height",
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            RuleKind::Title => "new-profile-kind-title",
            RuleKind::TitleContains => "new-profile-kind-title-contains",
            RuleKind::TitleRegex => "new-profile-kind-title-regex",
            RuleKind::TitleStartswith => "new-profile-kind-title-startswith",
            RuleKind::TitleEndswith => "new-profile-kind-title-endswith",
            RuleKind::Width => "new-profile-kind-width",
            RuleKind::Height => "new-profile-kind-height",
        }
    }

    fn from_id(id: &str) -> Option<RuleKind> {
        RuleKind::ALL.into_iter().find(|k| k.id() == id)
    }

    fn is_size(self) -> bool {
        matches!(self, RuleKind::Width | RuleKind::Height)
    }
}

/// One row of the rule editor.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// Which rule.
    pub kind: RuleKind,
    /// As typed.
    pub value: String,
}

/// The rules of a [`Match`], one row each. Unknown rules are not shown (and
/// [`match_of`] cannot keep them: the editor of a profile with one shows a
/// chip for it and leaves it out).
pub fn rules_of(matcher: &Match) -> Vec<Rule> {
    let mut rules = Vec::new();
    let mut text = |kind, value: &Option<String>| {
        if let Some(v) = value {
            rules.push(Rule {
                kind,
                value: v.clone(),
            });
        }
    };
    text(RuleKind::Title, &matcher.title);
    text(RuleKind::TitleContains, &matcher.title_contains);
    text(RuleKind::TitleRegex, &matcher.title_regex);
    text(RuleKind::TitleStartswith, &matcher.title_startswith);
    text(RuleKind::TitleEndswith, &matcher.title_endswith);
    for (kind, interval) in [
        (RuleKind::Width, &matcher.width),
        (RuleKind::Height, &matcher.height),
    ] {
        if let Some(i) = interval {
            let value = match i {
                Interval::Number(n) if n.fract() == 0.0 => format!("{}", *n as i64),
                Interval::Number(n) => n.to_string(),
                Interval::Text(t) => t.clone(),
            };
            rules.push(Rule { kind, value });
        }
    }
    rules
}

/// The [`Match`] of the editor's rows (empty values skipped; a later row of
/// the same kind wins). `None` = no rules.
pub fn match_of(rules: &[Rule]) -> Option<Match> {
    let mut m = Match::default();
    for rule in rules {
        let value = rule.value.trim();
        if value.is_empty() {
            continue;
        }
        let text = Some(value.to_owned());
        // A bare number stays a number, as 1.x writes it.
        let size = || match value.parse::<f64>() {
            Ok(n) => Interval::Number(n),
            Err(_) => Interval::Text(value.to_owned()),
        };
        match rule.kind {
            RuleKind::Title => m.title = text,
            RuleKind::TitleContains => m.title_contains = text,
            RuleKind::TitleRegex => m.title_regex = text,
            RuleKind::TitleStartswith => m.title_startswith = text,
            RuleKind::TitleEndswith => m.title_endswith = text,
            RuleKind::Width => m.width = Some(size()),
            RuleKind::Height => m.height = Some(size()),
        }
    }
    (!m.is_empty()).then_some(m)
}

/// The rules a window suggests: its title (contains) and its size.
pub fn rules_for(window: &WindowInfo) -> Vec<Rule> {
    vec![
        Rule {
            kind: RuleKind::TitleContains,
            value: window.title.clone(),
        },
        Rule {
            kind: RuleKind::Width,
            value: window.width().to_string(),
        },
        Rule {
            kind: RuleKind::Height,
            value: window.height().to_string(),
        },
    ]
}

/// Standard base64 (for `data:` URLs of icons).
pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// An icon as a 64 px PNG, from any image the `image` crate reads.
fn png_icon(image: image::DynamicImage) -> Option<Vec<u8>> {
    let image = image.resize(64, 64, image::imageops::FilterType::Lanczos3);
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// `_NET_WM_ICON` of a window, as PNG (on a blocking thread: it opens its
/// own X connection).
async fn capture_icon(xid: u32) -> Option<Vec<u8>> {
    tokio::task::spawn_blocking(move || {
        let x11 = lirtup_x11::X11::connect().ok()?;
        let icon = x11.window_icon(xid, 64).ok()??;
        let icon = image::RgbaImage::from_raw(icon.width, icon.height, icon.data)?;
        png_icon(image::DynamicImage::ImageRgba8(icon))
    })
    .await
    .ok()
    .flatten()
}

/// The rule editor: one row per rule (kind, value, remove) and «Agregar
/// regla». Bad regexes and intervals show under their row.
#[component]
pub fn RuleEditor(rules: Signal<Vec<Rule>>) -> Element {
    let mut rules = rules;
    let kinds: Vec<(String, String)> = RuleKind::ALL
        .into_iter()
        .map(|k| (k.id().to_owned(), tr(k.label_key())))
        .collect();
    let count = rules.read().len();
    rsx! {
        div { class: "flex flex-col gap-2",
            h2 { class: "t-ts", {tr("new-profile-rules")} }
            div { class: "flex flex-col gap-0.5",
                for i in 0..count {
                    RuleRow { key: "{i}", rules, index: i, kinds: kinds.clone() }
                }
            }
            div { class: "flex flex-wrap items-center gap-2",
                Button {
                    label: tr("new-profile-add-rule"),
                    variant: ButtonVariant::Text,
                    icon: "add",
                    onclick: move |_| rules.write().push(Rule { kind: RuleKind::TitleContains, value: String::new() }),
                }
                span { class: "t-bs text-on-surface-variant", {tr("new-profile-rules-help")} }
            }
        }
    }
}

#[component]
fn RuleRow(rules: Signal<Vec<Rule>>, index: usize, kinds: Vec<(String, String)>) -> Element {
    let mut rules = rules;
    let rule = rules.read().get(index).cloned();
    let Some(rule) = rule else {
        return rsx! {};
    };
    let kind = super::profile::rows::use_synced(rule.kind.id().to_owned());
    let value = super::profile::rows::use_synced(rule.value.clone());
    // Check the rule alone with the scaler's own checks.
    let error = match_of(std::slice::from_ref(&rule)).and_then(|m| {
        let warnings = m.check("");
        (!warnings.is_empty()).then(|| warnings[0].message.clone())
    });
    let placeholder = rule
        .kind
        .is_size()
        .then(|| tr("new-profile-rule-placeholder-size"));
    rsx! {
        div { class: "flex flex-wrap items-start gap-3 p-3 rounded-xs bg-surface-container-high",
            div { class: "min-w-[170px]",
                Select {
                    options: kinds,
                    value: kind,
                    label: tr("new-profile-rule-kind"),
                    on_change: move |id: String| {
                        if let Some(k) = RuleKind::from_id(&id)
                            && let Some(r) = rules.write().get_mut(index)
                        {
                            r.kind = k;
                        }
                    },
                }
            }
            div { class: "flex-1 min-w-[200px]",
                TextField {
                    value,
                    label: tr("new-profile-rule-value"),
                    placeholder,
                    error,
                    on_change: move |v: String| {
                        if let Some(r) = rules.write().get_mut(index) {
                            r.value = v;
                        }
                    },
                }
            }
            IconButton {
                icon: "close",
                label: tr("new-profile-remove-rule"),
                size: 32,
                onclick: move |_| {
                    let mut list = rules.write();
                    if index < list.len() {
                        list.remove(index);
                    }
                },
            }
        }
    }
}

/// «Coincide con N ventanas abiertas», green with matches, amber without.
#[component]
pub fn MatchPreview(matcher: Option<Match>) -> Element {
    let store = use_store();
    let Some(matcher) = matcher else {
        return rsx! {
            div { class: "flex items-center gap-3 p-4 rounded-lg bg-surface-container-high t-bm",
                Icon { name: "info" }
                span { {tr("new-profile-no-rules")} }
            }
        };
    };
    let s = store.read();
    let found: Vec<String> = s
        .windows_matching(&matcher)
        .into_iter()
        .map(|w| w.title.clone())
        .collect();
    drop(s);
    if found.is_empty() {
        return rsx! {
            div { class: "flex items-center gap-3 p-4 rounded-lg bg-tertiary-container text-on-tertiary-container t-bm", role: "status",
                Icon { name: "warning" }
                span { {tr("new-profile-matches-none")} }
            }
        };
    }
    let text = tr_args(
        "new-profile-matches",
        &[
            ("count", arg(found.len() as i64)),
            ("first", arg(found[0].clone())),
        ],
    );
    rsx! {
        div { class: "flex items-center gap-3 p-4 rounded-lg bg-success-container text-on-success-container t-bm", role: "status",
            Icon { name: "check_circle" }
            span { "{text}" }
        }
    }
}

/// The dialog (`Dialog::NewProfile`).
#[component]
pub fn NewProfileDialog(from: Option<WindowInfo>) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let open = use_signal(|| true);
    let mut name = use_signal(|| from.as_ref().map(|w| w.title.clone()).unwrap_or_default());
    let mut mode = use_signal(String::new);
    let mut copy_from = use_signal(String::new);
    let mut rules = use_signal(|| from.as_ref().map(rules_for).unwrap_or_default());
    let mut icon = use_signal(|| None::<Vec<u8>>);
    let mut selected = use_signal(|| from.as_ref().map(|w| w.xid));
    let mut pointing = use_signal(|| false);
    let mut touched = use_signal(|| false);

    // Take a window: name, rules and icon from it.
    let mut take = move |window: WindowInfo| {
        name.set(window.title.clone());
        rules.set(rules_for(&window));
        selected.set(Some(window.xid));
        let xid = window.xid;
        spawn(async move {
            if let Some(png) = capture_icon(xid).await {
                icon.set(Some(png));
            }
        });
    };
    // The icon of the window the dialog was opened from.
    let from_xid = from.as_ref().map(|w| w.xid);
    use_hook(move || {
        if let Some(xid) = from_xid {
            spawn(async move {
                if let Some(png) = capture_icon(xid).await {
                    icon.set(Some(png));
                }
            });
        }
    });
    // «Señalar ventana»: the core answers with WindowPicked.
    use_effect(move || {
        let picked = store.read().picked.clone();
        if !*pointing.peek() {
            return;
        }
        if let Some(window) = picked {
            pointing.set(false);
            if let Some(w) = window {
                let info = store
                    .peek()
                    .windows
                    .iter()
                    .find(|i| i.xid == w.xid)
                    .cloned()
                    .unwrap_or(WindowInfo {
                        xid: w.xid,
                        title: w.title.clone(),
                        rect: lirtup_x11::Rect::default(),
                        pid: w.pid,
                        class: None,
                    });
                take(info);
            }
        }
    });

    let s = store.read();
    let trimmed = name().trim().to_owned();
    let taken = {
        let lower = trimmed.to_lowercase();
        s.config.file.profiles.keys().any(|n| n.to_lowercase() == lower)
    };
    let error = if trimmed.is_empty() {
        touched().then(|| tr("new-profile-name-empty"))
    } else if taken {
        Some(tr("new-profile-name-taken"))
    } else {
        None
    };
    let valid = !trimmed.is_empty() && !taken;
    let mut modes = vec![(String::new(), tr("profile-mode-inherit"))];
    modes.extend(
        s.modes
            .store
            .modes
            .iter()
            .map(|(id, m)| (id.clone(), m.name.clone())),
    );
    let mut sources = vec![(String::new(), tr("nav-default-profile"))];
    sources.extend(s.config.file.profiles.keys().map(|n| (n.clone(), n.clone())));
    let windows = s.windows.clone();
    let config_path = s.config.file.path.clone();
    drop(s);
    let matcher = match_of(&rules.read());
    let (letters, color) = initials(if trimmed.is_empty() { "?" } else { &trimmed });
    let icon_src = icon
        .read()
        .as_ref()
        .map(|png| format!("data:image/png;base64,{}", base64(png)));

    let create = move |_| {
        touched.set(true);
        let name = name.peek().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let mut png = icon.peek().clone();
        if let Some(bytes) = &png {
            let path = crate::state::store::icon_path(&config_path, &name);
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, bytes));
            if let Err(e) = written {
                tracing::warn!("profile icon {}: {e}", path.display());
                png = None;
            }
        }
        let mode = mode.peek().clone();
        let copy = copy_from.peek().clone();
        dispatch(Action::CreateProfile(NewProfile {
            name,
            mode: (!mode.is_empty()).then_some(mode),
            copy_from: (!copy.is_empty()).then_some(ProfileRef::Named(copy)),
            matcher: match_of(&rules.peek()),
            icon: png,
        }));
    };

    rsx! {
        Dialog {
            open,
            icon: "person_add",
            title: tr("new-profile-title"),
            wide: true,
            on_close: move |_| dispatch(Action::CloseDialog),
            actions: rsx! {
                Button { label: tr("profile-cancel"), variant: ButtonVariant::Text, onclick: move |_| dispatch(Action::CloseDialog) }
                Button { label: tr("new-profile-create"), icon: "check", disabled: !valid, onclick: create }
            },
            div { class: "flex flex-wrap gap-6",
                div { class: "flex-[999_1_520px] min-w-0 flex flex-col gap-5",
                    p { class: "t-bm text-on-surface-variant", {tr("new-profile-intro")} }
                    div { class: "flex flex-wrap items-end gap-4",
                        div { class: "flex flex-col items-center gap-1.5",
                            if let Some(src) = icon_src {
                                img { class: "size-18 rounded-[22px] object-cover", src: "{src}", alt: "" }
                            } else {
                                span {
                                    class: "size-18 rounded-[22px] grid place-items-center t-tl text-white",
                                    style: "background: {color}",
                                    "aria-hidden": "true",
                                    "{letters}"
                                }
                            }
                            div { class: "flex gap-0.5",
                                IconButton {
                                    icon: "screenshot_region",
                                    label: tr("new-profile-icon-capture"),
                                    size: 32,
                                    disabled: selected().is_none(),
                                    onclick: move |_| {
                                        if let Some(xid) = selected() {
                                            spawn(async move {
                                                if let Some(png) = capture_icon(xid).await {
                                                    icon.set(Some(png));
                                                }
                                            });
                                        }
                                    },
                                }
                                label {
                                    class: "relative inline-grid",
                                    title: tr("new-profile-icon-file"),
                                    input {
                                        r#type: "file",
                                        accept: "image/*",
                                        class: "absolute inset-0 opacity-0 cursor-pointer",
                                        "aria-label": tr("new-profile-icon-file"),
                                        onchange: move |e: FormEvent| {
                                            if let Some(file) = e.files().into_iter().next() {
                                                spawn(async move {
                                                    let Ok(bytes) = file.read_bytes().await else { return };
                                                    let png = image::load_from_memory(&bytes).ok().and_then(png_icon);
                                                    if png.is_some() {
                                                        icon.set(png);
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
                                    disabled: icon.read().is_none(),
                                    onclick: move |_| icon.set(None),
                                }
                            }
                        }
                        div { class: "flex-[1_1_260px]",
                            TextField {
                                value: name,
                                label: tr("profile-name"),
                                error,
                                autofocus: true,
                                on_change: move |v: String| {
                                    touched.set(true);
                                    name.set(v);
                                },
                            }
                        }
                    }
                    div { class: "grid grid-cols-2 gap-3",
                        Select { options: modes, value: mode, label: tr("new-profile-mode"), on_change: move |v: String| mode.set(v) }
                        Select { options: sources, value: copy_from, label: tr("new-profile-copy-from"), on_change: move |v: String| copy_from.set(v) }
                    }
                    RuleEditor { rules }
                    MatchPreview { matcher }
                }
                aside { class: "flex-[1_1_300px] min-w-0 flex flex-col gap-3 p-5 rounded-lg bg-surface-container",
                    h2 { class: "t-tm", {tr("new-profile-capture")} }
                    p { class: "t-bs text-on-surface-variant", {tr("new-profile-capture-help")} }
                    div {
                        Button {
                            label: tr(if pointing() { "new-profile-pointing" } else { "new-profile-point" }),
                            variant: ButtonVariant::Tonal,
                            icon: "ads_click",
                            disabled: pointing(),
                            onclick: move |_| {
                                pointing.set(true);
                                dispatch(Action::PickWindow);
                            },
                        }
                    }
                    div { class: "flex flex-col gap-0.5", role: "listbox", "aria-label": tr("new-profile-capture"),
                        if windows.is_empty() {
                            span { class: "t-bs text-on-surface-variant", {tr("new-profile-no-windows")} }
                        }
                        for window in windows {
                            WindowPick {
                                key: "{window.xid}",
                                selected: selected() == Some(window.xid),
                                window: window.clone(),
                                on_pick: move |w: WindowInfo| take(w),
                            }
                        }
                    }
                    p { class: "t-bs text-on-surface-variant mt-auto", {tr("new-profile-from-tile")} }
                }
            }
        }
    }
}

/// One window of *Capturar ventana*.
#[component]
fn WindowPick(window: WindowInfo, selected: bool, on_pick: EventHandler<WindowInfo>) -> Element {
    let (letters, color) = initials(&window.title);
    let mut failed = use_signal(|| false);
    let size = format!("{}×{}", window.width(), window.height());
    let class = if selected {
        "flex items-center gap-3 min-h-15 px-4 rounded-xs text-left bg-secondary-container text-on-secondary-container"
    } else {
        "flex items-center gap-3 min-h-15 px-4 rounded-xs text-left bg-surface-container-high hover:bg-surface-container-highest"
    };
    let title = window.title.clone();
    rsx! {
        button {
            class,
            r#type: "button",
            role: "option",
            "aria-selected": "{selected}",
            onclick: move |_| on_pick.call(window.clone()),
            // The window's icon over its initials (the drawer's look), which
            // stay when the window has none.
            span {
                class: "relative size-8 shrink-0 rounded-md grid place-items-center t-ls text-white overflow-hidden",
                style: "background: {color}",
                "aria-hidden": "true",
                "{letters}"
                if !failed() {
                    img {
                        class: "absolute inset-0 size-full object-cover",
                        src: "/icon/{window.xid}",
                        alt: "",
                        onerror: move |_| failed.set(true),
                    }
                }
            }
            div { class: "flex-1 min-w-0 flex flex-col",
                span { class: "t-ts truncate", "{title}" }
                span { class: "t-bs mono text-on-surface-variant", "{size}" }
            }
            if selected {
                Icon { name: "check_circle", fill: true }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_round_trip() {
        let m = Match {
            title_contains: Some("BALDR SKY".into()),
            width: Some(Interval::Number(800.0)),
            height: Some(Interval::Text(">600".into())),
            ..Match::default()
        };
        let rules = rules_of(&m);
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[1].value, "800");
        assert_eq!(match_of(&rules), Some(m));
        assert_eq!(
            match_of(&[Rule {
                kind: RuleKind::Title,
                value: "  ".into()
            }]),
            None
        );
    }

    #[test]
    fn rules_of_a_window() {
        let w = WindowInfo {
            xid: 1,
            title: "Gin'iro, Haruka".into(),
            rect: lirtup_x11::Rect {
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
            },
            pid: None,
            class: None,
        };
        let m = match_of(&rules_for(&w)).unwrap();
        assert!(m.matches(&w.title, 1280, 720));
        assert!(!m.matches(&w.title, 1920, 1080));
    }

    #[test]
    fn base64_matches_the_rfc() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn icons_become_small_pngs() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::new(256, 128));
        let png = png_icon(image).unwrap();
        let back = image::load_from_memory(&png).unwrap();
        assert_eq!((back.width(), back.height()), (64, 32));
    }
}
