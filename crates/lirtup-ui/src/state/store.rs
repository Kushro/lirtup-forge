//! The bodies of the state (front C): [`Field`], the derived views of the
//! [`Store`] and [`update`].
//!
//! The signatures are the contract with B and D; the bodies are C's. The
//! tests of `update()` live here (`#[cfg(test)]`, no Dioxus).
//!
//! # Layers
//!
//! A setting is looked up by its YAML key in one layer (the top level of
//! `config.yaml`, a profile's `options`, a mode file), with the unsaved
//! edits of that layer on top. What a layer inherits is what the scaler
//! would use without it, in the order of `lirtup_config::resolve`:
//!
//! - a mode file inherits the system defaults;
//! - *Predeterminado* inherits the mode it names (mode keys only), then the
//!   system defaults;
//! - a profile inherits *Predeterminado*; for mode keys, the mode the
//!   profile names sits in between (and then *Predeterminado*'s mode is not
//!   used, as in the resolver).
//!
//! A `null` in a layer means «inherit», as `merge_layer` reads it.
//!
//! # Edits
//!
//! An edit equal to the saved value is dropped, so undoing a change by hand
//! clears the dot. Guardar merges the edits into the files as read
//! ([`apply_edits`]): keys keep their place, keys the GUI does not know stay.
//! Creating, renaming, duplicating and deleting profiles, and the menu bar
//! crop the scaler finds, write the saved file at once, leaving the edits
//! pending.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use indexmap::IndexMap;
use lirtup_config::{Config, ConfigFile, MODE_KEYS, Mapping, Mode, ModeStore, Profile, Value};
use lirtup_proto::{Command, Event};
use lirtup_x11::WindowInfo;

use super::{
    Action, BenchView, CompileState, ConfigDoc, Dialog, Effect, Field, Key, LinkState, ModesDoc, Origin,
    Saving, Scope, SessionState, Store, Toast, UnsavedChoice, Written,
};
use crate::route::{ModeId, ProfileName, ProfileRef, ProfileTab, Route};

/// [`Store::logs`] keeps at most this many lines.
pub const MAX_LOGS: usize = 200;

/// `hotkeys` is merged key by key: its settings are `hotkeys.<action>`.
const HOTKEYS: &str = "hotkeys";

/// The match rules of a profile are an edit like any other, under this key
/// (they live next to `options`, not in it).
pub const MATCH_KEY: &str = "match";

impl<T: Clone + PartialEq> Field<T> {
    /// The value the row shows: the edit, else this layer's, else the
    /// default.
    pub fn value(&self) -> T {
        match &self.edit {
            Some(Some(v)) => v.clone(),
            Some(None) => self.default.clone(),
            None => self.profile.clone().unwrap_or_else(|| self.default.clone()),
        }
    }

    /// Where the value comes from; `fmt` formats the saved value for
    /// «antes: …».
    pub fn origin(&self, fmt: impl Fn(&T) -> String) -> Origin {
        if self.is_dirty() {
            let before = self.profile.as_ref().unwrap_or(&self.default);
            Origin::Dirty { before: fmt(before) }
        } else if self.profile.is_some() {
            Origin::Overridden
        } else {
            Origin::Inherited
        }
    }

    /// There is an unsaved edit that changes the saved value.
    pub fn is_dirty(&self) -> bool {
        self.edit.as_ref().is_some_and(|e| *e != self.profile)
    }
}

const SCALING_KEYS: &[&str] = &["mode", "follow_focus", "pause_on_focus_loss", "daemon_exclude"];

const PRESENTATION_KEYS: &[&str] = &[
    "overlay_mode",
    "output_geometry",
    "auto_crop_menu_bar",
    "hide_cursor",
    "crop_top",
    "crop_bottom",
    "crop_left",
    "crop_right",
    "offset_x",
    "offset_y",
    "background_color",
    "frame_gen_engine",
    "frame_gen_multiplier",
    "frame_gen_flow_scale",
    "frame_gen_performance",
    "frame_gen_dll",
];

const DISPLAY_KEYS: &[&str] = &[
    "monitor",
    "gpu",
    "vulkan_present_mode",
    "max_fps",
    "present_on_change_only",
    "scale_factor",
];

const EXTRAS_KEYS: &[&str] = &[
    "screenshot_dir",
    "screenshot_filename",
    "show_osd",
    "osd_duration",
    "show_stats",
    "latency_log",
];

const ADVANCED_KEYS: &[&str] = &[
    "use_tile_processing",
    "use_damage_tracking",
    "tile_size",
    "tile_context_margin",
    "max_tile_layers",
    "area_threshold",
    "frame_timeout",
    "focus_poll_interval",
    "pipeline_poll_interval",
    "daemon_poll_interval",
    "max_capture_failures",
    "capture_failure_delay",
    "swapchain_debounce",
];

/// The keys of each profile tab, in display order (the tab's dot, Restablecer
/// per tab). Keys that are not in any tab are kept but never shown.
pub fn tab_keys(tab: ProfileTab) -> &'static [&'static str] {
    match tab {
        ProfileTab::Scaling => SCALING_KEYS,
        ProfileTab::Presentation => PRESENTATION_KEYS,
        ProfileTab::Display => DISPLAY_KEYS,
        ProfileTab::Extras => EXTRAS_KEYS,
        ProfileTab::Advanced => ADVANCED_KEYS,
    }
}

/// The tab a key belongs to.
pub fn tab_of(key: &str) -> Option<ProfileTab> {
    ProfileTab::ALL
        .into_iter()
        .find(|tab| tab_keys(*tab).contains(&key))
}

/// The keys of [`global_keys`] (6.7 of the analysis; the reasons are in
/// `docs/developer/rust-r3-profiles.md`).
const GLOBAL_KEYS: &[&str] = &[
    // Frame generation: the lsfg-vk layer is configured before the Vulkan
    // instance, once per session (1.x already treats it as global).
    "frame_gen_engine",
    "frame_gen_multiplier",
    "frame_gen_flow_scale",
    "frame_gen_performance",
    "frame_gen_dll",
    // The daemon looks for windows before any profile applies.
    "daemon",
    "daemon_poll_interval",
    // Window detection runs before the profile is chosen.
    "target_delay",
    "pid_timeout",
    "class_timeout",
    "total_timeout",
    "starting_phase",
    // The process logs once, for every session.
    "log_level",
    "log_file",
];

/// Keys that are global even when edited from a profile page (frame
/// generation; 6.7 of the analysis lists the rest to check): edits go to
/// *Predeterminado* and the row says so.
pub fn global_keys() -> &'static [&'static str] {
    GLOBAL_KEYS
}

/// Whether `key` is in [`global_keys`].
pub fn is_global(key: &str) -> bool {
    GLOBAL_KEYS.contains(&key)
}

/// The layer an edit of `key` from `scope` goes to: *Predeterminado* for
/// [`global_keys`] edited from a profile.
pub fn effective_scope(scope: &Scope, key: &str) -> Scope {
    match scope {
        Scope::Profile(ProfileRef::Named(_)) if is_global(key) => Scope::Profile(ProfileRef::Default),
        other => other.clone(),
    }
}

/// Where the icon of a profile goes (`icons/<name>.png` next to
/// `config.yaml`, with the characters 1.x replaces).
pub fn icon_path(config: &Path, name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let dir = match config.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => lirtup_config::paths::config_dir(),
    };
    dir.join("icons").join(format!("{}.png", safe.trim()))
}

/// The value as a row prints it in «antes: …»: YAML, without quotes.
pub fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "—".into(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => serde_yaml_ng::to_string(other)
            .map(|s| s.trim().replace('\n', " "))
            .unwrap_or_default(),
    }
}

/// `Config::default()` as a mapping: what a layer gives when nothing sets a
/// key.
fn system_defaults() -> &'static Mapping {
    static DEFAULTS: OnceLock<Mapping> = OnceLock::new();
    DEFAULTS.get_or_init(|| match serde_yaml_ng::to_value(Config::default()) {
        Ok(Value::Mapping(map)) => map,
        _ => Mapping::new(),
    })
}

/// `key` in a raw layer, `hotkeys.<action>` one level down. `null` counts
/// as not set.
fn lookup<'a>(layer: &'a Mapping, key: &str) -> Option<&'a Value> {
    let value = match key.split_once('.') {
        Some((HOTKEYS, action)) => layer.get(HOTKEYS)?.as_mapping()?.get(action),
        _ => layer.get(key),
    };
    value.filter(|v| !v.is_null())
}

/// Set (`Some`) or remove (`None`) `key` in a raw layer. An existing key
/// keeps its place.
fn set_in(layer: &mut Mapping, key: &str, value: Option<Value>) {
    match key.split_once('.') {
        Some((HOTKEYS, action)) => {
            let entry = layer.get_mut(HOTKEYS);
            match (entry, value) {
                (Some(Value::Mapping(map)), Some(v)) => {
                    map.insert(action.into(), v);
                }
                (Some(Value::Mapping(map)), None) => {
                    map.shift_remove(action);
                    if map.is_empty() {
                        layer.shift_remove(HOTKEYS);
                    }
                }
                (_, Some(v)) => {
                    let mut map = Mapping::new();
                    map.insert(action.into(), v);
                    layer.insert(HOTKEYS.into(), Value::Mapping(map));
                }
                (_, None) => {}
            }
        }
        _ => match value {
            Some(v) => {
                layer.insert(key.into(), v);
            }
            None => {
                layer.shift_remove(key);
            }
        },
    }
}

