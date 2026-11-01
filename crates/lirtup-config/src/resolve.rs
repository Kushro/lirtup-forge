//! Layering of defaults, file, mode, profile and command line into a
//! [`Config`] (the order is in the crate docs).

use crate::{Config, ConfigError, ConfigFile, ConfigWarning, Mapping, ModeStore, Profile, Value};

/// Which profile to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileChoice {
    /// Only the top-level options.
    None,
    /// A profile by name (`-p`, the GUI, the daemon); unknown names are an
    /// error.
    Named(String),
    /// The first profile whose `match` fits this window, if any.
    Auto {
        /// Window title.
        title: String,
        /// Window width.
        width: u32,
        /// Window height.
        height: u32,
    },
}

/// What [`resolve`] produced.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The final config.
    pub config: Config,
    /// Profile applied, if any.
    pub profile: Option<String>,
    /// Mode applied, if any.
    pub mode: Option<String>,
    /// Everything ignored or corrected on the way.
    pub warnings: Vec<ConfigWarning>,
}

/// Merge `top` over `base` key by key; `hotkeys` merges one level deeper.
///
/// A `null` in `top` does not override (1.x `apply_overrides` skips `None`):
/// it means "inherit".
pub fn merge_layer(base: &mut Mapping, top: &Mapping) {
    for (key, value) in top {
        if value.is_null() {
            continue;
        }
        if key.as_str() == Some("hotkeys")
            && let (Some(Value::Mapping(base_hk)), Value::Mapping(top_hk)) = (base.get_mut(key), value)
        {
            for (action, binding) in top_hk {
                base_hk.insert(action.clone(), binding.clone());
            }
            continue;
        }
        base.insert(key.clone(), value.clone());
    }
}

/// Build the config of one session.
///
/// `cli` holds the command-line overrides ([`crate::cli::ConfigArgs::overrides`];
/// empty for the GUI and the daemon).
pub fn resolve(
    file: &ConfigFile,
    modes: &ModeStore,
    profile: &ProfileChoice,
    cli: &Mapping,
) -> Result<Resolved, ConfigError> {
    let selected: Option<(String, &Profile)> = match profile {
        ProfileChoice::None => None,
        ProfileChoice::Named(name) => Some(
            find_profile(file, name)
                .map(|(n, p)| (n.to_owned(), p))
                .ok_or_else(|| ConfigError::UnknownProfile(name.clone()))?,
        ),
        ProfileChoice::Auto { title, width, height } => {
            crate::profile::find_matching_profile(&file.profiles, title, *width, *height)
                .map(|(n, p)| (n.to_owned(), p))
        }
    };
    let options = selected.as_ref().map(|(_, p)| &p.options);

    // The mode sits right below the layer that names it, so the loose mode
    // fields of that layer (and of every layer above) win over it.
    let mode_of = |map: &Mapping| {
        map.get("mode")
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
            .map(str::to_owned)
    };
    #[derive(PartialEq)]
    enum Named {
        General,
        Profile,
        Cli,
    }
    let named = if let Some(id) = mode_of(cli) {
        Some((id, Named::Cli))
    } else if let Some(id) = options.and_then(mode_of) {
        Some((id, Named::Profile))
    } else {
        mode_of(&file.general).map(|id| (id, Named::General))
    };
    let mode_layer = match &named {
        Some((id, _)) => {
            let raw = modes
                .raw
                .get(id)
                .ok_or_else(|| ConfigError::UnknownMode(id.clone()))?;
            let mut layer: Mapping = raw
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), Some("name" | "description")))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            layer.insert("mode".into(), Value::String(id.clone()));
            Some(layer)
        }
        None => None,
    };
    let at = |layer: Named| named.as_ref().is_some_and(|(_, n)| *n == layer);

    let mut merged = Mapping::new();
    if at(Named::General) {
        merge_layer(&mut merged, mode_layer.as_ref().unwrap_or(&Mapping::new()));
    }
    merge_layer(&mut merged, &file.general);
    if at(Named::Profile) {
        merge_layer(&mut merged, mode_layer.as_ref().unwrap_or(&Mapping::new()));
    }
    if let Some(options) = options {
        merge_layer(&mut merged, options);
    }
    if at(Named::Cli) {
        merge_layer(&mut merged, mode_layer.as_ref().unwrap_or(&Mapping::new()));
    }
    merge_layer(&mut merged, cli);
    // Whatever layer won, report the mode actually applied.
    match &named {
        Some((id, _)) => merged.insert("mode".into(), Value::String(id.clone())),
        None => merged.remove("mode"),
    };

    let (mut config, mut warnings) = Config::from_mapping(&merged);
    warnings.extend(config.validate());
    config.config_file = Some(file.path.clone()).filter(|p| !p.as_os_str().is_empty());
    for w in &warnings {
        tracing::warn!("config: {}: {}", w.key, w.message);
    }
    Ok(Resolved {
        config,
        profile: selected.map(|(name, _)| name),
        mode: named.map(|(id, _)| id),
        warnings,
    })
}

