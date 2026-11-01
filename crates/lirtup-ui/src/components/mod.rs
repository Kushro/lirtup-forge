//! The component library (6.2 of the analysis; front A writes the bodies).
//!
//! Every component of the design is declared here with its props; the
//! screens (B, C, D) use only these and never style a control themselves.
//! The base classes of each one are in the design's implementation notes
//! (artboard «Dioxus + Tailwind» of the canvas) and the tokens in
//! `design/tokens.json`. Every component:
//!
//! - has a visible focus ring (3 px, `--primary`) and works with the
//!   keyboard (Tab, Enter/Space, arrows where the design says);
//! - turns off shape morphs and springs under `prefers-reduced-motion`;
//! - takes its texts translated (the caller passes `t!(...)` results);
//!   its own fixed texts (aria labels) come from `components.ftl`.
//!
//! | Module | Components |
//! | --- | --- |
//! | [`buttons`] | [`Button`], [`IconButton`], [`SplitButton`] |
//! | [`inputs`] | [`Connected`], [`Switch`], [`Checkbox`], [`Slider`], [`Select`], [`TextField`], [`PathField`], [`ColorField`], [`FontField`] |
//! | [`rows`] | [`SettingRow`], [`SettingGroup`] |
//! | [`nav`] | [`NavDrawer`], [`NavHeader`], [`NavItem`], [`Tabs`] |
//! | [`tile`] | [`WindowTile`] |
//! | [`hotkey`] | [`HotkeyCapture`], [`Keycap`] |
//! | [`overlays`] | [`FloatingToolbar`], [`Snackbar`], [`Dialog`], [`Menu`], [`Tooltip`] |
//! | [`misc`] | [`Badge`], [`Chip`], [`Icon`], [`LoadingIndicator`] |
//! | [`gallery`] | [`Gallery`]: every component and variant, in both themes (`Route::Gallery`) |

pub mod buttons;
pub mod gallery;
pub mod hotkey;
pub mod inputs;
pub mod misc;
pub mod nav;
pub mod overlays;
pub mod rows;
pub mod tile;

pub use buttons::*;
pub use gallery::Gallery;
pub use hotkey::*;
pub use inputs::*;
pub use misc::*;
pub use nav::*;
pub use overlays::*;
pub use rows::*;
pub use tile::*;

/// One entry of a [`Menu`] or a [`SplitButton`]. `id` is what `on_select`
/// gets back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    /// Returned by `on_select`.
    pub id: &'static str,
    /// Material Symbols name.
    pub icon: Option<&'static str>,
    /// Text (translated).
    pub label: String,
    /// Error color (Eliminar).
    pub danger: bool,
    /// Greyed out.
    pub disabled: bool,
    /// A divider before this entry.
    pub divider_before: bool,
}

impl MenuItem {
    /// A plain entry.
    pub fn new(id: &'static str, label: impl Into<String>) -> MenuItem {
        MenuItem {
            id,
            icon: None,
            label: label.into(),
            danger: false,
            disabled: false,
            divider_before: false,
        }
    }

    /// With an icon.
    pub fn icon(mut self, icon: &'static str) -> MenuItem {
        self.icon = Some(icon);
        self
    }
}
