//! When `lirtup serve` exits (round 4, front Y).
//!
//! - Started by the GUI (`CoreLink` launches it): it exits once the last
//!   client leaves, unless the daemon is on with «Seguir ejecutándose tras
//!   salir» (`keep_after_exit`) or a session is running that the person did
//!   not stop.
//! - Started by hand or with `--daemon`: it runs until `Quit`, SIGTERM or
//!   SIGINT.
//! - `Restart` with the lsfg-vk layer `exec`s the scaler; [`Resume`] carries
//!   the session across it so the new process starts it again.

use std::time::{Duration, Instant};

use lirtup_proto::SessionKind;

/// How long `serve` waits after the last client leaves before exiting
/// (a GUI that restarts, `lirtup ctl` between commands).
pub const LINGER: Duration = Duration::from_secs(5);

/// The policy of [`self`](crate::lifecycle), fed by the server and the
/// engine.
// Front Y: remove the allowance when the methods read the fields.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Lifecycle {
    /// Started by the GUI (`--exit-with-clients`) rather than by hand.
    pub with_clients: bool,
    clients: usize,
    left_at: Option<Instant>,
}

impl Lifecycle {
    /// A new policy.
    pub fn new(with_clients: bool) -> Lifecycle {
        Lifecycle {
            with_clients,
            clients: 0,
            left_at: None,
        }
    }

    /// A client connected.
    pub fn connected(&mut self) {
        todo!()
    }

    /// A client left at `now`.
    pub fn left(&mut self, now: Instant) {
        let _ = now;
        todo!()
    }

    /// Whether to exit now: `daemon_keeps` = daemon on with
    /// `keep_after_exit`; `session` = a session is running.
    pub fn should_exit(&self, now: Instant, daemon_keeps: bool, session: bool) -> bool {
        let _ = (now, daemon_keeps, session);
        todo!()
    }
}

/// A session to start again after `exec` (`Restart` with the layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resume {
    /// The window.
    pub xid: u32,
    /// Fullscreen or windowed.
    pub kind: SessionKind,
    /// The profile the `Start` named.
    pub profile: Option<String>,
}

/// The variable that carries a [`Resume`] across `exec`.
pub const RESUME_ENV: &str = "LIRTUP_RESUME";

impl Resume {
    /// The value of [`RESUME_ENV`].
    pub fn encode(&self) -> String {
        todo!()
    }

    /// Read and remove [`RESUME_ENV`] (at startup, before any thread).
    pub fn take_from_env() -> Option<Resume> {
        todo!()
    }
}
