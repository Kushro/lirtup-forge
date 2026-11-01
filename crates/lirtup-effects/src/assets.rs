//! Embedded shaders and model descriptions.
//!
//! Paths are relative to the repository root (`src/upscaler/...`): the data
//! is not copied into `crates/` while the Python app still ships (one copy,
//! no drift; decision 5 of the round 0 contract). The blue noise table is the
//! only blob of this crate (`data/blue_noise.bin`, made once from 1.x
//! `blue_noise.py`).

macro_rules! shader {
    ($path:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../src/upscaler/shaders/",
            $path
        ))
    };
}

macro_rules! cunny {
    ($model:literal, $file:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../src/upscaler/srcnn/cunny/",
            $model,
            "/",
            $file
        ))
    };
}

/// Every native `.spv`, by its path relative to `src/upscaler/shaders/`.
static NATIVE: &[(&str, &[u8])] = &[
    (
        "converters/delinearize/delinearize.spv",
        shader!("converters/delinearize/delinearize.spv"),
    ),
    (
        "converters/dither/dither.spv",
        shader!("converters/dither/dither.spv"),
    ),
    (
        "converters/linearize/linearize.spv",
        shader!("converters/linearize/linearize.spv"),
    ),
    (
        "effects/anime4k/anime4k_upscale_s_pass1.spv",
        shader!("effects/anime4k/anime4k_upscale_s_pass1.spv"),
    ),
    (
        "effects/anime4k/anime4k_upscale_s_pass2.spv",
        shader!("effects/anime4k/anime4k_upscale_s_pass2.spv"),
    ),
    (
        "effects/anime4k/anime4k_upscale_s_pass3.spv",
        shader!("effects/anime4k/anime4k_upscale_s_pass3.spv"),
    ),
    (
        "effects/anime4k/anime4k_upscale_s_pass4.spv",
        shader!("effects/anime4k/anime4k_upscale_s_pass4.spv"),
    ),
    ("effects/bloom/bloom.spv", shader!("effects/bloom/bloom.spv")),
    ("effects/cas/cas.spv", shader!("effects/cas/cas.spv")),
    (
        "effects/crt_easymode/crt_easymode.spv",
        shader!("effects/crt_easymode/crt_easymode.spv"),
    ),
    ("effects/deband/deband.spv", shader!("effects/deband/deband.spv")),
    ("effects/grain/grain.spv", shader!("effects/grain/grain.spv")),
    (
        "effects/luma_sharpen/luma_sharpen.spv",
        shader!("effects/luma_sharpen/luma_sharpen.spv"),
    ),
    ("effects/lut/lut.spv", shader!("effects/lut/lut.spv")),
    (
        "effects/vignette/vignette.spv",
        shader!("effects/vignette/vignette.spv"),
    ),
    ("misc/clear/clear.spv", shader!("misc/clear/clear.spv")),
    (
        "misc/overlay_blender/overlay_blend.spv",
        shader!("misc/overlay_blender/overlay_blend.spv"),
    ),
    (
        "scalers/catmull_rom/catmull_rom.spv",
        shader!("scalers/catmull_rom/catmull_rom.spv"),
    ),
    ("scalers/copy/copy.spv", shader!("scalers/copy/copy.spv")),
    ("scalers/fsr/fsr.spv", shader!("scalers/fsr/fsr.spv")),
    (
        "scalers/lanczos/lanczos_adaptive.spv",
        shader!("scalers/lanczos/lanczos_adaptive.spv"),
    ),
    (
        "scalers/lanczos/lanczos_fixed.spv",
        shader!("scalers/lanczos/lanczos_fixed.spv"),
    ),
    ("scalers/nis/nis.spv", shader!("scalers/nis/nis.spv")),
];

/// SPIR-V of a native shader by its 1.x relative path
/// (`"scalers/lanczos/lanczos_fixed.spv"`, `"effects/cas/cas.spv"`, ...).
pub fn native_spirv(path: &str) -> Option<&'static [u8]> {
    NATIVE.iter().find(|(p, _)| *p == path).map(|(_, data)| *data)
}

/// Every native shader path known to [`native_spirv`].
pub fn native_paths() -> impl Iterator<Item = &'static str> {
    NATIVE.iter().map(|(p, _)| *p)
}

static VERYFAST_PASSES: &[&[u8]] = &[
    cunny!("veryfast", "Pass1.spv"),
    cunny!("veryfast", "Pass2.spv"),
    cunny!("veryfast", "Pass3.spv"),
    cunny!("veryfast", "Pass4.spv"),
];
static VERYFAST_TILE: &[&[u8]] = &[
    cunny!("veryfast", "Pass1_tile.spv"),
    cunny!("veryfast", "Pass2_tile.spv"),
    cunny!("veryfast", "Pass3_tile.spv"),
    cunny!("veryfast", "Pass4_tile.spv"),
];
static VERYFAST_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/upscaler/srcnn/cunny/veryfast/model.json"
));

