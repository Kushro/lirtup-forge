//! The hand ports of the retired native effects engine: Anime4K_Upscale_S
//! (four passes, 2x), LumaSharpen and CRT_Easymode (`shaders/effects/
//! {anime4k,luma_sharpen,crt_easymode}/`). Only with the `ports` feature,
//! which only the tests turn on: the app runs these effects through
//! MagpieFX (fx-unify), and its binary embeds none of these `.spv`.
//!
//! Kept as references: `tests/parity.rs` compares them with the 1.x Python
//! ports, and `lirtup-magpiefx/tests/ports.rs` with the same effects run
//! through MagpieFX. Like the MagpieFX chain over captured frames, the ports
//! read BGRA bytes (`shaders/effects/bgra_input.hlsli`) and write BGRA.
//!
//! Parameters are named like the effects' `//!PARAMETER`s in Magpie
//! (`sharpStrength`, `maskDotWidth`, ...: the table of
//! `lirtup_config::legacy`) and clamped to Magpie's ranges, as the native
//! engine did.

use std::sync::Arc;

use lirtup_config::legacy::{ANIME4K_EFFECT, CRT_EFFECT, LUMA_SHARPEN_EFFECT};
use lirtup_gpu::{Buffer, CommandRecorder, ComputePipeline, Device, Dispatch, Filter, Format, Sampler, Texture};

use crate::effects::spirv;
use crate::util::{self, Bindings, CbWriter, clampf};
use crate::{ParamValues, Pass, Result};

/// CRT-Easymode parameters (the old `crt_*` fields, Magpie's
/// `CRT/CRT_Easymode` parameters).
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)] // same names as the old config fields
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

/// The 17 `//!PARAMETER` names of `CRT/CRT_Easymode`, in the order of the
/// constant buffer.
pub const CRT_PARAM_NAMES: [&str; 17] = [
    "sharpnessH",
    "sharpnessV",
    "maskStrength",
    "maskDotWidth",
    "maskDotHeight",
    "maskStagger",
    "maskSize",
    "scanlineStrength",
    "scanlineBeamWidthMin",
    "scanlineBeamWidthMax",
    "scanlineBrightMin",
    "scanlineBrightMax",
    "scanlineCutoff",
    "gammaInput",
    "gammaOutput",
    "brightBoost",
    "dilation",
];

/// The four `//!PARAMETER` names of `Sharpen/LumaSharpen`.
pub const LUMA_SHARPEN_PARAM_NAMES: [&str; 4] = ["sharpStrength", "sharpClamp", "pattern", "offsetBias"];

impl CrtParams {
    /// Every parameter by its HLSL name, in [`CRT_PARAM_NAMES`] order.
    fn values(&self) -> [f64; 17] {
        [
            f64::from(self.sharpness_h),
            f64::from(self.sharpness_v),
            f64::from(self.mask_strength),
            f64::from(self.mask_dot_width),
            f64::from(self.mask_dot_height),
            f64::from(self.mask_stagger),
            f64::from(self.mask_size),
            f64::from(self.scanline_strength),
            f64::from(self.scanline_beam_width_min),
            f64::from(self.scanline_beam_width_max),
            f64::from(self.scanline_bright_min),
            f64::from(self.scanline_bright_max),
            f64::from(self.scanline_cutoff),
            f64::from(self.gamma_input),
            f64::from(self.gamma_output),
            f64::from(self.bright_boost),
            f64::from(self.dilation),
        ]
    }

