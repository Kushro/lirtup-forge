//! Things over the page: the floating toolbar, the snackbar, dialogs, menus
//! and tooltips.
//!
//! Focus handling (trap, first item, return to the opener) and the menu's
//! placement run as small scripts in the webview, on elements found by an
//! id of their own ([`use_dom_id`]).

use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;

use super::{Icon, IconButton, MenuItem};

/// A unique `id` for an element of this component instance.
pub(crate) fn use_dom_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    use_hook(|| format!("lf-{}", NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// Run a script in the webview, ignoring its result.
fn run_js(script: String) {
    spawn(async move {
        let _ = document::eval(&script).await;
    });
}

/// Remember the focused element under `key` (to give the focus back when
/// the overlay closes).
fn save_focus_js(key: &str) -> String {
    format!("(window.__lfFocus ??= {{}})['{key}'] = document.activeElement;")
}

/// Give the focus back to what had it when `key` opened.
fn restore_focus(key: &str) {
    let script = format!(
        "const e = (window.__lfFocus ?? {{}})['{key}']; if (e && e.isConnected) e.focus(); if (window.__lfFocus) delete window.__lfFocus['{key}'];"
    );
    // `use_drop` runs outside a component scope once the overlay is gone:
    // use the root's runtime to spawn.
    dioxus::core::spawn_forever(async move {
        let _ = document::eval(&script).await;
    });
}

/// What can take the focus inside an element (`d` in the script).
const FOCUSABLE: &str = "[...d.querySelectorAll('button:not([disabled]),[href],input:not([disabled]):not([type=hidden]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex=\"-1\"])')].filter(e => !e.dataset.sentinel && e.getClientRects().length)";

/// A floating toolbar (`.ftb`, `h-16 rounded-full`), at the bottom center
/// of the page (the save bar) — it animates in and out with `open`.
#[component]
pub fn FloatingToolbar(open: bool, children: Element) -> Element {
    let shut = if open { "" } else { "shut" };
    rsx! {
        div { class: "lf-ftb-wrap {shut}", "aria-hidden": (!open).then_some("true"), inert: (!open).then_some(true),
            div { class: "lf-ftb", role: "toolbar", {children} }
        }
    }
}

/// The snackbar (`.snack`): one at a time, bottom left, above the save bar.
#[component]
pub fn Snackbar(
    /// Text (translated).
    message: String,
    /// The action («Deshacer»).
    action: Option<String>,
    on_action: Option<EventHandler<()>>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "lf-snack", role: "status", "aria-live": "polite",
            span { class: "msg", "{message}" }
            if let Some(action) = action {
                super::Button {
                    label: action,
                    variant: super::ButtonVariant::Text,
                    onclick: move |_| {
                        if let Some(on_action) = on_action {
                            on_action(());
                        }
                        on_close(());
                    },
                }
            }
            IconButton { icon: "close", label: t!("cmp-snackbar-close"), size: 32, onclick: move |_| on_close(()) }
        }
    }
}

/// A modal dialog (`.dlg`, `rounded-xl`): scrim, focus trapped inside, Esc
/// and the scrim close it (`on_close`).
///
/// Closing sets `open` to `false` and calls `on_close`. On opening, the
/// focus goes to the dialog (or stays on a field with `autofocus`); on
/// closing it goes back to what had it.
#[component]
pub fn Dialog(
    open: Signal<bool>,
    /// Material Symbols name over the title (`edit_note`).
    icon: Option<&'static str>,
    /// Color of the icon: `tertiary` for unsaved changes, `error` for
    /// deleting (a CSS variable name without `--`).
    icon_color: Option<&'static str>,
    title: String,
    /// Wider for «Nuevo perfil» (with its side panel).
    #[props(default)]
    wide: bool,
    children: Element,
    /// The buttons, right aligned.
    actions: Element,
    on_close: Option<EventHandler<()>>,
) -> Element {
    if !open() {
        return rsx! {};
    }
    rsx! {
        DialogOpen { open, icon, icon_color, title, wide, actions, on_close, {children} }
    }
}

