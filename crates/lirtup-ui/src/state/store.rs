//! The bodies of the state (front C): [`Field`], the derived views of the
//! [`Store`] and [`update`].
//!
//! The signatures are the contract with B and D; the bodies are C's. The
//! tests of `update()` live here (`#[cfg(test)]`, no Dioxus).

use lirtup_config::{ConfigFile, Mapping, ModeStore, Value};
use lirtup_x11::WindowInfo;

use super::{Action, Effect, Field, Key, Origin, Scope, Store};
use crate::route::{ProfileRef, ProfileTab};

/// [`Store::logs`] keeps at most this many lines.
pub const MAX_LOGS: usize = 200;

impl<T: Clone + PartialEq> Field<T> {
    /// The value the row shows: the edit, else this layer's, else the
    /// default.
    pub fn value(&self) -> T {
        todo!()
    }

    /// Where the value comes from; `fmt` formats the saved value for
    /// «antes: …».
    pub fn origin(&self, fmt: impl Fn(&T) -> String) -> Origin {
        let _ = fmt;
        todo!()
    }

    /// There is an unsaved edit that changes the saved value.
    pub fn is_dirty(&self) -> bool {
        todo!()
    }
}

/// The keys of each profile tab, in display order (the tab's dot, Restablecer
/// per tab). Keys that are not in any tab are kept but never shown.
pub fn tab_keys(tab: ProfileTab) -> &'static [&'static str] {
    let _ = tab;
    todo!()
}

/// The tab a key belongs to.
pub fn tab_of(key: &str) -> Option<ProfileTab> {
    let _ = key;
    todo!()
}

/// Keys that are global even when edited from a profile page (frame
/// generation; 6.7 of the analysis lists the rest to check): edits go to
/// *Predeterminado* and the row says so.
pub fn global_keys() -> &'static [&'static str] {
    todo!()
}

/// The file with the edits of `doc` merged in (what Guardar writes).
/// Unknown keys stay as read.
pub fn apply_edits(
    file: &ConfigFile,
    edits: &indexmap::IndexMap<(ProfileRef, Key), Option<Value>>,
) -> ConfigFile {
    let _ = (file, edits);
    todo!()
}

/// The modes with their edits merged in, only the ones that changed.
pub fn apply_mode_edits(
    modes: &ModeStore,
    edits: &indexmap::IndexMap<(String, Key), Option<Value>>,
) -> Vec<(String, lirtup_config::Mode)> {
    let _ = (modes, edits);
    todo!()
}

impl Store {
    /// One setting of one layer, typed. `T` is read from the YAML value with
    /// serde; a value that does not parse counts as not set.
    pub fn field<T: Clone + PartialEq + serde::de::DeserializeOwned>(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Field<T> {
        let _ = (scope, key);
        todo!()
    }

    /// The raw layer of a scope as saved (`general`, a profile's `options`,
    /// a mode's mapping).
    pub fn layer(&self, scope: &Scope) -> Option<&Mapping> {
        let _ = scope;
        todo!()
    }

    /// The origin of one row (shorthand for `field(...).origin(...)` with the
    /// value printed as YAML).
    pub fn origin(&self, scope: &Scope, key: &str) -> Origin {
        let _ = (scope, key);
        todo!()
    }

    /// Unsaved edits of a scope (the save bar's «N cambios sin guardar» when
    /// the page is a profile or a mode).
    pub fn dirty_count(&self, scope: &Scope) -> usize {
        let _ = scope;
        todo!()
    }

    /// Unsaved edits anywhere (Guardar is enabled; leaving the GUI asks).
    pub fn has_edits(&self) -> bool {
        todo!()
    }

    /// The tab's dot.
    pub fn tab_dirty(&self, profile: &ProfileRef, tab: ProfileTab) -> bool {
        let _ = (profile, tab);
        todo!()
    }

    /// The drawer's dot.
    pub fn profile_dirty(&self, profile: &ProfileRef) -> bool {
        let _ = profile;
        todo!()
    }

    /// Profiles in drawer order: *Predeterminado* first, then the file
    /// order.
    pub fn profiles(&self) -> Vec<ProfileRef> {
        todo!()
    }

    /// The profile a session is scaling with (the drawer's amber ▶).
    pub fn running_profile(&self) -> Option<ProfileRef> {
        todo!()
    }

    /// The profile whose match rules fit a window (the tile's label and what
    /// a click scales with), as the core does (`find_matching_profile`).
    pub fn matching_profile(&self, window: &WindowInfo) -> Option<ProfileRef> {
        let _ = window;
        todo!()
    }

    /// The mode a profile uses after inheritance (its own `mode`, else
    /// *Predeterminado*'s).
    pub fn mode_of(&self, profile: &ProfileRef) -> Option<String> {
        let _ = profile;
        todo!()
    }
}

/// See [`super::update`].
pub(super) fn update(store: &mut Store, action: Action) -> Option<Effect> {
    let _ = (store, action);
    todo!()
}
