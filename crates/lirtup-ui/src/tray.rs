//! The tray (round 4, front Y; «Bandeja» of the design and 6.1).
//!
//! Menu: «Escalando: …» with fps and mode, Detener, Reiniciar, Perfil ▸,
//! Generación de cuadros ▸ (Apagada, ×2, ×3, ×4, Auto; with the reason when
//! the engine is missing), Modo daemon, Mostrar, Salir. The icon has four
//! states ([`TrayStatus`]). It lives in the GUI process (`tray-icon` and
//! `muda` through `dioxus::desktop::trayicon`, on the same event loop) and
//! acts only through actions, like any screen.
//!
//! Closing the window hides it to the tray when `gui.close_to_tray` and
//! `gui.show_tray_icon`; minimizing does when `gui.minimize_to_tray`;
//! `gui.start_hidden` starts in the tray (`main.rs`). Front Y changes the
//! title bar of `app.rs` for that (only its close and minimize handlers,
//! and the [`QuitDialog`] they may open).
//!
//! Also here, because they are the window's lifecycle (parity rows 32 to
//! 35 and 40 of the round 4 skeleton):
//!
//! - one instance: a second `lirtup-gui` asks the first to show itself and
//!   exits ([`forward_to_running`], [`claim_instance`]);
//! - a session hides the window to the tray and its end shows it again
//!   (1.x hid itself while scaling);
//! - the window is created hidden and shown here (see `main.rs`: it keeps
//!   the app running while hidden);
//! - «Salir» asks about unsaved edits ([`QuitDialog`]), then stops the
//!   session and the daemon unless «Seguir ejecutándose tras salir» keeps
//!   the daemon, saves the window geometry (`window_geometry` in
//!   `gui-config.yaml`) and closes;
//! - the icon's error state lasts until the window is shown or the menu
//!   used.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;

use dioxus::desktop::tao::event::{Event as TaoEvent, WindowEvent};
use dioxus::desktop::trayicon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use dioxus::desktop::trayicon::{Icon, TrayIcon, TrayIconBuilder};
use dioxus::desktop::{
    DesktopContext, WindowCloseBehaviour, use_muda_event_handler, use_tray_menu_event_handler, use_window,
    use_wry_event_handler,
};
use dioxus::prelude::*;
use lirtup_config::{FrameGenMultiplier, GuiConfig, Value};
use lirtup_proto::{Command, Event, FrameGenState, SessionKind, Target};

use crate::components::{Button, ButtonVariant, Dialog};
use crate::i18n::{LANG, arg, tr, tr_args};
use crate::link::CoreLink;
use crate::route::ProfileRef;
use crate::state::{Action, LinkState, SessionState, Store};

/// The tray icon's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayStatus {
    /// Nothing running.
    #[default]
    Idle,
    /// The daemon is watching.
    Watching,
    /// A session is scaling.
    Scaling,
    /// The last session failed, or the scaler is down.
    Error,
}

/// The state of the icon for a store (pure, for the tests). The scaler
/// counts as down only when it was running the daemon: the GUI starts it
/// on the first command, so `Down` is normal before that.
pub fn status(store: &Store) -> TrayStatus {
    match &store.session {
        SessionState::Running(_) | SessionState::Starting { .. } => TrayStatus::Scaling,
        SessionState::Failed { .. } => TrayStatus::Error,
        SessionState::Idle if store.daemon.on && matches!(store.link, LinkState::Down { .. }) => {
            TrayStatus::Error
        }
        SessionState::Idle if store.daemon.on => TrayStatus::Watching,
        SessionState::Idle => TrayStatus::Idle,
    }
}

/// [`status`] once the person has seen the error (opened the window or
/// used the menu): what the icon shows without it.
fn status_seen(store: &Store) -> TrayStatus {
    match status(store) {
        TrayStatus::Error if store.daemon.on => TrayStatus::Watching,
        TrayStatus::Error => TrayStatus::Idle,
        other => other,
    }
}

// ---- the icon ---------------------------------------------------------------

/// Side of the icon images, in pixels (panels scale it down).
const ICON_SIZE: u32 = 64;

/// The «diamond» symbol of the design as a convex polygon in a unit box
/// (y down, clockwise): table, right and left corners of the girdle, tip.
const GEM: [(f32, f32); 5] = [(0.28, 0.18), (0.72, 0.18), (0.94, 0.42), (0.5, 0.9), (0.06, 0.42)];

/// A small RGBA canvas, painted shape by shape with 4×4 supersampling.
struct Canvas {
    size: u32,
    rgba: Vec<u8>,
}

impl Canvas {
    fn new(size: u32) -> Canvas {
        Canvas {
            size,
            rgba: vec![0; (size * size * 4) as usize],
        }
    }

