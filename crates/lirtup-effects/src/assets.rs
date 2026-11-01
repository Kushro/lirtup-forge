//! Embedded shaders and model descriptions.
//!
//! Paths are relative to the repository root (`src/upscaler/...`); front E
//! writes the `include_bytes!` table. The data is not copied into `crates/`
//! while the Python app still ships (one copy, no drift).

/// SPIR-V of a native shader by its 1.x relative path
/// (`"scalers/lanczos/lanczos.spv"`, `"effects/cas/cas.spv"`, ...).
pub fn native_spirv(path: &str) -> Option<&'static [u8]> {
    todo!("front E")
}

/// `model.json` and the pass SPIR-V of a CuNNy model.
pub fn cunny_model(model: lirtup_config::Model) -> Option<CunnyAssets> {
    todo!("front E")
}

/// Assets of one CuNNy model (`srcnn/cunny/<model>/`).
#[derive(Debug, Clone, Copy)]
pub struct CunnyAssets {
    /// `model.json`.
    pub json: &'static str,
    /// `PassN.spv`, in order.
    pub passes: &'static [&'static [u8]],
    /// `PassN_tile.spv`, in order (texture-array variants for tiles).
    pub tile_passes: &'static [&'static [u8]],
}

/// The blue noise table of the dither converter (1.x `blue_noise.py`, as a
/// binary blob).
pub fn blue_noise() -> &'static [u8] {
    todo!("front E: convert blue_noise.py to a .bin once")
}
