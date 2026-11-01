//! Frame generation thread of the built-in engine (`native_framegen.py`):
//! cycles, shared images, background restart. Front F, round 2.
//!
//! The presenter (front P) owns a [`FrameGen`] when the built-in engine is in
//! use and calls it instead of presenting a finished frame itself:
//!
//! 1. [`FrameGen::mark_capture`] with the capture time of the game frame;
//! 2. [`FrameGen::present`] with the final screen texture: waits until the
//!    present thread is done with the previous cycle, copies the texture into
//!    one of the two shared inputs, asks lsfg for `multiplier - 1` frames and
//!    hands the cycle to the present thread (`lsfg-present`), which presents
//!    them and then the real one, paced by [`crate::pacing`]. Returns without
//!    waiting for the pacing.
//!
//! Every present is reported to the [`PresentSink`] with the capture time.
//! Any error turns frame generation off for the session (presentation goes
//! on, real frames only) and is reported with an `Event::FrameGen` whose
//! state is `NoNative` and whose `reason` says why.
//!
//! With the lsfg-vk layer, none of this runs: the layer generates frames
//! inside our own swapchain; [`FrameGen::new`] returns `None`.
//!
//! # How it works
//!
//! - **lsfg** starts on a thread of its own ([`EngineStart`]) at the first
//!   frame, and again whenever the number of generated frames, the flow
//!   scale or the performance mode changes (lsfg fixes them when it
//!   initializes). Meanwhile the real frames are presented alone, from the
//!   session thread. With *Auto*, lsfg waits for the first choice of the
//!   [`AutoMultiplier`], after [`AUTO_WARMUP_FRAMES`] frames.
//! - **Shared objects**: two inputs (they alternate; lsfg reads the previous
//!   and the current one), `generated` outputs, one semaphore for the input
//!   and one per output, all exported by fd. They are made for one size; a
//!   frame of another size makes new ones (and a new lsfg context).
//! - **A cycle** is one game frame: the copy into the input signals the input
//!   semaphore; lsfg waits on it and signals one output semaphore per
//!   generated frame. The present thread waits for each output on the CPU
//!   (an empty submit that waits on the semaphore, with a timeout), presents
//!   it at its time and then presents the input (a copy of the real frame).
//!   The first cycle of a context has nothing to interpolate from: its
//!   semaphores are waited all the same, and only the real frame is shown.
//! - **One cycle at a time**: [`FrameGen::present`] waits for the previous
//!   cycle before writing an input again, so lsfg never writes an image that
//!   is still being shown.
//! - Both threads present on the [`SharedSurface`], locked per present, and
//!   submit on the device queue, which `lirtup-gpu` serializes.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use lirtup_config::{Config, FrameGenEngine, FrameGenMultiplier};
use lirtup_fg::FrameGenState;
use lirtup_fg::dll::{DllKind, find_dll};
use lirtup_fg::layer::{self, LayerSetup};
use lirtup_fg::multiplier::{AUTO_START_MULTIPLIER, AutoMultiplier};
use lirtup_fg::native::{EngineOptions, EngineStart, LsfgContext, LsfgEngine};
use lirtup_gpu::{
    Device, ExternalSemaphore, ExternalTexture, Format, PresentOptions, PresentStatus, Submission,
    SubmitOptions, Texture, TextureDesc, TextureUsage,
};
use lirtup_proto::{Event, FrameGenReason, FrameGenStatus, Stage};

use crate::pacing::{FrameInterval, FramePacer, sleep_until};
use crate::stats::{FrameGenLine, FrameKind, PresentRecord, PresentSink};
use crate::surface::SharedSurface;
use crate::{PipelineError, Result};

/// Format of the images shared with lsfg (`R8G8B8A8_UNORM`, as 1.x). The
/// screen texture is copied in as it is, so any 4-byte format works.
const SHARED_FORMAT: Format = Format::R8G8B8A8Unorm;
/// A generated frame later than this is an error (500 ms, as 1.x).
const SEMAPHORE_TIMEOUT_NS: u64 = 500_000_000;
/// A present thread slower than this with one cycle is stuck.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
/// How long [`FrameGen::close`] waits for the present thread.
const STOP_TIMEOUT: Duration = Duration::from_secs(2);
/// Waiting for the previous copy into an input (long done in practice).
const COPY_TIMEOUT_NS: u64 = 1_000_000_000;
/// Game frames measured before lsfg starts with *Auto*, so it starts with a
/// sound choice.
pub const AUTO_WARMUP_FRAMES: u32 = 15;

/// What a [`FrameGen`] needs from the session.
pub struct FrameGenSetup {
    /// The device (its textures and semaphores are exported to lsfg by fd).
    pub device: Arc<Device>,
    /// Where frames go; locked per present.
    pub surface: SharedSurface,
    /// Display refresh rate (the automatic multiplier's ceiling).
    pub refresh_hz: f32,
    /// Size of the frames (the screen texture).
    pub size: (u32, u32),
    /// Present reports.
    pub sink: PresentSink,
    /// For `Event::FrameGen` when the state or the multiplier changes.
    pub events: Sender<Event>,
}

/// The fields of the configuration lsfg is initialized with.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Settings {
    multiplier: FrameGenMultiplier,
    flow_scale: f32,
    performance: bool,
}

impl Settings {
    fn from_config(config: &Config) -> Settings {
        Settings {
            multiplier: config.frame_gen_multiplier,
            flow_scale: config.frame_gen_flow_scale,
            performance: config.frame_gen_performance,
        }
    }
}

