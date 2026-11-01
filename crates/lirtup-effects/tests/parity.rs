//! Parity of the native passes with 1.x (criterion of 5.8): every case of
//! `tests/fixtures/parity/cases.json` runs on the GPU and its output is
//! compared with `refs/<name>.png`, written by `scripts/rust-parity-refs.py`
//! from the Python app on the same test image.
//!
//! Needs a Vulkan GPU and the references, so it is ignored by default:
//!
//! ```text
//! PYTHONPATH=src python scripts/rust-parity-refs.py --only effects
//! cargo test -p lirtup-effects --test parity -- --ignored
//! ```
//!
//! `LIRTUP_GPU` picks the adapter (index or name), as `--gpu`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lirtup_config::Model;
use lirtup_effects::Pass;
use lirtup_effects::effects::{CrtParams, LutPreset, NativeEffect};
use lirtup_effects::scalers::{Scaler, ScalerKind, ScalerParams};
use lirtup_effects::srcnn::{Srcnn, SrcnnOptions};
use lirtup_effects::util;
use lirtup_gpu::{Device, Format, Instance, InstanceOptions, SubmitOptions, Texture};
use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    input: InputSize,
    max_diff: u8,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct InputSize {
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    effect: Option<String>,
    scaler: Option<String>,
    srcnn: Option<String>,
    /// `rgba16f` for the passes that run on the 16-bit screen texture.
    input: Option<String>,
    preset: Option<String>,
    #[serde(default)]
    params: std::collections::BTreeMap<String, f64>,
    dst: Option<(u32, u32)>,
    max_diff: Option<u8>,
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity")
}

/// The test image of `make_input` in the script: RGBA8.
fn input_image(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut r = x * 255 / (width - 1).max(1);
            let mut g = y * 255 / (height - 1).max(1);
            let mut b = ((x * 7) ^ (y * 13)) & 255;
            if x % 23 == 0 {
                (r, g, b) = (255, 255, 255);
            }
            if x >= width * 3 / 4 && y >= height / 2 {
                let v = if (x / 4 + y / 4) % 2 == 1 { 64 } else { 192 };
                (r, g, b) = (v, v, v);
            }
            out.extend_from_slice(&[r as u8, g as u8, b as u8, 255]);
        }
    }
    out
}

/// IEEE half of `v` (0 ≤ v ≤ 1), round to nearest even, as Python's
/// `struct.pack("e")`.
fn f16_bits(v: f64) -> u16 {
    if v <= 0.0 {
        return 0;
    }
    let e = v.log2().floor() as i32;
    if e < -14 {
        // Subnormal: units of 2^-24.
        return (v * 2f64.powi(24)).round_ties_even() as u16;
    }
    let mut e = e;
    let mut m = ((v / 2f64.powi(e) - 1.0) * 1024.0).round_ties_even() as u32;
    if m == 1024 {
        m = 0;
        e += 1;
    }
    (((e + 15) as u32) << 10 | m) as u16
}

fn f16_value(bits: u16) -> f64 {
    let e = i32::from((bits >> 10) & 0x1f);
    let m = f64::from(bits & 0x3ff);
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    sign * match e {
        0 => m * 2f64.powi(-24),
        31 => f64::INFINITY,
        _ => (1.0 + m / 1024.0) * 2f64.powi(e - 15),
    }
}

