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
//!
//! - **2×**: every dirty tile is cut out of the captured frame with its
//!   margin (edge pixels replicated past the border, `extract_expanded_tiles`),
//!   written into one layer of an upload buffer and copied into the input
//!   array. The last SRCNN pass reads the residual from the full 1× texture
//!   (the upscaler's input), which the upscaler uploads first.
//! - **4×**: the first stage runs on the full frame (the upscaler records it
//!   first) and only the second stage is tiled: the 2× regions of the dirty
//!   tiles are copied on the GPU from the first stage's output. A tile whose
//!   margin crosses the border would need clamping: the frame goes full frame
//!   instead, as 1.x `_process_4x`.

use std::sync::Arc;

use lirtup_config::Config;
use lirtup_effects::Pass;
use lirtup_effects::srcnn::{Srcnn, SrcnnOptions, TileLayout, TilePlacement};
use lirtup_effects::util;
use lirtup_gpu::{Buffer, BufferDesc, CommandRecorder, CopyRegion, Device, Format, MemoryLocation, Texture};
use lirtup_x11::DamageRect;

use crate::Result;

/// Tile pipeline of one source size.
pub struct TileProcessor {
    src_size: (u32, u32),
    tile_size: u32,
    margin: u32,
    layers: u32,
    double: bool,
    area_threshold: f32,
    grid: (u32, u32),
    /// Expanded tiles, one per layer (`tile + 2 margin`, doubled for 4×).
    input: Texture,
    /// 2× only: the CPU-extracted tiles, one layer after another.
    staging: Option<Buffer>,
    /// The 2× residual for 4× (the first stage's full-frame output).
    residual: Texture,
    srcnn: Srcnn,
    placements: Vec<TilePlacement>,
}

impl TileProcessor {
    /// Tiles for a source of `src_size`. `residual` is what the last SRCNN
    /// pass adds: the 1× input for 2×, the first stage's 2× output for 4×.
    /// Writes into its own full-size output ([`TileProcessor::output`]),
    /// kept between frames: clean tiles keep last frame's result.
    pub fn new(
        device: &Arc<Device>,
        config: &Config,
        src_size: (u32, u32),
        residual: &Texture,
    ) -> Result<TileProcessor> {
        let tile_size = config.tile_size.max(1);
        let margin = config.tile_context_margin;
        let layers = config.max_tile_layers.max(1);
        let double = config.scaling.double_upscale;
        let scale = if double { 4 } else { 2 };
        let expanded = tile_size + 2 * margin;
        let feat = if double { expanded * 2 } else { expanded };
        let format = if double {
            residual.format()
        } else {
            Format::R8G8B8A8Unorm
        };
        let input = util::texture_array(device, feat, feat, layers, true, format, "tiles input")?;
        let staging = if double {
            None
        } else {
            Some(device.create_buffer(&BufferDesc {
                size: u64::from(expanded) * u64::from(expanded) * 4 * u64::from(layers),
                stride: 0,
                format: None,
                location: MemoryLocation::Upload,
                label: "tiles staging".to_owned(),
            })?)
        };
        let layout = TileLayout {
            tile_size,
            margin: if double { margin * 2 } else { margin },
            layers,
        };
        let srcnn = Srcnn::new_tiled(
            device,
            &input,
            residual,
            (src_size.0 * scale, src_size.1 * scale),
            &layout,
            &SrcnnOptions {
                model: config.scaling.model,
                tiles: Some(layout),
                fp16: lirtup_magpiefx::runtime::fp16_wanted(device),
            },
        )?;
        tracing::debug!(
            "tiles: {}x{}, tile {tile_size}, margin {margin}, {scale}x, {layers} layers",
            src_size.0,
            src_size.1
        );
        Ok(TileProcessor {
            src_size,
            tile_size,
            margin,
            layers,
            double,
            area_threshold: config.area_threshold,
            grid: (src_size.0.div_ceil(tile_size), src_size.1.div_ceil(tile_size)),
            input,
            staging,
            residual: residual.clone(),
            srcnn,
            placements: Vec::with_capacity(layers as usize),
        })
    }

    /// The full upscaled frame the tiles are written into.
    pub fn output(&self) -> &Texture {
        self.srcnn.output()
    }

    /// Whether this frame should go through tiles (`should_use_tile_mode`):
    /// some damage, fewer dirty tiles than layers, and the expanded dirty
    /// area under `area_threshold`.
    pub fn should_use(&self, damage: &[DamageRect]) -> bool {
        if damage.is_empty() {
            return false;
        }
        if dirty_tiles(damage, self.tile_size, self.grid).len() >= self.layers as usize {
            return false;
        }
        let expanded = lirtup_x11::tiles::expand_rects(damage, self.margin, self.src_size.0, self.src_size.1);
        lirtup_x11::tiles::dirty_fraction(&expanded, self.src_size.0, self.src_size.1) <= self.area_threshold
    }

