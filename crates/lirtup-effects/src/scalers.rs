//! Final resamplers (`shaders/scalers/`): copy, Catmull-Rom, Lanczos, FSR,
//! NIS.

use std::sync::Arc;

use lirtup_gpu::{CommandRecorder, Device, Texture};

use crate::{Pass, Result};

/// Which resampler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalerKind {
    /// 1:1 copy (same size).
    Copy,
    /// Catmull-Rom (downsampling).
    CatmullRom,
    /// Lanczos (up or adaptive down).
    Lanczos,
    /// FSR 1.0 EASU + RCAS (upsampling).
    Fsr,
    /// NVIDIA Image Scaling (upsampling).
    Nis,
}

/// Resampler parameters (from `ModeSettings`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalerParams {
    /// Kernel width (`blur`).
    pub blur: f32,
    /// `antiring_strength`.
    pub antiring_strength: f32,
    /// `tight_antiring`.
    pub tight_antiring: bool,
    /// `kernel_radius` (`None` = automatic).
    pub kernel_radius: Option<u32>,
}

/// A resampler from the input size to `output_size`, with the content placed
/// at `dst_rect` inside the output (letterbox, offset and zoom are done here).
pub struct Scaler {
    _private: (),
}

impl Scaler {
    /// Create. `dst_rect` is `(x, y, width, height)` inside `output_size`.
    pub fn new(
        device: &Arc<Device>,
        kind: ScalerKind,
        input: &Texture,
        output_size: (u32, u32),
        dst_rect: (i32, i32, u32, u32),
        params: &ScalerParams,
    ) -> Result<Scaler> {
        todo!("front E")
    }

    /// Move the content (offset and zoom hotkeys) without recreating.
    pub fn set_dst_rect(&mut self, dst_rect: (i32, i32, u32, u32)) {
        todo!("front E")
    }

    /// Letterbox color, RGBA.
    pub fn set_background(&mut self, rgba: [f32; 4]) {
        todo!("front E")
    }
}

impl Pass for Scaler {
    fn label(&self) -> &str {
        todo!("front E")
    }

    fn output(&self) -> &Texture {
        todo!("front E")
    }

    fn record(&self, recorder: &mut CommandRecorder) -> Result<()> {
        todo!("front E")
    }
}
