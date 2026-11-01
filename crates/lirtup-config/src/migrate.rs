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
//!   profiles with the same combination share one, and a combination equal
//!   to an existing mode names that mode instead of copying it;
//! - the top level and each profile then name their mode (`mode: <id>`) and
//!   lose the loose fields; a profile whose combination is the top level's
//!   names nothing (it inherits);
//! - `config.yaml` is copied first to `config.yaml.1x.bak`, and the mode
//!   files are written before `config.yaml`, so a failure leaves the 1.x
//!   file in place;
//! - resolving any window gives the same `Config` before and after (the
//!   criterion of 5.8, checked with the person's real configuration).
//!
//! A combination is what the resolver layers, so it depends on where the
//! layer's mode sits ([`crate::resolve::resolve`]): a profile that names a
//! mode gets the top level's loose fields, then that mode, then its own; a
//! profile that names none gets the top level's mode, then the top level's
//! loose fields, then its own.
//!
//! Keys of a mode file that are not mode fields (2.0 never writes them)
//! move with the mode when the new mode takes its place (the top level's,
//! or a profile's); a profile that inherited them and now names its own
//! mode gets those the top level does not set.
//!
//! [`apply`] also brings the 1.x GUI settings that 2.0 has a place for
//! ([`legacy_gui`]): the tray switches of the QSettings file
//! (`~/.config/linux-rt-upscaler.conf`) and the named palette as the seed
//! color.
//!
//! The GUI offers it on the first run with mode fields to move
//! (`Dialog::Migrate`); `lirtup migrate` does it from the command line.

use std::path::{Path, PathBuf};

use crate::{ConfigError, ConfigFile, GuiConfig, Mapping, ModeStore, Value};

/// The suffix of the backup of `config.yaml`.
pub const BACKUP_SUFFIX: &str = ".1x.bak";

/// Display name of the top level's mode.
pub const DEFAULT_MODE_NAME: &str = "Predeterminado";

/// The 1.x QSettings file, next to the config folder
/// (`QSettings("linux-rt-upscaler")`).
pub const QSETTINGS_FILE: &str = "linux-rt-upscaler.conf";

/// The named palettes of 1.x (`gui/config/palette.py`, `PRESETS`) and the
/// seed each becomes: its accent (`control`). `Auto`, `Light` and `Dark`
/// were the defaults and keep the default seed, so they are not here. The
/// same seeds as `lirtup-ui`'s `theme::PALETTES`, under the 1.x names.
pub const PALETTE_SEEDS: [(&str, &str); 22] = [
    ("Ayu Light", "#FA8D3E"),
    ("Ayu Mirage", "#FFCC66"),
    ("Catppuccin", "#CBA6F7"),
    ("Cobalt2", "#FFC600"),
    ("Cyberpunk", "#FF00FF"),
    ("Dracula", "#BD93F9"),
    ("Everforest Dark", "#A7C080"),
    ("GitHub Dark", "#58A6FF"),
    ("Gruvbox Dark", "#83A598"),
    ("Gruvbox Light", "#83A598"),
    ("High Contrast", "#FFFF00"),
    ("Kanagawa", "#7FB4CA"),
    ("Material Palenight", "#C792EA"),
    ("Monokai", "#FC9867"),
    ("Nord", "#88C0D0"),
    ("One Dark", "#61AFEF"),
    ("One Light", "#0184BC"),
    ("Rose Pine", "#C4A7E7"),
    ("Solarized Dark", "#268BD2"),
    ("Solarized Light", "#268BD2"),
    ("Synthwave 84", "#FF7EDB"),
    ("Tokyo Night", "#7AA2F7"),
];

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
    loose_count(&file.general) > 0 || file.profiles.values().any(|p| loose_count(&p.options) > 0)
}

