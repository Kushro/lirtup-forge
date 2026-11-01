//! Hotkeys: actions, parsed key combinations and the 1.x defaults
//! (`utils/hotkeys.py`).
//!
//! Grabbing them is `lirtup_x11::hotkeys`; capturing them in the GUI is the
//! `HotkeyCapture` component. Both use [`Hotkey`].

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Something a hotkey does inside `lirtup serve`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyAction {
    /// Start scaling the focused window in fullscreen (new in 2.0, the
    /// activation group of Inicio; grabbed by `lirtup serve`, so it works with
    /// the GUI closed).
    StartFullscreen,
    /// Start scaling the focused window as a window (new in 2.0).
    StartWindowed,
    /// Pause or resume scaling.
    ToggleScaling,
    /// Stop the session and exit.
    ExitApp,
    /// Save a screenshot of the output.
    Screenshot,
    /// Show or hide the stats panel.
    ToggleStats,
    /// Next SRCNN model.
    CycleModel,
    /// Next output geometry.
    CycleGeometry,
    /// Zoom and offset back to normal.
    RestoreView,
    /// Zoom in one step.
    ZoomIn,
    /// Zoom out one step.
    ZoomOut,
    /// Move the content up.
    OffsetUp,
    /// Move the content down.
    OffsetDown,
    /// Move the content left.
    OffsetLeft,
    /// Move the content right.
    OffsetRight,
}

impl HotkeyAction {
    /// All actions: activation (2.0), then the 1.x groups (session, view,
    /// zooming, panning).
    pub const ALL: [HotkeyAction; 15] = [
        HotkeyAction::StartFullscreen,
        HotkeyAction::StartWindowed,
        HotkeyAction::ToggleScaling,
        HotkeyAction::ExitApp,
        HotkeyAction::Screenshot,
        HotkeyAction::ToggleStats,
        HotkeyAction::CycleModel,
        HotkeyAction::CycleGeometry,
        HotkeyAction::RestoreView,
        HotkeyAction::ZoomIn,
        HotkeyAction::ZoomOut,
        HotkeyAction::OffsetLeft,
        HotkeyAction::OffsetRight,
        HotkeyAction::OffsetUp,
        HotkeyAction::OffsetDown,
    ];

    /// The YAML key (`toggle_scaling`, ...).
    pub fn as_str(self) -> &'static str {
        match self {
            HotkeyAction::StartFullscreen => "start_fullscreen",
            HotkeyAction::StartWindowed => "start_windowed",
            HotkeyAction::ToggleScaling => "toggle_scaling",
            HotkeyAction::ExitApp => "exit_app",
            HotkeyAction::Screenshot => "screenshot",
            HotkeyAction::ToggleStats => "toggle_stats",
            HotkeyAction::CycleModel => "cycle_model",
            HotkeyAction::CycleGeometry => "cycle_geometry",
            HotkeyAction::RestoreView => "restore_view",
            HotkeyAction::ZoomIn => "zoom_in",
            HotkeyAction::ZoomOut => "zoom_out",
            HotkeyAction::OffsetUp => "offset_up",
            HotkeyAction::OffsetDown => "offset_down",
            HotkeyAction::OffsetLeft => "offset_left",
            HotkeyAction::OffsetRight => "offset_right",
        }
    }

    /// The 1.x default binding.
    pub fn default_binding(self) -> &'static str {
        match self {
            HotkeyAction::StartFullscreen => "Alt+Shift+A",
            HotkeyAction::StartWindowed => "Alt+Shift+Q",
            HotkeyAction::ToggleScaling => "Alt+Shift+S",
            HotkeyAction::ExitApp => "Alt+Shift+Escape",
            HotkeyAction::Screenshot => "Alt+Shift+P",
            HotkeyAction::ToggleStats => "Alt+Shift+I",
            HotkeyAction::CycleModel => "Alt+Shift+M",
            HotkeyAction::CycleGeometry => "Alt+Shift+G",
            HotkeyAction::RestoreView => "Alt+Shift+R",
            HotkeyAction::ZoomIn => "Alt+Shift+Plus",
            HotkeyAction::ZoomOut => "Alt+Shift+Minus",
            HotkeyAction::OffsetUp => "Alt+Shift+Up",
            HotkeyAction::OffsetDown => "Alt+Shift+Down",
            HotkeyAction::OffsetLeft => "Alt+Shift+Left",
            HotkeyAction::OffsetRight => "Alt+Shift+Right",
        }
    }
}

impl FromStr for HotkeyAction {
    type Err = crate::ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        HotkeyAction::ALL
            .into_iter()
            .find(|a| a.as_str() == s)
            .ok_or_else(|| crate::ConfigError::Invalid {
                key: format!("hotkeys.{s}"),
                message: "unknown hotkey action".into(),
            })
    }
}

/// The 1.x `DEFAULT_HOTKEYS` plus the 2.0 activation pair, as strings.
pub fn default_hotkeys() -> BTreeMap<String, String> {
    HotkeyAction::ALL
        .into_iter()
        .map(|a| (a.as_str().to_owned(), a.default_binding().to_owned()))
        .collect()
}

/// Only the bindings that differ from the defaults (`diff_hotkeys`), for
/// saving.
pub fn diff_hotkeys(hotkeys: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    todo!("front C")
}

/// Modifier keys of a [`Hotkey`]. Bit values are the X11 modifier masks
/// (`Shift` = 0x1, `Control` = 0x4, `Mod1`/Alt = 0x8, `Mod4`/Super = 0x40), so
/// `lirtup-x11` passes them to `GrabKey` as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Modifiers(pub u16);

impl Modifiers {
    /// Shift.
    pub const SHIFT: Modifiers = Modifiers(0x0001);
    /// Control (`Ctrl`, `Control`).
    pub const CONTROL: Modifiers = Modifiers(0x0004);
    /// Alt (X11 Mod1).
    pub const ALT: Modifiers = Modifiers(0x0008);
    /// Super (`Super`, `Win`; X11 Mod4).
    pub const SUPER: Modifiers = Modifiers(0x0040);

    /// Whether every bit of `other` is set.
    pub fn contains(self, other: Modifiers) -> bool {
        self.0 & other.0 == other.0
    }

    /// No modifier.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Modifiers;

    fn bitor(self, rhs: Modifiers) -> Modifiers {
        Modifiers(self.0 | rhs.0)
    }
}

/// A parsed key combination: `"Ctrl+Alt+Plus"` → modifiers + key name.
///
/// The key is kept as the 1.x name (`S`, `Escape`, `Plus`, `Up`, `F5`, ...);
/// `lirtup-x11` maps it to a keysym and a keycode. Displayed and saved in the
/// 1.x order (`Ctrl+Alt+Shift+Super+Key`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hotkey {
    /// Modifiers.
    pub modifiers: Modifiers,
    /// Key name.
    pub key: String,
}

impl FromStr for Hotkey {
    type Err = crate::ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        todo!("front C: port parse_hotkey from utils/hotkeys.py")
    }
}

impl TryFrom<String> for Hotkey {
    type Error = crate::ConfigError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Hotkey> for String {
    fn from(value: Hotkey) -> Self {
        value.to_string()
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("front C")
    }
}
