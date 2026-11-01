//! The Unix socket server of `lirtup serve` (6.5).
//!
//! One task per client. A client sends `Hello` and gets a `Snapshot`, then
//! every event; commands before `Hello` are handled too, but events only
//! flow after it. A `Hello` of another protocol version is still answered
//! with a `Snapshot` (its `protocol` field tells the GUI which version runs),
//! then the connection is closed. A line that does not decode is answered
//! with an `Event::Log` to that client only. A client that falls behind the
//! broadcast gets a fresh `Snapshot` instead of the events it missed.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

use crate::proto::{CodecError, Command, Event, Level, PROTOCOL_VERSION, decode_line, encode_line};
use crate::{CoreError, Engine, Result};

/// How long `serve` waits for the session to stop on the way out.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Listen on `socket` (creating its folder with mode 0700 and removing a
/// stale socket), serve clients until `Quit` or a signal, then shut the
/// engine down. One task per client: read lines → [`Engine::handle`], and
/// forward [`Engine::subscribe`] to the client.
pub async fn serve(engine: Arc<Engine>, socket: &Path) -> Result<()> {
    let listener = bind(socket)?;
    tracing::info!("listening on {}", socket.display());

    use tokio::signal::unix::{SignalKind, signal};
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut quit = engine.quit_signal();

    let result = loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let engine = Arc::clone(&engine);
                    tokio::spawn(async move {
                        if let Err(e) = client(engine, stream).await {
                            tracing::debug!("client: {e}");
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("accept: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
            _ = sigint.recv() => { tracing::info!("SIGINT: stopping"); break Ok(()); }
            _ = sigterm.recv() => { tracing::info!("SIGTERM: stopping"); break Ok(()); }
            changed = quit.wait_for(|q| *q) => {
                if changed.is_ok() { tracing::info!("Quit: stopping"); }
                break Ok(());
            }
        }
    };

    drop(listener);
    let _ = std::fs::remove_file(socket);
    let engine_for_shutdown = Arc::clone(&engine);
    tokio::task::spawn_blocking(move || engine_for_shutdown.shutdown(SHUTDOWN_TIMEOUT))
        .await
        .ok();
    result
}

/// [`serve`] on a new multi-threaded tokio runtime, for `lirtup serve`
/// (which builds the [`Engine`] first, before any thread exists).
pub fn run(engine: Arc<Engine>, socket: &Path) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("lirtup-serve")
        .build()?;
    let result = runtime.block_on(serve(engine, socket));
    // Client tasks may still wait on their sockets.
    runtime.shutdown_timeout(Duration::from_millis(500));
    result
}

/// Create the folder (0700), refuse a live socket, remove a stale one, bind.
fn bind(socket: &Path) -> Result<UnixListener> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    if let Some(dir) = socket.parent().filter(|d| !d.as_os_str().is_empty()) {
        if !dir.exists() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
        }
    }
    if socket.exists() {
        match std::os::unix::net::UnixStream::connect(socket) {
            Ok(_) => return Err(CoreError::AlreadyRunning(socket.to_path_buf())),
            Err(_) => {
                tracing::debug!("removing stale socket {}", socket.display());
                std::fs::remove_file(socket)?;
            }
        }
    }
    let listener = UnixListener::bind(socket)?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// What the reader half of a client decided about one line.
enum Line {
    Command(Command),
    /// A `Hello` of another protocol version.
    OldHello(u32),
    Bad(String),
}

fn parse(line: &str) -> Line {
    match decode_line::<Command>(line) {
        Ok(command) => Line::Command(command),
        Err(CodecError::Version { got, .. }) => {
            let hello = serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|v| v.get("type").and_then(|t| t.as_str()) == Some("Hello"));
            if hello {
                Line::OldHello(got)
            } else {
                Line::Bad(format!(
                    "protocol version {got}, this lirtup serve speaks {PROTOCOL_VERSION}"
                ))
            }
        }
        Err(e) => Line::Bad(e.to_string()),
    }
}

async fn write_event(writer: &mut (impl AsyncWriteExt + Unpin), event: &Event) -> std::io::Result<()> {
    writer.write_all(encode_line(event).as_bytes()).await
}

/// One client, until it disconnects or the server stops.
async fn client(engine: Arc<Engine>, stream: UnixStream) -> Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    // `None` until `Hello`.
    let mut events: Option<broadcast::Receiver<Event>> = None;

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { return Ok(()) };
                if line.trim().is_empty() {
                    continue;
                }
                match parse(&line) {
                    Line::Command(Command::Hello { version, client }) => {
                        tracing::info!("client {client:?} (protocol {version})");
                        // Subscribe first: an event may come twice, never
                        // be lost.
                        events = Some(engine.subscribe());
                        write_event(&mut write, &Event::Snapshot(engine.snapshot())).await?;
                    }
                    Line::Command(command) => {
                        let engine = Arc::clone(&engine);
                        let name = format!("{command:?}");
                        // Some commands wait for the session thread.
                        let handled = tokio::task::spawn_blocking(move || engine.handle(command)).await;
                        let error = match handled {
                            Ok(Ok(())) => None,
                            Ok(Err(e)) => Some(e.to_string()),
                            Err(e) => Some(format!("the command panicked: {e}")),
                        };
                        if let Some(message) = error {
                            tracing::warn!("{name}: {message}");
                            write_event(&mut write, &Event::Log { level: Level::Error, message }).await?;
                        }
                    }
                    Line::OldHello(version) => {
                        tracing::warn!("a client speaks protocol {version}; closing it");
                        write_event(&mut write, &Event::Snapshot(engine.snapshot())).await?;
                        write.shutdown().await.ok();
                        return Ok(());
                    }
                    Line::Bad(message) => {
                        let message = format!("bad message: {message}");
                        write_event(&mut write, &Event::Log { level: Level::Warn, message }).await?;
                    }
                }
            }
            event = recv(&mut events) => match event {
                Ok(event) => write_event(&mut write, &event).await?,
                Err(RecvError::Lagged(n)) => {
                    tracing::debug!("client lagged by {n} events; sending a snapshot");
                    write_event(&mut write, &Event::Snapshot(engine.snapshot())).await?;
                }
                Err(RecvError::Closed) => return Ok(()),
            },
        }
    }
}

/// The next event, or never before `Hello`.
async fn recv(events: &mut Option<broadcast::Receiver<Event>>) -> Result<Event, RecvError> {
    match events {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}
