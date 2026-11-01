//! [`CoreLink`]: the GUI's client of the `lirtup serve` socket (6.5 of the
//! analysis).
//!
//! One task owns the connection. It connects, sends `Hello`, turns every
//! line into [`Action::Core`] and writes the commands it is given. When
//! there is no socket it reports [`LinkState::Down`] and keeps the commands
//! that arrive; the first one starts `lirtup serve` (the binary next to
//! this one, else `lirtup` in `PATH`) and they are sent once it answers.
//! A dropped connection (the scaler `exec`s itself on `Restart` with the
//! lsfg-vk layer, or crashed) reconnects by itself.
//!
//! The link never touches the store: it hands actions to the dispatcher
//! `app.rs` gives it, so `update()` stays the only writer.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;

use lirtup_proto::{Command, Event, PROTOCOL_VERSION, decode_line, encode_line, socket_path};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::state::{Action, LinkState};

/// Commands kept while the socket is down (the oldest go first).
const QUEUE: usize = 32;
/// Wait between connection attempts.
const RETRY: Duration = Duration::from_secs(2);
/// How long a launched `lirtup serve` has to answer.
const LAUNCH_WAIT: Duration = Duration::from_secs(5);

/// Sends commands to `lirtup serve`. Cheap to clone; provided as a context
/// by `app.rs` (`use_context::<CoreLink>()`).
#[derive(Clone)]
pub struct CoreLink {
    tx: UnboundedSender<Command>,
}

impl PartialEq for CoreLink {
    fn eq(&self, other: &Self) -> bool {
        self.tx.same_channel(&other.tx)
    }
}

/// The receiving half of a [`CoreLink`], for [`spawn`].
pub struct LinkRx(UnboundedReceiver<Command>);

impl CoreLink {
    /// A link and the receiver its task reads. Split so the dispatcher
    /// (which sends the effects' commands) can hold the link before the
    /// task starts.
    pub fn new() -> (CoreLink, LinkRx) {
        let (tx, rx) = unbounded_channel();
        (CoreLink { tx }, LinkRx(rx))
    }

    /// Queue a command. Answers arrive as events.
    pub fn send(&self, command: Command) {
        // The task lives as long as the app; a closed channel means the
        // app is quitting.
        let _ = self.tx.send(command);
    }
}

/// Start the link task on the current runtime (Dioxus desktop runs on
/// tokio; call it inside the app). `dispatch` gets every [`Action::Core`]
/// and [`Action::Link`].
pub fn spawn(rx: LinkRx, dispatch: impl FnMut(Action) + 'static) {
    dioxus::prelude::spawn(run(socket_path(), rx.0, dispatch));
}

/// The client name sent in `Hello`.
fn client_name() -> String {
    format!("lirtup-gui {}", env!("CARGO_PKG_VERSION"))
}

async fn run(socket: PathBuf, mut rx: UnboundedReceiver<Command>, mut dispatch: impl FnMut(Action)) {
    let mut queue: VecDeque<Command> = VecDeque::new();
    let mut launched = false;
    loop {
        dispatch(Action::Link(LinkState::Connecting));
        match UnixStream::connect(&socket).await {
            Ok(stream) => {
                launched = false;
                let reason = session(stream, &mut rx, &mut queue, &mut dispatch).await;
                match reason {
                    End::AppClosed => return,
                    End::Dropped(reason) => {
                        tracing::info!("lirtup serve: {reason}");
                        dispatch(Action::Link(LinkState::Down { reason }));
                    }
                }
            }
            Err(e) => {
                dispatch(Action::Link(LinkState::Down {
                    reason: format!("{}: {e}", socket.display()),
                }));
                // Wait for a command (then launch the scaler) or retry.
                tokio::select! {
                    command = rx.recv() => {
                        let Some(command) = command else { return };
                        if queue.len() == QUEUE {
                            queue.pop_front();
                        }
                        queue.push_back(command);
                        if !launched {
                            launched = true;
                            launch_server();
                            wait_for_socket(&socket, LAUNCH_WAIT).await;
                        }
                    }
                    () = tokio::time::sleep(RETRY) => {}
                }
            }
        }
    }
}

