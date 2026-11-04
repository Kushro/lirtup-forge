//! Native effects (`shaders/effects/`): CAS, deband, bloom, vignette, grain,
//! LUT.
//!
//! CAS, bloom, vignette, grain and LUT work in place, as in 1.x (the input is
//! bound as SRV and UAV and is also the output). Deband writes a new texture.
//! Parameters are clamped to the 1.x ranges. Every native effect keeps its
//! input's size.
//!
//! The hand ports of the retired native effects engine (Anime4K_Upscale_S,
//! LumaSharpen, CRT-Easymode) are no longer here: the app runs those effects
//! through MagpieFX, and the ports live on only as parity references in
//! `lirtup_effects::ports` (feature `ports`, tests only).

use std::sync::Arc;

use lirtup_config::ModeSettings;
use lirtup_gpu::{Buffer, CommandRecorder, ComputePipeline, Device, Dispatch, Filter, Format, Texture};

use crate::util::{self, Bindings, CbWriter, clampf};
use crate::{EffectError, ParamValues, Pass, Result};

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
    /// deband, CAS, bloom, vignette, grain, LUT (the MagpieFX chain runs
    /// between deband and CAS, built by the presenter). Front P decides where
    /// each one runs (before or after scaling); this is only the list.
    pub fn from_mode(mode: &ModeSettings) -> Vec<NativeEffect> {
        let mut out = Vec::new();
        if mode.deband_enabled {
            out.push(NativeEffect::Deband {
                strength: mode.deband_strength,
            });
        }
        if mode.cas_enabled {
            out.push(NativeEffect::Cas {
                strength: mode.cas_strength,
            });
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
            out.push(NativeEffect::Grain {
                strength: mode.grain_strength,
                size: mode.grain_size,
            });
        }
        if mode.lut_enabled {
            let preset = LutPreset::from_name(&mode.lut_preset).unwrap_or_else(|| {
                tracing::warn!("unknown LUT preset '{}', using identity", mode.lut_preset);
                LutPreset::Identity
            });
            out.push(NativeEffect::Lut {
                intensity: mode.lut_intensity,
                preset,
            });
        }
        out
    }

    /// Output size for an input size: every native effect keeps it
    /// (`final_size` is the size the content is shown at; no native effect
    /// renders at it since the ports went to `lirtup_effects::ports`).
    pub fn output_size(&self, input: (u32, u32), final_size: (u32, u32)) -> (u32, u32) {
        let _ = final_size;
        input
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
        }
    }

    /// Change one parameter by its config field name (`cas_strength`,
    /// `bloom_radius`, ...). Returns false for names this effect does not
    /// have.
    pub fn set_param(&mut self, name: &str, value: f64) -> bool {
        let f = value as f32;
        let u = value.max(0.0).round() as u32;
        match (self, name) {
            (NativeEffect::Cas { strength }, "cas_strength")
            | (NativeEffect::Deband { strength }, "deband_strength")
            | (NativeEffect::Bloom { strength, .. }, "bloom_strength")
            | (NativeEffect::Vignette { strength, .. }, "vignette_strength")
            | (NativeEffect::Grain { strength, .. }, "grain_strength") => *strength = f,
            (NativeEffect::Bloom { threshold, .. }, "bloom_threshold") => *threshold = f,
            (NativeEffect::Bloom { radius, .. }, "bloom_radius") => *radius = u,
            (NativeEffect::Vignette { radius, .. }, "vignette_radius") => *radius = f,
            (NativeEffect::Vignette { falloff, .. }, "vignette_falloff") => *falloff = f,
            (NativeEffect::Grain { size, .. }, "grain_size") => *size = f,
            (NativeEffect::Lut { intensity, .. }, "lut_intensity") => *intensity = f,
            _ => return false,
        }
        true
    }

    /// Create the pass. `output_size` is what [`NativeEffect::output_size`]
    /// gave (the input's size: every native effect keeps it).
    pub fn create(
        &self,
        device: &Arc<Device>,
        input: &Texture,
        output_size: (u32, u32),
    ) -> Result<Box<dyn Pass>> {
        let _ = output_size;
        Ok(Box::new(NativePass::new(device, *self, input)?))
    }
}

/// An embedded native `.spv` by its path, or an error naming it.
pub(crate) fn spirv(path: &str) -> Result<&'static [u8]> {
    crate::assets::native_spirv(path).ok_or_else(|| EffectError::Asset {
        name: path.to_owned(),
        message: "not embedded".to_owned(),
    })
}

/// One dispatch of a native pass.
struct Step {
    pipeline: ComputePipeline,
    groups: [u32; 3],
}

/// The pass behind every [`NativeEffect`].
struct NativePass {
    effect: NativeEffect,
    output: Texture,
    cb: Buffer,
    steps: Vec<Step>,
    frame: u64,
    _keep: Vec<Texture>,
}

