//! Daemon mode (round 4, front Y): watch the windows and start a session on
//! the first one that matches a profile with match rules
//! (`pipeline/daemon.py`, `gui/helpers/daemon.py` of 1.x).
//!
//! Rules, as 1.x:
//! - every `daemon_poll_interval` seconds, list the windows (`X11::list_windows`)
//!   and, with no session running, take the first window that matches a
//!   profile whose rules are not empty and that is not `daemon_exclude`
//!   (profiles in file order, then windows in stacking order); focus it and
//!   start with that profile after `target_delay` (the game's own setup);
//! - a session the daemon started that ends because its window closed sends
//!   the daemon back to watching; a window whose session ended any other
//!   way (the person stopped it, it failed) is not started again until it
//!   closes, nor is one the person was scaling when the daemon came on;
//! - LirtUp Forge's own windows never match (`X11::is_lirtup_window`).
//!
//! The engine owns the daemon thread and reaches it through [`DaemonHost`];
//! `SetDaemon` and `--daemon` turn it on and off. With `keep_after_exit`,
//! `lirtup serve` keeps running when the GUI quits (`crate::lifecycle`).

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use lirtup_config::{ConfigFile, Profile};
use lirtup_x11::{WindowInfo, X11};

/// What the daemon needs of the engine.
pub trait DaemonHost: Send + Sync + 'static {
    /// The window of the session running now (started by anyone), if any.
    fn running_window(&self) -> Option<u32>;
    /// A session is starting (a `Start` still looking for its window).
    fn starting(&self) -> bool;
    /// The configuration file as the engine has it now (after
    /// `ReloadConfig`).
    fn config_file(&self) -> ConfigFile;
    /// Start a session on `window` with `profile` (it sends `DaemonMatch`
    /// and the session events).
    fn start_matched(&self, window: WindowInfo, profile: String);
}

/// The daemon thread.
pub struct Daemon {
    stop: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

impl Daemon {
    /// Start watching every `poll`.
    pub fn spawn(host: Arc<dyn DaemonHost>, poll: Duration) -> crate::Result<Daemon> {
        let (stop, stopped) = mpsc::channel();
        let poll = poll.max(Duration::from_millis(100));
        let thread = std::thread::Builder::new()
            .name("lirtup-daemon".into())
            .spawn(move || watch(&*host, poll, &stopped))?;
        Ok(Daemon { stop, thread })
    }

