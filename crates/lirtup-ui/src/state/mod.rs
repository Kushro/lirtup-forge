//! The state of the GUI (6.3 of the analysis): one [`Store`] in a
//! `Signal`, changed only by [`update`], which is pure (no Dioxus, no I/O)
//! and tested without a window.
//!
//! - [`Action`] is everything the person does and everything that arrives
//!   from the core ([`Action::Core`]), the link and the window list.
//! - [`Effect`] is what `update` asks the outside world to do: send a
//!   command, write YAML, start a timer. `app.rs` runs effects
//!   ([`crate::app`]); results come back as actions ([`Action::Written`]).
//! - Settings are [`Field`]s: the value of the layer below (*Predeterminado*
//!   for a profile, the system defaults for *Predeterminado* and modes), the
//!   value this layer sets, and an unsaved edit. A row's [`Origin`], the
//!   count of the save bar and the dots of tabs and drawer derive from them
//!   (`use_memo` over the store in the screens).
//! - YAML is always written with `lirtup-config` (the structures the scaler
//!   reads), keeping the keys the GUI does not know: the store keeps the
//!   files as read ([`ConfigDoc::file`], [`ModesDoc::store`]) and the edits
//!   apart, and [`store::apply_edits`] merges them only to save.
//!
//! Language and theme are not here: they are their own signals
//! ([`crate::i18n::LANG`], [`crate::theme`]) so a change does not re-run
//! every memo of the store.
//!
//! Front C writes the bodies ([`store`]); the types below are the contract
//! the screens of B and D use.

pub mod store;

use indexmap::IndexMap;
use lirtup_config::{ConfigFile, GuiConfig, ModeStore, Value};
use lirtup_proto::{
    BenchReport, BenchRequest, Command, DaemonStatus, DiagReport, EffectInfo, Event, FrameGenStatus, GpuInfo,
    MonitorInfo, SessionKind, SessionView, Stage, StatsSample, Target, WindowRef,
};
use lirtup_x11::WindowInfo;

use crate::route::{ModeId, ProfileName, ProfileRef, ProfileTab, Route};

/// A setting key: the YAML key of the option in its layer (`follow_focus`,
/// `output_geometry`, `magpie_effects`, ...). `hotkeys` is one key per
/// action, `hotkeys.<action>`, because the layers merge it key by key.
pub type Key = String;

/// One setting of one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Field<T> {
    /// What the layer below gives (*Predeterminado* or the system default).
    pub default: T,
    /// What this layer sets in the file (`None` = inherits).
    pub profile: Option<T>,
    /// Unsaved edit: `None` untouched; `Some(Some(v))` a new value;
    /// `Some(None)` «volver a heredar» (remove the key on save).
    pub edit: Option<Option<T>>,
}

/// What a [`crate::components::SettingRow`] shows about where its value
/// comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// From the layer below (grey «Hereda de Predeterminado»).
    Inherited,
    /// Set by this layer (primary stripe and «volver a heredar»).
    Overridden,
    /// Unsaved (amber stripe, «antes: …» with the saved value as text).
    Dirty {
        /// The value before the edit, formatted for the row.
        before: String,
    },
}

/// Which layer an edit goes to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    /// *Predeterminado* or a profile, in `config.yaml`.
    Profile(ProfileRef),
    /// A mode file in `modes/`.
    Mode(ModeId),
}

/// `config.yaml` as read, plus the unsaved edits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigDoc {
    /// The file as read (or as last saved).
    pub file: ConfigFile,
    /// Unsaved edits per profile and key: `Some(v)` sets, `None` removes
    /// (inherit). Insertion order is the order of the edits.
    pub edits: IndexMap<(ProfileRef, Key), Option<Value>>,
}

/// `modes/*.yaml` as read, plus the unsaved edits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModesDoc {
    /// The folder as read.
    pub store: ModeStore,
    /// Unsaved edits per mode and key, as [`ConfigDoc::edits`].
    pub edits: IndexMap<(ModeId, Key), Option<Value>>,
    /// Profiles that name each mode (for «Este modo lo usan N perfiles»).
    pub users: IndexMap<ModeId, Vec<ProfileRef>>,
}

