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
//!
//! # The MagpieFX chain before the model (fx-unify)
//!
//! ```text
//! capture ─upload─▶ input ─▶ [magpie_effects_before_model] ─▶ SRCNN ×1/×2 (or none) ─▶ output
//! ```
//!
//! `magpie_effects_before_model` runs here, on the uploaded frame, every
//! frame (the upload and its damage tracking are unchanged): the first effect
//! swaps red and blue on read and the last one on write, as the presenter's
//! chain does ([`crate::magpie`]), so its output is BGRA bytes again and the
//! model takes it as it took the capture. An effect that does not fix its
//! OUTPUT size renders at its own input size ([`OutSize::Input`]), so the
//! model sees the capture's size unless an effect changes it (Anime4K ×2:
//! the upscaler's output is then the chain's output × the model's scale;
//! [`Upscaler::scale`] stays the model's). With `model: none` the chain's
//! output is the upscaler's output.
//!
//! The chain is compiled blocking in [`Upscaler::new`]; effects that fail to
//! compile or bind are dropped and reported ([`Upscaler::take_disabled`]).
//! New parameter values for the same effects go live
//! ([`Upscaler::update_pre_params`]); another list rebuilds the upscaler.
//!
//! **Tiles**: tile processing cuts the dirty tiles out of the captured
//! pixels on the CPU, which a neighbourhood effect before the model would
//! turn into seams. With a non-empty chain before the model the whole
//! upscaler runs full frame (`use_tile_processing` is ignored; logged at
//! debug level when the upscaler is built).

use std::sync::Arc;
use std::time::Instant;

use lirtup_config::{Config, Model};
use lirtup_effects::Pass;
use lirtup_effects::srcnn::{Srcnn, SrcnnOptions};
use lirtup_effects::util;
use lirtup_gpu::{
    Buffer, BufferDesc, CopyRegion, Device, Format, MemoryLocation, Submission, SubmitOptions, Texture,
    Timestamps,
};
use lirtup_magpiefx::spec::EffectSpec;
use lirtup_x11::DamageRect;

