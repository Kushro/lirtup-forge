//! The engine (`api/` of 1.x: `UpscalerSession`). Front K, round 2.
//!
//! # Startup order (`Engine::new`)
//!
//! 1. Resolve the configuration (`lirtup_config::resolve`) with the CLI
//!    overrides.
//! 2. **`lirtup_fg::layer::prepare`**, before the tokio runtime and before
//!    any other thread exists (it changes the environment): it writes
//!    `lsfg-vk.toml` or sets `DISABLE_LSFGVK=1`, so the user's implicit
//!    lsfg-vk layer never loads into our own instance unless the layer
//!    engine is wanted. Keep the [`lirtup_fg::layer::LayerSetup`] it
//!    returns. `lirtup serve` and `lirtup run` therefore build the engine
//!    first and the runtime after.
//! 3. Start the global hotkeys (`lirtup_x11::Hotkeys::start`).
//! 4. The GPU (`lirtup_gpu::Instance` + `Device`, from `gpu`/`VULKAN_DEVICE`)
//!    is opened on the first session and kept until shutdown.
//!
//! # What it uses from `lirtup-pipeline`
//!
//! - `pipeline::Session` / `SessionSetup` / `PipelineCommand`: one session
//!   at a time; its `crossbeam` events are relayed to the broadcast.
//! - `framegen::status_from_setup`: `EngineSnapshot.framegen` between
//!   sessions and the answer to `FrameGenProbe`.
//! - `bench`: only inside the `lirtup bench` child process (see
//!   `lirtup_pipeline::bench`); here, `Bench` spawns that child and relays
//!   its lines, `CancelBench` sends it SIGTERM.
//!
//! # Commands
//!
//! | Command | Done by |
//! | --- | --- |
//! | `Start` | resolve the target (`lirtup_x11::X11::find_by_title`, `wait_for_pid`, `active_after` for `delay_s`), the profile (`ProfileChoice::Auto` unless named), send `SessionStarting`, `Session::start` (stopping the old one with `StopReason::Replaced`) |
//! | `Stop`, `Restart` | `Session::stop`; `Restart` with the layer engine and a changed multiplier is `lirtup_fg::layer::restart_self` (the socket drops; the GUI reconnects) |
//! | `ReloadConfig` | re-read the files, `Hotkeys::rebind`, `PipelineCommand::UpdateConfig` |
//! | `Hotkey` and the hotkey thread | `StartFullscreen`/`StartWindowed` → `Start { target: Focused }`; `ExitApp` → stop (and quit with `lirtup run`); the rest → `PipelineCommand::Hotkey` |
//! | `PickWindow`, `ListDevices` | `lirtup_x11` on a blocking task |
//! | `ListEffects`, `CompileEffect` | `lirtup_magpiefx::Catalog` and `compile_effect_with` on a blocking task (cache in `paths::magpiefx_cache_dir`); `CompileEffect` compiles the four red/blue swap combinations (`swap_input`, `swap_output` ∈ ff, tf, ft, tt), so a session finds the effect in the cache wherever it lands in either chain (before or after the model, first, last or in between); the first error ends it and is the one `EffectCompile { done: true, error }` |
//! | `SetDaemon` | start or stop the daemon thread ([`crate::daemon`]); `keep_after_exit` is read by `lirtup serve` ([`crate::lifecycle`]) |
//! | `Diagnostics` | gather `DiagReport` |
//! | `PauseHotkeys` | count the pausers ([`Engine::pause_hotkeys`], per connection in the server); ungrab every hotkey when the count leaves 0, grab them again when it returns |
//! | `WatchGpuTimes` | count the watchers ([`Engine::watch_gpu_times`]; the server counts per connection); `PipelineCommand::SetTimestamps` when the count goes to or from 0 |
//! | `Preview` | keep the unsaved mode in the running session (`Running::preview`), resolve again with it and send `PipelineCommand::UpdateConfig { preview: true }`; ignored with no session |
//! | `PreviewEnd` | drop the preview and do a `ReloadConfig` (`preview: false`); also [`Engine::end_preview`], when the connection that sent the preview closes |
//! | `Quit` | shutdown |
//!
//! # Details of round 2
//!
//! - `handle` never waits for X11, the GPU or a compiler: `Start`,
//!   `Restart`, `PickWindow`, `ListEffects`, `CompileEffect`, `ListDevices`,
//!   `Diagnostics` and `Bench` run on threads of their own. It does wait
//!   for a running session to stop (`Stop`, `Quit`, a replacing `Start`).
//! - A `Start` while another is still looking for its window replaces it;
//!   `Stop` cancels it. A `Start` that fails before its session runs (no
//!   window, a bad profile, no device) sends `SessionError` and then
//!   `SessionStopped { reason: Error }`, as a failing session does.
//! - `StartFullscreen` and `StartWindowed` stop the running session instead
//!   of starting another (the overlay has the focus by then).
//! - Events of the session thread go through one relay thread, which keeps
//!   the snapshot up to date (`SessionStarted`, `SessionError`, `FrameGen`)
//!   before broadcasting them.
//! - The configuration of the snapshot and of the hotkeys is the top level
//!   (plus [`EngineOptions::profile`] and the CLI layer); each session
//!   resolves its own with its profile.
//!
//! # Round 4 (front Y)
//!
//! - The daemon is on at startup with `--daemon` or `daemon: true` in the
//!   top level; `SetDaemon` turns it on and off, and `ReloadConfig`
//!   restarts its thread when `daemon_poll_interval` changes.
//! - A `Start` on a given window (not `Focused`) focuses it first (1.x
//!   does it for `-t`, `--select` and the GUI). A `Target::Pid` searches by
//!   class too, with the launched program's name as the hint.
//! - `Restart` that `exec`s carries the session ([`Resume`]) and the
//!   daemon ([`DAEMON_ENV`]) to the new process, which `Engine::new` takes
//!   from the environment and starts again.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use arc_swap::ArcSwap;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use lirtup_config::resolve::{ProfileChoice, Resolved, resolve, resolve_with_mode};
use lirtup_config::{
    Config, ConfigError, ConfigFile, FrameGenEngine, FrameGenMultiplier, HotkeyAction, Mode, ModeStore, paths,
};
use lirtup_fg::FrameGenState;
use lirtup_fg::layer::{self, LayerSetup};
use lirtup_gpu::{Device, GpuError, Instance, InstanceOptions};
use lirtup_pipeline::framegen::status_from_setup;
use lirtup_pipeline::pipeline::ModeEntry;
use lirtup_pipeline::{PipelineCommand, Session, SessionSetup};
use lirtup_x11::acquire::PidSearch;
use lirtup_x11::{Hotkeys, WindowInfo, X11};
use tokio::sync::{broadcast, watch};

use crate::daemon::{Daemon, DaemonHost};
use crate::lifecycle::{RESUME_ENV, Resume};
use crate::listing;
use crate::proto::{
    BenchReport, Command, DaemonStatus, DiagReport, EngineSnapshot, Event, FrameGenStatus, Level,
    PROTOCOL_VERSION, SessionKind, SessionView, Stage, StopReason, Target,
};
use crate::{CoreError, Result};

/// Events kept for slow subscribers before they lag (a `Stats` every
/// 500 ms is the steady load).
const EVENT_CAPACITY: usize = 256;

/// How long a session gets to stop before the engine moves on.
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

/// How often the relay thread looks at the shutdown flag.
const RELAY_TICK: Duration = Duration::from_millis(100);

/// Version reported in the snapshot and in diagnostics.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Carries the daemon across a `Restart` that `exec`s: `on`, or `keep`
/// (on with `keep_after_exit`). Read and removed by [`Engine::new`].
pub const DAEMON_ENV: &str = "LIRTUP_RESUME_DAEMON";

/// How to create the engine.
#[derive(Debug, Clone, Default)]
pub struct EngineOptions {
    /// `config.yaml` (default: the XDG path).
    pub config_path: Option<PathBuf>,
    /// `modes/` (default: next to the config).
    pub modes_dir: Option<PathBuf>,
    /// Command-line overrides (`lirtup run`), applied on top of every
    /// session's config.
    pub cli_overrides: lirtup_config::Mapping,
    /// Start in daemon mode.
    pub daemon: bool,
    /// Profile of `lirtup run -p`: part of the startup configuration
    /// (frame generation is read once, before Vulkan) and the default of
    /// every `Start` without a profile.
    pub profile: Option<String>,
}

