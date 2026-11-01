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
//! | `ListEffects`, `CompileEffect` | `lirtup_magpiefx::Catalog` and `compile_effect` on a blocking task (cache in `paths::magpiefx_cache_dir`) |
//! | `SetDaemon` | state only in round 2 (the daemon is round 4) |
//! | `Diagnostics` | gather `DiagReport` |
//! | `Quit` | shutdown |

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::broadcast;

use crate::Result;
use crate::proto::{Command, EngineSnapshot, Event};

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
}

/// The engine. `Send + Sync`; shared as `Arc<Engine>` between the server
/// tasks.
pub struct Engine {
    _private: (),
}

impl Engine {
    /// Read the configuration, prepare frame generation (before any Vulkan
    /// call), start the hotkey thread. Does not open the GPU until a session
    /// starts.
    pub fn new(options: EngineOptions) -> Result<Engine> {
        todo!("front K")
    }

    /// Handle one command. Answers and progress arrive as events.
    pub fn handle(&self, command: Command) -> Result<()> {
        todo!("front K")
    }

    /// Subscribe to events. A lagging receiver gets `RecvError::Lagged` and
    /// should ask for a [`Engine::snapshot`].
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        todo!("front K")
    }

    /// The whole state now.
    pub fn snapshot(&self) -> EngineSnapshot {
        todo!("front K")
    }

    /// Stop the session and the threads, waiting at most `timeout`.
    pub fn shutdown(&self, timeout: Duration) {
        todo!("front K")
    }
}