/// The connection to `lirtup serve` ([`crate::link`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LinkState {
    /// Connecting (or starting `lirtup serve`).
    #[default]
    Connecting,
    /// `Hello` answered.
    Connected {
        /// The server's protocol version; different from ours → warning
        /// with «Reiniciar el escalador».
        protocol: u32,
        /// The server's version (`2.0.0`).
        version: String,
    },
    /// No socket (the scaler is down); the next command relaunches it.
    Down {
        /// Why, for the error card.
        reason: String,
    },
}

/// The session card of Inicio.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SessionState {
    /// Nothing running.
    #[default]
    Idle,
    /// `SessionStarting`.
    Starting {
        /// The window.
        target: WindowRef,
    },
    /// `SessionStarted`.
    Running(SessionView),
    /// `SessionError` (the card turns error-container with «Reintentar»).
    Failed {
        /// Where.
        stage: Stage,
        /// What.
        message: String,
        /// Whether «Reintentar» makes sense.
        retry: bool,
    },
}

/// A dialog over the current route.
#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    /// «Cambios sin guardar» before going to `to`.
    Unsaved {
        /// Where the person wanted to go.
        to: Route,
    },
    /// «Nuevo perfil», maybe from a window tile («Crear perfil»).
    NewProfile {
        /// The window it starts from (title, size, icon).
        from: Option<WindowInfo>,
    },
    /// «Medir en mi GPU» (front D).
    Bench {
        /// The mode it measures.
        mode: Option<ModeId>,
    },
    /// «Agregar efecto» (front D).
    EffectPicker {
        /// The mode the effect goes to.
        mode: ModeId,
    },
    /// Confirm deleting a profile.
    DeleteProfile(ProfileName),
    /// Confirm deleting a mode.
    DeleteMode(ModeId),
}

/// The answer of [`Dialog::Unsaved`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsavedChoice {
    /// «Guardar y cambiar».
    SaveAndGo,
    /// «Descartar y cambiar».
    DiscardAndGo,
    /// «Cancelar».
    Cancel,
}

/// The snackbar. Text is a Fluent key plus arguments, translated when it
/// is shown (`update` does not know the language).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Fluent key.
    pub key: &'static str,
    /// Fluent arguments.
    pub args: Vec<(&'static str, String)>,
    /// Shows «Deshacer» ([`Action::Undo`]).
    pub undo: bool,
}

/// What «Nuevo perfil» creates.
#[derive(Debug, Clone, PartialEq)]
pub struct NewProfile {
    /// Name (validated in the dialog: not empty, not taken).
    pub name: ProfileName,
    /// The mode it names (`None` = inherit from *Predeterminado*).
    pub mode: Option<ModeId>,
    /// Copy the options of this profile.
    pub copy_from: Option<ProfileRef>,
    /// The match rules.
    pub matcher: Option<lirtup_config::Match>,
    /// Icon: a PNG captured from the window or picked, saved next to the
    /// config (`icon:` key of the profile).
    pub icon: Option<Vec<u8>>,
}

/// A measurement in progress or done.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BenchView {
    /// What was asked.
    pub request: Option<BenchRequest>,
    /// `BenchProgress`.
    pub done: u32,
    /// `BenchProgress`.
    pub total: u32,
    /// `BenchResult`.
    pub report: Option<BenchReport>,
}

/// Compiles the GUI asked for or the session reported
/// (`Event::EffectCompile`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileState {
    /// Finished.
    pub done: bool,
    /// Compiler error.
    pub error: Option<String>,
    /// The running session disabled it.
    pub session: bool,
}

/// What was written, for [`Action::Written`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    /// `config.yaml`.
    Config,
    /// A mode file.
    Mode(ModeId),
    /// A mode file removed.
    ModeDeleted(ModeId),
    /// `gui-config.yaml`.
    Gui,
}

