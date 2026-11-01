//! Daemon mode (round 4, front Y): watch the windows and start a session on
//! the first one that matches a profile with match rules
//! (`pipeline/daemon.py`, `gui/helpers/daemon.py` of 1.x).
//!
//! Rules, as 1.x:
//! - every `daemon_poll_interval` seconds, list the windows (`X11::list_windows`)
//!   and, with no session running, take the first window that matches a
//!   profile whose rules are not empty and that is not `daemon_exclude`;
//!   wait `target_delay` before starting (the game's own setup);
//! - a session the daemon started that ends because its window closed sends
//!   the daemon back to watching; one the person stopped does not start the
//!   same window again until it closes;
//! - LirtUp Forge's own windows never match (`X11::is_lirtup_window`).
//!
//! The engine owns the daemon thread and reaches it through [`DaemonHost`];
//! `SetDaemon` and `--daemon` turn it on and off. With `keep_after_exit`,
//! `lirtup serve` keeps running when the GUI quits (`crate::lifecycle`).

use std::sync::Arc;
use std::time::Duration;

use lirtup_config::{ConfigFile, Profile};
use lirtup_x11::WindowInfo;

/// What the daemon needs of the engine.
pub trait DaemonHost: Send + Sync + 'static {
    /// A session is running (started by anyone).
    fn session_running(&self) -> bool;
    /// The configuration file as the engine has it now (after
    /// `ReloadConfig`).
    fn config_file(&self) -> ConfigFile;
    /// Start a session on `window` with `profile` (it sends `DaemonMatch`
    /// and the session events).
    fn start_matched(&self, window: WindowInfo, profile: String);
}

/// The daemon thread.
pub struct Daemon {
    _host: Arc<dyn DaemonHost>,
}

impl Daemon {
    /// Start watching every `poll`.
    pub fn spawn(host: Arc<dyn DaemonHost>, poll: Duration) -> crate::Result<Daemon> {
        let _ = (host, poll);
        todo!()
    }

    /// Stop watching and join the thread (a running session keeps going).
    pub fn stop(self) {
        todo!()
    }
}

/// Profiles the daemon watches: with match rules and not `daemon_exclude`
/// (`DaemonState.watching` counts them).
pub fn watched(file: &ConfigFile) -> Vec<(&str, &Profile)> {
    let _ = file;
    todo!()
}

/// The first window that matches a watched profile, skipping `ignored`
/// windows (the ones the person stopped) and LirtUp Forge's own
/// (`is_lirtup` decides, so this stays testable without X).
pub fn pick<'a>(
    windows: &'a [WindowInfo],
    file: &ConfigFile,
    ignored: &[u32],
    is_lirtup: impl Fn(&WindowInfo) -> bool,
) -> Option<(&'a WindowInfo, String)> {
    let _ = (windows, file, ignored, is_lirtup);
    todo!()
}