/// The engine. `Send + Sync`; shared as `Arc<Engine>` between the server
/// tasks. Dropping the last reference shuts it down.
pub struct Engine {
    inner: Arc<Inner>,
}

/// The frame generation settings `layer::prepare` saw: a change of these
/// with the layer engine needs an `exec` (`Restart`).
#[derive(Debug, Clone, PartialEq)]
struct Prepared {
    engine: FrameGenEngine,
    multiplier: FrameGenMultiplier,
    flow_scale: f32,
    performance: bool,
    dll: Option<PathBuf>,
}

impl Prepared {
    fn of(config: &Config) -> Prepared {
        Prepared {
            engine: config.frame_gen_engine,
            multiplier: config.frame_gen_multiplier,
            flow_scale: config.frame_gen_flow_scale,
            performance: config.frame_gen_performance,
            dll: config.frame_gen_dll.clone(),
        }
    }
}

/// `config.yaml` and `modes/`, as last read.
struct Files {
    file: ConfigFile,
    modes: ModeStore,
}

/// A `Start` as the engine keeps it.
#[derive(Debug, Clone)]
struct StartRequest {
    target: Target,
    kind: SessionKind,
    delay_s: Option<f32>,
    profile: Option<String>,
}

/// The running session.
struct Running {
    session: Session,
    window: WindowInfo,
    /// The profile the `Start` named (`None` = matched by window).
    profile: Option<String>,
    /// «Probar en vivo» (`Command::Preview`): unsaved edits of one mode,
    /// applied by [`Inner::resolve_for`] until `PreviewEnd` or the session
    /// ends (it lives and dies with `Running`).
    preview: Option<Preview>,
}

/// A mode editor preview (see [`Running::preview`]).
#[derive(Debug, Clone)]
struct Preview {
    /// The profile the GUI saw (informational: only logged).
    profile: Option<String>,
    /// Id of the previewed mode.
    mode: String,
    /// The mode file with the unsaved edits (`ModeStore::raw` shape).
    raw: lirtup_config::Mapping,
}

#[derive(Default)]
struct State {
    session: Option<Running>,
    /// From `SessionStarted`; only shown while the session lives.
    view: Option<SessionView>,
    last_error: Option<(Stage, String)>,
    daemon: DaemonStatus,
    framegen: FrameGenStatus,
    /// What `Restart` starts again: window, kind and named profile.
    last_start: Option<(u32, SessionKind, Option<String>)>,
    /// Clients that show GPU times (`WatchGpuTimes`).
    gpu_watchers: u32,
    /// Clients that capture a key combination (`PauseHotkeys`).
    hotkey_pausers: u32,
}

/// The daemon thread and the poll interval it runs with.
struct DaemonThread {
    daemon: Daemon,
    poll: Duration,
}

impl State {
    /// The session, unless its thread has ended.
    fn live(&self) -> Option<&Running> {
        self.session.as_ref().filter(|r| !r.session.is_finished())
    }
}

#[derive(Default)]
struct Gpu {
    instance: Option<Arc<Instance>>,
    device: Option<Arc<Device>>,
}

struct Inner {
    options: EngineOptions,
    layer: LayerSetup,
    prepared: Prepared,
    files: Mutex<Files>,
    /// Top-level configuration (see the module docs).
    config: ArcSwap<Config>,
    events: broadcast::Sender<Event>,
    /// Given to sessions; drained by the relay thread.
    pipeline_tx: Sender<Event>,
    state: Mutex<State>,
    gpu: Mutex<Gpu>,
    hotkeys: Mutex<Option<Hotkeys>>,
    /// PID of the `lirtup bench` child.
    bench: Mutex<Option<u32>>,
    /// Bumped by every `Start` and `Stop`: a start thread whose number is
    /// not the current one gives up.
    start_gen: AtomicU64,
    /// Start threads that have not finished (the daemon waits for them).
    starts: AtomicUsize,
    /// The daemon thread while the daemon is on.
    daemon: Mutex<Option<DaemonThread>>,
    stopping: AtomicBool,
    /// Tells the relay thread to drain and end.
    relay_stop: AtomicBool,
    quit: watch::Sender<bool>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn load_files(options: &EngineOptions) -> Result<Files> {
    let config_path = options.config_path.clone().unwrap_or_else(paths::config_path);
    let modes_dir = match (&options.modes_dir, &options.config_path) {
        (Some(dir), _) => dir.clone(),
        (None, Some(path)) => path
            .parent()
            .map(|d| d.join("modes"))
            .unwrap_or_else(paths::modes_dir),
        (None, None) => paths::modes_dir(),
    };
    let (file, file_warnings) = ConfigFile::load(&config_path)?;
    let (modes, mode_warnings) = ModeStore::load(&modes_dir)?;
    for w in file_warnings.iter().chain(&mode_warnings) {
        tracing::warn!("config: {}: {}", w.key, w.message);
    }
    Ok(Files { file, modes })
}

fn top_level(files: &Files, options: &EngineOptions) -> Result<Config> {
    let choice = match &options.profile {
        Some(name) => ProfileChoice::Named(name.clone()),
        None => ProfileChoice::None,
    };
    Ok(resolve(&files.file, &files.modes, &choice, &options.cli_overrides)?.config)
}

/// For a process that opens Vulkan but never presents (`lirtup bench`,
/// `lirtup list gpus`): read the configuration and run
/// `lirtup_fg::layer::prepare` with frame generation off, so the lsfg-vk
/// layer stays out of this process (`DISABLE_LSFGVK=1`, unless the user set
/// `LSFGVK_*` themselves). Call it before any thread and any
/// `Instance::new`. Returns the configuration as configured.
pub fn prepare_headless(options: &EngineOptions) -> Result<Config> {
    let config = top_level(&load_files(options)?, options)?;
    let mut off = config.clone();
    off.frame_gen_multiplier = FrameGenMultiplier::Fixed(1);
    layer::prepare(&off)?;
    Ok(config)
}

/// `LIRTUP_VULKAN_DEBUG=1` turns the validation layer on.
fn vulkan_debug() -> bool {
    std::env::var_os("LIRTUP_VULKAN_DEBUG").is_some_and(|v| v == "1")
}

impl Engine {
    /// Read the configuration, prepare frame generation (before any Vulkan
    /// call), start the hotkey thread. Does not open the GPU until a session
    /// starts.
    pub fn new(options: EngineOptions) -> Result<Engine> {
        // 0. What a `Restart` that `exec`ed carries (while this is the only
        //    thread: it changes the environment).
        let resume = Resume::take_from_env();
        let resume_daemon = take_daemon_env();
        // 1. Configuration.
        let files = load_files(&options)?;
        let config = top_level(&files, &options)?;
        // 2. The lsfg-vk layer, while this is the only thread.
        let layer = layer::prepare(&config)?;
        tracing::info!(
            "frame generation: {:?} (x{}, {:?})",
            layer.state,
            layer.multiplier,
            config.frame_gen_engine
        );
        let framegen = status_from_setup(&config, &layer);

        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let (pipeline_tx, pipeline_rx) = crossbeam_channel::unbounded();
        let (quit, _) = watch::channel(false);
        let watching = watching(&files.file);
        let daemon_on = options.daemon || config.daemon || resume_daemon.is_some();
        let inner = Arc::new(Inner {
            prepared: Prepared::of(&config),
            layer,
            files: Mutex::new(files),
            config: ArcSwap::from_pointee(config),
            events,
            pipeline_tx,
            state: Mutex::new(State {
                daemon: DaemonStatus {
                    on: false,
                    keep_after_exit: false,
                    watching,
                },
                framegen,
                ..State::default()
            }),
            gpu: Mutex::new(Gpu::default()),
            hotkeys: Mutex::new(None),
            bench: Mutex::new(None),
            start_gen: AtomicU64::new(0),
            starts: AtomicUsize::new(0),
            daemon: Mutex::new(None),
            stopping: AtomicBool::new(false),
            relay_stop: AtomicBool::new(false),
            quit,
            threads: Mutex::new(Vec::new()),
            options,
        });
        // The relay of session events.
        let relay = {
            let inner = Arc::clone(&inner);
            std::thread::Builder::new()
                .name("lirtup-events".into())
                .spawn(move || inner.relay(&pipeline_rx))?
        };
        lock(&inner.threads).push(relay);

        // 3. Global hotkeys (a failure leaves the engine without them).
        inner.start_hotkeys();

        // 4. The daemon, and the session a `Restart` carried.
        if daemon_on {
            inner.set_daemon(true, resume_daemon == Some(true));
        }
        if let Some(resume) = resume {
            tracing::info!("resuming the session on 0x{:08x} after the restart", resume.xid);
            inner.start(StartRequest {
                target: Target::Window(resume.xid),
                kind: resume.kind,
                delay_s: None,
                profile: resume.profile,
            });
        }
        Ok(Engine { inner })
    }

