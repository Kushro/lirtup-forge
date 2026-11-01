//! Native effects (`shaders/effects/`): CAS, deband, bloom, vignette, grain,
//! LUT, LumaSharpen, Anime4K S, CRT-Easymode.
//!
//! CAS, bloom, vignette, grain and LUT work in place, as in 1.x (the input is
//! bound as SRV and UAV and is also the output). Deband, LumaSharpen and
//! CRT-Easymode write a new texture; Anime4K S is four passes into a 2x
//! output. Parameters are clamped to the 1.x ranges.

use std::sync::Arc;

use lirtup_config::ModeSettings;
use lirtup_gpu::{Buffer, CommandRecorder, ComputePipeline, Device, Dispatch, Filter, Format, Texture};

use crate::util::{self, Bindings, CbWriter, clampf};
use crate::{EffectError, ParamValues, Pass, Result};

/// CRT-Easymode parameters (the `crt_*` fields).
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)] // same names as the config fields
pub struct CrtParams {
    pub sharpness_h: f32,
    pub sharpness_v: f32,
    pub mask_strength: f32,
    pub mask_dot_width: u32,
    pub mask_dot_height: u32,
    pub mask_stagger: u32,
    pub mask_size: u32,
    pub scanline_strength: f32,
    pub scanline_beam_width_min: f32,
    pub scanline_beam_width_max: f32,
    pub scanline_bright_min: f32,
    pub scanline_bright_max: f32,
    pub scanline_cutoff: u32,
    pub gamma_input: f32,
    pub gamma_output: f32,
    pub bright_boost: f32,
    pub dilation: u32,
}

impl Default for CrtParams {
    /// Magpie's defaults (1.x `crt_easymode.PARAMS`).
    fn default() -> Self {
        CrtParams {
            sharpness_h: 0.5,
            sharpness_v: 1.0,
            mask_strength: 0.3,
            mask_dot_width: 1,
            mask_dot_height: 1,
            mask_stagger: 0,
            mask_size: 1,
            scanline_strength: 1.0,
            scanline_beam_width_min: 1.5,
            scanline_beam_width_max: 1.5,
            scanline_bright_min: 0.35,
            scanline_bright_max: 0.65,
            scanline_cutoff: 400,
            gamma_input: 2.0,
            gamma_output: 1.8,
            bright_boost: 1.2,
            dilation: 1,
        }
    }
}

impl CrtParams {
    fn from_mode(m: &ModeSettings) -> CrtParams {
        CrtParams {
            sharpness_h: m.crt_sharpness_h,
            sharpness_v: m.crt_sharpness_v,
            mask_strength: m.crt_mask_strength,
            mask_dot_width: m.crt_mask_dot_width,
            mask_dot_height: m.crt_mask_dot_height,
            mask_stagger: m.crt_mask_stagger,
            mask_size: m.crt_mask_size,
            scanline_strength: m.crt_scanline_strength,
            scanline_beam_width_min: m.crt_scanline_beam_width_min,
            scanline_beam_width_max: m.crt_scanline_beam_width_max,
            scanline_bright_min: m.crt_scanline_bright_min,
            scanline_bright_max: m.crt_scanline_bright_max,
            scanline_cutoff: m.crt_scanline_cutoff,
            gamma_input: m.crt_gamma_input,
            gamma_output: m.crt_gamma_output,
            bright_boost: m.crt_bright_boost,
            dilation: m.crt_dilation,
        }
    }

