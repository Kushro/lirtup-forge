//! When `lirtup serve` exits (round 4, front Y).
//!
//! - Started by the GUI (`--exit-with-clients`): it exits once the last
//!   client has been gone for [`LINGER`], unless the daemon is on with
//!   «Seguir ejecutándose tras salir» (`keep_after_exit`) or a session is
//!   running (the GUI stops it on «Salir»; one that outlives a crashed GUI
//!   keeps going until it ends). With no client at all after startup it
//!   waits [`LINGER`] too.
//! - Started by hand or with `--daemon`: it runs until `Quit`, SIGTERM or
//!   SIGINT.
//! - `Restart` with the lsfg-vk layer `exec`s the scaler; [`Resume`] carries
//!   the session across it so the new process starts it again.
//!
//! Also the program `lirtup run -- prog` launches ([`launch`]): with X11
//! forced on it, and terminated when the run ends (1.x `env.py`,
//! `cli.py`).

use std::process::{Child, Command};
use std::time::{Duration, Instant};

use lirtup_proto::SessionKind;

/// How long `serve` waits after the last client leaves before exiting
/// (a GUI that restarts, `lirtup ctl` between commands).
pub const LINGER: Duration = Duration::from_secs(5);

/// The policy of [`self`](crate::lifecycle), fed by the server and the
/// engine.
#[derive(Debug, Clone)]
pub struct Lifecycle {
    /// Started by the GUI (`--exit-with-clients`) rather than by hand.
    pub with_clients: bool,
    clients: usize,
    /// When the client count last went to 0 (or the start).
    left_at: Option<Instant>,
}

impl Lifecycle {
    /// A new policy; the startup counts as a client that just left.
    pub fn new(with_clients: bool) -> Lifecycle {
        Lifecycle {
            with_clients,
            clients: 0,
            left_at: Some(Instant::now()),
        }
    }

    /// A client connected.
    pub fn connected(&mut self) {
        self.clients += 1;
        self.left_at = None;
    }

    /// A client left at `now`.
    pub fn left(&mut self, now: Instant) {
        self.clients = self.clients.saturating_sub(1);
        if self.clients == 0 {
            self.left_at = Some(now);
        }
    }

    /// Clients connected now.
    pub fn clients(&self) -> usize {
        self.clients
    }