/// The dialog while open (its own scope, so opening and closing run the
/// focus hooks).
#[component]
fn DialogOpen(
    open: Signal<bool>,
    icon: Option<&'static str>,
    icon_color: Option<&'static str>,
    title: String,
    wide: bool,
    children: Element,
    actions: Element,
    on_close: Option<EventHandler<()>>,
) -> Element {
    let id = use_dom_id();
    let mut down_on_scrim = use_signal(|| false);
    use_hook({
        let id = id.clone();
        move || {
            run_js(format!(
                "{} requestAnimationFrame(() => {{ const d = document.getElementById('{id}'); if (d && !d.contains(document.activeElement)) d.focus(); }});",
                save_focus_js(&id)
            ));
        }
    });
    use_drop({
        let id = id.clone();
        move || restore_focus(&id)
    });
    let mut close = move || {
        open.set(false);
        if let Some(on_close) = on_close {
            on_close(());
        }
    };
    // Focus trap: two sentinels send Tab and Shift+Tab around.
    let wrap = |to_last: bool, id: String| {
        move |_| {
            let pick = if to_last { "f[f.length - 1]" } else { "f[0]" };
            run_js(format!(
                "const d = document.getElementById('{id}'); if (d) {{ const f = {FOCUSABLE}; (f.length ? {pick} : d).focus(); }}"
            ));
        }
    };
    let wide = if wide { "wide" } else { "" };
    let icon_style = icon_color
        .map(|c| format!("color: var(--{c})"))
        .unwrap_or_default();
    let title_id = format!("{id}-t");
    rsx! {
        div {
            class: "lf-scrim",
            onmousedown: move |_| down_on_scrim.set(true),
            onclick: move |_| {
                if down_on_scrim() {
                    close();
                }
                down_on_scrim.set(false);
            },
            span { tabindex: "0", "data-sentinel": "1", onfocus: wrap(true, id.clone()) }
            div {
                id: "{id}",
                class: "lf-dlg {wide}",
                role: "dialog",
                "aria-modal": "true",
                "aria-labelledby": "{title_id}",
                tabindex: "-1",
                onmousedown: move |e| e.stop_propagation(),
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |e| {
                    if e.key() == Key::Escape {
                        e.stop_propagation();
                        close();
                    }
                },
                if let Some(icon) = icon {
                    span { style: "{icon_style}", Icon { name: icon, size: 32 } }
                }
                h2 { id: "{title_id}", class: "t-hs m-0", "{title}" }
                div { class: "body t-bm", {children} }
                div { class: "actions", {actions} }
            }
            span { tabindex: "0", "data-sentinel": "1", onfocus: wrap(false, id.clone()) }
        }
    }
}

/// One line of a [`MenuPopup`]: a [`MenuItem`] or an option of a `Select`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MenuEntry {
    pub label: String,
    pub icon: Option<&'static str>,
    pub danger: bool,
    pub disabled: bool,
    pub divider_before: bool,
    /// The current option of a `Select` (shown selected, focused first).
    pub selected: bool,
}

impl From<&MenuItem> for MenuEntry {
    fn from(item: &MenuItem) -> MenuEntry {
        MenuEntry {
            label: item.label.clone(),
            icon: item.icon,
            danger: item.danger,
            disabled: item.disabled,
            divider_before: item.divider_before,
            selected: false,
        }
    }
}

/// A menu (`.menu`, `rounded-sm`) at a point or under its anchor; arrows,
/// Enter and Esc.
///
/// With `at: None` the menu is placed under (or over, if it does not fit
/// below) its nearest positioned ancestor: put the anchor and the menu in a
/// `span { class: "lf-menu-anchor" }` (as [`super::SplitButton`] does).
/// Choosing an entry closes it.
#[component]
pub fn Menu(
    open: Signal<bool>,
    items: Vec<MenuItem>,
    /// Page coordinates (a right click); `None` = under the previous
    /// sibling (a split button).
    at: Option<(f64, f64)>,
    on_select: EventHandler<&'static str>,
) -> Element {
    let entries: Vec<MenuEntry> = items.iter().map(MenuEntry::from).collect();
    let ids: Vec<&'static str> = items.iter().map(|i| i.id).collect();
    rsx! {
        MenuPopup { open, entries, at, role: "menuitem", on_pick: move |i: usize| on_select(ids[i]) }
    }
}

/// The popup behind [`Menu`] and `Select`: entries by index.
#[component]
pub(crate) fn MenuPopup(
    open: Signal<bool>,
    entries: Vec<MenuEntry>,
    at: Option<(f64, f64)>,
    /// `menuitem`, or `option` for a select.
    role: &'static str,
    on_pick: EventHandler<usize>,
) -> Element {
    if !open() {
        return rsx! {};
    }
    rsx! {
        MenuOpen { open, entries, at, role, on_pick }
    }
}

