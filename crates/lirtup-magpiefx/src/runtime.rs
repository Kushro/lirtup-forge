//! Running an effect on the GPU (`effect.py`: `MagpieEffect`,
//! `compile_effect`).

use std::sync::Arc;

use lirtup_gpu::{CommandRecorder, Device, Texture};

use crate::compiler::{HlslCompiler, SpirvCache};
use crate::{CatalogEntry, EffectDesc, Result};

/// An effect parsed and compiled, not yet bound to textures. Cheap to keep
/// between sessions (the GUI's `CompileEffect` fills a cache of these in
/// `lirtup serve`).
#[derive(Clone)]
pub struct CompiledEffect {
    /// The description.
    pub desc: Arc<EffectDesc>,
    /// SPIR-V per pass.
    pub spirv: Arc<Vec<Vec<u8>>>,
    /// Whether the FP16 variant was compiled.
    pub fp16: bool,
}

/// Parse and compile every pass of an entry (blocking: run it with
/// `spawn_blocking` or on the pipeline thread before the first frame).
pub fn compile_effect(
    entry: &CatalogEntry,
    compilers: &[Box<dyn HlslCompiler>],
    cache: &SpirvCache,
    fp16: bool,
) -> Result<CompiledEffect> {
    todo!("front E")
}

/// Whether FP16 variants should be used on this device (`fp16_wanted`).
pub fn fp16_wanted(device: &Device) -> bool {
    todo!("front E")
}

/// A compiled effect bound to an input, with its intermediate textures,
/// lookup textures and samplers. Implements `lirtup_effects::Pass`.
pub struct MagpieEffect {
    _private: (),
}

impl MagpieEffect {
    /// Bind. `output_size` is the size the chain wants after this effect
    /// (`OUTPUT_WIDTH`/`OUTPUT_HEIGHT` in the expressions).
    pub fn new(
        device: &Arc<Device>,
        effect: &CompiledEffect,
        input: &Texture,
        output_size: (u32, u32),
        params: &lirtup_effects::ParamValues,
    ) -> Result<MagpieEffect> {
        todo!("front E")
    }

    /// Output size the effect's expressions give for an input size (to plan
    /// the chain before binding).
    pub fn output_size(effect: &CompiledEffect, input: (u32, u32), output: (u32, u32)) -> Result<(u32, u32)> {
        todo!("front E")
    }
}

impl lirtup_effects::Pass for MagpieEffect {
    fn label(&self) -> &str {
        todo!("front E")
    }

    fn output(&self) -> &Texture {
        todo!("front E")
    }

    fn record(&self, recorder: &mut CommandRecorder) -> lirtup_effects::Result<()> {
        todo!("front E")
    }

    fn set_params(&mut self, params: &lirtup_effects::ParamValues) -> lirtup_effects::Result<()> {
        todo!("front E")
    }

    fn set_frame(&mut self, frame: u64, time_s: f32) {
        todo!("front E")
    }
}