    /// Handle one command. Answers and progress arrive as events.
    pub fn handle(&self, command: Command) -> Result<()> {
        self.inner.handle(command)
    }

    /// One client more (`on`) or less wants GPU times. The server calls it
    /// once per connection that sends `WatchGpuTimes`, and with `false` when
    /// that connection closes; [`Engine::handle`] counts a direct caller as
    /// one watcher.
    pub fn watch_gpu_times(&self, on: bool) {
        self.inner.watch_gpu_times(on);
    }

    /// One client more (`on`) or less captures a key combination: while any
    /// does, the global hotkeys are released. The server calls it once per
    /// connection, as [`Engine::watch_gpu_times`].
    pub fn pause_hotkeys(&self, on: bool) {
        self.inner.pause_hotkeys(on);
    }

    /// The connection that sent the running `Command::Preview` closed: end
    /// the preview as `PreviewEnd` would (nothing if there is none). The
    /// server calls it from a per-connection guard, as
    /// [`Engine::watch_gpu_times`]. It reads the files again (as
    /// `ReloadConfig`), so the server calls it on a blocking thread.
    pub fn end_preview(&self) {
        if !self.inner.stopping.load(Ordering::SeqCst) {
            self.inner.set_preview(None);
        }
    }

    /// Subscribe to events. A lagging receiver gets `RecvError::Lagged` and
    /// should ask for a [`Engine::snapshot`].
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// The whole state now.
    pub fn snapshot(&self) -> EngineSnapshot {
        let st = lock(&self.inner.state);
        EngineSnapshot {
            protocol: PROTOCOL_VERSION,
            version: VERSION.into(),
            session: st.live().and(st.view.clone()),
            last_error: st.last_error.clone(),
            daemon: st.daemon,
            framegen: st.framegen.clone(),
        }
    }

    /// A session is running or starting (`lirtup serve` stays while one
    /// does; [`crate::lifecycle`]).
    pub fn session_running(&self) -> bool {
        self.inner.starts.load(Ordering::SeqCst) > 0 || lock(&self.inner.state).live().is_some()
    }

    /// The daemon is on with `keep_after_exit` («Seguir ejecutándose tras
    /// salir»).
    pub fn daemon_keeps(&self) -> bool {
        let daemon = lock(&self.inner.state).daemon;
        daemon.on && daemon.keep_after_exit
    }

    /// The top-level configuration in effect (the startup one, or the last
    /// `ReloadConfig`).
    pub fn config(&self) -> Arc<Config> {
        self.inner.config.load_full()
    }

    /// Becomes `true` after `Quit` (and on shutdown): `lirtup serve` stops
    /// listening.
    pub fn quit_signal(&self) -> watch::Receiver<bool> {
        self.inner.quit.subscribe()
    }

    /// Stop the session and the threads, waiting at most `timeout`.
    pub fn shutdown(&self, timeout: Duration) {
        self.inner.shutdown(timeout);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.inner.shutdown(STOP_TIMEOUT);
    }
}

/// Profiles the daemon would watch: with a `match` and not excluded.
fn watching(file: &ConfigFile) -> u32 {
    crate::daemon::watched(file).len() as u32
}

/// Read and remove [`DAEMON_ENV`]: `Some(keep_after_exit)` if the daemon
/// was on (at startup, before any thread).
#[allow(unsafe_code)]
fn take_daemon_env() -> Option<bool> {
    let value = std::env::var_os(DAEMON_ENV)?;
    // SAFETY: `Engine::new` runs before any other thread exists (as
    // `layer::prepare`, its documented contract).
    unsafe { std::env::remove_var(DAEMON_ENV) };
    match value.to_str() {
        Some("keep") => Some(true),
        Some("on") => Some(false),
        _ => None,
    }
}

/// The poll interval of the daemon (`daemon_poll_interval`, at least
/// 0.1 s).
fn daemon_poll(config: &Config) -> Duration {
    Duration::from_secs_f64(config.daemon_poll_interval.max(0.1))
}

/// The class hint of a launched program: the name of its executable, from
/// `/proc/<pid>/cmdline` (1.x passes `program[0]`), else `comm`.
fn class_hint(pid: u32) -> Option<String> {
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok();
    let first = cmdline
        .as_deref()
        .and_then(|c| c.split(|b| *b == 0).next())
        .and_then(|a| std::str::from_utf8(a).ok())
        .and_then(crate::lifecycle::Launched::class_hint);
    first.or_else(|| {
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .ok()
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty())
    })
}

/// The ways `CompileEffect` compiles an effect: the four red/blue swap
/// combinations (`(swap_input, swap_output)` = ff, tf, ft, tt), with the
/// GPU's `fp16`. A chain swaps on the first effect's read and the last
/// one's write, so these cover every place in either chain (before or after
/// the model): a session then finds the effect in the SPIR-V cache.
fn warm_up_options(fp16: bool) -> [lirtup_magpiefx::CompileOptions; 4] {
    let options = |swap_input, swap_output| lirtup_magpiefx::CompileOptions {
        fp16,
        swap_input,
        swap_output,
    };
    [options(false, false), options(true, false), options(false, true), options(true, true)]
}

/// `modes` with the entry of the previewed mode replaced by its unsaved
/// edits: both the parsed [`Mode`] and the raw mapping the resolver layers
/// (an id not in `modes/` yet is added).
fn apply_preview(modes: &ModeStore, preview: &Preview) -> ModeStore {
    let mut modes = modes.clone();
    let (mode, warnings) = Mode::from_mapping(&preview.mode, &preview.raw);
    for w in warnings {
        tracing::debug!("preview of {}: {}: {}", preview.mode, w.key, w.message);
    }
    modes.modes.insert(preview.mode.clone(), mode);
    modes.raw.insert(preview.mode.clone(), preview.raw.clone());
    modes
}

/// [`resolve_with_mode`], with the unsaved mode of `preview` in place of the
/// file of the same id.
///
/// The resolver only reads the mode it applies (`mode`, else the one the
/// profile or the top level names), so resolving on the patched store
/// changes the result exactly when that mode is the previewed one, whether
/// it is the profile's or the bar's temporary choice; any other mode
/// resolves as from the files.
fn resolve_previewed(
    file: &ConfigFile,
    modes: &ModeStore,
    choice: &ProfileChoice,
    cli: &lirtup_config::Mapping,
    mode: Option<&str>,
    preview: Option<&Preview>,
) -> std::result::Result<Resolved, ConfigError> {
    match preview.filter(|p| !p.mode.is_empty()) {
        Some(preview) => resolve_with_mode(file, &apply_preview(modes, preview), choice, cli, mode),
        None => resolve_with_mode(file, modes, choice, cli, mode),
    }
}

/// What the daemon thread holds: the engine, weakly (the engine owns the
/// thread).
struct DaemonLink(Weak<Inner>);

impl DaemonHost for DaemonLink {
    fn running_window(&self) -> Option<u32> {
        let inner = self.0.upgrade()?;
        let st = lock(&inner.state);
        st.live().map(|r| r.window.xid)
    }

    fn starting(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|inner| inner.starts.load(Ordering::SeqCst) > 0)
    }

    fn config_file(&self) -> ConfigFile {
        self.0
            .upgrade()
            .map(|inner| lock(&inner.files).file.clone())
            .unwrap_or_default()
    }

    fn start_matched(&self, window: WindowInfo, profile: String) {
        if let Some(inner) = self.0.upgrade() {
            inner.start_matched(window, profile);
        }
    }
}