/// As the script: `round(clamp(v, 0, 1) * 255)`.
fn quantize(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn device() -> Arc<Device> {
    let instance = Instance::new(&InstanceOptions {
        app_name: "lirtup-parity".into(),
        debug: false,
    })
    .expect("Vulkan instance");
    let gpu = std::env::var("LIRTUP_GPU").ok();
    let index = instance.select_adapter(gpu.as_deref()).expect("a Vulkan adapter");
    instance.create_device(index).expect("Vulkan device")
}

fn upload(device: &Arc<Device>, w: u32, h: u32, rgba8: &[u8], half: bool) -> Texture {
    let (format, data) = if half {
        let halves: Vec<u8> = rgba8
            .iter()
            .flat_map(|&v| f16_bits(f64::from(v) / 255.0).to_le_bytes())
            .collect();
        (Format::R16G16B16A16Float, halves)
    } else {
        (Format::R8G8B8A8Unorm, rgba8.to_vec())
    };
    let tex = util::texture(device, w, h, format, "parity input").expect("input texture");
    util::upload_texture(device, &tex, &data).expect("upload");
    tex
}

/// Tight 8-bit RGBA of a texture (RGBA16F quantized).
fn download_rgba8(tex: &Texture) -> Vec<u8> {
    let (w, h) = (tex.width() as usize, tex.height() as usize);
    let bpp = match tex.format() {
        Format::R8G8B8A8Unorm => 4,
        Format::R16G16B16A16Float => 8,
        other => panic!("output format {other:?} is not compared"),
    };
    let data = tex.download().expect("download");
    let pitch = (tex.row_pitch() as usize).max(w * bpp);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = &data[y * pitch..y * pitch + w * bpp];
        if bpp == 4 {
            out.extend_from_slice(row);
        } else {
            out.extend(
                row.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| quantize(f16_value(u16::from_le_bytes(*b)))),
            );
        }
    }
    out
}

fn run(device: &Arc<Device>, pass: &dyn Pass) {
    let mut recorder = device.recorder().expect("recorder");
    pass.record(&mut recorder).expect("record");
    device
        .submit(recorder, &SubmitOptions::default())
        .expect("submit")
        .wait(10_000_000_000)
        .expect("wait");
}

fn native_effect(case: &Case) -> NativeEffect {
    let p = |name: &str| {
        *case
            .params
            .get(name)
            .unwrap_or_else(|| panic!("{}: no {name}", case.name)) as f32
    };
    match case.effect.as_deref().expect("an effect case") {
        "deband" => NativeEffect::Deband {
            strength: p("deband_strength"),
        },
        "cas" => NativeEffect::Cas {
            strength: p("cas_strength"),
        },
        "bloom" => NativeEffect::Bloom {
            strength: p("bloom_strength"),
            threshold: p("bloom_threshold"),
            radius: p("bloom_radius") as u32,
        },
        "vignette" => NativeEffect::Vignette {
            strength: p("vignette_strength"),
            radius: p("vignette_radius"),
            falloff: p("vignette_falloff"),
        },
        "grain" => NativeEffect::Grain {
            strength: p("grain_strength"),
            size: p("grain_size"),
        },
        "lut" => NativeEffect::Lut {
            intensity: p("lut_intensity"),
            preset: LutPreset::from_name(case.preset.as_deref().unwrap_or("identity")).expect("a LUT preset"),
        },
        "luma_sharpen" => NativeEffect::LumaSharpen {
            strength: p("luma_sharpen_strength"),
            clamp: p("luma_sharpen_clamp"),
            pattern: p("luma_sharpen_pattern") as u32,
            offset_bias: p("luma_sharpen_offset_bias"),
        },
        "anime4k" => NativeEffect::Anime4kS,
        "crt_easymode" => NativeEffect::CrtEasymode(CrtParams::default()),
        other => panic!("unknown effect {other}"),
    }
}

fn scaler_kind(name: &str) -> ScalerKind {
    match name {
        "copy" => ScalerKind::Copy,
        "catmull_rom" => ScalerKind::CatmullRom,
        "lanczos" => ScalerKind::Lanczos,
        "fsr" => ScalerKind::Fsr,
        "nis" => ScalerKind::Nis,
        other => panic!("unknown scaler {other}"),
    }
}

