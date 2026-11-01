//! Scaling modes (6.6): `modes/<id>.yaml`, named by profiles with `mode: <id>`.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{ConfigError, ConfigWarning, Mapping, ModeSettings, Value};

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
        let mut store = ModeStore {
            dir: dir.to_path_buf(),
            ..ModeStore::default()
        };
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((store, Vec::new())),
            Err(source) => {
                return Err(ConfigError::Io {
                    path: dir.to_path_buf(),
                    source,
                });
            }
        };
        let mut files: Vec<(String, PathBuf)> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "yaml") && p.is_file())
            .filter_map(|p| Some((p.file_stem()?.to_str()?.to_owned(), p)))
            .filter(|(id, _)| !id.starts_with('.'))
            .collect();
        files.sort();
        let mut warnings = Vec::new();
        for (id, path) in files {
            let raw = match crate::util::read_mapping(&path) {
                Ok(raw) => raw.unwrap_or_default(),
                Err(e) => {
                    warnings.push(ConfigWarning {
                        key: format!("modes.{id}"),
                        message: format!("{e}; skipped"),
                    });
                    continue;
                }
            };
            let (mode, w) = Mode::from_mapping(&id, &raw);
            warnings.extend(w);
            store.modes.insert(id.clone(), mode);
            store.raw.insert(id, raw);
        }
        Ok((store, warnings))
    }

    /// [`ModeStore::load`] on [`crate::paths::modes_dir`].
    pub fn load_default() -> Result<(ModeStore, Vec<crate::ConfigWarning>), ConfigError> {
        Self::load(&crate::paths::modes_dir())
    }

    /// Write one mode (temporary file + rename), keeping its unknown keys.
    pub fn save(&mut self, id: &str, mode: Mode) -> Result<(), ConfigError> {
        let path = self.path(id)?;
        let raw = crate::util::to_mapping(&mode);
        crate::util::write_atomic(&path, &crate::util::to_yaml(&raw, &path)?)?;
        self.modes.insert(id.to_owned(), mode);
        self.raw.insert(id.to_owned(), raw);
        self.modes.sort_keys();
        self.raw.sort_keys();
        Ok(())
    }

    /// Delete a mode file.
    pub fn delete(&mut self, id: &str) -> Result<(), ConfigError> {
        let path = self.path(id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(ConfigError::Io { path, source }),
        }
        self.modes.shift_remove(id);
        self.raw.shift_remove(id);
        Ok(())
    }

    /// A free id derived from a display name (`"Modo de Firefox"` →
    /// `modo-de-firefox`, `-2`, ...).
    pub fn new_id(&self, name: &str) -> String {
        let base = slug(name);
        let taken = |id: &str| self.modes.contains_key(id) || self.dir.join(format!("{id}.yaml")).exists();
        if !taken(&base) {
            return base;
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !taken(id))
            .unwrap_or(base)
    }

    /// A mode by id.
    pub fn get(&self, id: &str) -> Option<&Mode> {
        self.modes.get(id)
    }

    /// `<dir>/<id>.yaml`, refusing ids that would leave the folder.
    fn path(&self, id: &str) -> Result<PathBuf, ConfigError> {
        if id.is_empty() || id.starts_with('.') || id.contains(['/', '\\', '\0']) {
            return Err(ConfigError::Invalid {
                key: "mode".into(),
                message: format!("invalid mode id {id:?}"),
            });
        }
        Ok(self.dir.join(format!("{id}.yaml")))
    }
}

impl Mode {
    /// Read a mode file: `name` (the id when missing), `description`, the
    /// mode keys (bad values keep their defaults, with a warning) and unknown
    /// keys in [`Mode::extra`].
    pub fn from_mapping(id: &str, raw: &Mapping) -> (Mode, Vec<ConfigWarning>) {
        let mut settings_map = Mapping::new();
        let mut extra = Mapping::new();
        let mut name = None;
        let mut description = None;
        for (key, value) in raw {
            match key.as_str() {
                Some("name") => name = Some(value_text(value)),
                Some("description") if !value.is_null() => description = Some(value_text(value)),
                Some("description") => {}
                Some(k) if crate::MODE_KEYS.contains(&k) => {
                    settings_map.insert(key.clone(), value.clone());
                }
                _ => {
                    extra.insert(key.clone(), value.clone());
                }
            }
        }
        let (settings, warnings) =
            crate::util::tolerant::<ModeSettings>(&settings_map, &format!("modes.{id}."));
        let mode = Mode {
            name: name.filter(|n| !n.is_empty()).unwrap_or_else(|| id.to_owned()),
            description,
            settings,
            extra,
        };
        (mode, warnings)
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => crate::util::key_string(other),
    }
}

/// `"Modo de Firefox"` → `modo-de-firefox`; accents folded, anything else
/// that is not a letter or digit becomes `-`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() { "modo".into() } else { out.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Modo de Firefox"), "modo-de-firefox");
        assert_eq!(slug("  Ánime  ñandú!! "), "anime-nandu");
        assert_eq!(slug("***"), "modo");
    }

    #[test]
    fn save_load_delete() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, w) = ModeStore::load(&dir.path().join("modes")).unwrap();
        assert!(w.is_empty() && store.modes.is_empty());
        store.dir = dir.path().join("modes");

        let id = store.new_id("Modo de Firefox");
        assert_eq!(id, "modo-de-firefox");
        let settings = ModeSettings {
            model: crate::Model::M4x24,
            bloom_strength: 0.05,
            ..ModeSettings::default()
        };
        let mode = Mode {
            name: "Modo de Firefox".into(),
            description: None,
            settings,
            extra: Mapping::new(),
        };
        store.save(&id, mode.clone()).unwrap();
        assert_eq!(store.new_id("Modo de Firefox"), "modo-de-firefox-2");

        let text = std::fs::read_to_string(store.dir.join("modo-de-firefox.yaml")).unwrap();
        assert!(text.contains("bloom_strength: 0.05"), "{text}");

        let (loaded, w) = ModeStore::load(&store.dir).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(loaded.modes[&id], mode);
        assert_eq!(loaded.raw[&id]["model"], Value::from("4x24"));

        store.delete(&id).unwrap();
        assert!(store.modes.is_empty());
        assert!(!store.dir.join("modo-de-firefox.yaml").exists());
        assert!(
            store
                .save("../evil", Mode::from_mapping("x", &Mapping::new()).0)
                .is_err()
        );
    }

    #[test]
    fn partial_and_bad_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("anime.yaml"),
            "model: 4x12\ncas_enabled: true\nfuture: 1\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("broken.yaml"), "model: [unclosed\n").unwrap();
        std::fs::write(dir.path().join("bad-value.yaml"), "name: Bad\nblur: high\n").unwrap();
        let (store, w) = ModeStore::load(dir.path()).unwrap();
        assert_eq!(store.modes.keys().collect::<Vec<_>>(), ["anime", "bad-value"]);
        assert_eq!(w.len(), 2, "{w:?}");
        let anime = &store.modes["anime"];
        assert_eq!(anime.name, "anime");
        assert_eq!(anime.settings.model, crate::Model::M4x12);
        assert!(anime.extra.contains_key("future"));
        assert_eq!(store.raw["anime"].len(), 3);
        assert_eq!(store.modes["bad-value"].settings.blur, 1.0);
    }
}