    /// Paint `color` where `inside(x, y)` holds (coordinates in 0..1).
    fn paint(&mut self, color: [u8; 3], inside: impl Fn(f32, f32) -> bool) {
        const SS: u32 = 4;
        let n = self.size as f32;
        for py in 0..self.size {
            for px in 0..self.size {
                let mut hits = 0;
                for sy in 0..SS {
                    for sx in 0..SS {
                        let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / n;
                        let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / n;
                        hits += u32::from(inside(x, y));
                    }
                }
                if hits == 0 {
                    continue;
                }
                let a = hits as f32 / (SS * SS) as f32;
                let i = ((py * self.size + px) * 4) as usize;
                let dst = &mut self.rgba[i..i + 4];
                let da = dst[3] as f32 / 255.0;
                let out_a = a + da * (1.0 - a);
                for c in 0..3 {
                    let src = color[c] as f32;
                    let d = dst[c] as f32;
                    dst[c] = ((src * a + d * da * (1.0 - a)) / out_a).round() as u8;
                }
                dst[3] = (out_a * 255.0).round() as u8;
            }
        }
    }
}

/// Distance from `(x, y)` inside the convex polygon `points` to its edge
/// (negative outside).
fn inset(points: &[(f32, f32)], x: f32, y: f32) -> f32 {
    let mut min = f32::MAX;
    for (i, &(ax, ay)) in points.iter().enumerate() {
        let (bx, by) = points[(i + 1) % points.len()];
        let (ex, ey) = (bx - ax, by - ay);
        let d = (ex * (y - ay) - ey * (x - ax)) / (ex * ex + ey * ey).sqrt();
        min = min.min(d);
    }
    min
}

/// The gem scaled by `scale` around the center.
fn gem(scale: f32) -> Vec<(f32, f32)> {
    GEM.iter()
        .map(|&(x, y)| (0.5 + (x - 0.5) * scale, 0.54 + (y - 0.54) * scale))
        .collect()
}

/// The icon of a state, as RGBA ([`ICON_SIZE`] square). Colors are the
/// dark scheme's (panels are dark on the person's Plasma; the light ones
/// still read the stroke): on-surface, primary, tertiary (amber), error.
pub fn icon_rgba(status: TrayStatus) -> Vec<u8> {
    const ON_SURFACE: [u8; 3] = [0xE3, 0xE2, 0xE9];
    const PRIMARY: [u8; 3] = [0xA8, 0xC8, 0xFF];
    const AMBER: [u8; 3] = [0xF2, 0xB6, 0x61];
    const ERROR: [u8; 3] = [0xFF, 0x8A, 0x80];
    const HALO: [u8; 3] = [0x1A, 0x1C, 0x22];
    let mut canvas = Canvas::new(ICON_SIZE);
    let outline = |canvas: &mut Canvas, points: &[(f32, f32)], width: f32, color| {
        let (left, right) = (points[4], points[2]);
        canvas.paint(color, |x, y| {
            let d = inset(points, x, y);
            // The girdle line between the side corners.
            d >= 0.0 && (d < width || ((y - left.1).abs() < width / 2.0 && x > left.0 && x < right.0))
        });
    };
    match status {
        TrayStatus::Idle => outline(&mut canvas, &gem(1.0), 0.085, ON_SURFACE),
        TrayStatus::Error => outline(&mut canvas, &gem(1.0), 0.1, ERROR),
        TrayStatus::Watching => {
            // The primary ring around a smaller gem.
            canvas.paint(PRIMARY, |x, y| {
                let (dx, dy) = ((x - 0.5).abs() - 0.29, (y - 0.5).abs() - 0.29);
                let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0);
                let d = outside - 0.17;
                (-0.075..=0.0).contains(&d)
            });
            outline(&mut canvas, &gem(0.62), 0.07, PRIMARY);
        }
        TrayStatus::Scaling => {
            let points = gem(1.0);
            canvas.paint(AMBER, |x, y| inset(&points, x, y) >= 0.0);
            // The dot, top right, with a dark halo so it reads on the gem.
            let dot = |r: f32| move |x: f32, y: f32| (x - 0.84).powi(2) + (y - 0.16).powi(2) <= r * r;
            canvas.paint(HALO, dot(0.16));
            canvas.paint(AMBER, dot(0.11));
        }
    }
    canvas.rgba
}

fn icon(status: TrayStatus) -> Option<Icon> {
    Icon::from_rgba(icon_rgba(status), ICON_SIZE, ICON_SIZE)
        .inspect_err(|e| tracing::warn!("tray icon: {e}"))
        .ok()
}

