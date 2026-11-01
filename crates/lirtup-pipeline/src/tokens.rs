//! The design tokens for the bar over the game (round 4, front T): the
//! colors, shapes, type and motion of the GUI, from the same
//! `design/tokens.json`. `scripts/gen-tokens.py` writes them to
//! `crates/lirtup-ui/design/tokens.rs`; this module includes that file so
//! there is one generated copy.

// The generated constants (`SHAPE_*`, `TYPE_*`) document themselves by name.
#[allow(missing_docs)]
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

/// A role (`"primary"`, `"glass"`, ...) as linear RGBA for a shader
/// (sRGB → linear, straight alpha). Unknown roles are magenta, so a typo
/// shows on screen.
pub fn color(role: &str, scheme: Scheme) -> [f32; 4] {
    let _ = (role, scheme);
    todo!()
}

/// The code point of a Material Symbols icon by name (`"push_pin"`), from
/// [`ICONS`]. The icon must be in the subset (`gen-tokens-icons.py` takes
/// the names it finds in the GUI and in this crate).
pub fn icon(name: &str) -> Option<u32> {
    ICONS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// The value of a [`Motion`] at `t` (0 to 1 of its duration): the cubic
/// Bézier of the token, for the bar's show and hide.
pub fn ease(motion: &Motion, t: f32) -> f32 {
    let _ = (motion, t);
    todo!()
}
