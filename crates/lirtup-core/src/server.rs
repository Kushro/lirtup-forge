//! The Unix socket server of `lirtup serve` (6.5).
//!
//! One task per client. A client sends `Hello` and gets a `Snapshot`, then
//! every event; commands before `Hello` are handled too, but events only
//! flow after it. A `Hello` of another protocol version is still answered
//! with a `Snapshot` (its `protocol` field tells the GUI which version runs),
//! then the connection is closed. A line that does not decode is answered
//! with an `Event::Log` to that client only. A client that falls behind the
//! broadcast gets a fresh `Snapshot` instead of the events it missed.
//!
//! When to stop is [`Lifecycle`]'s call (round 4, front Y): the server
//! counts the connections and asks it every second. With no client
//! connected (the GUI closed, the daemon scaling on its own), a session
//! that starts or fails is announced with a desktop notification
//! (`notify-send`, «Detener» stops the session).

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

use crate::lifecycle::Lifecycle;
use crate::proto::{CodecError, Command, Event, Level, PROTOCOL_VERSION, decode_line, encode_line};
use crate::{CoreError, Engine, Result};

/// How long `serve` waits for the session to stop on the way out.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// How often the server asks [`Lifecycle`] whether to stop.
const LIFECYCLE_TICK: Duration = Duration::from_secs(1);

fn lock(m: &Mutex<Lifecycle>) -> std::sync::MutexGuard<'_, Lifecycle> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Listen on `socket` (creating its folder with mode 0700 and removing a
/// stale socket), serve clients until `Quit`, a signal or `lifecycle`
/// says so, then shut the engine down. One task per client: read lines →
/// [`Engine::handle`], and forward [`Engine::subscribe`] to the client.
pub async fn serve(engine: Arc<Engine>, socket: &Path, lifecycle: Lifecycle) -> Result<()> {
    let listener = bind(socket)?;
    tracing::info!("listening on {}", socket.display());

    use tokio::signal::unix::{SignalKind, signal};
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut quit = engine.quit_signal();
    let lifecycle = Arc::new(Mutex::new(lifecycle));
    let mut tick = tokio::time::interval(LIFECYCLE_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut events = engine.subscribe();

    let result = loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let engine = Arc::clone(&engine);
                    let lifecycle = Arc::clone(&lifecycle);
                    lock(&lifecycle).connected();
                    tokio::spawn(async move {
                        if let Err(e) = client(engine, stream).await {
                            tracing::debug!("client: {e}");
                        }
                        lock(&lifecycle).left(Instant::now());
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
            _ = tick.tick() => {
                let exit = lock(&lifecycle).should_exit(
                    Instant::now(),
                    engine.daemon_keeps(),
                    engine.session_running(),
                );
                if exit {
                    tracing::info!("no client left and nothing to keep running: stopping");
                    break Ok(());
                }
            }
            event = events.recv() => match event {
                Ok(event) => {
                    if lock(&lifecycle).clients() == 0 {
                        notify_headless(&engine, &event);
                    }
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break Ok(()),
            },
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
pub fn run(engine: Arc<Engine>, socket: &Path, lifecycle: Lifecycle) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("lirtup-serve")
        .build()?;
    let result = runtime.block_on(serve(engine, socket, lifecycle));
    // Client tasks may still wait on their sockets.
    runtime.shutdown_timeout(Duration::from_millis(500));
    result
}

/// [`CoreError::AlreadyRunning`] if a server answers on `socket`. `lirtup
/// serve` checks it before building the [`Engine`] (which grabs the hotkeys
/// and writes the lsfg-vk configuration).
pub fn ensure_not_running(socket: &Path) -> Result<()> {
    if socket.exists() && std::os::unix::net::UnixStream::connect(socket).is_ok() {
        return Err(CoreError::AlreadyRunning(socket.to_path_buf()));
    }
    Ok(())
}

/// A desktop notification for `event` while no client is connected: the
/// daemon started a session (with «Detener»), or one failed. The GUI shows
/// its own when it is connected (`lirtup-ui/src/notify.rs`).
fn notify_headless(engine: &Arc<Engine>, event: &Event) {
    let spanish = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|v| !v.is_empty()))
        .is_some_and(|l| l.starts_with("es"));
    let (body, stop) = match event {
        Event::SessionStarted(view) if spanish => (
            format!(
                "Escalando: «{}». Utilice «Detener» en este aviso para volver.",
                view.window.title
            ),
            true,
        ),
        Event::SessionStarted(view) => (
            format!(
                "Scaling «{}». Use «Stop» in this notification to go back.",
                view.window.title
            ),
            true,
        ),
        Event::SessionError { message, .. } if spanish => (format!("No se pudo escalar: {message}"), false),
        Event::SessionError { message, .. } => (format!("Scaling failed: {message}"), false),
        _ => return,
    };
    let engine = Arc::clone(engine);
    let label = if spanish { "Detener" } else { "Stop" };
    let spawned = std::thread::Builder::new()
        .name("lirtup-notify".into())
        .spawn(move || {
            let mut command = std::process::Command::new("notify-send");
            command.arg("--app-name=LirtUp Forge").arg("--icon=lirtup-forge");
            if stop {
                command.arg(format!("--action=stop={label}"));
            }
            command.arg("LirtUp Forge").arg(body);
            match command.stdin(std::process::Stdio::null()).output() {
                Ok(out) if String::from_utf8_lossy(&out.stdout).trim() == "stop" => {
                    tracing::info!("notification: Detener");
                    if let Err(e) = engine.handle(Command::Stop) {
                        tracing::warn!("Stop: {e}");
                    }
                }
                Ok(_) => {}
                Err(e) => tracing::debug!("notify-send: {e}"),
            }
        });
    if let Err(e) = spawned {
        tracing::debug!("notification: {e}");
    }
}

/// Create the folder (0700), refuse a live socket, remove a stale one, bind.
fn bind(socket: &Path) -> Result<UnixListener> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    if let Some(dir) = socket.parent().filter(|d| !d.as_os_str().is_empty())
        && !dir.exists()
    {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    ensure_not_running(socket)?;
    if socket.exists() {
        tracing::debug!("removing stale socket {}", socket.display());
        std::fs::remove_file(socket)?;
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
    let mut gpu_watch = Counted {
        engine: Arc::clone(&engine),
        on: false,
        apply: Engine::watch_gpu_times,
    };
    let mut hotkey_pause = Counted {
        engine: Arc::clone(&engine),
        on: false,
        apply: Engine::pause_hotkeys,
    };

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
                    Line::Command(Command::WatchGpuTimes { on }) => gpu_watch.set(on),
                    Line::Command(Command::PauseHotkeys { on }) => hotkey_pause.set(on),
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

/// Whether one connection counts for a per-connection request
/// (`WatchGpuTimes`, `PauseHotkeys`); a closed connection stops counting.
struct Counted {
    engine: Arc<Engine>,
    on: bool,
    apply: fn(&Engine, bool),
}

impl Counted {
    fn set(&mut self, on: bool) {
        if on != self.on {
            self.on = on;
            (self.apply)(&self.engine, on);
        }
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.set(false);
    }
}

/// The next event, or never before `Hello`.
async fn recv(events: &mut Option<broadcast::Receiver<Event>>) -> Result<Event, RecvError> {
    match events {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}
