//! Final resamplers (`shaders/scalers/`): copy, Catmull-Rom, Lanczos, FSR,
//! NIS.
//!
//! As in 1.x `presenter._scale`: Copy, Catmull-Rom, Lanczos and FSR read
//! linear light, so the scaler first linearizes its input into an RGBA16F
//! texture and writes linear light into the output (the background outside
//! `dst_rect` is drawn by the shader); NIS reads and writes sRGB and draws
//! only `dst_rect`, so the output is cleared to the background first. The
//! output is RGBA16F at `output_size` (the 1.x `screen_tex`); the presenter
//! follows with `Delinearize` when [`Scaler::linear_output`] is true and with
//! `Dither` otherwise.

use std::sync::Arc;

use lirtup_gpu::{Buffer, CommandRecorder, ComputePipeline, Device, Dispatch, Filter, Format, Texture};

use crate::util::{self, Bindings, CbWriter};
use crate::{EffectError, Pass, Result};

/// Which resampler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalerKind {
    /// 1:1 copy (same size).
    Copy,
    /// Catmull-Rom (downsampling).
    CatmullRom,
    /// Lanczos (up or adaptive down).
    Lanczos,
    /// FSR 1.0 EASU + RCAS (upsampling).
    Fsr,
    /// NVIDIA Image Scaling (upsampling).
    Nis,
}

impl ScalerKind {
    /// Whether the shader reads linear light (all but NIS).
    pub fn linear(self) -> bool {
        self != ScalerKind::Nis
    }

    fn label(self) -> &'static str {
        match self {
            ScalerKind::Copy => "copy",
            ScalerKind::CatmullRom => "catmull_rom",
            ScalerKind::Lanczos => "lanczos",
            ScalerKind::Fsr => "fsr",
            ScalerKind::Nis => "nis",
        }
    }
}

/// Resampler parameters (from `ModeSettings`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalerParams {
    /// Kernel width (`blur`).
    pub blur: f32,
    /// `antiring_strength`.
    pub antiring_strength: f32,
    /// `tight_antiring`.
    pub tight_antiring: bool,
    /// `kernel_radius` (`None` = automatic).
    pub kernel_radius: Option<u32>,
}

/// Lanczos radius cap of 1.x.
const MAX_LANCZOS_RADIUS: u32 = 10;
/// NIS output block (`NIS_BLOCK_WIDTH` x `NIS_BLOCK_HEIGHT`).
const NIS_BLOCK: (u32, u32) = (32, 24);
/// Default NIS sharpness (1.x `NISScaler(sharpness=0.5)`).
const NIS_DEFAULT_SHARPNESS: f32 = 0.5;

/// A resampler from the input size to `output_size`, with the content placed
/// at `dst_rect` inside the output (letterbox, offset and zoom are done here).
pub struct Scaler {
    kind: ScalerKind,
    input_size: (u32, u32),
    output: Texture,
    params: ScalerParams,
    dst_rect: (i32, i32, u32, u32),
    background: [f32; 4],
    nis_sharpness: f32,
    cb: Buffer,
    linearize: Option<(ComputePipeline, Texture)>,
    /// Main pipeline; for Lanczos the adaptive variant.
    pipeline: ComputePipeline,
    /// Lanczos radius-2 fast path.
    lanczos_fixed: Option<ComputePipeline>,
    use_fixed: bool,
    _nis_coefs: Vec<Texture>,
}