/// A profile by name: exact first, then ignoring case (`find_profile` of 1.x).
pub fn find_profile<'a>(file: &'a ConfigFile, name: &str) -> Option<(&'a str, &'a Profile)> {
    file.profiles
        .get_key_value(name)
        .map(|(n, p)| (n.as_str(), p))
        .or_else(|| {
            file.profiles
                .iter()
                .find(|(n, _)| n.to_lowercase() == name.to_lowercase())
                .map(|(n, p)| (n.as_str(), p))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mode, Model};

    fn map(yaml: &str) -> Mapping {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    fn file(yaml: &str) -> ConfigFile {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        ConfigFile::load(&path).unwrap().0
    }

    fn modes(entries: &[(&str, &str)]) -> ModeStore {
        let mut store = ModeStore::default();
        for (id, yaml) in entries {
            let raw = map(yaml);
            store.modes.insert(id.to_string(), Mode::from_mapping(id, &raw).0);
            store.raw.insert(id.to_string(), raw);
        }
        store
    }

    const FILE: &str = "
model: faster
cas_enabled: true
hotkeys:
  zoom_in: Ctrl+Plus
profiles:
  Game:
    match: {title_contains: game}
    options:
      model: 4x12
      max_fps: null
      hotkeys: {zoom_out: Ctrl+Minus}
  Anime:
    options:
      mode: anime
      bloom_enabled: true
  AnimeLoose:
    options:
      mode: anime
      model: 8x32
";

    #[test]
    fn merge_hotkeys_one_level_deeper() {
        let mut base = map("a: 1\nhotkeys: {x: A, y: B}\n");
        merge_layer(&mut base, &map("a: 2\nb: null\nhotkeys: {y: C}\n"));
        assert_eq!(base, map("a: 2\nhotkeys: {x: A, y: C}\n"));
    }

    #[test]
    fn layers_in_order() {
        let file = file(FILE);
        let none = resolve(
            &file,
            &ModeStore::default(),
            &ProfileChoice::None,
            &Mapping::new(),
        )
        .unwrap();
        assert_eq!(none.config.scaling.model, Model::Faster);
        assert!(none.profile.is_none());

        let auto = ProfileChoice::Auto {
            title: "A Game".into(),
            width: 800,
            height: 600,
        };
        let r = resolve(&file, &ModeStore::default(), &auto, &Mapping::new()).unwrap();
        assert_eq!(r.profile.as_deref(), Some("Game"));
        assert_eq!(r.config.scaling.model, Model::M4x12);
        assert!(r.config.scaling.cas_enabled);
        assert_eq!(r.config.hotkeys["zoom_in"], "Ctrl+Plus");
        assert_eq!(r.config.hotkeys["zoom_out"], "Ctrl+Minus");
        assert_eq!(r.config.hotkeys["toggle_scaling"], "Alt+Shift+S");

        let cli = map("model: none\n");
        let r = resolve(&file, &ModeStore::default(), &auto, &cli).unwrap();
        assert_eq!(r.config.scaling.model, Model::None);
    }

    #[test]
    fn named_profiles() {
        let file = file(FILE);
        let store = modes(&[("anime", "name: Anime\nmodel: 4x24\ncas_enabled: false\n")]);
        let named = |n: &str| ProfileChoice::Named(n.into());
        assert!(matches!(
            resolve(&file, &store, &named("nope"), &Mapping::new()),
            Err(ConfigError::UnknownProfile(_))
        ));
        let r = resolve(&file, &store, &named("game"), &Mapping::new()).unwrap();
        assert_eq!(r.profile.as_deref(), Some("Game"));
    }

    #[test]
    fn mode_and_loose_fields() {
        let file = file(FILE);
        let store = modes(&[("anime", "name: Anime\nmodel: 4x24\ncas_enabled: false\n")]);
        let named = |n: &str| ProfileChoice::Named(n.into());

        // The mode wins over the top level...
        let r = resolve(&file, &store, &named("Anime"), &Mapping::new()).unwrap();
        assert_eq!(r.mode.as_deref(), Some("anime"));
        assert_eq!(r.config.mode.as_deref(), Some("anime"));
        assert_eq!(r.config.scaling.model, Model::M4x24);
        assert!(!r.config.scaling.cas_enabled);
        assert!(r.config.scaling.bloom_enabled);
        assert!(!r.config.extra.contains_key("name"));

        // ...and loose fields of the profile win over the mode.
        let r = resolve(&file, &store, &named("AnimeLoose"), &Mapping::new()).unwrap();
        assert_eq!(r.config.scaling.model, Model::M8x32);

        // --mode replaces the profile's mode and wins over its loose fields.
        let store2 = modes(&[
            ("anime", "name: Anime\nmodel: 4x24\n"),
            ("crt", "name: CRT\ncrt_enabled: true\nmodel: none\n"),
        ]);
        let r = resolve(&file, &store2, &named("AnimeLoose"), &map("mode: crt\n")).unwrap();
        assert_eq!(r.mode.as_deref(), Some("crt"));
        assert!(r.config.scaling.crt_enabled);
        assert_eq!(r.config.scaling.model, Model::None);

        assert!(matches!(
            resolve(&file, &ModeStore::default(), &named("Anime"), &Mapping::new()),
            Err(ConfigError::UnknownMode(_))
        ));
    }

    #[test]
    fn top_level_mode_loses_to_top_level_loose_fields() {
        let file = file("mode: anime\nmodel: fast\n");
        let store = modes(&[("anime", "name: Anime\nmodel: 4x24\ncas_enabled: true\n")]);
        let r = resolve(&file, &store, &ProfileChoice::None, &Mapping::new()).unwrap();
        assert_eq!(r.config.scaling.model, Model::Fast);
        assert!(r.config.scaling.cas_enabled);
    }
}
