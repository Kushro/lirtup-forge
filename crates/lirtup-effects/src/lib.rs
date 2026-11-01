//! GPU passes of LirtUp Forge: SRCNN (CuNNy), the native scalers and effects,
//! the color converters and the OSD.
//!
//! Every pass implements [`Pass`]: it is created with its input texture and
//! output size (resources are bound at creation, as `lirtup_gpu::ComputePipeline`
//! wants), records its dispatches into a `CommandRecorder`, and exposes its
//! output texture so the next pass can read it. Front P (round 2) builds the
//! chain from the config in `lirtup-pipeline::presenter`; this crate only
//! knows single passes.
//!
//! Shaders are precompiled `.spv`, embedded with `include_bytes!` from this
//! crate's `shaders/` and `models/cunny/` ([`assets`]).

pub mod assets;
pub mod convert;
pub mod effects;
pub mod error;
mod lut;
pub mod osd;
pub mod scalers;
pub mod srcnn;
pub mod ui_draw;
pub mod util;

pub use error::{EffectError, Result};

use lirtup_gpu::{CommandRecorder, Texture};

/// Live parameter values by name (MagpieFX `//!PARAMETER`s, or the fields of
/// a native effect).
pub type ParamValues = std::collections::BTreeMap<String, f64>;

/// One GPU pass (or a group of dispatches that acts as one).
pub trait Pass: Send {
    /// Name for logs, timestamps and the stats panel («ms por etapa»).
    fn label(&self) -> &str;

    /// The texture this pass writes; the next pass reads it.
    fn output(&self) -> &Texture;

    /// Record the dispatches of one frame. Timestamps are named after
    /// [`Pass::label`].
    fn record(&self, recorder: &mut CommandRecorder) -> Result<()>;

    /// Change parameters without recreating the pass (constant buffer or
    /// push constants). Unknown names are ignored.
    fn set_params(&mut self, params: &ParamValues) -> Result<()> {
        let _ = params;
        Ok(())
    }

    /// Per-frame constants (frame counter, time) for passes that animate
    /// (grain, MagpieFX `FrameCount`).
    fn set_frame(&mut self, frame: u64, time_s: f32) {
        let _ = (frame, time_s);
    }
}
