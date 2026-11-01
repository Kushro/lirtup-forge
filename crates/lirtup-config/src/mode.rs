//! Scaling modes (6.6): `modes/<id>.yaml`, named by profiles with `mode: <id>`.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{ConfigError, Mapping, ModeSettings};

/// Id of the mode created by the migration from the top-level options.
pub const DEFAULT_MODE_ID: &str = "predeterminado";

/// One scaling mode file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mode {
    /// Display name («Predeterminado», «Modo de Firefox», ...).
    pub name: String,
    /// Optional description shown in the mode list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Model, samplers, effects.
    #[serde(flatten)]
    pub settings: ModeSettings,
    /// Unknown keys, kept for saving.
    #[serde(flatten)]
    pub extra: Mapping,
}

/// All modes of a `modes/` folder, keyed by id (file stem), sorted by id.
#[derive(Debug, Clone, Default)]
pub struct ModeStore {
    /// Folder the modes were read from.
    pub dir: PathBuf,
    /// Id → mode.
    pub modes: IndexMap<String, Mode>,
    /// Id → the file exactly as read (the resolver layers this, so a partial
    /// file only overrides what it sets).
    pub raw: IndexMap<String, Mapping>,
}

impl ModeStore {
    /// Read every `*.yaml` of `dir`. A missing folder is an empty store; a bad
    /// file is skipped with a warning.
    pub fn load(dir: &Path) -> Result<(ModeStore, Vec<crate::ConfigWarning>), ConfigError> {
        todo!("front C")
    }

    /// [`ModeStore::load`] on [`crate::paths::modes_dir`].
    pub fn load_default() -> Result<(ModeStore, Vec<crate::ConfigWarning>), ConfigError> {
        Self::load(&crate::paths::modes_dir())
    }

    /// Write one mode (temporary file + rename), keeping its unknown keys.
    pub fn save(&mut self, id: &str, mode: Mode) -> Result<(), ConfigError> {
        todo!("front C")
    }

    /// Delete a mode file.
    pub fn delete(&mut self, id: &str) -> Result<(), ConfigError> {
        todo!("front C")
    }

    /// A free id derived from a display name (`"Modo de Firefox"` →
    /// `modo-de-firefox`, `-2`, ...).
    pub fn new_id(&self, name: &str) -> String {
        todo!("front C")
    }
}
