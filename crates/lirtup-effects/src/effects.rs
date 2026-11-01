//! Native effects (`shaders/effects/`): CAS, deband, bloom, vignette, grain,
//! LUT, LumaSharpen, Anime4K S, CRT-Easymode.

use std::sync::Arc;

use lirtup_config::ModeSettings;
use lirtup_gpu::{Device, Texture};

use crate::{Pass, Result};

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

/// Built-in LUT presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LutPreset {
    /// No change.
    #[default]
    Identity,
}

impl NativeEffect {
    /// The effects a mode enables, each from its fields, in the 1.x order:
    /// deband, then (native engine) Anime4K → LumaSharpen → CRT, then CAS,
    /// bloom, vignette, grain, LUT. Front P decides where each one runs
    /// (before or after scaling); this is only the list.
    pub fn from_mode(mode: &ModeSettings) -> Vec<NativeEffect> {
        todo!("front E")
    }

    /// Output size for an input size (`Anime4kS` doubles, CRT takes the
    /// final size, the rest keep it).
    pub fn output_size(&self, input: (u32, u32), final_size: (u32, u32)) -> (u32, u32) {
        todo!("front E")
    }

    /// Create the pass.
    pub fn create(
        &self,
        device: &Arc<Device>,
        input: &Texture,
        output_size: (u32, u32),
    ) -> Result<Box<dyn Pass>> {
        todo!("front E")
    }
}
