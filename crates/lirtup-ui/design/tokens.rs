//! Generated from `design/tokens.json` by `scripts/gen-tokens.py`; do not
//! edit by hand. The design tokens for the overlay's bar drawn in Vulkan
//! (round 4, front T): the same colors, shapes, type and motion as the GUI.

/// A color role: name and RGBA (sRGB, straight alpha).
pub type Role = (&'static str, [u8; 4]);

/// The dark scheme for the seed `#2F5F9E`.
pub const DARK: [Role; 39] = [
    ("primary", [167, 200, 255, 255]),
    ("on-primary", [4, 48, 95, 255]),
    ("primary-container", [36, 71, 119, 255]),
    ("on-primary-container", [213, 227, 255, 255]),
    ("secondary", [189, 199, 220, 255]),
    ("on-secondary", [39, 49, 65, 255]),
    ("secondary-container", [61, 71, 88, 255]),
    ("on-secondary-container", [217, 227, 248, 255]),
    ("tertiary", [255, 182, 140, 255]),
    ("on-tertiary", [82, 35, 0, 255]),
    ("tertiary-container", [114, 54, 9, 255]),
    ("on-tertiary-container", [255, 219, 201, 255]),
    ("error", [255, 180, 171, 255]),
    ("on-error", [105, 0, 5, 255]),
    ("error-container", [147, 0, 10, 255]),
    ("on-error-container", [255, 218, 214, 255]),
    ("success", [146, 214, 158, 255]),
    ("success-container", [10, 82, 39, 255]),
    ("on-success-container", [173, 243, 184, 255]),
    ("surface", [17, 19, 24, 255]),
    ("surface-dim", [17, 19, 24, 255]),
    ("surface-bright", [55, 57, 62, 255]),
    ("surface-container-lowest", [12, 14, 19, 255]),
    ("surface-container-low", [25, 28, 32, 255]),
    ("surface-container", [29, 32, 36, 255]),
    ("surface-container-high", [40, 42, 47, 255]),
    ("surface-container-highest", [51, 53, 58, 255]),
    ("on-surface", [225, 226, 233, 255]),
    ("on-surface-variant", [196, 198, 207, 255]),
    ("outline", [142, 145, 153, 255]),
    ("outline-variant", [67, 71, 78, 255]),
    ("inverse-surface", [225, 226, 233, 255]),
    ("inverse-on-surface", [46, 48, 53, 255]),
    ("inverse-primary", [62, 95, 144, 255]),
    ("scrim", [0, 0, 0, 153]),
    ("glass", [29, 32, 36, 219]),
    ("glass-content", [225, 226, 233, 255]),
    ("state-hover", [225, 226, 233, 20]),
    ("state-pressed", [225, 226, 233, 26]),
];

/// The light scheme for the seed `#2F5F9E`.
pub const LIGHT: [Role; 39] = [
    ("primary", [62, 95, 144, 255]),
    ("on-primary", [255, 255, 255, 255]),
    ("primary-container", [213, 227, 255, 255]),
    ("on-primary-container", [36, 71, 119, 255]),
    ("secondary", [85, 95, 113, 255]),
    ("on-secondary", [255, 255, 255, 255]),
    ("secondary-container", [217, 227, 248, 255]),
    ("on-secondary-container", [61, 71, 88, 255]),
    ("tertiary", [144, 77, 32, 255]),
    ("on-tertiary", [255, 255, 255, 255]),
    ("tertiary-container", [255, 219, 201, 255]),
    ("on-tertiary-container", [114, 54, 9, 255]),
    ("error", [186, 26, 26, 255]),
    ("on-error", [255, 255, 255, 255]),
    ("error-container", [255, 218, 214, 255]),
    ("on-error-container", [147, 0, 10, 255]),
    ("success", [42, 107, 61, 255]),
    ("success-container", [173, 243, 184, 255]),
    ("on-success-container", [10, 82, 39, 255]),
    ("surface", [249, 249, 255, 255]),
    ("surface-dim", [217, 218, 224, 255]),
    ("surface-bright", [249, 249, 255, 255]),
    ("surface-container-lowest", [255, 255, 255, 255]),
    ("surface-container-low", [243, 243, 250, 255]),
    ("surface-container", [237, 237, 244, 255]),
    ("surface-container-high", [231, 232, 238, 255]),
    ("surface-container-highest", [225, 226, 233, 255]),
    ("on-surface", [25, 28, 32, 255]),
    ("on-surface-variant", [67, 71, 78, 255]),
    ("outline", [116, 119, 127, 255]),
    ("outline-variant", [196, 198, 207, 255]),
    ("inverse-surface", [46, 48, 53, 255]),
    ("inverse-on-surface", [240, 240, 247, 255]),
    ("inverse-primary", [167, 200, 255, 255]),
    ("scrim", [0, 0, 0, 102]),
    ("glass", [29, 32, 36, 219]),
    ("glass-content", [225, 226, 233, 255]),
    ("state-hover", [25, 28, 32, 20]),
    ("state-pressed", [25, 28, 32, 26]),
];

/// Corner radii, px.
pub const SHAPE_XS: f32 = 4.0;
pub const SHAPE_SM: f32 = 8.0;
pub const SHAPE_MD: f32 = 12.0;
pub const SHAPE_LG: f32 = 16.0;
pub const SHAPE_LG_INC: f32 = 20.0;
pub const SHAPE_XL: f32 = 28.0;
pub const SHAPE_XL_INC: f32 = 32.0;
pub const SHAPE_XXL: f32 = 48.0;

/// A step of the M3 type scale (px, weight).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeStyle {
    pub size: f32,
    pub line: f32,
    pub weight: u16,
    pub tracking: f32,
}