/// Generated frames per real one for a configured multiplier (`None` when
/// it means off). *Auto* starts at ×2.
fn configured_generated(multiplier: FrameGenMultiplier) -> Option<u32> {
    match multiplier {
        FrameGenMultiplier::Auto => Some(u32::from(AUTO_START_MULTIPLIER) - 1),
        FrameGenMultiplier::Fixed(n) if n >= 2 => Some(u32::from(n.min(5)) - 1),
        FrameGenMultiplier::Fixed(_) => None,
    }
}

/// Where lsfg is.
enum Lsfg {
    /// Not started: before the first frame, while *Auto* measures, or after
    /// a change of options that came before it started.
    Idle,
    /// Initializing (or restarting) on `lsfg-restart`.
    Starting(EngineStart),
    /// Running.
    Ready(Arc<LsfgEngine>),
}

/// The images and semaphores shared with one lsfg context.
struct Shared {
    // Field order matters: the context is deleted in lsfg before the images
    // and semaphores it imported are released here.
    context: LsfgContext,
    inputs: [ExternalTexture; 2],
    outputs: Vec<ExternalTexture>,
    input_ready: ExternalSemaphore,
    outputs_ready: Vec<ExternalSemaphore>,
    size: (u32, u32),
    /// Frames pushed into this context.
    frames: u64,
}

/// What the present thread shows for one game frame.
struct Cycle {
    /// Empty for the first frame of a context.
    generated: Vec<Texture>,
    /// One per output, waited even when not shown.
    ready: Vec<ExternalSemaphore>,
    /// The input holding a copy of the real frame.
    real: Texture,
    capture: Option<Instant>,
    /// The game frame interval to spread the presents over.
    interval: Duration,
    multiplier: u8,
    /// The previous frame was not presented by the thread: start a new chain.
    new_chain: bool,
}

/// The `lsfg-present` thread.
struct PresentThread {
    cycles: Option<Sender<Cycle>>,
    /// One answer per cycle: `Err` with the reason when it failed.
    done: Receiver<std::result::Result<(), String>>,
    /// Disconnected once the thread has ended.
    exited: Receiver<()>,
    handle: Option<JoinHandle<()>>,
    /// A cycle was handed over and not answered yet.
    busy: bool,
}

impl PresentThread {
    fn spawn(device: Arc<Device>, surface: SharedSurface, sink: PresentSink) -> Result<PresentThread> {
        let (cycles_tx, cycles) = crossbeam_channel::bounded::<Cycle>(1);
        let (done_tx, done) = crossbeam_channel::unbounded();
        let (exit_tx, exited) = crossbeam_channel::bounded::<()>(0);
        let handle = std::thread::Builder::new()
            .name("lsfg-present".into())
            .spawn(move || {
                let _exit = exit_tx;
                let mut pacer = FramePacer::new();
                while let Ok(cycle) = cycles.recv() {
                    if cycle.new_chain {
                        pacer.reset();
                    }
                    let result = show(&device, &surface, &sink, &mut pacer, cycle).map_err(|e| e.to_string());
                    if done_tx.send(result).is_err() {
                        break;
                    }
                }
            })?;
        Ok(PresentThread {
            cycles: Some(cycles_tx),
            done,
            exited,
            handle: Some(handle),
            busy: false,
        })
    }

    /// Wait until the previous cycle is shown (`_drain`).
    fn drain(&mut self) -> std::result::Result<(), String> {
        if !self.busy {
            return Ok(());
        }
        match self.done.recv_timeout(DRAIN_TIMEOUT) {
            Ok(result) => {
                self.busy = false;
                result
            }
            Err(RecvTimeoutError::Timeout) => Err("the presentation thread is stuck".into()),
            Err(RecvTimeoutError::Disconnected) => {
                self.busy = false;
                Err("the presentation thread ended".into())
            }
        }
    }

    fn hand_over(&mut self, cycle: Cycle) -> std::result::Result<(), String> {
        let sent = self.cycles.as_ref().is_some_and(|tx| tx.send(cycle).is_ok());
        if !sent {
            return Err("the presentation thread ended".into());
        }
        self.busy = true;
        Ok(())
    }

    /// End the thread; a stuck one is left behind (it holds only `Arc`s).
    fn stop(mut self) {
        self.cycles = None;
        match self.exited.recv_timeout(STOP_TIMEOUT) {
            Err(RecvTimeoutError::Timeout) => {
                tracing::warn!("the frame generation presentation thread did not stop; leaving it");
            }
            _ => {
                if let Some(handle) = self.handle.take() {
                    let _ = handle.join();
                }
            }
        }
    }
}

/// Present the generated frames, then the real one, evenly spaced (`_show`).
fn show(
    device: &Arc<Device>,
    surface: &SharedSurface,
    sink: &PresentSink,
    pacer: &mut FramePacer,
    cycle: Cycle,
) -> Result<()> {
    if cycle.generated.is_empty() {
        // Nothing to show before the real frame, but lsfg signalled the
        // semaphores all the same: wait so they can be signalled again.
        for ready in &cycle.ready {
            wait_semaphore(device, ready)?;
        }
        pacer.real_presented(Instant::now());
        present_on(
            surface,
            sink,
            &cycle.real,
            cycle.capture,
            FrameKind::Real,
            cycle.multiplier,
        )?;
        return Ok(());
    }

    wait_semaphore(device, &cycle.ready[0])?;
    let times = pacer.schedule(cycle.generated.len(), cycle.interval, Instant::now());
    for (i, (frame, ready)) in cycle.generated.iter().zip(&cycle.ready).enumerate() {
        if i > 0 {
            wait_semaphore(device, ready)?;
        }
        sleep_until(times[i]);
        present_on(
            surface,
            sink,
            frame,
            cycle.capture,
            FrameKind::Generated,
            cycle.multiplier,
        )?;
    }
    sleep_until(times[cycle.generated.len()]);
    present_on(
        surface,
        sink,
        &cycle.real,
        cycle.capture,
        FrameKind::Real,
        cycle.multiplier,
    )
}