    /// Set one parameter by its HLSL name (`sharpnessH`, ...).
    fn set(&mut self, name: &str, v: f64) -> bool {
        let f = v as f32;
        let u = v.max(0.0).round() as u32;
        match name {
            "sharpnessH" => self.sharpness_h = f,
            "sharpnessV" => self.sharpness_v = f,
            "maskStrength" => self.mask_strength = f,
            "maskDotWidth" => self.mask_dot_width = u,
            "maskDotHeight" => self.mask_dot_height = u,
            "maskStagger" => self.mask_stagger = u,
            "maskSize" => self.mask_size = u,
            "scanlineStrength" => self.scanline_strength = f,
            "scanlineBeamWidthMin" => self.scanline_beam_width_min = f,
            "scanlineBeamWidthMax" => self.scanline_beam_width_max = f,
            "scanlineBrightMin" => self.scanline_bright_min = f,
            "scanlineBrightMax" => self.scanline_bright_max = f,
            "scanlineCutoff" => self.scanline_cutoff = u,
            "gammaInput" => self.gamma_input = f,
            "gammaOutput" => self.gamma_output = f,
            "brightBoost" => self.bright_boost = f,
            "dilation" => self.dilation = u,
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

/// One of the three hand ports, with its parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PortEffect {
    /// Anime4K_Upscale_S: doubles the size.
    Anime4kS,
    /// LumaSharpen.
    LumaSharpen {
        /// Strength (`sharpStrength`).
        strength: f32,
        /// Clamp (`sharpClamp`).
        clamp: f32,
        /// Pattern 0 to 3 (`pattern`).
        pattern: u32,
        /// Offset bias (`offsetBias`).
        offset_bias: f32,
    },
    /// CRT-Easymode: renders at the size it is given.
    CrtEasymode(CrtParams),
}

impl PortEffect {
    /// LumaSharpen with Magpie's defaults.
    pub const LUMA_SHARPEN_DEFAULT: PortEffect = PortEffect::LumaSharpen {
        strength: 0.65,
        clamp: 0.035,
        pattern: 1,
        offset_bias: 1.0,
    };

    /// The same effect with Magpie's default parameters.
    pub fn with_defaults(&self) -> PortEffect {
        match self {
            PortEffect::Anime4kS => PortEffect::Anime4kS,
            PortEffect::LumaSharpen { .. } => PortEffect::LUMA_SHARPEN_DEFAULT,
            PortEffect::CrtEasymode(_) => PortEffect::CrtEasymode(CrtParams::default()),
        }
    }

    /// Name for logs and timestamps (the labels of the native engine).
    pub fn label(&self) -> &'static str {
        match self {
            PortEffect::Anime4kS => "anime4k",
            PortEffect::LumaSharpen { .. } => "luma_sharpen",
            PortEffect::CrtEasymode(_) => "crt_easymode",
        }
    }

    /// The MagpieFX catalog name of the same effect (the names the
    /// translation of `lirtup_config::legacy` writes).
    pub fn magpie_name(&self) -> &'static str {
        match self {
            PortEffect::Anime4kS => ANIME4K_EFFECT,
            PortEffect::LumaSharpen { .. } => LUMA_SHARPEN_EFFECT,
            PortEffect::CrtEasymode(_) => CRT_EFFECT,
        }
    }

    /// Every parameter, by its MagpieFX (`//!PARAMETER`) name, unclamped.
    /// Anime4K has none.
    pub fn magpie_params(&self) -> ParamValues {
        match self {
            PortEffect::Anime4kS => ParamValues::new(),
            PortEffect::LumaSharpen {
                strength,
                clamp,
                pattern,
                offset_bias,
            } => {
                let values = [
                    f64::from(*strength),
                    f64::from(*clamp),
                    f64::from(*pattern),
                    f64::from(*offset_bias),
                ];
                LUMA_SHARPEN_PARAM_NAMES
                    .iter()
                    .zip(values)
                    .map(|(n, v)| ((*n).to_owned(), v))
                    .collect()
            }
            PortEffect::CrtEasymode(p) => CRT_PARAM_NAMES
                .iter()
                .zip(p.values())
                .map(|(n, v)| ((*n).to_owned(), v))
                .collect(),
        }
    }

    /// Change one parameter by its MagpieFX name. Returns false for names
    /// this effect does not have.
    pub fn set_param(&mut self, name: &str, value: f64) -> bool {
        let f = value as f32;
        let u = value.max(0.0).round() as u32;
        match (self, name) {
            (PortEffect::LumaSharpen { strength, .. }, "sharpStrength") => *strength = f,
            (PortEffect::LumaSharpen { clamp, .. }, "sharpClamp") => *clamp = f,
            (PortEffect::LumaSharpen { pattern, .. }, "pattern") => *pattern = u,
            (PortEffect::LumaSharpen { offset_bias, .. }, "offsetBias") => *offset_bias = f,
            (PortEffect::CrtEasymode(p), name) => return p.set(name, value),
            _ => return false,
        }
        true
    }

    /// Output size for an input size: Anime4K doubles, CRT takes
    /// `final_size` (the content size after the model, as in the native
    /// engine), LumaSharpen keeps it.
    pub fn output_size(&self, input: (u32, u32), final_size: (u32, u32)) -> (u32, u32) {
        match self {
            PortEffect::Anime4kS => (input.0 * 2, input.1 * 2),
            PortEffect::CrtEasymode(_) => (final_size.0.max(1), final_size.1.max(1)),
            PortEffect::LumaSharpen { .. } => input,
        }
    }

    /// Create the pass. `output_size` is only used by CRT (see
    /// [`PortEffect::output_size`]).
    pub fn create(
        &self,
        device: &Arc<Device>,
        input: &Texture,
        output_size: (u32, u32),
    ) -> Result<Box<dyn Pass>> {
        Ok(Box::new(PortPass::new(device, *self, input, output_size)?))
    }
}

