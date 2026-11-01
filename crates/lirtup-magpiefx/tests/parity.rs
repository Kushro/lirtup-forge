//! Parity of MagpieFX with 1.x (criterion of 5.8): every effect marked `ok`
//! in `docs/developer/magpiefx-report.md` runs on the GPU over the test image
//! and its OUTPUT is compared with `tests/fixtures/parity/refs/<name>.png`,
//! written by `scripts/rust-parity-refs.py` from the Python app (names with
//! `/` are stored with `__`).
//!
//! Needs a Vulkan GPU, dxc or glslang, and the references, so it is ignored
//! by default:
//!
//! ```text
//! PYTHONPATH=src python scripts/rust-parity-refs.py --only magpiefx
//! cargo test -p lirtup-magpiefx --test parity -- --ignored
//! ```
//!
//! `LIRTUP_GPU` picks the adapter; `LIRTUP_PARITY_CASE` runs one effect.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lirtup_effects::{Pass, util};
use lirtup_gpu::{Device, Format, Instance, InstanceOptions, SubmitOptions, Texture};
use lirtup_magpiefx::catalog::verified_names;
use lirtup_magpiefx::compiler::{SpirvCache, detect_compilers};
use lirtup_magpiefx::runtime::{compile_effect_with, fp16_wanted};
use lirtup_magpiefx::{Catalog, CompileOptions, MagpieEffect};
use regex::Regex;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity")
}

/// What `cases.json` says. It is a fixture of ours with a fixed shape, read
/// with patterns (this crate has no JSON parser).
struct Manifest {
    input: (u32, u32),
    target: (u32, u32),
    max_diff: u8,
    max_diff_by_effect: BTreeMap<String, u8>,
}

impl Manifest {
    fn read(path: &Path) -> Manifest {
        let text = std::fs::read_to_string(path).expect("cases.json");
        let number = |key: &str| -> u32 {
            let re = Regex::new(&format!(r#""{key}"\s*:\s*(\d+)"#)).unwrap();
            re.captures(&text).unwrap_or_else(|| panic!("no {key}"))[1]
                .parse()
                .unwrap()
        };
        let pair = Regex::new(r#""target"\s*:\s*\[\s*(\d+)\s*,\s*(\d+)\s*\]"#).unwrap();
        let target = pair.captures(&text).expect("target");
        let overrides = Regex::new(r#""max_diff_by_effect"\s*:\s*\{([^}]*)\}"#).unwrap();
        let entry = Regex::new(r#""([^"]+)"\s*:\s*(\d+)"#).unwrap();
        let max_diff_by_effect = overrides
            .captures(&text)
            .map(|c| {
                entry
                    .captures_iter(&c[1])
                    .map(|e| (e[1].to_owned(), e[2].parse().unwrap()))
                    .collect()
            })
            .unwrap_or_default();
        Manifest {
            input: (number("width"), number("height")),
            target: (target[1].parse().unwrap(), target[2].parse().unwrap()),
            max_diff: number("max_diff") as u8,
            max_diff_by_effect,
        }
    }
}

/// The test image of `make_input` in the script (and of the lirtup-effects
/// parity test): RGBA8.
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

/// Tight 8-bit RGBA of a texture (RGBA16F quantized); `None` for other
/// formats.
fn download_rgba8(tex: &Texture) -> Option<Vec<u8>> {
    let (w, h) = (tex.width() as usize, tex.height() as usize);
    let bpp = match tex.format() {
        Format::R8G8B8A8Unorm => 4,
        Format::R16G16B16A16Float => 8,
        _ => return None,
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
    Some(out)
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
#[ignore = "needs a Vulkan GPU, an HLSL compiler and the references of scripts/rust-parity-refs.py"]
fn verified_effects_match_python() {
    let dir = fixtures();
    let manifest = Manifest::read(&dir.join("cases.json"));
    let (w, h) = manifest.input;
    let device = device();
    let src = util::texture(&device, w, h, Format::R8G8B8A8Unorm, "parity input").expect("input texture");
    util::upload_texture(&device, &src, &input_image(w, h)).expect("upload");

    let compilers = detect_compilers();
    assert!(!compilers.is_empty(), "no HLSL compiler found (dxc or glslang)");
    let cache_dir = tempfile::tempdir().expect("temporary folder");
    let cache = SpirvCache::new(cache_dir.path().to_owned());
    let catalog = Catalog::scan(None);
    // As MagpieEffect.load of 1.x: RGBA input and output, FP16 by policy.
    let options = CompileOptions {
        fp16: fp16_wanted(&device),
        swap_input: false,
        swap_output: false,
    };
    let only = std::env::var("LIRTUP_PARITY_CASE").ok();

    let mut failures = Vec::new();
    let mut compared = 0;
    for &name in verified_names() {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let path = dir.join("refs").join(format!("{}.png", name.replace('/', "__")));
        let want = match image::open(&path) {
            Ok(img) => img.into_rgba8(),
            Err(e) => {
                failures.push(format!("{name}: no reference {} ({e})", path.display()));
                continue;
            }
        };
        let result = (|| {
            let entry = catalog.resolve(name)?;
            let compiled = compile_effect_with(&entry, &compilers, &cache, &options)?;
            let size = MagpieEffect::output_size(&compiled, (w, h), manifest.target)?;
            let effect = MagpieEffect::new(&device, &compiled, &src, size, &Default::default())?;
            let mut recorder = device.recorder()?;
            effect.record(&mut recorder)?;
            device
                .submit(recorder, &SubmitOptions::default())?
                .wait(10_000_000_000)?;
            Ok::<_, Box<dyn std::error::Error>>(effect)
        })();
        let effect = match result {
            Ok(effect) => effect,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let out = effect.output();
        let Some(got) = download_rgba8(out) else {
            failures.push(format!(
                "{name}: OUTPUT format {:?} is not compared",
                out.format()
            ));
            continue;
        };
        compared += 1;
        let max_diff = manifest
            .max_diff_by_effect
            .get(name)
            .copied()
            .unwrap_or(manifest.max_diff);
        match compare(name, (out.width(), out.height(), &got), &want, max_diff) {
            Some(problem) => failures.push(problem),
            None => println!("ok   {name}"),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failed ({compared} compared):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn manifest_reads() {
    let manifest = Manifest::read(&fixtures().join("cases.json"));
    assert_eq!(manifest.input, (160, 96));
    assert_eq!(manifest.target, (320, 192));
    assert!(manifest.max_diff_by_effect.is_empty());
}

#[test]
fn input_image_matches_the_script() {
    let img = input_image(160, 96);
    let px = |x: usize, y: usize| &img[(y * 160 + x) * 4..][..4];
    assert_eq!(px(0, 0), &[255, 255, 255, 255]);
    assert_eq!(px(1, 0), &[1, 0, 7, 255]);
    assert_eq!(px(124, 48), &[64, 64, 64, 255]);
}