// ---- the menu -----------------------------------------------------------------

/// The multipliers of «Generación de cuadros ▸», with their item ids.
const MULTIPLIERS: [(&str, FrameGenMultiplier); 5] = [
    ("framegen:1", FrameGenMultiplier::Fixed(1)),
    ("framegen:2", FrameGenMultiplier::Fixed(2)),
    ("framegen:3", FrameGenMultiplier::Fixed(3)),
    ("framegen:4", FrameGenMultiplier::Fixed(4)),
    ("framegen:auto", FrameGenMultiplier::Auto),
];

/// What gives the menu its shape. A change rebuilds it (`set_menu`):
/// libdbusmenu does not show a submenu's label changed after it was
/// exported. Everything else (texts of plain items, checks, enabled) is
/// updated in place, so a menu left open is not closed every 500 ms.
#[derive(Debug, Clone, PartialEq)]
struct Layout {
    /// «Perfil: …».
    profiles: String,
    /// «Generación de cuadros: …».
    framegen: String,
    /// The named profiles.
    names: Vec<String>,
    /// The second line (fps and mode, or the error).
    detail: bool,
}

fn layout(store: &Store) -> Layout {
    let profiles = match store.running_profile() {
        Some(p) => tr_args("tray-profile-of", &[("name", arg(profile_label(&p)))]),
        None => tr("tray-profile"),
    };
    let framegen = tr_args(
        "tray-framegen",
        &[("value", arg(multiplier_label(multiplier(store))))],
    );
    Layout {
        profiles,
        framegen,
        names: store.config.file.profiles.keys().cloned().collect(),
        detail: summary(store).1.is_some(),
    }
}

/// The global multiplier (*Predeterminado*, with its unsaved edit).
fn multiplier(store: &Store) -> FrameGenMultiplier {
    store
        .field(
            &crate::state::Scope::Profile(ProfileRef::Default),
            "frame_gen_multiplier",
        )
        .value()
}

/// The items [`Tray::sync`] updates.
struct Items {
    header: MenuItem,
    detail: Option<MenuItem>,
    stop: MenuItem,
    restart: MenuItem,
    profiles: Submenu,
    profile_items: Vec<(CheckMenuItem, String)>,
    fg_items: Vec<CheckMenuItem>,
    fg_reason: MenuItem,
    daemon: CheckMenuItem,
    show: MenuItem,
    quit: MenuItem,
}

