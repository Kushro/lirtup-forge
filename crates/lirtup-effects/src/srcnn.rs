//! SRCNN with the CuNNy models (`srcnn/`): 2x per run.

use std::sync::Arc;

use lirtup_config::Model;
use lirtup_gpu::{CommandRecorder, Device, Texture};

use crate::{Pass, Result};

/// Tile mode: the input is a texture array with one tile per layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileLayout {
    /// Tile interior size.
    pub tile_size: u32,
    /// Context margin around each tile.
    pub margin: u32,
    /// Layers (maximum tiles per frame).
    pub layers: u32,
}

/// Options of a SRCNN pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SrcnnOptions {
    /// Model; must not be [`Model::None`].
    pub model: Model,
    /// Tiles instead of the full frame.
    pub tiles: Option<TileLayout>,
    /// Use the FP16 variants if the device has `shaderFloat16`.
    pub fp16: bool,
}

/// A CuNNy model bound to an input: output is twice the input size (or, in
/// tile mode, twice each tile without its margin).
pub struct Srcnn {
    _private: (),
}

impl Srcnn {
    /// Load the model and create its passes.
    pub fn new(device: &Arc<Device>, input: &Texture, options: &SrcnnOptions) -> Result<Srcnn> {
        todo!("front E")
    }

    /// Number of dispatches per frame.
    pub fn pass_count(&self) -> usize {
        todo!("front E")
    }

    /// In tile mode, how many layers to run this frame (the rest are skipped).
    pub fn set_active_tiles(&mut self, count: u32) {
        todo!("front E")
    }
}

impl Pass for Srcnn {
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