    /// Set one field by its config name (`crt_sharpness_h`, ...).
    fn set(&mut self, name: &str, v: f64) -> bool {
        let f = v as f32;
        let u = v.max(0.0).round() as u32;
        match name {
            "crt_sharpness_h" => self.sharpness_h = f,
            "crt_sharpness_v" => self.sharpness_v = f,
            "crt_mask_strength" => self.mask_strength = f,
            "crt_mask_dot_width" => self.mask_dot_width = u,
            "crt_mask_dot_height" => self.mask_dot_height = u,
            "crt_mask_stagger" => self.mask_stagger = u,
            "crt_mask_size" => self.mask_size = u,
            "crt_scanline_strength" => self.scanline_strength = f,
            "crt_scanline_beam_width_min" => self.scanline_beam_width_min = f,
            "crt_scanline_beam_width_max" => self.scanline_beam_width_max = f,
            "crt_scanline_bright_min" => self.scanline_bright_min = f,
            "crt_scanline_bright_max" => self.scanline_bright_max = f,
            "crt_scanline_cutoff" => self.scanline_cutoff = u,
            "crt_gamma_input" => self.gamma_input = f,
            "crt_gamma_output" => self.gamma_output = f,
            "crt_bright_boost" => self.bright_boost = f,
            "crt_dilation" => self.dilation = u,
            _ => return false,
        }
        true
    }

    /// The 17 values after the sizes, clamped to Magpie's ranges.
    fn pack(&self, cb: CbWriter) -> CbWriter {
        let i = |v: u32, lo: i32, hi: i32| (v.min(i32::MAX as u32) as i32).clamp(lo, hi);
        cb.f32(clampf(self.sharpness_h, 0.0, 1.0))
            .f32(clampf(self.sharpness_v, 0.0, 1.0))
            .f32(clampf(self.mask_strength, 0.0, 1.0))
            .i32(i(self.mask_dot_width, 1, 100))
            .i32(i(self.mask_dot_height, 1, 100))
            .i32(i(self.mask_stagger, 0, 100))
            .i32(i(self.mask_size, 1, 100))
            .f32(clampf(self.scanline_strength, 0.0, 1.0))
            .f32(clampf(self.scanline_beam_width_min, 0.5, 5.0))
            .f32(clampf(self.scanline_beam_width_max, 0.5, 5.0))
            .f32(clampf(self.scanline_bright_min, 0.0, 1.0))
            .f32(clampf(self.scanline_bright_max, 0.0, 1.0))
            .i32(i(self.scanline_cutoff, 1, 1000))
            .f32(clampf(self.gamma_input, 0.1, 5.0))
            .f32(clampf(self.gamma_output, 0.1, 5.0))
            .f32(clampf(self.bright_boost, 1.0, 2.0))
            .i32(i(self.dilation, 0, 1))
    }
}

/// A native effect with its parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NativeEffect {
    /// Contrast Adaptive Sharpening.
    Cas {
        /// Strength.
        strength: f32,
    },
    /// Debanding.
    Deband {
        /// Strength.
        strength: f32,
    },
    /// Bloom.
    Bloom {
        /// Strength.
        strength: f32,
        /// Brightness threshold.
        threshold: f32,
        /// Blur radius in pixels.
        radius: u32,
    },
    /// Vignette.
    Vignette {
        /// Strength.
        strength: f32,
        /// Start distance.
        radius: f32,
        /// Softness.
        falloff: f32,
    },
    /// Film grain.
    Grain {
        /// Strength.
        strength: f32,
        /// Particle size.
        size: f32,
    },
    /// 3D LUT.
    Lut {
        /// Blend.
        intensity: f32,
        /// Built-in preset (`identity`, ...); stored by index in the pass.
        preset: LutPreset,
    },
    /// LumaSharpen.
    LumaSharpen {
        /// Strength.
        strength: f32,
        /// Clamp.
        clamp: f32,
        /// Pattern 0 to 3.
        pattern: u32,
        /// Offset bias.
        offset_bias: f32,
    },
    /// Anime4K_Upscale_S: doubles the size.
    Anime4kS,
    /// CRT-Easymode: scales to the output size itself (replaces the final
    /// scaler).
    CrtEasymode(CrtParams),
}