pub const TYPE_DISPLAY_LARGE: TypeStyle = TypeStyle { size: 57.0, line: 64.0, weight: 500, tracking: -0.50 };
pub const TYPE_DISPLAY_MEDIUM: TypeStyle = TypeStyle { size: 45.0, line: 52.0, weight: 500, tracking: -0.30 };
pub const TYPE_HEADLINE_LARGE: TypeStyle = TypeStyle { size: 32.0, line: 40.0, weight: 500, tracking: 0.00 };
pub const TYPE_HEADLINE_MEDIUM: TypeStyle = TypeStyle { size: 28.0, line: 36.0, weight: 500, tracking: 0.00 };
pub const TYPE_HEADLINE_SMALL: TypeStyle = TypeStyle { size: 24.0, line: 32.0, weight: 500, tracking: 0.00 };
pub const TYPE_TITLE_LARGE: TypeStyle = TypeStyle { size: 22.0, line: 28.0, weight: 500, tracking: 0.00 };
pub const TYPE_TITLE_MEDIUM: TypeStyle = TypeStyle { size: 16.0, line: 24.0, weight: 600, tracking: 0.15 };
pub const TYPE_TITLE_SMALL: TypeStyle = TypeStyle { size: 14.0, line: 20.0, weight: 600, tracking: 0.10 };
pub const TYPE_BODY_LARGE: TypeStyle = TypeStyle { size: 16.0, line: 24.0, weight: 400, tracking: 0.00 };
pub const TYPE_BODY_MEDIUM: TypeStyle = TypeStyle { size: 14.0, line: 20.0, weight: 400, tracking: 0.10 };
pub const TYPE_BODY_SMALL: TypeStyle = TypeStyle { size: 12.0, line: 16.0, weight: 400, tracking: 0.20 };
pub const TYPE_LABEL_LARGE: TypeStyle = TypeStyle { size: 14.0, line: 20.0, weight: 600, tracking: 0.10 };
pub const TYPE_LABEL_MEDIUM: TypeStyle = TypeStyle { size: 12.0, line: 16.0, weight: 600, tracking: 0.40 };
pub const TYPE_LABEL_SMALL: TypeStyle = TypeStyle { size: 11.0, line: 16.0, weight: 600, tracking: 0.50 };

/// A motion token: cubic Bézier control points and duration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    pub bezier: [f32; 4],
    pub ms: u32,
}

pub const MOTION_FAST_SPATIAL: Motion = Motion { bezier: [0.42, 1.67, 0.21, 0.9], ms: 350 };
pub const MOTION_SPATIAL: Motion = Motion { bezier: [0.38, 1.21, 0.22, 1.0], ms: 500 };
pub const MOTION_SLOW_SPATIAL: Motion = Motion { bezier: [0.39, 1.29, 0.35, 0.98], ms: 650 };
pub const MOTION_EFFECTS: Motion = Motion { bezier: [0.31, 0.94, 0.34, 1.0], ms: 200 };
pub const MOTION_FAST_EFFECTS: Motion = Motion { bezier: [0.31, 0.94, 0.34, 1.0], ms: 150 };

/// Material Symbols Rounded code points of the icons the GUI uses
/// (`design/icons.json`): `ab_glyph` applies no ligatures.
pub const ICONS: [(&str, u32); 23] = [
    ("add", 0xE145),
    ("build", 0xF8CD),
    ("close", 0xE5CD),
    ("crop_square", 0xE3C6),
    ("error", 0xF8B6),
    ("home", 0xE9B2),
    ("info", 0xE88E),
    ("key", 0xE73C),
    ("keyboard", 0xE312),
    ("layers_clear", 0xE53C),
    ("light", 0xF02A),
    ("monitor", 0xEF5B),
    ("more_horiz", 0xE5D3),
    ("palette", 0xE40A),
    ("remove", 0xE15B),
    ("restart_alt", 0xF053),
    ("settings", 0xE8B8),
    ("star", 0xF09A),
    ("switch", 0xE1F4),
    ("tab", 0xE8D8),
    ("tune", 0xE429),
    ("undo", 0xE166),
    ("web_asset", 0xE069),
];