#[component]
fn MenuOpen(
    open: Signal<bool>,
    entries: Vec<MenuEntry>,
    at: Option<(f64, f64)>,
    role: &'static str,
    on_pick: EventHandler<usize>,
) -> Element {
    let id = use_dom_id();
    // Place it inside the window and focus the selected or first entry.
    use_hook({
        let id = id.clone();
        move || {
            run_js(format!(
                "{save} requestAnimationFrame(() => {{
                    const m = document.getElementById('{id}'); if (!m) return;
                    const W = innerWidth, H = innerHeight; let r = m.getBoundingClientRect();
                    if (m.classList.contains('below') && r.bottom > H - 8 && r.top - r.height > 8) m.classList.replace('below', 'above');
                    r = m.getBoundingClientRect();
                    if (m.classList.contains('at')) {{
                        let x = r.left, y = r.top;
                        if (x + r.width > W - 8) x = Math.max(8, W - 8 - r.width);
                        if (y + r.height > H - 8) y = Math.max(8, y - r.height);
                        m.style.left = x + 'px'; m.style.top = y + 'px';
                    }} else if (r.left < 8) {{ m.style.right = 'auto'; m.style.left = '0'; }}
                    const d = m; const f = {FOCUSABLE};
                    (m.querySelector('[aria-selected=true]:not(:disabled)') ?? f[0] ?? m).focus();
                }});",
                save = save_focus_js(&id)
            ));
        }
    });
    use_drop({
        let id = id.clone();
        move || restore_focus(&id)
    });
    let mut close = move || open.set(false);
    let key_id = id.clone();
    let (class, style) = match at {
        Some((x, y)) => ("at", format!("left: {x}px; top: {y}px")),
        None => ("below", String::new()),
    };
    let selectable = role == "option";
    rsx! {
        div {
            class: "lf-menu-scrim",
            onclick: move |e| {
                e.stop_propagation();
                close();
            },
            oncontextmenu: move |e| {
                e.prevent_default();
                close();
            },
        }
        div {
            id: "{id}",
            class: "lf-menu {class}",
            style,
            role: if selectable { "listbox" } else { "menu" },
            tabindex: "-1",
            onclick: move |e| e.stop_propagation(),
            onkeydown: move |e| {
                let step = match e.key() {
                    Key::ArrowDown => "1",
                    Key::ArrowUp => "-1",
                    Key::Home => "'home'",
                    Key::End => "'end'",
                    Key::Escape | Key::Tab => {
                        e.stop_propagation();
                        if e.key() == Key::Escape {
                            e.prevent_default();
                        }
                        close();
                        return;
                    }
                    _ => return,
                };
                e.prevent_default();
                e.stop_propagation();
                run_js(format!(
                    "const d = document.getElementById('{key_id}'); if (d) {{ const f = {FOCUSABLE}; if (f.length) {{
                        const s = {step}; let i = f.indexOf(document.activeElement);
                        i = s === 'home' ? 0 : s === 'end' ? f.length - 1 : (i + s + f.length) % f.length;
                        f[i].focus(); }} }}"
                ));
            },
            for (i, entry) in entries.into_iter().enumerate() {
                Fragment { key: "{i}",
                if entry.divider_before {
                    div { class: "lf-hr", role: "separator" }
                }
                button {
                    r#type: "button",
                    class: "lf-mi lf-state",
                    class: if entry.selected { "on" },
                    class: if entry.danger { "danger" },
                    role,
                    "aria-selected": selectable.then_some(if entry.selected { "true" } else { "false" }),
                    disabled: entry.disabled,
                    onclick: move |_| {
                        close();
                        on_pick(i);
                    },
                    if let Some(icon) = entry.icon {
                        Icon { name: icon, size: 20 }
                    } else if selectable {
                        span { class: "lf-ms", style: "font-size: 20px", "aria-hidden": "true",
                            if entry.selected { "check" }
                        }
                    }
                    span { class: "lbl", "{entry.label}" }
                }
                }
            }
        }
    }
}

/// A tooltip (`.tip`) on hover and focus of its child, after 500 ms.
#[component]
pub fn Tooltip(
    text: String,
    children: Element,
    /// Where it opens: `""` under and centered (default), `"left"` / `"right"`
    /// under and aligned to that edge, `"side"` to the right (the rail).
    #[props(default)]
    place: &'static str,
) -> Element {
    rsx! {
        span { class: "lf-tipw {place}",
            {children}
            span { class: "lf-tip", role: "tooltip", "{text}" }
        }
    }
}
