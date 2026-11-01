//! HLSL generation, one compute shader per pass (`codegen.py`).

use crate::EffectDesc;

/// One entry of the constant buffer: name, HLSL type, byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CbEntry {
    /// Name.
    pub name: String,
    /// HLSL type (`float`, `int`, `uint2`, ...).
    pub ty: String,
    /// Byte offset.
    pub offset: u32,
}

/// Layout of the generated constant buffer (sizes, `FrameCount`, then the
/// parameters).
pub fn constant_buffer_layout(desc: &EffectDesc) -> Vec<CbEntry> {
    todo!("front E")
}

/// The HLSL of pass `pass_index` (0-based), FP16 variant or not.
pub fn generate_pass_source(desc: &EffectDesc, pass_index: usize, fp16: bool) -> String {
    todo!("front E")
}
