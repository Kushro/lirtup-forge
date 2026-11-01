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
//! As in 1.x `controller.py`: zoom steps through `50 %` to `800 %` of the
//! monitor in `fit` (25 % steps, starting at the configured geometry);
//! `CycleGeometry` goes `fit` → `stretch` → `cover`; the offsets move by 25
//! pixels with the 1.x signs (`OffsetLeft` adds to `offset_x`).
//!
//! # Session kind and overlay mode
//!
//! `SessionKind::Windowed` always uses `OverlayMode::Windowed`;
//! `SessionKind::Fullscreen` uses the configured `overlay_mode`, or
//! `fullscreen` if that is `windowed`.
//!
//! # Switching windows
//!
//! [`PipelineCommand::SwitchWindow`] (and `follow_focus`) keep the overlay,
//! the swapchain and the presenter: the overlay forwards input to the new
//! window (`OverlayCommand::SetTarget`), and only the capture and the
//! upscaler are rebuilt for it (in windowed mode the overlay also takes the
//! new window's size). The stats, the view and the configuration stay.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arc_swap::ArcSwapOption;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use lirtup_config::{Config, Crop, HotkeyAction, OutputGeometry, OverlayMode};
use lirtup_effects::Pass;
use lirtup_fg::layer::LayerSetup;
use lirtup_gpu::{Device, Format, PresentMode, Timestamps};
use lirtup_proto::{Event, SessionKind, SessionView, Size, Stage, StopReason, WindowRef};
use lirtup_x11::{
    Capture, CaptureOptions, CoordinateMapper, DamageRect, OverlayOptions, Rect, RgbaImage, WindowInfo, X11,
    X11Error,
};

use crate::framegen::{FrameGen, FrameGenSetup};
use crate::menubar::{MenuBarCheck, MenuBarDetector};
use crate::osd::Osd;
use crate::presenter::{Presenter, output_geometry};
use crate::stats::{FrameGenLine, FrameKind, PanelContext, PresentRecord, Stats, StatsOptions, StatsPanel};
use crate::surface::{OverlayCommand, OverlayNotice, OverlayThread, PresentSurface, SurfaceDesc};
use crate::upscale::{Upscaler, next_model};
use crate::{PipelineError, Result};

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
    /// Time every pass with Vulkan timestamps (a client shows GPU times;
    /// `SetTimestamps` changes it). The stats panel asks for them too while
    /// it is visible.
    pub timestamps: bool,
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
    /// A client started or stopped showing GPU times (see
    /// [`SessionSetup::timestamps`]).
    SetTimestamps(bool),
    /// End the session; `SessionStopped` carries `reason`.
    Stop(StopReason),
}

/// A running session. Dropping it stops the session (`StopReason::User`)
/// and joins its thread.
pub struct Session {
    commands: Sender<PipelineCommand>,
    view: Arc<ArcSwapOption<SessionView>>,
    finished: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Session {
    /// Spawn the session thread. Returns as soon as the thread runs; setup
    /// errors arrive as `Event::SessionError` (the window may close, the
    /// device may fail while effects compile, ...).
    pub fn start(setup: SessionSetup) -> Result<Session> {
        let (commands, rx) = crossbeam_channel::unbounded();
        let view = Arc::new(ArcSwapOption::empty());
        let finished = Arc::new(AtomicBool::new(false));
        let thread = {
            let view = view.clone();
            let finished = finished.clone();
            std::thread::Builder::new()
                .name("lirtup-session".to_owned())
                .spawn(move || session_thread(setup, rx, view, finished))?
        };
        Ok(Session {
            commands,
            view,
            finished,
            thread: Some(thread),
        })
    }

    /// Send a command. `false` if the session has ended.
    pub fn send(&self, command: PipelineCommand) -> bool {
        !self.is_finished() && self.commands.send(command).is_ok()
    }

    /// The session as the GUI shows it, once started (`None` while setting
    /// up).
    pub fn view(&self) -> Option<SessionView> {
        self.view.load_full().map(|v| (*v).clone())
    }

    /// The thread has ended (stopped, failed, window closed).
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    /// Stop with `reason` and wait at most `timeout` for the thread.
    pub fn stop(mut self, reason: StopReason, timeout: Duration) {
        let _ = self.commands.send(PipelineCommand::Stop(reason));
        self.join(timeout);
    }

    fn join(&mut self, timeout: Duration) {
        let Some(thread) = self.thread.take() else { return };
        let deadline = Instant::now() + timeout;
        while !thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if thread.is_finished() {
            let _ = thread.join();
        } else {
            tracing::warn!("session thread did not stop within {timeout:?}; detaching it");
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.thread.is_some() {
            let _ = self.commands.send(PipelineCommand::Stop(StopReason::User));
            self.join(Duration::from_secs(5));
        }
    }
}

// -------------------------------------------------------------------------
// The thread
// -------------------------------------------------------------------------

/// Sets `finished` however the thread ends; on a panic, also reports it.
struct FinishGuard {
    finished: Arc<AtomicBool>,
    view: Arc<ArcSwapOption<SessionView>>,
    events: Sender<Event>,
}

impl Drop for FinishGuard {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let _ = self.events.send(Event::SessionError {
                stage: Stage::Runtime,
                message: "the session thread panicked".to_owned(),
                retry: false,
            });
            let _ = self.events.send(Event::SessionStopped {
                reason: StopReason::Error,
            });
        }
        self.view.store(None);
        self.finished.store(true, Ordering::Release);
    }
}

fn session_thread(
    setup: SessionSetup,
    rx: Receiver<PipelineCommand>,
    view: Arc<ArcSwapOption<SessionView>>,
    finished: Arc<AtomicBool>,
) {
    let events = setup.events.clone();
    let _guard = FinishGuard {
        finished,
        view: view.clone(),
        events: events.clone(),
    };
    let result = match Runner::new(setup, view) {
        Ok(mut runner) => {
            let result = runner.run(&rx);
            runner.close();
            result
        }
        Err(e) => Err(e),
    };
    match result {
        Ok(reason) => {
            let _ = events.send(Event::SessionStopped { reason });
        }
        Err(e) => {
            tracing::error!("session failed ({:?}): {e}", e.stage());
            let _ = events.send(Event::SessionError {
                stage: e.stage(),
                message: e.to_string(),
                retry: false,
            });
            let _ = events.send(Event::SessionStopped {
                reason: StopReason::Error,
            });
        }
    }
}