/// Built-in LUT presets (1.x `LUT_PRESETS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LutPreset {
    /// No change.
    #[default]
    Identity,
    /// `warm`: warm sunset.
    Warm,
    /// `cool`: cool night.
    Cool,
    /// `split`: split tone (cyan shadows, orange highlights).
    Split,
    /// `vivid`: more saturation.
    Vivid,
    /// `pastel`: soft pastel.
    Pastel,
    /// `lofi`: vintage lo-fi.
    Lofi,
    /// `bleach`: bleach bypass.
    Bleach,
    /// `film`: film stock S-curve.
    Film,
    /// `noir`: high-contrast monochrome.
    Noir,
    /// `sepia`.
    Sepia,
    /// `cyano`: cyanotype.
    Cyano,
}

impl LutPreset {
    /// All presets, in the 1.x order.
    pub const ALL: [LutPreset; 12] = [
        LutPreset::Identity,
        LutPreset::Warm,
        LutPreset::Cool,
        LutPreset::Split,
        LutPreset::Vivid,
        LutPreset::Pastel,
        LutPreset::Lofi,
        LutPreset::Bleach,
        LutPreset::Film,
        LutPreset::Noir,
        LutPreset::Sepia,
        LutPreset::Cyano,
    ];

    /// The config spelling (`lut_preset`).
    pub fn name(self) -> &'static str {
        match self {
            LutPreset::Identity => "identity",
            LutPreset::Warm => "warm",
            LutPreset::Cool => "cool",
            LutPreset::Split => "split",
            LutPreset::Vivid => "vivid",
            LutPreset::Pastel => "pastel",
            LutPreset::Lofi => "lofi",
            LutPreset::Bleach => "bleach",
            LutPreset::Film => "film",
            LutPreset::Noir => "noir",
            LutPreset::Sepia => "sepia",
            LutPreset::Cyano => "cyano",
        }
    }

    /// From the config spelling.
    pub fn from_name(name: &str) -> Option<LutPreset> {
        LutPreset::ALL.into_iter().find(|p| p.name() == name)
    }
}

impl NativeEffect {
    /// The effects a mode enables, each from its fields, in the 1.x order:
    /// deband, then (native engine) Anime4K → LumaSharpen → CRT, then CAS,
    /// bloom, vignette, grain, LUT. Front P decides where each one runs
    /// (before or after scaling); this is only the list.
    pub fn from_mode(mode: &ModeSettings) -> Vec<NativeEffect> {
        let mut out = Vec::new();
        if mode.deband_enabled {
            out.push(NativeEffect::Deband { strength: mode.deband_strength });
        }
        if mode.effects_engine == lirtup_config::EffectsEngine::Native {
            if mode.anime4k_enabled {
                out.push(NativeEffect::Anime4kS);
            }
            if mode.luma_sharpen_enabled {
                out.push(NativeEffect::LumaSharpen {
                    strength: mode.luma_sharpen_strength,
                    clamp: mode.luma_sharpen_clamp,
                    pattern: mode.luma_sharpen_pattern,
                    offset_bias: mode.luma_sharpen_offset_bias,
                });
            }
            if mode.crt_enabled {
                out.push(NativeEffect::CrtEasymode(CrtParams::from_mode(mode)));
            }
        }
        if mode.cas_enabled {
            out.push(NativeEffect::Cas { strength: mode.cas_strength });
        }
        if mode.bloom_enabled {
            out.push(NativeEffect::Bloom {
                strength: mode.bloom_strength,
                threshold: mode.bloom_threshold,
                radius: mode.bloom_radius,
            });
        }
        if mode.vignette_enabled {
            out.push(NativeEffect::Vignette {
                strength: mode.vignette_strength,
                radius: mode.vignette_radius,
                falloff: mode.vignette_falloff,
            });
        }
        if mode.grain_enabled {
            out.push(NativeEffect::Grain { strength: mode.grain_strength, size: mode.grain_size });
        }
        if mode.lut_enabled {
            let preset = LutPreset::from_name(&mode.lut_preset).unwrap_or_else(|| {
                tracing::warn!("unknown LUT preset '{}', using identity", mode.lut_preset);
                LutPreset::Identity
            });
            out.push(NativeEffect::Lut { intensity: mode.lut_intensity, preset });
        }
        out
    }

