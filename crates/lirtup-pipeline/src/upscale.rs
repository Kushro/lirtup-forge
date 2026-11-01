//! Model selection, double upscale, tiles or full frame (`upscale.py`).
//! Front P, round 2.
//!
//! Only the session thread uses it; front P may reshape it freely. The
//! skeleton keeps the 1.x split: an [`Upscaler`] uploads the captured frame
//! and runs SRCNN (one or two passes for `double_upscale`), in full-frame
//! mode or, with `use_tile_processing`, on the dirty tiles only
//! ([`crate::tiles`]), falling back to full frame when too many tiles are
//! dirty. With `model: none` it only uploads (the `PassthroughUpscaler`).

use std::sync::Arc;

use lirtup_config::{Config, Model};
use lirtup_gpu::{Device, Texture};
use lirtup_x11::DamageRect;

use crate::Result;

/// What one upscale did (for the stats).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpscaleOutcome {
    /// Fraction of the frame that was processed (1.0 in full-frame mode).
    pub dirty_fraction: f32,
    /// Tiles processed (`None` in full-frame mode).
    pub tiles: Option<u32>,
}

/// SRCNN (or a plain upload) for one source size.
pub struct Upscaler {
    _private: (),
}

impl Upscaler {
    /// For a captured frame of `src_size` (after crop), with the model,
    /// `double_upscale`, tile settings and damage settings of `config`.
    pub fn new(device: &Arc<Device>, config: &Config, src_size: (u32, u32)) -> Result<Upscaler> {
        todo!("front P")
    }

    /// Upload the captured frame (BGRA, `stride` bytes per row; only the
    /// `damage` rectangles when `full` is false) and record the upscale.
    /// Submits its own work (the presenter's recording waits on it).
    pub fn process(
        &mut self,
        pixels: &[u8],
        stride: u32,
        full: bool,
        damage: &[DamageRect],
    ) -> Result<UpscaleOutcome> {
        todo!("front P")
    }

    /// The upscaled frame (the presenter's source).
    pub fn output(&self) -> &Texture {
        todo!("front P")
    }

    /// Scale factor (1, 2 or 4).
    pub fn scale(&self) -> u32 {
        todo!("front P")
    }

    /// The model in use.
    pub fn model(&self) -> Model {
        todo!("front P")
    }
}