use crate::Result;
use crate::magpie::{
    MagpieContext, OutSize, SWAP_RB_LABEL, build_magpie_chain, parse_chain, same_names, set_chain_params,
};
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
    /// The 1× frame as captured (also the 2× tile residual: tiles only run
    /// without a chain before the model).
    input: Texture,
    /// The MagpieFX chain before the model (its passes, `SwapRB` included),
    /// reading `input`; the model reads its last output.
    pre: Vec<Box<dyn Pass>>,
    /// The effects of `pre`, in order (failed ones dropped).
    pre_specs: Vec<EffectSpec>,
    /// Effects of the chain before the model disabled since the last
    /// [`Upscaler::take_disabled`].
    disabled: Vec<(String, String)>,
    /// Frame counter and start, for the chain's animated effects
    /// (`FrameCount`).
    frame: u64,
    started: Instant,
    /// Mirror of the captured frame, `width * 4` bytes per row.
    staging: Buffer,
    /// Full-frame SRCNN: none, one, or two for `double_upscale`.
    stages: Vec<Srcnn>,
    tiles: Option<TileProcessor>,
    first_tile_frame: bool,
    /// Something was uploaded already: the upload buffer and the input
    /// mirror the frame, so damage alone is enough.
    primed: bool,
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

        // The chain before the model, compiled now (blocking) so the first
        // frame does not stall.
        let mut disabled = Vec::new();
        let requested = parse_chain(&config.scaling.magpie_effects_before_model);
        let (pre, pre_specs) = if requested.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            let mut ctx = MagpieContext::new(device);
            ctx.precompile(&requested, &mut disabled);
            // What failed to compile is reported once, not again by binding.
            let specs = without_disabled(&requested, &disabled);
            let mut magpie = Some(ctx);
            build_magpie_chain(device, &mut magpie, &input, &specs, OutSize::Input, &mut disabled)
        };
        // What the model reads: the chain's output, or the frame.
        let model_input = pre.last().map(|p| p.output()).unwrap_or(&input).clone();

        let mut stages = Vec::new();
        if model != Model::None {
            let options = SrcnnOptions {
                model,
                tiles: None,
                fp16: lirtup_magpiefx::runtime::fp16_wanted(device),
            };
            let first = Srcnn::new(device, &model_input, &options)?;
            if config.scaling.double_upscale {
                let second = Srcnn::new(device, first.output(), &options)?;
                stages.push(first);
                stages.push(second);
            } else {
                stages.push(first);
            }
        }
        let scale = 1 << stages.len();
        let use_tiles = tiles_allowed(model, config.use_tile_processing, pre.len());
        if model != Model::None && config.use_tile_processing && !use_tiles {
            tracing::debug!(
                "upscaler: tile processing off, full frame: a MagpieFX chain runs before the model"
            );
        }
        let tiles = if use_tiles {
            let residual = if stages.len() == 2 {
                stages[0].output()
            } else {
                &model_input
            };
            Some(TileProcessor::new(device, config, src_size, residual)?)
        } else {
            None
        };
        tracing::debug!(
            "upscaler: {w}x{h}, {} effects before the model, model {}, {scale}x, tiles {}",
            pre_specs.len(),
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
            pre,
            pre_specs,
            disabled,
            frame: 0,
            started: Instant::now(),
            staging,
            stages,
            tiles,
            first_tile_frame: true,
            primed: false,
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
        // A new upscaler (another model, a new size) starts from the whole
        // frame: the capture buffer holds it, the upload buffer does not.
        let full = full || !self.primed;
        self.primed = true;
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

        // The chain before the model, on the whole frame (no tiles with it).
        if !self.pre.is_empty() {
            let time = self.started.elapsed().as_secs_f32();
            for pass in &mut self.pre {
                pass.set_frame(self.frame, time);
            }
            self.frame = self.frame.wrapping_add(1);
            for pass in &self.pre {
                pass.record(&mut recorder)?;
            }
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
                        mapped[y * row + x0..y * row + x1]
                            .copy_from_slice(&pixels[y * stride + x0..y * stride + x1]);
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
            (None, None) => self.pre.last().map(|p| p.output()).unwrap_or(&self.input),
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

    /// Labels for the chain (`SessionView.chain`, the stats panel): the
    /// effects bound before the model, then the SRCNN passes. The effects'
    /// labels are their passes' (as `Presenter::chain` once built: the file
    /// stem, which also names their timestamps), without the `SwapRB` passes.
    pub fn labels(&self) -> Vec<String> {
        self.pre
            .iter()
            .filter(|p| p.label() != SWAP_RB_LABEL)
            .map(|p| p.label().to_owned())
            .chain(self.stages.iter().map(|s| s.label().to_owned()))
            .collect()
    }

    // ---- the MagpieFX chain before the model (fx-unify) -----------------

    /// The effects of `magpie_effects_before_model` actually bound (failed
    /// ones dropped), in order.
    pub fn pre_chain(&self) -> &[EffectSpec] {
        &self.pre_specs
    }

    /// The effects of the chain before the model disabled (compile or bind
    /// error) since the last call: `(name, error)`, sent by the session as
    /// `Event::EffectCompile { session: true }` like
    /// `Presenter::take_disabled`.
    pub fn take_disabled(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.disabled)
    }

    /// Same effects before the model with new parameter values: applied
    /// live (`set_params`, no recompile, no new textures). `false` when the
    /// upscaler must be rebuilt instead (another list of effects, or one that
    /// failed before).
    pub fn update_pre_params(&mut self, config: &Config) -> Result<bool> {
        let specs = parse_chain(&config.scaling.magpie_effects_before_model);
        if !same_names(&specs, &self.pre_specs) {
            return Ok(false);
        }
        if specs.is_empty() {
            return Ok(true);
        }
        // `set_params` writes upload buffers: nothing of ours may be in
        // flight (normally nothing is: the presenter waited for its frame,
        // which comes after ours on the same queue).
        self.finish()?;
        if !set_chain_params(&mut self.pre, &specs)? {
            return Ok(false);
        }
        self.pre_specs = specs;
        Ok(true)
    }
}

/// Whether the upscaler runs in tile mode: a model, `use_tile_processing`,
/// and no effect bound before the model (a neighbourhood effect would make
/// seams at the tile borders: full frame then).
fn tiles_allowed(model: Model, use_tile_processing: bool, pre_passes: usize) -> bool {
    model != Model::None && use_tile_processing && pre_passes == 0
}

/// `specs` without the effects already in `disabled` (failed to compile).
fn without_disabled(specs: &[EffectSpec], disabled: &[(String, String)]) -> Vec<EffectSpec> {
    specs
        .iter()
        .filter(|s| !disabled.iter().any(|(name, _)| *name == s.name))
        .cloned()
        .collect()
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
    fn tiles_fall_back_to_full_frame_with_a_chain_before_the_model() {
        let model = Model::ALL
            .iter()
            .copied()
            .find(|m| *m != Model::None)
            .expect("a model");
        assert!(tiles_allowed(model, true, 0));
        assert!(!tiles_allowed(model, true, 3), "a chain before the model: full frame");
        assert!(!tiles_allowed(model, false, 0));
        assert!(!tiles_allowed(Model::None, true, 0));
    }

    #[test]
    fn failed_effects_are_not_bound_again() {
        let spec = |name: &str| EffectSpec {
            name: name.to_owned(),
            params: Default::default(),
        };
        let specs = [spec("CAS"), spec("Broken"), spec("Sharpen/LumaSharpen"), spec("Broken")];
        let disabled = [("Broken".to_owned(), "does not compile".to_owned())];
        let kept = without_disabled(&specs, &disabled);
        assert_eq!(kept, vec![spec("CAS"), spec("Sharpen/LumaSharpen")]);
        assert_eq!(without_disabled(&specs, &[]), specs.to_vec());
    }

    #[test]
    fn next_model_wraps() {
        let last = *Model::ALL.last().unwrap();
        assert_eq!(next_model(last), Model::ALL[0]);
        assert_eq!(next_model(Model::ALL[0]), Model::ALL[1]);
    }
}
