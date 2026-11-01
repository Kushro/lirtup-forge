//! `lirtup serve`: the scaler and daemon for lirtup-gui, behind its socket.
//! Its lifecycle (when it exits, the session carried across a `Restart`
//! that `exec`s) is round 4, front Y: `lirtup_core::lifecycle`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use lirtup_core::{Engine, EngineOptions};
use lirtup_proto::socket_path;

/// Run until `Quit`, SIGTERM or SIGINT.
pub fn serve(socket: Option<PathBuf>, daemon: bool, config: Option<PathBuf>) -> anyhow::Result<ExitCode> {
    let socket = socket.unwrap_or_else(socket_path);
    lirtup_core::server::ensure_not_running(&socket)?;
    // The engine first (it runs `layer::prepare`), the runtime after.
    let engine = Arc::new(Engine::new(EngineOptions {
        daemon,
        config_path: config,
        ..EngineOptions::default()
    })?);
    lirtup_core::server::run(engine, &socket)?;
    Ok(ExitCode::SUCCESS)
}
