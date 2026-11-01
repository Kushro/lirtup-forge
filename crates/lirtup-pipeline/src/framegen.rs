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

use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::Sender;
use lirtup_config::Config;
use lirtup_fg::layer::LayerSetup;
use lirtup_gpu::{Device, Texture};
use lirtup_proto::{Event, FrameGenStatus};

use crate::Result;
use crate::stats::PresentSink;
use crate::surface::SharedSurface;

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

/// The built-in engine for one session.
pub struct FrameGen {
    _private: (),
}

impl FrameGen {
    /// `Some` when `config` asks for the built-in engine and `layer.state`
    /// allows it (lsfg is initialized lazily, on the first frame). `None`
    /// with the layer, with frame generation off, or without
    /// `VK_KHR_external_*` on the device (then `Event::FrameGen` with
    /// `NoNative` is sent).
    pub fn new(config: &Config, layer: &LayerSetup, setup: FrameGenSetup) -> Result<Option<FrameGen>> {
        todo!("front F")
    }

    /// The capture time of the frame the next [`FrameGen::present`] shows.
    pub fn mark_capture(&mut self, capture: Instant) {
        todo!("front F")
    }

    /// Generate the frames before `texture` and hand them, with `texture`,
    /// to the present thread; or present `texture` alone when generation is
    /// off, restarting or failed. `texture` is the presenter's final screen
    /// texture: it is copied before this returns, so the presenter may
    /// render the next frame into it.
    pub fn present(&mut self, texture: &Texture) -> Result<()> {
        todo!("front F")
    }

    /// Generating now (the session thread presents only new frames then:
    /// `present_unchanged` is skipped, as in 1.x).
    pub fn is_active(&self) -> bool {
        todo!("front F")
    }

    /// The multiplier in effect (`None` when not generating).
    pub fn active_multiplier(&self) -> Option<u8> {
        todo!("front F")
    }

    /// The monitor changed.
    pub fn set_refresh_hz(&mut self, hz: f32) {
        todo!("front F")
    }

    /// The screen texture changed size: drop the lsfg context; the next
    /// frame creates one at the new size.
    pub fn resize(&mut self, size: (u32, u32)) {
        todo!("front F")
    }

    /// New multiplier, flow scale or performance mode (`ReloadConfig`). A
    /// different number of generated frames restarts lsfg in the background.
    pub fn update_config(&mut self, config: &Config) {
        todo!("front F")
    }

    /// For `EngineSnapshot.framegen` and `Event::FrameGen`.
    pub fn status(&self) -> FrameGenStatus {
        todo!("front F")
    }

    /// Stop the present thread and release lsfg and the shared objects.
    pub fn close(self) {
        todo!("front F")
    }
}

/// The frame generation status before any session (or between sessions),
/// from what `lirtup_fg::layer::prepare` did at startup: what
/// `lirtup-core` puts in `EngineSnapshot.framegen` and answers
/// `Command::FrameGenProbe` with.
pub fn status_from_setup(config: &Config, layer: &LayerSetup) -> FrameGenStatus {
    todo!("front F")
}
