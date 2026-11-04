//! Layering of defaults, file, mode, profile and command line into a
//! [`Config`] (the order is in the crate docs).
//!
//! Settings of the retired native effects engine (the keys of
//! [`crate::legacy::LEGACY_KEYS`]) are translated to MagpieFX effects on the
//! merged file layers, before the command line goes on top
//! ([`crate::legacy::translate_native`]); on the command line they are
//! ignored with a warning.

use crate::legacy::{LEGACY_KEYS, translate_native};
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
    /// The mode the layers name, before [`resolve_with_mode`] replaced it
    /// (equal to [`Resolved::mode`] without a replacement).
    pub named_mode: Option<String>,
    /// Everything ignored or corrected on the way.
    pub warnings: Vec<ConfigWarning>,
    /// Keys of the applied mode that loose mode settings in the layer naming
    /// it (or above) override, so the mode does not show: empty when the
    /// mode applies as written (typical after `lirtup migrate`).
    pub shadowed: Vec<String>,
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
    resolve_with_mode(file, modes, profile, cli, None)
}

/// [`resolve`] with another mode in place of the one the layers name (the
/// mode chip of the bar over the game, for this session only).
///
/// The replacement takes the place of the named mode, in the same layer:
/// the loose mode fields of that layer and of the layers above still win
/// over it, so what the session shows is what saving `mode: <id>` in that
/// layer gives. With no mode named, it goes where the profile (or, without
/// one, the top level) would name it.
pub fn resolve_with_mode(
    file: &ConfigFile,
    modes: &ModeStore,
    profile: &ProfileChoice,
    cli: &Mapping,
    mode: Option<&str>,
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
    let named_mode = named.as_ref().map(|(id, _)| id.clone());
    let named = match (mode.filter(|m| !m.is_empty()), named) {
        (Some(id), Some((_, layer))) => Some((id.to_owned(), layer)),
        (Some(id), None) if options.is_some() => Some((id.to_owned(), Named::Profile)),
        (Some(id), None) => Some((id.to_owned(), Named::General)),
        (None, named) => named,
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
    // Legacy keys on the command line are not translated: they would mix
    // the old rule with layers already translated. Dropped with a warning.
    let mut warnings: Vec<ConfigWarning> = Vec::new();
    let cli_dropped: Vec<String> = cli
        .keys()
        .filter_map(Value::as_str)
        .filter(|k| LEGACY_KEYS.contains(k))
        .map(str::to_owned)
        .collect();
    let cli_clean: Mapping;
    let cli: &Mapping = if cli_dropped.is_empty() {
        cli
    } else {
        for key in &cli_dropped {
            warnings.push(ConfigWarning {
                key: key.clone(),
                message: "retired native engine setting, ignored; use --magpie-effect".to_owned(),
            });
        }
        cli_clean = cli
            .iter()
            .filter(|(k, _)| !k.as_str().is_some_and(|k| LEGACY_KEYS.contains(&k)))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        &cli_clean
    };
    // Loose mode keys in the naming layer and above win over the mode's
    // (legacy keys count as mode keys: an unmigrated profile with loose
    // native fields over a mode still shadows it).
    let shadowed: Vec<String> = match (&named, &mode_layer) {
        (Some((_, layer)), Some(mode_layer)) => {
            let mut above: Vec<&Mapping> = Vec::new();
            if *layer == Named::General {
                above.push(&file.general);
            }
            if *layer != Named::Cli
                && let Some(options) = options
            {
                above.push(options);
            }
            above.push(cli);
            mode_layer
                .keys()
                .filter_map(Value::as_str)
                .filter(|k| {
                    *k != "mode" && (crate::model::MODE_KEYS.contains(k) || LEGACY_KEYS.contains(k))
                })
                .filter(|k| above.iter().any(|m| m.get(*k).is_some_and(|v| !v.is_null())))
                .map(str::to_owned)
                .collect()
        }
        _ => Vec::new(),
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
    // The file layers are complete: the native engine becomes MagpieFX
    // effects here, so the command line (`--magpie-effect`) wins over them.
    translate_native(&mut merged, &mut warnings);
    merge_layer(&mut merged, cli);
    // Whatever layer won, report the mode actually applied.
    match &named {
        Some((id, _)) => merged.insert("mode".into(), Value::String(id.clone())),
        None => merged.remove("mode"),
    };

    let (mut config, read_warnings) = Config::from_mapping(&merged);
    warnings.extend(read_warnings);
    warnings.extend(config.validate());
    if let Some((id, layer)) = &named
        && !shadowed.is_empty()
    {
        let layer = match layer {
            Named::General => "the top level",
            Named::Profile => "the profile",
            Named::Cli => "the command line",
        };
        warnings.push(ConfigWarning {
            key: "mode".to_owned(),
            message: format!(
                "mode '{id}' is shadowed by loose mode settings in {layer} ({}); move them into a mode \
                 (lirtup migrate) or remove them",
                shadowed.join(", ")
            ),
        });
    }
    config.config_file = Some(file.path.clone()).filter(|p| !p.as_os_str().is_empty());
    for w in &warnings {
        tracing::warn!("config: {}: {}", w.key, w.message);
    }
    Ok(Resolved {
        config,
        profile: selected.map(|(name, _)| name),
        mode: named.map(|(id, _)| id),
        named_mode,
        warnings,
        shadowed,
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
    use crate::legacy::{CRT_EFFECT, LUMA_SHARPEN_EFFECT};
    use crate::{MagpieEffectSpec, Mode, Model};

    fn map(yaml: &str) -> Mapping {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    fn file(yaml: &str) -> ConfigFile {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        ConfigFile::load(&path).unwrap().0
    }

    /// The chain a 2.1 mode with `crt_enabled: true` (and nothing else of
    /// the native engine) translates to.
    fn crt() -> Vec<MagpieEffectSpec> {
        vec![MagpieEffectSpec::Short(CRT_EFFECT.into())]
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
        assert_eq!(r.config.scaling.magpie_effects, crt());
        assert_eq!(r.config.scaling.model, Model::None);

        assert!(matches!(
            resolve(&file, &ModeStore::default(), &named("Anime"), &Mapping::new()),
            Err(ConfigError::UnknownMode(_))
        ));
    }

    #[test]
    fn a_replaced_mode_keeps_the_layer() {
        let file = file(FILE);
        let store = modes(&[
            ("anime", "name: Anime\nmodel: 4x24\n"),
            ("crt", "name: CRT\ncrt_enabled: true\nmodel: none\n"),
        ]);
        let named = |n: &str| ProfileChoice::Named(n.into());
        let with = |p: &ProfileChoice, m| resolve_with_mode(&file, &store, p, &Mapping::new(), m).unwrap();

        // The profile's loose fields still win over the replacement, as they
        // would after saving `mode: crt` in the profile.
        let r = with(&named("AnimeLoose"), Some("crt"));
        assert_eq!(r.mode.as_deref(), Some("crt"));
        assert_eq!(r.named_mode.as_deref(), Some("anime"));
        assert_eq!(r.config.mode.as_deref(), Some("crt"));
        assert_eq!(r.config.scaling.magpie_effects, crt());
        assert_eq!(r.config.scaling.model, Model::M8x32);

        // A profile that names no mode: the replacement goes in its layer,
        // so the profile's loose fields win and the top level's lose.
        let r = with(&named("Game"), Some("crt"));
        assert_eq!(r.named_mode, None);
        assert_eq!(r.config.scaling.model, Model::M4x12);
        assert_eq!(r.config.scaling.magpie_effects, crt());

        // No profile: where the top level would name it (its loose fields win).
        let r = with(&ProfileChoice::None, Some("crt"));
        assert_eq!(r.config.scaling.model, Model::Faster);
        assert_eq!(r.config.scaling.magpie_effects, crt());

        // None, or an empty id, changes nothing.
        let r = with(&named("Anime"), None);
        assert_eq!(r.mode, r.named_mode);
        assert_eq!(with(&named("Anime"), Some("")).mode.as_deref(), Some("anime"));

        assert!(matches!(
            resolve_with_mode(&file, &store, &named("Anime"), &Mapping::new(), Some("nope")),
            Err(ConfigError::UnknownMode(_))
        ));
    }

    #[test]
    fn loose_settings_that_shadow_the_mode_are_reported() {
        let file = file(FILE);
        let store = modes(&[
            ("anime", "name: Anime\nmodel: 4x24\n"),
            ("crt", "name: CRT\ncrt_enabled: true\nmodel: none\n"),
        ]);
        let named = |n: &str| ProfileChoice::Named(n.into());
        let with = |p: &ProfileChoice, m| resolve_with_mode(&file, &store, p, &Mapping::new(), m).unwrap();

        // The top level sets `model` and names no mode: a replacement there
        // loses its `model` to the loose key, and says so.
        let r = with(&ProfileChoice::None, Some("crt"));
        assert_eq!(r.shadowed, vec!["model".to_owned()]);
        assert!(
            r.warnings
                .iter()
                .any(|w| w.key == "mode" && w.message.contains("top level"))
        );

        // The profile's loose `model` shadows the mode it names.
        let r = with(&named("AnimeLoose"), None);
        assert_eq!(r.shadowed, vec!["model".to_owned()]);

        // A mode whose keys nobody sets loose applies as written.
        let r = with(&named("Anime"), None);
        assert!(r.shadowed.is_empty());
        assert!(!r.warnings.iter().any(|w| w.key == "mode"));

        // Null means inherit, so it shadows nothing.
        let r = with(&named("Game"), Some("anime"));
        assert_eq!(r.shadowed, vec!["model".to_owned()]);
        let r = with(&named("Game"), Some("crt"));
        assert_eq!(r.shadowed, vec!["model".to_owned()]);
    }

    #[test]
    fn top_level_mode_loses_to_top_level_loose_fields() {
        let file = file("mode: anime\nmodel: fast\n");
        let store = modes(&[("anime", "name: Anime\nmodel: 4x24\ncas_enabled: true\n")]);
        let r = resolve(&file, &store, &ProfileChoice::None, &Mapping::new()).unwrap();
        assert_eq!(r.config.scaling.model, Model::Fast);
        assert!(r.config.scaling.cas_enabled);
    }

    #[test]
    fn native_settings_of_the_files_are_translated() {
        // A 2.1 mode with the CRT on, and a profile with a loose CRT field
        // over it (an unmigrated profile): one CRT effect with the loose
        // value, reported as shadowing nothing of the mode.
        let file = file(
            "profiles:\n  Retro:\n    options:\n      mode: crt\n      crt_sharpness_h: 0.7\n\
             \x20 Plain:\n    options:\n      mode: crt\n      crt_enabled: false\n",
        );
        let store = modes(&[("crt", "name: CRT\ncrt_enabled: true\nmodel: none\n")]);
        let named = |n: &str| ProfileChoice::Named(n.into());
        let r = resolve(&file, &store, &named("Retro"), &Mapping::new()).unwrap();
        let mut params = std::collections::BTreeMap::new();
        params.insert("sharpnessH".to_owned(), 0.7);
        assert_eq!(
            r.config.scaling.magpie_effects,
            vec![MagpieEffectSpec::Full {
                name: CRT_EFFECT.into(),
                params
            }]
        );
        assert!(r.shadowed.is_empty(), "{:?}", r.shadowed);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert!(r.config.extra.is_empty(), "{:?}", r.config.extra);

        // A loose native field over the same field of the mode shadows it.
        let r = resolve(&file, &store, &named("Plain"), &Mapping::new()).unwrap();
        assert_eq!(r.shadowed, vec!["crt_enabled".to_owned()]);
        assert!(r.config.scaling.magpie_effects.is_empty());

        // `--magpie-effect` goes on top of the translated list.
        let cli = map("magpie_effects: [CAS]\n");
        let r = resolve(&file, &store, &named("Retro"), &cli).unwrap();
        assert_eq!(
            r.config.scaling.magpie_effects,
            vec![MagpieEffectSpec::Short("CAS".into())]
        );
    }

    #[test]
    fn native_settings_on_the_command_line_are_ignored() {
        let file = file("model: fast\n");
        let cli = map("crt_enabled: true\neffects_engine: native\nmodel: none\n");
        let r = resolve(&file, &ModeStore::default(), &ProfileChoice::None, &cli).unwrap();
        assert!(r.config.scaling.magpie_effects.is_empty());
        assert_eq!(r.config.scaling.model, Model::None);
        let warned: Vec<&str> = r.warnings.iter().map(|w| w.key.as_str()).collect();
        assert_eq!(warned, ["crt_enabled", "effects_engine"]);
        assert!(r.warnings[0].message.contains("--magpie-effect"));
        assert!(r.config.extra.is_empty(), "{:?}", r.config.extra);
    }

    #[test]
    fn the_magpiefx_engine_of_a_profile_keeps_the_mode_chain() {
        let file = file("profiles:\n  P:\n    options:\n      mode: m\n      effects_engine: magpiefx\n");
        let store = modes(&[(
            "m",
            "name: M\nluma_sharpen_enabled: true\nmagpie_effects: [CAS]\n\
             magpie_effects_before_model: [Bicubic]\n",
        )]);
        let r = resolve(&file, &store, &ProfileChoice::Named("P".into()), &Mapping::new()).unwrap();
        assert_eq!(
            r.config.scaling.magpie_effects,
            vec![MagpieEffectSpec::Short("CAS".into())]
        );
        assert_eq!(
            r.config.scaling.magpie_effects_before_model,
            vec![MagpieEffectSpec::Short("Bicubic".into())]
        );
        // Without the profile the mode runs its native LumaSharpen.
        let r = resolve_with_mode(&file, &store, &ProfileChoice::None, &Mapping::new(), Some("m")).unwrap();
        assert_eq!(
            r.config.scaling.magpie_effects,
            vec![MagpieEffectSpec::Short(LUMA_SHARPEN_EFFECT.into())]
        );
    }
}