/// Two values the same for the person: numbers by value (`1` and `1.0`),
/// `null` as missing.
fn same(a: Option<&Value>, b: Option<&Value>) -> bool {
    let a = a.filter(|v| !v.is_null());
    let b = b.filter(|v| !v.is_null());
    match (a, b) {
        (None, None) => true,
        (Some(Value::Number(x)), Some(Value::Number(y))) => x.as_f64() == y.as_f64(),
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// The file with the edits of `doc` merged in (what Guardar writes).
/// Unknown keys stay as read.
pub fn apply_edits(file: &ConfigFile, edits: &IndexMap<(ProfileRef, Key), Option<Value>>) -> ConfigFile {
    let mut out = file.clone();
    for ((profile, key), value) in edits {
        match profile {
            ProfileRef::Default => set_in(&mut out.general, key, value.clone()),
            ProfileRef::Named(name) => {
                let Some(p) = out.profiles.get_mut(name) else {
                    continue;
                };
                if key == MATCH_KEY {
                    p.matcher = value
                        .clone()
                        .and_then(|v| serde_yaml_ng::from_value(v).ok())
                        .filter(|m: &lirtup_config::Match| !m.is_empty());
                } else {
                    set_in(&mut p.options, key, value.clone());
                }
            }
        }
    }
    out
}

/// The modes with their edits merged in, only the ones that changed.
pub fn apply_mode_edits(
    modes: &ModeStore,
    edits: &IndexMap<(String, Key), Option<Value>>,
) -> Vec<(String, lirtup_config::Mode, Mapping)> {
    let mut changed: IndexMap<String, Mapping> = IndexMap::new();
    for ((id, key), value) in edits {
        let Some(raw) = modes.raw.get(id) else {
            continue;
        };
        let layer = changed.entry(id.clone()).or_insert_with(|| raw.clone());
        set_in(layer, key, value.clone());
    }
    changed
        .into_iter()
        .filter(|(id, raw)| modes.raw.get(id) != Some(raw))
        .map(|(id, raw)| {
            let (mut mode, _) = Mode::from_mapping(&id, &raw);
            // `from_mapping` falls back to the id; keep the name as read.
            if let Some(old) = modes.modes.get(&id)
                && !raw.contains_key("name")
            {
                mode.name = old.name.clone();
            }
            (id, mode, raw)
        })
        .collect()
}

/// `raw` with its `name` set to `name` (a new or copied mode file).
fn named_raw(mut raw: Mapping, name: &str) -> Mapping {
    raw.insert("name".into(), Value::String(name.to_owned()));
    raw
}

impl Store {
    /// One setting of one layer, typed. `T` is read from the YAML value with
    /// serde; a value that does not parse counts as not set.
    pub fn field<T: Clone + PartialEq + serde::de::DeserializeOwned>(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Field<T> {
        let scope = effective_scope(scope, key);
        let parse = |v: Value| serde_yaml_ng::from_value::<T>(v).ok();
        let profile = self.saved(&scope, key).and_then(parse);
        let edit = match self.edit_of(&scope, key) {
            None => None,
            Some(None) => Some(None),
            Some(Some(v)) => parse(v).map(Some),
        };
        let below = self.below(&scope, key);
        let default = parse(below.unwrap_or(Value::Null))
            .or_else(|| lookup(system_defaults(), key).cloned().and_then(parse))
            .or_else(|| parse(Value::Null))
            .or_else(|| profile.clone())
            .or_else(|| edit.clone().flatten())
            .unwrap_or_else(|| panic!("setting {key:?} does not read as the type asked for"));
        Field {
            default,
            profile,
            edit,
        }
    }

    /// The raw layer of a scope as saved (`general`, a profile's `options`,
    /// a mode's mapping).
    pub fn layer(&self, scope: &Scope) -> Option<&Mapping> {
        match scope {
            Scope::Profile(ProfileRef::Default) => Some(&self.config.file.general),
            Scope::Profile(ProfileRef::Named(name)) => {
                self.config.file.profiles.get(name).map(|p| &p.options)
            }
            Scope::Mode(id) => self.modes.store.raw.get(id),
        }
    }

    /// The origin of one row (shorthand for `field(...).origin(...)` with the
    /// value printed as YAML).
    pub fn origin(&self, scope: &Scope, key: &str) -> Origin {
        self.field::<Value>(scope, key).origin(format_value)
    }

    /// Unsaved edits of a scope (the save bar's «N cambios sin guardar» when
    /// the page is a profile or a mode).
    pub fn dirty_count(&self, scope: &Scope) -> usize {
        match scope {
            Scope::Profile(p) => self
                .config
                .edits
                .iter()
                .filter(|((q, key), v)| q == p && !same(v.as_ref(), self.saved(scope, key).as_ref()))
                .count(),
            Scope::Mode(id) => self
                .modes
                .edits
                .iter()
                .filter(|((m, key), v)| m == id && !same(v.as_ref(), self.saved(scope, key).as_ref()))
                .count(),
        }
    }

    /// Unsaved edits anywhere (Guardar is enabled; leaving the GUI asks).
    pub fn has_edits(&self) -> bool {
        !self.config.edits.is_empty() || !self.modes.edits.is_empty()
    }

    /// The tab's dot.
    pub fn tab_dirty(&self, profile: &ProfileRef, tab: ProfileTab) -> bool {
        tab_keys(tab).iter().any(|key| {
            let scope = effective_scope(&Scope::Profile(profile.clone()), key);
            let Scope::Profile(p) = &scope else { return false };
            self.config
                .edits
                .get(&(p.clone(), (*key).to_owned()))
                .is_some_and(|v| !same(v.as_ref(), self.saved(&scope, key).as_ref()))
        })
    }

    /// The drawer's dot.
    pub fn profile_dirty(&self, profile: &ProfileRef) -> bool {
        self.dirty_count(&Scope::Profile(profile.clone())) > 0
    }

    /// Profiles in drawer order: *Predeterminado* first, then the file
    /// order.
    pub fn profiles(&self) -> Vec<ProfileRef> {
        std::iter::once(ProfileRef::Default)
            .chain(self.config.file.profiles.keys().cloned().map(ProfileRef::Named))
            .collect()
    }

    /// The profile a session is scaling with (the drawer's amber ▶).
    pub fn running_profile(&self) -> Option<ProfileRef> {
        let SessionState::Running(view) = &self.session else {
            return None;
        };
        match &view.profile {
            None => Some(ProfileRef::Default),
            Some(name) => lirtup_config::resolve::find_profile(&self.config.file, name)
                .map(|(n, _)| ProfileRef::Named(n.to_owned())),
        }
    }

    /// The profile whose match rules fit a window (the tile's label and what
    /// a click scales with), as the core does (`find_matching_profile`).
    pub fn matching_profile(&self, window: &WindowInfo) -> Option<ProfileRef> {
        lirtup_config::profile::find_matching_profile(
            &self.config.file.profiles,
            &window.title,
            window.width(),
            window.height(),
        )
        .map(|(name, _)| ProfileRef::Named(name.to_owned()))
    }

    /// The mode a profile uses after inheritance (its own `mode`, else
    /// *Predeterminado*'s).
    pub fn mode_of(&self, profile: &ProfileRef) -> Option<String> {
        let own = |p: &ProfileRef| {
            self.effective(&Scope::Profile(p.clone()), "mode")
                .and_then(|v| v.as_str().map(str::to_owned))
                .filter(|m| !m.is_empty())
        };
        own(profile).or_else(|| own(&ProfileRef::Default))
    }

    /// Unsaved edits of the page a route shows: its scope's, plus, on a
    /// profile, the [`global_keys`] edited on *Predeterminado* (they show
    /// on every profile page). What the save bar counts and what leaving
    /// the page asks about.
    pub fn page_edits(&self, route: &Route) -> usize {
        match route {
            Route::Profile { id, .. } => {
                let own = self.dirty_count(&Scope::Profile(id.clone()));
                let global = match id {
                    ProfileRef::Default => 0,
                    ProfileRef::Named(_) => self
                        .config
                        .edits
                        .keys()
                        .filter(|(p, key)| *p == ProfileRef::Default && is_global(key))
                        .count(),
                };
                own + global
            }
            Route::Modes { id: Some(id) } => self.dirty_count(&Scope::Mode(id.clone())),
            _ => 0,
        }
    }

    /// Profiles that would match a window with these rules, among the open
    /// windows (the «Coincide con N ventanas» of «Nuevo perfil»).
    pub fn windows_matching(&self, matcher: &lirtup_config::Match) -> Vec<&WindowInfo> {
        self.windows
            .iter()
            .filter(|w| matcher.matches(&w.title, w.width(), w.height()))
            .collect()
    }

    /// The saved value of `key` in a layer (`null` = not set). `match` of a
    /// profile is its rules.
    fn saved(&self, scope: &Scope, key: &str) -> Option<Value> {
        if key == MATCH_KEY
            && let Scope::Profile(ProfileRef::Named(name)) = scope
        {
            return self
                .config
                .file
                .profiles
                .get(name)?
                .matcher
                .as_ref()
                .and_then(|m| serde_yaml_ng::to_value(m).ok());
        }
        lookup(self.layer(scope)?, key).cloned()
    }

    /// The unsaved edit of `key`: `None` untouched, `Some(None)` inherit.
    /// An edit of the whole `hotkeys` covers each `hotkeys.<action>`.
    fn edit_of(&self, scope: &Scope, key: &str) -> Option<Option<Value>> {
        let get = |k: &str| -> Option<&Option<Value>> {
            match scope {
                Scope::Profile(p) => self.config.edits.get(&(p.clone(), k.to_owned())),
                Scope::Mode(id) => self.modes.edits.get(&(id.clone(), k.to_owned())),
            }
        };
        if let Some(edit) = get(key) {
            return Some(edit.clone().filter(|v| !v.is_null()));
        }
        let (HOTKEYS, action) = key.split_once('.')? else {
            return None;
        };
        let whole = get(HOTKEYS)?;
        Some(whole.as_ref().and_then(|m| m.as_mapping()?.get(action).cloned()))
    }

    /// The value of a layer with its edit (`None` = not set here).
    fn effective(&self, scope: &Scope, key: &str) -> Option<Value> {
        match self.edit_of(scope, key) {
            Some(edit) => edit,
            None => self.saved(scope, key),
        }
    }

    /// What the layers below `scope` give for `key` (with their edits).
    fn below(&self, scope: &Scope, key: &str) -> Option<Value> {
        let system = || lookup(system_defaults(), key).cloned();
        let mode = |id: &str| self.effective(&Scope::Mode(id.to_owned()), key);
        let is_mode_key = MODE_KEYS.contains(&key);
        match scope {
            // A mode file without `name` shows its id (`Mode::from_mapping`).
            Scope::Mode(id) if key == "name" => {
                self.modes.store.get(id).map(|m| Value::String(m.name.clone()))
            }
            Scope::Mode(_) => system(),
            Scope::Profile(ProfileRef::Default) => {
                let named = self.mode_named_by(&ProfileRef::Default);
                named
                    .filter(|_| is_mode_key)
                    .and_then(|m| mode(&m))
                    .or_else(system)
            }
            Scope::Profile(ProfileRef::Named(_)) if key == MATCH_KEY => None,
            Scope::Profile(profile @ ProfileRef::Named(_)) => {
                let default = Scope::Profile(ProfileRef::Default);
                match self.mode_named_by(profile).filter(|_| is_mode_key) {
                    // The profile's mode replaces Predeterminado's, and sits
                    // above Predeterminado's loose fields.
                    Some(m) => mode(&m).or_else(|| self.effective(&default, key)).or_else(system),
                    None => self
                        .effective(&default, key)
                        .or_else(|| self.below(&default, key)),
                }
            }
        }
    }

    /// The mode one layer names itself (with its edit).
    fn mode_named_by(&self, profile: &ProfileRef) -> Option<String> {
        self.effective(&Scope::Profile(profile.clone()), "mode")
            .and_then(|v| v.as_str().map(str::to_owned))
            .filter(|m| !m.is_empty())
    }

    /// Record an edit, or drop it when it gives back the saved value.
    fn set_edit(&mut self, scope: &Scope, key: &str, value: Option<Value>) {
        let value = value.filter(|v| !v.is_null());
        let saved = self.saved(scope, key);
        let unchanged = same(value.as_ref(), saved.as_ref());
        match scope {
            Scope::Profile(p) => {
                let k = (p.clone(), key.to_owned());
                if unchanged {
                    self.config.edits.shift_remove(&k);
                } else {
                    self.config.edits.insert(k, value);
                }
            }
            Scope::Mode(id) => {
                let k = (id.clone(), key.to_owned());
                if unchanged {
                    self.modes.edits.shift_remove(&k);
                } else {
                    self.modes.edits.insert(k, value);
                }
            }
        }
    }

    /// Drop the edit of `key`.
    fn revert(&mut self, scope: &Scope, key: &str) {
        match scope {
            Scope::Profile(p) => {
                self.config.edits.shift_remove(&(p.clone(), key.to_owned()));
            }
            Scope::Mode(id) => {
                self.modes.edits.shift_remove(&(id.clone(), key.to_owned()));
            }
        }
    }

    /// Who names each mode (for the shared mode banner).
    fn refresh_users(&mut self) {
        let mut users: IndexMap<ModeId, Vec<ProfileRef>> = self
            .modes
            .store
            .modes
            .keys()
            .map(|id| (id.clone(), Vec::new()))
            .collect();
        for profile in self.profiles() {
            if let Some(id) = self.mode_named_by(&profile)
                && let Some(list) = users.get_mut(&id)
            {
                list.push(profile);
            }
        }
        self.modes.users = users;
    }
}

/// Whether an [`Action::Navigate`] or a profile page needs `name`.
fn profile_exists(store: &Store, profile: &ProfileRef) -> bool {
    match profile {
        ProfileRef::Default => true,
        ProfileRef::Named(name) => store.config.file.profiles.contains_key(name),
    }
}

/// A name for a copy: `<name> (2)`, `(3)`, ... (language-neutral: `update`
/// does not know the language).
fn copy_name(taken: impl Fn(&str) -> bool, name: &str) -> String {
    (2..)
        .map(|n| format!("{name} ({n})"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| format!("{name} (2)"))
}

/// A profile name is taken, ignoring case (`find_profile` matches that way).
fn name_taken(file: &ConfigFile, name: &str) -> bool {
    let lower = name.to_lowercase();
    file.profiles.keys().any(|n| n.to_lowercase() == lower)
}

fn toast(key: &'static str, args: Vec<(&'static str, String)>) -> Toast {
    Toast {
        key,
        args,
        undo: false,
    }
}

/// Round 4, front M: on the first load, offer moving the loose 1.x mode
/// fields to modes (`Dialog::Migrate`), unless it was declined
/// (`gui.migration_declined`) or another dialog is open.
fn offer_migration(store: &mut Store) {
    if store.gui.migration_declined
        || store.dialog.is_some()
        || !lirtup_config::migrate::needs_migration(&store.config.file)
    {
        return;
    }
    let plan = lirtup_config::migrate::plan(&store.config.file, &store.modes.store);
    if !plan.is_empty() {
        store.dialog = Some(Dialog::Migrate(plan));
    }
}

fn join(a: Option<Effect>, b: Option<Effect>) -> Option<Effect> {
    match (a, b) {
        (None, b) => b,
        (a, None) => a,
        (Some(Effect::Many(mut a)), Some(b)) => {
            a.push(b);
            Some(Effect::Many(a))
        }
        (Some(a), Some(b)) => Some(Effect::Many(vec![a, b])),
    }
}

fn send(command: Command) -> Option<Effect> {
    Some(Effect::Send(command))
}

/// Show the snackbar (`app.rs` dismisses it after a while if it is still
/// the same one).
fn show(store: &mut Store, t: Toast) -> Option<Effect> {
    store.toast = Some(t.clone());
    Some(Effect::Toast(t))
}

/// Start (or join) [`Store::saving`] with `writes` writes on their way.
/// Call it after the store already shows the result.
fn begin_saving(
    store: &mut Store,
    before: (ConfigDoc, ModesDoc, Option<ConfigFile>),
    writes: usize,
    done: Toast,
    undo_to: Option<ConfigFile>,
    restart: bool,
) {
    match &mut store.saving {
        Some(saving) => {
            saving.pending += writes;
            saving.done = done;
            saving.undo_to = undo_to;
            saving.restart |= restart;
        }
        None => {
            store.saving = Some(Saving {
                config: before.0,
                modes: before.1,
                undo: before.2,
                pending: writes,
                error: None,
                done,
                undo_to,
                restart,
            });
        }
    }
}

fn snapshot(store: &Store) -> (ConfigDoc, ModesDoc, Option<ConfigFile>) {
    (store.config.clone(), store.modes.clone(), store.undo.clone())
}

/// Write the saved `config.yaml` after a change to its structure (a profile
/// created, renamed, duplicated or deleted; the menu bar crop). The edits
/// stay pending. `change` gets the saved file and the edits.
fn write_structure(
    store: &mut Store,
    done: Toast,
    change: impl FnOnce(&mut ConfigFile, &mut IndexMap<(ProfileRef, Key), Option<Value>>),
) -> Option<Effect> {
    let before = snapshot(store);
    change(&mut store.config.file, &mut store.config.edits);
    begin_saving(store, before, 1, done, None, false);
    Some(Effect::WriteConfig(store.config.file.clone()))
}

/// Guardar: every edit of config and modes.
fn save(store: &mut Store, restart: bool) -> Option<Effect> {
    if !store.has_edits() {
        return restart.then(|| Effect::Send(Command::Restart));
    }
    let before = snapshot(store);
    let mut effects = Vec::new();
    let mut undo_to = None;
    if !store.config.edits.is_empty() {
        let file = apply_edits(&store.config.file, &store.config.edits);
        undo_to = Some(std::mem::replace(&mut store.config.file, file));
        store.config.edits.clear();
        effects.push(Effect::WriteConfig(store.config.file.clone()));
    }
    for (id, mode, raw) in apply_mode_edits(&store.modes.store, &store.modes.edits) {
        store.modes.store.raw.insert(id.clone(), raw.clone());
        store.modes.store.modes.insert(id.clone(), mode.clone());
        effects.push(Effect::WriteMode { id, mode, raw });
    }
    store.modes.edits.clear();
    if effects.is_empty() {
        return None;
    }
    let mut done = match store.route.profile() {
        Some(ProfileRef::Named(name)) => toast("toast-saved", vec![("name", name.clone())]),
        _ => toast("profile-toast-saved", Vec::new()),
    };
    done.undo = undo_to.is_some();
    begin_saving(store, before, effects.len(), done, undo_to, restart);
    Some(Effect::Many(effects))
}

/// Drop the edits of the page `route` shows (see [`Store::page_edits`]).
fn discard_page(store: &mut Store, route: &Route) {
    match route {
        Route::Profile { id, .. } => {
            let named = matches!(id, ProfileRef::Named(_));
            store
                .config
                .edits
                .retain(|(p, key), _| p != id && !(named && *p == ProfileRef::Default && is_global(key)));
        }
        Route::Modes { id: Some(id) } => store.modes.edits.retain(|(m, _), _| m != id),
        _ => {}
    }
}

/// Every key a layer knows (the `Config` fields), for «Volver todo a los
/// valores del sistema». Keys the GUI does not know stay.
fn known_keys() -> impl Iterator<Item = &'static str> {
    system_defaults().keys().filter_map(Value::as_str)
}

/// One [`Action::Written`]: count it, and when the last write of a save
/// arrives, say how it went.
fn written(store: &mut Store, result: Result<Written, String>) -> Option<Effect> {
    if matches!(result, Ok(Written::Gui)) {
        return None;
    }
    let Some(saving) = &mut store.saving else {
        // A write nobody waits for (gui-config.yaml): only errors matter.
        return match result {
            Err(error) => show(store, toast("toast-write-failed", vec![("error", error)])),
            Ok(_) => None,
        };
    };
    if let Err(error) = result
        && saving.error.is_none()
    {
        saving.error = Some(error);
    }
    saving.pending = saving.pending.saturating_sub(1);
    if saving.pending > 0 {
        return None;
    }
    let saving = store.saving.take()?;
    if let Some(error) = saving.error {
        store.config = saving.config;
        store.modes = saving.modes;
        store.undo = saving.undo;
        return show(store, toast("toast-write-failed", vec![("error", error)]));
    }
    store.undo = saving.undo_to;
    let mut effect = send(Command::ReloadConfig);
    if saving.restart && matches!(store.session, SessionState::Running(_)) {
        effect = join(effect, send(Command::Restart));
    }
    join(effect, show(store, saving.done))
}

/// An event of the core.
fn core(store: &mut Store, event: Event) -> Option<Effect> {
    match event {
        Event::Snapshot(snapshot) => {
            store.session = match (snapshot.session, snapshot.last_error) {
                (Some(view), _) => SessionState::Running(view),
                // An error from before this connection: the GUI does not know
                // what was being scaled, so there is nothing to retry.
                (None, Some((stage, message))) => SessionState::Failed {
                    stage,
                    message,
                    retry: false,
                },
                (None, None) => SessionState::Idle,
            };
            store.daemon = snapshot.daemon;
            store.framegen = snapshot.framegen;
            if let LinkState::Connected { protocol, version } = &mut store.link {
                *protocol = snapshot.protocol;
                *version = snapshot.version;
            }
        }
        Event::SessionStarting { target } => store.session = SessionState::Starting { target },
        Event::SessionStarted(view) => store.session = SessionState::Running(view),
        Event::SessionStopped { reason } => {
            // An error already turned the card into an error card.
            let failed = matches!(store.session, SessionState::Failed { .. });
            if !(failed && reason == lirtup_proto::StopReason::Error) {
                store.session = SessionState::Idle;
            }
            store.stats = None;
        }
        Event::SessionError {
            stage,
            message,
            retry,
        } => {
            store.session = SessionState::Failed {
                stage,
                message,
                retry,
            };
            store.stats = None;
        }
        Event::Stats(sample) => store.stats = Some(sample),
        Event::FrameGen(status) => store.framegen = status,
        Event::DaemonState {
            on,
            keep_after_exit,
            watching,
        } => {
            store.daemon = lirtup_proto::DaemonStatus {
                on,
                keep_after_exit,
                watching,
            };
        }
        Event::DaemonMatch { .. } => {}
        Event::MenuBarDetected { profile, crop } => return menu_bar(store, profile, crop),
        Event::SaveSetting { profile, key, value } => return save_setting(store, profile, key, value),
        Event::WindowPicked { window } => store.picked = Some(window),
        Event::Effects { effects } => store.effects = effects,
        Event::EffectCompile {
            name,
            done,
            error,
            session,
        } => {
            store
                .compiling
                .insert(name, CompileState { done, error, session });
        }
        Event::Devices { monitors, gpus } => {
            store.monitors = monitors;
            store.gpus = gpus;
        }
        Event::BenchProgress { done, total } => {
            store.bench.done = done;
            store.bench.total = total;
        }
        Event::BenchResult(report) => store.bench.report = Some(report),
        Event::Diagnostics(report) => store.diagnostics = Some(report),
        Event::Log { level, message } => {
            store.logs.push((level, message));
            let extra = store.logs.len().saturating_sub(MAX_LOGS);
            store.logs.drain(..extra);
        }
    }
    None
}

/// The scaler found a Wine menu bar: keep the crop in the session's
/// profile and turn detection off there, so the next session does not crop
/// twice (`remember_menu_bar_crop` of 1.x). Without a profile it is only
/// for this session.
fn menu_bar(store: &mut Store, profile: Option<String>, crop: lirtup_config::Crop) -> Option<Effect> {
    let found = profile
        .as_deref()
        .and_then(|name| lirtup_config::resolve::find_profile(&store.config.file, name))
        .map(|(name, _)| name.to_owned());
    let Some(name) = found else {
        tracing::info!("menu bar crop not saved: the session has no profile");
        return None;
    };
    let done = toast("profile-toast-menubar", vec![("name", name.clone())]);
    write_structure(store, done, |file, _| {
        if let Some(p) = file.profiles.get_mut(&name) {
            for (key, value) in [
                ("crop_top", crop.top),
                ("crop_bottom", crop.bottom),
                ("crop_left", crop.left),
                ("crop_right", crop.right),
            ] {
                set_in(&mut p.options, key, Some(Value::from(value)));
            }
            set_in(&mut p.options, "auto_crop_menu_bar", Some(Value::Bool(false)));
        }
    })
}

/// `SaveSetting` from the bar over the game: write the key in the session's
/// profile (or *Predeterminado* for a global key or no profile). Unsaved
/// edits of the same key are dropped: the live value wins.
fn save_setting(store: &mut Store, profile: Option<String>, key: String, value: Value) -> Option<Effect> {
    let found = profile
        .as_deref()
        .and_then(|name| lirtup_config::resolve::find_profile(&store.config.file, name))
        .map(|(name, _)| ProfileRef::Named(name.to_owned()))
        .unwrap_or(ProfileRef::Default);
    let Scope::Profile(target) = effective_scope(&Scope::Profile(found), &key) else {
        return None;
    };
    store.config.edits.shift_remove(&(target.clone(), key.clone()));
    let done = match &target {
        ProfileRef::Default => toast("profile-toast-setting-default", Vec::new()),
        ProfileRef::Named(name) => toast("profile-toast-setting", vec![("name", name.clone())]),
    };
    write_structure(store, done, move |file, _| {
        let layer = match &target {
            ProfileRef::Default => Some(&mut file.general),
            ProfileRef::Named(name) => file.profiles.get_mut(name).map(|p| &mut p.options),
        };
        if let Some(layer) = layer {
            set_in(layer, &key, Some(value));
        }
    })
}

/// «Crear» of «Nuevo perfil». The dialog validated the name; a name that
/// is empty or taken does nothing.
fn create_profile(store: &mut Store, new: super::NewProfile) -> Option<Effect> {
    let name = new.name.trim().to_owned();
    if name.is_empty() || name_taken(&store.config.file, &name) {
        return None;
    }
    let mut profile = match &new.copy_from {
        Some(ProfileRef::Named(from)) => store.config.file.profiles.get(from).cloned().unwrap_or_default(),
        // Copying Predeterminado = inheriting it.
        _ => Profile::default(),
    };
    profile.extra.shift_remove("icon");
    profile.matcher = new.matcher.filter(|m| !m.is_empty());
    match new.mode {
        Some(mode) => set_in(&mut profile.options, "mode", Some(Value::String(mode))),
        None if new.copy_from.is_none() => set_in(&mut profile.options, "mode", None),
        None => {}
    }
    if new.icon.is_some() {
        let path = icon_path(&store.config.file.path, &name);
        profile
            .extra
            .insert("icon".into(), Value::String(path.to_string_lossy().into_owned()));
    }
    store.dialog = None;
    store.route = Route::Profile {
        id: ProfileRef::Named(name.clone()),
        tab: ProfileTab::default(),
    };
    let done = toast("profile-toast-created", vec![("name", name.clone())]);
    write_structure(store, done, |file, _| {
        file.profiles.insert(name, profile);
    })
}

/// The profile as `<name> (2)`, right after the original.
fn duplicate_profile(store: &mut Store, name: ProfileName) -> Option<Effect> {
    let index = store.config.file.profiles.get_index_of(&name)?;
    let copy = copy_name(|n| name_taken(&store.config.file, n), &name);
    let profile = store.config.file.profiles[index].clone();
    store.route = Route::Profile {
        id: ProfileRef::Named(copy.clone()),
        tab: ProfileTab::default(),
    };
    let done = toast("profile-toast-duplicated", vec![("name", copy.clone())]);
    write_structure(store, done, |file, _| {
        file.profiles.shift_insert(index + 1, copy, profile);
    })
}

fn delete_profile(store: &mut Store, name: ProfileName) -> Option<Effect> {
    if !store.config.file.profiles.contains_key(&name) {
        store.dialog = None;
        return None;
    }
    store.dialog = None;
    let gone = ProfileRef::Named(name.clone());
    if store.route.profile() == Some(&gone) {
        store.route = Route::Profile {
            id: ProfileRef::Default,
            tab: ProfileTab::default(),
        };
    }
    let done = toast("profile-toast-deleted", vec![("name", name.clone())]);
    write_structure(store, done, |file, edits| {
        file.profiles.shift_remove(&name);
        edits.retain(|(p, _), _| *p != gone);
    })
}

fn rename_profile(store: &mut Store, from: ProfileName, to: ProfileName) -> Option<Effect> {
    let to = to.trim().to_owned();
    let index = store.config.file.profiles.get_index_of(&from)?;
    let same_name = to.to_lowercase() == from.to_lowercase();
    if to.is_empty() || to == from || (!same_name && name_taken(&store.config.file, &to)) {
        return None;
    }
    let old = ProfileRef::Named(from.clone());
    let new = ProfileRef::Named(to.clone());
    if let Route::Profile { id, .. } = &mut store.route
        && *id == old
    {
        *id = new.clone();
    }
    let done = toast("profile-toast-renamed", vec![("name", to.clone())]);
    write_structure(store, done, |file, edits| {
        let profiles = std::mem::take(&mut file.profiles);
        file.profiles = profiles
            .into_iter()
            .enumerate()
            .map(|(i, (n, p))| if i == index { (to.clone(), p) } else { (n, p) })
            .collect();
        *edits = std::mem::take(edits)
            .into_iter()
            .map(|((p, k), v)| {
                if p == old {
                    ((new.clone(), k), v)
                } else {
                    ((p, k), v)
                }
            })
            .collect();
    })
}

fn create_mode(store: &mut Store, name: String, copy_from: Option<ModeId>) -> Option<Effect> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return None;
    }
    let id = store.modes.store.new_id(&name);
    // A copy keeps exactly the keys its source sets; a new mode sets none.
    let source = copy_from
        .as_ref()
        .and_then(|from| store.modes.store.raw.get(from))
        .cloned()
        .unwrap_or_default();
    let raw = named_raw(source, &name);
    write_mode(
        store,
        id.clone(),
        raw,
        toast("profile-toast-mode-created", vec![("name", name)]),
    );
    store.route = Route::Modes { id: Some(id.clone()) };
    let mode = store.modes.store.get(&id).cloned()?;
    let raw = store.modes.store.raw.get(&id).cloned()?;
    Some(Effect::WriteMode { id, mode, raw })
}

