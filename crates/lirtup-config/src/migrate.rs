//! Migration of a 1.x configuration to scaling modes (round 4, front M;
//! 6.6 of the analysis).
//!
//! 1.x keeps the mode fields ([`crate::MODE_KEYS`]: model, double upscale,
//! sampling, effects engine and chain, final touches) loose, at the top
//! level and in each profile. 2.0 reads them that way too (they win over
//! the mode a layer names), so nothing breaks without migrating. Migrating
//! makes them modes:
//!
//! - every distinct combination of mode fields (the top level, and each
//!   profile's own fields over it) becomes one mode in `modes/`, named
//!   «Predeterminado» for the top level and «Modo de <perfil>» for the rest;
//!   profiles with the same combination share one;
//! - the top level and each profile then name their mode (`mode: <id>`) and
//!   lose the loose fields; a profile whose combination is the top level's
//!   names nothing (it inherits);
//! - `config.yaml` is copied first to `config.yaml.1x.bak`, and the mode
//!   files are written before `config.yaml`, so a failure leaves the 1.x
//!   file in place;
//! - resolving any window gives the same `Config` before and after (the
//!   criterion of 5.8, checked with the person's real configuration).
//!
//! The GUI offers it on the first run with mode fields to move
//! (`Dialog::Migrate`); `lirtup migrate` does it from the command line.

use std::path::PathBuf;

use crate::{ConfigError, ConfigFile, Mapping, ModeStore};

/// The suffix of the backup of `config.yaml`.
pub const BACKUP_SUFFIX: &str = ".1x.bak";

/// What a migration would do.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MigrationPlan {
    /// New modes: id (file stem) and the file's content (a raw layer with
    /// `name` and the fields it sets).
    pub modes: Vec<(String, Mapping)>,
    /// The mode the top level names (`None` = no mode fields at the top).
    pub top_mode: Option<String>,
    /// For each profile with mode fields: its name and the mode it will
    /// name (`None` = the top level's, by inheritance).
    pub profiles: Vec<(String, Option<String>)>,
    /// How many loose fields move into modes (for the dialog).
    pub moved_fields: usize,
}

impl MigrationPlan {
    /// Nothing to do.
    pub fn is_empty(&self) -> bool {
        self.modes.is_empty() && self.profiles.is_empty() && self.top_mode.is_none()
    }
}

/// Whether the file has loose mode fields to move.
pub fn needs_migration(file: &ConfigFile) -> bool {
    let _ = file;
    todo!()
}

/// The plan for `file`, with ids that do not clash with `modes`.
pub fn plan(file: &ConfigFile, modes: &ModeStore) -> MigrationPlan {
    let _ = (file, modes);
    todo!()
}

/// Apply `plan`: back up `config.yaml`, write the modes, then the new
/// `config.yaml`. Returns the backup's path.
pub fn apply(
    file: &mut ConfigFile,
    modes: &mut ModeStore,
    plan: &MigrationPlan,
) -> Result<PathBuf, ConfigError> {
    let _ = (file, modes, plan);
    todo!()
}
