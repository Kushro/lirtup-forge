//! The theme (front A, 6.3 of the analysis): the Material 3 scheme computed
//! with `material-colors` from the seed color of `gui-config.yaml` (the 20
//! legacy palettes are seeds too, [`PALETTES`]), written as CSS variables in
//! the `style` of the root element. Tailwind only sees the variables;
//! `tokens.css` holds the same roles for the default seed as the fallback.
//!
//! The scheme is M3's tonal spot with two fixed families on top, as the
//! canvas asks: the tertiary is always amber (what is «live»: the session,
//! unsaved changes, frame generation) and `success` is a green that M3 does
//! not have. `design/tokens.json` holds what [`roles`] gives for its seed
//! (a test checks it; `scripts/gen-tokens` rewrites it).

use std::cell::RefCell;
use std::sync::OnceLock;

use dioxus::prelude::*;
use lirtup_config::gui::Theme;
use material_colors::color::Rgb;
use material_colors::dynamic_color::{DynamicScheme, Variant};
use material_colors::palette::TonalPalette;

/// Light or dark, after resolving `Theme::System`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Scheme {
    /// Dark (the default).
    #[default]
    Dark,
    /// Light.
    Light,
}

/// The scheme now (Ajustes ▸ Tema writes it; `app.rs` sets it on load).
pub static SCHEME: GlobalSignal<Scheme> = Signal::global(Scheme::default);

/// The seed of the canvas (`gui-config.yaml`'s default).
pub const DEFAULT_SEED: &str = "#2F5F9E";

/// The 1.x palettes of «Estilo de la GUI» as seeds: their accent
/// (`control`) becomes the primary. Light and Dark were the defaults and
/// map to [`DEFAULT_SEED`]; the light and dark variants of Gruvbox and
/// Solarized share an accent, so they are one seed each.
pub const PALETTES: [(&str, &str); 20] = [
    ("Ayu Light", "#FA8D3E"),
    ("Ayu Mirage", "#FFCC66"),
    ("Catppuccin", "#CBA6F7"),
    ("Cobalt2", "#FFC600"),
    ("Cyberpunk", "#FF00FF"),
    ("Dracula", "#BD93F9"),
    ("Everforest", "#A7C080"),
    ("GitHub Dark", "#58A6FF"),
    ("Gruvbox", "#83A598"),
    ("High Contrast", "#FFFF00"),
    ("Kanagawa", "#7FB4CA"),
    ("Material Palenight", "#C792EA"),
    ("Monokai", "#FC9867"),
    ("Nord", "#88C0D0"),
    ("One Dark", "#61AFEF"),
    ("One Light", "#0184BC"),
    ("Rose Pine", "#C4A7E7"),
    ("Solarized", "#268BD2"),
    ("Synthwave 84", "#FF7EDB"),
    ("Tokyo Night", "#7AA2F7"),
];

/// The color roles, in the order of `tokens.json`.
pub const ROLES: [&str; 39] = [
    "primary",
    "on-primary",
    "primary-container",
    "on-primary-container",
    "secondary",
    "on-secondary",
    "secondary-container",
    "on-secondary-container",
    "tertiary",
    "on-tertiary",
    "tertiary-container",
    "on-tertiary-container",
    "error",
    "on-error",
    "error-container",
    "on-error-container",
    "success",
    "success-container",
    "on-success-container",
    "surface",
    "surface-dim",
    "surface-bright",
    "surface-container-lowest",
    "surface-container-low",
    "surface-container",
    "surface-container-high",
    "surface-container-highest",
    "on-surface",
    "on-surface-variant",
    "outline",
    "outline-variant",
    "inverse-surface",
    "inverse-on-surface",
    "inverse-primary",
    "scrim",
    "glass",
    "glass-content",
    "state-hover",
    "state-pressed",
];

/// Hue of the amber tertiary (the canvas's light `--tertiary`, `#8A4A1F`)
/// and the chroma of its palette.
const AMBER: (f64, f64) = (49.5, 40.0);
/// Hue and chroma of the success green (the canvas's `#276A3C`).
const GREEN: (f64, f64) = (153.0, 40.0);

/// `Theme::System` resolved: the desktop's color scheme (`GTK_THEME`, then
/// the `color-scheme` and `gtk-theme` of GNOME's settings, which Plasma
/// mirrors), read once; dark when nothing says.
pub fn resolve(theme: Theme) -> Scheme {
    match theme {
        Theme::Light => Scheme::Light,
        Theme::Dark => Scheme::Dark,
        Theme::System => {
            static SYSTEM: OnceLock<Scheme> = OnceLock::new();
            *SYSTEM.get_or_init(system_scheme)
        }
    }
}