/// Put a mode in the store as it will be on disk and count the write.
fn write_mode(store: &mut Store, id: ModeId, raw: Mapping, done: Toast) {
    let before = snapshot(store);
    let (mode, _) = Mode::from_mapping(&id, &raw);
    store.modes.store.raw.insert(id.clone(), raw);
    store.modes.store.modes.insert(id, mode);
    store.modes.store.modes.sort_keys();
    store.modes.store.raw.sort_keys();
    begin_saving(store, before, 1, done, None, false);
}

/// «Duplicar antes de editar»: the copy takes the unsaved edits of the
/// original (the person meant them for the copy) and, if asked, the profile.
fn duplicate_mode(store: &mut Store, id: ModeId, for_profile: Option<ProfileRef>) -> Option<Effect> {
    let original = store.modes.store.get(&id)?.clone();
    let name = copy_name(
        |n| store.modes.store.modes.values().any(|m| m.name == n),
        &original.name,
    );
    let copy_id = store.modes.store.new_id(&name);
    let raw = named_raw(store.modes.store.raw.get(&id).cloned().unwrap_or_default(), &name);
    write_mode(
        store,
        copy_id.clone(),
        raw,
        toast("profile-toast-mode-created", vec![("name", name)]),
    );
    store.modes.edits = std::mem::take(&mut store.modes.edits)
        .into_iter()
        .map(|((m, k), v)| {
            if m == id {
                ((copy_id.clone(), k), v)
            } else {
                ((m, k), v)
            }
        })
        .collect();
    if let Some(profile) = for_profile {
        store.set_edit(
            &Scope::Profile(profile),
            "mode",
            Some(Value::String(copy_id.clone())),
        );
    }
    store.route = Route::Modes {
        id: Some(copy_id.clone()),
    };
    let mode = store.modes.store.get(&copy_id).cloned()?;
    let raw = store.modes.store.raw.get(&copy_id).cloned()?;
    Some(Effect::WriteMode {
        id: copy_id,
        mode,
        raw,
    })
}

