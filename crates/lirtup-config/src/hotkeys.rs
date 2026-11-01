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
    let defaults = default_hotkeys();
    hotkeys
        .iter()
        .filter(|(action, binding)| defaults.get(*action) != Some(*binding))
        .map(|(action, binding)| (action.clone(), binding.clone()))
        .collect()
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

    /// The modifier named `name` (`Ctrl`, `Control`, `Alt`, `Shift`, `Super`,
    /// `Win`; any case), if it is one.
    pub fn from_name(name: &str) -> Option<Modifiers> {
        match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Modifiers::CONTROL),
            "alt" => Some(Modifiers::ALT),
            "shift" => Some(Modifiers::SHIFT),
            "super" | "win" => Some(Modifiers::SUPER),
            _ => None,
        }
    }
}

/// Display and save order of the modifiers (`MODIFIER_ORDER` of 1.x).
const MODIFIER_ORDER: [(Modifiers, &str); 4] = [
    (Modifiers::CONTROL, "Ctrl"),
    (Modifiers::ALT, "Alt"),
    (Modifiers::SHIFT, "Shift"),
    (Modifiers::SUPER, "Super"),
];

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
        let invalid = |message: &str| crate::ConfigError::Invalid {
            key: format!("hotkey {s:?}"),
            message: message.into(),
        };
        let mut modifiers = Modifiers::default();
        let mut key: Option<&str> = None;
        for raw in s.split('+') {
            let token = raw.trim();
            if token.is_empty() {
                return Err(invalid("empty token"));
            }
            if let Some(m) = Modifiers::from_name(token) {
                modifiers = modifiers | m;
            } else if key.is_none() {
                key = Some(token);
            } else {
                return Err(invalid("more than one key name"));
            }
        }
        let key = key.ok_or_else(|| invalid("no key name"))?;
        Ok(Hotkey {
            modifiers,
            key: key.to_owned(),
        })
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
        for (modifier, name) in MODIFIER_ORDER {
            if self.modifiers.contains(modifier) {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display() {
        let h: Hotkey = "Alt+Shift+S".parse().unwrap();
        assert_eq!(h.modifiers, Modifiers::ALT | Modifiers::SHIFT);
        assert_eq!(h.key, "S");
        assert_eq!(h.to_string(), "Alt+Shift+S");
        let h: Hotkey = " shift + ctrl + Plus ".parse().unwrap();
        assert_eq!(h.to_string(), "Ctrl+Shift+Plus");
        let h: Hotkey = "Win+F5".parse().unwrap();
        assert_eq!(h.to_string(), "Super+F5");
        let h: Hotkey = "Escape".parse().unwrap();
        assert!(h.modifiers.is_empty());
    }

    #[test]
    fn parse_errors() {
        assert!("".parse::<Hotkey>().is_err());
        assert!("Alt++S".parse::<Hotkey>().is_err());
        assert!("Alt+Shift".parse::<Hotkey>().is_err());
        assert!("A+B".parse::<Hotkey>().is_err());
    }

    #[test]
    fn every_default_parses() {
        for action in HotkeyAction::ALL {
            let h: Hotkey = action.default_binding().parse().unwrap();
            assert_eq!(h.to_string(), action.default_binding());
            assert_eq!(action.as_str().parse::<HotkeyAction>().unwrap(), action);
        }
    }

    #[test]
    fn diff_keeps_only_changes() {
        let mut hk = default_hotkeys();
        hk.insert("zoom_in".into(), "Ctrl+Plus".into());
        hk.insert("future_action".into(), "Alt+X".into());
        let diff = diff_hotkeys(&hk);
        assert_eq!(diff.len(), 2);
        assert_eq!(diff["zoom_in"], "Ctrl+Plus");
        assert!(diff_hotkeys(&default_hotkeys()).is_empty());
    }
}
