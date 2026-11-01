//! Criterion of 5.8 for front C: the 1.x files are read without changes, and
//! a save writes back the same options and profiles.

use std::path::{Path, PathBuf};

use lirtup_config::profile::IntervalOp;
use lirtup_config::resolve::{ProfileChoice, resolve};
use lirtup_config::{
    Config, ConfigFile, EffectsEngine, FrameGenMultiplier, LogLevel, MagpieEffectSpec, Mapping, ModeStore,
    Model, Value,
};

fn example_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config-example.yaml")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn without_hotkeys(map: &Mapping) -> Mapping {
    let mut map = map.clone();
    map.remove("hotkeys");
    map
}

/// Every profile resolves to the same config before and after a save.
fn assert_same_resolution(a: &ConfigFile, b: &ConfigFile) {
    let modes = ModeStore::default();
    let none = Mapping::new();
    let choices = std::iter::once(ProfileChoice::None)
        .chain(a.profiles.keys().map(|n| ProfileChoice::Named(n.clone())));
    for choice in choices {
        let mut ra = resolve(a, &modes, &choice, &none).unwrap().config;
        let mut rb = resolve(b, &modes, &choice, &none).unwrap().config;
        ra.config_file = None;
        rb.config_file = None;
        assert_eq!(ra, rb, "{choice:?}");
    }
}

/// Load, save to a copy, load the copy: same options and profiles; a
/// second save writes the same bytes; the original is never touched.
fn round_trip(original: &Path) -> (ConfigFile, ConfigFile) {
    let before = std::fs::read(original).unwrap();
    let (file, _) = ConfigFile::load(original).unwrap();
    assert_eq!(std::fs::read(original).unwrap(), before, "load changed the file");

    let dir = tempfile::tempdir().unwrap();
    let mut copy = file.clone();
    copy.path = dir.path().join("config.yaml");
    copy.save().unwrap();
    let (reloaded, warnings) = ConfigFile::load(&copy.path).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");

    assert_eq!(without_hotkeys(&reloaded.general), without_hotkeys(&file.general));
    assert_eq!(
        reloaded.profiles.keys().collect::<Vec<_>>(),
        file.profiles.keys().collect::<Vec<_>>()
    );
    for (name, profile) in &file.profiles {
        let back = &reloaded.profiles[name];
        assert_eq!(back.matcher, profile.matcher, "{name}");
        assert_eq!(
            without_hotkeys(&back.options),
            without_hotkeys(&profile.options),
            "{name}"
        );
    }
    assert_same_resolution(&file, &reloaded);

    let first = std::fs::read(&copy.path).unwrap();
    reloaded.save().unwrap();
    assert_eq!(
        std::fs::read(&copy.path).unwrap(),
        first,
        "a second save changed the file"
    );
    assert!(copy.path.with_file_name("config.yaml.bak1").is_file());
    assert_eq!(std::fs::read(original).unwrap(), before);
    (file, reloaded)
}