/// The menu of `layout`.
fn build(layout: &Layout) -> Option<(Menu, Items)> {
    let header = MenuItem::with_id("tray:header", "", false, None);
    let detail = layout
        .detail
        .then(|| MenuItem::with_id("tray:detail", "", false, None));
    let stop = MenuItem::with_id("tray:stop", "", true, None);
    let restart = MenuItem::with_id("tray:restart", "", true, None);
    let profiles = Submenu::with_id("tray:profiles", &layout.profiles, true);
    let profile_items: Vec<(CheckMenuItem, String)> = layout
        .names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let item = CheckMenuItem::with_id(format!("tray:profile:{i}"), name, true, false, None);
            (item, name.clone())
        })
        .collect();
    for (item, _) in &profile_items {
        profiles.append(item).ok()?;
    }
    let framegen = Submenu::with_id("tray:framegen", &layout.framegen, true);
    let fg_items: Vec<CheckMenuItem> = MULTIPLIERS
        .iter()
        .map(|(id, _)| CheckMenuItem::with_id(*id, "", true, false, None))
        .collect();
    let fg_reason = MenuItem::with_id("framegen:reason", "", false, None);
    for item in &fg_items {
        framegen.append(item).ok()?;
    }
    framegen.append(&PredefinedMenuItem::separator()).ok()?;
    framegen.append(&fg_reason).ok()?;
    let daemon = CheckMenuItem::with_id("tray:daemon", "", true, false, None);
    let show = MenuItem::with_id("tray:show", "", true, None);
    let quit = MenuItem::with_id("tray:quit", "", true, None);
    let menu = Menu::new();
    menu.append(&header).ok()?;
    if let Some(detail) = &detail {
        menu.append(detail).ok()?;
    }
    menu.append_items(&[
        &PredefinedMenuItem::separator(),
        &stop,
        &restart,
        &profiles,
        &framegen,
        &PredefinedMenuItem::separator(),
        &daemon,
        &show,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .ok()?;
    let items = Items {
        header,
        detail,
        stop,
        restart,
        profiles,
        profile_items,
        fg_items,
        fg_reason,
        daemon,
        show,
        quit,
    };
    Some((menu, items))
}

/// The tray icon, its menu and the shape the menu was built with.
struct Tray {
    icon: TrayIcon,
    shown: TrayStatus,
    layout: Layout,
    items: Items,
}

impl Tray {
    fn new(store: &Store) -> Option<Tray> {
        let layout = layout(store);
        let (menu, items) = build(&layout)?;
        let shown = status(store);
        let mut builder = TrayIconBuilder::new()
            .with_id("lirtup")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .with_tooltip("LirtUp Forge");
        if let Some(icon) = icon(shown) {
            builder = builder.with_icon(icon);
        }
        let icon = builder
            .build()
            .inspect_err(|e| tracing::warn!("cannot create the tray icon: {e}"))
            .ok()?;
        Some(Tray {
            icon,
            shown,
            layout,
            items,
        })
    }

    /// Put the store (and the language) into the menu and the icon.
    fn sync(&mut self, store: &Store, status: TrayStatus) {
        if status != self.shown {
            self.icon.set_icon(icon(status)).ok();
            self.shown = status;
        }
        let wanted = layout(store);
        if wanted != self.layout
            && let Some((menu, items)) = build(&wanted)
        {
            self.icon.set_menu(Some(Box::new(menu)));
            self.items = items;
            self.layout = wanted;
        }
        let items = &self.items;
        let (header, detail) = summary(store);
        self.icon
            .set_tooltip(Some(format!("LirtUp Forge · {header}")))
            .ok();
        items.header.set_text(header);
        if let (Some(item), Some(text)) = (&items.detail, detail) {
            item.set_text(text);
        }

        let running = matches!(store.session, SessionState::Running(_));
        let busy = running || matches!(store.session, SessionState::Starting { .. });
        items.stop.set_text(tr("tray-stop"));
        items.stop.set_enabled(busy);
        items.restart.set_text(tr("tray-restart"));
        items.restart.set_enabled(running);

        // Perfil ▸ (named profiles; a session restarts with the chosen one).
        let current = store.running_profile();
        items.profiles.set_enabled(running);
        for (item, name) in &items.profile_items {
            item.set_checked(current.as_ref() == Some(&ProfileRef::Named(name.clone())));
        }

        // Generación de cuadros ▸ (global: Predeterminado).
        let multiplier = multiplier(store);
        let fg = &store.framegen;
        let reason = crate::screens::profile::framegen::reason_text(fg);
        let locked = fg.env_locked;
        for (item, (_, m)) in items.fg_items.iter().zip(MULTIPLIERS) {
            item.set_text(multiplier_label(m));
            item.set_checked(m == multiplier);
            // Auto needs the native engine; with the engine missing only
            // «Apagada» is offered.
            let usable = match m {
                FrameGenMultiplier::Fixed(1) => true,
                FrameGenMultiplier::Auto => fg.native_available && reason.is_none(),
                _ => reason.is_none() || fg.state == FrameGenState::Active,
            };
            item.set_enabled(!locked && usable);
        }
        items
            .fg_reason
            .set_text(reason.unwrap_or_else(|| tr("tray-framegen-note")));

        items.daemon.set_text(tr("tray-daemon"));
        items.daemon.set_checked(store.daemon.on);
        items.show.set_text(tr("tray-show"));
        items.quit.set_text(tr("tray-quit"));
    }
}

/// The two lines at the top of the menu: what is happening, and fps and
/// mode while scaling.
fn summary(store: &Store) -> (String, Option<String>) {
    match &store.session {
        SessionState::Running(view) => {
            let header = tr_args("tray-scaling", &[("title", arg(view.window.title.clone()))]);
            // «60 → 120 fps · Anime nítido»: the mode, else the profile.
            let name = view
                .mode
                .clone()
                .or_else(|| view.profile.clone())
                .unwrap_or_else(|| tr("nav-default-profile"));
            let detail = match &store.stats {
                Some(s) => format!("{:.0} → {:.0} fps · {name}", s.capture_fps, s.present_fps),
                None => name,
            };
            (header, Some(detail))
        }
        SessionState::Starting { target } => (
            tr_args("tray-starting", &[("title", arg(target.title.clone()))]),
            None,
        ),
        SessionState::Failed { message, .. } => (tr("tray-failed"), Some(message.clone())),
        SessionState::Idle if store.daemon.on => (
            tr_args(
                "tray-watching",
                &[("count", arg(i64::from(store.daemon.watching)))],
            ),
            None,
        ),
        SessionState::Idle => (tr("tray-idle"), None),
    }
}

fn multiplier_label(m: FrameGenMultiplier) -> String {
    match m {
        FrameGenMultiplier::Fixed(1) => tr("fg-off"),
        FrameGenMultiplier::Auto => tr("fg-multiplier-auto"),
        FrameGenMultiplier::Fixed(n) => format!("×{n}"),
    }
}

fn profile_label(profile: &ProfileRef) -> String {
    match profile {
        ProfileRef::Default => tr("nav-default-profile"),
        ProfileRef::Named(name) => name.clone(),
    }
}

// ---- the hook -------------------------------------------------------------------

/// Create the tray and keep its menu in step with the store (called once by
/// `app.rs`). Also: the second instance's «mostrar», the window's close
/// button of the window manager, and hiding while a session runs.
pub fn use_tray(store: Signal<Store>, dispatch: Callback<Action>) {
    let window = use_window();
    let core = use_context::<CoreLink>();
    let tray: Rc<RefCell<Option<Tray>>> = use_hook(|| Rc::new(RefCell::new(None)));
    // The error was seen (window shown or menu used).
    let mut seen = use_signal(|| false);
    // The window was hidden because a session started (shown when it ends).
    let mut hidden_for_session = use_signal(|| false);
    let mut was_scaling = use_signal(|| false);

    {
        let window = window.clone();
        use_hook(move || {
            // `main.rs` creates the window hidden (see there): show it,
            // unless it starts in the tray.
            let gui = store.peek().gui.clone();
            if !(gui.start_hidden && tray_on(&gui)) {
                window.window.set_visible(true);
            }
            // The close button of the window manager goes through
            // `close_window` below; quitting sets `WindowCloses` itself.
            window.set_close_behavior(WindowCloseBehaviour::WindowHides);
            if let Some(listener) = take_instance() {
                spawn(listen_for_others(listener, window, dispatch));
            }
        });
    }

    // Icon and menu.
    {
        let tray = Rc::clone(&tray);
        use_effect(move || {
            let s = store.read();
            let _lang = LANG.read();
            let mut slot = tray.borrow_mut();
            if !s.gui.show_tray_icon {
                *slot = None;
                return;
            }
            if slot.is_none() {
                *slot = Tray::new(&s);
            }
            let raw = status(&s);
            if raw != TrayStatus::Error {
                seen.set(false);
            } else if s.visible {
                seen.set(true);
            }
            let shown = if raw == TrayStatus::Error && *seen.peek() {
                status_seen(&s)
            } else {
                raw
            };
            if let Some(tray) = slot.as_mut() {
                tray.sync(&s, shown);
            }
        });
    }

    // Hide while scaling (1.x), show again when the session ends.
    {
        let window = window.clone();
        use_effect(move || {
            let s = store.read();
            let scaling = matches!(s.session, SessionState::Running(_));
            let tray_on = s.gui.show_tray_icon;
            drop(s);
            if scaling == *was_scaling.peek() {
                return;
            }
            was_scaling.set(scaling);
            let w = &window.window;
            if scaling {
                if w.is_visible() && !w.is_minimized() {
                    if tray_on {
                        w.set_visible(false);
                    } else {
                        w.set_minimized(true);
                    }
                    hidden_for_session.set(true);
                    dispatch(Action::WindowVisible(false));
                }
            } else if *hidden_for_session.peek() {
                hidden_for_session.set(false);
                show_window(&window, dispatch);
            }
        });
    }

    // The menu.
    {
        let window = window.clone();
        let tray = Rc::clone(&tray);
        let core = core.clone();
        let on_menu = move |event: &MenuEvent| {
            seen.set(true);
            let id = event.id.0.as_str();
            tracing::debug!("tray menu: {id}");
            // What the items need, read before dispatching (which writes
            // the store).
            let (daemon_on, running) = {
                let s = store.peek();
                let running = match &s.session {
                    SessionState::Running(view) => Some((view.window.xid, view.kind)),
                    _ => None,
                };
                (s.daemon.on, running)
            };
            match id {
                "tray:stop" => dispatch(Action::StopScaling),
                "tray:restart" => dispatch(Action::RestartScaling),
                "tray:daemon" => dispatch(Action::SetDaemon { on: !daemon_on }),
                "tray:show" => {
                    hidden_for_session.set(false);
                    show_window(&window, dispatch);
                }
                "tray:quit" => request_quit(store, &window, &core, dispatch),
                _ if id.starts_with("tray:profile:") => {
                    let name = tray.borrow().as_ref().and_then(|t| {
                        t.items
                            .profile_items
                            .iter()
                            .find(|(item, _)| item.id() == &event.id)
                            .map(|(_, name)| name.clone())
                    });
                    if let (Some(name), Some((xid, kind))) = (name, running) {
                        dispatch(Action::StartScaling {
                            target: Target::Window(xid),
                            kind,
                            delay_s: None,
                            profile: Some(name),
                        });
                    }
                }
                _ => {
                    if let Some((_, m)) = MULTIPLIERS.iter().find(|(i, _)| *i == id) {
                        set_multiplier(store, dispatch, *m);
                    }
                }
            }
            // A check item toggles itself on click: put the store back.
            let s = store.peek();
            if let Some(t) = tray.borrow_mut().as_mut() {
                let shown = t.shown;
                t.sync(&s, shown);
            }
        };
        // dioxus-desktop 0.7.10 sets muda's one global handler twice; the
        // menu bar's wins, so tray clicks arrive as `MudaMenuEvent`. Listen
        // to both (the ids of the tray never clash with `dioxus-*`).
        let on_menu = Rc::new(RefCell::new(on_menu));
        let on_muda = Rc::clone(&on_menu);
        use_tray_menu_event_handler(move |event: &MenuEvent| (on_menu.borrow_mut())(event));
        use_muda_event_handler(move |event: &MenuEvent| (on_muda.borrow_mut())(event));
    }

    // The window manager's close button (Alt+F4): hidden already by
    // `WindowHides`; without the tray, that is quitting.
    use_wry_event_handler(move |event, _| {
        if let TaoEvent::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            close_window(&window, store, &core, dispatch, true);
        }
    });
}

