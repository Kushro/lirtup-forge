//! The effect catalog (`list_effects`, `find_effect`), for `ListEffects`.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::{EffectDesc, MagpieError, Result};

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

impl CatalogEntry {
    /// An entry for a `.hlsl` file outside the catalog (a path in
    /// `magpie_effects:`); named after its file stem, not verified.
    pub fn from_path(path: &Path) -> CatalogEntry {
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        CatalogEntry { name, path: path.to_path_buf(), origin: EffectOrigin::User, verified: false }
    }

    /// The effect's file stem (`Anime4K_Upscale_S`), the name the parser and
    /// the logs use.
    pub fn stem(&self) -> String {
        self.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| self.name.clone())
    }
}

/// All effects found.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Entries sorted by name; user effects shadow built-in ones.
    pub entries: Vec<CatalogEntry>,
}

/// Environment variable that overrides [`builtin_dir`].
pub const EFFECTS_DIR_ENV: &str = "LIRTUP_EFFECTS_DIR";

/// Where an installed build keeps them (round 4).
const SYSTEM_EFFECTS_DIR: &str = "/usr/share/lirtup/effects";

/// Folder of the built-in effects: `$LIRTUP_EFFECTS_DIR`, else the
/// repository's `src/upscaler/magpiefx/effects` when it exists (development
/// builds; decision 5 of the contract), else `/usr/share/lirtup/effects`.
pub fn builtin_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(EFFECTS_DIR_ENV) {
        return PathBuf::from(dir);
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/upscaler/magpiefx/effects");
    if repo.is_dir() {
        return repo.canonicalize().unwrap_or(repo);
    }
    PathBuf::from(SYSTEM_EFFECTS_DIR)
}

static REPORT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/developer/magpiefx-report.md"));

static VERIFIED: LazyLock<Vec<&'static str>> = LazyLock::new(|| parse_report(REPORT));

/// Effects whose `run` column says `ok` in the report's table
/// (`| \`Name.hlsl\` | dxc | glslang | run |`), without `.hlsl`.
fn parse_report(report: &'static str) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = report
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            // ["", name, dxc, glslang, run, ""]
            if cells.len() != 6 {
                return None;
            }
            let name = cells[1].strip_prefix('`')?.strip_suffix('`')?.strip_suffix(".hlsl")?;
            cells[4].starts_with("ok").then_some(name)
        })
        .collect();
    names.sort_unstable();
    names
}

/// Names of the effects verified in `magpiefx-report.md` (a table embedded at
/// build time).
pub fn verified_names() -> &'static [&'static str] {
    &VERIFIED
}

