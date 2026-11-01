//! Finding the target window: by title, by PID of a launched program (two
//! phases), or the active one after a delay (`window/acquisition.py`).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::{Result, WindowInfo, X11, X11Error};

/// Pause between two looks at the window tree (1.x: 0.2 s).
const POLL: Duration = Duration::from_millis(200);

/// How to wait for a window of a launched program.
#[derive(Debug, Clone, PartialEq)]
pub struct PidSearch {
    /// PID of the launched program (its descendants count too).
    pub pid: u32,
    /// Substring of `WM_CLASS` for the class phase.
    pub class_hint: Option<String>,
    /// Seconds of the PID (+class) phase.
    pub pid_timeout: f64,
    /// Seconds of the class-only phase.
    pub class_timeout: f64,
    /// Maximum seconds overall.
    pub total_timeout: f64,
    /// 1 = PID first, 2 = class first.
    pub starting_phase: u8,
}

impl X11 {
    /// First window whose title contains `needle` (case-insensitive) or
    /// matches `regex`.
    ///
    /// `regex` wins when both are given (1.x) and is case-insensitive; an
    /// invalid pattern is an [`X11Error::Request`].
    pub fn find_by_title(&self, needle: Option<&str>, regex: Option<&str>) -> Result<Option<WindowInfo>> {
        let windows = self.list_windows()?;
        if let Some(pattern) = regex {
            let re = regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|e| X11Error::Request(format!("invalid title regex {pattern:?}: {e}")))?;
            return Ok(windows.into_iter().find(|w| re.is_match(&w.title)));
        }
        if let Some(needle) = needle {
            let needle = needle.to_lowercase();
            return Ok(windows
                .into_iter()
                .find(|w| w.title.to_lowercase().contains(&needle)));
        }
        Ok(None)
    }

    /// Poll until a window of `search.pid` appears (`find_window_by_pid`).
    ///
    /// Phase 1 looks for a viewable window whose `_NET_WM_PID` is the program
    /// or one of its descendants (re-read every round, launchers fork late)
    /// and, with a class hint, whose `WM_CLASS` contains it. Phase 2 looks by
    /// class only. They alternate until `total_timeout` (≤ 0: no limit), then
    /// [`X11Error::Timeout`].
    pub fn wait_for_pid(&self, search: &PidSearch) -> Result<WindowInfo> {
        let start = Instant::now();
        let total = (search.total_timeout > 0.0).then(|| Duration::from_secs_f64(search.total_timeout));
        let hint = search
            .class_hint
            .as_deref()
            .map(str::to_lowercase)
            .filter(|h| !h.is_empty());
        let mut phase = if search.starting_phase == 2 && hint.is_some() {
            2
        } else {
            1
        };
        loop {
            let phase_len = Duration::from_secs_f64(
                if phase == 1 {
                    search.pid_timeout
                } else {
                    search.class_timeout
                }
                .max(POLL.as_secs_f64()),
            );
            let phase_start = Instant::now();
            tracing::debug!("waiting for a window of pid {}: phase {phase}", search.pid);
            while phase_start.elapsed() < phase_len {
                if total.is_some_and(|t| start.elapsed() > t) {
                    return Err(X11Error::Timeout(search.total_timeout));
                }
                let pids = (phase == 1).then(|| descendant_pids(search.pid));
                if let Some(found) = self.find_window(pids.as_ref(), hint.as_deref())? {
                    tracing::debug!(
                        "found window 0x{:x} ({}) in phase {phase}",
                        found.xid,
                        found.title
                    );
                    return Ok(found);
                }
                std::thread::sleep(POLL);
            }
            if hint.is_some() {
                phase = if phase == 1 { 2 } else { 1 };
            }
        }
    }

    /// Sleep `delay`, then take the active window (`target_delay`).
    pub fn active_after(&self, delay: Duration) -> Result<Option<WindowInfo>> {
        std::thread::sleep(delay);
        self.active_window()
    }

    /// One pass of the PID / class search over the whole window tree.
    fn find_window(
        &self,
        pids: Option<&HashSet<u32>>,
        class_hint: Option<&str>,
    ) -> Result<Option<WindowInfo>> {
        if pids.is_none() && class_hint.is_none() {
            return Ok(None);
        }
        for win in self.all_windows()? {
            if win == self.root || !self.is_viewable(win)? {
                continue;
            }
            if let Some(pids) = pids {
                match self.window_pid(win)? {
                    Some(pid) if pids.contains(&pid) => {}
                    _ => continue,
                }
            }
            if let Some(hint) = class_hint {
                let Some(class) = self.window_class(win)? else {
                    continue;
                };
                if !class_matches(&class, hint) {
                    continue;
                }
            }
            if let Some(info) = self.window_info(win)? {
                return Ok(Some(info));
            }
        }
        Ok(None)
    }
}

/// Case-insensitive substring of the instance or the class (`hint` is
/// already lowercase).
fn class_matches(class: &crate::WindowClass, hint: &str) -> bool {
    class.instance.to_lowercase().contains(hint) || class.class.to_lowercase().contains(hint)
}

/// `pid` and every descendant, from `/proc/<pid>/task/<tid>/children`.
fn descendant_pids(pid: u32) -> HashSet<u32> {
    let mut out = HashSet::new();
    let mut stack = vec![pid];
    while let Some(p) = stack.pop() {
        if !out.insert(p) {
            continue;
        }
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{p}/task")) else {
            continue;
        };
        for task in tasks.flatten() {
            if let Ok(children) = std::fs::read_to_string(task.path().join("children")) {
                stack.extend(children.split_whitespace().filter_map(|c| c.parse::<u32>().ok()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_hint_matches_either_name() {
        let c = crate::WindowClass {
            instance: "steam_app_42".into(),
            class: "Steam".into(),
        };
        assert!(class_matches(&c, "steam"));
        assert!(class_matches(&c, "app_42"));
        assert!(!class_matches(&c, "wine"));
    }

    #[test]
    fn descendants_include_self() {
        let me = std::process::id();
        assert!(descendant_pids(me).contains(&me));
        // A PID that cannot exist still yields itself.
        assert_eq!(descendant_pids(u32::MAX), HashSet::from([u32::MAX]));
    }
}