/// Zoom levels of `ZoomIn`/`ZoomOut` (`ZOOM_LEVELS`: 50 % to 800 %).
const ZOOM_LEVELS: std::ops::RangeInclusive<u32> = 2..=32;
/// One zoom step, in percent.
const ZOOM_STEP: u32 = 25;
/// Geometries of `CycleGeometry` (`OUTPUT_GEOMETRIES`).
const GEOMETRIES: [&str; 3] = ["fit", "stretch", "cover"];
/// Offset hotkey step, in pixels (`controller.py`).
const OFFSET_STEP: i32 = 25;
/// How often `Event::Stats` and the panel are refreshed.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(500);

/// `hide_cursor` (ms) as the overlay takes it.
fn hide_cursor_after(config: &Config) -> Option<Duration> {
    config.hide_cursor.map(|ms| Duration::from_millis(u64::from(ms)))
}

/// The overlay mode of a session kind (module docs).
pub fn overlay_mode(kind: SessionKind, configured: OverlayMode) -> OverlayMode {
    match (kind, configured) {
        (SessionKind::Windowed, _) => OverlayMode::Windowed,
        (SessionKind::Fullscreen, OverlayMode::Windowed) => OverlayMode::Fullscreen,
        (SessionKind::Fullscreen, mode) => mode,
    }
}

/// The overlay's area (`compute_overlay_geometry`): the monitor, or in
/// windowed mode the geometry's size for the window, centered on the
/// monitor and moved by the configured offset.
pub fn overlay_rect(config: &Config, mode: OverlayMode, window: (u32, u32), monitor: Rect) -> Rect {
    if mode != OverlayMode::Windowed {
        return monitor;
    }
    let geometry = config
        .output_geometry
        .parse()
        .unwrap_or(lirtup_config::Geometry::Fit);
    let ((w, h), _, _) = output_geometry(&geometry, window, (monitor.width, monitor.height));
    let (w, h) = (w.max(1), h.max(1));
    Rect {
        // 1.x centers vertically with the monitor's width (a bug); the
        // height is used here.
        x: monitor.x + (i64::from(monitor.width) - i64::from(w)).div_euclid(2) as i32 + config.offset_x,
        y: monitor.y + (i64::from(monitor.height) - i64::from(h)).div_euclid(2) as i32 + config.offset_y,
        width: w,
        height: h,
    }
}

/// Size left after cropping, or `None` when nothing is left.
fn cropped(size: (u32, u32), crop: &Crop) -> Option<(u32, u32)> {
    let w = size.0.checked_sub(crop.left + crop.right)?;
    let h = size.1.checked_sub(crop.top + crop.bottom)?;
    (w > 0 && h > 0).then_some((w, h))
}

fn add_crop(a: Crop, b: Crop) -> Crop {
    Crop {
        top: a.top + b.top,
        bottom: a.bottom + b.bottom,
        left: a.left + b.left,
        right: a.right + b.right,
    }
}

fn present_mode(mode: lirtup_config::PresentMode) -> PresentMode {
    match mode {
        lirtup_config::PresentMode::Fifo => PresentMode::Fifo,
        lirtup_config::PresentMode::Mailbox => PresentMode::Mailbox,
        lirtup_config::PresentMode::Immediate => PresentMode::Immediate,
    }
}

