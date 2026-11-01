//! The contract between `lirtup serve` and `lirtup-gui` (6.5 of the
//! analysis). Re-exported as `lirtup_core::proto`.
//!
//! # Wire format
//!
//! A Unix stream socket at [`socket_path`] (`$XDG_RUNTIME_DIR/lirtup/lirtup.sock`).
//! One JSON object per line, UTF-8, `\n`-terminated. Every line is an
//! [`Envelope`]: the protocol version `v` plus the message, internally tagged
//! by `type`:
//!
//! ```text
//! {"v":1,"type":"Hello","version":1,"client":"lirtup-gui 2.0.0"}
//! {"v":1,"type":"Start","target":"focused","kind":"fullscreen","delay_s":3.0,"profile":null}
//! {"v":1,"type":"Stats","capture_fps":59.9, ...}
//! ```
//!
//! The client sends [`Command`]s; the server sends [`Event`]s. On connect the
//! client sends `Hello` and gets a `Snapshot` with the whole state, then only
//! events. Debuggable with `socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/lirtup/lirtup.sock`.
//!
//! # Rules
//!
//! - The GUI is the only writer of YAML. The scaler reads; what it discovers
//!   and must be saved (the menu bar crop) arrives as an event.
//! - Profiles and modes travel by name; the scaler reads the disk after
//!   `ReloadConfig`.
//! - `Hello.version` different from [`PROTOCOL_VERSION`] → the GUI warns and
//!   offers to restart the scaler.
//! - High-frequency data (`Stats`) is one sample every 500 ms, never one per
//!   frame.
//! - Window lists and thumbnails do not cross the socket (the GUI reads them
//!   with `lirtup-x11`).
//!
//! # Why a separate crate
//!
//! The pipeline fills `StatsSample` and the GUI reads it, and the pipeline is
//! below `lirtup-core`; a crate under both keeps the dependency graph
//! one-way and keeps Vulkan out of the GUI binary (it depends on this crate,
//! not on `lirtup-core`).

pub mod codec;
pub mod types;

pub use codec::{CodecError, Envelope, decode_line, encode_line, socket_path};
pub use types::*;

use serde::{Deserialize, Serialize};

/// Version of this protocol. Bumped on any incompatible change.
pub const PROTOCOL_VERSION: u32 = 1;

/// Client → server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Command {
    /// First message of a connection; answered with [`Event::Snapshot`].
    Hello {
        /// The client's [`PROTOCOL_VERSION`].
        version: u32,
        /// `lirtup-gui 2.0.0`, `lirtup ctl`, ...
        client: String,
    },
    /// Start scaling (replaces a running session).
    Start {
        /// Which window.
        target: Target,
        /// Fullscreen or windowed.
        kind: SessionKind,
        /// Wait this long first (deferred scaling; the target is taken after
        /// the delay when it is `Focused`).
        delay_s: Option<f32>,
        /// Profile by name; `None` = the one whose match fits, or the
        /// defaults.
        profile: Option<String>,
    },
    /// Stop the session.
    Stop,
    /// Restart with the same window and settings; with the lsfg-vk layer it
    /// `exec`s the scaler (the connection drops and the GUI reconnects).
    Restart,
    /// The GUI saved YAML: read config, modes and profiles again and apply
    /// what can be applied live.
    ReloadConfig,
    /// Daemon mode.
    SetDaemon {
        /// On or off.
        on: bool,
        /// Keep `lirtup serve` running after the GUI quits.
        keep_after_exit: bool,
    },
    /// Run a hotkey action as if pressed (tray, toolbar, `lirtup ctl`).
    Hotkey {
        /// The action.
        action: lirtup_config::HotkeyAction,
    },
    /// «Señalar ventana»: crosshair cursor; answered with
    /// [`Event::WindowPicked`].
    PickWindow,
    /// List the MagpieFX catalog; answered with [`Event::Effects`].
    ListEffects,
    /// Compile one effect ahead of time; progress in [`Event::EffectCompile`].
    CompileEffect {
        /// Catalog name.
        name: String,
    },
    /// List monitors and GPUs; answered with [`Event::Devices`].
    ListDevices,
    /// Check frame generation; answered with [`Event::FrameGen`].
    FrameGenProbe,
    /// «Medir en mi GPU».
    Bench {
        /// What to measure.
        req: BenchRequest,
    },
    /// Stop a running measurement.
    CancelBench,
    /// Collect diagnostics; answered with [`Event::Diagnostics`].
    Diagnostics,
    /// Stop the session and exit `lirtup serve`.
    Quit,
}

/// Server → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    /// Whole state, after `Hello`.
    Snapshot(EngineSnapshot),
    /// A `Start` was accepted (window found, setting up).
    SessionStarting {
        /// The window.
        target: WindowRef,
    },
    /// Scaling.
    SessionStarted(SessionView),
    /// The session ended.
    SessionStopped {
        /// Why.
        reason: StopReason,
    },
    /// The session failed (or could not start). Always followed by
    /// `SessionStopped { reason: Error }`, also when it failed before the
    /// session existed.
    SessionError {
        /// Where.
        stage: Stage,
        /// What (shown in the error card).
        message: String,
        /// Whether «Reintentar» makes sense.
        retry: bool,
    },
    /// Every 500 ms while scaling.
    Stats(StatsSample),
    /// Frame generation state changed (or after `FrameGenProbe`).
    FrameGen(FrameGenStatus),
    /// Daemon state changed.
    DaemonState {
        /// On or off.
        on: bool,
        /// Keep running after the GUI quits.
        keep_after_exit: bool,
        /// Profiles with match rules being watched.
        watching: u32,
    },
    /// The daemon found a window for a profile (it starts by itself).
    DaemonMatch {
        /// The window.
        window: WindowRef,
        /// The profile.
        profile: String,
    },
    /// A Wine menu bar was detected; the GUI saves the crop in the profile.
    MenuBarDetected {
        /// Profile to save it in (`None` = top level).
        profile: Option<String>,
        /// Crop to save.
        crop: lirtup_config::Crop,
    },
    /// Answer to `PickWindow` (`None` = cancelled).
    WindowPicked {
        /// The window.
        window: Option<WindowRef>,
    },
    /// Answer to `ListEffects`.
    Effects {
        /// The catalog.
        effects: Vec<EffectInfo>,
    },
    /// Progress of `CompileEffect` (and of compiles the session triggers).
    EffectCompile {
        /// Effect.
        name: String,
        /// Finished (with or without error).
        done: bool,
        /// Compiler error.
        error: Option<String>,
    },
    /// Answer to `ListDevices`.
    Devices {
        /// Monitors.
        monitors: Vec<MonitorInfo>,
        /// GPUs.
        gpus: Vec<GpuInfo>,
    },
    /// Progress of `Bench`.
    BenchProgress {
        /// Cases done.
        done: u32,
        /// Cases in total.
        total: u32,
    },
    /// Result of `Bench` (also after `CancelBench`, with what was measured).
    BenchResult(BenchReport),
    /// Answer to `Diagnostics`.
    Diagnostics(DiagReport),
    /// A log line the GUI may show (warnings and errors of the scaler).
    Log {
        /// Level.
        level: Level,
        /// Text.
        message: String,
    },
}