    /// Output size for an input size (`Anime4kS` doubles, CRT takes the
    /// final size, the rest keep it).
    pub fn output_size(&self, input: (u32, u32), final_size: (u32, u32)) -> (u32, u32) {
        match self {
            NativeEffect::Anime4kS => (input.0 * 2, input.1 * 2),
            NativeEffect::CrtEasymode(_) => (final_size.0.max(1), final_size.1.max(1)),
            _ => input,
        }
    }

    /// Whether the pass writes its input in place (its output is the input).
    pub fn in_place(&self) -> bool {
        matches!(
            self,
            NativeEffect::Cas { .. }
                | NativeEffect::Bloom { .. }
                | NativeEffect::Vignette { .. }
                | NativeEffect::Grain { .. }
                | NativeEffect::Lut { .. }
        )
    }

    /// Name for logs and timestamps (the 1.x labels).
    pub fn label(&self) -> &'static str {
        match self {
            NativeEffect::Cas { .. } => "cas",
            NativeEffect::Deband { .. } => "deband",
            NativeEffect::Bloom { .. } => "bloom",
            NativeEffect::Vignette { .. } => "vignette",
            NativeEffect::Grain { .. } => "grain",
            NativeEffect::Lut { .. } => "lut",
            NativeEffect::LumaSharpen { .. } => "luma_sharpen",
            NativeEffect::Anime4kS => "anime4k",
            NativeEffect::CrtEasymode(_) => "crt_easymode",
        }
    }

    /// Change one parameter by its config field name (`cas_strength`,
    /// `bloom_radius`, `crt_gamma_input`, ...). Returns false for names this
    /// effect does not have.
    pub fn set_param(&mut self, name: &str, value: f64) -> bool {
        let f = value as f32;
        let u = value.max(0.0).round() as u32;
        match (self, name) {
            (NativeEffect::Cas { strength }, "cas_strength")
            | (NativeEffect::Deband { strength }, "deband_strength")
            | (NativeEffect::Bloom { strength, .. }, "bloom_strength")
            | (NativeEffect::Vignette { strength, .. }, "vignette_strength")
            | (NativeEffect::Grain { strength, .. }, "grain_strength")
            | (NativeEffect::LumaSharpen { strength, .. }, "luma_sharpen_strength") => *strength = f,
            (NativeEffect::Bloom { threshold, .. }, "bloom_threshold") => *threshold = f,
            (NativeEffect::Bloom { radius, .. }, "bloom_radius") => *radius = u,
            (NativeEffect::Vignette { radius, .. }, "vignette_radius") => *radius = f,
            (NativeEffect::Vignette { falloff, .. }, "vignette_falloff") => *falloff = f,
            (NativeEffect::Grain { size, .. }, "grain_size") => *size = f,
            (NativeEffect::Lut { intensity, .. }, "lut_intensity") => *intensity = f,
            (NativeEffect::LumaSharpen { clamp, .. }, "luma_sharpen_clamp") => *clamp = f,
            (NativeEffect::LumaSharpen { pattern, .. }, "luma_sharpen_pattern") => *pattern = u,
            (NativeEffect::LumaSharpen { offset_bias, .. }, "luma_sharpen_offset_bias") => *offset_bias = f,
            (NativeEffect::CrtEasymode(p), name) => return p.set(name, value),
            _ => return false,
        }
        true
    }

    /// Create the pass.
    pub fn create(&self, device: &Arc<Device>, input: &Texture, output_size: (u32, u32)) -> Result<Box<dyn Pass>> {
        Ok(Box::new(NativePass::new(device, *self, input, output_size)?))
    }
}