/// The plan for `file`, with ids that do not clash with `modes`.
pub fn plan(file: &ConfigFile, modes: &ModeStore) -> MigrationPlan {
    if !needs_migration(file) {
        return MigrationPlan::default();
    }
    let mut plan = MigrationPlan {
        moved_fields: loose_count(&file.general)
            + file
                .profiles
                .values()
                .map(|p| loose_count(&p.options))
                .sum::<usize>(),
        ..MigrationPlan::default()
    };
    // Ids handed out so far: `new_id` sees them as taken.
    let mut scratch = modes.clone();
    let mut new_modes: Vec<(String, Mapping)> = Vec::new();
    let mut pick = |content: Mapping, name: &str, new_modes: &mut Vec<(String, Mapping)>| {
        if let Some((id, _)) = modes.raw.iter().find(|(_, raw)| strip_names(raw) == content) {
            return id.clone();
        }
        if let Some((id, _)) = new_modes.iter().find(|(_, raw)| strip_names(raw) == content) {
            return id.clone();
        }
        let id = scratch.new_id(name);
        scratch
            .modes
            .insert(id.clone(), crate::Mode::from_mapping(&id, &Mapping::new()).0);
        let mut raw = Mapping::new();
        raw.insert("name".into(), Value::String(name.to_owned()));
        raw.extend(content);
        new_modes.push((id.clone(), raw));
        id
    };

    // The top level: its mode, then its loose fields.
    let top_named = named(&file.general);
    let top_raw = top_named.and_then(|id| modes.raw.get(id)).map(strip_names);
    let top_extra = top_raw.as_ref().map(non_mode).unwrap_or_default();
    let mut top = Mapping::new();
    if let Some(raw) = &top_raw {
        crate::resolve::merge_layer(&mut top, &mode_fields(raw));
    }
    crate::resolve::merge_layer(&mut top, &mode_fields(&file.general));
    let top = ordered(top);
    let top_content = with_extra(top.clone(), &top_extra);
    if !top_content.is_empty() {
        plan.top_mode = Some(pick(top_content, DEFAULT_MODE_NAME, &mut new_modes));
    }
    let top_loose = loose_count(&file.general) > 0;

    for (name, profile) in &file.profiles {
        let own = mode_fields(&profile.options);
        let names = named(&profile.options);
        if own.is_empty() && (names.is_none() || !top_loose) {
            // Inherits the top level's mode, or names a mode that the top
            // level's loose fields never reached: same result untouched.
            continue;
        }
        let target = match names {
            Some(id) => {
                let raw = modes.raw.get(id).map(strip_names).unwrap_or_default();
                let mut combo = mode_fields(&file.general);
                crate::resolve::merge_layer(&mut combo, &mode_fields(&raw));
                crate::resolve::merge_layer(&mut combo, &own);
                Some(with_extra(ordered(combo), &non_mode(&raw)))
            }
            None => {
                let mut combo = top.clone();
                crate::resolve::merge_layer(&mut combo, &own);
                let combo = ordered(combo);
                if combo == top {
                    None
                } else {
                    // The top level's mode sat below the top level; this
                    // mode sits above it, so only what the top level does
                    // not set keeps its effect.
                    let inherited: Mapping = top_extra
                        .iter()
                        .filter(|(k, _)| file.general.get(*k).is_none_or(Value::is_null))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    Some(with_extra(combo, &inherited))
                }
            }
        };
        let id = target.map(|content| pick(content, &format!("Modo de {name}"), &mut new_modes));
        plan.profiles.push((name.clone(), id));
    }
    plan.modes = new_modes;
    plan
}

/// Apply `plan`: back up `config.yaml`, write the modes, then the new
/// `config.yaml`. Returns the backup's path.
///
/// Then the 1.x GUI settings ([`legacy_gui`]) go to `gui-config.yaml` next
/// to `config.yaml`; a failure there only logs.
pub fn apply(
    file: &mut ConfigFile,
    modes: &mut ModeStore,
    plan: &MigrationPlan,
) -> Result<PathBuf, ConfigError> {
    let backup = backup(&file.path)?;
    for (id, raw) in &plan.modes {
        modes.save_raw(id, raw.clone())?;
    }
    *file = migrated(file, plan);
    file.save()?;
    tracing::info!(
        "migrated {} to modes ({} fields, {} new modes); backup in {}",
        file.path.display(),
        plan.moved_fields,
        plan.modes.len(),
        backup.display()
    );
    if let Some(dir) = file.path.parent()
        && let Err(e) = apply_legacy_gui(dir)
    {
        tracing::warn!("1.x GUI settings: {e}");
    }
    Ok(backup)
}