    /// Whether to exit now: `daemon_keeps` = daemon on with
    /// `keep_after_exit`; `session` = a session is running.
    pub fn should_exit(&self, now: Instant, daemon_keeps: bool, session: bool) -> bool {
        if !self.with_clients || self.clients > 0 || daemon_keeps || session {
            return false;
        }
        self.left_at
            .is_some_and(|at| now.saturating_duration_since(at) >= LINGER)
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
    /// The value of [`RESUME_ENV`]: `<xid>;<kind>` or
    /// `<xid>;<kind>;<profile>` (the profile last, so it may hold `;`).
    pub fn encode(&self) -> String {
        let kind = match self.kind {
            SessionKind::Fullscreen => "fullscreen",
            SessionKind::Windowed => "windowed",
        };
        match &self.profile {
            Some(profile) => format!("{};{kind};{profile}", self.xid),
            None => format!("{};{kind}", self.xid),
        }
    }

    /// Parse [`Resume::encode`].
    pub fn decode(text: &str) -> Option<Resume> {
        let mut parts = text.splitn(3, ';');
        let xid = parts.next()?.parse().ok()?;
        let kind = match parts.next()? {
            "fullscreen" => SessionKind::Fullscreen,
            "windowed" => SessionKind::Windowed,
            _ => return None,
        };
        let profile = parts.next().map(str::to_owned);
        Some(Resume { xid, kind, profile })
    }

    /// Read and remove [`RESUME_ENV`] (at startup, before any thread).
    #[allow(unsafe_code)]
    pub fn take_from_env() -> Option<Resume> {
        let value = std::env::var_os(RESUME_ENV)?;
        // SAFETY: called at startup, before any other thread exists (its
        // documented contract), so nothing reads the environment
        // concurrently.
        unsafe { std::env::remove_var(RESUME_ENV) };
        let resume = value.to_str().and_then(Resume::decode);
        if resume.is_none() {
            tracing::warn!("{RESUME_ENV}: cannot read {value:?}; not resuming");
        }
        resume
    }
}

/// Variables that force X11 on a launched program (1.x `env.py`):
/// toolkits and the Mesa Vulkan drivers go through XWayland, where the
/// scaler can capture them. `__GLX_VENDOR_LIBRARY_NAME=nvidia` of 1.x is
/// left out on purpose: it breaks GLX with AMD and Intel.
pub const FORCE_X11_ENV: [(&str, &str); 3] = [
    ("XDG_SESSION_TYPE", "x11"),
    ("MESA_VK_WSI", "x11"),
    ("RADV_DEBUG", "no_wayland_wsi"),
];

/// A program launched by `lirtup run -- prog`: terminated (SIGTERM, then
/// reaped) when dropped, as 1.x does when the run ends or the window never
/// appears.
#[derive(Debug)]
pub struct Launched {
    child: Child,
}

/// Start `program` with `args` and X11 forced ([`FORCE_X11_ENV`], no
/// `WAYLAND_DISPLAY`).
pub fn launch(program: &str, args: &[String]) -> std::io::Result<Launched> {
    let mut command = Command::new(program);
    command
        .args(args)
        .envs(FORCE_X11_ENV)
        .env_remove("WAYLAND_DISPLAY");
    let child = command.spawn()?;
    Ok(Launched { child })
}

impl Launched {
    /// Its PID.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// The executable's name, the class hint of the window search (1.x:
    /// the launched program's basename, `acquisition.py`).
    pub fn class_hint(program: &str) -> Option<String> {
        std::path::Path::new(program)
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }
}

impl Drop for Launched {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return;
        }
        tracing::debug!("terminating the launched program (pid {})", self.child.id());
        if let Ok(pid) = libc::pid_t::try_from(self.child.id()) {
            // SAFETY: `kill` takes plain integers; the pid is our own child,
            // not reaped yet (`try_wait` above found it running).
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exits_only_with_clients_gone() {
        let t0 = Instant::now();
        let mut by_hand = Lifecycle::new(false);
        by_hand.connected();
        by_hand.left(t0);
        assert!(!by_hand.should_exit(t0 + LINGER * 2, false, false));

        let mut gui = Lifecycle::new(true);
        gui.connected();
        assert!(!gui.should_exit(t0 + LINGER * 2, false, false));
        gui.connected();
        gui.left(t0);
        assert_eq!(gui.clients(), 1);
        assert!(!gui.should_exit(t0 + LINGER * 2, false, false));
        gui.left(t0);
        assert!(!gui.should_exit(t0 + LINGER / 2, false, false));
        assert!(gui.should_exit(t0 + LINGER, false, false));
        // The daemon with «Seguir ejecutándose», or a session, keep it.
        assert!(!gui.should_exit(t0 + LINGER, true, false));
        assert!(!gui.should_exit(t0 + LINGER, false, true));
        // A client that comes back cancels the countdown.
        gui.connected();
        assert!(!gui.should_exit(t0 + LINGER * 3, false, false));
    }

    #[test]
    fn nobody_connects_after_startup() {
        let gui = Lifecycle::new(true);
        assert!(!gui.should_exit(Instant::now(), false, false));
        assert!(gui.should_exit(Instant::now() + LINGER, false, false));
    }

    #[test]
    fn resume_round_trips() {
        for resume in [
            Resume {
                xid: 0x0420_0007,
                kind: SessionKind::Fullscreen,
                profile: None,
            },
            Resume {
                xid: 1,
                kind: SessionKind::Windowed,
                profile: Some("Utawarerumono; Mask of Truth".into()),
            },
        ] {
            assert_eq!(Resume::decode(&resume.encode()), Some(resume));
        }
        assert_eq!(Resume::decode(""), None);
        assert_eq!(Resume::decode("12;sideways"), None);
        assert_eq!(Resume::decode("x;fullscreen"), None);
    }

    #[test]
    fn class_hint_is_the_basename() {
        assert_eq!(Launched::class_hint("/usr/bin/vkcube").as_deref(), Some("vkcube"));
        assert_eq!(Launched::class_hint("game.exe").as_deref(), Some("game"));
        assert_eq!(Launched::class_hint(""), None);
    }

    #[test]
    fn launched_program_is_terminated() {
        let launched = launch("sleep", &["30".into()]).unwrap();
        let pid = launched.id();
        drop(launched);
        // Reaped: no such process any more.
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}