enum End {
    /// The command channel closed: the GUI is quitting.
    AppClosed,
    /// The server went away.
    Dropped(String),
}

async fn session(
    stream: UnixStream,
    rx: &mut UnboundedReceiver<Command>,
    queue: &mut VecDeque<Command>,
    dispatch: &mut impl FnMut(Action),
) -> End {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let hello = Command::Hello {
        version: PROTOCOL_VERSION,
        client: client_name(),
    };
    if let Err(e) = write.write_all(encode_line(&hello).as_bytes()).await {
        return End::Dropped(e.to_string());
    }
    while let Some(command) = queue.pop_front() {
        if let Err(e) = write.write_all(encode_line(&command).as_bytes()).await {
            return End::Dropped(e.to_string());
        }
    }
    loop {
        tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(line)) if line.trim().is_empty() => {}
                Ok(Some(line)) => match decode_line::<Event>(&line) {
                    Ok(event) => {
                        if let Event::Snapshot(snapshot) = &event {
                            dispatch(Action::Link(LinkState::Connected {
                                protocol: snapshot.protocol,
                                version: snapshot.version.clone(),
                            }));
                        }
                        dispatch(Action::Core(event));
                    }
                    Err(e) => tracing::warn!("lirtup serve sent a bad line: {e}"),
                },
                Ok(None) => return End::Dropped("the scaler closed the connection".into()),
                Err(e) => return End::Dropped(e.to_string()),
            },
            command = rx.recv() => {
                let Some(command) = command else { return End::AppClosed };
                if let Err(e) = write.write_all(encode_line(&command).as_bytes()).await {
                    // Keep it for the next connection.
                    queue.push_back(command);
                    return End::Dropped(e.to_string());
                }
            }
        }
    }
}

/// Start `lirtup serve` detached (it outlives the GUI in daemon mode).
fn launch_server() {
    use std::os::unix::process::CommandExt;

    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("lirtup")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("lirtup"));
    tracing::info!("starting {} serve", exe.display());
    // `--exit-with-clients`: it exits after the GUI unless the daemon keeps
    // running after exit (round 4, front Y). Its own process group, so a
    // signal to the GUI's (Ctrl+C in a terminal) does not reach it.
    match std::process::Command::new(&exe)
        .arg("serve")
        .arg("--exit-with-clients")
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
    {
        // Not waited for: it is reaped when the GUI exits (or by init).
        Ok(child) => drop(child),
        Err(e) => tracing::error!("cannot start {}: {e}", exe.display()),
    }
}

async fn wait_for_socket(socket: &std::path::Path, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if UnixStream::connect(socket).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    fn temp_socket(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lirtup-ui-link-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("s.sock");
        let _ = std::fs::remove_file(&path);
        path
    }

    #[tokio::test]
    async fn hello_snapshot_and_commands() {
        let socket = temp_socket("hello");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            let hello: Command = decode_line(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert!(matches!(
                hello,
                Command::Hello {
                    version: PROTOCOL_VERSION,
                    ..
                }
            ));
            let snapshot = Event::Snapshot(lirtup_proto::EngineSnapshot {
                protocol: PROTOCOL_VERSION,
                version: "2.0.0".into(),
                ..Default::default()
            });
            write.write_all(encode_line(&snapshot).as_bytes()).await.unwrap();
            let stop: Command = decode_line(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(stop, Command::Stop);
        });

        let (tx, rx) = unbounded_channel();
        let (atx, mut arx) = unbounded_channel();
        let task = tokio::spawn(run(socket.clone(), rx, move |a| {
            let _ = atx.send(a);
        }));
        let mut seen = Vec::new();
        while let Some(action) = arx.recv().await {
            let snapshot = matches!(action, Action::Core(Event::Snapshot(_)));
            seen.push(action);
            if snapshot {
                break;
            }
        }
        assert!(seen.contains(&Action::Link(LinkState::Connected {
            protocol: PROTOCOL_VERSION,
            version: "2.0.0".into()
        })));
        tx.send(Command::Stop).unwrap();
        server.await.unwrap();
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
    }
}
