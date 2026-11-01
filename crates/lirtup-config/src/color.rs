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
        self.try_rgba().unwrap_or_else(|| {
            tracing::warn!("invalid color {self:?}, falling back to opaque black");
            [0.0, 0.0, 0.0, 1.0]
        })
    }

    /// Normalized **RGBA**, or `None` when the string is not a color.
    pub fn try_rgba(&self) -> Option<[f32; 4]> {
        match self {
            BackgroundColor::Bgra([b, g, r, a]) => {
                let all = [*r, *g, *b, *a];
                all.iter()
                    .all(|v| v.is_finite())
                    .then(|| all.map(|v| v.clamp(0.0, 1.0)))
            }
            BackgroundColor::Css(s) => parse_css(s),
        }
    }

    /// Hex string (`#RRGGBB` or `#RRGGBBAA`) for saving.
    pub fn to_hex(&self) -> String {
        let [r, g, b, a] = self.to_rgba();
        let byte = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        if a >= 1.0 {
            format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b), byte(a))
        }
    }
}

/// Parse a CSS color into RGBA floats.
fn parse_css(text: &str) -> Option<[f32; 4]> {
    let s = text.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    let lower = s.to_ascii_lowercase();
    if let Some(args) = function_args(&lower, &["rgba", "rgb"]) {
        return parse_rgb(&args);
    }
    if let Some(args) = function_args(&lower, &["hsla", "hsl"]) {
        return parse_hsl(&args);
    }
    if lower == "transparent" {
        return Some([0.0, 0.0, 0.0, 0.0]);
    }
    let rgb = NAMED_COLORS
        .binary_search_by(|(name, _)| name.cmp(&lower.as_str()))
        .ok()
        .map(|i| NAMED_COLORS[i].1)?;
    let channel = |shift: u32| ((rgb >> shift) & 0xff) as f32 / 255.0;
    Some([channel(16), channel(8), channel(0), 1.0])
}

/// `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` (alpha last, as 1.x assumes).
fn parse_hex(hex: &str) -> Option<[f32; 4]> {
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|d| d * 17);
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let [r, g, b, a] = match hex.len() {
        3 => [digit(0)?, digit(1)?, digit(2)?, 255],
        4 => [digit(0)?, digit(1)?, digit(2)?, digit(3)?],
        6 => [pair(0)?, pair(2)?, pair(4)?, 255],
        8 => [pair(0)?, pair(2)?, pair(4)?, pair(6)?],
        _ => return None,
    };
    Some([r, g, b, a].map(|v| v as f32 / 255.0))
}

/// The comma-separated arguments of `name(...)` for one of `names`.
fn function_args(s: &str, names: &[&str]) -> Option<Vec<String>> {
    let name = names.iter().find(|n| s.starts_with(**n))?;
    let rest = s[name.len()..].trim_start();
    let inner = rest.strip_prefix('(')?.trim_end().strip_suffix(')')?;
    Some(inner.split(',').map(|a| a.trim().to_owned()).collect())
}

/// Alpha as a float (0 to 1) or a percentage.
fn parse_alpha(text: Option<&String>) -> Option<f32> {
    match text {
        None => Some(1.0),
        Some(t) => match t.strip_suffix('%') {
            Some(p) => p.trim().parse::<f32>().ok().map(|v| v / 100.0),
            None => t.parse::<f32>().ok(),
        }
        .map(|v| v.clamp(0.0, 1.0)),
    }
}

/// `rgb(r, g, b)` / `rgba(r, g, b, a)`: channels 0 to 255 (clamped, as 1.x).
fn parse_rgb(args: &[String]) -> Option<[f32; 4]> {
    if !(3..=4).contains(&args.len()) {
        return None;
    }
    let channel = |t: &String| -> Option<f32> {
        match t.strip_suffix('%') {
            Some(p) => p.trim().parse::<f32>().ok().map(|v| v / 100.0),
            None => t.parse::<f32>().ok().map(|v| v / 255.0),
        }
        .map(|v| v.clamp(0.0, 1.0))
    };
    Some([
        channel(&args[0])?,
        channel(&args[1])?,
        channel(&args[2])?,
        parse_alpha(args.get(3))?,
    ])
}

/// `hsl(h, s%, l%)` / `hsla(h, s%, l%, a)`.
fn parse_hsl(args: &[String]) -> Option<[f32; 4]> {
    if !(3..=4).contains(&args.len()) {
        return None;
    }
    let hue = args[0]
        .trim_end_matches("deg")
        .trim()
        .parse::<f32>()
        .ok()?
        .rem_euclid(360.0)
        / 360.0;
    let pct = |t: &String| -> Option<f32> {
        t.trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| (v / 100.0).clamp(0.0, 1.0))
    };
    let (s, l) = (pct(&args[1])?, pct(&args[2])?);
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let channel = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    Some([
        channel(hue + 1.0 / 3.0),
        channel(hue),
        channel(hue - 1.0 / 3.0),
        parse_alpha(args.get(3))?,
    ])
}

/// SVG / CSS named colors (what `QColor` accepts), sorted by name.
const NAMED_COLORS: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn css(s: &str) -> BackgroundColor {
        BackgroundColor::Css(s.into())
    }

    #[test]
    fn named_colors_are_sorted() {
        assert!(NAMED_COLORS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn parses_the_1x_formats() {
        assert_eq!(css("black").to_rgba(), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(css("Red").to_rgba(), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(css("transparent").to_rgba(), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(css("#fff").to_rgba(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(css("#ff000080").to_hex(), "#ff000080");
        assert_eq!(css("#00FF00").to_hex(), "#00ff00");
        assert_eq!(css("rgb(0, 0, 255)").to_hex(), "#0000ff");
        assert_eq!(css("rgba(255,0,0,0.5)").to_hex(), "#ff000080");
        assert_eq!(css("hsl(120, 100%, 50%)").to_hex(), "#00ff00");
    }

    #[test]
    fn invalid_falls_back_to_black() {
        assert_eq!(css("nope").try_rgba(), None);
        assert_eq!(css("#12345").try_rgba(), None);
        assert_eq!(css("nope").to_rgba(), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn tuple_is_bgra() {
        let c = BackgroundColor::Bgra([1.0, 0.0, 0.0, 1.0]);
        assert_eq!(c.to_rgba(), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(c.to_hex(), "#0000ff");
        let yaml: BackgroundColor = serde_yaml_ng::from_str("[0.0, 0.5, 1.0, 1.0]").unwrap();
        assert_eq!(yaml, BackgroundColor::Bgra([0.0, 0.5, 1.0, 1.0]));
    }
}
