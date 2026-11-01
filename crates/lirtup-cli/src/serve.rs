//! `lirtup serve`: the scaler and daemon for lirtup-gui, behind its socket.
//! Its lifecycle (when it exits, the session carried across a `Restart`
//! that `exec`s) is round 4, front Y: `lirtup_core::lifecycle`.
//!
//! Also `lirtup run -d` (1.x `upscale -d`): the daemon in the foreground,
//! without a socket, until SIGINT or SIGTERM.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use lirtup_core::broadcast::error::RecvError;
use lirtup_core::lifecycle::Lifecycle;
use lirtup_core::proto::Event;
use lirtup_core::{Engine, EngineOptions};
use lirtup_proto::socket_path;

/// How long the foreground daemon waits for the session on the way out.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Run until `Quit`, SIGTERM or SIGINT; with `exit_with_clients` (the GUI
/// launched it), also once the last client has gone and neither the daemon
/// with «Seguir ejecutándose tras salir» nor a session keeps it.
pub fn serve(
    socket: Option<PathBuf>,
    daemon: bool,
    config: Option<PathBuf>,
    exit_with_clients: bool,
) -> anyhow::Result<ExitCode> {
    let socket = socket.unwrap_or_else(socket_path);
    lirtup_core::server::ensure_not_running(&socket)?;
    // The engine first (it runs `layer::prepare` and takes the session a
    // `Restart` carried), the runtime after.
    let engine = Arc::new(Engine::new(EngineOptions {
        daemon,
        config_path: config,
        ..EngineOptions::default()
    })?);
    lirtup_core::server::run(engine, &socket, Lifecycle::new(exit_with_clients))?;
    Ok(ExitCode::SUCCESS)
}

/// `lirtup run -d`: the engine was built with the daemon on (`-d` is the
/// `daemon` key of the CLI layer); log what it does until a signal.
pub fn daemon_foreground(engine: Arc<Engine>) -> anyhow::Result<ExitCode> {
    eprintln!(
        "lirtup: daemon mode; watching {} profiles (Ctrl+C to stop)",
        engine.snapshot().daemon.watching
    );
    let mut events = engine.subscribe();
    {
        let engine = Arc::clone(&engine);
        lirtup_core::signals::on_terminate(move || {
            engine.shutdown(STOP_TIMEOUT);
            std::process::exit(130);
        })?;
    }
    loop {
        match events.blocking_recv() {
            Ok(Event::DaemonMatch { window, profile }) => {
                tracing::info!("daemon: {:?} matches {profile:?}", window.title);
            }
            Ok(Event::SessionStarted(view)) => tracing::info!(
                "scaling {:?} {}x{} → {}x{}",
                view.window.title,
                view.src.width,
                view.src.height,
                view.dst.width,
                view.dst.height
            ),
            Ok(Event::SessionStopped { reason }) => tracing::info!("stopped ({reason:?}); watching"),
            Ok(_) | Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
    Ok(ExitCode::SUCCESS)
}
