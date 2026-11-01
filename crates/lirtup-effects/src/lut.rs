//! The built-in 3D LUT presets (1.x `effects/lut/presets.py`), generated on
//! the CPU: 32³ RGBA8 texels, red along x, green along y, blue along the
//! slices.

use crate::effects::LutPreset;

/// Edge of the LUT cube.
pub(crate) const LUT_SIZE: u32 = 32;

const fn luma(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn s_curve(v: f64) -> f64 {
    if v < 0.5 { 2.0 * v * v } else { 1.0 - (-2.0 * v + 2.0).powf(2.0) / 2.0 }
}

fn lofi(v: f64) -> f64 {
    let v = v * 0.9 + 0.05;
    if v > 0.5 { v * 0.8 + 0.2 } else { v }
}

/// The three output channels of a preset for one input color.
fn grade(preset: LutPreset, r: f64, g: f64, b: f64) -> [f64; 3] {
    match preset {
        LutPreset::Identity => [r, g, b],
        LutPreset::Warm => [r * 1.05 + 0.02, g * 0.98 + 0.01, b * 0.92 - 0.01],
        LutPreset::Cool => [r * 0.95, g * 1.02, (b * 1.08 + 0.02).min(1.0)],
        LutPreset::Film => {
            let l = luma(r, g, b);
            let lift = (s_curve(l) - l) * 0.1;
            [r * 0.92 + lift, g * 0.92 + lift, b * 0.92 + lift]
        }
        LutPreset::Vivid => {
            let l = luma(r, g, b);
            [l + (r - l) * 1.2, l + (g - l) * 1.2, l + (b - l) * 1.2]
        }
        LutPreset::Pastel => [(r * 0.95 + 0.08).min(1.0), (g * 0.95 + 0.08).min(1.0), (b * 0.95 + 0.08).min(1.0)],
        LutPreset::Noir => {
            let l = ((luma(r, g, b) - 0.5) * 1.2 + 0.5).clamp(0.0, 1.0);
            [l, l, l]
        }
        LutPreset::Sepia => {
            let l = luma(r, g, b);
            [(l * 1.12 + 0.08).min(1.0), (l * 0.95 + 0.04).min(1.0), (l * 0.78).min(1.0)]
        }
        LutPreset::Bleach => {
            let c = ((luma(r, g, b) - 0.5) * 1.3 + 0.5).clamp(0.0, 1.0);
            [r * 0.7 + c * 0.3, g * 0.7 + c * 0.3, b * 0.7 + c * 0.3]
        }
        LutPreset::Split => {
            let l = luma(r, g, b);
            let hi = l * l;
            let shadow_g = (1.0 - l) * 0.3;
            let shadow_b = (1.0 - l) * 0.5;
            [
                r * (1.0 - hi) + hi,
                g * (1.0 - hi - shadow_g) + hi * 0.7 + shadow_g,
                b * (1.0 - hi - shadow_b) + hi * 0.3 + shadow_b,
            ]
        }
        LutPreset::Cyano => {
            let l = luma(r, g, b);
            [l * 0.2, l * 0.7, l]
        }
        LutPreset::Lofi => [lofi(r), lofi(g), lofi(b)],
    }
}

/// The RGBA8 texels of a preset (`build_lut`), slice after slice.
pub(crate) fn lut_data(preset: LutPreset) -> Vec<u8> {
    let n = LUT_SIZE as usize;
    let step = (LUT_SIZE - 1) as f64;
    let mut data = Vec::with_capacity(n * n * n * 4);
    for z in 0..n {
        let b = z as f64 / step;
        for y in 0..n {
            let g = y as f64 / step;
            for x in 0..n {
                let r = x as f64 / step;
                for c in grade(preset, r, g, b) {
                    // int() truncates, as in 1.x
                    data.push((c.clamp(0.0, 1.0) * 255.0) as u8);
                }
                data.push(255);
            }
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_a_ramp() {
        let data = lut_data(LutPreset::Identity);
        assert_eq!(data.len(), 32 * 32 * 32 * 4);
        assert_eq!(&data[0..4], &[0, 0, 0, 255]);
        // x = 31 of the first row is full red
        assert_eq!(&data[31 * 4..32 * 4], &[255, 0, 0, 255]);
        // last texel is white
        assert_eq!(&data[data.len() - 4..], &[255, 255, 255, 255]);
    }

    #[test]
    fn presets_by_name() {
        for preset in LutPreset::ALL {
            assert_eq!(LutPreset::from_name(preset.name()), Some(preset));
            assert_eq!(lut_data(preset).len(), 32 * 32 * 32 * 4);
        }
        assert_eq!(LutPreset::from_name("nope"), None);
    }
}
