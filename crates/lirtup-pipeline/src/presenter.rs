//! Effect chain and presentation (`presenter.py`). Front P, round 2.
//!
//! The presenter takes the upscaled texture and runs, in one recording:
//! MagpieFX chain or native effects (the `lirtup-effects` passes, in the 1.x
//! order), the scaler into the screen texture (`output_size`, letterbox,
//! zoom and offset), `Delinearize` or `Dither`, then the extra passes the
//! session thread gives it (the OSD and the stats panel of front S, which
//! blend onto [`Presenter::screen`]). Then it presents the screen texture:
//! through the [`FrameGen`] of front F when there is one, else straight to
//! the [`SharedSurface`] of front K.
//!
//! Signatures used by other fronts: [`Presenter::screen`] (S rebinds its
//! blends to it), [`PresentOutcome`] (the session thread), the
//! `framegen`/`surface` arguments of [`Presenter::new`]. The rest is front
//! P's to reshape.
//!
//! **CPU/GPU races of `lirtup-effects`** (`set_params`, `set_frame`,
//! `set_dst_rect`, `OverlayBlend::set_image` write `Upload` buffers): the
//! presenter calls them only after the previous frame's submission has
//! finished (the wait at the top of every frame, as 1.x
//! `wait_for_last_present`), so no per-frame ring is needed. If profiling
//! shows that wait costs a frame of latency, the ring goes in round 4.

use std::sync::Arc;

use lirtup_config::Config;
use lirtup_effects::Pass;
use lirtup_gpu::{Device, Texture, Timestamps};
use lirtup_x11::{CoordinateMapper, RgbaImage};

use crate::Result;
use crate::framegen::FrameGen;
use crate::surface::SharedSurface;

/// What a present did.
#[derive(Debug, Clone, PartialEq)]
pub struct PresentOutcome {
    /// GPU time per pass of this frame (`None` when timestamps were not
    /// asked for: stats panel hidden and no GUI listening).
    pub timestamps: Option<Timestamps>,
    /// The swapchain must be recreated (out of date or suboptimal): the
    /// session thread calls [`Presenter::resize`] with the overlay size.
    pub recreate: bool,
}

/// The effect chain and the screen texture of one session.
pub struct Presenter {
    _private: (),
}

impl Presenter {
    /// Build the chain for `config` (compiling MagpieFX effects, blocking)
    /// for a source of `src_size`, presenting to `surface` (or through
    /// `framegen`).
    pub fn new(
        device: &Arc<Device>,
        config: &Config,
        src_size: (u32, u32),
        surface: SharedSurface,
        framegen: Option<FrameGen>,
    ) -> Result<Presenter> {
        todo!("front P")
    }

    /// The final 8-bit screen texture (swapchain size). The OSD and the
    /// stats panel blend onto it; it changes on [`Presenter::resize`].
    pub fn screen(&self) -> &Texture {
        todo!("front P")
    }

    /// The upscaled frame to present next.
    pub fn set_source(&mut self, texture: &Texture) {
        todo!("front P")
    }

    /// Run the chain, then the `overlays` (in order), and present. With
    /// frame generation, the [`FrameGen`] presents (and paces) instead.
    /// `timestamps`: time every pass on the GPU.
    pub fn present(&mut self, overlays: &[&dyn Pass], timestamps: bool) -> Result<PresentOutcome> {
        todo!("front P")
    }

    /// Present the last screen texture again (`present_on_change_only:
    /// false` and nothing changed). Not called while frame generation is
    /// active.
    pub fn present_unchanged(&mut self) -> Result<PresentOutcome> {
        todo!("front P")
    }

    /// The swapchain (overlay) has a new size: recreate it, the screen
    /// texture and the scaler output. The caller then rebinds the OSD and
    /// the panel to [`Presenter::screen`].
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        todo!("front P")
    }

    /// New effect parameters, chain or scaler (`ReloadConfig`, `CycleModel`
    /// does not come here: that is the upscaler). Rebuilds only what
    /// changed.
    pub fn update_config(&mut self, config: &Config) -> Result<()> {
        todo!("front P")
    }

    /// Zoom (1.0 = fit) and offset in output pixels (`ZoomIn`,
    /// `OffsetLeft`, `RestoreView`, ...).
    pub fn set_view(&mut self, zoom: f32, offset: (i32, i32)) {
        todo!("front P")
    }

    /// Where the content is drawn in the overlay, for the input mapping
    /// (`OverlayCommand::SetMapper`). `crop_offset` and `source_size` are
    /// filled by the session thread.
    pub fn mapper(&self) -> CoordinateMapper {
        todo!("front P")
    }

    /// Labels of the chain, in order (`SessionView.chain`, the stats panel).
    pub fn chain(&self) -> Vec<String> {
        todo!("front P")
    }

    /// The frame generation engine, if any (status, multiplier).
    pub fn framegen(&self) -> Option<&FrameGen> {
        todo!("front P")
    }

    /// Mutable access, for `update_config` and `set_refresh_hz`.
    pub fn framegen_mut(&mut self) -> Option<&mut FrameGen> {
        todo!("front P")
    }

    /// Read back the screen texture (`Screenshot`): RGBA8, swapchain size.
    pub fn screenshot(&self) -> Result<RgbaImage> {
        todo!("front P")
    }

    /// Release everything (closes the [`FrameGen`] first).
    pub fn close(self) {
        todo!("front P")
    }
}