impl Scaler {
    /// Create. `dst_rect` is `(x, y, width, height)` inside `output_size`.
    pub fn new(
        device: &Arc<Device>,
        kind: ScalerKind,
        input: &Texture,
        output_size: (u32, u32),
        dst_rect: (i32, i32, u32, u32),
        params: &ScalerParams,
    ) -> Result<Scaler> {
        let label = kind.label();
        let spirv = |path: &str| {
            crate::assets::native_spirv(path)
                .ok_or_else(|| EffectError::Asset { name: path.to_owned(), message: "not embedded".to_owned() })
        };
        let output =
            util::texture(device, output_size.0, output_size.1, Format::R16G16B16A16Float, &format!("{label} output"))?;
        // Big enough for every layout (FSR: 112 bytes).
        let cb = util::constant_buffer(device, 112, &format!("{label} cb"))?;

        let linearize = if kind.linear() {
            let linear = util::texture(
                device,
                input.width(),
                input.height(),
                Format::R16G16B16A16Float,
                &format!("{label} linear input"),
            )?;
            let pipeline = util::compute(
                device,
                spirv("converters/linearize/linearize.spv")?,
                "main",
                &Bindings { cb: None, srv: &[input], uav: &[&linear], samplers: &[], push_size: 0 },
                "linearize",
            )?;
            Some((pipeline, linear))
        } else {
            None
        };
        let source = linearize.as_ref().map(|(_, t)| t).unwrap_or(input);

        let mut nis_coefs = Vec::new();
        let mut lanczos_fixed = None;
        let pipeline = match kind {
            ScalerKind::Copy => util::compute(
                device,
                spirv("scalers/copy/copy.spv")?,
                "main",
                &Bindings { cb: Some(&cb), srv: &[source], uav: &[&output], samplers: &[], push_size: 0 },
                label,
            )?,
            ScalerKind::CatmullRom => {
                let linear = util::sampler(device, Filter::Linear)?;
                util::compute(
                    device,
                    spirv("scalers/catmull_rom/catmull_rom.spv")?,
                    "main",
                    &Bindings { cb: Some(&cb), srv: &[source], uav: &[&output], samplers: &[&linear], push_size: 0 },
                    label,
                )?
            }
            ScalerKind::Lanczos => {
                let point = util::sampler(device, Filter::Point)?;
                lanczos_fixed = Some(util::compute(
                    device,
                    spirv("scalers/lanczos/lanczos_fixed.spv")?,
                    "main",
                    &Bindings { cb: Some(&cb), srv: &[source], uav: &[&output], samplers: &[&point], push_size: 0 },
                    "lanczos fixed",
                )?);
                util::compute(
                    device,
                    spirv("scalers/lanczos/lanczos_adaptive.spv")?,
                    "main",
                    &Bindings { cb: Some(&cb), srv: &[source], uav: &[&output], samplers: &[], push_size: 0 },
                    label,
                )?
            }
            ScalerKind::Fsr => {
                let linear = util::sampler(device, Filter::Linear)?;
                util::compute(
                    device,
                    spirv("scalers/fsr/fsr.spv")?,
                    "main",
                    &Bindings { cb: Some(&cb), srv: &[source], uav: &[&output], samplers: &[&linear], push_size: 0 },
                    label,
                )?
            }
            ScalerKind::Nis => {
                let scale = coef_texture(device, &nis::COEF_SCALE, "nis coef_scaler")?;
                let usm = coef_texture(device, &nis::COEF_USM, "nis coef_usm")?;
                let linear = util::sampler(device, Filter::Linear)?;
                let p = util::compute(
                    device,
                    spirv("scalers/nis/nis.spv")?,
                    "main",
                    &Bindings {
                        cb: Some(&cb),
                        srv: &[source, &scale, &usm],
                        uav: &[&output],
                        samplers: &[&linear],
                        push_size: 0,
                    },
                    label,
                )?;
                nis_coefs = vec![scale, usm];
                p
            }
        };

        let mut scaler = Scaler {
            kind,
            input_size: (input.width(), input.height()),
            output,
            params: *params,
            dst_rect,
            background: [0.0, 0.0, 0.0, 1.0],
            nis_sharpness: NIS_DEFAULT_SHARPNESS,
            cb,
            linearize,
            pipeline,
            lanczos_fixed,
            use_fixed: false,
            _nis_coefs: nis_coefs,
        };
        scaler.upload_constants()?;
        Ok(scaler)
    }

    /// Move the content (offset and zoom hotkeys) without recreating.
    pub fn set_dst_rect(&mut self, dst_rect: (i32, i32, u32, u32)) {
        self.dst_rect = dst_rect;
        if let Err(e) = self.upload_constants() {
            tracing::warn!("{}: cannot update constants: {e}", self.kind.label());
        }
    }

    /// Letterbox color, RGBA.
    pub fn set_background(&mut self, rgba: [f32; 4]) {
        self.background = rgba;
        if let Err(e) = self.upload_constants() {
            tracing::warn!("{}: cannot update constants: {e}", self.kind.label());
        }
    }

    /// NIS sharpness, 0 to 1 (default 0.5). Ignored by the other kinds.
    pub fn set_nis_sharpness(&mut self, sharpness: f32) {
        self.nis_sharpness = sharpness;
        if let Err(e) = self.upload_constants() {
            tracing::warn!("{}: cannot update constants: {e}", self.kind.label());
        }
    }

    /// Change blur, anti-ringing and kernel radius without recreating.
    pub fn set_params(&mut self, params: &ScalerParams) {
        self.params = *params;
        if let Err(e) = self.upload_constants() {
            tracing::warn!("{}: cannot update constants: {e}", self.kind.label());
        }
    }

    /// Kind.
    pub fn kind(&self) -> ScalerKind {
        self.kind
    }

    /// Whether the output holds linear light (then `Delinearize` follows;
    /// otherwise `Dither`).
    pub fn linear_output(&self) -> bool {
        self.kind.linear()
    }