/// One dispatch of a port.
struct Step {
    pipeline: ComputePipeline,
    groups: [u32; 3],
}

/// The pass behind every [`PortEffect`] (the old `NativePass` arms).
struct PortPass {
    effect: PortEffect,
    input_size: (u32, u32),
    output: Texture,
    cb: Buffer,
    steps: Vec<Step>,
    _keep: Vec<Texture>,
}

impl PortPass {
    fn new(
        device: &Arc<Device>,
        effect: PortEffect,
        input: &Texture,
        output_size: (u32, u32),
    ) -> Result<PortPass> {
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

        let (output, steps) = match effect {
            PortEffect::LumaSharpen { .. } | PortEffect::CrtEasymode(_) => {
                let (path, filter) = match effect {
                    PortEffect::LumaSharpen { .. } => ("effects/luma_sharpen/luma_sharpen.spv", Filter::Linear),
                    _ => ("effects/crt_easymode/crt_easymode.spv", Filter::Point),
                };
                let out_size = effect.output_size(size, output_size);
                let out = new_output(device, out_size)?;
                let smp = util::sampler(device, filter)?;
                let p = util::compute(
                    device,
                    spirv(path)?,
                    "main",
                    &Bindings {
                        cb: Some(&cb),
                        srv: &[input],
                        uav: &[&out],
                        samplers: &[&smp],
                        push_size: 0,
                    },
                    label,
                )?;
                let steps = vec![Step {
                    pipeline: p,
                    groups: util::groups(out_size, (16, 16)),
                }];
                (out, steps)
            }
            PortEffect::Anime4kS => {
                let tex1 = util::texture(device, size.0, size.1, Format::R16G16B16A16Float, "anime4k tex1")?;
                let tex2 = util::texture(device, size.0, size.1, Format::R16G16B16A16Float, "anime4k tex2")?;
                let out_size = effect.output_size(size, output_size);
                let out = new_output(device, out_size)?;
                let point = util::sampler(device, Filter::Point)?;
                let linear = util::sampler(device, Filter::Linear)?;
                let pass = |i: usize, srv: &[&Texture], uav: &Texture, samplers: &[&Sampler]| {
                    util::compute(
                        device,
                        spirv(&format!("effects/anime4k/anime4k_upscale_s_pass{i}.spv"))?,
                        "main",
                        &Bindings {
                            cb: Some(&cb),
                            srv,
                            uav: &[uav],
                            samplers,
                            push_size: 0,
                        },
                        label,
                    )
                };
                let steps = vec![
                    Step {
                        pipeline: pass(1, &[input], &tex1, &[&point])?,
                        groups: util::groups(size, (16, 16)),
                    },
                    Step {
                        pipeline: pass(2, &[&tex1], &tex2, &[&point])?,
                        groups: util::groups(size, (16, 16)),
                    },
                    Step {
                        pipeline: pass(3, &[&tex2], &tex1, &[&point])?,
                        groups: util::groups(size, (16, 16)),
                    },
                    Step {
                        pipeline: pass(4, &[input, &tex1], &out, &[&point, &linear])?,
                        groups: util::groups(out_size, (16, 16)),
                    },
                ];
                keep.extend([tex1, tex2]);
                (out, steps)
            }
        };

        let pass = PortPass {
            effect,
            input_size: size,
            output,
            cb,
            steps,
            _keep: keep,
        };
        pass.upload()?;
        Ok(pass)
    }

    fn upload(&self) -> Result<()> {
        let cb = constants(&self.effect, self.input_size, (self.output.width(), self.output.height()));
        self.cb.write(0, &cb.finish())?;
        Ok(())
    }
}

/// The constant buffer of a port: the sizes (`_inputSize`, `_outputSize`,
/// `_inputPt`, `_outputPt`), then its parameters.
fn constants(effect: &PortEffect, (w, h): (u32, u32), (ow, oh): (u32, u32)) -> CbWriter {
    let sizes = CbWriter::new()
        .u32(w)
        .u32(h)
        .u32(ow)
        .u32(oh)
        .f32(1.0 / w as f32)
        .f32(1.0 / h as f32)
        .f32(1.0 / ow as f32)
        .f32(1.0 / oh as f32);
    match *effect {
        PortEffect::LumaSharpen {
            strength,
            clamp,
            pattern,
            offset_bias,
        } => sizes
            .f32(clampf(strength, 0.0, 3.0))
            .f32(clampf(clamp, 0.0, 1.0))
            .i32(pattern.min(3) as i32)
            .f32(clampf(offset_bias, 0.0, 6.0)),
        PortEffect::Anime4kS => sizes,
        PortEffect::CrtEasymode(p) => p.pack(sizes).u32(0).u32(0).u32(0),
    }
}

