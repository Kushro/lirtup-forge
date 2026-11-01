//! The session card (6.1 «Tarjeta "Escalando …"»): amber while starting
//! and running, error-container with the detail and «Reintentar» when the
//! session failed. Nothing while idle.

use dioxus::prelude::*;
use lirtup_proto::{SessionView, Stage, StatsSample};

use super::LAST_START;
use crate::app::{use_dispatch, use_store};
use crate::components::{Button, ButtonVariant, Icon, LoadingIndicator};
use crate::state::{Action, SessionState, Store};

/// The card.
#[component]
pub fn SessionCard() -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let s = store.read();
    match s.session.clone() {
        SessionState::Idle => rsx! {},
        SessionState::Starting { target } => rsx! {
            section {
                class: "flex flex-wrap items-center gap-5 px-6 py-5 rounded-xl bg-tertiary-container text-on-tertiary-container",
                role: "status",
                LoadingIndicator { size: 48, label: t!("home-session-starting-aria") }
                div { class: "flex flex-col gap-0.5 flex-[1_1_280px] min-w-0",
                    span { class: "t-tl truncate", {t!("home-session-starting", title = target.title)} }
                }
                Button {
                    label: t!("home-session-stop"),
                    variant: ButtonVariant::Tertiary,
                    icon: "stop",
                    onclick: move |_| dispatch(Action::StopScaling),
                }
            }
        },
        SessionState::Running(view) => {
            let line = summary(&view, s.stats.as_ref(), &s);
            let warning = s.stats.as_ref().and_then(|st| st.gpu_warning.clone());
            drop(s);
            rsx! {
                section {
                    class: "flex flex-wrap items-center gap-5 px-6 py-5 rounded-xl bg-tertiary-container text-on-tertiary-container",
                    role: "status",
                    span { class: "grid place-items-center size-14 shrink-0 rounded-[18px] bg-tertiary text-on-tertiary",
                        Icon { name: "play_arrow", fill: true, size: 32 }
                    }
                    div { class: "flex flex-col gap-0.5 flex-[1_1_280px] min-w-0",
                        span { class: "t-tl truncate", {t!("home-session-running", title = view.window.title.clone())} }
                        span { class: "mono t-bs", "{line}" }
                        if let Some(w) = warning {
                            span { class: "t-bs flex items-center gap-1",
                                Icon { name: "warning", size: 18 }
                                {t!("home-session-gpu-warning", gpu = format!("{:.1}", w.gpu_ms), budget = format!("{:.1}", w.budget_ms))}
                            }
                        }
                    }
                    Button {
                        label: t!("home-session-restart"),
                        variant: ButtonVariant::Outlined,
                        icon: "restart_alt",
                        onclick: move |_| dispatch(Action::RestartScaling),
                    }
                    Button {
                        label: t!("home-session-stop"),
                        variant: ButtonVariant::Tertiary,
                        icon: "stop",
                        title: t!("home-session-stop-hint"),
                        onclick: move |_| dispatch(Action::StopScaling),
                    }
                }
            }
        }
        SessionState::Failed {
            stage,
            message,
            retry,
        } => {
            drop(s);
            let last = LAST_START.read().clone();
            rsx! {
                section {
                    class: "flex flex-wrap items-center gap-5 px-6 py-5 rounded-xl bg-error-container text-on-error-container",
                    role: "alert",
                    span { class: "grid place-items-center size-14 shrink-0 rounded-[18px] bg-error text-on-error",
                        Icon { name: "error", fill: true, size: 32 }
                    }
                    div { class: "flex flex-col gap-0.5 flex-[1_1_280px] min-w-0",
                        span { class: "t-tl", {t!("home-session-failed", stage = stage_label(stage))} }
                        span { class: "mono t-bs break-words select-text", "{message}" }
                    }
                    if let (true, Some(action)) = (retry, last) {
                        Button {
                            label: t!("home-session-retry"),
                            variant: ButtonVariant::Danger,
                            icon: "refresh",
                            onclick: move |_| dispatch(action.clone()),
                        }
                    }
                }
            }
        }
    }
}

/// `Anime nítido · 1280×720 → 2560×1440 · 60 → 120 fps (×2)`.
fn summary(view: &SessionView, stats: Option<&StatsSample>, store: &Store) -> String {
    let mut parts = Vec::new();
    if let Some(mode) = &view.mode {
        let name = store
            .modes
            .store
            .modes
            .get(mode)
            .map(|m| m.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| mode.clone());
        parts.push(name);
    }
    parts.push(format!(
        "{}×{} → {}×{}",
        view.src.width, view.src.height, view.dst.width, view.dst.height
    ));
    if let Some(stats) = stats {
        let mut fps = format!("{:.0} → {:.0} fps", stats.capture_fps, stats.present_fps);
        if let Some(m) = stats.multiplier {
            fps.push_str(&format!(" (×{m})"));
        }
        parts.push(fps);
    }
    parts.join(" · ")
}

/// Where a session failed, translated.
pub fn stage_label(stage: Stage) -> String {
    match stage {
        Stage::Config => t!("home-stage-config"),
        Stage::Window => t!("home-stage-window"),
        Stage::Device => t!("home-stage-device"),
        Stage::Capture => t!("home-stage-capture"),
        Stage::Effects => t!("home-stage-effects"),
        Stage::Overlay => t!("home-stage-overlay"),
        Stage::Swapchain => t!("home-stage-swapchain"),
        Stage::FrameGen => t!("home-stage-framegen"),
        Stage::Runtime => t!("home-stage-runtime"),
    }
}

#[cfg(test)]
mod tests {
    use lirtup_proto::{SessionKind, Size, WindowRef};

    use super::*;

    #[test]
    fn summary_line() {
        let view = SessionView {
            window: WindowRef {
                xid: 1,
                title: "Juego".into(),
                class: None,
                pid: None,
            },
            profile: None,
            mode: Some("anime".into()),
            kind: SessionKind::Fullscreen,
            src: Size {
                width: 1280,
                height: 720,
            },
            dst: Size {
                width: 2560,
                height: 1440,
            },
            chain: Vec::new(),
            started_at_ms: 0,
        };
        let store = Store::default();
        assert_eq!(summary(&view, None, &store), "anime · 1280×720 → 2560×1440");
        let stats = StatsSample {
            capture_fps: 59.8,
            present_fps: 119.6,
            multiplier: Some(2),
            ..StatsSample::default()
        };
        assert_eq!(
            summary(&view, Some(&stats), &store),
            "anime · 1280×720 → 2560×1440 · 60 → 120 fps (×2)"
        );
    }
}