/// Every `.hlsl` under `root` as `(name, path)`, names with `/`.
fn scan_root(root: &Path) -> Vec<(String, PathBuf)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().is_some_and(|e| e == "hlsl") {
                let rel = path.strip_prefix(root).unwrap_or(&path).with_extension("");
                let name = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                out.push((name, path));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

impl Catalog {
    /// Scan the built-in folder and `user_dir`.
    pub fn scan(user_dir: Option<&Path>) -> Catalog {
        Catalog::scan_dirs(&builtin_dir(), user_dir)
    }

    /// [`Catalog::scan`] with an explicit built-in folder.
    pub fn scan_dirs(builtin: &Path, user_dir: Option<&Path>) -> Catalog {
        let verified = verified_names();
        let mut entries: Vec<CatalogEntry> = scan_root(builtin)
            .into_iter()
            .map(|(name, path)| CatalogEntry {
                verified: verified.binary_search(&name.as_str()).is_ok(),
                name,
                path,
                origin: EffectOrigin::Builtin,
            })
            .collect();
        if let Some(user) = user_dir {
            for (name, path) in scan_root(user) {
                entries.retain(|e| e.name != name);
                entries.push(CatalogEntry { name, path, origin: EffectOrigin::User, verified: false });
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Catalog { entries }
    }

    /// Find by name or path: the exact name (with or without `.hlsl`), the
    /// path of an entry, or (like 1.x `find_effect`) a file name anywhere in
    /// the tree, user effects first.
    pub fn find(&self, name_or_path: &str) -> Option<&CatalogEntry> {
        let stem = name_or_path.strip_suffix(".hlsl").unwrap_or(name_or_path).replace('\\', "/");
        if let Some(e) = self.entries.iter().find(|e| e.name == stem) {
            return Some(e);
        }
        let as_path = Path::new(name_or_path);
        let canonical = as_path.canonicalize().ok();
        if let Some(e) = self.entries.iter().find(|e| {
            e.path == as_path || canonical.as_ref().is_some_and(|c| e.path.canonicalize().ok().as_ref() == Some(c))
        }) {
            return Some(e);
        }
        let base = stem.rsplit('/').next().unwrap_or(&stem);
        let by_file = |origin| {
            self.entries
                .iter()
                .find(|e| e.origin == origin && e.path.file_stem().is_some_and(|s| s.to_string_lossy() == base))
        };
        by_file(EffectOrigin::User).or_else(|| by_file(EffectOrigin::Builtin))
    }

    /// [`Catalog::find`], or an entry for an existing `.hlsl` path outside
    /// the catalog.
    pub fn resolve(&self, name_or_path: &str) -> Result<CatalogEntry> {
        if let Some(e) = self.find(name_or_path) {
            return Ok(e.clone());
        }
        let path = Path::new(name_or_path);
        if path.is_file() {
            return Ok(CatalogEntry::from_path(path));
        }
        Err(MagpieError::NotFound(name_or_path.to_owned()))
    }

    /// Read and parse an entry (for `EffectInfo`: passes and parameters).
    pub fn describe(&self, entry: &CatalogEntry) -> Result<EffectDesc> {
        let source = read_source(&entry.path)?;
        crate::parser::parse_effect(&source, &entry.stem())
    }
}

/// Read an effect file (UTF-8, BOM dropped, as 1.x `utf-8-sig`).
pub(crate) fn read_source(path: &Path) -> Result<String> {
    let source = std::fs::read_to_string(path)?;
    Ok(source.strip_prefix('\u{feff}').map(str::to_owned).unwrap_or(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_lists_86_verified_effects() {
        let names = verified_names();
        assert_eq!(names.len(), 86);
        assert!(names.contains(&"Anime4K/Anime4K_Upscale_S"));
        assert!(names.contains(&"Pixel Art/MMPX"));
        assert!(names.contains(&"FXAA/FXAA_High"));
        assert!(!names.contains(&"NIS/NIS"));
        assert!(!names.contains(&"RAVU/RAVU_R2"));
    }

    #[test]
    fn builtin_catalog() {
        let catalog = Catalog::scan(None);
        assert_eq!(catalog.entries.len(), 117);
        assert_eq!(catalog.entries.iter().filter(|e| e.verified).count(), 86);
        assert!(catalog.entries.windows(2).all(|w| w[0].name < w[1].name));
        let e = catalog.find("Sharpen/LumaSharpen").unwrap();
        assert_eq!(catalog.find("LumaSharpen").unwrap(), e);
        assert_eq!(catalog.find("LumaSharpen.hlsl").unwrap(), e);
        assert_eq!(catalog.find(&e.path.to_string_lossy()).unwrap(), e);
        assert!(catalog.find("Nope").is_none());
        let desc = catalog.describe(e).unwrap();
        assert_eq!(desc.name, "LumaSharpen");
        assert!(!desc.params.is_empty());
    }

    #[test]
    fn user_effects_shadow_builtins() {
        let builtin = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(builtin.path().join("Sharpen")).unwrap();
        std::fs::write(builtin.path().join("Sharpen/LumaSharpen.hlsl"), "x").unwrap();
        std::fs::write(builtin.path().join("A.hlsl"), "x").unwrap();
        std::fs::write(builtin.path().join("A.hlsli"), "x").unwrap();
        std::fs::write(user.path().join("A.hlsl"), "y").unwrap();
        std::fs::write(user.path().join("Mine.hlsl"), "y").unwrap();
        let catalog = Catalog::scan_dirs(builtin.path(), Some(user.path()));
        let names: Vec<_> = catalog.entries.iter().map(|e| (e.name.as_str(), e.origin)).collect();
        assert_eq!(
            names,
            [
                ("A", EffectOrigin::User),
                ("Mine", EffectOrigin::User),
                ("Sharpen/LumaSharpen", EffectOrigin::Builtin)
            ]
        );
        assert!(catalog.resolve("Mine").is_ok());
        assert!(matches!(catalog.resolve("Other"), Err(MagpieError::NotFound(_))));
        let outside = user.path().join("sub.hlsl");
        std::fs::write(&outside, "z").unwrap();
        assert_eq!(catalog.resolve(&outside.to_string_lossy()).unwrap().name, "sub");
    }
}
