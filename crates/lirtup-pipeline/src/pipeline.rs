//! The session thread (`pipeline.py`, `session.py`, `controller.py`).
//! Front P, round 2.
//!
//! [`Session::start`] spawns one `std::thread` per session that owns the
//! capture, the [`crate::upscale::Upscaler`], the
//! [`crate::presenter::Presenter`], the [`crate::surface::OverlayThread`],
//! the OSD, the stats and the menu bar detector, and runs the 1.x loop
//! (`_process_one_frame`): wait for the last present, capture, menu bar,
//! OSD/stats update, idle detection, upscale, present, input mapping,
//! swapchain recreation, fps cap, pause. Between frames it drains its
//! [`PipelineCommand`]s and the overlay's notices.
//!
//! This is what front K's `Engine` uses; nothing else of the crate is
//! needed by `lirtup-core` except [`crate::framegen::status_from_setup`],
//! [`crate::bench`] (in the `lirtup bench` child) and [`crate::surface`]
//! (only through the session).
//!
//! # Events the session thread sends
//!
//! On `SessionSetup::events`, in this order:
//!
//! - `Event::SessionStarted` once the first frame is presented
//!   (`Engine` sends `SessionStarting` itself before [`Session::start`]);
//! - `Event::Stats` every 500 ms ([`crate::stats::Stats::poll`]);
//! - `Event::FrameGen` when the frame generation state or the active
//!   multiplier changes (front F sends it from the presenter's `FrameGen`);
//! - `Event::MenuBarDetected` once, if a bar is cropped;
//! - `Event::SessionError` on failure (with `retry: false`; `Engine`
//!   decides about retries), then `Event::SessionStopped { reason: Error }`;
//! - `Event::SessionStopped` when it ends for any other reason (stop, window
//!   closed, overlay closed).
//!
//! # Hotkeys
//!
//! `Engine` forwards the session hotkeys as [`PipelineCommand::Hotkey`]:
//! `ToggleScaling`, `Screenshot`, `ToggleStats`, `CycleModel`,
//! `CycleGeometry`, `RestoreView`, `ZoomIn`, `ZoomOut`, `Offset*`. It keeps
//! `StartFullscreen`, `StartWindowed` and `ExitApp` for itself.
//!
//! # Session kind and overlay mode
//!
//! `SessionKind::Windowed` always uses `OverlayMode::Windowed`;
//! `SessionKind::Fullscreen` uses the configured `overlay_mode`, or
//! `fullscreen` if that is `windowed`.

use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender;
use lirtup_config::{Config, HotkeyAction};
use lirtup_fg::layer::LayerSetup;
use lirtup_gpu::Device;
use lirtup_proto::{Event, SessionKind, SessionView, StopReason};
use lirtup_x11::WindowInfo;

use crate::Result;

/// Everything a session needs.
pub struct SessionSetup {
    /// The device (`Engine` opens it once, after `layer::prepare`, and keeps
    /// it between sessions).
    pub device: Arc<Device>,
    /// The window to scale.
    pub window: WindowInfo,
    /// Fullscreen or windowed.
    pub kind: SessionKind,
    /// The resolved configuration (profile, mode and CLI layers applied).
    pub config: Arc<Config>,
    /// Profile name, for `SessionView` and the panel.
    pub profile: Option<String>,
    /// Mode name, for `SessionView` and the panel.
    pub mode: Option<String>,
    /// What `lirtup_fg::layer::prepare` did at startup.
    pub layer: LayerSetup,
    /// Where the session's events go (`Engine` relays them to the socket).
    pub events: Sender<Event>,
}

/// What `Engine` can ask of a running session.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineCommand {
    /// `ReloadConfig` resolved again for this window (same profile, or a
    /// new one if the match changed).
    UpdateConfig {
        /// The new configuration.
        config: Arc<Config>,
        /// Profile name.
        profile: Option<String>,
        /// Mode name.
        mode: Option<String>,
    },
    /// A session hotkey (see the module docs for which).
    Hotkey(HotkeyAction),
    /// Pause or resume (`ToggleScaling` from the tray or the GUI).
    SetPaused(bool),
    /// Scale another window with the same settings (`follow_focus`, the
    /// daemon replacing a session).
    SwitchWindow(WindowInfo),
    /// End the session; `SessionStopped` carries `reason`.
    Stop(StopReason),
}

/// A running session. Dropping it stops the session (`StopReason::User`)
/// and joins its thread.
pub struct Session {
    _private: (),
}

impl Session {
    /// Spawn the session thread. Returns as soon as the thread runs; setup
    /// errors arrive as `Event::SessionError` (the window may close, the
    /// device may fail while effects compile, ...).
    pub fn start(setup: SessionSetup) -> Result<Session> {
        todo!("front P")
    }

    /// Send a command. `false` if the session has ended.
    pub fn send(&self, command: PipelineCommand) -> bool {
        todo!("front P")
    }

    /// The session as the GUI shows it, once started (`None` while setting
    /// up).
    pub fn view(&self) -> Option<SessionView> {
        todo!("front P")
    }

    /// The thread has ended (stopped, failed, window closed).
    pub fn is_finished(&self) -> bool {
        todo!("front P")
    }

    /// Stop with `reason` and wait at most `timeout` for the thread.
    pub fn stop(self, reason: StopReason, timeout: Duration) {
        todo!("front P")
    }
}
