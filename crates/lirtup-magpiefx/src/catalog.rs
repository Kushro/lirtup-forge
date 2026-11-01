//! The effect catalog (`list_effects`, `find_effect`), for `ListEffects`.

use std::path::{Path, PathBuf};

use crate::{EffectDesc, Result};

/// Where an effect comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectOrigin {
    /// Shipped (`src/upscaler/magpiefx/effects`, later `/usr/share/lirtup`).
    Builtin,
    /// `~/.config/linux-rt-upscaler/effects`.
    User,
}

/// One effect of the catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    /// Name (path relative to its root, without `.hlsl`; `Anime4K/...`).
    pub name: String,
    /// File.
    pub path: PathBuf,
    /// Origin.
    pub origin: EffectOrigin,
    /// Checked against Magpie's output (`magpiefx-report.md`); the GUI labels
    /// the rest «Sin probar».
    pub verified: bool,
}

/// All effects found.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Entries sorted by name; user effects shadow built-in ones.
    pub entries: Vec<CatalogEntry>,
}

/// Folder of the built-in effects.
pub fn builtin_dir() -> PathBuf {
    todo!("front E")
}

/// Names of the effects verified in `magpiefx-report.md` (a table embedded at
/// build time).
pub fn verified_names() -> &'static [&'static str] {
    todo!("front E")
}

impl Catalog {
    /// Scan the built-in folder and `user_dir`.
    pub fn scan(user_dir: Option<&Path>) -> Catalog {
        todo!("front E")
    }

    /// Find by name or path.
    pub fn find(&self, name_or_path: &str) -> Option<&CatalogEntry> {
        todo!("front E")
    }

    /// Read and parse an entry (for `EffectInfo`: passes and parameters).
    pub fn describe(&self, entry: &CatalogEntry) -> Result<EffectDesc> {
        todo!("front E")
    }
}