impl Inner {
    // ---- events -------------------------------------------------------

    /// Update the snapshot from an event, then broadcast it.
    fn publish(&self, event: Event) {
        let mut finished = None;
        {
            let mut st = lock(&self.state);
            match &event {
                Event::SessionStarted(view) => {
                    st.view = Some(view.clone());
                    st.last_error = None;
                }
                Event::SessionStopped { .. } => {
                    if st.live().is_none() {
                        finished = st.session.take();
                        st.view = None;
                    }
                }
                Event::SessionError { stage, message, .. } => {
                    st.last_error = Some((*stage, message.clone()));
                }
                Event::FrameGen(status) => st.framegen = status.clone(),
                Event::DaemonState {
                    on,
                    keep_after_exit,
                    watching,
                } => {
                    st.daemon = DaemonStatus {
                        on: *on,
                        keep_after_exit: *keep_after_exit,
                        watching: *watching,
                    };
                }
                _ => {}
            }
        }
        // Joins a thread that has already ended.
        drop(finished);
        // No subscriber is fine.
        let _ = self.events.send(event);
    }

    fn log(&self, level: Level, message: impl Into<String>) {
        let message = message.into();
        match level {
            Level::Error => tracing::error!("{message}"),
            Level::Warn => tracing::warn!("{message}"),
            Level::Info => tracing::info!("{message}"),
            Level::Debug => tracing::debug!("{message}"),
        }
        self.publish(Event::Log { level, message });
    }

    /// A `Start` failed: `SessionError`, then `SessionStopped { Error }`, the
    /// same pair a failing session sends.
    fn fail(&self, stage: Stage, message: impl Into<String>, retry: bool) {
        let message = message.into();
        tracing::error!("session {stage:?}: {message}");
        self.publish(Event::SessionError {
            stage,
            message,
            retry,
        });
        self.publish(Event::SessionStopped {
            reason: StopReason::Error,
        });
    }