/// Everything in the GUI.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Store {
    /// The screen.
    pub route: Route,
    /// The dialog over it.
    pub dialog: Option<Dialog>,
    /// The snackbar.
    pub toast: Option<Toast>,
    /// `config.yaml`.
    pub config: ConfigDoc,
    /// `modes/`.
    pub modes: ModesDoc,
    /// `gui-config.yaml` (Ajustes saves it at once, no save bar).
    pub gui: GuiConfig,
    /// The config as it was before the last save («Deshacer» of the toast).
    pub undo: Option<ConfigFile>,
    /// Top-level windows (`windows.rs`, every `auto_refresh_ms`).
    pub windows: Vec<WindowInfo>,
    /// The main window is visible (not hidden to the tray nor minimized):
    /// thumbnails and GPU times run only then.
    pub visible: bool,
    /// The session card.
    pub session: SessionState,
    /// The last `Stats`.
    pub stats: Option<StatsSample>,
    /// Frame generation.
    pub framegen: FrameGenStatus,
    /// Daemon.
    pub daemon: DaemonStatus,
    /// The socket.
    pub link: LinkState,
    /// The MagpieFX catalog (`ListEffects`).
    pub effects: Vec<EffectInfo>,
    /// Compiles by effect name.
    pub compiling: IndexMap<String, CompileState>,
    /// Monitors (`ListDevices`).
    pub monitors: Vec<MonitorInfo>,
    /// GPUs (`ListDevices`).
    pub gpus: Vec<GpuInfo>,
    /// «Medir en mi GPU».
    pub bench: BenchView,
    /// Acerca de ▸ Diagnóstico.
    pub diagnostics: Option<DiagReport>,
    /// `WindowPicked` arrived for «Señalar ventana» (the dialog takes it).
    pub picked: Option<Option<WindowRef>>,
    /// Warnings and errors of the scaler (`Event::Log`), newest last, at
    /// most [`store::MAX_LOGS`].
    pub logs: Vec<(lirtup_proto::Level, String)>,
    /// Whether the GUI told the core it wants GPU times (to send
    /// `WatchGpuTimes` only on changes).
    pub gpu_times: bool,
}