static FASTER_PASSES: &[&[u8]] = &[
    cunny!("faster", "Pass1.spv"),
    cunny!("faster", "Pass2.spv"),
    cunny!("faster", "Pass3.spv"),
    cunny!("faster", "Pass4.spv"),
];
static FASTER_TILE: &[&[u8]] = &[
    cunny!("faster", "Pass1_tile.spv"),
    cunny!("faster", "Pass2_tile.spv"),
    cunny!("faster", "Pass3_tile.spv"),
    cunny!("faster", "Pass4_tile.spv"),
];
static FASTER_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/upscaler/srcnn/cunny/faster/model.json"
));

static FAST_PASSES: &[&[u8]] = &[
    cunny!("fast", "Pass1.spv"),
    cunny!("fast", "Pass2.spv"),
    cunny!("fast", "Pass3.spv"),
    cunny!("fast", "Pass4.spv"),
];
static FAST_TILE: &[&[u8]] = &[
    cunny!("fast", "Pass1_tile.spv"),
    cunny!("fast", "Pass2_tile.spv"),
    cunny!("fast", "Pass3_tile.spv"),
    cunny!("fast", "Pass4_tile.spv"),
];
static FAST_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/upscaler/srcnn/cunny/fast/model.json"
));

static M3X12_PASSES: &[&[u8]] = &[
    cunny!("3x12", "Pass1.spv"),
    cunny!("3x12", "Pass2.spv"),
    cunny!("3x12", "Pass3.spv"),
    cunny!("3x12", "Pass4.spv"),
    cunny!("3x12", "Pass5.spv"),
];
static M3X12_TILE: &[&[u8]] = &[
    cunny!("3x12", "Pass1_tile.spv"),
    cunny!("3x12", "Pass2_tile.spv"),
    cunny!("3x12", "Pass3_tile.spv"),
    cunny!("3x12", "Pass4_tile.spv"),
    cunny!("3x12", "Pass5_tile.spv"),
];
static M3X12_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/upscaler/srcnn/cunny/3x12/model.json"
));

static M4X12_PASSES: &[&[u8]] = &[
    cunny!("4x12", "Pass1.spv"),
    cunny!("4x12", "Pass2.spv"),
    cunny!("4x12", "Pass3.spv"),
    cunny!("4x12", "Pass4.spv"),
    cunny!("4x12", "Pass5.spv"),
    cunny!("4x12", "Pass6.spv"),
];
static M4X12_TILE: &[&[u8]] = &[
    cunny!("4x12", "Pass1_tile.spv"),
    cunny!("4x12", "Pass2_tile.spv"),
    cunny!("4x12", "Pass3_tile.spv"),
    cunny!("4x12", "Pass4_tile.spv"),
    cunny!("4x12", "Pass5_tile.spv"),
    cunny!("4x12", "Pass6_tile.spv"),
];
static M4X12_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/upscaler/srcnn/cunny/4x12/model.json"
));

static M4X16_PASSES: &[&[u8]] = &[
    cunny!("4x16", "Pass1.spv"),
    cunny!("4x16", "Pass2.spv"),
    cunny!("4x16", "Pass3.spv"),
    cunny!("4x16", "Pass4.spv"),
    cunny!("4x16", "Pass5.spv"),
    cunny!("4x16", "Pass6.spv"),
];
static M4X16_TILE: &[&[u8]] = &[
    cunny!("4x16", "Pass1_tile.spv"),
    cunny!("4x16", "Pass2_tile.spv"),
    cunny!("4x16", "Pass3_tile.spv"),
    cunny!("4x16", "Pass4_tile.spv"),
    cunny!("4x16", "Pass5_tile.spv"),
    cunny!("4x16", "Pass6_tile.spv"),
];
static M4X16_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/upscaler/srcnn/cunny/4x16/model.json"
));

static M4X24_PASSES: &[&[u8]] = &[
    cunny!("4x24", "Pass1.spv"),
    cunny!("4x24", "Pass2.spv"),
    cunny!("4x24", "Pass3.spv"),
    cunny!("4x24", "Pass4.spv"),
    cunny!("4x24", "Pass5.spv"),
    cunny!("4x24", "Pass6.spv"),
];
static M4X24_TILE: &[&[u8]] = &[
    cunny!("4x24", "Pass1_tile.spv"),
    cunny!("4x24", "Pass2_tile.spv"),
    cunny!("4x24", "Pass3_tile.spv"),
    cunny!("4x24", "Pass4_tile.spv"),
    cunny!("4x24", "Pass5_tile.spv"),
    cunny!("4x24", "Pass6_tile.spv"),
];
static M4X24_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/upscaler/srcnn/cunny/4x24/model.json"
));