fn delete_mode(store: &mut Store, id: ModeId) -> Option<Effect> {
    store.dialog = None;
    let mode = store.modes.store.get(&id)?.clone();
    let before = snapshot(store);
    store.modes.store.modes.shift_remove(&id);
    store.modes.store.raw.shift_remove(&id);
    store.modes.edits.retain(|(m, _), _| *m != id);
    if store.route.mode() == Some(id.as_str()) {
        store.route = Route::Modes { id: None };
    }
    let done = toast("profile-toast-mode-deleted", vec![("name", mode.name)]);
    begin_saving(store, before, 1, done, None, false);
    Some(Effect::DeleteModeFile(id))
}

/// Restablecer ▸ the three choices.
fn reset(store: &mut Store, action: Action) {
    match action {
        Action::ResetTab { profile, tab } => {
            for key in tab_keys(tab) {
                let scope = effective_scope(&Scope::Profile(profile.clone()), key);
                store.revert(&scope, key);
            }
        }
        Action::RemoveProfileOverrides { profile, tab } => {
            let scope = Scope::Profile(profile.clone());
            for key in tab_keys(tab) {
                // A profile page does not remove Predeterminado's globals.
                if effective_scope(&scope, key) == scope {
                    store.set_edit(&scope, key, None);
                }
            }
        }
        Action::ResetAll { profile } => {
            let scope = Scope::Profile(profile);
            let keys: Vec<&str> = known_keys()
                // Hotkeys are edited in Ajustes, not on a profile page.
                .filter(|key| *key != HOTKEYS)
                .filter(|key| effective_scope(&scope, key) == scope)
                .collect();
            for key in keys {
                store.set_edit(&scope, key, None);
            }
        }
        _ => {}
    }
}

/// Send `WatchGpuTimes` when «window visible, session running and route
/// Inicio» changes (the session card is the one who reads GPU times; the
/// stats panel asks the core itself). A new connection starts at off.
fn gpu_times(store: &mut Store) -> Option<Effect> {
    let connected = matches!(store.link, LinkState::Connected { .. });
    let want = connected
        && store.visible
        && store.route == Route::Home
        && matches!(store.session, SessionState::Running(_));
    if want == store.gpu_times {
        return None;
    }
    store.gpu_times = want;
    connected.then(|| Effect::Send(Command::WatchGpuTimes { on: want }))
}

/// See [`super::update`].
pub(super) fn update(store: &mut Store, action: Action) -> Option<Effect> {
    let effect = match action {
        // ---- navigation and dialogs -------------------------------------
        Action::Navigate(to) => {
            if !store.route.same_page(&to) && store.page_edits(&store.route) > 0 {
                store.dialog = Some(Dialog::Unsaved { to });
            } else {
                store.route = to;
            }
            None
        }
        Action::OpenDialog(dialog) => {
            if matches!(dialog, Dialog::NewProfile { .. }) {
                store.picked = None;
            }
            store.dialog = Some(dialog);
            None
        }
        Action::CloseDialog => {
            store.dialog = None;
            None
        }
        Action::Unsaved(choice) => {
            let Some(Dialog::Unsaved { to }) = store.dialog.take() else {
                return None;
            };
            match choice {
                UnsavedChoice::Cancel => None,
                UnsavedChoice::SaveAndGo => {
                    let effect = save(store, false);
                    store.route = to;
                    effect
                }
                UnsavedChoice::DiscardAndGo => {
                    let from = store.route.clone();
                    discard_page(store, &from);
                    store.route = to;
                    None
                }
            }
        }
        Action::DismissToast => {
            store.toast = None;
            None
        }

        // ---- settings ----------------------------------------------------
        Action::SetField { scope, key, value } => {
            let scope = effective_scope(&scope, &key);
            store.set_edit(&scope, &key, Some(value));
            None
        }
        Action::Inherit { scope, key } => {
            let scope = effective_scope(&scope, &key);
            store.set_edit(&scope, &key, None);
            None
        }
        Action::Revert { scope, key } => {
            let scope = effective_scope(&scope, &key);
            store.revert(&scope, &key);
            None
        }
        action @ (Action::ResetTab { .. }
        | Action::RemoveProfileOverrides { .. }
        | Action::ResetAll { .. }) => {
            reset(store, action);
            None
        }
        Action::Save => save(store, false),
        Action::SaveAndRestart => save(store, true),
        Action::Undo => match store.undo.take() {
            None => None,
            Some(previous) => {
                let before = (store.config.clone(), store.modes.clone(), Some(previous.clone()));
                store.config.file = previous;
                let file = store.config.file.clone();
                store.config.edits.retain(|(p, _), _| match p {
                    ProfileRef::Default => true,
                    ProfileRef::Named(name) => file.profiles.contains_key(name),
                });
                store.toast = None;
                begin_saving(store, before, 1, toast("toast-undone", Vec::new()), None, false);
                Some(Effect::WriteConfig(file))
            }
        },

        // ---- profiles and modes ------------------------------------------
        Action::CreateProfile(new) => create_profile(store, new),
        Action::DuplicateProfile(name) => duplicate_profile(store, name),
        Action::DeleteProfile(name) => delete_profile(store, name),
        Action::RenameProfile { from, to } => rename_profile(store, from, to),
        Action::SetMatch { profile, matcher } => {
            let value = matcher
                .filter(|m| !m.is_empty())
                .and_then(|m| serde_yaml_ng::to_value(m).ok());
            store.set_edit(&Scope::Profile(ProfileRef::Named(profile)), MATCH_KEY, value);
            None
        }
        Action::CreateMode { name, copy_from } => create_mode(store, name, copy_from),
        Action::DuplicateMode { id, for_profile } => duplicate_mode(store, id, for_profile),
        Action::DeleteMode(id) => delete_mode(store, id),

        // ---- the core ----------------------------------------------------
        Action::StartScaling {
            target,
            kind,
            delay_s,
            profile,
        } => send(Command::Start {
            target,
            kind,
            delay_s,
            profile,
        }),
        Action::StopScaling => {
            // Detener also closes the error card (there is nothing to stop).
            if matches!(store.session, SessionState::Failed { .. }) {
                store.session = SessionState::Idle;
            }
            send(Command::Stop)
        }
        Action::RestartScaling => send(Command::Restart),
        Action::SetDaemon { on } => send(Command::SetDaemon {
            on,
            keep_after_exit: store.gui.keep_running_after_exit,
        }),
        Action::PickWindow => {
            store.picked = None;
            send(Command::PickWindow)
        }
        Action::ListEffects => send(Command::ListEffects),
        Action::CompileEffect(name) => {
            store.compiling.insert(
                name.clone(),
                CompileState {
                    done: false,
                    error: None,
                    session: false,
                },
            );
            send(Command::CompileEffect { name })
        }
        Action::ListDevices => send(Command::ListDevices),
        Action::ProbeFrameGen => send(Command::FrameGenProbe),
        Action::RunBench(req) => {
            store.bench = BenchView {
                request: Some(req.clone()),
                ..BenchView::default()
            };
            send(Command::Bench { req })
        }
        Action::CancelBench => send(Command::CancelBench),
        Action::RequestDiagnostics => send(Command::Diagnostics),
        Action::Core(event) => core(store, event),
        Action::Link(link) => {
            if !matches!(link, LinkState::Connected { .. }) {
                // The server forgets a closed connection's WatchGpuTimes.
                store.gpu_times = false;
            }
            store.link = link;
            None
        }

        // ---- the GUI itself ----------------------------------------------
        Action::Windows(windows) => {
            store.windows = windows;
            None
        }
        Action::WindowVisible(visible) => {
            store.visible = visible;
            None
        }
        Action::SetGui(gui) => {
            let keep_changed = gui.keep_running_after_exit != store.gui.keep_running_after_exit;
            store.gui = gui.clone();
            let mut effect = Some(Effect::WriteGui(gui));
            if keep_changed && store.daemon.on {
                effect = join(
                    effect,
                    send(Command::SetDaemon {
                        on: true,
                        keep_after_exit: store.gui.keep_running_after_exit,
                    }),
                );
            }
            effect
        }
        Action::Loaded { config, modes, gui } => {
            // The first read of the files (startup), not a reload.
            let first = store.config.file.path.as_os_str().is_empty();
            let mut edits = std::mem::take(&mut store.config.edits);
            edits.retain(|(p, _), _| match p {
                ProfileRef::Default => true,
                ProfileRef::Named(name) => config.profiles.contains_key(name),
            });
            let mut mode_edits = std::mem::take(&mut store.modes.edits);
            mode_edits.retain(|(m, _), _| modes.modes.contains_key(m));
            store.config = ConfigDoc { file: config, edits };
            store.modes = ModesDoc {
                store: modes,
                edits: mode_edits,
                users: IndexMap::new(),
            };
            store.gui = gui;
            if first {
                offer_migration(store);
            }
            None
        }
        Action::Written(result) => written(store, result),
        Action::Migrate => match store.dialog.take() {
            Some(Dialog::Migrate(plan)) => Some(Effect::Migrate(plan)),
            other => {
                store.dialog = other;
                None
            }
        },
        Action::SkipMigration => {
            if matches!(store.dialog, Some(Dialog::Migrate(_))) {
                store.dialog = None;
            }
            store.gui.migration_declined = true;
            Some(Effect::WriteGui(store.gui.clone()))
        }
        Action::Migrated(Ok(backup)) => {
            // «Deshacer» would bring back the loose fields over the modes.
            store.undo = None;
            show(store, toast("migrate-done", vec![("backup", backup)]))
        }
        Action::Migrated(Err(error)) => show(store, toast("migrate-failed", vec![("error", error)])),
    };

    // A profile page whose profile is gone (deleted, a failed create, a
    // reload) shows Predeterminado.
    if let Some(profile) = store.route.profile()
        && !profile_exists(store, profile)
    {
        store.route = Route::Profile {
            id: ProfileRef::Default,
            tab: ProfileTab::default(),
        };
    }
    store.refresh_users();
    join(effect, gpu_times(store))
}

