//! The screens of the GUI (6.1 of the analysis).
//!
//! The route lives in the [`crate::state::Store`], not in a router: going
//! to another screen is an [`crate::state::Action::Navigate`], so `update()`
//! can stop it with the unsaved changes dialog (flow 3) and the tests can
//! check that without Dioxus. A desktop window has no URL to keep in sync.
//! *Nuevo perfil*, *Cambios sin guardar* and *Medir en mi GPU* are dialogs
//! over the current route ([`crate::state::Dialog`]), not routes.

use serde::{Deserialize, Serialize};

/// Id of a scaling mode (file stem in `modes/`).
pub type ModeId = String;

/// Name of a profile (key in `profiles:`). The *Predeterminado* profile is
/// [`ProfileRef::Default`], not a name.
pub type ProfileName = String;

/// A profile of the drawer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProfileRef {
    /// *Predeterminado*: the top-level options of `config.yaml`.
    Default,
    /// A profile by name.
    Named(ProfileName),
}

/// Where the GUI is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Route {
    /// Inicio: session card, activation, window grid (front B).
    #[default]
    Home,
    /// Modos de escalado: the list (`None`) or one mode's editor (front D).
    Modes {
        /// The mode being edited.
        id: Option<ModeId>,
    },
    /// A profile page (front C).
    Profile {
        /// Which profile.
        id: ProfileRef,
        /// Which tab.
        tab: ProfileTab,
    },
    /// Ajustes (front D).
    Settings {
        /// Which tab.
        tab: SettingsTab,
    },
    /// Acerca de (front B).
    About,
    /// Every component with its variants (front A; reached with
    /// `LIRTUP_GUI_GALLERY=1` or Ctrl+Shift+G, not from the drawer).
    Gallery,
}

/// Tabs of a profile page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProfileTab {
    /// Escalado: mode, follow focus, pause on focus loss, daemon exclusion.
    #[default]
    Scaling,
    /// Presentación: window mode, geometry, menu bar, cursor, crop, background,
    /// frame generation.
    Presentation,
    /// Pantalla: monitor, GPU, present mode, fps limit, scale factor.
    Display,
    /// Extras: screenshots, OSD, stats panel.
    Extras,
    /// Avanzado: tiles, damage tracking, timing and recovery.
    Advanced,
}

impl ProfileTab {
    /// In display order.
    pub const ALL: [ProfileTab; 5] = [
        ProfileTab::Scaling,
        ProfileTab::Presentation,
        ProfileTab::Display,
        ProfileTab::Extras,
        ProfileTab::Advanced,
    ];

    /// The Fluent key of its label (`profile.ftl`).
    pub fn label_key(self) -> &'static str {
        match self {
            ProfileTab::Scaling => "profile-tab-scaling",
            ProfileTab::Presentation => "profile-tab-presentation",
            ProfileTab::Display => "profile-tab-display",
            ProfileTab::Extras => "profile-tab-extras",
            ProfileTab::Advanced => "profile-tab-advanced",
        }
    }

    /// Material Symbols name.
    pub fn icon(self) -> &'static str {
        match self {
            ProfileTab::Scaling => "tune",
            ProfileTab::Presentation => "web_asset",
            ProfileTab::Display => "monitor",
            ProfileTab::Extras => "more_horiz",
            ProfileTab::Advanced => "build",
        }
    }
}

/// Tabs of Ajustes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SettingsTab {
    /// Idioma, tema, semilla, bandeja, daemon.
    #[default]
    General,
    /// Atajos (`HotkeyCapture`).
    Hotkeys,
    /// Estilo: zoom, font, grid.
    Style,
}

impl SettingsTab {
    /// In display order.
    pub const ALL: [SettingsTab; 3] = [SettingsTab::General, SettingsTab::Hotkeys, SettingsTab::Style];

    /// The Fluent key of its label (`settings.ftl`).
    pub fn label_key(self) -> &'static str {
        match self {
            SettingsTab::General => "settings-tab-general",
            SettingsTab::Hotkeys => "settings-tab-hotkeys",
            SettingsTab::Style => "settings-tab-style",
        }
    }

    /// Material Symbols name.
    pub fn icon(self) -> &'static str {
        match self {
            SettingsTab::General => "settings",
            SettingsTab::Hotkeys => "keyboard",
            SettingsTab::Style => "palette",
        }
    }
}

impl Route {
    /// The profile this route edits, if any (the drawer highlights it, and
    /// leaving it with changes asks first).
    pub fn profile(&self) -> Option<&ProfileRef> {
        match self {
            Route::Profile { id, .. } => Some(id),
            _ => None,
        }
    }

    /// The mode this route edits, if any.
    pub fn mode(&self) -> Option<&str> {
        match self {
            Route::Modes { id } => id.as_deref(),
            _ => None,
        }
    }

    /// Whether `other` is the same page (only the tab differs): switching
    /// tabs never asks about unsaved changes.
    pub fn same_page(&self, other: &Route) -> bool {
        match (self, other) {
            (Route::Profile { id: a, .. }, Route::Profile { id: b, .. }) => a == b,
            (Route::Settings { .. }, Route::Settings { .. }) => true,
            _ => self == other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_of_one_profile_are_the_same_page() {
        let a = Route::Profile {
            id: ProfileRef::Default,
            tab: ProfileTab::Scaling,
        };
        let b = Route::Profile {
            id: ProfileRef::Default,
            tab: ProfileTab::Display,
        };
        let c = Route::Profile {
            id: ProfileRef::Named("Ys VIII".into()),
            tab: ProfileTab::Scaling,
        };
        assert!(a.same_page(&b));
        assert!(!a.same_page(&c));
        assert!(!a.same_page(&Route::Home));
        assert!(Route::Home.same_page(&Route::Home));
    }
}