fn spirv(path: &str) -> Result<&'static [u8]> {
    crate::assets::native_spirv(path)
        .ok_or_else(|| EffectError::Asset { name: path.to_owned(), message: "not embedded".to_owned() })
}

/// One dispatch of a native pass.
struct Step {
    pipeline: ComputePipeline,
    groups: [u32; 3],
}

/// The pass behind every [`NativeEffect`].
struct NativePass {
    effect: NativeEffect,
    input_size: (u32, u32),
    output: Texture,
    cb: Buffer,
    steps: Vec<Step>,
    frame: u64,
    _keep: Vec<Texture>,
}

impl NativePass {
    fn new(device: &Arc<Device>, effect: NativeEffect, input: &Texture, output_size: (u32, u32)) -> Result<NativePass> {
        let label = effect.label();
        let size = (input.width(), input.height());
        let cb = util::constant_buffer(device, 112, &format!("{label} cb"))?;
        let mut keep = Vec::new();
        let new_output = |device: &Arc<Device>, s: (u32, u32)| {
            util::texture(device, s.0, s.1, Format::R8G8B8A8Unorm, &format!("{label} output"))
        };
        let single = |pipeline: ComputePipeline, out: (u32, u32)| vec![Step { pipeline, groups: util::groups(out, (16, 16)) }];

        let (output, steps) = match effect {
            NativeEffect::Cas { .. }
            | NativeEffect::Bloom { .. }
            | NativeEffect::Vignette { .. }
            | NativeEffect::Grain { .. } => {
                let path = match effect {
                    NativeEffect::Cas { .. } => "effects/cas/cas.spv",
                    NativeEffect::Bloom { .. } => "effects/bloom/bloom.spv",
                    NativeEffect::Vignette { .. } => "effects/vignette/vignette.spv",
                    _ => "effects/grain/grain.spv",
                };
                let p = util::compute(
                    device,
                    spirv(path)?,
                    "main",
                    &Bindings { cb: Some(&cb), srv: &[input], uav: &[input], samplers: &[], push_size: 0 },
                    label,
                )?;
                (input.clone(), single(p, size))
            }
            NativeEffect::Lut { preset, .. } => {
                let n = crate::lut::LUT_SIZE;
                let lut = util::texture_array(device, n, n, n, true, Format::R8G8B8A8Unorm, "lut table")?;
                util::upload_texture(device, &lut, &crate::lut::lut_data(preset))?;
                let linear = util::sampler(device, Filter::Linear)?;
                let p = util::compute(
                    device,
                    spirv("effects/lut/lut.spv")?,
                    "main",
                    &Bindings { cb: Some(&cb), srv: &[input, &lut], uav: &[input], samplers: &[&linear], push_size: 0 },
                    label,
                )?;
                keep.push(lut);
                (input.clone(), single(p, size))
            }
            NativeEffect::Deband { .. } => {
                let out = new_output(device, size)?;
                let p = util::compute(
                    device,
                    spirv("effects/deband/deband.spv")?,
                    "main",
                    &Bindings { cb: Some(&cb), srv: &[input], uav: &[&out], samplers: &[], push_size: 0 },
                    label,
                )?;
                (out, single(p, size))
            }
            NativeEffect::LumaSharpen { .. } | NativeEffect::CrtEasymode(_) => {
                let (path, filter, out_size) = match effect {
                    NativeEffect::LumaSharpen { .. } => ("effects/luma_sharpen/luma_sharpen.spv", Filter::Linear, size),
                    _ => ("effects/crt_easymode/crt_easymode.spv", Filter::Point, effect.output_size(size, output_size)),
                };
                let out = new_output(device, out_size)?;
                let smp = util::sampler(device, filter)?;
                let p = util::compute(
                    device,
                    spirv(path)?,
                    "main",
                    &Bindings { cb: Some(&cb), srv: &[input], uav: &[&out], samplers: &[&smp], push_size: 0 },
                    label,
                )?;
                (out, single(p, out_size))
            }
            NativeEffect::Anime4kS => {
                let tex1 = util::texture(device, size.0, size.1, Format::R16G16B16A16Float, "anime4k tex1")?;
                let tex2 = util::texture(device, size.0, size.1, Format::R16G16B16A16Float, "anime4k tex2")?;
                let out_size = (size.0 * 2, size.1 * 2);
                let out = new_output(device, out_size)?;
                let point = util::sampler(device, Filter::Point)?;
                let linear = util::sampler(device, Filter::Linear)?;
                let pass = |i: usize, srv: &[&Texture], uav: &Texture, samplers: &[&lirtup_gpu::Sampler]| {
                    util::compute(
                        device,
                        spirv(&format!("effects/anime4k/anime4k_upscale_s_pass{i}.spv"))?,
                        "main",
                        &Bindings { cb: Some(&cb), srv, uav: &[uav], samplers, push_size: 0 },
                        label,
                    )
                };
                let steps = vec![
                    Step { pipeline: pass(1, &[input], &tex1, &[&point])?, groups: util::groups(size, (16, 16)) },
                    Step { pipeline: pass(2, &[&tex1], &tex2, &[&point])?, groups: util::groups(size, (16, 16)) },
                    Step { pipeline: pass(3, &[&tex2], &tex1, &[&point])?, groups: util::groups(size, (16, 16)) },
                    Step {
                        pipeline: pass(4, &[input, &tex1], &out, &[&point, &linear])?,
                        groups: util::groups(out_size, (16, 16)),
                    },
                ];
                keep.extend([tex1, tex2]);
                (out, steps)
            }
        };

        let pass = NativePass { effect, input_size: size, output, cb, steps, frame: 0, _keep: keep };
        pass.upload()?;
        Ok(pass)
    }

