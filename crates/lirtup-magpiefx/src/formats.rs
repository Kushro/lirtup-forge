//! MagpieFX format names (`formats.py`) → `lirtup_gpu::Format`, texel types.

use lirtup_gpu::Format;

/// What the codegen and the runtime need about a `//!FORMAT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatDesc {
    /// GPU format.
    pub format: Format,
    /// HLSL texel type (`float4`, `uint`, ...).
    pub texel_type: &'static str,
    /// Channels.
    pub channels: u32,
}

/// Look up a format name (`R8G8B8A8_UNORM`, ...).
pub fn lookup(name: &str) -> Option<FormatDesc> {
    todo!("front E")
}
