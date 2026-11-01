//! Color converters (`shaders/converters/`): linearize, delinearize, dither
//! (with the blue noise table).

use std::sync::Arc;

use lirtup_gpu::{Device, Format, Texture};

use crate::{Pass, Result};

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

impl Conversion {
    /// Create the pass writing `output_format`.
    pub fn create(
        &self,
        device: &Arc<Device>,
        input: &Texture,
        output_format: Format,
    ) -> Result<Box<dyn Pass>> {
        todo!("front E")
    }
}