    fn upload(&self) -> Result<()> {
        let (w, h) = self.input_size;
        let (ow, oh) = (self.output.width(), self.output.height());
        let frame = self.frame as u32;
        let sizes = || {
            CbWriter::new()
                .u32(w)
                .u32(h)
                .u32(ow)
                .u32(oh)
                .f32(1.0 / w as f32)
                .f32(1.0 / h as f32)
                .f32(1.0 / ow as f32)
                .f32(1.0 / oh as f32)
        };
        let cb = match self.effect {
            NativeEffect::Cas { strength } => CbWriter::new().f32(clampf(strength, 0.0, 1.0)).u32(ow).u32(oh).u32(0),
            NativeEffect::Deband { strength } => CbWriter::new().f32(clampf(strength, 0.0, 1.0)).u32(ow).u32(oh).u32(frame),
            NativeEffect::Bloom { strength, threshold, radius } => CbWriter::new()
                .f32(clampf(strength, 0.0, 1.0))
                .u32(ow)
                .u32(oh)
                .f32(clampf(threshold, 0.0, 1.0))
                .u32(radius.clamp(1, 16)),
            NativeEffect::Vignette { strength, radius, falloff } => CbWriter::new()
                .f32(clampf(strength, 0.0, 1.0))
                .u32(ow)
                .u32(oh)
                .f32(clampf(radius, 0.0, 2.0))
                .f32(clampf(falloff, 0.1, 10.0)),
            NativeEffect::Grain { strength, size } => {
                CbWriter::new().f32(clampf(strength, 0.0, 1.0)).u32(ow).u32(oh).u32(frame).f32(size.max(1.0))
            }
            NativeEffect::Lut { intensity, .. } => {
                CbWriter::new().f32(clampf(intensity, 0.0, 1.0)).u32(crate::lut::LUT_SIZE).u32(ow).u32(oh)
            }
            NativeEffect::LumaSharpen { strength, clamp, pattern, offset_bias } => sizes()
                .f32(clampf(strength, 0.0, 3.0))
                .f32(clampf(clamp, 0.0, 1.0))
                .i32(pattern.min(3) as i32)
                .f32(clampf(offset_bias, 0.0, 6.0)),
            NativeEffect::Anime4kS => sizes(),
            NativeEffect::CrtEasymode(p) => p.pack(sizes()).u32(0).u32(0).u32(0),
        };
        self.cb.write(0, &cb.finish())?;
        Ok(())
    }
}