/// Present `texture` (the whole swapchain) and report it. An out-of-date
/// swapchain loses the frame: the session thread recreates it.
fn present_on(
    surface: &SharedSurface,
    sink: &PresentSink,
    texture: &Texture,
    capture: Option<Instant>,
    kind: FrameKind,
    multiplier: u8,
) -> Result<()> {
    let status = surface
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .present(texture, &PresentOptions::default())?;
    if status != PresentStatus::OutOfDate {
        sink.record(PresentRecord {
            capture,
            presented: Instant::now(),
            kind,
            multiplier,
        });
    }
    Ok(())
}

/// Block until lsfg signals `semaphore` (`wait_semaphore` of 1.x): an empty
/// submit that waits on it, and its fence.
fn wait_semaphore(device: &Arc<Device>, semaphore: &ExternalSemaphore) -> Result<()> {
    let recorder = device.recorder()?;
    let submission = device.submit(
        recorder,
        &SubmitOptions {
            wait: &[semaphore],
            ..SubmitOptions::default()
        },
    )?;
    submission.wait(SEMAPHORE_TIMEOUT_NS).map_err(|e| {
        PipelineError::other(
            Stage::FrameGen,
            format!("lsfg did not finish a generated frame in time ({e})"),
        )
    })?;
    Ok(())
}

/// The built-in engine for one session.
pub struct FrameGen {
    device: Arc<Device>,
    surface: SharedSurface,
    sink: PresentSink,
    events: Sender<Event>,
    /// `Lossless.dll`.
    dll: PathBuf,
    settings: Settings,
    /// `Some` with *Auto*.
    auto: Option<AutoMultiplier>,
    refresh_hz: f32,
    size: (u32, u32),
    /// Generated frames per real one lsfg runs with, or will start with.
    generated: u32,
    /// `Active`, `Off` (by a `ReloadConfig`) or `NoNative` (failed).
    state: FrameGenState,
    reason_code: Option<FrameGenReason>,
    reason: Option<String>,
    lsfg: Lsfg,
    shared: Option<Shared>,
    thread: Option<PresentThread>,
    /// The last copy into an input (its command buffer and the input).
    pending_copy: Option<Submission>,
    interval: FrameInterval,
    frames_seen: u32,
    capture: Option<Instant>,
    /// The last frame was presented from this thread, not by the cycles.
    chain_broken: bool,
    /// What does not change during the session (layer, DLL, env).
    base: FrameGenStatus,
    reported: Option<FrameGenStatus>,
}

impl FrameGen {
    /// `Some` when `config` asks for the built-in engine and `layer.state`
    /// allows it (lsfg is initialized lazily, on the first frame). `None`
    /// with the layer, with frame generation off, or without
    /// `VK_KHR_external_*` on the device (then `Event::FrameGen` with
    /// `NoNative` is sent).
    pub fn new(config: &Config, layer: &LayerSetup, setup: FrameGenSetup) -> Result<Option<FrameGen>> {
        if config.frame_gen_engine != FrameGenEngine::Native || layer.state != FrameGenState::Active {
            return Ok(None);
        }
        let Some(generated) = configured_generated(config.frame_gen_multiplier) else {
            return Ok(None);
        };
        let mut base = status_from_setup(config, layer);
        let dll = find_dll(DllKind::Lossless, config.frame_gen_dll.as_deref());
        let unavailable = if !setup.device.info().supports_external_fd {
            Some((
                FrameGenReason::NoExternalFd,
                "the GPU driver cannot share memory by fd (VK_KHR_external_memory_fd)",
            ))
        } else if dll.is_none() {
            Some((
                FrameGenReason::LosslessDllMissing,
                "Lossless.dll (Lossless Scaling on Steam) was not found",
            ))
        } else {
            None
        };
        if let Some((code, reason)) = unavailable {
            tracing::warn!("native frame generation is not available: {reason}");
            base.state = lirtup_proto::FrameGenState::NoNative;
            base.active = None;
            base.native_available = false;
            base.reason_code = Some(code);
            base.reason = Some(reason.to_string());
            let _ = setup.events.send(Event::FrameGen(base));
            return Ok(None);
        }
        let dll = dll.unwrap_or_default();
        base.native_available = true;
        let auto = (config.frame_gen_multiplier == FrameGenMultiplier::Auto)
            .then(|| AutoMultiplier::new(setup.refresh_hz, None));
        let mut fg = FrameGen {
            device: setup.device,
            surface: setup.surface,
            sink: setup.sink,
            events: setup.events,
            dll,
            settings: Settings::from_config(config),
            auto,
            refresh_hz: setup.refresh_hz,
            size: setup.size,
            generated,
            state: FrameGenState::Active,
            reason_code: None,
            reason: None,
            lsfg: Lsfg::Idle,
            shared: None,
            thread: None,
            pending_copy: None,
            interval: FrameInterval::new(),
            frames_seen: 0,
            capture: None,
            chain_broken: true,
            base,
            reported: None,
        };
        fg.report();
        Ok(Some(fg))
    }