    fn upload_constants(&mut self) -> Result<()> {
        let data = scaler_constants(
            self.kind,
            self.input_size,
            (self.output.width(), self.output.height()),
            self.dst_rect,
            self.background,
            &self.params,
            self.nis_sharpness,
        );
        self.use_fixed = data.lanczos_fixed;
        self.cb.write(0, &data.bytes)?;
        Ok(())
    }
}

/// Packed constants and the Lanczos variant they are for.
struct ScalerConstants {
    bytes: Vec<u8>,
    lanczos_fixed: bool,
}

/// Lanczos radii for a scale (`update_constants` of 1.x `LanczosScaler`):
/// `(rx, ry, fixed)`.
fn lanczos_radii(src: (u32, u32), dst: (u32, u32), override_radius: Option<u32>) -> (u32, u32, bool) {
    if let Some(r) = override_radius {
        return (r, r, false);
    }
    let radius = |s: u32, d: u32| {
        let scale = d as f64 / s.max(1) as f64;
        if scale >= 1.0 { 2 } else { ((2.0 / scale).ceil() as u32).min(MAX_LANCZOS_RADIUS) }
    };
    let (rx, ry) = (radius(src.0, dst.0), radius(src.1, dst.1));
    (rx, ry, rx == 2 && ry == 2)
}

fn scaler_constants(
    kind: ScalerKind,
    src: (u32, u32),
    screen: (u32, u32),
    dst: (i32, i32, u32, u32),
    background: [f32; 4],
    params: &ScalerParams,
    nis_sharpness: f32,
) -> ScalerConstants {
    let (dx, dy, dw, dh) = (dst.0, dst.1, dst.2.max(1), dst.3.max(1));
    let common = || {
        CbWriter::new()
            .f32s(&background)
            .u32(src.0)
            .u32(src.1)
            .u32(screen.0)
            .u32(screen.1)
            .i32(dx)
            .i32(dy)
            .i32(dw as i32)
            .i32(dh as i32)
    };
    let mut lanczos_fixed = false;
    let cb = match kind {
        ScalerKind::Copy => common(),
        // 1.x never passes `blur`/`antiring_strength` to Catmull-Rom, so it
        // always runs with its constructor defaults (1.0, 0.8). Kept for
        // parity; see the round 1 note.
        ScalerKind::CatmullRom => common().f32(1.0).f32(0.8),
        ScalerKind::Lanczos => {
            let (rx, ry, fixed) = lanczos_radii(src, (dw, dh), params.kernel_radius);
            lanczos_fixed = fixed;
            if fixed {
                common().f32(params.blur)
            } else {
                common()
                    .u32(rx)
                    .u32(ry)
                    .f32(params.blur)
                    .f32(params.antiring_strength)
                    .u32(params.tight_antiring as u32)
            }
        }
        ScalerKind::Fsr => {
            let scale_x = src.0 as f32 / dw as f32;
            let scale_y = src.1 as f32 / dh as f32;
            let px = 1.0 / src.0 as f32;
            let py = 1.0 / src.1 as f32;
            let easu = [
                scale_x,
                scale_y,
                0.5 * scale_x - 0.5,
                0.5 * scale_y - 0.5,
                px,
                py,
                px,
                -py,
                -px,
                2.0 * py,
                px,
                2.0 * py,
                0.0,
                4.0 * py,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
            ];
            CbWriter::new()
                .f32s(&easu)
                .i32(dx)
                .i32(dy)
                .i32(dw as i32)
                .i32(dh as i32)
                .f32s(&background)
        }
        ScalerKind::Nis => nis::constants(src, dst, nis_sharpness),
    };
    ScalerConstants { bytes: cb.finish(), lanczos_fixed }
}

fn coef_texture(device: &Arc<Device>, coefs: &[[f32; 8]; 64], label: &str) -> Result<Texture> {
    let tex = util::texture(device, 2, 64, Format::R32G32B32A32Float, label)?;
    let bytes: Vec<u8> = coefs.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
    util::upload_texture(device, &tex, &bytes)?;
    Ok(tex)
}

impl Pass for Scaler {
    fn label(&self) -> &str {
        self.kind.label()
    }

    fn output(&self) -> &Texture {
        &self.output
    }