impl Pass for NativePass {
    fn label(&self) -> &str {
        self.effect.label()
    }

    fn output(&self) -> &Texture {
        &self.output
    }

    fn record(&self, recorder: &mut CommandRecorder) -> Result<()> {
        for step in &self.steps {
            recorder.dispatch(&Dispatch {
                pipeline: &step.pipeline,
                groups: step.groups,
                push: &[],
                timestamp: Some(self.effect.label()),
            });
        }
        Ok(())
    }

    fn set_params(&mut self, params: &ParamValues) -> Result<()> {
        let mut changed = false;
        for (name, value) in params {
            changed |= self.effect.set_param(name, *value);
        }
        if changed { self.upload() } else { Ok(()) }
    }

    fn set_frame(&mut self, frame: u64, _time_s: f32) {
        // Only deband and grain animate (temporal dither and noise).
        if matches!(self.effect, NativeEffect::Deband { .. } | NativeEffect::Grain { .. }) {
            self.frame = frame;
            if let Err(e) = self.upload() {
                tracing::warn!("{}: cannot update constants: {e}", self.effect.label());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_mode_keeps_the_1x_order() {
        let mut mode = ModeSettings::default();
        mode.deband_enabled = true;
        mode.anime4k_enabled = true;
        mode.crt_enabled = true;
        mode.cas_enabled = true;
        mode.grain_enabled = true;
        mode.lut_enabled = true;
        mode.lut_preset = "sepia".to_owned();
        mode.effects_engine = lirtup_config::EffectsEngine::Native;
        let labels: Vec<_> = NativeEffect::from_mode(&mode).iter().map(|e| e.label()).collect();
        assert_eq!(labels, ["deband", "anime4k", "crt_easymode", "cas", "grain", "lut"]);

        mode.effects_engine = lirtup_config::EffectsEngine::Magpiefx;
        let labels: Vec<_> = NativeEffect::from_mode(&mode).iter().map(|e| e.label()).collect();
        assert_eq!(labels, ["deband", "cas", "grain", "lut"]);
    }

    #[test]
    fn output_sizes() {
        let crt = NativeEffect::CrtEasymode(CrtParams::default());
        assert_eq!(NativeEffect::Anime4kS.output_size((640, 360), (1920, 1080)), (1280, 720));
        assert_eq!(crt.output_size((640, 360), (1920, 1080)), (1920, 1080));
        assert_eq!(NativeEffect::Cas { strength: 0.4 }.output_size((640, 360), (1920, 1080)), (640, 360));
    }

    #[test]
    fn set_param_by_config_name() {
        let mut e = NativeEffect::Bloom { strength: 0.1, threshold: 0.8, radius: 4 };
        assert!(e.set_param("bloom_radius", 6.0));
        assert!(!e.set_param("cas_strength", 1.0));
        assert_eq!(e, NativeEffect::Bloom { strength: 0.1, threshold: 0.8, radius: 6 });
        let mut crt = NativeEffect::CrtEasymode(CrtParams::default());
        assert!(crt.set_param("crt_gamma_input", 2.4));
    }

    #[test]
    fn crt_constants_are_28_words() {
        let words = CrtParams::default().pack(CbWriter::new().f32s(&[0.0; 8])).u32(0).u32(0).u32(0);
        assert_eq!(words.len(), 112);
    }
}
