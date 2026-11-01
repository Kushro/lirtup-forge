//! Model selection, double upscale, tiles or full frame (`upscale.py`).
//! Front P, round 2.
//!
//! Only the session thread uses it; front P may reshape it freely. The
//! skeleton keeps the 1.x split: an [`Upscaler`] uploads the captured frame
//! and runs SRCNN (one or two passes for `double_upscale`), in full-frame
//! mode or, with `use_tile_processing`, on the dirty tiles only
//! ([`crate::tiles`]), falling back to full frame when too many tiles are
//! dirty. With `model: none` it only uploads (the `PassthroughUpscaler`).
//!
//! The captured frame goes through one upload buffer that mirrors the frame
//! (tightly packed): only the damaged rows are written, and only the damaged
//! rectangles are copied when the damage is small (`use_damage_tracking`).
//!
//! In tile mode the tile output is the upscaler's output for the whole
//! session. A full-frame pass (the first frame, a fallback) writes the
//! SRCNN's own output, which is then copied into it: `Srcnn` binds its output
//! at creation (1.x rebinds it with `_rebind_full_frame_output`).

use std::sync::Arc;

use lirtup_config::{Config, Model};
use lirtup_effects::Pass;
use lirtup_effects::srcnn::{Srcnn, SrcnnOptions};
use lirtup_effects::util;
use lirtup_gpu::{
    Buffer, BufferDesc, CopyRegion, Device, Format, MemoryLocation, Submission,
    SubmitOptions, Texture, Timestamps,
};
use lirtup_x11::DamageRect;

use crate::Result;
use crate::tiles::TileProcessor;

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
    device: Arc<Device>,
    model: Model,
    src_size: (u32, u32),
    scale: u32,
    damage_tracking: bool,
    area_threshold: f32,
    frame_timeout: u64,
    /// The 1× frame (also the 2× tile residual).
    input: Texture,
    /// Mirror of the captured frame, `width * 4` bytes per row.
    staging: Buffer,
    /// Full-frame SRCNN: none, one, or two for `double_upscale`.
    stages: Vec<Srcnn>,
    tiles: Option<TileProcessor>,
    first_tile_frame: bool,
    /// The last submission, with timestamps if asked for.
    last: Option<Submission>,
}

impl Upscaler {
    /// For a captured frame of `src_size` (after crop), with the model,
    /// `double_upscale`, tile settings and damage settings of `config`.
    pub fn new(device: &Arc<Device>, config: &Config, src_size: (u32, u32)) -> Result<Upscaler> {
        let (w, h) = src_size;
        let model = config.scaling.model;
        let input = util::texture(device, w, h, Format::R8G8B8A8Unorm, "upscaler input")?;
        let staging = device.create_buffer(&BufferDesc {
            size: u64::from(w) * u64::from(h) * 4,
            stride: 0,
            format: None,
            location: MemoryLocation::Upload,
            label: "upscaler staging".to_owned(),
        })?;
        let mut stages = Vec::new();
        if model != Model::None {
            let options = SrcnnOptions {
                model,
                tiles: None,
                fp16: lirtup_magpiefx::runtime::fp16_wanted(device),
            };
            let first = Srcnn::new(device, &input, &options)?;
            if config.scaling.double_upscale {
                let second = Srcnn::new(device, first.output(), &options)?;
                stages.push(first);
                stages.push(second);
            } else {
                stages.push(first);
            }
        }
        let scale = 1 << stages.len();
        let tiles = if model != Model::None && config.use_tile_processing {
            let residual = if stages.len() == 2 {
                stages[0].output()
            } else {
                &input
            };
            Some(TileProcessor::new(device, config, src_size, residual)?)
        } else {
            None
        };
        tracing::debug!(
            "upscaler: {w}x{h}, model {}, {scale}x, tiles {}",
            model.as_str(),
            tiles.is_some()
        );
        Ok(Upscaler {
            device: device.clone(),
            model,
            src_size,
            scale,
            damage_tracking: config.use_damage_tracking,
            area_threshold: config.area_threshold,
            frame_timeout: config.frame_timeout,
            input,
            staging,
            stages,
            tiles,
            first_tile_frame: true,
            last: None,
        })
    }

    /// Upload the captured frame (BGRA, `stride` bytes per row; only the
    /// `damage` rectangles when `full` is false) and record the upscale.
    /// Submits its own work (the presenter's recording waits on it: same
    /// queue); [`Upscaler::finish`] collects it.
    pub fn process(
        &mut self,
        pixels: &[u8],
        stride: u32,
        full: bool,
        damage: &[DamageRect],
        timestamps: bool,
    ) -> Result<UpscaleOutcome> {
        // The upload buffer is rewritten below: the last copy must be done.
        self.finish()?;
        let (w, h) = self.src_size;
        let partial = !full && self.damage_tracking && !damage.is_empty();
        let mut recorder = self.device.recorder()?;

        // Upload.
        let small = partial && lirtup_x11::tiles::dirty_fraction(damage, w, h) <= self.area_threshold;
        self.write_staging(pixels, stride, if partial { Some(damage) } else { None })?;
        if small {
            let regions = damage_regions(damage, self.src_size);
            if !regions.is_empty() {
                recorder.copy_buffer_to_texture(&self.staging, &self.input, &regions);
            }
        } else {
            recorder.copy_buffer_to_texture(&self.staging, &self.input, &[]);
        }

        // Upscale.
        let mut outcome = UpscaleOutcome {
            dirty_fraction: if partial {
                lirtup_x11::tiles::dirty_fraction(damage, w, h)
            } else {
                1.0
            },
            tiles: None,
        };
        match self.tiles.as_mut() {
            None => {
                for stage in &self.stages {
                    stage.record(&mut recorder)?;
                }
            }
            Some(tiles) => {
                let tiled = partial && !self.first_tile_frame && tiles.should_use(damage);
                let mut done = false;
                if tiled {
                    let double = self.stages.len() == 2;
                    if double {
                        self.stages[0].record(&mut recorder)?;
                    }
                    if let Some(n) = tiles.record(&mut recorder, pixels, stride, damage)? {
                        outcome.tiles = Some(n);
                        done = true;
                    } else {
                        // Fall back to full frame for the rest.
                        for stage in self.stages.iter().skip(usize::from(double)) {
                            stage.record(&mut recorder)?;
                        }
                    }
                } else {
                    for stage in &self.stages {
                        stage.record(&mut recorder)?;
                    }
                }
                if !done {
                    let full_out = self.stages.last().expect("tiles imply a model").output();
                    recorder.copy_texture(full_out, tiles.output(), &[]);
                    outcome.dirty_fraction = 1.0;
                }
                self.first_tile_frame = false;
            }
        }

        self.last = Some(self.device.submit(
            recorder,
            &SubmitOptions {
                timestamps,
                ..SubmitOptions::default()
            },
        )?);
        Ok(outcome)
    }

