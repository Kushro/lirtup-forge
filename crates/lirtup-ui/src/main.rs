//! `lirtup-gui`: the Dioxus Desktop GUI of LirtUp Forge 2.0 (section 6 of the
//! analysis). It talks to `lirtup serve` over its socket ([`link`]) and is
//! the only writer of the YAML files ([`state`]).
//!
//! | Module | Owner (round 3) | What |
//! | --- | --- | --- |
//! | [`app`] | reviewer | contexts, effect runner, title bar, drawer, save bar, snackbar, dialogs, shortcuts |
//! | [`route`] | reviewer | the screens and tabs |
//! | [`link`] | reviewer | `CoreLink`, the socket client |
//! | [`i18n`] | reviewer | `t!`, the Fluent files |
//! | [`state`] | reviewer (types), C (`store.rs` bodies) | `Store`, `Action`, `Effect`, `update` |
//! | [`components`] | A | the component library and its gallery |
//! | [`theme`] | A | the scheme from the seed color |
//! | [`screens`] | B, C, D | one folder per front |
//! | [`windows`], [`thumbs`] | B | window list, previews and icons |
//! | [`tray`], [`notify`] | Y (round 4) | the tray and desktop notifications |

// The skeleton declares every signature with `todo!()` bodies; each front
// removes the allowances of its files when it writes them.
#![allow(dead_code, unused_imports)]

// First, so `t!` is in scope in every module after it.
#[macro_use]
mod i18n;
mod app;
mod components;
#[cfg(test)]
mod i18n_check;
mod link;
mod notify;
mod route;
mod screens;
mod state;
mod theme;
mod thumbs;
mod tray;
mod windows;

use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder, WindowCloseBehaviour};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LIRTUP_LOG")
                .unwrap_or_else(|_| "info,lirtup_ui=debug".into()),
        )
        .init();

    // One instance: a second one shows the first and exits (round 4,
    // front Y, `tray`).
    if tray::forward_to_running() {
        tracing::info!("lirtup-gui is already running; showing it");
        return;
    }
    tray::claim_instance();

    // Where the window was (front Y). It is created hidden and `tray`
    // shows it unless `start_hidden`: a window created hidden makes the
    // Dioxus interpreter apply edits at once instead of on
    // `requestAnimationFrame`, which WebKit never runs while the window is
    // hidden; without that the whole app (the link to the scaler too)
    // stalls while it sits in the tray.
    let gui = lirtup_config::GuiConfig::load(None).unwrap_or_default();
    let geometry = tray::saved_geometry(&gui);
    let mut window = WindowBuilder::new()
        .with_title("LirtUp Forge")
        .with_decorations(false)
        .with_visible(false)
        .with_inner_size(geometry.map_or(LogicalSize::new(1280.0, 820.0), |g| {
            LogicalSize::new(g.width, g.height)
        }))
        .with_min_inner_size(LogicalSize::new(720.0, 520.0));
    if let Some(g) = geometry {
        window = window
            .with_position(LogicalPosition::new(g.x, g.y))
            .with_maximized(g.maximized);
    }
    let config = Config::new()
        .with_close_behaviour(WindowCloseBehaviour::WindowHides)
        .with_window(window)
        .with_menu(None)
        .with_disable_context_menu(!cfg!(debug_assertions))
        // wry's file-drop handler eats the HTML5 drag events the effect
        // chain reorders with (front D's note); the GUI takes no file drops.
        .with_disable_drag_drop_handler(true)
        // The dark surface, so the window does not flash white.
        .with_background_color((0x11, 0x13, 0x18, 0xff));
    // The event loop never returns; a stale instance socket is replaced by
    // the next `claim_instance`.
    dioxus::LaunchBuilder::desktop().with_cfg(config).launch(app::App);
}