/// Run one case; the output as 8-bit RGBA with its size.
fn run_case(device: &Arc<Device>, case: &Case, src: &[u8], (w, h): (u32, u32)) -> (u32, u32, Vec<u8>) {
    let half = case.input.as_deref() == Some("rgba16f");
    let input = upload(device, w, h, src, half);
    let pass: Box<dyn Pass> = if case.effect.is_some() {
        let effect = native_effect(case);
        let size = effect.output_size((w, h), case.dst.unwrap_or((w, h)));
        effect.create(device, &input, size).expect("native pass")
    } else if let Some(kind) = &case.scaler {
        let dst = case.dst.expect("a scaler case has dst");
        // ModeSettings defaults, as Scaler.configure(Config()) in 1.x.
        let params = ScalerParams {
            blur: 1.0,
            antiring_strength: 0.8,
            tight_antiring: true,
            kernel_radius: None,
        };
        let mut scaler = Scaler::new(
            device,
            scaler_kind(kind),
            &input,
            dst,
            (0, 0, dst.0, dst.1),
            &params,
        )
        .expect("scaler");
        scaler.set_background([0.0, 0.0, 0.0, 1.0]);
        Box::new(scaler)
    } else {
        let name = case.srcnn.as_deref().expect("a srcnn case");
        let model: Model = serde_json::from_value(serde_json::json!(name)).expect("a model name");
        let options = SrcnnOptions {
            model,
            tiles: None,
            fp16: false, // 1.x has no FP16 SRCNN
        };
        Box::new(Srcnn::new(device, &input, &options).expect("srcnn"))
    };
    run(device, pass.as_ref());
    let out = pass.output();
    (out.width(), out.height(), download_rgba8(out))
}

/// `None` when equal within `max_diff`; else what differs.
fn compare(name: &str, got: (u32, u32, &[u8]), want: &image::RgbaImage, max_diff: u8) -> Option<String> {
    if (got.0, got.1) != want.dimensions() {
        return Some(format!(
            "{name}: {}x{} instead of {}x{}",
            got.0,
            got.1,
            want.width(),
            want.height()
        ));
    }
    let mut worst = 0u8;
    let mut bad = 0usize;
    let mut first = None;
    for (i, (a, b)) in got.2.iter().zip(want.as_raw()).enumerate() {
        let d = a.abs_diff(*b);
        worst = worst.max(d);
        if d > max_diff {
            bad += 1;
            first.get_or_insert(i / 4);
        }
    }
    let first = first?;
    let w = got.0 as usize;
    Some(format!(
        "{name}: {bad} channels off by more than {max_diff} (max {worst}), first at ({}, {})",
        first % w,
        first / w
    ))
}

#[test]
#[ignore = "needs a Vulkan GPU and the references of scripts/rust-parity-refs.py"]
fn native_passes_match_python() {
    let dir = fixtures();
    let manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(dir.join("cases.json")).expect("cases.json"))
            .expect("manifest");
    let size = (manifest.input.width, manifest.input.height);
    let src = input_image(size.0, size.1);
    let device = device();
    let only = std::env::var("LIRTUP_PARITY_CASE").ok();

    let mut failures = Vec::new();
    let mut compared = 0;
    for case in &manifest.cases {
        if only.as_deref().is_some_and(|o| o != case.name) {
            continue;
        }
        let path = dir.join("refs").join(format!("{}.png", case.name));
        let want = match image::open(&path) {
            Ok(img) => img.into_rgba8(),
            Err(e) => {
                failures.push(format!("{}: no reference {} ({e})", case.name, path.display()));
                continue;
            }
        };
        let (w, h, got) = run_case(&device, case, &src, size);
        compared += 1;
        if let Some(problem) = compare(
            &case.name,
            (w, h, &got),
            &want,
            case.max_diff.unwrap_or(manifest.max_diff),
        ) {
            failures.push(problem);
        } else {
            println!("ok   {}", case.name);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {compared} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn half_floats_round_like_python() {
    // struct.pack("<e", v / 255) for a few values.
    assert_eq!(f16_bits(0.0), 0);
    assert_eq!(f16_bits(1.0), 0x3c00);
    assert_eq!(f16_bits(0.5), 0x3800);
    assert_eq!(f16_bits(1.0 / 255.0), 0x1c04);
    for v in 0..=255u8 {
        let x = f64::from(v) / 255.0;
        assert_eq!(quantize(f16_value(f16_bits(x))), v, "{v}");
    }
}

#[test]
fn input_image_matches_the_script() {
    let img = input_image(160, 96);
    let px = |x: usize, y: usize| &img[(y * 160 + x) * 4..][..4];
    assert_eq!(px(0, 0), &[255, 255, 255, 255]);
    assert_eq!(px(1, 0), &[1, 0, 7, 255]);
    assert_eq!(px(159, 1), &[255, 2, ((159 * 7) ^ 13) as u8, 255]);
    assert_eq!(px(120, 48), &[192, 192, 192, 255]);
    assert_eq!(px(124, 48), &[64, 64, 64, 255]);
}
