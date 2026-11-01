//! The BGRA ↔ RGBA swizzle around the chain (capture is BGRA, effects expect
//! RGBA; `SWAP_RB_EFFECT` in 1.x).

/// HLSL of the swap-R/B pass.
pub fn swap_rb_source() -> &'static str {
    todo!("front E")
}