    /// Record the dirty tiles of a captured frame (BGRA, `stride` bytes per
    /// row). The caller recorded the residual first (the 1× upload, or the
    /// first 4× stage) in the same recording, and waited for the previous
    /// submission (the upload buffer is rewritten here).
    ///
    /// Returns the number of tiles, or `None` when this frame cannot go
    /// through tiles (too many, or a 4× edge tile): record it full frame.
    pub fn record(
        &mut self,
        recorder: &mut CommandRecorder,
        pixels: &[u8],
        stride: u32,
        damage: &[DamageRect],
    ) -> Result<Option<u32>> {
        let tiles = dirty_tiles(damage, self.tile_size, self.grid);
        if tiles.is_empty() {
            return Ok(Some(0));
        }
        if tiles.len() > self.layers as usize {
            tracing::warn!("{} dirty tiles > {} layers, full frame", tiles.len(), self.layers);
            return Ok(None);
        }
        let scale = if self.double { 4 } else { 2 };
        let full_out = (self.src_size.0 * scale, self.src_size.1 * scale);
        let expanded = self.tile_size + 2 * self.margin;
        let mut regions = Vec::with_capacity(tiles.len());
        if self.double {
            for (layer, &(tx, ty)) in tiles.iter().enumerate() {
                let Some((x0, y0)) = interior_origin(tx, ty, self.tile_size, self.margin, self.src_size)
                else {
                    return Ok(None);
                };
                regions.push(CopyRegion {
                    src_offset: u64::from(x0 * 2),
                    src_y: y0 * 2,
                    dst_slice: layer as u32,
                    width: expanded * 2,
                    height: expanded * 2,
                    ..CopyRegion::default()
                });
            }
            recorder.copy_texture(&self.residual, &self.input, &regions);
        } else {
            let staging = self.staging.as_ref().expect("2x tiles have a staging buffer");
            let tile_bytes = (expanded * expanded * 4) as usize;
            staging.write_with(|mapped| {
                for (layer, &(tx, ty)) in tiles.iter().enumerate() {
                    let out = &mut mapped[layer * tile_bytes..(layer + 1) * tile_bytes];
                    extract_expanded(
                        pixels,
                        stride,
                        self.src_size,
                        tx,
                        ty,
                        self.tile_size,
                        self.margin,
                        out,
                    );
                }
            })?;
            for layer in 0..tiles.len() {
                regions.push(CopyRegion {
                    src_offset: (layer * tile_bytes) as u64,
                    dst_slice: layer as u32,
                    width: expanded,
                    height: expanded,
                    ..CopyRegion::default()
                });
            }
            recorder.copy_buffer_to_texture(staging, &self.input, &regions);
        }
        self.placements.clear();
        self.placements.extend(
            tiles
                .iter()
                .map(|&(tx, ty)| TilePlacement::from_grid(tx, ty, self.tile_size, scale, full_out)),
        );
        self.srcnn.set_tiles(&self.placements);
        self.srcnn.record(recorder)?;
        Ok(Some(tiles.len() as u32))
    }
}

/// Grid cells touched by `damage`, row-major, without duplicates
/// (`_count_dirty_tile_cells`, `collect_dirty_tile_coords`).
pub fn dirty_tiles(damage: &[DamageRect], tile_size: u32, grid: (u32, u32)) -> Vec<(u32, u32)> {
    let ts = i64::from(tile_size.max(1));
    let mut cells = vec![false; (grid.0 * grid.1) as usize];
    for r in damage.iter().filter(|r| r.width > 0 && r.height > 0) {
        let x0 = (i64::from(r.x).max(0) / ts) as u32;
        let y0 = (i64::from(r.y).max(0) / ts) as u32;
        let x1 = ((i64::from(r.x) + i64::from(r.width) + ts - 1) / ts).clamp(0, i64::from(grid.0)) as u32;
        let y1 = ((i64::from(r.y) + i64::from(r.height) + ts - 1) / ts).clamp(0, i64::from(grid.1)) as u32;
        for ty in y0..y1 {
            for tx in x0..x1 {
                cells[(ty * grid.0 + tx) as usize] = true;
            }
        }
    }
    cells
        .iter()
        .enumerate()
        .filter(|(_, dirty)| **dirty)
        .map(|(i, _)| (i as u32 % grid.0, i as u32 / grid.0))
        .collect()
}

