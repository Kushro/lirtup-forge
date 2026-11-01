//! `background_color`: a CSS color string or a 4-tuple, as in `utils/color.py`.

use serde::{Deserialize, Serialize};

/// Color of the letterbox bars.
///
/// YAML accepts what 1.x accepts: named colors (`black`, `transparent`), hex
/// (`#RGB`, `#RRGGBB`, `#RRGGBBAA`), `rgb(...)`, `rgba(...)`, `hsl(...)`, or a
/// list of four floats. Saved as hex.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackgroundColor {
    /// Four floats in **BGRA** order, 0 to 1 (what the 1.x shader takes).
    Bgra([f32; 4]),
    /// A CSS color string.
    Css(String),
}

impl Default for BackgroundColor {
    fn default() -> Self {
        BackgroundColor::Css("black".into())
    }
}

impl BackgroundColor {
    /// Normalized **RGBA**, 0 to 1. Invalid strings fall back to opaque black
    /// with a warning, as `color_string_to_float4` does.
    pub fn to_rgba(&self) -> [f32; 4] {
        todo!("front C: port utils/color.py")
    }

    /// Hex string (`#RRGGBB` or `#RRGGBBAA`) for saving.
    pub fn to_hex(&self) -> String {
        todo!("front C")
    }
}