/// Spanish or English by `LANG`, as 1.x.
fn tr(es: &str, en: &str) -> String {
    let spanish = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .is_some_and(|v| v.starts_with("es"));
    if spanish { es } else { en }.to_owned()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn window_ref(window: &WindowInfo) -> WindowRef {
    WindowRef {
        xid: window.xid,
        title: window.title.clone(),
        class: window.class.as_ref().map(|c| c.class.clone()),
        pid: window.pid,
    }
}

fn merge_timestamps(a: Option<Timestamps>, b: Option<Timestamps>) -> Option<Timestamps> {
    match (a, b) {
        (Some(mut a), Some(b)) => {
            a.dispatches.extend(b.dispatches);
            a.total_ms += b.total_ms;
            Some(a)
        }
        (a, b) => a.or(b),
    }
}

/// Everything that depends on the target window.
struct Parts {
    window: WindowInfo,
    /// Configured crop plus the detected menu bar.
    crop: Crop,
    capture: Capture,
    buffer: Vec<u8>,
    overlay: OverlayThread,
    upscaler: Upscaler,
    presenter: Presenter,
    osd: Osd,
    panel: StatsPanel,
}

/// The session thread's state.
struct Runner {
    device: Arc<Device>,
    events: Sender<Event>,
    layer: LayerSetup,
    kind: SessionKind,
    profile: Option<String>,
    mode: Option<String>,
    /// The working configuration (hotkeys change model and geometry here).
    config: Config,
    initial_geometry: OutputGeometry,
    x11: X11,
    monitor: Rect,
    refresh_hz: f32,
    stats: Stats,
    parts: Option<Parts>,
    menu_crop: Crop,
    menubar: Option<MenuBarDetector>,
    /// Zoom in percent (100 = the geometry).
    zoom: u32,
    view_offset: (i32, i32),
    user_paused: bool,
    focus_paused: bool,
    next_focus_check: Instant,
    /// Present the next frame even if the window did not change.
    stale: bool,
    mapper: Option<CoordinateMapper>,
    started_at_ms: Option<u64>,
    capture_failures: u32,
    last_frame: Option<Instant>,
    damage: Vec<DamageRect>,
    view: Arc<ArcSwapOption<SessionView>>,
    /// [`SessionSetup::timestamps`].
    timestamps: bool,
}

impl Runner {
    fn new(setup: SessionSetup, view: Arc<ArcSwapOption<SessionView>>) -> Result<Runner> {
        let config = (*setup.config).clone();
        let x11 = X11::connect()?;
        let (monitor, refresh) = x11.monitor_area(&config.monitor)?;
        let refresh_hz = refresh.unwrap_or(60.0);
        let stats = Stats::new(StatsOptions {
            sample_interval: SAMPLE_INTERVAL,
            refresh_hz,
            latency_csv: config.latency_log.clone(),
        })?;
        let now = Instant::now();
        let mut runner = Runner {
            device: setup.device,
            events: setup.events,
            layer: setup.layer,
            kind: setup.kind,
            profile: setup.profile,
            mode: setup.mode,
            initial_geometry: config.output_geometry.clone(),
            menubar: config.auto_crop_menu_bar.then(|| MenuBarDetector::new(now)),
            config,
            x11,
            monitor,
            refresh_hz,
            stats,
            parts: None,
            menu_crop: Crop::default(),
            zoom: 100,
            view_offset: (0, 0),
            user_paused: false,
            focus_paused: false,
            next_focus_check: now,
            stale: true,
            mapper: None,
            started_at_ms: None,
            capture_failures: 0,
            last_frame: None,
            damage: Vec::new(),
            view,
            timestamps: setup.timestamps,
        };
        runner.parts = Some(runner.build(setup.window)?);
        Ok(runner)
    }

    fn overlay_mode(&self) -> OverlayMode {
        overlay_mode(self.kind, self.config.overlay_mode)
    }

    /// Capture, overlay, swapchain, upscaler, presenter, OSD and panel for
    /// `window`.
    fn build(&mut self, window: WindowInfo) -> Result<Parts> {
        let crop = add_crop(self.config.crop(), self.menu_crop);
        let capture = Capture::new(&CaptureOptions {
            window: window.xid,
            crop,
            tile_size: self.config.tile_size,
            damage_tracking: self.config.use_damage_tracking,
        })?;
        let src = capture.size();
        let buffer = vec![0u8; capture.buffer_len()];

        let mode = self.overlay_mode();
        let rect = overlay_rect(
            &self.config,
            mode,
            (window.width(), window.height()),
            self.monitor,
        );
        let (hotkeys, _) = self.config.hotkey_bindings();
        let overlay = OverlayThread::spawn(
            OverlayOptions {
                mode,
                rect,
                title: format!("lirtup — {}", window.title),
                target: window.xid,
                opacity: self.config.overlay_opacity_max,
                click_through: mode == OverlayMode::TopTransparent,
                bypass_compositor: mode == OverlayMode::Fullscreen,
            },
            hotkeys,
        )?;
        let size = overlay.rect();
        let surface = PresentSurface::create(
            &self.device,
            overlay.surface(),
            &SurfaceDesc {
                format: Format::B8G8R8A8UnormSrgb,
                buffers: 2,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: present_mode(self.config.vulkan_present_mode),
            },
        )
        .map_err(|e| PipelineError::other(Stage::Swapchain, e.to_string()))?
        .shared();

        let framegen = FrameGen::new(
            &self.config,
            &self.layer,
            FrameGenSetup {
                device: self.device.clone(),
                surface: surface.clone(),
                refresh_hz: self.refresh_hz,
                size: (size.width.max(1), size.height.max(1)),
                sink: self.stats.sink(),
                events: self.events.clone(),
            },
        )?;
        let upscaler = Upscaler::new(&self.device, &self.config, src)
            .map_err(|e| PipelineError::other(Stage::Effects, e.to_string()))?;
        let mut presenter = Presenter::new(
            &self.device,
            &self.config,
            src,
            (self.monitor.width, self.monitor.height),
            surface,
            framegen,
        )?;
        presenter.set_source(upscaler.output());
        let osd = Osd::new(&self.device, presenter.screen(), &self.config)?;
        let panel = StatsPanel::new(&self.device, presenter.screen(), self.config.show_stats)?;
        overlay.send(OverlayCommand::HideCursorAfter(hide_cursor_after(&self.config)));
        self.stale = true;
        self.mapper = None;
        let mut parts = Parts {
            window,
            crop,
            capture,
            buffer,
            overlay,
            upscaler,
            presenter,
            osd,
            panel,
        };
        self.apply_view(&mut parts);
        Ok(parts)
    }

    /// Tear down what depends on the window (presenter first: it closes the
    /// frame generation thread, which presents on the overlay).
    fn teardown(parts: Parts) {
        let Parts {
            presenter,
            overlay,
            osd,
            panel,
            upscaler,
            ..
        } = parts;
        presenter.close();
        drop((osd, panel, upscaler));
        overlay.close(Duration::from_secs(2));
    }

    fn close(mut self) {
        if let Some(parts) = self.parts.take() {
            Runner::teardown(parts);
        }
        if let Err(e) = self.stats.close() {
            tracing::warn!("closing the stats: {e}");
        }
    }

    // ---------------------------------------------------------------------
    // Loop
    // ---------------------------------------------------------------------

    fn run(&mut self, rx: &Receiver<PipelineCommand>) -> Result<StopReason> {
        loop {
            // Commands.
            loop {
                match rx.try_recv() {
                    Ok(command) => {
                        if let Some(reason) = self.handle_command(command)? {
                            return Ok(reason);
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(StopReason::User),
                }
            }
            // Overlay notices.
            if let Some(reason) = self.drain_notices()? {
                return Ok(reason);
            }
            self.check_focus()?;

            if self.user_paused || self.focus_paused {
                self.wait_paused(rx);
                continue;
            }
            match self.frame()? {
                FrameResult::Stop(reason) => return Ok(reason),
                FrameResult::Idle => self.wait_idle(rx),
                FrameResult::Done => {}
            }
        }
    }

    /// Paused: wake on a command or a notice, or after the poll interval.
    fn wait_paused(&self, rx: &Receiver<PipelineCommand>) {
        let timeout = Duration::from_secs_f64(self.config.pipeline_poll_interval.max(0.01));
        self.wait_any(rx, timeout);
    }

    /// Nothing to present: wait a refresh interval (no fence paces the
    /// loop), waking early on a command or a notice.
    fn wait_idle(&self, rx: &Receiver<PipelineCommand>) {
        let timeout = Duration::from_secs_f32(1.0 / self.refresh_hz.max(1.0));
        self.wait_any(rx, timeout);
    }

    /// Block until a command or an overlay notice is ready (without taking
    /// it: the loop drains both next), or `timeout`.
    fn wait_any(&self, rx: &Receiver<PipelineCommand>, timeout: Duration) {
        let mut select = crossbeam_channel::Select::new();
        select.recv(rx);
        if let Some(parts) = self.parts.as_ref() {
            select.recv(parts.overlay.notices());
        }
        let _ = select.ready_timeout(timeout);
    }

    fn handle_command(&mut self, command: PipelineCommand) -> Result<Option<StopReason>> {
        match command {
            PipelineCommand::Stop(reason) => return Ok(Some(reason)),
            PipelineCommand::Hotkey(action) => self.handle_hotkey(action)?,
            PipelineCommand::SetPaused(paused) => self.set_paused(paused),
            PipelineCommand::SetTimestamps(on) => {
                self.timestamps = on;
                if !on && !self.parts.as_ref().is_some_and(|p| p.panel.visible()) {
                    self.stats.clear_gpu_times();
                }
            }
            PipelineCommand::SwitchWindow(window) => self.switch_window(window)?,
            PipelineCommand::UpdateConfig {
                config,
                profile,
                mode,
            } => {
                self.profile = profile;
                self.mode = mode;
                self.update_config((*config).clone())?;
            }
        }
        Ok(None)
    }

    fn drain_notices(&mut self) -> Result<Option<StopReason>> {
        loop {
            let notice = {
                let Some(parts) = self.parts.as_ref() else {
                    return Ok(None);
                };
                match parts.overlay.notices().try_recv() {
                    Ok(n) => n,
                    Err(TryRecvError::Empty) => return Ok(None),
                    Err(TryRecvError::Disconnected) => {
                        return Err(PipelineError::other(Stage::Overlay, "the overlay thread ended"));
                    }
                }
            };
            match notice {
                OverlayNotice::Configured(rect) => self.resize(rect.width, rect.height)?,
                OverlayNotice::Exposed => self.stale = true,
                OverlayNotice::Hotkey(action) => self.handle_hotkey(action)?,
                OverlayNotice::Closed => return Ok(Some(StopReason::User)),
                OverlayNotice::Failed => {
                    return Err(PipelineError::other(Stage::Overlay, "the overlay window failed"));
                }
            }
        }
    }

    /// The overlay (swapchain) has a new size.
    fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        let Some(parts) = self.parts.as_mut() else {
            return Ok(());
        };
        parts.presenter.resize(width, height)?;
        parts.osd.rebind(&self.device, parts.presenter.screen())?;
        parts.panel.rebind(&self.device, parts.presenter.screen())?;
        self.stale = true;
        Ok(())
    }

    /// `follow_focus` and `pause_on_focus_loss` (`_update_pause_state`).
    fn check_focus(&mut self) -> Result<()> {
        if !self.config.follow_focus && !self.config.pause_on_focus_loss {
            return Ok(());
        }
        let now = Instant::now();
        if now < self.next_focus_check {
            return Ok(());
        }
        self.next_focus_check = now + Duration::from_secs_f64(self.config.focus_poll_interval.max(0.05));
        let Some(active) = self.x11.active_window()? else {
            return Ok(());
        };
        let (target, overlay) = match self.parts.as_ref() {
            Some(p) => (p.window.xid, p.overlay.surface().window()),
            None => return Ok(()),
        };
        if active.xid == target || active.xid == overlay {
            if self.focus_paused {
                self.focus_paused = false;
                self.show_overlay();
            }
            return Ok(());
        }
        if self.config.follow_focus {
            return self.switch_window(active);
        }
        if !self.focus_paused {
            self.focus_paused = true;
            if let Some(p) = self.parts.as_ref() {
                p.overlay.send(OverlayCommand::SetVisible(false));
            }
        }
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) {
        self.user_paused = paused;
        if paused {
            if let Some(p) = self.parts.as_ref() {
                p.overlay.send(OverlayCommand::SetVisible(false));
            }
        } else {
            self.show_overlay();
        }
    }

    fn show_overlay(&mut self) {
        if !self.user_paused && !self.focus_paused {
            if let Some(p) = self.parts.as_ref() {
                p.overlay.send(OverlayCommand::SetVisible(true));
            }
            self.stale = true;
        }
    }

    fn switch_window(&mut self, window: WindowInfo) -> Result<()> {
        if self.parts.as_ref().is_some_and(|p| p.window.xid == window.xid) {
            return Ok(());
        }
        tracing::info!("switching to window 0x{:x} ({})", window.xid, window.title);
        self.menu_crop = Crop::default();
        self.menubar = self
            .config
            .auto_crop_menu_bar
            .then(|| MenuBarDetector::new(Instant::now()));
        self.started_at_ms = None;
        let Some(parts) = self.parts.as_mut() else {
            self.parts = Some(self.build(window)?);
            return Ok(());
        };
        let xid = window.xid;
        parts.window = window;
        parts.overlay.send(OverlayCommand::SetTarget(xid));
        self.rebuild_capture()?;
        if self.overlay_mode() == OverlayMode::Windowed
            && let Some(parts) = self.parts.as_ref()
        {
            let rect = overlay_rect(
                &self.config,
                OverlayMode::Windowed,
                (parts.window.width(), parts.window.height()),
                self.monitor,
            );
            parts.overlay.send(OverlayCommand::SetMode {
                mode: OverlayMode::Windowed,
                rect,
            });
        }
        // The content rect may not change: send the mapper again anyway.
        self.mapper = None;
        self.stale = true;
        Ok(())
    }

    /// The window changed size: new capture and upscaler (and overlay size
    /// in windowed mode).
    fn window_resized(&mut self) -> Result<Option<StopReason>> {
        let Some(parts) = self.parts.as_mut() else {
            return Ok(None);
        };
        let Some(info) = self.x11.window_info(parts.window.xid)? else {
            return Ok(Some(StopReason::WindowClosed));
        };
        parts.window = info;
        self.rebuild_capture()?;
        if self.overlay_mode() == OverlayMode::Windowed
            && let Some(parts) = self.parts.as_ref()
        {
            let rect = overlay_rect(
                &self.config,
                OverlayMode::Windowed,
                (parts.window.width(), parts.window.height()),
                self.monitor,
            );
            parts.overlay.send(OverlayCommand::SetMode {
                mode: OverlayMode::Windowed,
                rect,
            });
        }
        Ok(None)
    }

    /// New capture (crop or size changed) and the upscaler for its size.
    fn rebuild_capture(&mut self) -> Result<()> {
        let crop = add_crop(self.config.crop(), self.menu_crop);
        let Some(parts) = self.parts.as_mut() else {
            return Ok(());
        };
        if cropped((parts.window.width(), parts.window.height()), &crop).is_none() {
            return Err(PipelineError::other(
                Stage::Capture,
                format!(
                    "nothing left of {}x{} after cropping",
                    parts.window.width(),
                    parts.window.height()
                ),
            ));
        }
        parts.capture = Capture::new(&CaptureOptions {
            window: parts.window.xid,
            crop,
            tile_size: self.config.tile_size,
            damage_tracking: self.config.use_damage_tracking,
        })?;
        parts.crop = crop;
        parts.buffer = vec![0u8; parts.capture.buffer_len()];
        let src = parts.capture.size();
        parts.presenter.set_capture_size(src);
        self.recreate_upscaler()
    }

    fn recreate_upscaler(&mut self) -> Result<()> {
        let Some(parts) = self.parts.as_mut() else {
            return Ok(());
        };
        // The old one may still be in flight.
        parts.upscaler.finish()?;
        parts.upscaler = Upscaler::new(&self.device, &self.config, parts.capture.size())?;
        parts.presenter.set_source(parts.upscaler.output());
        self.stale = true;
        self.publish_view();
        Ok(())
    }

    fn update_config(&mut self, config: Config) -> Result<()> {
        let old = std::mem::replace(&mut self.config, config);
        // The view the hotkeys changed is kept.
        self.initial_geometry = self.config.output_geometry.clone();
        let needs_capture = old.crop() != self.config.crop()
            || old.tile_size != self.config.tile_size
            || old.use_damage_tracking != self.config.use_damage_tracking;
        let needs_upscaler = old.scaling.model != self.config.scaling.model
            || old.scaling.double_upscale != self.config.scaling.double_upscale
            || old.use_tile_processing != self.config.use_tile_processing
            || old.tile_context_margin != self.config.tile_context_margin
            || old.max_tile_layers != self.config.max_tile_layers
            || old.area_threshold != self.config.area_threshold;
        if old.overlay_mode != self.config.overlay_mode
            || old.monitor != self.config.monitor
            || old.vulkan_present_mode != self.config.vulkan_present_mode
        {
            // The overlay and the swapchain depend on these: rebuild.
            if let Some(parts) = self.parts.take() {
                let window = parts.window.clone();
                Runner::teardown(parts);
                let (monitor, refresh) = self.x11.monitor_area(&self.config.monitor)?;
                self.monitor = monitor;
                self.refresh_hz = refresh.unwrap_or(60.0);
                self.stats.set_refresh_hz(self.refresh_hz);
                self.parts = Some(self.build(window)?);
            }
            return Ok(());
        }
        if needs_capture {
            self.rebuild_capture()?;
        } else if needs_upscaler {
            self.recreate_upscaler()?;
        }
        let Some(parts) = self.parts.as_mut() else {
            return Ok(());
        };
        parts.presenter.update_config(&self.config)?;
        parts.osd.update_config(&self.config);
        if old.show_stats != self.config.show_stats {
            parts.panel.set_visible(self.config.show_stats);
        }
        if old.overlay_opacity_max != self.config.overlay_opacity_max {
            parts
                .overlay
                .send(OverlayCommand::SetOpacity(self.config.overlay_opacity_max));
        }
        if old.hide_cursor != self.config.hide_cursor {
            parts
                .overlay
                .send(OverlayCommand::HideCursorAfter(hide_cursor_after(&self.config)));
        }
        let mut parts = self.parts.take().expect("checked above");
        self.apply_view(&mut parts);
        self.parts = Some(parts);
        self.stale = true;
        self.publish_view();
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Hotkeys (`controller.py`)
    // ---------------------------------------------------------------------

    fn handle_hotkey(&mut self, action: HotkeyAction) -> Result<()> {
        match action {
            HotkeyAction::ToggleScaling => {
                let paused = !self.user_paused;
                self.set_paused(paused);
            }
            HotkeyAction::Screenshot => self.screenshot(),
            HotkeyAction::ToggleStats => {
                if let Some(p) = self.parts.as_mut() {
                    let visible = !p.panel.visible();
                    p.panel.set_visible(visible);
                    self.stale = true;
                    if !visible && !self.timestamps {
                        self.stats.clear_gpu_times();
                    }
                }
            }
            HotkeyAction::CycleModel => {
                self.config.scaling.model = next_model(self.config.scaling.model);
                self.recreate_upscaler()?;
                let model = self.config.scaling.model.as_str();
                self.osd(&tr(&format!("Modelo: {model}"), &format!("Model: {model}")));
            }
            HotkeyAction::CycleGeometry => {
                let current = GEOMETRIES
                    .iter()
                    .position(|g| *g == self.config.output_geometry.0.trim())
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let next = GEOMETRIES[current % GEOMETRIES.len()];
                self.config.output_geometry = OutputGeometry(next.to_owned());
                self.zoom = 100;
                self.apply_config_view()?;
                self.osd(&tr(&format!("Geometría: {next}"), &format!("Geometry: {next}")));
            }
            HotkeyAction::RestoreView => {
                self.config.output_geometry = self.initial_geometry.clone();
                self.zoom = 100;
                self.view_offset = (0, 0);
                self.apply_config_view()?;
                self.osd(&tr("Vista restaurada", "View restored"));
            }
            HotkeyAction::ZoomIn | HotkeyAction::ZoomOut => {
                self.zoom = step_zoom(self.zoom, action == HotkeyAction::ZoomIn);
                self.apply_config_view()?;
                let zoom = self.zoom;
                self.osd(&format!("Zoom: {zoom}%"));
            }
            HotkeyAction::OffsetLeft => self.move_view(OFFSET_STEP, 0),
            HotkeyAction::OffsetRight => self.move_view(-OFFSET_STEP, 0),
            HotkeyAction::OffsetUp => self.move_view(0, OFFSET_STEP),
            HotkeyAction::OffsetDown => self.move_view(0, -OFFSET_STEP),
            HotkeyAction::StartFullscreen | HotkeyAction::StartWindowed | HotkeyAction::ExitApp => {
                tracing::debug!("{action:?} is handled by the engine");
            }
        }
        Ok(())
    }

    fn move_view(&mut self, dx: i32, dy: i32) {
        self.view_offset.0 += dx;
        self.view_offset.1 += dy;
        if let Some(mut parts) = self.parts.take() {
            self.apply_view(&mut parts);
            self.parts = Some(parts);
        }
        self.stale = true;
    }

    /// The geometry or the zoom changed: tell the presenter.
    fn apply_config_view(&mut self) -> Result<()> {
        if let Some(mut parts) = self.parts.take() {
            let result = parts.presenter.update_config(&self.config);
            self.apply_view(&mut parts);
            self.parts = Some(parts);
            result?;
        }
        self.stale = true;
        Ok(())
    }

    /// Zoom and the whole content offset: the configured one outside
    /// windowed mode (there it moves the window), plus the hotkeys'.
    fn apply_view(&self, parts: &mut Parts) {
        let base = if self.overlay_mode() == OverlayMode::Windowed {
            (0, 0)
        } else {
            (self.config.offset_x, self.config.offset_y)
        };
        parts.presenter.set_view(
            self.zoom as f32 / 100.0,
            (base.0 + self.view_offset.0, base.1 + self.view_offset.1),
        );
    }

    fn osd(&mut self, text: &str) {
        if let Some(p) = self.parts.as_mut() {
            p.osd.show(text);
        }
    }

    /// Read the screen back and save it on another thread
    /// (`_save_screenshot`).
    fn screenshot(&mut self) {
        let Some(parts) = self.parts.as_ref() else { return };
        let image = match parts.presenter.screenshot() {
            Ok(image) => image,
            Err(e) => {
                tracing::error!("screenshot failed: {e}");
                self.osd(&tr("Captura fallida", "Screenshot failed"));
                return;
            }
        };
        let path = screenshot_path(
            &self.config.screenshot_dir,
            &self.config.screenshot_filename,
            &ScreenshotVars {
                unix_s: now_ms() / 1000,
                title: &parts.window.title,
                profile: self.profile.as_deref().unwrap_or(&parts.window.title),
                model: self.config.scaling.model.as_str(),
                width: image.width,
                height: image.height,
            },
        );
        let spawned = std::thread::Builder::new()
            .name("lirtup-screenshot".to_owned())
            .spawn(move || match save_png(&path, &image) {
                Ok(()) => tracing::info!("screenshot saved to '{}'", path.display()),
                Err(e) => tracing::error!("cannot save the screenshot '{}': {e}", path.display()),
            });
        match spawned {
            Ok(_) => self.osd(&tr("Captura guardada", "Screenshot saved")),
            Err(e) => {
                tracing::error!("screenshot thread: {e}");
                self.osd(&tr("Captura fallida", "Screenshot failed"));
            }
        }
    }

    // ---------------------------------------------------------------------
    // One frame (`_process_one_frame`)
    // ---------------------------------------------------------------------

    fn frame(&mut self) -> Result<FrameResult> {
        // The previous present was waited for inside `Presenter::present`
        // (module docs of `presenter`); the fps cap paces the loop here.
        if let (Some(max), Some(last)) = (self.config.max_fps, self.last_frame)
            && max > 0
        {
            let next = last + Duration::from_secs_f64(1.0 / f64::from(max));
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            }
        }

        // 1. Capture.
        let capture_time = Instant::now();
        let grabbed = {
            let parts = self.parts.as_mut().expect("parts while running");
            parts.capture.grab(&mut parts.buffer, &mut self.damage)
        };
        let frame = match grabbed {
            Ok(frame) => frame,
            Err(X11Error::NoWindow(_)) => return Ok(FrameResult::Stop(StopReason::WindowClosed)),
            Err(X11Error::Resized { .. }) => {
                return Ok(match self.window_resized()? {
                    Some(reason) => FrameResult::Stop(reason),
                    None => FrameResult::Done,
                });
            }
            Err(e) => {
                self.capture_failures += 1;
                tracing::error!("frame grab failed: {e}");
                if self.capture_failures >= self.config.max_capture_failures {
                    return Err(PipelineError::other(
                        Stage::Capture,
                        format!("{} capture failures in a row: {e}", self.capture_failures),
                    ));
                }
                std::thread::sleep(Duration::from_secs_f64(
                    self.config.capture_failure_delay.max(0.0),
                ));
                return Ok(FrameResult::Done);
            }
        };
        self.capture_failures = 0;
        self.stats.record_capture(capture_time, frame.dirty);

        // 2. Menu bar.
        if let Some(detector) = self.menubar.as_mut() {
            let parts = self.parts.as_ref().expect("parts while running");
            match detector.check(
                capture_time,
                &parts.buffer,
                frame.width,
                frame.height,
                frame.stride,
            ) {
                MenuBarCheck::Pending => {}
                MenuBarCheck::NotFound => self.menubar = None,
                MenuBarCheck::Found(crop) => {
                    self.menubar = None;
                    self.menu_crop = crop;
                    self.rebuild_capture()?;
                    let _ = self.events.send(Event::MenuBarDetected {
                        profile: self.profile.clone(),
                        crop: add_crop(self.config.crop(), crop),
                    });
                    self.osd(&tr("Barra de menú recortada", "Menu bar cropped"));
                    return Ok(FrameResult::Done);
                }
            }
        }

        // 3. OSD and panel.
        let now = Instant::now();
        let sample = self.stats.latest().cloned();
        let context = self.panel_context();
        let parts = self.parts.as_mut().expect("parts while running");
        let mut redraw = parts.osd.update(now)?;
        if let Some(sample) = sample {
            redraw |= parts.panel.update(&sample, &context)?;
        }

        // 4. Idle frame.
        let fg_active = parts.presenter.framegen().is_some_and(|fg| fg.is_active());
        if !frame.dirty && !redraw && !self.stale {
            if self.config.present_on_change_only || fg_active {
                self.poll_stats();
                return Ok(FrameResult::Idle);
            }
            let outcome = parts.presenter.present_unchanged()?;
            self.stats.sink().record(PresentRecord {
                capture: None,
                presented: Instant::now(),
                kind: FrameKind::Real,
                multiplier: 1,
            });
            if outcome.recreate {
                let rect = parts.overlay.rect();
                self.resize(rect.width, rect.height)?;
            }
            self.last_frame = Some(capture_time);
            self.poll_stats();
            return Ok(FrameResult::Done);
        }
        self.stale = false;

        // 5. Upscale.
        let render_start = Instant::now();
        let full = frame.full || !frame.dirty;
        let timed = self.timestamps || parts.panel.visible();
        let upscale = parts
            .upscaler
            .process(&parts.buffer, frame.stride, full, &self.damage, timed)?;
        self.stats.record_dirty(upscale.dirty_fraction, upscale.tiles);

        // 6. Present.
        parts.presenter.set_source(parts.upscaler.output());
        if let Some(fg) = parts.presenter.framegen_mut() {
            fg.mark_capture(capture_time);
        }
        let overlays: Vec<&dyn Pass> = parts.osd.pass().into_iter().chain(parts.panel.pass()).collect();
        let outcome = parts.presenter.present(&overlays, timed)?;
        let upscale_times = parts.upscaler.finish()?;
        if let Some(times) = merge_timestamps(upscale_times, outcome.timestamps.clone()) {
            self.stats.set_gpu_times(&times);
        }
        let has_fg = parts.presenter.framegen().is_some();
        if !has_fg {
            // With frame generation, `FrameGen` reports its own presents.
            self.stats.sink().record(PresentRecord {
                capture: Some(capture_time),
                presented: Instant::now(),
                kind: FrameKind::Real,
                multiplier: 1,
            });
        }
        self.stats
            .set_multiplier(parts.presenter.framegen().and_then(|fg| fg.active_multiplier()));
        self.stats.record_render(
            render_start.elapsed().as_secs_f32() * 1000.0,
            outcome.waited.as_secs_f32() * 1000.0,
        );
        self.last_frame = Some(capture_time);

        // 7. Input mapping.
        let mut mapper = parts.presenter.mapper();
        mapper.source_size = parts.capture.size();
        mapper.crop_offset = (parts.crop.left, parts.crop.top);
        if self.mapper != Some(mapper) {
            parts.overlay.send(OverlayCommand::SetMapper(mapper));
            self.mapper = Some(mapper);
        }

        // 8. Swapchain recreation.
        if outcome.recreate {
            let rect = parts.overlay.rect();
            self.resize(rect.width, rect.height)?;
        }

        // 9. Started, stats.
        if self.started_at_ms.is_none() {
            self.started_at_ms = Some(now_ms());
            if let Some(view) = self.session_view() {
                let _ = self.events.send(Event::SessionStarted(view));
            }
            self.publish_view();
        }
        self.poll_stats();
        Ok(FrameResult::Done)
    }

    fn poll_stats(&mut self) {
        if let Some(sample) = self.stats.poll(Instant::now()) {
            let _ = self.events.send(Event::Stats(sample));
        }
        let disabled = self
            .parts
            .as_mut()
            .map(|p| p.presenter.take_disabled())
            .unwrap_or_default();
        for (name, error) in disabled {
            let _ = self.events.send(Event::EffectCompile {
                name,
                done: true,
                error: Some(error),
                session: true,
            });
        }
    }

    // ---------------------------------------------------------------------
    // Views
    // ---------------------------------------------------------------------

    fn chain(&self) -> Vec<String> {
        let Some(parts) = self.parts.as_ref() else {
            return Vec::new();
        };
        let mut chain = parts.upscaler.labels();
        chain.extend(parts.presenter.chain());
        chain
    }

    fn session_view(&self) -> Option<SessionView> {
        let parts = self.parts.as_ref()?;
        let (sw, sh) = parts.capture.size();
        let screen = parts.presenter.screen();
        Some(SessionView {
            window: window_ref(&parts.window),
            profile: self.profile.clone(),
            mode: self.mode.clone(),
            kind: self.kind,
            src: Size {
                width: sw,
                height: sh,
            },
            dst: Size {
                width: screen.width(),
                height: screen.height(),
            },
            chain: self.chain(),
            started_at_ms: self.started_at_ms?,
        })
    }

    /// Update what [`Session::view`] returns (once started).
    fn publish_view(&self) {
        if let Some(view) = self.session_view() {
            self.view.store(Some(Arc::new(view)));
        }
    }

    fn panel_context(&self) -> PanelContext {
        let Some(parts) = self.parts.as_ref() else {
            return PanelContext::default();
        };
        let c = &self.config;
        let m = &c.scaling;
        let screen = parts.presenter.screen();
        let framegen = match parts.presenter.framegen() {
            Some(fg) => fg.panel_line(),
            None => match self.layer.state {
                lirtup_fg::FrameGenState::Active
                    if c.frame_gen_engine == lirtup_config::FrameGenEngine::Layer =>
                {
                    Some(FrameGenLine {
                        text: format!("x{} lsfg-vk", self.layer.multiplier),
                        multiplier: Some(self.layer.multiplier),
                        suffix: " est.".to_owned(),
                        layer: true,
                        ..FrameGenLine::default()
                    })
                }
                lirtup_fg::FrameGenState::External => Some(FrameGenLine {
                    text: "lsfg-vk (LSFGVK_* env)".to_owned(),
                    ..FrameGenLine::default()
                }),
                _ => None,
            },
        };
        let post = [
            ("Deband".to_owned(), m.deband_enabled),
            ("CAS".to_owned(), m.cas_enabled),
            ("Bloom".to_owned(), m.bloom_enabled),
            ("Vignette".to_owned(), m.vignette_enabled),
            (format!("LUT {}", m.lut_preset), m.lut_enabled),
            ("Grain".to_owned(), m.grain_enabled),
        ]
        .into_iter()
        .filter_map(|(name, on)| on.then_some(name))
        .collect();
        let generating = parts.presenter.framegen().is_some_and(|fg| fg.is_active())
            || (self.layer.state == lirtup_fg::FrameGenState::Active
                && c.frame_gen_engine == lirtup_config::FrameGenEngine::Layer);
        let mut present = serde_name(&c.vulkan_present_mode);
        if c.present_on_change_only || generating {
            present += ", new frames only";
        }
        PanelContext {
            title: parts.window.title.clone(),
            profile: self.profile.clone(),
            mode: self.mode.clone(),
            model: m.model.as_str().to_owned(),
            double_upscale: m.double_upscale,
            src: parts.capture.size(),
            crop: parts.crop,
            dst: (screen.width(), screen.height()),
            output: format!(
                "{}  {}/{}",
                c.output_geometry.0,
                serde_name(&m.upsampler),
                serde_name(&m.downsampler)
            ),
            chain: self.chain(),
            engine: match m.effects_engine {
                lirtup_config::EffectsEngine::Magpiefx => "MagpieFX".to_owned(),
                _ => "Native".to_owned(),
            },
            post,
            present,
            framegen,
        }
    }
}

/// The config spelling of a unit enum (`lanczos`, `fifo`, ...).
fn serde_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// What one turn of the loop did.
enum FrameResult {
    /// Presented (or skipped a failed grab).
    Done,
    /// Nothing changed and nothing was presented: wait a little.
    Idle,
    /// The session ends.
    Stop(StopReason),
}

/// One zoom step in or out, wrapping like 1.x (`50 %` ↔ `800 %`).
pub fn step_zoom(zoom: u32, zoom_in: bool) -> u32 {
    let levels: Vec<u32> = ZOOM_LEVELS.map(|i| i * ZOOM_STEP).collect();
    let i = levels.iter().position(|&l| l >= zoom).unwrap_or(levels.len() - 1);
    let i = if zoom_in {
        if levels[i] > zoom {
            i
        } else {
            (i + 1) % levels.len()
        }
    } else if i == 0 {
        levels.len() - 1
    } else {
        i - 1
    };
    levels[i]
}

/// What the screenshot filename template can use (`_download_and_save`).
pub struct ScreenshotVars<'a> {
    /// Unix time in seconds.
    pub unix_s: u64,
    /// Window title.
    pub title: &'a str,
    /// Profile (the title when there is none).
    pub profile: &'a str,
    /// SRCNN model.
    pub model: &'a str,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// The screenshot path: `screenshot_filename` in `screenshot_dir`, with
/// `{timestamp}` (or `{timestamp:%Y%m%d_%H%M%S}`, `%Y %m %d %H %M %S` in
/// UTC), `{title}`, `{profile}`, `{model}`, `{width}` and `{height}`; `.png`
/// added without an image extension.
pub fn screenshot_path(dir: &Path, template: &str, vars: &ScreenshotVars<'_>) -> PathBuf {
    let mut name = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        name.push_str(&rest[..start]);
        let Some(len) = rest[start..].find('}') else {
            name.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let field = &rest[start + 1..start + len];
        let (key, format) = field.split_once(':').unwrap_or((field, "%Y%m%d_%H%M%S"));
        match key {
            "timestamp" => name.push_str(&format_utc(vars.unix_s, format)),
            "title" => name.push_str(&sanitize(vars.title)),
            "profile" => name.push_str(&sanitize(vars.profile)),
            "model" => name.push_str(vars.model),
            "width" => name.push_str(&vars.width.to_string()),
            "height" => name.push_str(&vars.height.to_string()),
            _ => {
                name.push('{');
                name.push_str(field);
                name.push('}');
            }
        }
        rest = &rest[start + len + 1..];
    }
    name.push_str(rest);
    let lower = name.to_lowercase();
    if ![".png", ".jpg", ".jpeg", ".bmp"]
        .iter()
        .any(|e| lower.ends_with(e))
    {
        name.push_str(".png");
    }
    lirtup_config::paths::expand_tilde(dir).join(name)
}

/// A title as a file name part: no path separators or control characters.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// `strftime` with `%Y %m %d %H %M %S %%`, in UTC.
fn format_utc(unix_s: u64, format: &str) -> String {
    let days = (unix_s / 86_400) as i64;
    let secs = unix_s % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('Y') => out.push_str(&format!("{year:04}")),
            Some('m') => out.push_str(&format!("{month:02}")),
            Some('d') => out.push_str(&format!("{day:02}")),
            Some('H') => out.push_str(&format!("{:02}", secs / 3600)),
            Some('M') => out.push_str(&format!("{:02}", secs / 60 % 60)),
            Some('S') => out.push_str(&format!("{:02}", secs % 60)),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

fn save_png(path: &Path, image: &RgbaImage) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // 1.x saves RGB.
    let rgb: Vec<u8> = image
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    image::save_buffer(
        path,
        &rgb,
        image.width,
        image.height,
        image::ExtendedColorType::Rgb8,
    )
    .map_err(|e| PipelineError::other(Stage::Runtime, format!("saving '{}': {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_mode_follows_the_session_kind() {
        assert_eq!(
            overlay_mode(SessionKind::Windowed, OverlayMode::Fullscreen),
            OverlayMode::Windowed
        );
        assert_eq!(
            overlay_mode(SessionKind::Fullscreen, OverlayMode::Windowed),
            OverlayMode::Fullscreen
        );
        assert_eq!(
            overlay_mode(SessionKind::Fullscreen, OverlayMode::TopTransparent),
            OverlayMode::TopTransparent
        );
    }

    #[test]
    fn overlay_rect_covers_the_monitor_or_centers_the_window() {
        let monitor = Rect {
            x: 1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let mut config = Config::default();
        assert_eq!(
            overlay_rect(&config, OverlayMode::AlwaysOnTop, (800, 600), monitor),
            monitor
        );
        config.output_geometry = OutputGeometry("1600x".into());
        config.offset_x = 10;
        assert_eq!(
            overlay_rect(&config, OverlayMode::Windowed, (800, 600), monitor),
            Rect {
                x: 1920 + 160 + 10,
                y: -60,
                width: 1600,
                height: 1200
            }
        );
    }

    #[test]
    fn zoom_steps_wrap() {
        assert_eq!(step_zoom(100, true), 125);
        assert_eq!(step_zoom(100, false), 75);
        assert_eq!(step_zoom(50, false), 800);
        assert_eq!(step_zoom(800, true), 50);
    }

    #[test]
    fn crop_helpers() {
        let crop = Crop {
            top: 10,
            bottom: 0,
            left: 5,
            right: 5,
        };
        assert_eq!(cropped((100, 100), &crop), Some((90, 90)));
        assert_eq!(cropped((10, 100), &crop), None);
        assert_eq!(add_crop(crop, crop).top, 20);
    }

    #[test]
    fn screenshot_path_fills_the_template() {
        let vars = ScreenshotVars {
            unix_s: 1_700_000_000, // 2023-11-14 22:13:20 UTC
            title: "a/b",
            profile: "game",
            model: "4x12",
            width: 1920,
            height: 1080,
        };
        let path = screenshot_path(
            Path::new("/tmp/shots"),
            "Screenshot_{timestamp:%Y%m%d_%H%M%S}.png",
            &vars,
        );
        assert_eq!(path, PathBuf::from("/tmp/shots/Screenshot_20231114_221320.png"));
        let path = screenshot_path(
            Path::new("/tmp"),
            "{profile}/{model}_{width}x{height}_{title}",
            &vars,
        );
        assert_eq!(path, PathBuf::from("/tmp/game/4x12_1920x1080_a_b.png"));
        let path = screenshot_path(Path::new("/tmp"), "{unknown}.jpg", &vars);
        assert_eq!(path, PathBuf::from("/tmp/{unknown}.jpg"));
    }

    #[test]
    fn timestamps_merge() {
        let a = Timestamps {
            dispatches: vec![("srcnn".into(), 1.0)],
            total_ms: 1.0,
        };
        let b = Timestamps {
            dispatches: vec![("lanczos".into(), 0.5)],
            total_ms: 0.5,
        };
        let m = merge_timestamps(Some(a.clone()), Some(b)).unwrap();
        assert_eq!(m.dispatches.len(), 2);
        assert_eq!(m.total_ms, 1.5);
        assert_eq!(merge_timestamps(Some(a.clone()), None), Some(a));
    }
}