    /// The relay thread: session events → snapshot → broadcast.
    fn relay(&self, rx: &Receiver<Event>) {
        loop {
            if self.relay_stop.load(Ordering::SeqCst) {
                for event in rx.try_iter() {
                    self.publish(event);
                }
                return;
            }
            match rx.recv_timeout(RELAY_TICK) {
                Ok(event) => self.publish(event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    // ---- threads --------------------------------------------------------

    /// Run `work` on a detached thread named `name`.
    fn spawn(self: &Arc<Self>, name: &str, work: impl FnOnce(&Arc<Inner>) + Send + 'static) {
        let inner = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || work(&inner));
        if let Err(e) = spawned {
            self.log(Level::Error, format!("cannot start a thread: {e}"));
        }
    }

    fn start_hotkeys(self: &Arc<Self>) {
        let (bindings, warnings) = self.config.load().hotkey_bindings();
        for w in warnings {
            tracing::warn!("config: {}: {}", w.key, w.message);
        }
        let (hotkeys, rx, errors) = match Hotkeys::start(&bindings) {
            Ok(started) => started,
            Err(e) => {
                tracing::warn!("global hotkeys are off: {e}");
                return;
            }
        };
        for e in errors {
            tracing::warn!("hotkey: {e}");
        }
        *lock(&self.hotkeys) = Some(hotkeys);
        let inner = Arc::clone(self);
        // Ends when the hotkeys stop (their sender is dropped).
        let dispatcher = std::thread::Builder::new()
            .name("lirtup-hotkey-actions".into())
            .spawn(move || {
                for action in rx {
                    tracing::debug!("hotkey {}", action.as_str());
                    inner.hotkey(action);
                }
            });
        match dispatcher {
            Ok(thread) => lock(&self.threads).push(thread),
            Err(e) => tracing::warn!("global hotkeys are off: {e}"),
        }
    }

    // ---- commands -------------------------------------------------------

    fn handle(self: &Arc<Self>, command: Command) -> Result<()> {
        if self.stopping.load(Ordering::SeqCst) {
            return Ok(());
        }
        match command {
            Command::Hello { .. } => {
                // The server answers it; a direct caller gets the snapshot
                // by `Engine::snapshot`.
            }
            Command::Start {
                target,
                kind,
                delay_s,
                profile,
            } => self.start(StartRequest {
                target,
                kind,
                delay_s,
                profile,
            }),
            Command::Stop => self.stop_session(StopReason::User),
            Command::Restart => self.restart(),
            Command::ReloadConfig => self.reload()?,
            Command::SetDaemon { on, keep_after_exit } => self.set_daemon(on, keep_after_exit),
            Command::Hotkey { action } => self.hotkey(action),
            Command::PickWindow => self.spawn("lirtup-pick", |inner| {
                let picked = X11::connect().and_then(|x| x.pick_window());
                let window = match picked {
                    Ok(w) => w.as_ref().map(listing::window_ref),
                    Err(e) => {
                        inner.log(Level::Error, format!("cannot pick a window: {e}"));
                        None
                    }
                };
                inner.publish(Event::WindowPicked { window });
            }),
            Command::ListEffects => self.spawn("lirtup-effects", |inner| {
                inner.publish(Event::Effects {
                    effects: listing::effects(),
                });
            }),
            Command::CompileEffect { name } => self.spawn("lirtup-compile", move |inner| {
                inner.compile_effect(&name);
            }),
            Command::ListDevices => self.spawn("lirtup-devices", |inner| {
                let monitors = X11::connect()
                    .map_err(CoreError::from)
                    .and_then(|x| listing::monitors(&x))
                    .unwrap_or_else(|e| {
                        inner.log(Level::Warn, format!("cannot list monitors: {e}"));
                        Vec::new()
                    });
                let gpus = inner.gpus().unwrap_or_else(|e| {
                    inner.log(Level::Warn, format!("cannot list GPUs: {e}"));
                    Vec::new()
                });
                inner.publish(Event::Devices { monitors, gpus });
            }),
            Command::FrameGenProbe => {
                let status = self.framegen_status();
                self.publish(Event::FrameGen(status));
            }
            Command::Bench { req } => self.bench(&req)?,
            Command::CancelBench => self.cancel_bench(),
            Command::Diagnostics => self.spawn("lirtup-diagnostics", |inner| {
                let report = inner.diagnostics();
                inner.publish(Event::Diagnostics(report));
            }),
            Command::WatchGpuTimes { on } => self.watch_gpu_times(on),
            Command::PauseHotkeys { on } => self.pause_hotkeys(on),
            Command::Preview { profile, mode, raw } => {
                if mode.is_empty() {
                    self.log(Level::Warn, "Preview: no mode id");
                } else {
                    self.set_preview(Some(Preview { profile, mode, raw }));
                }
            }
            Command::PreviewEnd => self.set_preview(None),
            Command::Quit => {
                self.stop_session(StopReason::Quit);
                self.quit.send_replace(true);
            }
        }
        Ok(())
    }

    /// `Preview` (`Some`) or `PreviewEnd` (`None`): keep it in the running
    /// session and send the session its configuration again. A new preview
    /// resolves from the files as last read; ending one reads them again
    /// (`ReloadConfig`), so the session never shows the stale file of a mode
    /// the GUI has just saved. Nothing happens without a session, or when
    /// there is no preview to end.
    fn set_preview(self: &Arc<Self>, preview: Option<Preview>) {
        let what = preview
            .as_ref()
            .map(|p| format!("mode {:?} (profile {:?})", p.mode, p.profile));
        let had = {
            let mut st = lock(&self.state);
            match st.session.as_mut().filter(|r| !r.session.is_finished()) {
                Some(running) => {
                    let had = running.preview.is_some();
                    running.preview = preview;
                    Some(had)
                }
                None => None,
            }
        };
        match (had, what) {
            (None, Some(what)) => tracing::debug!("Preview of {what}: no session, ignored"),
            (None, None) | (Some(false), None) => tracing::debug!("PreviewEnd: no preview"),
            (Some(_), Some(what)) => {
                tracing::debug!("previewing {what}");
                self.update_session();
            }
            (Some(true), None) => {
                tracing::debug!("preview ended");
                if let Err(e) = self.reload() {
                    self.log(Level::Error, format!("cannot read the configuration again: {e}"));
                }
            }
        }
    }

    fn pause_hotkeys(&self, on: bool) {
        let mut st = lock(&self.state);
        let before = st.hotkey_pausers > 0;
        st.hotkey_pausers = if on {
            st.hotkey_pausers.saturating_add(1)
        } else {
            st.hotkey_pausers.saturating_sub(1)
        };
        let after = st.hotkey_pausers > 0;
        drop(st);
        if before != after {
            tracing::debug!("global hotkeys {}", if after { "paused" } else { "resumed" });
            self.rebind_hotkeys();
        }
    }

    /// Turn the daemon on or off (and its `keep_after_exit`), then publish
    /// `DaemonState`.
    fn set_daemon(self: &Arc<Self>, on: bool, keep_after_exit: bool) {
        {
            let mut slot = lock(&self.daemon);
            match (on, slot.is_some()) {
                (true, false) => {
                    let poll = daemon_poll(&self.config.load());
                    let link: Arc<dyn DaemonHost> = Arc::new(DaemonLink(Arc::downgrade(self)));
                    match Daemon::spawn(link, poll) {
                        Ok(daemon) => *slot = Some(DaemonThread { daemon, poll }),
                        Err(e) => {
                            drop(slot);
                            self.log(Level::Error, format!("cannot start the daemon: {e}"));
                            return;
                        }
                    }
                }
                (false, true) => {
                    if let Some(thread) = slot.take() {
                        thread.daemon.stop();
                    }
                }
                _ => {}
            }
        }
        let watching = watching(&lock(&self.files).file);
        self.publish(Event::DaemonState {
            on,
            keep_after_exit,
            watching,
        });
    }

    /// The daemon found `window` for `profile`: `DaemonMatch`, then a
    /// `Start` after `target_delay`, in the overlay mode of that profile.
    fn start_matched(self: &Arc<Self>, window: WindowInfo, profile: String) {
        let kind = match self.resolve_with(&window, Some(&profile), None, None) {
            Ok(r) if r.config.overlay_mode == lirtup_config::OverlayMode::Windowed => SessionKind::Windowed,
            _ => SessionKind::Fullscreen,
        };
        let delay = self.config.load().target_delay;
        self.publish(Event::DaemonMatch {
            window: listing::window_ref(&window),
            profile: profile.clone(),
        });
        self.start(StartRequest {
            target: Target::Window(window.xid),
            kind,
            delay_s: Some(delay as f32).filter(|d| *d > 0.0),
            profile: Some(profile),
        });
    }

    /// `ReloadConfig` with the daemon on: a new `daemon_poll_interval`
    /// restarts its thread.
    fn restart_daemon_if_needed(self: &Arc<Self>) {
        let poll = daemon_poll(&self.config.load());
        let mut slot = lock(&self.daemon);
        if slot.as_ref().is_some_and(|t| t.poll != poll)
            && let Some(old) = slot.take()
        {
            old.daemon.stop();
            let link: Arc<dyn DaemonHost> = Arc::new(DaemonLink(Arc::downgrade(self)));
            match Daemon::spawn(link, poll) {
                Ok(daemon) => *slot = Some(DaemonThread { daemon, poll }),
                Err(e) => tracing::error!("cannot restart the daemon: {e}"),
            }
        }
    }

    /// Grab the configured hotkeys again, or none while they are paused.
    fn rebind_hotkeys(&self) {
        let bindings = if lock(&self.state).hotkey_pausers > 0 {
            Vec::new()
        } else {
            self.config.load().hotkey_bindings().0
        };
        if let Some(hotkeys) = lock(&self.hotkeys).as_ref() {
            match hotkeys.rebind(&bindings) {
                Ok(errors) => errors.iter().for_each(|e| tracing::warn!("hotkey: {e}")),
                Err(e) => tracing::warn!("cannot rebind the hotkeys: {e}"),
            }
        }
    }

    fn watch_gpu_times(&self, on: bool) {
        let mut st = lock(&self.state);
        let before = st.gpu_watchers > 0;
        st.gpu_watchers = if on {
            st.gpu_watchers.saturating_add(1)
        } else {
            st.gpu_watchers.saturating_sub(1)
        };
        let after = st.gpu_watchers > 0;
        if before != after
            && let Some(running) = st.live()
        {
            running.session.send(PipelineCommand::SetTimestamps(after));
        }
    }

    fn hotkey(self: &Arc<Self>, action: HotkeyAction) {
        match action {
            HotkeyAction::StartFullscreen | HotkeyAction::StartWindowed => {
                if lock(&self.state).live().is_some() {
                    self.stop_session(StopReason::User);
                } else {
                    let kind = if action == HotkeyAction::StartWindowed {
                        SessionKind::Windowed
                    } else {
                        SessionKind::Fullscreen
                    };
                    self.start(StartRequest {
                        target: Target::Focused,
                        kind,
                        delay_s: None,
                        profile: None,
                    });
                }
            }
            HotkeyAction::ExitApp => self.stop_session(StopReason::User),
            other => {
                let st = lock(&self.state);
                if let Some(running) = st.live() {
                    running.session.send(PipelineCommand::Hotkey(other));
                }
            }
        }
    }

    // ---- sessions -------------------------------------------------------

    fn cancelled(&self, generation: u64) -> bool {
        self.stopping.load(Ordering::SeqCst) || self.start_gen.load(Ordering::SeqCst) != generation
    }

    /// Take the session out of the state and stop it (on this thread).
    fn stop_session(&self, reason: StopReason) {
        // Also cancels a start that is still looking for its window.
        self.start_gen.fetch_add(1, Ordering::SeqCst);
        let running = lock(&self.state).session.take();
        if let Some(running) = running {
            tracing::info!("stopping the session ({reason:?})");
            running.session.stop(reason, STOP_TIMEOUT);
        }
    }

    fn start(self: &Arc<Self>, request: StartRequest) {
        let generation = self.start_gen.fetch_add(1, Ordering::SeqCst) + 1;
        self.starts.fetch_add(1, Ordering::SeqCst);
        let inner = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("lirtup-start".into())
            .spawn(move || {
                inner.run_start(generation, request);
                inner.starts.fetch_sub(1, Ordering::SeqCst);
            });
        if let Err(e) = spawned {
            self.starts.fetch_sub(1, Ordering::SeqCst);
            self.log(Level::Error, format!("cannot start a thread: {e}"));
        }
    }

    /// Find the window, resolve its configuration, open the device and start
    /// the session (on a start thread).
    fn run_start(self: &Arc<Self>, generation: u64, request: StartRequest) {
        let config = self.config.load_full();
        let x11 = match X11::connect() {
            Ok(x) => x,
            Err(e) => return self.fail(Stage::Window, e.to_string(), true),
        };
        let window = match self.find_window(&x11, &request, &config, generation) {
            _ if self.cancelled(generation) => return,
            Ok(Some(window)) => window,
            Ok(None) => {
                return self.fail(
                    Stage::Window,
                    format!("no window found for {:?}", request.target),
                    true,
                );
            }
            Err(e) => return self.fail(Stage::Window, e.to_string(), true),
        };
        // A window named by the caller (a tile, `-t`, `--select`, the
        // daemon) comes to the front first, as 1.x does.
        if !matches!(request.target, Target::Focused)
            && let Err(e) = x11.activate_window(window.xid)
        {
            tracing::debug!("cannot focus 0x{:08x}: {e}", window.xid);
        }
        drop(x11);

        let profile = request.profile.clone().or_else(|| self.options.profile.clone());
        // A new session starts without a preview (the old session's, if
        // any, dies with it).
        let resolved = match self.resolve_with(&window, profile.as_deref(), None, None) {
            Ok(r) => r,
            // 1.x (`config/setup.py`): an unknown `-p` warns and goes on
            // with the base configuration.
            Err(CoreError::Config(lirtup_config::ConfigError::UnknownProfile(name))) => {
                self.log(
                    Level::Warn,
                    format!("no profile named {name:?}: using the base configuration"),
                );
                match self.resolve_with(&window, None, None, None) {
                    Ok(r) => r,
                    Err(e) => return self.fail(Stage::Config, e.to_string(), false),
                }
            }
            Err(e) => return self.fail(Stage::Config, e.to_string(), false),
        };
        tracing::info!(
            "scaling 0x{:08x} {:?} (profile {:?}, mode {:?})",
            window.xid,
            window.title,
            resolved.profile,
            resolved.mode
        );
        self.publish(Event::SessionStarting {
            target: listing::window_ref(&window),
        });

        let device = match self.device(&resolved.config) {
            Ok(d) => d,
            Err(e) => return self.fail(Stage::Device, e.to_string(), true),
        };
        if self.cancelled(generation) {
            return;
        }

        // One session at a time.
        let old = lock(&self.state).session.take();
        if let Some(old) = old {
            old.session.stop(StopReason::Replaced, STOP_TIMEOUT);
        }

        let setup = SessionSetup {
            device,
            window: window.clone(),
            kind: request.kind,
            config: Arc::new(resolved.config),
            profile: resolved.profile,
            mode: resolved.mode,
            layer: self.layer.clone(),
            events: self.pipeline_tx.clone(),
            timestamps: lock(&self.state).gpu_watchers > 0,
            resolve: Some({
                let inner = Arc::clone(self);
                let profile = profile.clone();
                Arc::new(move |window: &WindowInfo, mode: Option<&str>| {
                    match inner.resolve_for(window, profile.as_deref(), mode) {
                        Ok(resolved) => Some(resolved),
                        Err(e) => {
                            tracing::warn!("cannot resolve the configuration of {}: {e}", window.title);
                            None
                        }
                    }
                })
            }),
            modes: Some({
                let inner = Arc::clone(self);
                Arc::new(move || inner.mode_entries())
            }),
        };
        match Session::start(setup) {
            Ok(session) => {
                let mut st = lock(&self.state);
                if self.cancelled(generation) {
                    drop(st);
                    session.stop(StopReason::Replaced, STOP_TIMEOUT);
                    return;
                }
                st.last_start = Some((window.xid, request.kind, request.profile.clone()));
                st.session = Some(Running {
                    session,
                    window,
                    profile: request.profile,
                    preview: None,
                });
            }
            Err(e) => self.fail(e.stage(), e.to_string(), false),
        }
    }

    /// The window of a `Start`, after its delay.
    fn find_window(
        &self,
        x11: &X11,
        request: &StartRequest,
        config: &Config,
        generation: u64,
    ) -> Result<Option<WindowInfo>> {
        let delay = request
            .delay_s
            .filter(|d| d.is_finite() && *d > 0.0)
            .map(Duration::from_secs_f32);
        if let Target::Focused = request.target {
            let window = match delay {
                Some(d) => x11.active_after(d)?,
                None => x11.active_window()?,
            };
            // The activation hotkey pressed over the GUI, or over a stale
            // overlay: never scale LirtUp Forge itself.
            if let Some(w) = &window
                && x11.is_lirtup_window(w)?
            {
                return Err(CoreError::Session {
                    stage: Stage::Window,
                    message: format!(
                        "the focused window is LirtUp Forge itself ({}); focus the window to scale",
                        w.title
                    ),
                });
            }
            return Ok(window);
        }
        if let Some(delay) = delay {
            // In small steps, so a `Stop` cancels it.
            let end = std::time::Instant::now() + delay;
            while std::time::Instant::now() < end {
                if self.cancelled(generation) {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(50).min(end - std::time::Instant::now()));
            }
        }
        Ok(match &request.target {
            Target::Window(xid) => x11.window_info(*xid)?,
            Target::Title(title) => x11.find_by_title(Some(title), None)?,
            Target::Pid(pid) => Some(x11.wait_for_pid(&PidSearch {
                pid: *pid,
                class_hint: class_hint(*pid),
                pid_timeout: config.pid_timeout,
                class_timeout: config.class_timeout,
                total_timeout: config.total_timeout,
                starting_phase: config.starting_phase,
            })?),
            Target::Focused => unreachable!("handled above"),
        })
    }

    /// The configuration of one window: the named profile, or the one whose
    /// `match` fits; `mode` replaces the mode it names (the bar's mode chip,
    /// [`resolve_with_mode`]), with the preview of the running session
    /// ([`resolve_previewed`]). This is the `SessionSetup.resolve` of the
    /// session thread (the bar's temporary mode, a window switch with
    /// `follow_focus`), so those show the preview too; a new session
    /// resolves with [`Inner::resolve_with`] and no preview.
    fn resolve_for(
        &self,
        window: &WindowInfo,
        profile: Option<&str>,
        mode: Option<&str>,
    ) -> Result<Resolved> {
        // Never hold `state` and `files` at once: clone, release, resolve.
        let preview = lock(&self.state).live().and_then(|r| r.preview.clone());
        self.resolve_with(window, profile, mode, preview.as_ref())
    }

    /// [`Inner::resolve_for`] with an explicit preview (`None`: the files).
    fn resolve_with(
        &self,
        window: &WindowInfo,
        profile: Option<&str>,
        mode: Option<&str>,
        preview: Option<&Preview>,
    ) -> Result<Resolved> {
        let choice = match profile {
            Some(name) => ProfileChoice::Named(name.into()),
            None => ProfileChoice::Auto {
                title: window.title.clone(),
                width: window.width(),
                height: window.height(),
            },
        };
        let files = lock(&self.files);
        Ok(resolve_previewed(
            &files.file,
            &files.modes,
            &choice,
            &self.options.cli_overrides,
            mode,
            preview,
        )?)
    }

    /// The modes of `modes/`, for the bar's mode chip (read again after
    /// every `ReloadConfig`).
    fn mode_entries(&self) -> Vec<ModeEntry> {
        lock(&self.files)
            .modes
            .modes
            .iter()
            .map(|(id, mode)| ModeEntry {
                id: id.clone(),
                name: mode.name.clone(),
            })
            .collect()
    }

    fn instance(&self) -> Result<Arc<Instance>> {
        let mut gpu = lock(&self.gpu);
        if let Some(instance) = &gpu.instance {
            return Ok(Arc::clone(instance));
        }
        let instance = Instance::new(&InstanceOptions {
            app_name: "lirtup".into(),
            debug: vulkan_debug(),
        })?;
        gpu.instance = Some(Arc::clone(&instance));
        Ok(instance)
    }

    /// The device, opened on the first session and kept until shutdown.
    fn device(&self, config: &Config) -> Result<Arc<Device>> {
        let instance = self.instance()?;
        let mut gpu = lock(&self.gpu);
        if let Some(device) = &gpu.device {
            return Ok(Arc::clone(device));
        }
        let selector = listing::gpu_selector(config);
        let index = instance
            .select_adapter(selector.as_deref())
            .ok_or_else(|| GpuError::NoDevice("no Vulkan device".into()))?;
        let adapter = &instance.adapters()[index];
        tracing::info!("GPU: {} ({})", adapter.name, adapter.driver);
        let device = instance.create_device(index)?;
        gpu.device = Some(Arc::clone(&device));
        Ok(device)
    }

    fn gpus(&self) -> Result<Vec<lirtup_proto::GpuInfo>> {
        let instance = self.instance()?;
        let selector = listing::gpu_selector(&self.config.load());
        Ok(listing::gpus(&instance, selector.as_deref()))
    }

    // ---- restart and reload --------------------------------------------

    /// Whether the frame generation settings changed in a way only a new
    /// process applies (the layer reads them when it loads).
    fn layer_needs_exec(&self) -> bool {
        if self.layer.state == FrameGenState::External {
            return false;
        }
        let now = Prepared::of(&self.config.load());
        let layer_involved =
            self.prepared.engine == FrameGenEngine::Layer || now.engine == FrameGenEngine::Layer;
        layer_involved && now != self.prepared
    }

    fn restart(self: &Arc<Self>) {
        if self.layer_needs_exec() {
            // The session and the daemon, for the new process.
            let (resume, daemon) = {
                let st = lock(&self.state);
                let resume = st
                    .live()
                    .and(st.last_start.clone())
                    .map(|(xid, kind, profile)| Resume { xid, kind, profile });
                (resume, st.daemon)
            };
            self.stop_session(StopReason::Restart);
            // Release the grabs, the daemon and the X connections before
            // `exec`.
            if let Some(hotkeys) = lock(&self.hotkeys).take() {
                hotkeys.stop();
            }
            let daemon_thread = lock(&self.daemon).take();
            if let Some(thread) = daemon_thread {
                thread.daemon.stop();
            }
            self.cancel_bench();
            let resume = resume.map(|r| r.encode());
            let mut env: Vec<(&str, &str)> = Vec::new();
            if let Some(resume) = &resume {
                env.push((RESUME_ENV, resume));
            }
            if daemon.on {
                env.push((DAEMON_ENV, if daemon.keep_after_exit { "keep" } else { "on" }));
            }
            let error = layer::restart_self(&env);
            self.log(Level::Error, format!("cannot restart lirtup: {error}"));
            self.start_hotkeys();
            if daemon.on {
                self.set_daemon(true, daemon.keep_after_exit);
            }
            return;
        }
        let last = lock(&self.state).last_start.clone();
        match last {
            Some((xid, kind, profile)) => {
                self.stop_session(StopReason::Restart);
                self.start(StartRequest {
                    target: Target::Window(xid),
                    kind,
                    delay_s: None,
                    profile,
                });
            }
            None => self.log(Level::Info, "Restart: no session to restart"),
        }
    }

    /// `ReloadConfig`: read the files again, rebind the hotkeys, send the
    /// session its new configuration.
    fn reload(self: &Arc<Self>) -> Result<()> {
        let files = load_files(&self.options)?;
        let config = top_level(&files, &self.options)?;
        let watching = watching(&files.file);
        *lock(&self.files) = files;
        self.config.store(Arc::new(config));
        let config = self.config.load_full();

        self.rebind_hotkeys();
        self.restart_daemon_if_needed();

        let daemon = lock(&self.state).daemon;
        if daemon.watching != watching {
            self.publish(Event::DaemonState {
                on: daemon.on,
                keep_after_exit: daemon.keep_after_exit,
                watching,
            });
        }

        // The preview, if any, stays (the GUI ends it with `PreviewEnd`).
        if !self.update_session() {
            let status = status_from_setup(&config, &self.layer);
            if status != lock(&self.state).framegen {
                self.publish(Event::FrameGen(status));
            }
        }
        if self.layer_needs_exec() {
            self.log(
                Level::Info,
                "the frame generation changes apply after Restart (lirtup restarts itself)",
            );
        }
        Ok(())
    }

    /// Resolve the running session's configuration again (the files as
    /// last read, plus its preview) and send it `UpdateConfig`. `false` when
    /// no session runs.
    fn update_session(&self) -> bool {
        let live = {
            let st = lock(&self.state);
            st.live()
                .map(|r| (r.window.clone(), r.profile.clone(), r.preview.clone()))
        };
        let Some((window, profile, preview)) = live else {
            return false;
        };
        let profile = profile.or_else(|| self.options.profile.clone());
        match self.resolve_with(&window, profile.as_deref(), None, preview.as_ref()) {
            Ok(resolved) => {
                let st = lock(&self.state);
                if let Some(running) = st.live() {
                    running.session.send(PipelineCommand::UpdateConfig {
                        config: Arc::new(resolved.config),
                        profile: resolved.profile,
                        mode: resolved.mode,
                        preview: preview.is_some(),
                    });
                }
            }
            Err(e) => self.log(Level::Error, format!("the new configuration does not apply: {e}")),
        }
        true
    }

    fn framegen_status(&self) -> FrameGenStatus {
        let st = lock(&self.state);
        if st.live().is_some() {
            st.framegen.clone()
        } else {
            drop(st);
            status_from_setup(&self.config.load(), &self.layer)
        }
    }

    // ---- effects, bench, diagnostics -----------------------------------

    fn compile_effect(&self, name: &str) {
        self.publish(Event::EffectCompile {
            name: name.into(),
            done: false,
            error: None,
            session: false,
        });
        let fp16 = {
            let gpu = lock(&self.gpu);
            gpu.device
                .as_ref()
                .is_some_and(|d| lirtup_magpiefx::runtime::fp16_wanted(d))
        };
        let result = (|| -> Result<()> {
            let catalog = listing::catalog();
            let entry = catalog.resolve(name)?;
            let compilers = lirtup_magpiefx::compiler::detect_compilers();
            let cache = lirtup_magpiefx::compiler::SpirvCache::new(paths::magpiefx_cache_dir());
            // Every place the effect may take in a chain; the first error
            // stops (the others would most likely fail the same way).
            for options in warm_up_options(fp16) {
                lirtup_magpiefx::runtime::compile_effect_with(&entry, &compilers, &cache, &options)?;
            }
            Ok(())
        })();
        let error = result.err().map(|e| e.to_string());
        if let Some(e) = &error {
            tracing::warn!("{name}: {e}");
        }
        self.publish(Event::EffectCompile {
            name: name.into(),
            done: true,
            error,
            session: false,
        });
    }

    /// `lirtup bench --json <request>` in a child process; its lines are
    /// relayed as they come.
    fn bench(self: &Arc<Self>, request: &lirtup_proto::BenchRequest) -> Result<()> {
        use std::io::BufRead;
        use std::process::{Command as Process, Stdio};

        let mut slot = lock(&self.bench);
        if slot.is_some() {
            self.log(Level::Warn, "a measurement is already running");
            return Ok(());
        }
        let exe = std::env::current_exe()?;
        let json = serde_json::to_string(request).expect("BenchRequest serializes");
        let mut process = Process::new(exe);
        process.arg("bench").arg("--json").arg(json);
        if let Some(gpu) = listing::gpu_selector(&self.config.load()) {
            process.arg("--gpu").arg(gpu);
        }
        let mut child = process.stdin(Stdio::null()).stdout(Stdio::piped()).spawn()?;
        *slot = Some(child.id());
        drop(slot);
        let stdout = child.stdout.take().expect("stdout is piped");

        self.spawn("lirtup-bench", move |inner| {
            let mut finished = false;
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                match lirtup_proto::decode_line::<Event>(&line) {
                    Ok(event) => {
                        finished |= matches!(event, Event::BenchResult(_));
                        inner.publish(event);
                    }
                    Err(e) => tracing::debug!("bench: {e}: {line}"),
                }
            }
            // Cleared before reaping, so `cancel_bench` never signals a pid
            // the system may already have reused.
            *lock(&inner.bench) = None;
            let status = child.wait();
            if !finished {
                let why = match status {
                    Ok(s) => s.to_string(),
                    Err(e) => e.to_string(),
                };
                inner.log(
                    Level::Error,
                    format!("the measurement ended without a result ({why})"),
                );
                inner.publish(Event::BenchResult(BenchReport {
                    gpu: String::new(),
                    entries: Vec::new(),
                    cancelled: true,
                }));
            }
        });
        Ok(())
    }

    /// SIGTERM to the bench child, which then prints its partial result.
    #[allow(unsafe_code)]
    fn cancel_bench(&self) {
        let Some(pid) = *lock(&self.bench) else { return };
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return;
        };
        // SAFETY: `kill` takes plain integers and touches no memory of ours.
        // The pid is our own child, still unreaped while `self.bench` holds
        // it (the relay thread clears it after `wait`).
        if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
            let e = std::io::Error::last_os_error();
            self.log(Level::Error, format!("cannot stop the measurement: {e}"));
        }
    }