static M4X32_PASSES: &[&[u8]] = &[
    cunny!("4x32", "Pass1.spv"),
    cunny!("4x32", "Pass2.spv"),
    cunny!("4x32", "Pass3.spv"),
    cunny!("4x32", "Pass4.spv"),
    cunny!("4x32", "Pass5.spv"),
    cunny!("4x32", "Pass6.spv"),
];
static M4X32_TILE: &[&[u8]] = &[
    cunny!("4x32", "Pass1_tile.spv"),
    cunny!("4x32", "Pass2_tile.spv"),
    cunny!("4x32", "Pass3_tile.spv"),
    cunny!("4x32", "Pass4_tile.spv"),
    cunny!("4x32", "Pass5_tile.spv"),
    cunny!("4x32", "Pass6_tile.spv"),
];
static M4X32_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/upscaler/srcnn/cunny/4x32/model.json"
));

static M8X32_PASSES: &[&[u8]] = &[
    cunny!("8x32", "Pass1.spv"),
    cunny!("8x32", "Pass2.spv"),
    cunny!("8x32", "Pass3.spv"),
    cunny!("8x32", "Pass4.spv"),
    cunny!("8x32", "Pass5.spv"),
    cunny!("8x32", "Pass6.spv"),
    cunny!("8x32", "Pass7.spv"),
    cunny!("8x32", "Pass8.spv"),
    cunny!("8x32", "Pass9.spv"),
    cunny!("8x32", "Pass10.spv"),
];
static M8X32_TILE: &[&[u8]] = &[
    cunny!("8x32", "Pass1_tile.spv"),
    cunny!("8x32", "Pass2_tile.spv"),
    cunny!("8x32", "Pass3_tile.spv"),
    cunny!("8x32", "Pass4_tile.spv"),
    cunny!("8x32", "Pass5_tile.spv"),
    cunny!("8x32", "Pass6_tile.spv"),
    cunny!("8x32", "Pass7_tile.spv"),
    cunny!("8x32", "Pass8_tile.spv"),
    cunny!("8x32", "Pass9_tile.spv"),
    cunny!("8x32", "Pass10_tile.spv"),
];
static M8X32_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/upscaler/srcnn/cunny/8x32/model.json"
));

/// `model.json` and the pass SPIR-V of a CuNNy model.
pub fn cunny_model(model: lirtup_config::Model) -> Option<CunnyAssets> {
    use lirtup_config::Model;
    let (json, passes, tile_passes) = match model {
        Model::None => return None,
        Model::VeryFast => (VERYFAST_JSON, VERYFAST_PASSES, VERYFAST_TILE),
        Model::Faster => (FASTER_JSON, FASTER_PASSES, FASTER_TILE),
        Model::Fast => (FAST_JSON, FAST_PASSES, FAST_TILE),
        Model::M3x12 => (M3X12_JSON, M3X12_PASSES, M3X12_TILE),
        Model::M4x12 => (M4X12_JSON, M4X12_PASSES, M4X12_TILE),
        Model::M4x16 => (M4X16_JSON, M4X16_PASSES, M4X16_TILE),
        Model::M4x24 => (M4X24_JSON, M4X24_PASSES, M4X24_TILE),
        Model::M4x32 => (M4X32_JSON, M4X32_PASSES, M4X32_TILE),
        Model::M8x32 => (M8X32_JSON, M8X32_PASSES, M8X32_TILE),
    };
    Some(CunnyAssets {
        json,
        passes,
        tile_passes,
    })
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

static BLUE_NOISE: &[u8] = include_bytes!("../data/blue_noise.bin");

/// The blue noise table of the dither converter: 64x64 RGBA8 (1.x
/// `blue_noise.py`, `LDR_RGBA_0.png` of momentsingraphics.de).
pub fn blue_noise() -> &'static [u8] {
    BLUE_NOISE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_complete() {
        assert_eq!(blue_noise().len(), 64 * 64 * 4);
        assert!(native_spirv("scalers/lanczos/lanczos_fixed.spv").is_some());
        assert!(native_spirv("nope.spv").is_none());
        for model in lirtup_config::Model::ALL {
            match cunny_model(model) {
                None => assert_eq!(model, lirtup_config::Model::None),
                Some(assets) => {
                    assert_eq!(assets.passes.len(), assets.tile_passes.len());
                    // SPIR-V magic number
                    assert!(assets.passes.iter().all(|p| p[..4] == [0x03, 0x02, 0x23, 0x07]));
                }
            }
        }
    }
}