/// Everything that changes the store.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // ---- navigation and dialogs -----------------------------------------
    /// Go to a route (asks first when the page being left has edits).
    Navigate(Route),
    /// Open a dialog.
    OpenDialog(Dialog),
    /// Close the dialog (Cancel, Esc, the scrim).
    CloseDialog,
    /// The answer of [`Dialog::Unsaved`].
    Unsaved(UnsavedChoice),
    /// The snackbar timed out or was closed.
    DismissToast,

    // ---- settings --------------------------------------------------------
    /// An edit of one setting.
    SetField {
        /// Layer.
        scope: Scope,
        /// Key.
        key: Key,
        /// New value.
        value: Value,
    },
    /// «Volver a heredar»: remove the key from this layer on save.
    Inherit {
        /// Layer.
        scope: Scope,
        /// Key.
        key: Key,
    },
    /// Drop the unsaved edit of one setting.
    Revert {
        /// Layer.
        scope: Scope,
        /// Key.
        key: Key,
    },
    /// Restablecer ▸ «Deshacer los cambios de esta pestaña».
    ResetTab {
        /// Profile.
        profile: ProfileRef,
        /// Tab.
        tab: ProfileTab,
    },
    /// Restablecer ▸ «Quitar los ajustes del perfil en esta pestaña».
    RemoveProfileOverrides {
        /// Profile.
        profile: ProfileRef,
        /// Tab.
        tab: ProfileTab,
    },
    /// Restablecer ▸ «Volver todo a los valores del sistema» (the whole
    /// profile, or *Predeterminado*, back to the defaults; unsaved until
    /// Guardar).
    ResetAll {
        /// Profile.
        profile: ProfileRef,
    },
    /// Guardar (Ctrl+S): every edit of config and modes.
    Save,
    /// «Deshacer» of the save toast: write [`Store::undo`] back.
    Undo,

    // ---- profiles and modes ----------------------------------------------
    /// «Crear» of «Nuevo perfil».
    CreateProfile(NewProfile),
    /// Duplicate a profile (Ctrl+D) as «<nombre> (copia)».
    DuplicateProfile(ProfileName),
    /// Delete a profile (after [`Dialog::DeleteProfile`]).
    DeleteProfile(ProfileName),
    /// Rename a profile (the header's «Editar perfil»).
    RenameProfile {
        /// Old name.
        from: ProfileName,
        /// New name.
        to: ProfileName,
    },
    /// Change the match rules of a profile (unsaved until Guardar).
    SetMatch {
        /// Profile.
        profile: ProfileName,
        /// Rules (`None` = only by name).
        matcher: Option<lirtup_config::Match>,
    },
    /// A new mode, empty or copying another.
    CreateMode {
        /// Display name.
        name: String,
        /// Copy this mode's settings.
        copy_from: Option<ModeId>,
    },
    /// Duplicate a mode («Duplicar antes de editar» of the shared mode
    /// banner); the profile being edited switches to the copy.
    DuplicateMode {
        /// The mode.
        id: ModeId,
        /// The profile to switch to the copy (`None` = only copy).
        for_profile: Option<ProfileRef>,
    },
    /// Delete a mode (after [`Dialog::DeleteMode`]).
    DeleteMode(ModeId),

    // ---- the core ----------------------------------------------------------
    /// Escalar: from a tile, the session card or «Empezar».
    StartScaling {
        /// Which window.
        target: Target,
        /// Fullscreen or windowed.
        kind: SessionKind,
        /// Deferred scaling, seconds.
        delay_s: Option<f32>,
        /// Profile by name (`None` = the core matches).
        profile: Option<ProfileName>,
    },
    /// Detener.
    StopScaling,
    /// Reiniciar (also after changing the frame generation multiplier).
    RestartScaling,
    /// The daemon switch.
    SetDaemon {
        /// On or off.
        on: bool,
    },
    /// «Señalar ventana».
    PickWindow,
    /// Load the MagpieFX catalog.
    ListEffects,
    /// Compile an effect ahead of time.
    CompileEffect(String),
    /// Load monitors and GPUs.
    ListDevices,
    /// Check frame generation.
    ProbeFrameGen,
    /// «Medir».
    RunBench(BenchRequest),
    /// Cancel the measurement.
    CancelBench,
    /// Acerca de ▸ Diagnóstico.
    RequestDiagnostics,
    /// An event of the core.
    Core(Event),
    /// The socket connected or dropped.
    Link(LinkState),

    // ---- the GUI itself ----------------------------------------------------
    /// The window list (`windows.rs`).
    Windows(Vec<WindowInfo>),
    /// The main window was shown or hidden (tray, minimize).
    WindowVisible(bool),
    /// Ajustes changed (written at once).
    SetGui(GuiConfig),
    /// The files were read again (startup, or the core's
    /// `MenuBarDetected` was saved by another path).
    Loaded {
        /// `config.yaml`.
        config: ConfigFile,
        /// `modes/`.
        modes: ModeStore,
        /// `gui-config.yaml`.
        gui: GuiConfig,
    },
    /// An [`Effect`] that writes finished.
    Written(Result<Written, String>),
}

/// What [`update`] asks the outside world to do (run by `app.rs`).
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Send a command to `lirtup serve`.
    Send(Command),
    /// Write `config.yaml` (`ConfigFile::save`, with its backups); then
    /// [`Action::Written`].
    WriteConfig(ConfigFile),
    /// Write one mode file; then [`Action::Written`].
    WriteMode {
        /// Id (file stem).
        id: ModeId,
        /// Content.
        mode: lirtup_config::Mode,
    },
    /// Remove one mode file; then [`Action::Written`].
    DeleteModeFile(ModeId),
    /// Write `gui-config.yaml`; then [`Action::Written`].
    WriteGui(GuiConfig),
    /// Show the snackbar and dismiss it after a while
    /// ([`Action::DismissToast`]).
    Toast(Toast),
    /// Several, in order.
    Many(Vec<Effect>),
}

/// Apply `action` to `store`. Pure: everything outside comes back as the
/// returned [`Effect`].
pub fn update(store: &mut Store, action: Action) -> Option<Effect> {
    store::update(store, action)
}
