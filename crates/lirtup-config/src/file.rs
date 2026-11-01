//! `config.yaml`: top-level options plus `profiles:`, read and written
//! without losing what this version does not know.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::{ConfigError, ConfigWarning, Mapping, Profile};

/// The parsed `config.yaml`.
#[derive(Debug, Clone, Default)]
pub struct ConfigFile {
    /// Where it was read from (and is saved to).
    pub path: PathBuf,
    /// Top-level options without `profiles` (the *Predeterminado* profile of
    /// the GUI), as written.
    pub general: Mapping,
    /// Profiles in file order.
    pub profiles: IndexMap<String, Profile>,
}

impl ConfigFile {
    /// Read a file. A missing file is an empty config (not an error), as 1.x.
    pub fn load(path: &Path) -> Result<(ConfigFile, Vec<ConfigWarning>), ConfigError> {
        todo!("front C: port config/yaml.py")
    }

    /// [`ConfigFile::load`] on [`crate::paths::config_path`].
    pub fn load_default() -> Result<(ConfigFile, Vec<ConfigWarning>), ConfigError> {
        Self::load(&crate::paths::config_path())
    }

    /// Write to [`ConfigFile::path`]: temporary file in the same folder, then
    /// `rename`. Key order and unknown keys are kept; `hotkeys` are written as
    /// a diff against the defaults.
    ///
    /// Only the GUI calls this (the scaler never writes YAML).
    pub fn save(&self) -> Result<(), ConfigError> {
        todo!("front C")
    }

    /// A profile by name.
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }
}