/// «Generación de cuadros ▸ ×N»: save the global multiplier (only that
/// key, as the bar over the game does) and restart the session once
/// `ReloadConfig` went out.
fn set_multiplier(store: Signal<Store>, dispatch: Callback<Action>, m: FrameGenMultiplier) {
    let value = match m {
        FrameGenMultiplier::Auto => Value::String("auto".into()),
        FrameGenMultiplier::Fixed(n) => Value::from(u64::from(n)),
    };
    dispatch(Action::Core(Event::SaveSetting {
        profile: None,
        key: "frame_gen_multiplier".into(),
        value,
    }));
    if !matches!(store.peek().session, SessionState::Running(_)) {
        return;
    }
    spawn(async move {
        // The write ends with `ReloadConfig`; `Restart` goes after it.
        for _ in 0..100 {
            if store.peek().saving.is_none() {
                dispatch(Action::RestartScaling);
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tracing::warn!("the multiplier was not saved in time; not restarting");
    });
}

/// Show, unminimize and focus the main window.
pub fn show_window(window: &DesktopContext, dispatch: Callback<Action>) {
    window.window.set_visible(true);
    window.window.set_minimized(false);
    window.window.set_focus();
    dispatch(Action::WindowVisible(true));
}

/// The tray is there to come back from.
fn tray_on(gui: &GuiConfig) -> bool {
    gui.show_tray_icon
}

/// The title bar's minimize: to the tray with `minimize_to_tray`, else the
/// taskbar.
pub fn minimize_window(window: &DesktopContext, store: Signal<Store>, dispatch: Callback<Action>) {
    let to_tray = {
        let gui = &store.peek().gui;
        tray_on(gui) && gui.minimize_to_tray
    };
    if to_tray {
        window.window.set_visible(false);
    } else {
        window.window.set_minimized(true);
    }
    dispatch(Action::WindowVisible(false));
}

/// The title bar's close (and the window manager's, `hidden` = it hid the
/// window already): to the tray with `close_to_tray`, else quit.
pub fn close_window(
    window: &DesktopContext,
    store: Signal<Store>,
    core: &CoreLink,
    dispatch: Callback<Action>,
    hidden: bool,
) {
    let to_tray = {
        let gui = &store.peek().gui;
        tray_on(gui) && gui.close_to_tray
    };
    if to_tray {
        if !hidden {
            window.window.set_visible(false);
        }
        dispatch(Action::WindowVisible(false));
        return;
    }
    if hidden && store.peek().has_edits() {
        // The question needs the window.
        show_window(window, dispatch);
    }
    request_quit(store, window, core, dispatch);
}

/// «Salir»: ask about unsaved edits first ([`QuitDialog`]).
pub fn request_quit(
    store: Signal<Store>,
    window: &DesktopContext,
    core: &CoreLink,
    dispatch: Callback<Action>,
) {
    if store.peek().has_edits() {
        show_window(window, dispatch);
        *QUIT_ASKED.write() = true;
    } else {
        quit(store, window, core);
    }
}

/// Quit now: stop the session and the daemon unless «Seguir ejecutándose
/// tras salir» keeps the daemon (`lirtup serve` then exits by itself, see
/// `lirtup_core::lifecycle`), save the window geometry, close.
fn quit(store: Signal<Store>, window: &DesktopContext, core: &CoreLink) {
    let s = store.peek();
    let keep = s.daemon.on && s.gui.keep_running_after_exit;
    if !keep {
        if !matches!(s.session, SessionState::Idle) {
            core.send(Command::Stop);
        }
        if s.daemon.on {
            core.send(Command::SetDaemon {
                on: false,
                keep_after_exit: false,
            });
        }
    }
    let mut gui = s.gui.clone();
    drop(s);
    save_geometry(window, &mut gui);
    if let Err(e) = gui.save(None) {
        tracing::warn!("gui-config.yaml: {e}");
    }
    let window = window.clone();
    spawn(async move {
        // Let the link write the commands.
        tokio::time::sleep(Duration::from_millis(300)).await;
        window.set_close_behavior(WindowCloseBehaviour::WindowCloses);
        window.close();
    });
}

/// `«Salir» asked with unsaved edits`: [`QuitDialog`] is open.
pub static QUIT_ASKED: GlobalSignal<bool> = Signal::global(|| false);

/// «Cambios sin guardar» before quitting: Guardar y salir / Descartar y
/// salir / Cancelar. Rendered by the title bar.
#[component]
pub fn QuitDialog() -> Element {
    let store = crate::app::use_store();
    let dispatch = crate::app::use_dispatch();
    let core = use_context::<CoreLink>();
    let window = use_window();
    let mut open = use_signal(|| false);
    use_effect(move || open.set(QUIT_ASKED()));
    if !QUIT_ASKED() {
        return rsx! {};
    }
    let cancel = move |_| *QUIT_ASKED.write() = false;
    let (core2, window2) = (core.clone(), window.clone());
    let discard = move |_| {
        *QUIT_ASKED.write() = false;
        quit(store, &window2, &core2);
    };
    let save = move |_| {
        *QUIT_ASKED.write() = false;
        dispatch(Action::Save);
        let (core, window) = (core.clone(), window.clone());
        spawn(async move {
            for _ in 0..100 {
                if store.peek().saving.is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            // A failed write keeps the edits: stay, the snackbar says why.
            if store.peek().has_edits() {
                return;
            }
            quit(store, &window, &core);
        });
    };
    rsx! {
        Dialog {
            open,
            icon: "edit_note",
            icon_color: "tertiary",
            title: tr("unsaved-title"),
            on_close: move |_| *QUIT_ASKED.write() = false,
            actions: rsx! {
                Button { label: tr("unsaved-cancel"), variant: ButtonVariant::Text, onclick: cancel }
                Button { label: tr("quit-discard"), variant: ButtonVariant::Outlined, onclick: discard }
                Button { label: tr("quit-save"), icon: "save", onclick: save }
            },
            p { class: "t-bm", {tr("quit-body")} }
        }
    }
}

// ---- window geometry ----------------------------------------------------------------

/// The key of `gui-config.yaml` (kept in `GuiConfig::extra`) with the
/// window's last position and size, in logical pixels, and whether it was
/// maximized (1.x kept them in QSettings).
pub const GEOMETRY_KEY: &str = "window_geometry";

/// Where and how big the window was.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Geometry {
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
    /// Maximized (the size is the one before).
    #[serde(default)]
    pub maximized: bool,
}

/// The saved geometry, if any and sane (`main.rs` opens the window with
/// it).
pub fn saved_geometry(gui: &GuiConfig) -> Option<Geometry> {
    let value = gui.extra.get(GEOMETRY_KEY)?;
    let g: Geometry = serde_yaml_ng::from_value(value.clone()).ok()?;
    (g.width >= 320.0 && g.height >= 240.0 && g.x.is_finite() && g.y.is_finite()).then_some(g)
}

fn save_geometry(window: &DesktopContext, gui: &mut GuiConfig) {
    let w = &window.window;
    let scale = w.scale_factor();
    let maximized = w.is_maximized();
    let mut geometry = saved_geometry(gui);
    if !maximized || geometry.is_none() {
        let size = w.inner_size().to_logical::<f64>(scale);
        let pos = w
            .outer_position()
            .map(|p| p.to_logical::<f64>(scale))
            .unwrap_or_default();
        geometry = Some(Geometry {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            maximized,
        });
    }
    if let Some(mut g) = geometry {
        g.maximized = maximized;
        if let Ok(value) = serde_yaml_ng::to_value(g) {
            gui.extra.insert(Value::String(GEOMETRY_KEY.into()), value);
        }
    }
}

// ---- one instance -------------------------------------------------------------------

/// The listener of the first instance, from [`claim_instance`] (in `main`)
/// to [`use_tray`].
static INSTANCE: Mutex<Option<UnixListener>> = Mutex::new(None);

/// The socket of the running GUI, next to the scaler's.
fn instance_socket() -> PathBuf {
    lirtup_proto::socket_path().with_file_name("gui.sock")
}

/// Ask a running `lirtup-gui` to show its window. `true` if one answered
/// (this one then exits, as 1.x did).
pub fn forward_to_running() -> bool {
    let Ok(mut stream) = UnixStream::connect(instance_socket()) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    if stream.write_all(b"show\n").is_err() {
        return false;
    }
    let mut answer = [0u8; 3];
    stream.read_exact(&mut answer).is_ok() && &answer == b"ok\n"
}

/// Take the instance socket (removing a stale one). Without it the GUI
/// still runs, only without the single-instance check.
pub fn claim_instance() {
    use std::os::unix::fs::DirBuilderExt;
    let path = instance_socket();
    if let Some(dir) = path.parent() {
        let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
    }
    let _ = std::fs::remove_file(&path);
    match UnixListener::bind(&path) {
        Ok(listener) => *INSTANCE.lock().unwrap_or_else(|e| e.into_inner()) = Some(listener),
        Err(e) => tracing::warn!(
            "{}: {e}; a second lirtup-gui will not be detected",
            path.display()
        ),
    }
}

fn take_instance() -> Option<UnixListener> {
    INSTANCE.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// Answer the second instances: «show» shows the window.
async fn listen_for_others(listener: UnixListener, window: DesktopContext, dispatch: Callback<Action>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    if listener.set_nonblocking(true).is_err() {
        return;
    }
    let Ok(listener) = tokio::net::UnixListener::from_std(listener) else {
        return;
    };
    while let Ok((stream, _)) = listener.accept().await {
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        if BufReader::new(read).read_line(&mut line).await.is_ok() && line.trim() == "show" {
            tracing::info!("another lirtup-gui asked to show this one");
            show_window(&window, dispatch);
            let _ = write.write_all(b"ok\n").await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lirtup_proto::{DaemonStatus, SessionView, Size, Stage, WindowRef};

    fn view() -> SessionView {
        SessionView {
            window: WindowRef {
                xid: 1,
                title: "Utawarerumono".into(),
                class: None,
                pid: None,
            },
            profile: None,
            mode: None,
            kind: SessionKind::Fullscreen,
            src: Size::default(),
            dst: Size::default(),
            chain: Vec::new(),
            started_at_ms: 0,
        }
    }

    #[test]
    fn four_states() {
        let mut store = Store::default();
        assert_eq!(status(&store), TrayStatus::Idle);
        // Down before the scaler ever ran is not an error.
        store.link = LinkState::Down {
            reason: "no socket".into(),
        };
        assert_eq!(status(&store), TrayStatus::Idle);
        store.daemon = DaemonStatus {
            on: true,
            keep_after_exit: false,
            watching: 2,
        };
        assert_eq!(status(&store), TrayStatus::Error);
        store.link = LinkState::Connected {
            protocol: 1,
            version: "2.0.0".into(),
        };
        assert_eq!(status(&store), TrayStatus::Watching);
        store.session = SessionState::Running(view());
        assert_eq!(status(&store), TrayStatus::Scaling);
        store.session = SessionState::Failed {
            stage: Stage::Capture,
            message: "BadWindow".into(),
            retry: true,
        };
        assert_eq!(status(&store), TrayStatus::Error);
        // Seen: back to what the daemon shows.
        assert_eq!(status_seen(&store), TrayStatus::Watching);
        store.daemon.on = false;
        assert_eq!(status_seen(&store), TrayStatus::Idle);
    }

    #[test]
    fn icons_differ_and_are_square() {
        let all = [
            TrayStatus::Idle,
            TrayStatus::Watching,
            TrayStatus::Scaling,
            TrayStatus::Error,
        ]
        .map(icon_rgba);
        for (i, a) in all.iter().enumerate() {
            assert_eq!(a.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
            assert!(a.chunks(4).any(|p| p[3] == 255), "state {i} draws nothing");
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn geometry_round_trips_through_extra() {
        let mut gui = GuiConfig::default();
        assert_eq!(saved_geometry(&gui), None);
        let g = Geometry {
            x: 10.0,
            y: 20.0,
            width: 1280.0,
            height: 820.0,
            maximized: true,
        };
        gui.extra.insert(
            Value::String(GEOMETRY_KEY.into()),
            serde_yaml_ng::to_value(g).unwrap(),
        );
        assert_eq!(saved_geometry(&gui), Some(g));
        // Too small: ignored.
        gui.extra.insert(
            Value::String(GEOMETRY_KEY.into()),
            serde_yaml_ng::to_value(Geometry { width: 10.0, ..g }).unwrap(),
        );
        assert_eq!(saved_geometry(&gui), None);
    }
}
