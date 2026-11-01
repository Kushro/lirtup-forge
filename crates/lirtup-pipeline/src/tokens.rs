//! The design tokens for the bar over the game (round 4, front T): the
//! colors, shapes, type and motion of the GUI, from the same
//! `design/tokens.json`. `scripts/gen-tokens.py` writes them to
//! `crates/lirtup-ui/design/tokens.rs`; this module includes that file so
//! there is one generated copy.

// The generated constants (`SHAPE_*`, `TYPE_*`) document themselves by name;
// `cargo fmt` leaves the file as `gen-tokens.py` writes it.
#[allow(missing_docs)]
#[rustfmt::skip]
#[path = "../../lirtup-ui/design/tokens.rs"]
mod generated;

pub use generated::*;

/// Light or dark. The bar itself is always on the dark glass
/// (`--glass`, `--glass-content`), whatever the GUI's theme; the scheme
/// matters for the stats panel and the OSD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    /// Dark.
    #[default]
    Dark,
    /// Light.
    Light,
}

/// A role (`"primary"`, `"glass"`, ...) as RGBA from 0 to 1, straight
/// alpha. The channels stay **sRGB-encoded**, as in the CSS: the screen
/// texture the bar blends onto is `R8G8B8A8_UNORM` holding the captured
/// bytes as they are, and the swapchain copy does no conversion, so a
/// linear value would come out darker than the GUI's. Unknown roles are
/// magenta, so a typo shows on screen.
pub fn color(role: &str, scheme: Scheme) -> [f32; 4] {
    let roles: &[Role] = match scheme {
        Scheme::Dark => &DARK,
        Scheme::Light => &LIGHT,
    };
    roles
        .iter()
        .find(|(name, _)| *name == role)
        .map(|(_, rgba)| rgba.map(|c| f32::from(c) / 255.0))
        .unwrap_or([1.0, 0.0, 1.0, 1.0])
}

/// `color` with its alpha multiplied by `opacity` (fades, `.18` tints of
/// the design).
pub fn with_alpha(mut color: [f32; 4], opacity: f32) -> [f32; 4] {
    color[3] *= opacity.clamp(0.0, 1.0);
    color
}

/// The code point of a Material Symbols icon by name (`"push_pin"`), from
/// [`ICONS`]. The icon must be in the subset (`gen-tokens-icons.py` takes
/// the names it finds in the GUI and in this crate).
pub fn icon(name: &str) -> Option<u32> {
    ICONS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// The value of a [`Motion`] at `t` (0 to 1 of its duration): the cubic
/// Bézier of the token, for the bar's show and hide.
/// Spatial tokens overshoot: the value goes past 1 before it settles, as
/// the CSS `cubic-bezier` does.
pub fn ease(motion: &Motion, t: f32) -> f32 {
    let [x1, y1, x2, y2] = motion.bezier;
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 || t == 1.0 {
        return t;
    }
    // Solve x(s) = t for the curve parameter s (Newton, then bisection when
    // the slope is flat), then evaluate y(s).
    let bez = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    let slope = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
    };
    let mut s = t;
    for _ in 0..8 {
        let dx = bez(x1, x2, s) - t;
        if dx.abs() < 1e-5 {
            return bez(y1, y2, s);
        }
        let d = slope(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - dx / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    s = t;
    for _ in 0..30 {
        let x = bez(x1, x2, s);
        if (x - t).abs() < 1e-5 {
            break;
        }
        if x < t {
            lo = s;
        } else {
            hi = s;
        }
        s = (lo + hi) / 2.0;
    }
    bez(y1, y2, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_the_css_values() {
        let primary = color("primary", Scheme::Dark);
        assert_eq!(primary, [167.0 / 255.0, 200.0 / 255.0, 1.0, 1.0]);
        assert!(color("glass", Scheme::Light)[3] < 0.9);
        assert_eq!(color("no-such-role", Scheme::Dark), [1.0, 0.0, 1.0, 1.0]);
        assert_eq!(with_alpha([1.0; 4], 0.18)[3], 0.18);
    }

    #[test]
    fn icons_of_the_bar_are_in_the_table() {
        for name in crate::toolbar::ICON_NAMES {
            assert!(
                icon(name).is_some(),
                "{name} is not in ICONS: run scripts/gen-tokens"
            );
        }
    }

    #[test]
    fn easing_ends_and_overshoots() {
        for m in [MOTION_FAST_SPATIAL, MOTION_SPATIAL, MOTION_EFFECTS] {
            assert_eq!(ease(&m, 0.0), 0.0);
            assert_eq!(ease(&m, 1.0), 1.0);
        }
        // `fast-spatial` (y1 = 1.67) passes 1 before the end.
        assert!((0..100).any(|i| ease(&MOTION_FAST_SPATIAL, i as f32 / 100.0) > 1.0));
        // `effects` does not, and it is monotonic.
        let values: Vec<f32> = (0..=100)
            .map(|i| ease(&MOTION_EFFECTS, i as f32 / 100.0))
            .collect();
        assert!(values.windows(2).all(|w| w[1] >= w[0] - 1e-4));
        assert!(values.iter().all(|v| *v <= 1.0 + 1e-4));
        // Linear control points give back t.
        let linear = Motion {
            bezier: [0.25, 0.25, 0.75, 0.75],
            ms: 100,
        };
        assert!((ease(&linear, 0.3) - 0.3).abs() < 1e-3);
    }
}