#[cfg(test)]
mod tests {
    use lirtup_config::{GuiConfig, Match};
    use lirtup_proto::{
        FrameGenStatus, Level, SessionKind, SessionView, Size, StopReason, Target, WindowRef,
    };

    use super::*;
    use crate::route::SettingsTab;

    /// A `config.yaml` as 1.x writes it: top-level options, then profiles
    /// with `match`, `options` and keys 2.0 does not know.
    const FILE_1X: &str = "\
model: 4x24
follow_focus: false
max_fps: 60
hotkeys:
  zoom_in: Ctrl+Plus
future_key: keep me
profiles:
  BALDR SKY:
    match:
      title_contains: BALDR SKY
      width: 800
    options:
      overlay_mode: fullscreen
      crop_top: 20
      follow_focus: true
      unknown_option: 3
    icon: /home/x/.config/linux-rt-upscaler/icons/BALDR SKY.png
  Firefox:
    match:
      title_regex: .*Firefox.*
    options:
      model: fast
";

    /// A store with `yaml` read from a temporary `config.yaml`.
    fn store_with(yaml: &str) -> Store {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "lirtup-ui-store-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        let (config, warnings) = ConfigFile::load(&path).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let mut store = Store::default();
        update(
            &mut store,
            Action::Loaded {
                config,
                modes: ModeStore::default(),
                // FILE_1X has loose mode fields: without this the migration
                // dialog opens on load (its own tests are at the end).
                gui: GuiConfig {
                    migration_declined: true,
                    ..GuiConfig::default()
                },
            },
        );
        store
    }

    fn baldr() -> ProfileRef {
        ProfileRef::Named("BALDR SKY".into())
    }

    fn scope(p: ProfileRef) -> Scope {
        Scope::Profile(p)
    }

    fn set(store: &mut Store, p: ProfileRef, key: &str, value: impl Into<Value>) -> Option<Effect> {
        update(
            store,
            Action::SetField {
                scope: scope(p),
                key: key.into(),
                value: value.into(),
            },
        )
    }

    fn page(p: ProfileRef, tab: ProfileTab) -> Route {
        Route::Profile { id: p, tab }
    }

    fn effects(effect: Option<Effect>) -> Vec<Effect> {
        match effect {
            None => Vec::new(),
            Some(Effect::Many(list)) => list.into_iter().flat_map(|e| effects(Some(e))).collect(),
            Some(e) => vec![e],
        }
    }

    fn written_config(effect: Option<Effect>) -> ConfigFile {
        effects(effect)
            .into_iter()
            .find_map(|e| match e {
                Effect::WriteConfig(file) => Some(file),
                _ => None,
            })
            .expect("a WriteConfig")
    }

    fn sends(effect: &Option<Effect>, command: &Command) -> bool {
        effects(effect.clone()).contains(&Effect::Send(command.clone()))
    }

    fn session() -> SessionView {
        SessionView {
            window: WindowRef {
                xid: 7,
                title: "BALDR SKY".into(),
                class: None,
                pid: None,
            },
            profile: Some("baldr sky".into()),
            mode: None,
            kind: SessionKind::Fullscreen,
            src: Size {
                width: 800,
                height: 600,
            },
            dst: Size {
                width: 1920,
                height: 1080,
            },
            chain: Vec::new(),
            started_at_ms: 0,
        }
    }

    // ---- Field ---------------------------------------------------------

    #[test]
    fn field_value_origin_and_dirty() {
        let inherited = Field {
            default: 1,
            profile: None,
            edit: None,
        };
        assert_eq!(inherited.value(), 1);
        assert_eq!(inherited.origin(|v| v.to_string()), Origin::Inherited);

        let overridden = Field {
            default: 1,
            profile: Some(2),
            edit: None,
        };
        assert_eq!(overridden.value(), 2);
        assert_eq!(overridden.origin(|v| v.to_string()), Origin::Overridden);

        let edited = Field {
            default: 1,
            profile: Some(2),
            edit: Some(Some(3)),
        };
        assert_eq!(edited.value(), 3);
        assert!(edited.is_dirty());
        assert_eq!(
            edited.origin(|v| v.to_string()),
            Origin::Dirty { before: "2".into() }
        );

        let inherit = Field {
            default: 1,
            profile: Some(2),
            edit: Some(None),
        };
        assert_eq!(inherit.value(), 1);
        assert_eq!(
            inherit.origin(|v| v.to_string()),
            Origin::Dirty { before: "2".into() }
        );

        // An edit that gives back the saved value is not dirty.
        let back = Field {
            default: 1,
            profile: Some(2),
            edit: Some(Some(2)),
        };
        assert!(!back.is_dirty());
        let noop_inherit = Field {
            default: 1,
            profile: None,
            edit: Some(None),
        };
        assert!(!noop_inherit.is_dirty());
        assert_eq!(noop_inherit.origin(|v| v.to_string()), Origin::Inherited);
    }

    #[test]
    fn tabs_cover_their_keys_once() {
        for tab in ProfileTab::ALL {
            for key in tab_keys(tab) {
                assert_eq!(tab_of(key), Some(tab), "{key}");
                assert!(system_defaults().contains_key(*key), "{key} is not a Config key");
            }
        }
        assert_eq!(tab_of("model"), None);
        for key in global_keys() {
            assert!(system_defaults().contains_key(*key), "{key} is not a Config key");
        }
    }

    // ---- inheritance ---------------------------------------------------------

    #[test]
    fn inheritance_by_layer() {
        let store = store_with(FILE_1X);
        // Predeterminado over the system defaults.
        let max_fps: Field<Option<u32>> = store.field(&scope(ProfileRef::Default), "max_fps");
        assert_eq!(max_fps.default, None);
        assert_eq!(max_fps.value(), Some(60));
        assert_eq!(
            store.origin(&scope(ProfileRef::Default), "max_fps"),
            Origin::Overridden
        );
        assert_eq!(
            store.origin(&scope(ProfileRef::Default), "show_osd"),
            Origin::Inherited
        );

        // A profile over Predeterminado.
        let fps: Field<Option<u32>> = store.field(&scope(baldr()), "max_fps");
        assert_eq!((fps.default, fps.profile), (Some(60), None));
        assert_eq!(store.origin(&scope(baldr()), "max_fps"), Origin::Inherited);
        let focus: Field<bool> = store.field(&scope(baldr()), "follow_focus");
        assert_eq!((focus.default, focus.value()), (false, true));
        assert_eq!(store.origin(&scope(baldr()), "follow_focus"), Origin::Overridden);

        // hotkeys key by key.
        let zoom: Field<String> = store.field(&scope(ProfileRef::Default), "hotkeys.zoom_in");
        assert_eq!(zoom.value(), "Ctrl+Plus");
        let toggle: Field<String> = store.field(&scope(ProfileRef::Default), "hotkeys.toggle_scaling");
        assert_eq!((toggle.profile, toggle.default.as_str()), (None, "Alt+Shift+S"));
    }

    #[test]
    fn modes_sit_where_the_resolver_puts_them() {
        let mut store =
            store_with("mode: anime\nmodel: faster\nprofiles:\n  A:\n    options: {mode: crt}\n  B: {}\n");
        let mut modes = ModeStore::default();
        for (id, yaml) in [
            ("anime", "name: Anime\nmodel: 4x24\ncas_enabled: true\n"),
            ("crt", "name: CRT\ncas_enabled: false\n"),
        ] {
            let raw: Mapping = serde_yaml_ng::from_str(yaml).unwrap();
            modes.modes.insert(id.into(), Mode::from_mapping(id, &raw).0);
            modes.raw.insert(id.into(), raw);
        }
        let config = store.config.file.clone();
        update(
            &mut store,
            Action::Loaded {
                config,
                modes,
                gui: GuiConfig::default(),
            },
        );

        // Predeterminado's loose `model` wins over its mode.
        let model: Field<String> = store.field(&scope(ProfileRef::Default), "model");
        assert_eq!(
            (model.default.as_str(), model.value().as_str()),
            ("4x24", "faster")
        );
        let cas: Field<bool> = store.field(&scope(ProfileRef::Default), "cas_enabled");
        assert!(cas.value());
        // A profile with its own mode: the mode is above Predeterminado's
        // loose fields, and Predeterminado's mode is not used.
        let cas: Field<bool> = store.field(&scope(ProfileRef::Named("A".into())), "cas_enabled");
        assert!(!cas.value());
        let model: Field<String> = store.field(&scope(ProfileRef::Named("A".into())), "model");
        assert_eq!(model.value(), "faster");
        // A profile without one inherits Predeterminado (and its mode).
        let cas: Field<bool> = store.field(&scope(ProfileRef::Named("B".into())), "cas_enabled");
        assert!(cas.value());
        assert_eq!(
            store.mode_of(&ProfileRef::Named("B".into())).as_deref(),
            Some("anime")
        );
        assert_eq!(
            store.mode_of(&ProfileRef::Named("A".into())).as_deref(),
            Some("crt")
        );
        assert_eq!(store.modes.users["anime"], [ProfileRef::Default]);
        assert_eq!(store.modes.users["crt"], [ProfileRef::Named("A".into())]);
    }

    #[test]
    fn null_means_inherit() {
        let store = store_with("max_fps: 30\nprofiles:\n  A:\n    options: {max_fps: null}\n");
        let fps: Field<Option<u32>> = store.field(&scope(ProfileRef::Named("A".into())), "max_fps");
        assert_eq!((fps.profile, fps.value()), (None, Some(30)));
    }

    // ---- dirty: row, tab, drawer ------------------------------------------------

    #[test]
    fn dirty_in_row_tab_and_drawer() {
        let mut store = store_with(FILE_1X);
        set(&mut store, baldr(), "overlay_mode", "windowed");
        assert_eq!(
            store.origin(&scope(baldr()), "overlay_mode"),
            Origin::Dirty {
                before: "fullscreen".into()
            }
        );
        assert!(store.tab_dirty(&baldr(), ProfileTab::Presentation));
        assert!(!store.tab_dirty(&baldr(), ProfileTab::Scaling));
        assert!(store.profile_dirty(&baldr()));
        assert!(!store.profile_dirty(&ProfileRef::Default));
        assert_eq!(store.dirty_count(&scope(baldr())), 1);
        assert!(store.has_edits());

        // Setting the saved value back clears all three.
        set(&mut store, baldr(), "overlay_mode", "fullscreen");
        assert_eq!(store.origin(&scope(baldr()), "overlay_mode"), Origin::Overridden);
        assert!(!store.tab_dirty(&baldr(), ProfileTab::Presentation));
        assert!(!store.profile_dirty(&baldr()));
        assert!(!store.has_edits());

        // Numbers compare by value.
        set(&mut store, baldr(), "crop_top", 20.0);
        assert!(!store.has_edits());

        // Inherit and Revert.
        update(
            &mut store,
            Action::Inherit {
                scope: scope(baldr()),
                key: "crop_top".into(),
            },
        );
        assert_eq!(
            store.origin(&scope(baldr()), "crop_top"),
            Origin::Dirty { before: "20".into() }
        );
        assert_eq!(store.field::<u32>(&scope(baldr()), "crop_top").value(), 0);
        update(
            &mut store,
            Action::Revert {
                scope: scope(baldr()),
                key: "crop_top".into(),
            },
        );
        assert!(!store.has_edits());
        // Inheriting what is already inherited is no edit.
        update(
            &mut store,
            Action::Inherit {
                scope: scope(baldr()),
                key: "max_fps".into(),
            },
        );
        assert!(!store.has_edits());
    }