    /// Wait for the last [`Upscaler::process`] and return its timestamps (if
    /// asked for). Cheap once the presenter's work, submitted after it, is
    /// done.
    pub fn finish(&mut self) -> Result<Option<Timestamps>> {
        match self.last.take() {
            Some(submission) => Ok(submission.wait(self.frame_timeout)?),
            None => Ok(None),
        }
    }

    /// Copy the changed rows of the frame into the upload buffer (all rows
    /// without `damage`).
    fn write_staging(&self, pixels: &[u8], stride: u32, damage: Option<&[DamageRect]>) -> Result<()> {
        let (w, h) = self.src_size;
        let row = w as usize * 4;
        let stride = stride as usize;
        self.staging.write_with(|mapped| match damage {
            None => {
                for y in 0..h as usize {
                    mapped[y * row..(y + 1) * row].copy_from_slice(&pixels[y * stride..y * stride + row]);
                }
            }
            Some(damage) => {
                for r in damage_regions(damage, self.src_size) {
                    let x0 = r.dst_x as usize * 4;
                    let x1 = x0 + r.width as usize * 4;
                    for y in r.dst_y as usize..(r.dst_y + r.height) as usize {
                        mapped[y * row + x0..y * row + x1].copy_from_slice(&pixels[y * stride + x0..y * stride + x1]);
                    }
                }
            }
        })?;
        Ok(())
    }

    /// The upscaled frame (the presenter's source).
    pub fn output(&self) -> &Texture {
        match (&self.tiles, self.stages.last()) {
            (Some(tiles), _) => tiles.output(),
            (None, Some(stage)) => stage.output(),
            (None, None) => &self.input,
        }
    }

    /// Scale factor (1, 2 or 4).
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// The model in use.
    pub fn model(&self) -> Model {
        self.model
    }

    /// Labels of the SRCNN passes, for the chain (`SessionView.chain`).
    pub fn labels(&self) -> Vec<String> {
        self.stages.iter().map(|s| s.label().to_owned()).collect()
    }
}

/// The damage rectangles clamped to the frame, as copies out of a tightly
/// packed mirror of the frame (same position in the buffer and the texture).
fn damage_regions(damage: &[DamageRect], size: (u32, u32)) -> Vec<CopyRegion> {
    let (w, h) = (i64::from(size.0), i64::from(size.1));
    damage
        .iter()
        .filter_map(|r| {
            let x0 = i64::from(r.x).clamp(0, w);
            let y0 = i64::from(r.y).clamp(0, h);
            let x1 = (i64::from(r.x) + i64::from(r.width)).clamp(0, w);
            let y1 = (i64::from(r.y) + i64::from(r.height)).clamp(0, h);
            (x1 > x0 && y1 > y0).then(|| CopyRegion {
                src_offset: ((y0 * w + x0) * 4) as u64,
                src_row_length: size.0,
                dst_x: x0 as u32,
                dst_y: y0 as u32,
                width: (x1 - x0) as u32,
                height: (y1 - y0) as u32,
                ..CopyRegion::default()
            })
        })
        .collect()
}

/// The models `CycleModel` goes through, in the 1.x order
/// (`UPSCALING_MODELS`).
pub fn next_model(current: Model) -> Model {
    let all = Model::ALL;
    let i = all.iter().position(|m| *m == current).unwrap_or(0);
    all[(i + 1) % all.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_regions_are_clamped_and_mirror_the_frame() {
        let damage = [
            DamageRect {
                x: -4,
                y: 2,
                width: 10,
                height: 3,
                hash: 0,
            },
            DamageRect {
                x: 90,
                y: 90,
                width: 20,
                height: 20,
                hash: 0,
            },
            DamageRect {
                x: 200,
                y: 0,
                width: 5,
                height: 5,
                hash: 0,
            },
        ];
        let regions = damage_regions(&damage, (100, 100));
        assert_eq!(regions.len(), 2);
        assert_eq!(
            regions[0],
            CopyRegion {
                src_offset: (2 * 100) * 4,
                src_row_length: 100,
                dst_x: 0,
                dst_y: 2,
                width: 6,
                height: 3,
                ..CopyRegion::default()
            }
        );
        assert_eq!((regions[1].width, regions[1].height), (10, 10));
        assert_eq!(regions[1].src_offset, (90 * 100 + 90) * 4);
    }

    #[test]
    fn next_model_wraps() {
        let last = *Model::ALL.last().unwrap();
        assert_eq!(next_model(last), Model::ALL[0]);
        assert_eq!(next_model(Model::ALL[0]), Model::ALL[1]);
    }
}
