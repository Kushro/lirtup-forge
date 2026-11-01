//! `config.yaml`: top-level options plus `profiles:`, read and written
//! without losing what this version does not know.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::{BackgroundColor, ConfigError, ConfigWarning, Mapping, Profile, Value};

/// Backups kept by [`ConfigFile::save`] (`config.yaml.bak1` is the newest),
/// as `DEFAULT_MAX_BACKUPS` of 1.x.
pub const MAX_BACKUPS: u32 = 5;

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
        let mut file = ConfigFile {
            path: path.to_path_buf(),
            ..ConfigFile::default()
        };
        let Some(mut map) = crate::util::read_mapping(path)? else {
            tracing::debug!("no config file at {}", path.display());
            return Ok((file, Vec::new()));
        };
        let mut warnings = Vec::new();
        match map.remove("profiles") {
            None | Some(Value::Null) => {}
            Some(Value::Mapping(profiles)) => {
                for (name, value) in &profiles {
                    let name = crate::util::key_string(name);
                    let (profile, w) = Profile::from_value(&name, value);
                    warnings.extend(w);
                    file.profiles.insert(name, profile);
                }
            }
            Some(other) => {
                warnings.push(ConfigWarning {
                    key: "profiles".into(),
                    message: format!(
                        "expected a mapping, found {}; no profiles loaded",
                        crate::util::display(&other)
                    ),
                });
            }
        }
        file.general = map;
        tracing::debug!("loaded {}", path.display());
        Ok((file, warnings))
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
        let text = self.to_yaml()?;
        rotate_backups(&self.path);
        crate::util::write_atomic(&self.path, &text)
    }

    /// The YAML [`ConfigFile::save`] writes.
    pub fn to_yaml(&self) -> Result<String, ConfigError> {
        let mut top = normalize_options(&self.general);
        if !self.profiles.is_empty() {
            let mut profiles = Mapping::new();
            for (name, profile) in &self.profiles {
                let mut profile = profile.clone();
                profile.options = normalize_options(&profile.options);
                let value = serde_yaml_ng::to_value(&profile).map_err(|e| ConfigError::Yaml {
                    path: self.path.clone(),
                    message: e.to_string(),
                })?;
                profiles.insert(Value::String(name.clone()), value);
            }
            top.insert("profiles".into(), Value::Mapping(profiles));
        }
        crate::util::to_yaml(&top, &self.path)
    }

    /// A profile by name.
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }
}

/// One layer of options as 1.x saves it: `hotkeys` only with what differs
/// from the defaults (dropped when nothing does), a `background_color` tuple
/// as hex.
fn normalize_options(options: &Mapping) -> Mapping {
    let mut out = options.clone();
    if let Some(Value::Mapping(hotkeys)) = out.get("hotkeys") {
        let defaults = crate::hotkeys::default_hotkeys();
        let diff: Mapping = hotkeys
            .iter()
            .filter(|(k, v)| {
                let default = k.as_str().and_then(|k| defaults.get(k));
                v.as_str() != default.map(String::as_str) || default.is_none()
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if diff.is_empty() {
            out.remove("hotkeys");
        } else {
            out.insert("hotkeys".into(), Value::Mapping(diff));
        }
    }
    if let Some(color @ Value::Sequence(_)) = out.get("background_color")
        && let Ok(color @ BackgroundColor::Bgra(_)) =
            serde_yaml_ng::from_value::<BackgroundColor>(color.clone())
    {
        out.insert("background_color".into(), Value::String(color.to_hex()));
    }
    out
}

/// `config.yaml.bak{N}`.
fn backup_path(path: &Path, n: u32) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".bak{n}"));
    PathBuf::from(name)
}

/// Shift `config.yaml.bak1..` up by one (dropping the oldest) and copy the
/// current file to `.bak1`. Failures only log: a backup never blocks a save.
fn rotate_backups(path: &Path) {
    if MAX_BACKUPS == 0 || !path.is_file() {
        return;
    }
    let _ = std::fs::remove_file(backup_path(path, MAX_BACKUPS));
    for n in (1..MAX_BACKUPS).rev() {
        let src = backup_path(path, n);
        if src.is_file()
            && let Err(e) = std::fs::rename(&src, backup_path(path, n + 1))
        {
            tracing::debug!("backup rotation of {}: {e}", src.display());
        }
    }
    if let Err(e) = std::fs::copy(path, backup_path(path, 1)) {
        tracing::warn!("cannot back up {}: {e}", path.display());
    }
}
