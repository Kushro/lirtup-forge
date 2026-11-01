//! The engine (`api/` of 1.x: `UpscalerSession`).

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
