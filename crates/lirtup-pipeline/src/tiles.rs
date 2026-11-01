//! Tile processing on the GPU side (`tile.py`, `utils.py`). Front P,
//! round 2.
//!
//! Only [`crate::upscale`] uses it; front P may reshape it freely. The CPU
//! side (tile grid, hashes, merging damage) is already in
//! `lirtup_x11::tiles`; the GPU side (layer arrays, `Srcnn::new_tiled`,
//! `TilePlacement`) in `lirtup_effects::srcnn`. What is left here is the
//! glue of `TileProcessor`: pack dirty tiles with their context margin into
//! the array layers, batch them by `max_tile_layers`, and run the 2× or 4×
//! (double upscale) tile pipeline.

use std::sync::Arc;

use lirtup_config::Config;
use lirtup_gpu::{Device, Texture};
use lirtup_x11::DamageRect;

use crate::Result;

/// Tile pipeline of one source size.
pub struct TileProcessor {
    _private: (),
}

impl TileProcessor {
    /// Writes into `output` (the full upscaled frame, kept between frames:
    /// clean tiles keep last frame's result).
    pub fn new(
        device: &Arc<Device>,
        config: &Config,
        src_size: (u32, u32),
        output: &Texture,
    ) -> Result<TileProcessor> {
        todo!("front P")
    }

    /// Whether this frame should go through tiles (`should_use_tile_mode`:
    /// dirty area under `area_threshold`).
    pub fn should_use(&self, damage: &[DamageRect]) -> bool {
        todo!("front P")
    }

    /// Process the dirty tiles of a captured frame (BGRA, `stride` bytes per
    /// row). Returns the number of tiles processed.
    pub fn process(&mut self, pixels: &[u8], stride: u32, damage: &[DamageRect]) -> Result<u32> {
        todo!("front P")
    }
}