    #[test]
    fn edits_of_the_layer_below_show_through() {
        let mut store = store_with(FILE_1X);
        set(&mut store, ProfileRef::Default, "show_osd", false);
        let osd: Field<bool> = store.field(&scope(baldr()), "show_osd");
        assert!(!osd.value());
        assert_eq!(store.origin(&scope(baldr()), "show_osd"), Origin::Inherited);
        assert!(store.profile_dirty(&ProfileRef::Default));
        assert!(!store.profile_dirty(&baldr()));
    }

    #[test]
    fn global_keys_go_to_predeterminado() {
        let mut store = store_with(FILE_1X);
        store.route = page(baldr(), ProfileTab::Presentation);
        set(&mut store, baldr(), "frame_gen_multiplier", 3);
        assert!(
            store
                .config
                .edits
                .contains_key(&(ProfileRef::Default, "frame_gen_multiplier".into()))
        );
        assert!(!store.profile_dirty(&baldr()));
        assert!(store.profile_dirty(&ProfileRef::Default));
        // ...but the row, the tab and the page of the profile show it.
        assert!(matches!(
            store.origin(&scope(baldr()), "frame_gen_multiplier"),
            Origin::Dirty { .. }
        ));
        assert!(store.tab_dirty(&baldr(), ProfileTab::Presentation));
        assert_eq!(store.page_edits(&store.route), 1);
        assert_eq!(
            crate::screens::profile::save_bar_scope(&store),
            Some((scope(baldr()), 1))
        );
        let file = apply_edits(&store.config.file, &store.config.edits);
        assert_eq!(file.general["frame_gen_multiplier"], Value::from(3));
        assert!(
            !file.profiles["BALDR SKY"]
                .options
                .contains_key("frame_gen_multiplier")
        );
    }

    // ---- navigation and «Cambios sin guardar» ----------------------------------

    #[test]
    fn leaving_a_page_with_edits_asks() {
        let mut store = store_with(FILE_1X);
        store.route = page(baldr(), ProfileTab::Scaling);
        set(&mut store, baldr(), "follow_focus", false);

        // Another tab of the same profile: no question.
        update(&mut store, Action::Navigate(page(baldr(), ProfileTab::Display)));
        assert_eq!(store.route, page(baldr(), ProfileTab::Display));
        assert_eq!(store.dialog, None);

        // Another page: the dialog, and the route stays.
        update(&mut store, Action::Navigate(Route::Home));
        assert_eq!(store.dialog, Some(Dialog::Unsaved { to: Route::Home }));
        assert_eq!(store.route, page(baldr(), ProfileTab::Display));

        // Cancelar.
        update(&mut store, Action::Unsaved(UnsavedChoice::Cancel));
        assert_eq!(store.dialog, None);
        assert!(store.has_edits());
        assert_eq!(store.route, page(baldr(), ProfileTab::Display));

        // Descartar y cambiar.
        update(&mut store, Action::Navigate(Route::Home));
        update(&mut store, Action::Unsaved(UnsavedChoice::DiscardAndGo));
        assert!(!store.has_edits());
        assert_eq!(store.route, Route::Home);

        // Without edits, no question.
        update(
            &mut store,
            Action::Navigate(Route::Settings {
                tab: SettingsTab::General,
            }),
        );
        assert_eq!(store.dialog, None);
    }

    #[test]
    fn save_and_go_saves_then_navigates() {
        let mut store = store_with(FILE_1X);
        store.route = page(baldr(), ProfileTab::Scaling);
        set(&mut store, baldr(), "follow_focus", false);
        update(&mut store, Action::Navigate(Route::Home));
        let effect = update(&mut store, Action::Unsaved(UnsavedChoice::SaveAndGo));
        let file = written_config(effect);
        assert_eq!(
            file.profiles["BALDR SKY"].options["follow_focus"],
            Value::Bool(false)
        );
        assert_eq!(store.route, Route::Home);
        assert_eq!(store.dialog, None);
        assert!(!store.has_edits());
    }

    #[test]
    fn editing_a_mode_page_asks_too() {
        let mut store = Store::default();
        let raw: Mapping = serde_yaml_ng::from_str("name: Anime\nmodel: 4x24\n").unwrap();
        store
            .modes
            .store
            .modes
            .insert("anime".into(), Mode::from_mapping("anime", &raw).0);
        store.modes.store.raw.insert("anime".into(), raw);
        store.route = Route::Modes {
            id: Some("anime".into()),
        };
        update(
            &mut store,
            Action::SetField {
                scope: Scope::Mode("anime".into()),
                key: "cas_enabled".into(),
                value: true.into(),
            },
        );
        assert_eq!(store.dirty_count(&Scope::Mode("anime".into())), 1);
        update(&mut store, Action::Navigate(Route::Home));
        assert!(matches!(store.dialog, Some(Dialog::Unsaved { .. })));
    }

    // ---- Guardar, Deshacer, errors -------------------------------------------

    #[test]
    fn save_writes_then_reloads_and_offers_undo() {
        let mut store = store_with(FILE_1X);
        store.route = page(baldr(), ProfileTab::Presentation);
        let before = store.config.file.clone();
        set(&mut store, baldr(), "overlay_mode", "windowed");

        let effect = update(&mut store, Action::Save);
        assert!(
            !sends(&effect, &Command::ReloadConfig),
            "ReloadConfig only after the write"
        );
        let file = written_config(effect);
        assert_eq!(
            file.profiles["BALDR SKY"].options["overlay_mode"],
            Value::from("windowed")
        );
        // The store shows the result at once.
        assert!(!store.has_edits());
        assert_eq!(store.config.file, file);
        assert_eq!(store.origin(&scope(baldr()), "overlay_mode"), Origin::Overridden);
        assert!(store.saving.is_some());

        let effect = update(&mut store, Action::Written(Ok(Written::Config)));
        assert!(sends(&effect, &Command::ReloadConfig));
        let toast = store.toast.clone().unwrap();
        assert_eq!(toast.key, "toast-saved");
        assert_eq!(toast.args, [("name", "BALDR SKY".to_string())]);
        assert!(toast.undo);
        assert!(effects(effect).contains(&Effect::Toast(toast)));
        assert_eq!(store.undo.as_ref(), Some(&before));
        assert_eq!(store.saving, None);

        // Deshacer writes the file before the save back.
        let effect = update(&mut store, Action::Undo);
        assert_eq!(written_config(effect), before);
        assert_eq!(store.config.file, before);
        assert_eq!(store.toast, None);
        let effect = update(&mut store, Action::Written(Ok(Written::Config)));
        assert!(sends(&effect, &Command::ReloadConfig));
        assert_eq!(store.toast.as_ref().unwrap().key, "toast-undone");
        assert!(!store.toast.as_ref().unwrap().undo);
        assert_eq!(store.undo, None);
        assert_eq!(update(&mut store, Action::Undo), None);
    }

    #[test]
    fn a_failed_write_gives_the_edits_back() {
        let mut store = store_with(FILE_1X);
        let before = store.config.file.clone();
        set(&mut store, baldr(), "overlay_mode", "windowed");
        update(&mut store, Action::Save);
        let effect = update(&mut store, Action::Written(Err("disk full".into())));
        assert!(!sends(&effect, &Command::ReloadConfig));
        assert_eq!(store.config.file, before);
        assert!(store.profile_dirty(&baldr()));
        let toast = store.toast.clone().unwrap();
        assert_eq!(
            (toast.key, toast.args),
            ("toast-write-failed", vec![("error", "disk full".to_string())])
        );
        assert_eq!(store.undo, None);
    }

    #[test]
    fn save_without_edits_does_nothing() {
        let mut store = store_with(FILE_1X);
        assert_eq!(update(&mut store, Action::Save), None);
        assert_eq!(store.saving, None);
    }

    #[test]
    fn save_writes_modes_too() {
        let mut store = Store::default();
        let raw: Mapping = serde_yaml_ng::from_str("name: Anime\nmodel: 4x24\n").unwrap();
        store
            .modes
            .store
            .modes
            .insert("anime".into(), Mode::from_mapping("anime", &raw).0);
        store.modes.store.raw.insert("anime".into(), raw);
        update(
            &mut store,
            Action::SetField {
                scope: Scope::Mode("anime".into()),
                key: "cas_enabled".into(),
                value: true.into(),
            },
        );
        let list = effects(update(&mut store, Action::Save));
        let [Effect::WriteMode { id, mode, raw }] = list.as_slice() else {
            panic!("{list:?}");
        };
        assert_eq!(id, "anime");
        assert_eq!(mode.name, "Anime");
        assert!(mode.settings.cas_enabled);
        assert_eq!(mode.settings.model, lirtup_config::Model::M4x24);
        // Only the keys the file had plus the edited one: the rest still
        // comes from the layers below.
        let mut keys: Vec<&str> = raw.keys().filter_map(|k| k.as_str()).collect();
        keys.sort_unstable();
        let mut expected: Vec<&str> = store.modes.store.raw["anime"]
            .keys()
            .filter_map(|k| k.as_str())
            .collect();
        expected.sort_unstable();
        assert_eq!(keys, expected);
        assert!(keys.contains(&"cas_enabled") && !keys.contains(&"crt_enabled"));
        let effect = update(&mut store, Action::Written(Ok(Written::Mode("anime".into()))));
        assert!(sends(&effect, &Command::ReloadConfig));
        assert_eq!(store.toast.as_ref().unwrap().key, "profile-toast-saved");
    }

    #[test]
    fn save_and_restart_restarts_after_the_reload() {
        let mut store = store_with(FILE_1X);
        store.session = SessionState::Running(session());
        set(&mut store, ProfileRef::Default, "frame_gen_multiplier", 2);
        let effect = update(&mut store, Action::SaveAndRestart);
        assert!(!sends(&effect, &Command::Restart));
        let list = effects(update(&mut store, Action::Written(Ok(Written::Config))));
        let reload = list
            .iter()
            .position(|e| *e == Effect::Send(Command::ReloadConfig))
            .unwrap();
        let restart = list
            .iter()
            .position(|e| *e == Effect::Send(Command::Restart))
            .unwrap();
        assert!(reload < restart);
    }

    // ---- 1.x files round trip --------------------------------------------------

    #[test]
    fn a_1x_file_saves_back_unchanged_but_for_the_edit() {
        let mut store = store_with(FILE_1X);
        let path = store.config.file.path.clone();
        let original = store.config.file.to_yaml().unwrap();
        // Unknown keys survive the read.
        assert!(store.config.file.general.contains_key("future_key"));

        set(&mut store, baldr(), "crop_top", 32);
        set(&mut store, ProfileRef::Default, "hotkeys.zoom_out", "Ctrl+Minus");
        let file = written_config(update(&mut store, Action::Save));
        file.save().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();

        // Exactly the two edits, in place; nothing else moved.
        let expected = original
            .replace("      crop_top: 20\n", "      crop_top: 32\n")
            .replace(
                "  zoom_in: Ctrl+Plus\n",
                "  zoom_in: Ctrl+Plus\n  zoom_out: Ctrl+Minus\n",
            );
        assert_eq!(text, expected);
        assert!(text.contains("unknown_option: 3") && text.contains("future_key: keep me"));
        assert!(text.contains("icon: /home/x/.config/linux-rt-upscaler/icons/BALDR SKY.png"));

        // Reading back gives the same file.
        let (again, _) = ConfigFile::load(&path).unwrap();
        assert_eq!(again, file);
    }

    #[test]
    fn the_1x_text_itself_reads_and_writes_back() {
        let store = store_with(FILE_1X);
        assert_eq!(store.config.file.to_yaml().unwrap(), FILE_1X);
    }