impl NativePass {
    fn new(device: &Arc<Device>, effect: NativeEffect, input: &Texture) -> Result<NativePass> {
        let label = effect.label();
        let size = (input.width(), input.height());
        let cb = util::constant_buffer(device, 112, &format!("{label} cb"))?;
        let mut keep = Vec::new();
        let new_output = |device: &Arc<Device>, s: (u32, u32)| {
            util::texture(
                device,
                s.0,
                s.1,
                Format::R8G8B8A8Unorm,
                &format!("{label} output"),
            )
        };
        let single = |pipeline: ComputePipeline, out: (u32, u32)| {
            vec![Step {
                pipeline,
                groups: util::groups(out, (16, 16)),
            }]
        };

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
                    &Bindings {
                        cb: Some(&cb),
                        srv: &[input],
                        uav: &[input],
                        samplers: &[],
                        push_size: 0,
                    },
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
                    &Bindings {
                        cb: Some(&cb),
                        srv: &[input, &lut],
                        uav: &[input],
                        samplers: &[&linear],
                        push_size: 0,
                    },
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
                    &Bindings {
                        cb: Some(&cb),
                        srv: &[input],
                        uav: &[&out],
                        samplers: &[],
                        push_size: 0,
                    },
                    label,
                )?;
                (out, single(p, size))
            }
        };

        let pass = NativePass {
            effect,
            output,
            cb,
            steps,
            frame: 0,
            _keep: keep,
        };
        pass.upload()?;
        Ok(pass)
    }

    fn upload(&self) -> Result<()> {
        let (ow, oh) = (self.output.width(), self.output.height());
        let frame = self.frame as u32;
        let cb = match self.effect {
            NativeEffect::Cas { strength } => CbWriter::new()
                .f32(clampf(strength, 0.0, 1.0))
                .u32(ow)
                .u32(oh)
                .u32(0),
            NativeEffect::Deband { strength } => CbWriter::new()
                .f32(clampf(strength, 0.0, 1.0))
                .u32(ow)
                .u32(oh)
                .u32(frame),
            NativeEffect::Bloom {
                strength,
                threshold,
                radius,
            } => CbWriter::new()
                .f32(clampf(strength, 0.0, 1.0))
                .u32(ow)
                .u32(oh)
                .f32(clampf(threshold, 0.0, 1.0))
                .u32(radius.clamp(1, 16)),
            NativeEffect::Vignette {
                strength,
                radius,
                falloff,
            } => CbWriter::new()
                .f32(clampf(strength, 0.0, 1.0))
                .u32(ow)
                .u32(oh)
                .f32(clampf(radius, 0.0, 2.0))
                .f32(clampf(falloff, 0.1, 10.0)),
            NativeEffect::Grain { strength, size } => CbWriter::new()
                .f32(clampf(strength, 0.0, 1.0))
                .u32(ow)
                .u32(oh)
                .u32(frame)
                .f32(size.max(1.0)),
            NativeEffect::Lut { intensity, .. } => CbWriter::new()
                .f32(clampf(intensity, 0.0, 1.0))
                .u32(crate::lut::LUT_SIZE)
                .u32(ow)
                .u32(oh),
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
        if matches!(
            self.effect,
            NativeEffect::Deband { .. } | NativeEffect::Grain { .. }
        ) {
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
        let mode = ModeSettings {
            deband_enabled: true,
            cas_enabled: true,
            bloom_enabled: true,
            vignette_enabled: true,
            grain_enabled: true,
            lut_enabled: true,
            lut_preset: "sepia".to_owned(),
            ..ModeSettings::default()
        };
        let effects = NativeEffect::from_mode(&mode);
        let labels: Vec<_> = effects.iter().map(|e| e.label()).collect();
        assert_eq!(labels, ["deband", "cas", "bloom", "vignette", "grain", "lut"]);
        assert!(matches!(
            effects.last(),
            Some(NativeEffect::Lut {
                preset: LutPreset::Sepia,
                ..
            })
        ));

        let mode = ModeSettings {
            deband_enabled: true,
            grain_enabled: true,
            ..ModeSettings::default()
        };
        let labels: Vec<_> = NativeEffect::from_mode(&mode).iter().map(|e| e.label()).collect();
        assert_eq!(labels, ["deband", "grain"]);
        assert!(NativeEffect::from_mode(&ModeSettings::default()).is_empty());
    }

    #[test]
    fn output_sizes() {
        for effect in [
            NativeEffect::Cas { strength: 0.4 },
            NativeEffect::Deband { strength: 0.5 },
            NativeEffect::Lut {
                intensity: 1.0,
                preset: LutPreset::Identity,
            },
        ] {
            assert_eq!(effect.output_size((640, 360), (1920, 1080)), (640, 360));
        }
        assert!(NativeEffect::Cas { strength: 0.4 }.in_place());
        assert!(!NativeEffect::Deband { strength: 0.5 }.in_place());
    }

    #[test]
    fn set_param_by_config_name() {
        let mut e = NativeEffect::Bloom {
            strength: 0.1,
            threshold: 0.8,
            radius: 4,
        };
        assert!(e.set_param("bloom_radius", 6.0));
        assert!(!e.set_param("cas_strength", 1.0));
        assert_eq!(
            e,
            NativeEffect::Bloom {
                strength: 0.1,
                threshold: 0.8,
                radius: 6
            }
        );
        let mut cas = NativeEffect::Cas { strength: 0.4 };
        assert!(!cas.set_param("crt_gamma_input", 2.4));
        assert!(!cas.set_param("luma_sharpen_strength", 1.0));
    }
}
