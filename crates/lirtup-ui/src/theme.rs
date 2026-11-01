//! The theme (front A, 6.3 of the analysis): the Material 3 scheme computed
//! with `material-colors` from the seed color of `gui-config.yaml` (the 20
//! legacy palettes are seeds too), written as CSS variables in the `style`
//! of the root element. Tailwind only sees the variables; `tokens.css`
//! holds the canvas's values as the fallback.

use dioxus::prelude::*;
use lirtup_config::gui::Theme;

/// Light or dark, after resolving `Theme::System`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    /// Dark (the default).
    #[default]
    Dark,
    /// Light.
    Light,
}

/// The scheme now (Ajustes ▸ Tema writes it; `app.rs` sets it on load).
pub static SCHEME: GlobalSignal<Scheme> = Signal::global(Scheme::default);

/// `Theme::System` resolved (GTK's `prefers-color-scheme`, else dark).
pub fn resolve(theme: Theme) -> Scheme {
    match theme {
        Theme::Light => Scheme::Light,
        Theme::Dark | Theme::System => Scheme::Dark,
    }
}

/// The root element's `style`: `--primary: …; --on-primary: …; …` for every
/// role of `tokens.json`, computed from `seed` (`#RRGGBB`). Until front A
/// writes it, empty (the canvas's colors from `tokens.css` apply).
pub fn root_style(seed: &str, scheme: Scheme) -> String {
    let _ = (seed, scheme);
    String::new()
}