    /// The criterion of 5.8 on the person's own files: `LIRTUP_UI_CONFIG`
    /// names a copy of a real `config.yaml`; reading it and saving it
    /// without edits must give the same text. `cargo test -p lirtup-ui --
    /// --ignored real_config`.
    #[test]
    #[ignore = "needs LIRTUP_UI_CONFIG"]
    fn real_config_round_trip() {
        let path = std::env::var_os("LIRTUP_UI_CONFIG").expect("LIRTUP_UI_CONFIG");
        let text = std::fs::read_to_string(&path).unwrap();
        let store = store_with(&text);
        // Every row of every page reads (no panic on a real file).
        for profile in store.profiles() {
            let scope = scope(profile);
            for tab in ProfileTab::ALL {
                for key in tab_keys(tab) {
                    let _ = store.origin(&scope, key);
                }
            }
        }
        let saved = apply_edits(&store.config.file, &store.config.edits)
            .to_yaml()
            .unwrap();
        // Comments and formatting are not kept (1.x drops them too, with
        // `yaml.safe_dump`), nor the hotkeys equal to the defaults (1.x
        // `diff_hotkeys`); every other key and value is. A file 1.x wrote
        // has neither, and comes back as the same text.
        fn normalize(layer: &mut lirtup_config::Mapping) {
            use lirtup_config::Value;
            let Some(Value::Mapping(keys)) = layer.get("hotkeys") else {
                return;
            };
            let keys: std::collections::BTreeMap<String, String> = keys
                .iter()
                .filter_map(|(k, v)| Some((k.as_str()?.to_owned(), v.as_str()?.to_owned())))
                .collect();
            let diff = lirtup_config::hotkeys::diff_hotkeys(&keys);
            if diff.is_empty() {
                layer.remove("hotkeys");
            } else {
                layer.insert("hotkeys".into(), serde_yaml_ng::to_value(diff).unwrap());
            }
        }
        let value = |t: &str| {
            let mut top: lirtup_config::Mapping = serde_yaml_ng::from_str(t).unwrap();
            normalize(&mut top);
            if let Some(lirtup_config::Value::Mapping(profiles)) = top.get_mut("profiles") {
                for (_, p) in profiles.iter_mut() {
                    if let Some(lirtup_config::Value::Mapping(o)) = p.get_mut("options") {
                        normalize(o);
                    }
                }
            }
            top
        };
        assert_eq!(value(&saved), value(&text));
        if !text.lines().any(|l| l.trim_start().starts_with('#')) {
            assert_eq!(saved, text);
        }
    }

    // ---- Restablecer -------------------------------------------------------------

    #[test]
    fn reset_tab_undoes_only_that_tab() {
        let mut store = store_with(FILE_1X);
        set(&mut store, baldr(), "overlay_mode", "windowed");
        set(&mut store, baldr(), "frame_gen_multiplier", 2);
        set(&mut store, baldr(), "follow_focus", false);
        update(
            &mut store,
            Action::ResetTab {
                profile: baldr(),
                tab: ProfileTab::Presentation,
            },
        );
        assert!(!store.tab_dirty(&baldr(), ProfileTab::Presentation));
        assert!(store.tab_dirty(&baldr(), ProfileTab::Scaling));
        // The global edit shown in that tab goes too.
        assert!(!store.profile_dirty(&ProfileRef::Default));
    }

    #[test]
    fn remove_overrides_inherits_the_tab() {
        let mut store = store_with(FILE_1X);
        update(
            &mut store,
            Action::RemoveProfileOverrides {
                profile: baldr(),
                tab: ProfileTab::Presentation,
            },
        );
        assert_eq!(store.dirty_count(&scope(baldr())), 2); // overlay_mode, crop_top
        assert_eq!(store.field::<u32>(&scope(baldr()), "crop_top").value(), 0);
        assert_eq!(store.origin(&scope(baldr()), "follow_focus"), Origin::Overridden);
        let file = apply_edits(&store.config.file, &store.config.edits);
        let options = &file.profiles["BALDR SKY"].options;
        assert!(!options.contains_key("overlay_mode") && !options.contains_key("crop_top"));
        assert!(options.contains_key("follow_focus") && options.contains_key("unknown_option"));
    }

    #[test]
    fn reset_all_keeps_unknown_keys_and_hotkeys() {
        let mut store = store_with(FILE_1X);
        update(&mut store, Action::ResetAll { profile: baldr() });
        let file = apply_edits(&store.config.file, &store.config.edits);
        let profile = &file.profiles["BALDR SKY"];
        assert_eq!(profile.options.len(), 1, "{:?}", profile.options);
        assert!(profile.options.contains_key("unknown_option"));
        assert!(profile.matcher.is_some());

        update(
            &mut store,
            Action::ResetAll {
                profile: ProfileRef::Default,
            },
        );
        let file = apply_edits(&store.config.file, &store.config.edits);
        let keys: Vec<_> = file.general.keys().filter_map(Value::as_str).collect();
        assert_eq!(keys, ["hotkeys", "future_key"]);
    }

    // ---- profiles ---------------------------------------------------------------

    #[test]
    fn create_profile_writes_and_opens_it() {
        let mut store = store_with(FILE_1X);
        store.dialog = Some(Dialog::NewProfile { from: None });
        set(&mut store, baldr(), "follow_focus", false);
        let matcher = Match {
            title_contains: Some("Haruka".into()),
            ..Match::default()
        };
        let effect = update(
            &mut store,
            Action::CreateProfile(super::super::NewProfile {
                name: "Gin'iro, Haruka".into(),
                mode: Some("anime".into()),
                copy_from: Some(baldr()),
                matcher: Some(matcher.clone()),
                icon: Some(vec![1, 2, 3]),
            }),
        );
        let file = written_config(effect);
        let p = &file.profiles["Gin'iro, Haruka"];
        assert_eq!(p.matcher.as_ref(), Some(&matcher));
        assert_eq!(p.options["mode"], Value::from("anime"));
        assert_eq!(p.options["crop_top"], Value::from(20)); // copied
        assert!(
            p.extra["icon"]
                .as_str()
                .unwrap()
                .ends_with("icons/Gin_iro_ Haruka.png")
        );
        // The pending edit of another profile stays pending and unwritten.
        assert_eq!(
            file.profiles["BALDR SKY"].options["follow_focus"],
            Value::Bool(true)
        );
        assert!(store.profile_dirty(&baldr()));
        assert_eq!(store.dialog, None);
        assert_eq!(
            store.route,
            page(ProfileRef::Named("Gin'iro, Haruka".into()), ProfileTab::Scaling)
        );
        assert_eq!(store.profiles().len(), 4);

        // A taken name (any case) does nothing.
        let taken = super::super::NewProfile {
            name: "baldr sky".into(),
            mode: None,
            copy_from: None,
            matcher: None,
            icon: None,
        };
        assert_eq!(update(&mut store, Action::CreateProfile(taken)), None);
    }

    #[test]
    fn rename_duplicate_delete() {
        let mut store = store_with(FILE_1X);
        store.route = page(baldr(), ProfileTab::Display);
        set(&mut store, baldr(), "max_fps", 30);

        let file = written_config(update(
            &mut store,
            Action::RenameProfile {
                from: "BALDR SKY".into(),
                to: "Baldr Sky DC".into(),
            },
        ));
        assert_eq!(
            file.profiles.keys().collect::<Vec<_>>(),
            ["Baldr Sky DC", "Firefox"]
        );
        let renamed = ProfileRef::Named("Baldr Sky DC".into());
        assert_eq!(store.route, page(renamed.clone(), ProfileTab::Display));
        assert!(store.profile_dirty(&renamed), "the edits follow the name");
        update(&mut store, Action::Written(Ok(Written::Config)));

        let file = written_config(update(
            &mut store,
            Action::DuplicateProfile("Baldr Sky DC".into()),
        ));
        assert_eq!(
            file.profiles.keys().collect::<Vec<_>>(),
            ["Baldr Sky DC", "Baldr Sky DC (2)", "Firefox"]
        );
        assert_eq!(
            store.route,
            page(ProfileRef::Named("Baldr Sky DC (2)".into()), ProfileTab::Scaling)
        );
        update(&mut store, Action::Written(Ok(Written::Config)));

        store.route = page(renamed.clone(), ProfileTab::Scaling);
        store.dialog = Some(Dialog::DeleteProfile("Baldr Sky DC".into()));
        let file = written_config(update(&mut store, Action::DeleteProfile("Baldr Sky DC".into())));
        assert!(!file.profiles.contains_key("Baldr Sky DC"));
        assert_eq!(store.dialog, None);
        assert_eq!(store.route, page(ProfileRef::Default, ProfileTab::Scaling));
        assert!(!store.has_edits(), "its edits went with it");
    }

    #[test]
    fn set_match_is_an_edit_until_saved() {
        let mut store = store_with(FILE_1X);
        let matcher = Match {
            title: Some("BALDR SKY DC".into()),
            ..Match::default()
        };
        update(
            &mut store,
            Action::SetMatch {
                profile: "BALDR SKY".into(),
                matcher: Some(matcher.clone()),
            },
        );
        assert!(store.profile_dirty(&baldr()));
        assert_eq!(
            store.config.file.profiles["BALDR SKY"]
                .matcher
                .as_ref()
                .unwrap()
                .title,
            None
        );
        let file = written_config(update(&mut store, Action::Save));
        assert_eq!(file.profiles["BALDR SKY"].matcher, Some(matcher));
        update(
            &mut store,
            Action::SetMatch {
                profile: "BALDR SKY".into(),
                matcher: None,
            },
        );
        let file = written_config(update(&mut store, Action::Save));
        assert_eq!(file.profiles["BALDR SKY"].matcher, None);
    }

    #[test]
    fn matching_and_running_profile() {
        let mut store = store_with(FILE_1X);
        let window = |title: &str, w, h| WindowInfo {
            xid: 1,
            title: title.into(),
            rect: lirtup_x11::Rect {
                x: 0,
                y: 0,
                width: w,
                height: h,
            },
            pid: None,
            class: None,
        };
        assert_eq!(
            store.matching_profile(&window("BALDR SKY (18+ Patch)", 800, 600)),
            Some(baldr())
        );
        assert_eq!(store.matching_profile(&window("BALDR SKY", 1024, 768)), None);
        assert_eq!(
            store.matching_profile(&window("Mozilla Firefox", 10, 10)),
            Some(ProfileRef::Named("Firefox".into()))
        );
        assert_eq!(store.running_profile(), None);
        store.session = SessionState::Running(session());
        assert_eq!(store.running_profile(), Some(baldr()));
        store.windows = vec![
            window("BALDR SKY (18+ Patch)", 800, 600),
            window("Firefox", 800, 600),
        ];
        let m = Match {
            title_contains: Some("baldr".into()),
            ..Match::default()
        };
        assert_eq!(store.windows_matching(&m).len(), 1);
    }

    // ---- events of the core ----------------------------------------------------

    #[test]
    fn menu_bar_crop_is_saved_in_the_profile() {
        let mut store = store_with(FILE_1X);
        let crop = lirtup_config::Crop {
            top: 24,
            bottom: 0,
            left: 0,
            right: 0,
        };
        let effect = update(
            &mut store,
            Action::Core(Event::MenuBarDetected {
                profile: Some("baldr sky".into()),
                crop,
            }),
        );
        let file = written_config(effect);
        let options = &file.profiles["BALDR SKY"].options;
        assert_eq!(options["crop_top"], Value::from(24));
        assert_eq!(options["crop_bottom"], Value::from(0));
        assert_eq!(options["auto_crop_menu_bar"], Value::Bool(false));
        let effect = update(&mut store, Action::Written(Ok(Written::Config)));
        assert!(sends(&effect, &Command::ReloadConfig));

        // Without a profile, only for this session.
        let effect = update(
            &mut store,
            Action::Core(Event::MenuBarDetected { profile: None, crop }),
        );
        assert_eq!(effect, None);
    }

    #[test]
    fn save_setting_from_the_bar() {
        // A global key goes to Predeterminado, even from a profile.
        let mut store = store_with(FILE_1X);
        let effect = update(
            &mut store,
            Action::Core(Event::SaveSetting {
                profile: Some("BALDR SKY".into()),
                key: "frame_gen_multiplier".into(),
                value: Value::from(3),
            }),
        );
        let file = written_config(effect);
        assert_eq!(file.general["frame_gen_multiplier"], Value::from(3));
        assert!(
            !file.profiles["BALDR SKY"]
                .options
                .contains_key("frame_gen_multiplier")
        );
        update(&mut store, Action::Written(Ok(Written::Config)));

        // A profile key goes to the session's profile.
        let effect = update(
            &mut store,
            Action::Core(Event::SaveSetting {
                profile: Some("BALDR SKY".into()),
                key: "output_geometry".into(),
                value: Value::from("stretch"),
            }),
        );
        let file = written_config(effect);
        assert_eq!(
            file.profiles["BALDR SKY"].options["output_geometry"],
            Value::from("stretch")
        );
    }