impl Pass for PortPass {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_sizes() {
        let crt = PortEffect::CrtEasymode(CrtParams::default());
        assert_eq!(PortEffect::Anime4kS.output_size((640, 360), (1920, 1080)), (1280, 720));
        assert_eq!(crt.output_size((640, 360), (1920, 1080)), (1920, 1080));
        assert_eq!(crt.output_size((640, 360), (0, 0)), (1, 1));
        assert_eq!(
            PortEffect::LUMA_SHARPEN_DEFAULT.output_size((640, 360), (1920, 1080)),
            (640, 360)
        );
    }

    #[test]
    fn crt_constants_are_28_words() {
        let words = CrtParams::default()
            .pack(CbWriter::new().f32s(&[0.0; 8]))
            .u32(0)
            .u32(0)
            .u32(0);
        assert_eq!(words.len(), 112);
        let crt = PortEffect::CrtEasymode(CrtParams::default());
        assert_eq!(constants(&crt, (160, 96), (320, 192)).len(), 112);
        assert_eq!(constants(&PortEffect::Anime4kS, (160, 96), (320, 192)).len(), 32);
        assert_eq!(constants(&PortEffect::LUMA_SHARPEN_DEFAULT, (160, 96), (160, 96)).len(), 48);
    }

    #[test]
    fn magpie_params_cover_the_hlsl_names() {
        let crt = PortEffect::CrtEasymode(CrtParams::default()).magpie_params();
        assert_eq!(crt.len(), 17);
        for name in CRT_PARAM_NAMES {
            assert!(crt.contains_key(name), "{name}");
        }
        assert_eq!(crt["sharpnessH"], 0.5);
        assert_eq!(crt["scanlineCutoff"], 400.0);
        assert_eq!(crt["dilation"], 1.0);

        let luma = PortEffect::LUMA_SHARPEN_DEFAULT.magpie_params();
        assert_eq!(
            luma.keys().map(String::as_str).collect::<Vec<_>>(),
            ["offsetBias", "pattern", "sharpClamp", "sharpStrength"]
        );
        assert_eq!(luma["pattern"], 1.0);
        assert!(PortEffect::Anime4kS.magpie_params().is_empty());
    }

    #[test]
    fn set_param_by_hlsl_name_round_trips() {
        let mut crt = PortEffect::CrtEasymode(CrtParams::default());
        for (i, name) in CRT_PARAM_NAMES.iter().enumerate() {
            // Small integers: exact in f32 and in u32.
            assert!(crt.set_param(name, (i % 2 + 1) as f64), "{name}");
        }
        let params = crt.magpie_params();
        for (i, name) in CRT_PARAM_NAMES.iter().enumerate() {
            assert_eq!(params[*name], (i % 2 + 1) as f64, "{name}");
        }
        assert!(!crt.set_param("sharpStrength", 1.0));

        let mut luma = PortEffect::LUMA_SHARPEN_DEFAULT;
        assert!(luma.set_param("pattern", 2.4));
        assert!(luma.set_param("sharpStrength", 1.5));
        assert!(!luma.set_param("sharpnessH", 1.0));
        assert_eq!(
            luma,
            PortEffect::LumaSharpen {
                strength: 1.5,
                clamp: 0.035,
                pattern: 2,
                offset_bias: 1.0
            }
        );
        assert!(!PortEffect::Anime4kS.set_param("pattern", 1.0));
    }

    #[test]
    fn names_and_defaults() {
        assert_eq!(PortEffect::Anime4kS.magpie_name(), "Anime4K/Anime4K_Upscale_S");
        assert_eq!(PortEffect::LUMA_SHARPEN_DEFAULT.magpie_name(), "Sharpen/LumaSharpen");
        assert_eq!(
            PortEffect::CrtEasymode(CrtParams::default()).magpie_name(),
            "CRT/CRT_Easymode"
        );
        let tuned = PortEffect::LumaSharpen {
            strength: 2.0,
            clamp: 0.5,
            pattern: 3,
            offset_bias: 4.0,
        };
        assert_eq!(tuned.with_defaults(), PortEffect::LUMA_SHARPEN_DEFAULT);
        assert_eq!(tuned.label(), "luma_sharpen");
        assert_eq!(PortEffect::Anime4kS.label(), "anime4k");
    }
}
