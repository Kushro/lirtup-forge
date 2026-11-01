//! The window list (front B, 6.4 of the analysis): a thread with its own
//! `lirtup-x11` connection lists the top-level windows every
//! `auto_refresh_ms` and hands them to the store (`Action::Windows`), only
//! when the list changed. The GUI's own window and the scaler's overlay
//! (`_LIRTUP_OVERLAY`, already left out by `X11::list_windows`, and the
//! `lirtup` class) are left out.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use dioxus::prelude::*;
use lirtup_x11::{WindowInfo, X11};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::state::{Action, Store};

/// Wait this long before trying again when there is no X connection.
const RECONNECT: Duration = Duration::from_secs(5);
/// Bounds of `auto_refresh_ms` (Ajustes ▸ Estilo).
const MIN_INTERVAL_MS: u32 = 200;
const MAX_INTERVAL_MS: u32 = 60_000;

/// Start the list (called once by `app.rs`). The thread ends when the app
/// does (its channel closes).
pub fn use_window_list(dispatch: Callback<Action>) {
    let interval = use_hook(|| Arc::new(AtomicU32::new(1000)));

    // Follow `gui.auto_refresh_ms` (the store is a context of `Root`).
    let store = try_use_context::<Signal<Store>>();
    let follow = interval.clone();
    use_effect(move || {
        if let Some(store) = store {
            let ms = store.read().gui.auto_refresh_ms;
            follow.store(ms.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS), Ordering::Relaxed);
        }
    });

    use_hook(move || {
        let (tx, mut rx) = unbounded_channel::<Vec<WindowInfo>>();
        let spawned = std::thread::Builder::new()
            .name("lirtup-windows".into())
            .spawn(move || run(tx, interval));
        if let Err(e) = spawned {
            tracing::error!("window list thread: {e}");
            return;
        }
        spawn(async move {
            while let Some(list) = rx.recv().await {
                dispatch(Action::Windows(list));
            }
        });
    });
}

/// The thread: list, filter, send on change, sleep.
fn run(tx: UnboundedSender<Vec<WindowInfo>>, interval: Arc<AtomicU32>) {
    let pid = std::process::id();
    let mut last: Option<Vec<WindowInfo>> = None;
    while !tx.is_closed() {
        let x11 = match X11::connect() {
            Ok(x11) => x11,
            Err(e) => {
                tracing::warn!("window list: no X connection: {e}");
                std::thread::sleep(RECONNECT);
                continue;
            }
        };
        while !tx.is_closed() {
            match x11.list_windows() {
                Ok(list) => {
                    let list = without_own(list, pid);
                    if last.as_ref() != Some(&list) {
                        last = Some(list.clone());
                        if tx.send(list).is_err() {
                            return;
                        }
                    }
                }
                Err(e) => {
                    // A broken connection: connect again.
                    tracing::warn!("window list: {e}");
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(u64::from(interval.load(Ordering::Relaxed))));
        }
    }
}

/// The list without the GUI and the scaler's own windows.
pub fn without_own(mut list: Vec<WindowInfo>, own_pid: u32) -> Vec<WindowInfo> {
    list.retain(|w| !is_own(w, own_pid));
    list
}

/// A window of LirtUp Forge itself: this process, or a `WM_CLASS` named
/// `lirtup` (the overlay: `lirtup`/`LirtUp`) or `lirtup-…` (the GUI, whose
/// class comes from the binary name).
pub fn is_own(window: &WindowInfo, own_pid: u32) -> bool {
    if window.pid == Some(own_pid) {
        return true;
    }
    let ours = |name: &str| {
        let name = name.to_ascii_lowercase();
        name == "lirtup" || name.starts_with("lirtup-") || name.starts_with("lirtup_")
    };
    window
        .class
        .as_ref()
        .is_some_and(|c| ours(&c.instance) || ours(&c.class))
}

#[cfg(test)]
mod tests {
    use lirtup_x11::{Rect, WindowClass};

    use super::*;

    fn window(xid: u32, pid: Option<u32>, class: Option<(&str, &str)>) -> WindowInfo {
        WindowInfo {
            xid,
            title: format!("w{xid}"),
            rect: Rect::default(),
            pid,
            class: class.map(|(instance, class)| WindowClass {
                instance: instance.into(),
                class: class.into(),
            }),
        }
    }

    #[test]
    fn leaves_out_the_gui_and_the_overlay() {
        let list = vec![
            window(1, Some(10), Some(("firefox", "firefox"))),
            window(2, Some(99), Some(("game.exe", "game.exe"))),
            window(3, Some(20), Some(("lirtup", "LirtUp"))),
            window(4, Some(30), Some(("lirtup-gui", "Lirtup-gui"))),
            window(5, None, None),
            window(6, Some(40), Some(("lirtupish", "Lirtupish"))),
        ];
        let kept: Vec<u32> = without_own(list, 99).iter().map(|w| w.xid).collect();
        assert_eq!(kept, vec![1, 5, 6]);
    }

    /// Against the real X server, when there is one: the list never holds
    /// this process's windows nor a `lirtup` class.
    #[test]
    fn real_list_has_no_own_windows() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let Ok(x11) = X11::connect() else { return };
        let list = without_own(x11.list_windows().expect("list_windows"), std::process::id());
        assert!(list.iter().all(|w| !is_own(w, std::process::id())));
        assert!(list.iter().all(|w| !w.title.is_empty()));
    }
}