fn system_scheme() -> Scheme {
    fn gsettings(key: &str) -> Option<String> {
        let out = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", key])
            .output()
            .ok()?;
        out.status.success().then(|| {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .trim_matches('\'')
                .to_lowercase()
        })
    }
    if let Ok(theme) = std::env::var("GTK_THEME") {
        return if theme.to_lowercase().contains("dark") {
            Scheme::Dark
        } else {
            Scheme::Light
        };
    }
    match gsettings("color-scheme").as_deref() {
        Some("prefer-dark") => Scheme::Dark,
        Some("prefer-light") => Scheme::Light,
        Some(_) => match gsettings("gtk-theme") {
            Some(name) if !name.contains("dark") => Scheme::Light,
            _ => Scheme::Dark,
        },
        None => Scheme::Dark,
    }
}

/// `#RRGGBB` (or `RRGGBB`) to a color; `None` if it is not one.
pub fn parse_hex(seed: &str) -> Option<Rgb> {
    let hex = seed.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok().map(Rgb::from_u32)
}

fn hex(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c.red, c.green, c.blue)
}

fn rgba(c: Rgb, alpha: &str) -> String {
    format!("rgba({},{},{},{alpha})", c.red, c.green, c.blue)
}

fn dynamic(seed: Rgb, dark: bool) -> DynamicScheme {
    let mut s = DynamicScheme::by_variant(seed, &Variant::TonalSpot, dark, None);
    s.tertiary_palette = TonalPalette::of(AMBER.0, AMBER.1);
    s
}

/// Every role of [`ROLES`] for `seed` (an invalid seed falls back to
/// [`DEFAULT_SEED`]), as CSS values: `#RRGGBB`, or `rgba(…)` for the
/// translucent ones.
pub fn roles(seed: &str, scheme: Scheme) -> Vec<(&'static str, String)> {
    let seed = parse_hex(seed).unwrap_or_else(|| parse_hex(DEFAULT_SEED).expect("valid default seed"));
    let dark = scheme == Scheme::Dark;
    let s = dynamic(seed, dark);
    // The overlay's glass is always dark, whatever the app's scheme.
    let glass = if dark { s.clone() } else { dynamic(seed, true) };
    let green = TonalPalette::of(GREEN.0, GREEN.1);
    let (success, success_container, on_success_container) = if dark {
        (green.tone(80), green.tone(30), green.tone(90))
    } else {
        (green.tone(40), green.tone(90), green.tone(30))
    };
    let on_surface = s.on_surface();
    let values = [
        hex(s.primary()),
        hex(s.on_primary()),
        hex(s.primary_container()),
        hex(s.on_primary_container()),
        hex(s.secondary()),
        hex(s.on_secondary()),
        hex(s.secondary_container()),
        hex(s.on_secondary_container()),
        hex(s.tertiary()),
        hex(s.on_tertiary()),
        hex(s.tertiary_container()),
        hex(s.on_tertiary_container()),
        hex(s.error()),
        hex(s.on_error()),
        hex(s.error_container()),
        hex(s.on_error_container()),
        hex(success),
        hex(success_container),
        hex(on_success_container),
        hex(s.surface()),
        hex(s.surface_dim()),
        hex(s.surface_bright()),
        hex(s.surface_container_lowest()),
        hex(s.surface_container_low()),
        hex(s.surface_container()),
        hex(s.surface_container_high()),
        hex(s.surface_container_highest()),
        hex(on_surface),
        hex(s.on_surface_variant()),
        hex(s.outline()),
        hex(s.outline_variant()),
        hex(s.inverse_surface()),
        hex(s.inverse_on_surface()),
        hex(s.inverse_primary()),
        if dark { "rgba(0,0,0,.6)" } else { "rgba(0,0,0,.4)" }.to_string(),
        rgba(glass.surface_container(), ".86"),
        hex(glass.on_surface()),
        rgba(on_surface, ".08"),
        rgba(on_surface, ".10"),
    ];
    ROLES.into_iter().zip(values).collect()
}

/// The folder of the bundled fonts, copied as is by `dx` (the `@font-face`
/// of `main.css` name `/assets/fonts/…`). Without `dx`, the desktop
/// renderer serves `/assets/` from the crate.
const FONTS: Asset = asset!("/assets/fonts", AssetOptions::folder().with_hash_suffix(false));

/// The root element's `style`: `--primary: …; --on-primary: …; …` for every
/// role, computed from `seed` (`#RRGGBB`). Cached for the last seed and
/// scheme, since `app.rs` calls it on every render of the root.
pub fn root_style(seed: &str, scheme: Scheme) -> String {
    thread_local! {
        static LAST: RefCell<Option<(String, Scheme, String)>> = const { RefCell::new(None) };
    }
    LAST.with_borrow_mut(|last| {
        if let Some((s, sc, style)) = last.as_ref()
            && s == seed
            && *sc == scheme
        {
            return style.clone();
        }
        // Keeps the font folder in the bundle.
        let _ = FONTS.bundled();
        let style: String = roles(seed, scheme)
            .into_iter()
            .map(|(role, value)| format!("--{role}: {value}; "))
            .collect();
        *last = Some((seed.to_string(), scheme, style.clone()));
        style
    })
}