    fn record(&self, recorder: &mut CommandRecorder) -> Result<()> {
        let label = self.kind.label();
        if let Some((pipeline, linear)) = &self.linearize {
            recorder.dispatch(&Dispatch {
                pipeline,
                groups: util::groups((linear.width(), linear.height()), (16, 16)),
                push: &[],
                timestamp: Some(label),
            });
        }
        let screen = (self.output.width(), self.output.height());
        if self.kind == ScalerKind::Nis {
            recorder.clear(&self.output, self.background);
            recorder.dispatch(&Dispatch {
                pipeline: &self.pipeline,
                groups: util::groups((self.dst_rect.2, self.dst_rect.3), NIS_BLOCK),
                push: &[],
                timestamp: Some(label),
            });
            return Ok(());
        }
        let pipeline = match (&self.lanczos_fixed, self.use_fixed) {
            (Some(fixed), true) => fixed,
            _ => &self.pipeline,
        };
        recorder.dispatch(&Dispatch { pipeline, groups: util::groups(screen, (16, 16)), push: &[], timestamp: Some(label) });
        Ok(())
    }
}

/// NIS constants and coefficient tables (`scalers/nis/__init__.py`).
mod nis {
    use crate::util::CbWriter;

    pub(super) fn constants(src: (u32, u32), dst: (i32, i32, u32, u32), sharpness: f32) -> CbWriter {
        let sharpness = sharpness.clamp(0.0, 1.0);
        let (dw, dh) = (dst.2.max(1), dst.3.max(1));
        let slider = sharpness - 0.5;
        let max_scale = if slider >= 0.0 { 1.25 } else { 1.75 };
        let min_scale = if slider >= 0.0 { 1.25 } else { 1.0 };
        let limit_scale = if slider >= 0.0 { 1.25 } else { 1.0 };
        let detect_ratio = 2.0 * 1127.0 / 1024.0;
        let detect_thres = 64.0 / 1024.0;
        let min_contrast = 2.0;
        let max_contrast = 10.0;
        let sharp_start_y = 0.45;
        let sharp_end_y = 0.9;
        let strength_min = (0.4 + slider * min_scale * 1.2f32).max(0.0);
        let strength_max = 1.6 + slider * max_scale * 1.8;
        let limit_min = (0.14 + slider * limit_scale * 0.32f32).max(0.1);
        let limit_max = 0.5 + slider * limit_scale * 0.6;
        CbWriter::new()
            .f32s(&[
                detect_ratio,
                detect_thres,
                min_contrast,
                1.0 / (max_contrast - min_contrast),
                1.0,
                1.0 / 255.0,
                sharp_start_y,
                1.0 / (sharp_end_y - sharp_start_y),
                strength_min,
                strength_max - strength_min,
                limit_min,
                limit_max - limit_min,
                src.0 as f32 / dw as f32,
                src.1 as f32 / dh as f32,
                1.0 / dw as f32,
                1.0 / dh as f32,
                1.0 / src.0 as f32,
                1.0 / src.1 as f32,
            ])
            .u32(0)
            .u32(0)
            .u32(src.0)
            .u32(src.1)
            .u32(dst.0.max(0) as u32)
            .u32(dst.1.max(0) as u32)
            .u32(dw)
            .u32(dh)
            .f32(0.0)
            .f32(0.0)
    }

    include!("nis_coefs.rs");
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAMS: ScalerParams = ScalerParams { blur: 1.0, antiring_strength: 1.0, tight_antiring: true, kernel_radius: None };

    #[test]
    fn lanczos_variant_follows_the_scale() {
        assert_eq!(lanczos_radii((1280, 720), (2560, 1440), None), (2, 2, true));
        assert_eq!(lanczos_radii((2560, 1440), (1280, 720), None), (4, 4, false));
        assert_eq!(lanczos_radii((10000, 100), (100, 100), None), (10, 2, false));
        assert_eq!(lanczos_radii((100, 100), (200, 200), Some(3)), (3, 3, false));
    }

    #[test]
    fn constant_sizes_match_the_shaders() {
        let size = |kind| {
            scaler_constants(kind, (100, 100), (200, 200), (0, 0, 200, 200), [0.0; 4], &PARAMS, 0.5).bytes.len()
        };
        // 1.x struct sizes rounded up to 16
        assert_eq!(size(ScalerKind::Copy), 48); // ffffIIIIiiii = 48
        assert_eq!(size(ScalerKind::CatmullRom), 64); // + ff = 56
        assert_eq!(size(ScalerKind::Lanczos), 64); // fixed: + f = 52
        assert_eq!(size(ScalerKind::Fsr), 112);
        assert_eq!(size(ScalerKind::Nis), 112); // 18f 8I 2f = 112
        let down = scaler_constants(ScalerKind::Lanczos, (400, 400), (200, 200), (0, 0, 200, 200), [0.0; 4], &PARAMS, 0.5);
        assert!(!down.lanczos_fixed);
        assert_eq!(down.bytes.len(), 80); // ffffIIIIiiiiIIffI = 68
    }

    #[test]
    fn nis_tables_have_64_phases() {
        assert_eq!(nis::COEF_SCALE.len(), 64);
        assert_eq!(nis::COEF_USM[32][2], 0.3262);
    }
}