/// Top-left of tile `(tx, ty)` expanded by `margin`, when the expanded
/// region lies fully inside the frame (no clamping needed).
fn interior_origin(tx: u32, ty: u32, tile_size: u32, margin: u32, size: (u32, u32)) -> Option<(u32, u32)> {
    let x0 = (tx * tile_size).checked_sub(margin)?;
    let y0 = (ty * tile_size).checked_sub(margin)?;
    let side = tile_size + 2 * margin;
    (x0 + side <= size.0 && y0 + side <= size.1).then_some((x0, y0))
}

/// Copy tile `(tx, ty)` with `margin` pixels around it into `out`
/// (`(tile + 2 margin)²` BGRA texels, tightly packed); pixels past the
/// frame repeat the nearest edge pixel (`extract_expanded_tiles`).
#[allow(clippy::too_many_arguments)]
pub fn extract_expanded(
    pixels: &[u8],
    stride: u32,
    size: (u32, u32),
    tx: u32,
    ty: u32,
    tile_size: u32,
    margin: u32,
    out: &mut [u8],
) {
    let side = (tile_size + 2 * margin) as i64;
    let x0 = i64::from(tx * tile_size) - i64::from(margin);
    let y0 = i64::from(ty * tile_size) - i64::from(margin);
    let (w, h) = (i64::from(size.0), i64::from(size.1));
    let stride = stride as usize;
    // Columns inside the frame are copied as one run per row.
    let in_x0 = x0.clamp(0, w);
    let in_x1 = (x0 + side).clamp(0, w);
    for row in 0..side {
        let sy = (y0 + row).clamp(0, h - 1) as usize;
        let src_row = &pixels[sy * stride..sy * stride + size.0 as usize * 4];
        let dst_row = &mut out[(row * side * 4) as usize..((row + 1) * side * 4) as usize];
        if in_x1 > in_x0 {
            let d0 = ((in_x0 - x0) * 4) as usize;
            let d1 = ((in_x1 - x0) * 4) as usize;
            dst_row[d0..d1].copy_from_slice(&src_row[(in_x0 * 4) as usize..(in_x1 * 4) as usize]);
        }
        for col in 0..side {
            let sx = x0 + col;
            if sx >= in_x0 && sx < in_x1 {
                continue;
            }
            let sx = sx.clamp(0, w - 1) as usize;
            let d = (col * 4) as usize;
            dst_row[d..d + 4].copy_from_slice(&src_row[sx * 4..sx * 4 + 4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> DamageRect {
        DamageRect {
            x,
            y,
            width,
            height,
            hash: 0,
        }
    }

    #[test]
    fn dirty_tiles_are_row_major_and_unique() {
        let tiles = dirty_tiles(
            &[rect(70, 10, 10, 10), rect(0, 0, 64, 64), rect(10, 10, 5, 5)],
            64,
            (3, 2),
        );
        assert_eq!(tiles, vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn dirty_tiles_clamp_to_the_grid() {
        let tiles = dirty_tiles(&[rect(-10, 100, 500, 500), rect(0, 0, 0, 10)], 64, (3, 2));
        assert_eq!(tiles, vec![(0, 1), (1, 1), (2, 1)]);
    }

    #[test]
    fn interior_origin_rejects_edge_tiles() {
        assert_eq!(interior_origin(0, 1, 64, 4, (256, 256)), None);
        assert_eq!(interior_origin(1, 1, 64, 4, (256, 256)), Some((60, 60)));
        assert_eq!(interior_origin(3, 1, 64, 4, (256, 256)), None);
    }

    #[test]
    fn extract_expanded_replicates_edges() {
        // 3x2 frame, pixel value = index; stride with padding.
        let (w, h, stride) = (3u32, 2u32, 16u32);
        let mut pixels = vec![0u8; (stride * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let o = (y * stride + x * 4) as usize;
                pixels[o..o + 4].fill((y * w + x) as u8);
            }
        }
        // Tile (0, 0) of size 2 with margin 1: 4x4, starting at (-1, -1).
        let mut out = vec![0u8; 4 * 4 * 4];
        extract_expanded(&pixels, stride, (w, h), 0, 0, 2, 1, &mut out);
        let got: Vec<u8> = out.chunks(4).map(|p| p[0]).collect();
        #[rustfmt::skip]
        let want = vec![
            0, 0, 1, 2,
            0, 0, 1, 2,
            3, 3, 4, 5,
            3, 3, 4, 5,
        ];
        assert_eq!(got, want);
    }
}