    /// The capture time of the frame the next [`FrameGen::present`] shows.
    pub fn mark_capture(&mut self, capture: Instant) {
        self.capture = Some(capture);
    }

    /// Generate the frames before `texture` and hand them, with `texture`,
    /// to the present thread; or present `texture` alone when generation is
    /// off, restarting or failed. `texture` is the presenter's final screen
    /// texture: it is copied before this returns, so the presenter may
    /// render the next frame into it.
    pub fn present(&mut self, texture: &Texture) -> Result<()> {
        let now = Instant::now();
        let capture = self.capture.take();
        self.interval.observe_frame(now);
        self.frames_seen = self.frames_seen.saturating_add(1);
        if self.state == FrameGenState::Active {
            match self.try_generate(texture, capture, now) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(e) => self.fail(e.to_string()),
            }
        }
        self.present_direct(texture, capture)
    }

    /// Generating now (the session thread presents only new frames then:
    /// `present_unchanged` is skipped, as in 1.x).
    ///
    /// True also while lsfg starts or restarts: the frame interval is
    /// measured from the frames presented here, so they must stay new ones.
    pub fn is_active(&self) -> bool {
        self.state == FrameGenState::Active
    }

    /// The multiplier in effect (`None` when not generating, also while
    /// lsfg starts or restarts).
    pub fn active_multiplier(&self) -> Option<u8> {
        (self.state == FrameGenState::Active && matches!(self.lsfg, Lsfg::Ready(_)))
            .then(|| self.multiplier())
    }

    /// The monitor changed.
    pub fn set_refresh_hz(&mut self, hz: f32) {
        if hz > 0.0 {
            self.refresh_hz = hz;
            if let Some(auto) = &mut self.auto {
                auto.set_refresh(hz);
            }
        }
    }

    /// The screen texture changed size: drop the lsfg context; the next
    /// frame creates one at the new size.
    pub fn resize(&mut self, size: (u32, u32)) {
        if size == self.size {
            return;
        }
        self.size = size;
        if let Err(reason) = self.drain() {
            self.fail(reason);
            return;
        }
        self.release_shared();
    }

    /// New multiplier, flow scale or performance mode (`ReloadConfig`). A
    /// different number of generated frames restarts lsfg in the background.
    ///
    /// Turning frame generation off (or switching to the layer) stops lsfg
    /// for the session; turning it back on starts it again. After a failure
    /// (`NoNative`) it stays off until the next session.
    pub fn update_config(&mut self, config: &Config) {
        self.base.engine = config.frame_gen_engine;
        self.base.configured = config.frame_gen_multiplier;
        let settings = Settings::from_config(config);
        let wanted = configured_generated(config.frame_gen_multiplier)
            .filter(|_| config.frame_gen_engine == FrameGenEngine::Native);
        if self.state == FrameGenState::NoNative {
            self.settings = settings;
            self.report();
            return;
        }
        let Some(wanted) = wanted else {
            if self.state == FrameGenState::Active {
                tracing::info!("native frame generation turned off");
                self.shutdown();
                self.state = FrameGenState::Off;
            }
            self.settings = settings;
            self.report();
            return;
        };
        let auto = settings.multiplier == FrameGenMultiplier::Auto;
        if self.state == FrameGenState::Off {
            self.state = FrameGenState::Active;
            self.settings = settings;
            self.auto = auto.then(|| AutoMultiplier::new(self.refresh_hz, None));
            self.generated = wanted;
            self.frames_seen = 0;
            self.report();
            return;
        }
        if settings == self.settings {
            return;
        }
        let previous = std::mem::replace(&mut self.settings, settings);
        let generated = if auto {
            // Keep running at the current multiplier; Auto moves from there.
            let current = self.multiplier();
            self.auto
                .get_or_insert_with(|| AutoMultiplier::new(self.refresh_hz, Some(current)));
            self.generated
        } else {
            self.auto = None;
            wanted
        };
        let same_lsfg = generated == self.generated
            && previous.flow_scale == settings.flow_scale
            && previous.performance == settings.performance;
        if same_lsfg {
            self.report();
            return;
        }
        if let Err(e) = self
            .drain()
            .map_err(reason_error)
            .and_then(|()| self.restart(generated))
        {
            self.fail(e.to_string());
            return;
        }
        self.report();
    }

    /// The «Frame gen» row of the stats panel, as 1.x `_stats_rows`
    /// (`None` when off).
    pub fn panel_line(&self) -> Option<FrameGenLine> {
        let m = self.multiplier();
        let mode = if self.auto.is_some() {
            format!("auto x{m}")
        } else {
            format!("x{m}")
        };
        match self.state {
            FrameGenState::NoNative => Some(FrameGenLine {
                text: format!("{mode} built-in failed, off"),
                warning: true,
                ..FrameGenLine::default()
            }),
            FrameGenState::Active => Some(match &self.lsfg {
                Lsfg::Ready(engine) => FrameGenLine {
                    text: format!("{mode} built-in {}", if engine.fp16() { "FP16" } else { "FP32" }),
                    multiplier: Some(m),
                    suffix: format!(
                        ", pacing {:.1} ms",
                        self.interval.interval().as_secs_f64() * 1000.0
                    ),
                    ..FrameGenLine::default()
                },
                _ => FrameGenLine {
                    text: format!("{mode} built-in, restarting lsfg"),
                    ..FrameGenLine::default()
                },
            }),
            _ => None,
        }
    }

    /// For `EngineSnapshot.framegen` and `Event::FrameGen`.
    pub fn status(&self) -> FrameGenStatus {
        let active = self.state == FrameGenState::Active;
        FrameGenStatus {
            state: to_proto(self.state),
            active: active.then(|| self.multiplier()),
            reason_code: self.reason_code,
            reason: self.reason.clone(),
            ..self.base.clone()
        }
    }

    /// Stop the present thread and release lsfg and the shared objects.
    pub fn close(mut self) {
        self.shutdown();
    }

    // ------------------------------------------------------------------

    fn multiplier(&self) -> u8 {
        u8::try_from(self.generated + 1).unwrap_or(u8::MAX)
    }

    /// The generation path of [`FrameGen::present`]: `true` when the cycle
    /// was handed to the present thread.
    fn try_generate(&mut self, texture: &Texture, capture: Option<Instant>, now: Instant) -> Result<bool> {
        self.drain().map_err(reason_error)?;
        if self.auto.is_some() {
            self.update_auto(now)?;
        }
        if !self.poll_lsfg()? {
            return Ok(false);
        }
        let cycle = self.generate(texture, capture)?;
        if self.thread.is_none() {
            self.thread = Some(PresentThread::spawn(
                self.device.clone(),
                self.surface.clone(),
                self.sink.clone(),
            )?);
        }
        if let Some(thread) = &mut self.thread {
            thread.hand_over(cycle).map_err(reason_error)?;
        }
        Ok(true)
    }

    /// Wait for the present thread's previous cycle.
    fn drain(&mut self) -> std::result::Result<(), String> {
        match &mut self.thread {
            Some(thread) => thread.drain(),
            None => Ok(()),
        }
    }

    /// Follow the automatic multiplier (`_update_auto`): pick the first
    /// multiplier after the warm-up, then restart lsfg when the choice
    /// changes. Does nothing while lsfg starts.
    fn update_auto(&mut self, now: Instant) -> Result<()> {
        if matches!(self.lsfg, Lsfg::Starting(_)) {
            return Ok(());
        }
        let fps = self.interval.fps();
        let warming_up = self.frames_seen <= AUTO_WARMUP_FRAMES;
        let Some(auto) = &mut self.auto else {
            return Ok(());
        };
        if auto.chosen().is_none() && warming_up {
            return Ok(());
        }
        let generated = u32::from(auto.update(fps, now)) - 1;
        if generated == self.generated {
            return Ok(());
        }
        if matches!(self.lsfg, Lsfg::Idle) {
            self.generated = generated;
        } else {
            self.restart(generated)?;
            self.report();
        }
        Ok(())
    }

    /// Start lsfg when it can start, collect it once started. `true` when
    /// it runs.
    fn poll_lsfg(&mut self) -> Result<bool> {
        match &mut self.lsfg {
            Lsfg::Ready(_) => Ok(true),
            Lsfg::Starting(start) => match start.poll() {
                None => Ok(false),
                Some(Ok(engine)) => {
                    let options = engine.options();
                    tracing::info!(
                        "native frame generation x{}{} on {} ({} shaders, flow {:.2}{})",
                        options.generated + 1,
                        if self.auto.is_some() { " (auto)" } else { "" },
                        self.device.info().name,
                        if engine.fp16() { "FP16" } else { "FP32" },
                        options.flow_scale,
                        if options.performance { ", performance" } else { "" },
                    );
                    self.lsfg = Lsfg::Ready(engine);
                    Ok(true)
                }
                Some(Err(e)) => Err(PipelineError::other(
                    Stage::FrameGen,
                    format!("starting lsfg: {e}"),
                )),
            },
            Lsfg::Idle => {
                let waiting = self.auto.as_ref().is_some_and(|a| a.chosen().is_none());
                if !waiting {
                    self.lsfg =
                        Lsfg::Starting(EngineStart::spawn(None, self.engine_options(self.generated))?);
                }
                Ok(false)
            }
        }
    }

    /// Restart lsfg for `generated` frames (and the current settings) in the
    /// background (`_begin_restart`). The present thread must be idle.
    fn restart(&mut self, generated: u32) -> Result<()> {
        self.release_shared();
        self.generated = generated;
        let previous = match std::mem::replace(&mut self.lsfg, Lsfg::Idle) {
            Lsfg::Idle => return Ok(()),
            Lsfg::Starting(start) => start.wait().ok(),
            Lsfg::Ready(engine) => Some(engine),
        };
        tracing::info!("restarting lsfg for x{}", generated + 1);
        self.lsfg = Lsfg::Starting(EngineStart::spawn(previous, self.engine_options(generated))?);
        Ok(())
    }

    fn engine_options(&self, generated: u32) -> EngineOptions {
        let info = self.device.info();
        EngineOptions {
            dll: self.dll.clone(),
            vendor_id: info.vendor_id,
            device_id: info.device_id,
            generated,
            flow_scale: self.settings.flow_scale.clamp(0.25, 1.0),
            performance: self.settings.performance,
            hdr: false,
            fp16: info.supports_fp16,
        }
    }

    /// Copy `texture` into the next input and start lsfg on it (`_generate`;
    /// the present thread is idle and lsfg is ready).
    fn generate(&mut self, texture: &Texture, capture: Option<Instant>) -> Result<Cycle> {
        let size = (texture.width(), texture.height());
        if self.shared.as_ref().is_none_or(|s| s.size != size) {
            self.release_shared();
            self.shared = Some(self.create_shared(size)?);
        }
        // The previous copy is done (it is long done: the present thread
        // already showed the frame it copied).
        if let Some(copy) = self.pending_copy.take() {
            copy.wait(COPY_TIMEOUT_NS)?;
        }
        let Some(shared) = self.shared.as_mut() else {
            return Err(PipelineError::other(Stage::FrameGen, "no lsfg context"));
        };
        let real = shared.inputs[(shared.frames % 2) as usize].texture().clone();
        let mut recorder = self.device.recorder()?;
        recorder.copy_texture(texture, &real, &[]);
        self.pending_copy = Some(self.device.submit(
            recorder,
            &SubmitOptions {
                signal: &[&shared.input_ready],
                ..SubmitOptions::default()
            },
        )?);
        let outputs_ready = shared
            .outputs_ready
            .iter()
            .map(ExternalSemaphore::export_fd)
            .collect::<lirtup_gpu::Result<Vec<_>>>()?;
        shared
            .context
            .present(shared.input_ready.export_fd()?, outputs_ready)?;
        let first = shared.frames == 0;
        shared.frames += 1;
        Ok(Cycle {
            // The first frame has nothing before it to interpolate from.
            generated: if first {
                Vec::new()
            } else {
                shared.outputs.iter().map(|t| t.texture().clone()).collect()
            },
            ready: shared.outputs_ready.clone(),
            real,
            capture,
            interval: self.interval.interval(),
            multiplier: u8::try_from(self.generated + 1).unwrap_or(u8::MAX),
            new_chain: std::mem::take(&mut self.chain_broken),
        })
    }

    /// New shared images and semaphores, and an lsfg context on them
    /// (`_create_context`).
    fn create_shared(&self, (width, height): (u32, u32)) -> Result<Shared> {
        let Lsfg::Ready(engine) = &self.lsfg else {
            return Err(PipelineError::other(Stage::FrameGen, "lsfg is not running"));
        };
        let device = &self.device;
        let texture = |label: String| {
            device.create_external_texture(&TextureDesc {
                width,
                height,
                format: SHARED_FORMAT,
                slices: 1,
                force_array_view: false,
                usage: TextureUsage::default(),
                label,
            })
        };
        let inputs = [texture("lsfg input 0".into())?, texture("lsfg input 1".into())?];
        let outputs = (0..self.generated)
            .map(|i| texture(format!("lsfg output {i}")))
            .collect::<lirtup_gpu::Result<Vec<_>>>()?;
        let input_ready = device.create_external_semaphore()?;
        let outputs_ready = (0..self.generated)
            .map(|_| device.create_external_semaphore())
            .collect::<lirtup_gpu::Result<Vec<_>>>()?;
        let output_fds = outputs
            .iter()
            .map(ExternalTexture::export_fd)
            .collect::<lirtup_gpu::Result<Vec<_>>>()?;
        let context = engine.create_context(
            [inputs[0].export_fd()?, inputs[1].export_fd()?],
            output_fds,
            width,
            height,
            SHARED_FORMAT.to_vk().as_raw() as u32,
        )?;
        tracing::debug!("native frame generation context {width}x{height}");
        Ok(Shared {
            context,
            inputs,
            outputs,
            input_ready,
            outputs_ready,
            size: (width, height),
            frames: 0,
        })
    }

    /// Drop the lsfg context and the shared objects (the present thread is
    /// idle). The next generated frame starts a new chain.
    fn release_shared(&mut self) {
        self.shared = None;
        self.chain_broken = true;
    }

    /// Present `texture` from the session thread, real frames only.
    fn present_direct(&mut self, texture: &Texture, capture: Option<Instant>) -> Result<()> {
        self.chain_broken = true;
        present_on(&self.surface, &self.sink, texture, capture, FrameKind::Real, 1)
    }

    /// Turn frame generation off for the session (`_disable`).
    fn fail(&mut self, reason: String) {
        tracing::error!("native frame generation failed, turning it off: {reason}");
        self.shutdown();
        self.state = FrameGenState::NoNative;
        self.reason_code = Some(FrameGenReason::EngineFailed);
        self.reason = Some(reason);
        self.report();
    }

    /// Stop the present thread, release the shared objects and lsfg.
    fn shutdown(&mut self) {
        if let Some(thread) = self.thread.take() {
            thread.stop();
        }
        self.pending_copy = None;
        self.release_shared();
        match std::mem::replace(&mut self.lsfg, Lsfg::Idle) {
            // lsfg cannot be finalized while it initializes: wait for it.
            Lsfg::Starting(start) => drop(start.wait()),
            Lsfg::Ready(engine) => drop(engine),
            Lsfg::Idle => {}
        }
    }

    /// Send `Event::FrameGen` when the status changed.
    fn report(&mut self) {
        let status = self.status();
        if self.reported.as_ref() != Some(&status) {
            let _ = self.events.send(Event::FrameGen(status.clone()));
            self.reported = Some(status);
        }
    }
}