/// `file` after `plan`, in memory: the top level and the profiles of the
/// plan lose their loose mode fields and name their mode. A profile the
/// plan names that is no longer in the file is skipped.
pub fn migrated(file: &ConfigFile, plan: &MigrationPlan) -> ConfigFile {
    let mut out = file.clone();
    out.general = rename_layer(&file.general, plan.top_mode.as_deref(), plan.top_mode.is_some());
    for (name, mode) in &plan.profiles {
        if let Some(profile) = out.profiles.get_mut(name) {
            profile.options = rename_layer(&profile.options, mode.as_deref(), true);
        }
    }
    out
}

/// The 1.x GUI settings 2.0 has a place for, over `gui`; `None` when they
/// change nothing.
///
/// - `qsettings` (the text of `~/.config/linux-rt-upscaler.conf`): the
///   `[tray]` switches (`enabled` → `show_tray_icon`, `close_to_tray`,
///   `minimize_to_tray`, `start_hidden`, `keep_running_on_exit` →
///   `keep_running_after_exit`), only those written in the file;
/// - `palette_preset` of `gui-config.yaml`, kept in [`GuiConfig::extra`]:
///   a named palette becomes [`GuiConfig::seed_color`] when the seed is
///   still the default.
///
/// The window geometry and the position of the overlay in windowed mode
/// stay in the QSettings file: 2.0 does not keep them yet.
pub fn legacy_gui(gui: &GuiConfig, qsettings: Option<&str>) -> Option<GuiConfig> {
    let mut out = gui.clone();
    if let Some(text) = qsettings {
        let tray = ini_section(text, "tray");
        let flag = |key: &str| {
            tray.iter().find(|(k, _)| k == key).and_then(|(_, v)| {
                match v.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" => Some(true),
                    "false" | "0" => Some(false),
                    _ => None,
                }
            })
        };
        if let Some(on) = flag("enabled") {
            out.show_tray_icon = on;
        }
        if let Some(on) = flag("close_to_tray") {
            out.close_to_tray = on;
        }
        if let Some(on) = flag("minimize_to_tray") {
            out.minimize_to_tray = on;
        }
        if let Some(on) = flag("start_hidden") {
            out.start_hidden = on;
        }
        if let Some(on) = flag("keep_running_on_exit") {
            out.keep_running_after_exit = on;
        }
    }
    let preset = gui.extra.get("palette_preset").and_then(Value::as_str);
    if let Some(seed) = preset.and_then(|p| PALETTE_SEEDS.iter().find(|(name, _)| *name == p))
        && gui
            .seed_color
            .eq_ignore_ascii_case(&GuiConfig::default().seed_color)
    {
        out.seed_color = seed.1.to_owned();
    }
    (out != *gui).then_some(out)
}