    fn diagnostics(&self) -> DiagReport {
        let config = self.config.load_full();
        let gpus = self.gpus().unwrap_or_else(|e| {
            tracing::warn!("diagnostics: {e}");
            Vec::new()
        });
        let vulkan = lock(&self.gpu).instance.as_ref().and_then(|i| {
            let selected = gpus.iter().find(|g| g.selected)?;
            i.adapters()
                .get(selected.index as usize)
                .map(|a| a.api_version.clone())
        });
        let display = match X11::connect() {
            Ok(x) if x.is_xwayland() => "XWayland".to_owned(),
            Ok(_) => "X11".to_owned(),
            Err(e) => format!("no X display ({e})"),
        };
        let (dxc, glslang) = listing::compiler_versions();
        DiagReport {
            version: VERSION.into(),
            gpus,
            vulkan,
            display,
            framegen: self.framegen_status(),
            lossless_dll: lirtup_fg::dll::find_dll(
                lirtup_fg::dll::DllKind::Lossless,
                config.frame_gen_dll.as_deref(),
            )
            .map(|p| p.display().to_string()),
            dxc,
            glslang,
            config_dir: paths::config_dir().display().to_string(),
        }
    }

    // ---- shutdown --------------------------------------------------------

    fn shutdown(&self, timeout: Duration) {
        if self.stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        // The daemon first, so it starts nothing more.
        let daemon = lock(&self.daemon).take();
        if let Some(thread) = daemon {
            thread.daemon.stop();
        }
        self.start_gen.fetch_add(1, Ordering::SeqCst);
        let running = lock(&self.state).session.take();
        if let Some(running) = running {
            running.session.stop(StopReason::Quit, timeout);
        }
        self.cancel_bench();
        if let Some(hotkeys) = lock(&self.hotkeys).take() {
            hotkeys.stop();
        }
        self.relay_stop.store(true, Ordering::SeqCst);
        let threads = std::mem::take(&mut *lock(&self.threads));
        let me = std::thread::current().id();
        for thread in threads {
            // `Drop` may run on one of them (the last `Arc` of a client).
            if thread.thread().id() != me {
                let _ = thread.join();
            }
        }
        self.quit.send_replace(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lirtup_config::{Mapping, Model, Profile, Value};

    /// A mapping of string keys and string or float values.
    fn map(entries: &[(&str, Value)]) -> Mapping {
        entries
            .iter()
            .map(|(k, v)| (Value::String((*k).into()), v.clone()))
            .collect()
    }

    fn s(text: &str) -> Value {
        Value::String(text.into())
    }

    /// Modes `calidad` (`faster`, CAS 0.4) and `rapido` (`none`).
    fn store() -> ModeStore {
        let mut store = ModeStore::default();
        for (id, raw) in [
            (
                "calidad",
                map(&[
                    ("name", s("Calidad")),
                    ("model", s("faster")),
                    ("cas_strength", Value::from(0.4)),
                ]),
            ),
            ("rapido", map(&[("name", s("Rápido")), ("model", s("none"))])),
        ] {
            store.modes.insert(id.into(), Mode::from_mapping(id, &raw).0);
            store.raw.insert(id.into(), raw);
        }
        store
    }

    /// The top level names `calidad`; profile `Ys` names `rapido`, profile
    /// `Loose` names `calidad` with a loose `model` of its own.
    fn file() -> ConfigFile {
        let mut file = ConfigFile {
            general: map(&[("mode", s("calidad"))]),
            ..ConfigFile::default()
        };
        file.profiles.insert(
            "Ys".into(),
            Profile {
                options: map(&[("mode", s("rapido"))]),
                ..Profile::default()
            },
        );
        file.profiles.insert(
            "Loose".into(),
            Profile {
                options: map(&[("mode", s("calidad")), ("model", s("4x16"))]),
                ..Profile::default()
            },
        );
        file
    }

    fn preview(mode: &str, raw: Mapping) -> Preview {
        Preview {
            profile: None,
            mode: mode.into(),
            raw,
        }
    }

    /// `calidad` with the unsaved edits: `4x12`, CAS 0.9.
    fn edited() -> Preview {
        preview(
            "calidad",
            map(&[
                ("name", s("Calidad (editado)")),
                ("model", s("4x12")),
                ("cas_strength", Value::from(0.9)),
            ]),
        )
    }

    fn resolved(choice: &ProfileChoice, mode: Option<&str>, preview: Option<&Preview>) -> Resolved {
        resolve_previewed(&file(), &store(), choice, &Mapping::new(), mode, preview).unwrap()
    }

    fn named(name: &str) -> ProfileChoice {
        ProfileChoice::Named(name.into())
    }

    #[test]
    fn apply_preview_replaces_the_entry_in_both_maps() {
        let original = store();
        let edited = edited();
        let patched = apply_preview(&original, &edited);
        assert_eq!(patched.raw["calidad"], edited.raw);
        assert_eq!(patched.modes["calidad"].name, "Calidad (editado)");
        assert_eq!(patched.modes["calidad"].settings.model, Model::M4x12);
        // In place: the order of the modes (the bar's list) is kept.
        let ids: Vec<&str> = patched.modes.keys().map(String::as_str).collect();
        assert_eq!(ids, ["calidad", "rapido"]);
        // The other modes and the original store are untouched.
        assert_eq!(patched.raw["rapido"], original.raw["rapido"]);
        assert_eq!(patched.modes["rapido"], original.modes["rapido"]);
        assert_eq!(original, store());
    }

    #[test]
    fn apply_preview_adds_a_mode_not_saved_yet() {
        let new = preview("nuevo", map(&[("model", s("4x12"))]));
        let patched = apply_preview(&store(), &new);
        assert_eq!(patched.modes.len(), 3);
        // No `name`: the id.
        assert_eq!(patched.modes["nuevo"].name, "nuevo");
        assert_eq!(patched.raw["nuevo"], new.raw);
    }

    #[test]
    fn the_named_mode_shows_the_preview() {
        let base = resolved(&ProfileChoice::None, None, None);
        assert_eq!(base.config.scaling.model, Model::Faster);
        let r = resolved(&ProfileChoice::None, None, Some(&edited()));
        assert_eq!(r.mode.as_deref(), Some("calidad"));
        assert_eq!(r.config.scaling.model, Model::M4x12);
        assert!((r.config.scaling.cas_strength - 0.9).abs() < 1e-6);
    }

    #[test]
    fn another_mode_resolves_from_the_files() {
        // Profile `Ys` names `rapido`: the preview of `calidad` changes nothing.
        let without = resolved(&named("Ys"), None, None);
        let with = resolved(&named("Ys"), None, Some(&edited()));
        assert_eq!(with.mode.as_deref(), Some("rapido"));
        assert_eq!(with.config, without.config);
        assert_eq!(with.config.scaling.model, Model::None);
    }

    #[test]
    fn the_bars_temporary_mode_shows_the_preview() {
        // The chip chose `calidad` over the profile's `rapido`.
        let r = resolved(&named("Ys"), Some("calidad"), Some(&edited()));
        assert_eq!(r.mode.as_deref(), Some("calidad"));
        assert_eq!(r.named_mode.as_deref(), Some("rapido"));
        assert_eq!(r.config.scaling.model, Model::M4x12);
        // And a temporary mode other than the previewed one does not.
        let r = resolved(&ProfileChoice::None, Some("rapido"), Some(&edited()));
        assert_eq!(r.config.scaling.model, Model::None);
    }

    #[test]
    fn loose_fields_of_the_profile_still_win() {
        // As saving the mode would: `Loose` sets `model` itself.
        let r = resolved(&named("Loose"), None, Some(&edited()));
        assert_eq!(r.config.scaling.model, Model::M4x16);
        assert!((r.config.scaling.cas_strength - 0.9).abs() < 1e-6);
    }

    #[test]
    fn a_mode_not_saved_yet_previews_as_the_temporary_mode() {
        let new = preview("nuevo", map(&[("model", s("4x12"))]));
        let r = resolved(&ProfileChoice::None, Some("nuevo"), Some(&new));
        assert_eq!(r.mode.as_deref(), Some("nuevo"));
        assert_eq!(r.config.scaling.model, Model::M4x12);
        // Without the preview it is unknown.
        let err = resolve_previewed(
            &file(),
            &store(),
            &ProfileChoice::None,
            &Mapping::new(),
            Some("nuevo"),
            None,
        );
        assert!(matches!(err, Err(ConfigError::UnknownMode(id)) if id == "nuevo"));
    }

    #[test]
    fn warm_up_covers_every_swap_combination() {
        for fp16 in [false, true] {
            let all = warm_up_options(fp16);
            assert!(all.iter().all(|o| o.fp16 == fp16));
            let mut combos: Vec<(bool, bool)> = all.iter().map(|o| (o.swap_input, o.swap_output)).collect();
            combos.sort_unstable();
            combos.dedup();
            assert_eq!(combos, [(false, false), (false, true), (true, false), (true, true)]);
        }
    }

    #[test]
    fn a_preview_without_an_id_is_ignored() {
        let empty = preview("", map(&[("model", s("4x12"))]));
        let r = resolved(&ProfileChoice::None, None, Some(&empty));
        assert_eq!(r.config.scaling.model, Model::Faster);
    }
}