impl Drop for FrameGen {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// A frame generation failure described by text.
fn reason_error(reason: String) -> PipelineError {
    PipelineError::other(Stage::FrameGen, reason)
}

/// The frame generation status before any session (or between sessions),
/// from what `lirtup_fg::layer::prepare` did at startup: what
/// `lirtup-core` puts in `EngineSnapshot.framegen` and answers
/// `Command::FrameGenProbe` with.
///
/// `native_available` here only means `Lossless.dll` was found: whether the
/// GPU shares memory by fd is known once a session opens the device (a
/// [`FrameGen`] then reports `NoNative` if it does not).
pub fn status_from_setup(config: &Config, layer: &LayerSetup) -> FrameGenStatus {
    let engine = config.frame_gen_engine;
    let hint = config.frame_gen_dll.as_deref();
    let lossless = find_dll(DllKind::Lossless, hint).is_some();
    let dll_found = match engine {
        FrameGenEngine::Native => lossless,
        FrameGenEngine::Layer => find_dll(DllKind::LsfgVk, hint).is_some(),
    };
    FrameGenStatus {
        engine,
        configured: config.frame_gen_multiplier,
        state: to_proto(layer.state),
        active: (layer.state == FrameGenState::Active).then_some(layer.multiplier),
        layer_installed: layer::find_layer().is_some(),
        dll_found,
        native_available: lossless,
        env_locked: layer.state == FrameGenState::External || layer::external_env(),
        reason_code: reason_code(layer.state, engine),
        reason: reason_for(layer.state, engine),
    }
}

/// `lirtup_fg::FrameGenState` as the protocol spells it.
fn to_proto(state: FrameGenState) -> lirtup_proto::FrameGenState {
    use lirtup_proto::FrameGenState as P;
    match state {
        FrameGenState::Off => P::Off,
        FrameGenState::Active => P::Active,
        FrameGenState::NoLayer => P::NoLayer,
        FrameGenState::NoDll => P::NoDll,
        FrameGenState::External => P::External,
        FrameGenState::NoNative => P::NoNative,
    }
}

/// The code of [`reason_for`].
fn reason_code(state: FrameGenState, engine: FrameGenEngine) -> Option<FrameGenReason> {
    Some(match state {
        FrameGenState::Off | FrameGenState::Active => return None,
        FrameGenState::NoLayer => FrameGenReason::LayerMissing,
        FrameGenState::NoDll => match engine {
            FrameGenEngine::Native => FrameGenReason::LosslessDllMissing,
            FrameGenEngine::Layer => FrameGenReason::LsfgVkDllMissing,
        },
        FrameGenState::External => FrameGenReason::EnvLocked,
        // `layer::prepare` never says it; `FrameGen` does, with its own code.
        FrameGenState::NoNative => FrameGenReason::EngineFailed,
    })
}

/// Why frame generation is not running, for the states that need a reason.
fn reason_for(state: FrameGenState, engine: FrameGenEngine) -> Option<String> {
    let reason = match state {
        FrameGenState::Off | FrameGenState::Active => return None,
        FrameGenState::NoLayer => "the lsfg-vk layer is not installed",
        FrameGenState::NoDll => match engine {
            FrameGenEngine::Native => "Lossless.dll (Lossless Scaling on Steam) was not found",
            FrameGenEngine::Layer => {
                "lsfg-vk.dll (the lsfg-vk branch of Lossless Scaling on Steam) was not found"
            }
        },
        FrameGenState::External => "configured by LSFGVK_* environment variables",
        FrameGenState::NoNative => "the native engine is not available",
    };
    Some(reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_multipliers() {
        assert_eq!(configured_generated(FrameGenMultiplier::Fixed(0)), None);
        assert_eq!(configured_generated(FrameGenMultiplier::Fixed(1)), None);
        assert_eq!(configured_generated(FrameGenMultiplier::Fixed(2)), Some(1));
        assert_eq!(configured_generated(FrameGenMultiplier::Fixed(4)), Some(3));
        assert_eq!(configured_generated(FrameGenMultiplier::Fixed(9)), Some(4));
        assert_eq!(
            configured_generated(FrameGenMultiplier::Auto),
            Some(u32::from(AUTO_START_MULTIPLIER) - 1)
        );
    }

    #[test]
    fn states_map_to_the_protocol() {
        use lirtup_proto::FrameGenState as P;
        let pairs = [
            (FrameGenState::Off, P::Off),
            (FrameGenState::Active, P::Active),
            (FrameGenState::NoLayer, P::NoLayer),
            (FrameGenState::NoDll, P::NoDll),
            (FrameGenState::External, P::External),
            (FrameGenState::NoNative, P::NoNative),
        ];
        for (fg, proto) in pairs {
            assert_eq!(to_proto(fg), proto);
        }
    }

    #[test]
    fn reasons_only_when_not_running() {
        assert_eq!(reason_for(FrameGenState::Off, FrameGenEngine::Native), None);
        assert_eq!(reason_for(FrameGenState::Active, FrameGenEngine::Layer), None);
        assert!(
            reason_for(FrameGenState::NoDll, FrameGenEngine::Native)
                .unwrap()
                .contains("Lossless.dll")
        );
        assert!(
            reason_for(FrameGenState::NoDll, FrameGenEngine::Layer)
                .unwrap()
                .contains("lsfg-vk.dll")
        );
        assert_eq!(reason_code(FrameGenState::Active, FrameGenEngine::Native), None);
        assert_eq!(
            reason_code(FrameGenState::NoDll, FrameGenEngine::Layer),
            Some(FrameGenReason::LsfgVkDllMissing)
        );
        assert_eq!(
            reason_code(FrameGenState::External, FrameGenEngine::Native),
            Some(FrameGenReason::EnvLocked)
        );
    }

    #[test]
    fn status_from_setup_reports_the_layer_setup() {
        let config = Config {
            frame_gen_engine: FrameGenEngine::Layer,
            frame_gen_multiplier: FrameGenMultiplier::Fixed(3),
            ..Config::default()
        };
        let active = LayerSetup {
            state: FrameGenState::Active,
            multiplier: 3,
            toml: None,
        };
        let status = status_from_setup(&config, &active);
        assert_eq!(status.engine, FrameGenEngine::Layer);
        assert_eq!(status.configured, FrameGenMultiplier::Fixed(3));
        assert_eq!(status.state, lirtup_proto::FrameGenState::Active);
        assert_eq!(status.active, Some(3));
        assert_eq!(status.reason, None);

        let off = LayerSetup {
            state: FrameGenState::Off,
            multiplier: 1,
            toml: None,
        };
        let status = status_from_setup(&Config::default(), &off);
        assert_eq!(status.state, lirtup_proto::FrameGenState::Off);
        assert_eq!(status.active, None);

        let external = LayerSetup {
            state: FrameGenState::External,
            multiplier: 2,
            toml: None,
        };
        let status = status_from_setup(&config, &external);
        assert!(status.env_locked);
        assert!(status.reason.is_some());
    }

    #[test]
    fn settings_compare_by_value() {
        let config = Config {
            frame_gen_multiplier: FrameGenMultiplier::Auto,
            frame_gen_flow_scale: 0.5,
            ..Config::default()
        };
        let a = Settings::from_config(&config);
        assert_eq!(a, Settings::from_config(&config.clone()));
        let b = Settings::from_config(&Config {
            frame_gen_performance: !config.frame_gen_performance,
            ..config
        });
        assert_ne!(a, b);
    }

    /// Criterion of 5.8 (the part without a window): the built-in engine
    /// with real fds of `lirtup-gpu`. lsfg imports two inputs, an output and
    /// the semaphores, generates a frame between a red and a blue one, and
    /// the validation layer stays quiet. Needs a GPU with external fd
    /// support and `Lossless.dll` (Steam, or `LIRTUP_LOSSLESS_DLL`).
    #[test]
    #[ignore = "needs a GPU and Lossless.dll"]
    fn lsfg_generates_with_lirtup_gpu_fds() {
        use lirtup_gpu::{Instance, InstanceOptions};

        let dll = std::env::var_os("LIRTUP_LOSSLESS_DLL")
            .map(PathBuf::from)
            .or_else(|| find_dll(DllKind::Lossless, None))
            .expect("Lossless.dll not found");
        let instance = Instance::new(&InstanceOptions {
            app_name: "lirtup-framegen-test".into(),
            debug: true,
        })
        .unwrap();
        let adapter = instance.best_adapter().expect("no GPU");
        let device = instance.create_device(adapter).unwrap();
        let info = device.info().clone();
        assert!(info.supports_external_fd, "{} cannot share by fd", info.name);

        let options = EngineOptions::from_config(
            &Config::default(),
            dll,
            info.vendor_id,
            info.device_id,
            1,
            info.supports_fp16,
        );
        let engine = EngineStart::spawn(None, options).unwrap().wait().unwrap();

        let (width, height) = (256, 128);
        let desc = |label: &str| TextureDesc {
            width,
            height,
            format: SHARED_FORMAT,
            slices: 1,
            force_array_view: false,
            usage: TextureUsage::default(),
            label: label.into(),
        };
        let inputs = [
            device.create_external_texture(&desc("in 0")).unwrap(),
            device.create_external_texture(&desc("in 1")).unwrap(),
        ];
        let output = device.create_external_texture(&desc("out")).unwrap();
        let input_ready = device.create_external_semaphore().unwrap();
        let output_ready = device.create_external_semaphore().unwrap();
        let mut context = engine
            .create_context(
                [inputs[0].export_fd().unwrap(), inputs[1].export_fd().unwrap()],
                vec![output.export_fd().unwrap()],
                width,
                height,
                SHARED_FORMAT.to_vk().as_raw() as u32,
            )
            .unwrap();

        for (frame, color) in [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]]
            .into_iter()
            .enumerate()
        {
            let mut recorder = device.recorder().unwrap();
            recorder.clear(inputs[frame % 2].texture(), color);
            device
                .submit(
                    recorder,
                    &SubmitOptions {
                        signal: &[&input_ready],
                        ..SubmitOptions::default()
                    },
                )
                .unwrap()
                .wait(COPY_TIMEOUT_NS)
                .unwrap();
            context
                .present(
                    input_ready.export_fd().unwrap(),
                    vec![output_ready.export_fd().unwrap()],
                )
                .unwrap();
            wait_semaphore(&device, &output_ready).unwrap();
        }

        let pixels = output.texture().download().unwrap();
        let center = ((height / 2 * width + width / 2) * 4) as usize;
        let rgba = &pixels[center..center + 4];
        eprintln!("generated center pixel: {rgba:?}");
        assert!(
            u32::from(rgba[0]) + u32::from(rgba[2]) > 64,
            "the generated frame is black: {rgba:?}"
        );
        drop(context);
        drop(engine);
        let messages = device.debug_messages();
        assert!(messages.is_empty(), "validation: {messages:#?}");
    }
}