/// [`legacy_gui`] on the files of `config_dir`: `gui-config.yaml` in it, the
/// QSettings file next to it. Writes `gui-config.yaml` only when something
/// changes; returns whether it did.
pub fn apply_legacy_gui(config_dir: &Path) -> Result<bool, ConfigError> {
    let gui_path = config_dir.join("gui-config.yaml");
    let gui = GuiConfig::load(Some(&gui_path))?;
    let qsettings = config_dir
        .parent()
        .map(|d| d.join(QSETTINGS_FILE))
        .and_then(|p| std::fs::read_to_string(p).ok());
    match legacy_gui(&gui, qsettings.as_deref()) {
        Some(new) => {
            new.save(Some(&gui_path))?;
            tracing::info!("1.x GUI settings moved to {}", gui_path.display());
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Copy `path` to the first free of `<path>.1x.bak`, `.1x.bak2`, ... (an
/// earlier backup is the real 1.x file: never overwritten). A missing file
/// still gets the path (nothing to copy).
fn backup(path: &Path) -> Result<PathBuf, ConfigError> {
    let with = |suffix: String| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    let target = std::iter::once(with(BACKUP_SUFFIX.to_owned()))
        .chain((2..).map(|n| with(format!("{BACKUP_SUFFIX}{n}"))))
        .find(|p| !p.exists())
        .unwrap_or_else(|| with(BACKUP_SUFFIX.to_owned()));
    if path.is_file() {
        std::fs::copy(path, &target).map_err(|source| ConfigError::Io {
            path: target.clone(),
            source,
        })?;
    }
    Ok(target)
}

/// A layer without its loose mode fields, naming `mode` where the first of
/// them (or its old `mode:`) was. `set` false keeps the old `mode:`.
fn rename_layer(layer: &Mapping, mode: Option<&str>, set: bool) -> Mapping {
    let mut out = Mapping::new();
    let mut placed = false;
    for (key, value) in layer {
        let k = key.as_str().unwrap_or_default();
        let is_mode_field = crate::MODE_KEYS.contains(&k);
        if !set {
            if !is_mode_field {
                out.insert(key.clone(), value.clone());
            }
            continue;
        }
        if is_mode_field || k == "mode" {
            if !placed {
                placed = true;
                if let Some(id) = mode {
                    out.insert("mode".into(), Value::String(id.to_owned()));
                }
            }
            continue;
        }
        out.insert(key.clone(), value.clone());
    }
    if set
        && !placed
        && let Some(id) = mode
    {
        out.insert("mode".into(), Value::String(id.to_owned()));
    }
    out
}

/// The mode a layer names (`mode:`), if any.
fn named(layer: &Mapping) -> Option<&str> {
    layer
        .get("mode")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
}

/// The non-null mode fields of a layer.
fn mode_fields(layer: &Mapping) -> Mapping {
    layer
        .iter()
        .filter(|(k, v)| !v.is_null() && k.as_str().is_some_and(|k| crate::MODE_KEYS.contains(&k)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// How many non-null mode fields a layer sets.
fn loose_count(layer: &Mapping) -> usize {
    mode_fields(layer).len()
}

/// The keys of a mode file that are not mode fields (`name` and
/// `description` already out).
fn non_mode(raw: &Mapping) -> Mapping {
    raw.iter()
        .filter(|(k, v)| !v.is_null() && !k.as_str().is_some_and(|k| crate::MODE_KEYS.contains(&k)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// A mode file as the resolver layers it: without `name` and `description`.
fn strip_names(raw: &Mapping) -> Mapping {
    raw.iter()
        .filter(|(k, _)| !matches!(k.as_str(), Some("name" | "description")))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Mode fields in [`crate::MODE_KEYS`] order (the order 2.0 writes).
fn ordered(fields: Mapping) -> Mapping {
    crate::MODE_KEYS
        .iter()
        .filter_map(|k| fields.get(*k).map(|v| (Value::from(*k), v.clone())))
        .collect()
}

/// `fields` plus `extra` after them.
fn with_extra(mut fields: Mapping, extra: &Mapping) -> Mapping {
    for (k, v) in extra {
        fields.insert(k.clone(), v.clone());
    }
    fields
}

/// The `key=value` lines of one `[section]` of an INI file (QSettings).
fn ini_section(text: &str, section: &str) -> Vec<(String, String)> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name.eq_ignore_ascii_case(section);
        } else if inside && let Some((k, v)) = line.split_once('=') {
            out.push((k.trim().to_owned(), v.trim().to_owned()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::{ProfileChoice, resolve};

    /// A config folder in a temporary `XDG_CONFIG_HOME`-like layout:
    /// `<tmp>/linux-rt-upscaler/{config.yaml,modes/}`.
    struct Dir {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
    }

    impl Dir {
        fn new(config: &str, modes: &[(&str, &str)]) -> Dir {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join(crate::paths::APP_DIR);
            std::fs::create_dir_all(dir.join("modes")).unwrap();
            std::fs::write(dir.join("config.yaml"), config).unwrap();
            for (id, yaml) in modes {
                std::fs::write(dir.join("modes").join(format!("{id}.yaml")), yaml).unwrap();
            }
            Dir { _tmp: tmp, dir }
        }

        fn load(&self) -> (ConfigFile, ModeStore) {
            let file = ConfigFile::load(&self.dir.join("config.yaml")).unwrap().0;
            let modes = ModeStore::load(&self.dir.join("modes")).unwrap().0;
            (file, modes)
        }
    }

    /// Every choice that can resolve differently: the top level and each
    /// profile by name. The applied mode is left out of the comparison
    /// (the point is that it changes).
    fn resolutions(file: &ConfigFile, modes: &ModeStore) -> Vec<(String, crate::Config)> {
        std::iter::once(ProfileChoice::None)
            .chain(file.profiles.keys().map(|n| ProfileChoice::Named(n.clone())))
            .map(|choice| {
                let mut config = resolve(file, modes, &choice, &Mapping::new()).unwrap().config;
                config.mode = None;
                (format!("{choice:?}"), config)
            })
            .collect()
    }

    /// Migrate the folder and check: same `Config` everywhere, no loose
    /// mode fields left, the backup is the old file, and migrating again
    /// does nothing.
    fn migrate_and_check(dir: &Dir) -> (MigrationPlan, ConfigFile, ModeStore) {
        let (mut file, mut modes) = dir.load();
        let before = resolutions(&file, &modes);
        let original = std::fs::read_to_string(&file.path).unwrap();
        assert!(needs_migration(&file));
        let plan = plan(&file, &modes);
        assert!(!plan.is_empty());
        let backup = apply(&mut file, &mut modes, &plan).unwrap();
        assert_eq!(backup, dir.dir.join("config.yaml.1x.bak"));
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);

        let (file, modes) = dir.load();
        assert!(!needs_migration(&file), "{:?}", file.general);
        assert!(super::plan(&file, &modes).is_empty());
        let after = resolutions(&file, &modes);
        for ((choice, a), (_, b)) in before.iter().zip(&after) {
            assert_eq!(a, b, "{choice} resolves differently after migrating");
        }
        (plan, file, modes)
    }

    /// The person's `config.yaml` on 2026-11-01 (1.x, no profiles).
    const REAL: &str = "\
effects_engine: magpiefx
magpie_effects:
- name: Anime4K_Upscale_Denoise_UL
  params: {}
- name: Anime4K_Restore_Soft_UL
  params: {}
- name: Lanczos
  params: {}
auto_crop_menu_bar: true
show_stats: true
frame_gen_multiplier: 4
";

    #[test]
    fn real_config() {
        let dir = Dir::new(REAL, &[]);
        let (plan, file, modes) = migrate_and_check(&dir);
        assert_eq!(plan.moved_fields, 2);
        assert_eq!(plan.top_mode.as_deref(), Some("predeterminado"));
        assert!(plan.profiles.is_empty());
        let mode = &modes.modes["predeterminado"];
        assert_eq!(mode.name, "Predeterminado");
        assert_eq!(mode.settings.magpie_effects.len(), 3);
        assert_eq!(
            file.general.keys().filter_map(Value::as_str).collect::<Vec<_>>(),
            ["mode", "auto_crop_menu_bar", "show_stats", "frame_gen_multiplier"]
        );
    }

    #[test]
    fn user_fixture_with_profiles() {
        let dir = Dir::new(include_str!("../tests/fixtures/config-1x-user.yaml"), &[]);
        let (plan, file, _) = migrate_and_check(&dir);
        assert_eq!(plan.top_mode.as_deref(), Some("predeterminado"));
        assert_eq!(plan.profiles.len(), 3);
        assert_eq!(plan.modes.len(), 4);
        assert_eq!(
            file.profiles["Juego"].options["mode"],
            Value::from("modo-de-juego")
        );
        assert_eq!(
            file.profiles["Juego"].options["output_geometry"],
            Value::from("cover")
        );
    }

    /// `config-example.yaml`: every field at the top level, and its profiles.
    #[test]
    fn config_example() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config-example.yaml");
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let dir = Dir::new(&text, &[]);
        let (plan, _, _) = migrate_and_check(&dir);
        assert_eq!(plan.top_mode.as_deref(), Some("predeterminado"));
    }

    /// `LIRTUP_MIGRATE_REAL=<config dir>`: migrate a copy of that folder
    /// (`config.yaml` and `modes/`) and compare. Never writes to it.
    #[test]
    fn real_config_from_env() {
        let Some(src) = std::env::var_os("LIRTUP_MIGRATE_REAL") else {
            return;
        };
        let src = PathBuf::from(src);
        let config = std::fs::read_to_string(src.join("config.yaml")).unwrap();
        let mut mode_files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(src.join("modes")) {
            for e in entries.filter_map(Result::ok) {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "yaml") {
                    let id = p.file_stem().unwrap().to_string_lossy().into_owned();
                    mode_files.push((id, std::fs::read_to_string(&p).unwrap()));
                }
            }
        }
        let refs: Vec<(&str, &str)> = mode_files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let dir = Dir::new(&config, &refs);
        let (plan, _, _) = migrate_and_check(&dir);
        eprintln!("{plan:#?}");
    }

    const PROFILES: &str = "
model: fast
cas_enabled: true
show_stats: true
hotkeys:
  zoom_in: Ctrl+Plus
profiles:
  Same:
    match: {title_contains: same}
    options:
      model: fast
      max_fps: 30
  Inherits:
    options:
      max_fps: 60
  Anime:
    options:
      model: 4x24
      bloom_enabled: true
  AnimeToo:
    icon: anime.png
    options:
      bloom_enabled: true
      model: 4x24
  Nulls:
    options:
      model: null
      scale_mode: fit
  NamesCrt:
    options:
      mode: crt
  NamesCrtLoose:
    options:
      mode: crt
      grain_enabled: true
";

    #[test]
    fn profiles() {
        let dir = Dir::new(
            PROFILES,
            &[("crt", "name: CRT\ncrt_enabled: true\nmodel: none\n")],
        );
        let (plan, file, modes) = migrate_and_check(&dir);
        assert_eq!(plan.moved_fields, 2 + 1 + 2 + 2 + 1);
        assert_eq!(plan.top_mode.as_deref(), Some("predeterminado"));
        let profiles: Vec<(&str, Option<&str>)> = plan
            .profiles
            .iter()
            .map(|(n, m)| (n.as_str(), m.as_deref()))
            .collect();
        assert_eq!(
            profiles,
            [
                ("Same", None),
                ("Anime", Some("modo-de-anime")),
                ("AnimeToo", Some("modo-de-anime")),
                ("NamesCrt", Some("modo-de-namescrt")),
                ("NamesCrtLoose", Some("modo-de-namescrtloose")),
            ]
        );
        // The top level's cas_enabled reached NamesCrt through its mode.
        let crt = &modes.raw["modo-de-namescrt"];
        assert_eq!(crt["cas_enabled"], Value::Bool(true));
        assert_eq!(crt["model"], Value::from("none"));
        assert!(modes.raw.contains_key("crt"));

        assert!(file.profiles["Same"].options.get("mode").is_none());
        assert_eq!(file.profiles["Same"].options["max_fps"], Value::from(30));
        assert_eq!(file.profiles["AnimeToo"].extra["icon"], Value::from("anime.png"));
        assert_eq!(file.general["hotkeys"]["zoom_in"], Value::from("Ctrl+Plus"));
        assert_eq!(file.profiles["Nulls"].options.get("model"), Some(&Value::Null));
    }

    #[test]
    fn reuses_existing_modes_and_free_ids() {
        // `anime` already holds Anime's combination; `predeterminado` is taken.
        let dir = Dir::new(
            "model: fast\nprofiles:\n  Anime:\n    options:\n      model: 4x24\n",
            &[
                ("anime", "name: Anime\nmodel: 4x24\n"),
                ("predeterminado", "name: Otro\nmodel: none\n"),
            ],
        );
        let (plan, _, modes) = migrate_and_check(&dir);
        assert_eq!(plan.top_mode.as_deref(), Some("predeterminado-2"));
        assert_eq!(plan.profiles, [("Anime".to_owned(), Some("anime".to_owned()))]);
        assert_eq!(plan.modes.len(), 1);
        assert_eq!(modes.raw["predeterminado"]["model"], Value::from("none"));
    }

    #[test]
    fn top_level_mode_and_loose_fields() {
        let dir = Dir::new(
            "mode: anime\nmodel: fast\nprofiles:\n  P:\n    options:\n      grain_enabled: true\n",
            &[(
                "anime",
                "name: Anime\nmodel: 4x24\ncas_enabled: true\nfuture_key: 1\n",
            )],
        );
        let (plan, file, modes) = migrate_and_check(&dir);
        let top = plan.top_mode.unwrap();
        assert_eq!(file.general["mode"], Value::from(top.as_str()));
        assert_eq!(modes.raw[&top]["model"], Value::from("fast"));
        assert_eq!(modes.raw[&top]["future_key"], Value::from(1));
        let p = plan.profiles[0].1.clone().unwrap();
        assert_eq!(modes.raw[&p]["future_key"], Value::from(1));
    }

    #[test]
    fn nothing_to_do() {
        let dir = Dir::new("mode: anime\nshow_stats: true\n", &[("anime", "model: 4x24\n")]);
        let (file, modes) = dir.load();
        assert!(!needs_migration(&file));
        assert!(plan(&file, &modes).is_empty());
        let only_nulls = Dir::new("model: null\nprofiles:\n  P:\n    options: {model: null}\n", &[]);
        assert!(!needs_migration(&only_nulls.load().0));
    }

    #[test]
    fn second_backup_keeps_the_first() {
        let dir = Dir::new("model: fast\n", &[]);
        std::fs::write(dir.dir.join("config.yaml.1x.bak"), "old\n").unwrap();
        let (mut file, mut modes) = dir.load();
        let plan = plan(&file, &modes);
        let backup = apply(&mut file, &mut modes, &plan).unwrap();
        assert_eq!(backup, dir.dir.join("config.yaml.1x.bak2"));
        assert_eq!(
            std::fs::read_to_string(dir.dir.join("config.yaml.1x.bak")).unwrap(),
            "old\n"
        );
    }

    #[test]
    fn legacy_gui_settings() {
        let qs = "[mainwindow]\ngeometry=@ByteArray(x)\n\n[overlay]\nwindowed_position=@Point(2400 532)\n\n\
                  [tray]\nenabled=false\n";
        let gui = GuiConfig::default();
        let new = legacy_gui(&gui, Some(qs)).unwrap();
        assert!(!new.show_tray_icon);
        assert_eq!(new.close_to_tray, gui.close_to_tray);

        let mut gui = GuiConfig::default();
        gui.extra.insert("palette_preset".into(), "Auto".into());
        assert!(legacy_gui(&gui, None).is_none());
        gui.extra.insert("palette_preset".into(), "Nord".into());
        assert_eq!(legacy_gui(&gui, None).unwrap().seed_color, "#88C0D0");
        gui.seed_color = "#123456".into();
        assert!(legacy_gui(&gui, None).is_none(), "a seed chosen in 2.0 stays");

        let qs = "[tray]\nenabled=true\nclose_to_tray=false\nminimize_to_tray=true\nstart_hidden=true\n\
                  keep_running_on_exit=true\n";
        let new = legacy_gui(&GuiConfig::default(), Some(qs)).unwrap();
        assert!(new.show_tray_icon && !new.close_to_tray && new.minimize_to_tray);
        assert!(new.start_hidden && new.keep_running_after_exit);
    }

    #[test]
    fn apply_moves_legacy_gui() {
        let dir = Dir::new("model: fast\n", &[]);
        std::fs::write(
            dir.dir.parent().unwrap().join(QSETTINGS_FILE),
            "[tray]\nenabled=false\n",
        )
        .unwrap();
        std::fs::write(
            dir.dir.join("gui-config.yaml"),
            "palette_preset: Dracula\nzoom: 110\n",
        )
        .unwrap();
        let (mut file, mut modes) = dir.load();
        let plan = plan(&file, &modes);
        apply(&mut file, &mut modes, &plan).unwrap();
        let gui = GuiConfig::load(Some(&dir.dir.join("gui-config.yaml"))).unwrap();
        assert!(!gui.show_tray_icon);
        assert_eq!(gui.seed_color, "#BD93F9");
        assert_eq!(gui.zoom, 110);
        assert!(gui.extra.contains_key("palette_preset"));
    }
}