    /// Stop watching and join the thread (a running session keeps going).
    pub fn stop(self) {
        let _ = self.stop.send(());
        if self.thread.join().is_err() {
            tracing::warn!("the daemon thread panicked");
        }
    }
}

/// The thread: one X connection, a scan every `poll` until `stopped`.
fn watch(host: &dyn DaemonHost, poll: Duration, stopped: &mpsc::Receiver<()>) {
    let x11 = match X11::connect() {
        Ok(x) => x,
        Err(e) => {
            tracing::error!("daemon: cannot open the X display: {e}");
            return;
        }
    };
    tracing::info!("daemon: watching every {:.1} s", poll.as_secs_f32());
    let mut tracker = Tracker::default();
    // What was running when the daemon came on is the person's.
    tracker.observe(host.running_window());
    loop {
        if let Err(e) = scan(host, &x11, &mut tracker) {
            tracing::warn!("daemon: {e}");
        }
        match stopped.recv_timeout(poll) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    tracing::info!("daemon: stopped");
}

fn scan(host: &dyn DaemonHost, x11: &X11, tracker: &mut Tracker) -> crate::Result<()> {
    let running = host.running_window();
    tracker.observe(running);
    if running.is_some() || host.starting() {
        return Ok(());
    }
    let windows = x11.list_windows()?;
    tracker.forget_closed(&windows);
    let file = host.config_file();
    let is_lirtup = |w: &WindowInfo| x11.is_lirtup_window(w).unwrap_or(true);
    if let Some((window, profile)) = pick(&windows, &file, &tracker.ignored, is_lirtup) {
        tracing::info!(
            "daemon: {:?} (0x{:08x}) matches the profile {profile:?}",
            window.title,
            window.xid
        );
        if let Err(e) = x11.activate_window(window.xid) {
            tracing::debug!("daemon: cannot focus 0x{:08x}: {e}", window.xid);
        }
        tracker.started(window.xid);
        host.start_matched(window.clone(), profile);
    }
    Ok(())
}

/// Which windows the daemon leaves alone.
#[derive(Debug, Default)]
struct Tracker {
    /// The session's window at the last scan.
    running: Option<u32>,
    /// Started by the daemon, not running yet.
    pending: Option<u32>,
    /// Windows whose session ended while open (see the module docs).
    ignored: Vec<u32>,
}

impl Tracker {
    fn observe(&mut self, running: Option<u32>) {
        if running.is_some() {
            self.pending = None;
        }
        if self.running != running
            && let Some(ended) = self.running
            && !self.ignored.contains(&ended)
        {
            self.ignored.push(ended);
        }
        self.running = running;
    }

    fn started(&mut self, xid: u32) {
        self.pending = Some(xid);
    }

    /// Forget the ignored windows that closed. A start that never ran (it
    /// failed before its session existed) ignores its window too.
    fn forget_closed(&mut self, windows: &[WindowInfo]) {
        if let Some(xid) = self.pending.take()
            && !self.ignored.contains(&xid)
        {
            self.ignored.push(xid);
        }
        self.ignored.retain(|xid| windows.iter().any(|w| w.xid == *xid));
    }
}

/// Profiles the daemon watches: with match rules and not `daemon_exclude`
/// (`DaemonState.watching` counts them).
pub fn watched(file: &ConfigFile) -> Vec<(&str, &Profile)> {
    file.profiles
        .iter()
        .filter(|(_, p)| p.matcher.as_ref().is_some_and(|m| !m.is_empty()) && !p.daemon_excluded())
        .map(|(name, p)| (name.as_str(), p))
        .collect()
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
    let candidates: Vec<&WindowInfo> = windows
        .iter()
        .filter(|w| !ignored.contains(&w.xid) && !w.is_lirtup_class())
        .collect();
    for (name, profile) in watched(file) {
        let matcher = profile.matcher.as_ref()?;
        let found = candidates
            .iter()
            .find(|w| matcher.matches(&w.title, w.width(), w.height()) && !is_lirtup(w));
        if let Some(window) = found {
            return Some((window, name.to_owned()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use lirtup_x11::Rect;

    fn window(xid: u32, title: &str) -> WindowInfo {
        WindowInfo {
            xid,
            title: title.into(),
            rect: Rect {
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
            },
            pid: None,
            class: None,
        }
    }

    fn file(yaml: &str) -> ConfigFile {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("lirtup-daemon-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        let file = ConfigFile::load(&path).unwrap().0;
        let _ = std::fs::remove_dir_all(&dir);
        file
    }

    const PROFILES: &str = "\
profiles:
  Excluded:
    match: {title_contains: Demons}
    options: {daemon_exclude: true}
  ByName:
    options: {}
  Uta:
    match: {title_contains: Utawarerumono}
  Wa:
    match: {title_contains: WHITE ALBUM}
";

    #[test]
    fn watches_profiles_with_rules_not_excluded() {
        let file = file(PROFILES);
        let names: Vec<&str> = watched(&file).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["Uta", "Wa"]);
    }

    #[test]
    fn picks_in_profile_order_and_skips() {
        let file = file(PROFILES);
        let windows = [
            window(1, "Demons Roots"),
            window(2, "WHITE ALBUM2"),
            window(3, "Utawarerumono: Mask of Truth"),
            window(4, "Firefox"),
        ];
        let never = |_: &WindowInfo| false;
        let (w, profile) = pick(&windows, &file, &[], never).unwrap();
        assert_eq!((w.xid, profile.as_str()), (3, "Uta"));
        // The one the person stopped is left alone.
        let (w, profile) = pick(&windows, &file, &[3], never).unwrap();
        assert_eq!((w.xid, profile.as_str()), (2, "Wa"));
        // LirtUp Forge's own windows never match.
        assert!(pick(&windows, &file, &[3], |w: &WindowInfo| w.xid == 2).is_none());
        assert!(pick(&windows[..1], &file, &[], never).is_none());
    }

    #[test]
    fn tracker_ignores_ended_sessions_until_their_window_closes() {
        let windows = [window(7, "a"), window(8, "b")];
        let mut t = Tracker::default();
        // The person was scaling 7 when the daemon came on.
        t.observe(Some(7));
        t.observe(None);
        assert_eq!(t.ignored, [7]);
        // The daemon starts 8; it runs, then the person stops it.
        t.started(8);
        t.observe(Some(8));
        t.forget_closed(&windows);
        t.observe(None);
        assert_eq!(t.ignored, [7, 8]);
        // 7 closes.
        t.forget_closed(&windows[1..]);
        assert_eq!(t.ignored, [8]);
        // A start that never ran ignores its window.
        let mut t = Tracker::default();
        t.started(7);
        t.observe(None);
        t.forget_closed(&windows);
        assert_eq!(t.ignored, [7]);
    }
}
