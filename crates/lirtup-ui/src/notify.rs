//! Desktop notifications (round 4, front Y): the session started (by the
//! person or by the daemon) or failed, with a «Detener» action
//! (`notify-rust`, the freedesktop notifications over D-Bus). Only while
//! the window is hidden or unfocused; never one per `Stats`. With the GUI
//! closed, `lirtup serve` sends its own (`lirtup_core::server`).

use dioxus::desktop::tao::event::{Event as TaoEvent, WindowEvent};
use dioxus::desktop::use_wry_event_handler;
use dioxus::prelude::*;
use lirtup_proto::Command;

use crate::i18n::{arg, tr, tr_args};
use crate::link::CoreLink;
use crate::state::{Action, SessionState, Store};

/// What a session change is worth telling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// Scaling this window («Detener» stops it).
    Scaling(String),
    /// Scaling failed, with the reason.
    Failed(String),
}

/// What to notify when the session goes from `before` to `now` (pure, for
/// the tests). `None` for the first look (the snapshot of a session that
/// was already running).
pub fn notice(before: Option<&SessionState>, now: &SessionState) -> Option<Notice> {
    let before = before?;
    match (before, now) {
        (SessionState::Running(a), SessionState::Running(b)) if a.window.xid == b.window.xid => None,
        (_, SessionState::Running(view)) => Some(Notice::Scaling(view.window.title.clone())),
        (SessionState::Failed { .. }, SessionState::Failed { .. }) => None,
        (_, SessionState::Failed { message, .. }) => Some(Notice::Failed(message.clone())),
        _ => None,
    }
}

/// Watch the store and notify (called once by `app.rs`).
pub fn use_notifications(store: Signal<Store>, dispatch: Callback<Action>) {
    let _ = dispatch;
    let core = use_context::<CoreLink>();
    let mut focused = use_signal(|| true);
    let mut before = use_signal(|| None::<SessionState>);

    use_wry_event_handler(move |event, _| {
        if let TaoEvent::WindowEvent {
            event: WindowEvent::Focused(f),
            ..
        } = event
        {
            focused.set(*f);
        }
    });

    use_effect(move || {
        let s = store.read();
        let now = s.session.clone();
        let quiet = s.visible && *focused.peek();
        drop(s);
        let previous = before.peek().clone();
        before.set(Some(now.clone()));
        if quiet {
            return;
        }
        if let Some(notice) = notice(previous.as_ref(), &now) {
            show(notice, core.clone());
        }
    });
}

/// Show `notice` on a thread of its own (waiting for «Detener» blocks).
fn show(notice: Notice, core: CoreLink) {
    let (body, stop) = match &notice {
        Notice::Scaling(title) => (tr_args("notify-scaling", &[("title", arg(title.clone()))]), true),
        Notice::Failed(message) => (
            tr_args("notify-failed", &[("error", arg(message.clone()))]),
            false,
        ),
    };
    let label = tr("notify-stop");
    let spawned = std::thread::Builder::new()
        .name("lirtup-notify".into())
        .spawn(move || {
            let mut n = notify_rust::Notification::new();
            n.appname("LirtUp Forge")
                .summary("LirtUp Forge")
                .body(&body)
                .icon("lirtup-forge");
            if stop {
                n.action("stop", &label);
            }
            match n.show() {
                Ok(handle) if stop => handle.wait_for_action(|action| {
                    if action == "stop" {
                        tracing::info!("notification: Detener");
                        core.send(Command::Stop);
                    }
                }),
                Ok(_) => {}
                Err(e) => tracing::debug!("notification: {e}"),
            }
        });
    if let Err(e) = spawned {
        tracing::debug!("notification: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lirtup_proto::{SessionKind, SessionView, Size, Stage, WindowRef};

    fn running(xid: u32) -> SessionState {
        SessionState::Running(SessionView {
            window: WindowRef {
                xid,
                title: format!("w{xid}"),
                class: None,
                pid: None,
            },
            profile: None,
            mode: None,
            kind: SessionKind::Fullscreen,
            src: Size::default(),
            dst: Size::default(),
            chain: Vec::new(),
            started_at_ms: 0,
        })
    }

    #[test]
    fn only_changes_are_told() {
        let failed = SessionState::Failed {
            stage: Stage::Capture,
            message: "BadWindow".into(),
            retry: true,
        };
        // The first look is the snapshot: nothing.
        assert_eq!(notice(None, &running(1)), None);
        assert_eq!(
            notice(Some(&SessionState::Idle), &running(1)),
            Some(Notice::Scaling("w1".into()))
        );
        // Stats and the like do not change the session.
        assert_eq!(notice(Some(&running(1)), &running(1)), None);
        assert_eq!(
            notice(Some(&running(1)), &running(2)),
            Some(Notice::Scaling("w2".into()))
        );
        assert_eq!(
            notice(Some(&running(1)), &failed),
            Some(Notice::Failed("BadWindow".into()))
        );
        assert_eq!(notice(Some(&failed), &failed), None);
        assert_eq!(notice(Some(&running(1)), &SessionState::Idle), None);
    }
}
