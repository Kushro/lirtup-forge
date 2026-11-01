//! Color converters (`shaders/converters/`): linearize, delinearize, dither
//! (with the blue noise table).

use std::sync::Arc;

use lirtup_gpu::{CommandRecorder, ComputePipeline, Device, Dispatch, Filter, Format, Texture};

use crate::util::{self, Bindings, CbWriter};
use crate::{EffectError, Pass, Result};

/// Which conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Conversion {
    /// sRGB → linear (into a float format).
    Linearize,
    /// Linear → sRGB.
    Delinearize,
    /// Float → 8 bit with blue noise dithering.
    Dither,
}

/// Dither amplitude of 1.x (one 8-bit step).
const DITHER_STRENGTH: f32 = 1.0 / 255.0;

impl Conversion {
    /// Create the pass writing `output_format`.
    pub fn create(&self, device: &Arc<Device>, input: &Texture, output_format: Format) -> Result<Box<dyn Pass>> {
        Ok(Box::new(ConvertPass::new(device, *self, input, output_format)?))
    }

    fn label(self) -> &'static str {
        match self {
            Conversion::Linearize => "linearize",
            Conversion::Delinearize => "delinearize",
            Conversion::Dither => "dither",
        }
    }

    fn spirv_path(self) -> &'static str {
        match self {
            Conversion::Linearize => "converters/linearize/linearize.spv",
            Conversion::Delinearize => "converters/delinearize/delinearize.spv",
            Conversion::Dither => "converters/dither/dither.spv",
        }
    }
}

struct ConvertPass {
    kind: Conversion,
    pipeline: ComputePipeline,
    output: Texture,
    _noise: Option<Texture>,
}

impl ConvertPass {
    fn new(device: &Arc<Device>, kind: Conversion, input: &Texture, format: Format) -> Result<ConvertPass> {
        let label = kind.label();
        let (w, h) = (input.width(), input.height());
        let output = util::texture(device, w, h, format, &format!("{label} output"))?;
        let spirv = crate::assets::native_spirv(kind.spirv_path())
            .ok_or_else(|| EffectError::Asset { name: kind.spirv_path().to_owned(), message: "not embedded".to_owned() })?;
        let (pipeline, noise) = match kind {
            Conversion::Linearize => (
                util::compute(
                    device,
                    spirv,
                    "main",
                    &Bindings { cb: None, srv: &[input], uav: &[&output], samplers: &[], push_size: 0 },
                    label,
                )?,
                None,
            ),
            Conversion::Delinearize | Conversion::Dither => {
                let noise = blue_noise_texture(device)?;
                let cb = util::constant_buffer_with(device, CbWriter::new().u32(w).u32(h).f32(DITHER_STRENGTH), label)?;
                let point = util::sampler(device, Filter::Point)?;
                let p = util::compute(
                    device,
                    spirv,
                    "main",
                    &Bindings { cb: Some(&cb), srv: &[input, &noise], uav: &[&output], samplers: &[&point], push_size: 0 },
                    label,
                )?;
                (p, Some(noise))
            }
        };
        Ok(ConvertPass { kind, pipeline, output, _noise: noise })
    }
}

/// The 64x64 RGBA8 blue noise texture.
fn blue_noise_texture(device: &Arc<Device>) -> Result<Texture> {
    let tex = util::texture(device, 64, 64, Format::R8G8B8A8Unorm, "blue noise")?;
    util::upload_texture(device, &tex, crate::assets::blue_noise())?;
    Ok(tex)
}

impl Pass for ConvertPass {
    fn label(&self) -> &str {
        self.kind.label()
    }

    fn output(&self) -> &Texture {
        &self.output
    }

    fn record(&self, recorder: &mut CommandRecorder) -> Result<()> {
        recorder.dispatch(&Dispatch {
            pipeline: &self.pipeline,
            groups: util::groups((self.output.width(), self.output.height()), (16, 16)),
            push: &[],
            timestamp: Some(self.kind.label()),
        });
        Ok(())
    }
}