    #[test]
    fn events_update_the_store() {
        let mut store = Store::default();
        update(
            &mut store,
            Action::Core(Event::SessionStarting {
                target: session().window,
            }),
        );
        assert!(matches!(store.session, SessionState::Starting { .. }));
        update(&mut store, Action::Core(Event::SessionStarted(session())));
        assert_eq!(store.session, SessionState::Running(session()));
        update(
            &mut store,
            Action::Core(Event::Stats(lirtup_proto::StatsSample::default())),
        );
        assert!(store.stats.is_some());
        update(
            &mut store,
            Action::Core(Event::SessionError {
                stage: lirtup_proto::Stage::Capture,
                message: "x".into(),
                retry: true,
            }),
        );
        update(
            &mut store,
            Action::Core(Event::SessionStopped {
                reason: StopReason::Error,
            }),
        );
        assert!(
            matches!(store.session, SessionState::Failed { .. }),
            "the error card stays"
        );
        update(
            &mut store,
            Action::Core(Event::SessionStopped {
                reason: StopReason::User,
            }),
        );
        assert_eq!(store.session, SessionState::Idle);
        assert_eq!(store.stats, None);

        let status = FrameGenStatus {
            env_locked: true,
            ..FrameGenStatus::default()
        };
        update(&mut store, Action::Core(Event::FrameGen(status.clone())));
        assert_eq!(store.framegen, status);
        update(
            &mut store,
            Action::Core(Event::DaemonState {
                on: true,
                keep_after_exit: false,
                watching: 3,
            }),
        );
        assert_eq!(store.daemon.watching, 3);
        update(&mut store, Action::Core(Event::WindowPicked { window: None }));
        assert_eq!(store.picked, Some(None));
        update(
            &mut store,
            Action::Core(Event::EffectCompile {
                name: "CAS".into(),
                done: true,
                error: Some("e".into()),
                session: true,
            }),
        );
        assert!(store.compiling["CAS"].session);
        update(
            &mut store,
            Action::Core(Event::BenchProgress { done: 2, total: 5 }),
        );
        assert_eq!((store.bench.done, store.bench.total), (2, 5));
        for i in 0..MAX_LOGS + 5 {
            update(
                &mut store,
                Action::Core(Event::Log {
                    level: Level::Warn,
                    message: i.to_string(),
                }),
            );
        }
        assert_eq!(store.logs.len(), MAX_LOGS);
        assert_eq!(store.logs[0].1, "5");
    }

    #[test]
    fn commands_of_b_and_d_are_sent() {
        let mut store = Store::default();
        let start = Action::StartScaling {
            target: Target::Focused,
            kind: SessionKind::Windowed,
            delay_s: Some(3.0),
            profile: None,
        };
        assert!(sends(
            &update(&mut store, start),
            &Command::Start {
                target: Target::Focused,
                kind: SessionKind::Windowed,
                delay_s: Some(3.0),
                profile: None
            }
        ));
        assert!(sends(&update(&mut store, Action::StopScaling), &Command::Stop));
        store.gui.keep_running_after_exit = true;
        assert!(sends(
            &update(&mut store, Action::SetDaemon { on: true }),
            &Command::SetDaemon {
                on: true,
                keep_after_exit: true
            }
        ));
        store.picked = Some(None);
        assert!(sends(
            &update(&mut store, Action::PickWindow),
            &Command::PickWindow
        ));
        assert_eq!(store.picked, None);
        assert!(sends(
            &update(&mut store, Action::CompileEffect("CAS".into())),
            &Command::CompileEffect { name: "CAS".into() }
        ));
        assert!(!store.compiling["CAS"].done);
        let gui = GuiConfig {
            zoom: 120,
            ..GuiConfig::default()
        };
        assert_eq!(
            update(&mut store, Action::SetGui(gui.clone())),
            Some(Effect::WriteGui(gui))
        );
        assert_eq!(update(&mut store, Action::Written(Ok(Written::Gui))), None);
    }

    #[test]
    fn stop_closes_the_error_card() {
        let mut store = Store {
            session: SessionState::Failed {
                stage: lirtup_proto::Stage::Window,
                message: "gone".into(),
                retry: true,
            },
            ..Store::default()
        };
        assert!(sends(&update(&mut store, Action::StopScaling), &Command::Stop));
        assert_eq!(store.session, SessionState::Idle);
    }

    #[test]
    fn a_mode_is_renamed_with_its_name_key() {
        let mut store = Store::default();
        // A file without `name`: the row shows the id.
        let raw: Mapping = serde_yaml_ng::from_str("model: 4x24\n").unwrap();
        store
            .modes
            .store
            .modes
            .insert("anime".into(), Mode::from_mapping("anime", &raw).0);
        store.modes.store.raw.insert("anime".into(), raw);
        let scope = Scope::Mode("anime".into());
        let name: Field<String> = store.field(&scope, "name");
        assert_eq!((name.value().as_str(), name.profile.as_deref()), ("anime", None));
        update(
            &mut store,
            Action::SetField {
                scope: scope.clone(),
                key: "name".into(),
                value: "Anime nítido".into(),
            },
        );
        assert_eq!(store.field::<String>(&scope, "name").value(), "Anime nítido");
        let list = effects(update(&mut store, Action::Save));
        let [Effect::WriteMode { mode, .. }] = list.as_slice() else {
            panic!("{list:?}")
        };
        assert_eq!(mode.name, "Anime nítido");
        assert_eq!(mode.settings.model, lirtup_config::Model::M4x24);
    }

    // ---- WatchGpuTimes -------------------------------------------------------------

    #[test]
    fn gpu_times_only_with_someone_looking() {
        let mut store = Store::default();
        let on = Command::WatchGpuTimes { on: true };
        let off = Command::WatchGpuTimes { on: false };
        let connected = LinkState::Connected {
            protocol: 1,
            version: "2.0.0".into(),
        };
        assert_eq!(update(&mut store, Action::Link(connected.clone())), None);
        assert_eq!(update(&mut store, Action::WindowVisible(true)), None);
        // Session starts on Inicio with the window visible: on.
        assert!(sends(
            &update(&mut store, Action::Core(Event::SessionStarted(session()))),
            &on
        ));
        // Nothing new: nothing sent.
        assert!(!sends(
            &update(&mut store, Action::Core(Event::Stats(Default::default()))),
            &on
        ));
        // Another page: off; back: on.
        assert!(sends(&update(&mut store, Action::Navigate(Route::About)), &off));
        assert!(sends(&update(&mut store, Action::Navigate(Route::Home)), &on));
        // Hidden: off.
        assert!(sends(&update(&mut store, Action::WindowVisible(false)), &off));
        assert!(sends(&update(&mut store, Action::WindowVisible(true)), &on));
        // The socket drops: nothing to send; it reconnects: on again.
        let down = update(
            &mut store,
            Action::Link(LinkState::Down {
                reason: "gone".into(),
            }),
        );
        assert_eq!(down, None);
        assert!(!store.gpu_times);
        assert!(sends(&update(&mut store, Action::Link(connected)), &on));
        // The session ends: off.
        assert!(sends(
            &update(
                &mut store,
                Action::Core(Event::SessionStopped {
                    reason: StopReason::User
                })
            ),
            &off
        ));
    }

    // ---- modes -----------------------------------------------------------------

    #[test]
    fn create_duplicate_delete_modes() {
        let mut store = store_with("profiles:\n  A: {}\n");
        let effect = update(
            &mut store,
            Action::CreateMode {
                name: "Anime nítido".into(),
                copy_from: None,
            },
        );
        let Some(Effect::WriteMode { id, mode, .. }) = effect else {
            panic!("{effect:?}")
        };
        assert_eq!(
            (id.as_str(), mode.name.as_str()),
            ("anime-nitido", "Anime nítido")
        );
        assert_eq!(store.route, Route::Modes { id: Some(id.clone()) });
        update(&mut store, Action::Written(Ok(Written::Mode(id.clone()))));

        update(
            &mut store,
            Action::SetField {
                scope: Scope::Mode(id.clone()),
                key: "cas_enabled".into(),
                value: true.into(),
            },
        );
        let effect = update(
            &mut store,
            Action::DuplicateMode {
                id: id.clone(),
                for_profile: Some(ProfileRef::Named("A".into())),
            },
        );
        let Some(Effect::WriteMode { id: copy, mode, .. }) = effect else {
            panic!("{effect:?}")
        };
        assert_eq!(mode.name, "Anime nítido (2)");
        assert_eq!(
            store.dirty_count(&Scope::Mode(copy.clone())),
            1,
            "the edits went to the copy"
        );
        assert_eq!(store.dirty_count(&Scope::Mode(id.clone())), 0);
        assert_eq!(store.mode_of(&ProfileRef::Named("A".into())), Some(copy.clone()));
        update(&mut store, Action::Written(Ok(Written::Mode(copy.clone()))));

        store.dialog = Some(Dialog::DeleteMode(id.clone()));
        assert_eq!(
            update(&mut store, Action::DeleteMode(id.clone())),
            Some(Effect::DeleteModeFile(id.clone()))
        );
        assert!(store.modes.store.get(&id).is_none());
        assert_eq!(store.dialog, None);
    }

    // ---- round 4, front M: the migration to modes -------------------------

    fn loaded(store: &mut Store, yaml: &str, gui: GuiConfig) -> Option<Effect> {
        let config = store_with(yaml).config.file;
        update(
            store,
            Action::Loaded {
                config,
                modes: ModeStore::default(),
                gui,
            },
        )
    }

    #[test]
    fn migration_is_offered_on_the_first_load() {
        let mut store = Store::default();
        loaded(&mut store, FILE_1X, GuiConfig::default());
        let Some(Dialog::Migrate(plan)) = store.dialog.clone() else {
            panic!("no migration dialog: {:?}", store.dialog);
        };
        assert_eq!(plan.top_mode.as_deref(), Some("predeterminado"));
        assert_eq!(
            plan.profiles,
            [("Firefox".to_owned(), Some("modo-de-firefox".to_owned()))]
        );

        // A reload (another path saved the file) does not ask again.
        update(&mut store, Action::CloseDialog);
        loaded(&mut store, FILE_1X, GuiConfig::default());
        assert_eq!(store.dialog, None);
    }

    #[test]
    fn migration_is_not_offered_when_declined_or_not_needed() {
        let mut store = Store::default();
        let declined = GuiConfig {
            migration_declined: true,
            ..GuiConfig::default()
        };
        loaded(&mut store, FILE_1X, declined);
        assert_eq!(store.dialog, None);

        let mut store = Store::default();
        loaded(&mut store, "mode: anime\nmax_fps: 60\n", GuiConfig::default());
        assert_eq!(store.dialog, None);
    }

    #[test]
    fn migrate_runs_the_plan() {
        let mut store = Store::default();
        loaded(&mut store, FILE_1X, GuiConfig::default());
        let Some(Dialog::Migrate(plan)) = store.dialog.clone() else {
            panic!("no migration dialog");
        };
        let effect = update(&mut store, Action::Migrate);
        assert!(
            matches!(&effect, Some(Effect::Migrate(p)) if *p == plan)
                || matches!(&effect, Some(Effect::Many(e)) if e.contains(&Effect::Migrate(plan.clone()))),
            "{effect:?}"
        );
        assert_eq!(store.dialog, None);
        // Without the dialog there is nothing to run.
        assert_eq!(update(&mut store, Action::Migrate), None);
    }

    #[test]
    fn skip_migration_is_remembered() {
        let mut store = Store::default();
        loaded(&mut store, FILE_1X, GuiConfig::default());
        let effect = update(&mut store, Action::SkipMigration);
        assert_eq!(store.dialog, None);
        assert!(store.gui.migration_declined);
        let written = match effect {
            Some(Effect::WriteGui(gui)) => gui,
            Some(Effect::Many(e)) => e
                .into_iter()
                .find_map(|e| match e {
                    Effect::WriteGui(gui) => Some(gui),
                    _ => None,
                })
                .unwrap(),
            other => panic!("{other:?}"),
        };
        assert!(written.migration_declined);
    }

    #[test]
    fn migrated_shows_a_toast() {
        let mut store = store_with(FILE_1X);
        update(&mut store, Action::Migrated(Ok("/x/config.yaml.1x.bak".into())));
        let toast = store.toast.clone().unwrap();
        assert_eq!(
            (toast.key, toast.args),
            (
                "migrate-done",
                vec![("backup", "/x/config.yaml.1x.bak".to_string())]
            )
        );
        update(&mut store, Action::Migrated(Err("disk full".into())));
        let toast = store.toast.clone().unwrap();
        assert_eq!(
            (toast.key, toast.args),
            ("migrate-failed", vec![("error", "disk full".to_string())])
        );
    }
}