/// WCAG 2.1 contrast ratio of two `#RRGGBB` colors.
pub fn contrast(a: &str, b: &str) -> f64 {
    fn lum(c: Rgb) -> f64 {
        let f = |x: u8| {
            let x = f64::from(x) / 255.0;
            if x <= 0.03928 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.red) + 0.7152 * f(c.green) + 0.0722 * f(c.blue)
    }
    let (Some(a), Some(b)) = (parse_hex(a), parse_hex(b)) else {
        return 0.0;
    };
    let (la, lb) = (lum(a), lum(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The pairs `scripts/contrast.py` measures: (foreground, background,
/// minimum ratio). Text and indicators at 4.5:1, outlines at 3:1.
pub fn contrast_pairs() -> Vec<(&'static str, &'static str, f64)> {
    let mut pairs: Vec<(&str, &str, f64)> = [
        ("on-primary", "primary"),
        ("on-primary-container", "primary-container"),
        ("on-secondary", "secondary"),
        ("on-secondary-container", "secondary-container"),
        ("on-tertiary", "tertiary"),
        ("on-tertiary-container", "tertiary-container"),
        ("on-error", "error"),
        ("on-error-container", "error-container"),
        ("on-success-container", "success-container"),
    ]
    .into_iter()
    .map(|(fg, bg)| (fg, bg, 4.5))
    .collect();
    for bg in [
        "surface",
        "surface-container-lowest",
        "surface-container-low",
        "surface-container",
        "surface-container-high",
        "surface-container-highest",
    ] {
        for fg in [
            "on-surface",
            "on-surface-variant",
            "primary",
            "tertiary",
            "error",
            "success",
        ] {
            pairs.push((fg, bg, 4.5));
        }
        pairs.push(("outline", bg, 3.0));
    }
    pairs.push(("inverse-on-surface", "inverse-surface", 4.5));
    pairs.push(("inverse-primary", "inverse-surface", 4.5));
    pairs.push(("surface", "success", 4.5));
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> serde_json::Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/design/tokens.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// `tokens.json` holds what `roles` gives for its seed. With
    /// `LIRTUP_THEME_DUMP=<file>` the computed colors go to that file
    /// instead (`scripts/gen-tokens` merges them into `tokens.json`).
    #[test]
    fn tokens_json_matches_the_seed() {
        let tokens = tokens();
        let seed = tokens["seed"].as_str().unwrap();
        let computed: serde_json::Map<String, serde_json::Value> =
            [("dark", Scheme::Dark), ("light", Scheme::Light)]
                .into_iter()
                .map(|(name, scheme)| {
                    let roles = roles(seed, scheme)
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), serde_json::Value::String(v)))
                        .collect();
                    (name.to_string(), serde_json::Value::Object(roles))
                })
                .collect();
        if let Some(path) = std::env::var_os("LIRTUP_THEME_DUMP") {
            std::fs::write(path, serde_json::to_string(&computed).unwrap()).unwrap();
            return;
        }
        for (name, roles) in &computed {
            for (role, value) in roles.as_object().unwrap() {
                assert_eq!(
                    tokens["color"][name][role].as_str().map(str::to_uppercase),
                    value.as_str().map(str::to_uppercase),
                    "{name} {role}: run scripts/gen-tokens"
                );
            }
        }
    }

    /// Every pair passes in both schemes for the default seed and the 20
    /// legacy palettes.
    #[test]
    fn every_seed_passes_contrast() {
        let mut bad = Vec::new();
        let seeds = std::iter::once(DEFAULT_SEED).chain(PALETTES.iter().map(|(_, s)| *s));
        for seed in seeds {
            for scheme in [Scheme::Dark, Scheme::Light] {
                let roles: std::collections::HashMap<_, _> = roles(seed, scheme).into_iter().collect();
                for (fg, bg, need) in contrast_pairs() {
                    let ratio = contrast(&roles[fg], &roles[bg]);
                    if ratio < need {
                        bad.push(format!("{seed} {scheme:?}: {fg} on {bg} = {ratio:.2}"));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn style_has_every_role_and_bad_seeds_fall_back() {
        let style = root_style("#123456", Scheme::Light);
        for role in ROLES {
            assert!(style.contains(&format!("--{role}: ")), "{role}");
        }
        assert_eq!(roles("nope", Scheme::Dark), roles(DEFAULT_SEED, Scheme::Dark));
        assert_eq!(resolve(Theme::Light), Scheme::Light);
    }
}
