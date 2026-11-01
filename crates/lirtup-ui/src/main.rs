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

// The skeleton declares every signature with `todo!()` bodies; each front
// removes the allowances of its files when it writes them.
#![allow(dead_code, unused_imports)]

// First, so `t!` is in scope in every module after it.
#[macro_use]
mod i18n;
mod app;
mod components;
mod link;
mod route;
mod screens;
mod state;
mod theme;
mod thumbs;
mod windows;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LIRTUP_LOG")
                .unwrap_or_else(|_| "info,lirtup_ui=debug".into()),
        )
        .init();

    let window = WindowBuilder::new()
        .with_title("LirtUp Forge")
        .with_decorations(false)
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(LogicalSize::new(720.0, 520.0));
    let config = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_disable_context_menu(!cfg!(debug_assertions))
        // The dark surface, so the window does not flash white.
        .with_background_color((0x11, 0x13, 0x18, 0xff));
    dioxus::LaunchBuilder::desktop().with_cfg(config).launch(app::App);
}