#[test]
fn reads_config_example() {
    let (file, warnings) = ConfigFile::load(&example_path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");

    let (config, warnings) = Config::from_mapping(&file.general);
    assert!(warnings.is_empty(), "{warnings:?}");
    // The example spells pipeline_poll_interval as pipeline_idle_interval;
    // 1.x ignores it too. Nothing else is unknown.
    assert_eq!(
        config.extra.keys().filter_map(Value::as_str).collect::<Vec<_>>(),
        ["pipeline_idle_interval"]
    );
    assert!(config.clone().validate().is_empty());
    assert_eq!(config.pid_timeout, 10.0);
    assert_eq!(config.total_timeout, 30.0);
    assert_eq!(config.log_level, LogLevel::Warning);
    assert_eq!(config.scaling.model, Model::Fast);
    assert_eq!(config.frame_gen_multiplier, FrameGenMultiplier::Fixed(1));
    assert_eq!(config.screenshot_dir, PathBuf::from("~/Pictures/Screenshots"));
    assert_eq!(config.hotkeys, lirtup_config::hotkeys::default_hotkeys());

    let names: Vec<_> = file.profiles.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        ["manual", "vlc_player", "wide_firefox", "emulators", "720p"]
    );
    assert!(file.profiles["manual"].matcher.is_none());
    let p720 = &file.profiles["720p"];
    assert!(p720.daemon_excluded());
    assert_eq!(
        p720.matcher
            .as_ref()
            .unwrap()
            .height
            .as_ref()
            .unwrap()
            .parse()
            .unwrap(),
        IntervalOp::Eq(720.0)
    );

    let pick = |title: &str, w, h| {
        lirtup_config::profile::find_matching_profile(&file.profiles, title, w, h).map(|(n, _)| n.to_owned())
    };
    assert_eq!(pick("VLC media player", 800, 600).as_deref(), Some("vlc_player"));
    assert_eq!(
        pick("Mozilla Firefox", 1920, 1080).as_deref(),
        Some("wide_firefox")
    );
    assert_eq!(pick("Mozilla Firefox", 1280, 1080), None);
    assert_eq!(
        pick("Dolphin 5.0 | Zelda", 640, 480).as_deref(),
        Some("emulators")
    );
    assert_eq!(pick("Some game", 1280, 720).as_deref(), Some("720p"));

    let auto = ProfileChoice::Auto {
        title: "Some game".into(),
        width: 1280,
        height: 720,
    };
    let r = resolve(&file, &ModeStore::default(), &auto, &Mapping::new()).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(r.profile.as_deref(), Some("720p"));
    assert_eq!(r.config.scaling.model, Model::M8x32);
    assert!(r.config.scaling.double_upscale);
    assert!(r.config.daemon_exclude);
}

#[test]
fn config_example_round_trip() {
    let (file, reloaded) = round_trip(&example_path());
    // Every hotkey of the example is a default, so the save drops them.
    assert!(file.general.contains_key("hotkeys"));
    assert!(!reloaded.general.contains_key("hotkeys"));
}

#[test]
fn reads_1x_user_profiles() {
    let (file, warnings) = ConfigFile::load(&fixture("config-1x-user.yaml")).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let (config, warnings) = Config::from_mapping(&file.general);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(config.extra.is_empty());
    assert_eq!(config.scaling.effects_engine, EffectsEngine::Magpiefx);
    assert_eq!(config.scaling.magpie_effects.len(), 3);
    assert!(matches!(
        &config.scaling.magpie_effects[2],
        MagpieEffectSpec::Full { name, params } if name == "Lanczos" && params.is_empty()
    ));
    assert_eq!(config.frame_gen_multiplier, FrameGenMultiplier::Fixed(4));

    let r = resolve(
        &file,
        &ModeStore::default(),
        &ProfileChoice::Auto {
            title: "Genshin Impact".into(),
            width: 1920,
            height: 1080,
        },
        &Mapping::new(),
    )
    .unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(r.profile.as_deref(), Some("Juego"));
    assert_eq!(r.config.scaling.model, Model::M4x24);
    assert_eq!(r.config.background_color.to_hex(), "#202020");
    assert_eq!(r.config.hotkeys["zoom_in"], "Ctrl+Alt+Plus");
    assert_eq!(r.config.scaling.effects_engine, EffectsEngine::Magpiefx);
    assert!(file.profiles["Video"].daemon_excluded());
}

#[test]
fn user_profiles_round_trip() {
    let (file, reloaded) = round_trip(&fixture("config-1x-user.yaml"));
    // A changed hotkey survives the diff.
    let hk = |f: &ConfigFile| f.profiles["Juego"].options["hotkeys"]["zoom_in"].clone();
    assert_eq!(hk(&reloaded), hk(&file));
}

/// The person's own file, when there is one (the criterion of 5.8 names
/// it). Read-only: the copy goes to a temporary folder. Set
/// `LIRTUP_CONFIG_FILE` to test another file.
#[test]
fn reads_the_installed_config_if_any() {
    let path = std::env::var_os("LIRTUP_CONFIG_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(lirtup_config::paths::config_path);
    if !path.is_file() {
        eprintln!("no {}, skipped", path.display());
        return;
    }
    let (file, warnings) = ConfigFile::load(&path).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let (_, warnings) = Config::from_mapping(&file.general);
    assert!(warnings.is_empty(), "{warnings:?}");
    for (name, profile) in &file.profiles {
        let (_, warnings) = Config::from_mapping(&profile.options);
        assert!(warnings.is_empty(), "profile {name}: {warnings:?}");
    }
    round_trip(&path);
}
