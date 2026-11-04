//! The hand ports of the retired native engine (`lirtup_effects::ports`,
//! feature `ports`) against the same effects run through MagpieFX: same
//! test image, same parameters (the table of `lirtup_config::legacy`),
//! FP32. Needs a Vulkan GPU and an HLSL compiler (or the precompiled seed),
//! so the GPU test is ignored by default:
//!
//! ```text
//! cargo test -p lirtup-magpiefx --test ports -- --ignored --nocapture
//! ```
//!
//! `LIRTUP_GPU` picks the adapter (index or name); `LIRTUP_PORTS_CASE` runs
//! one case (by its label, e.g. `CRT_Easymode tuned`).
//!
//! The plain test checks that what `lirtup_config::legacy::translate_native`
//! writes for an old file (catalog name, parameter names and values) is what
//! the ports call the same settings.

use std::collections::BTreeMap;
use std::sync::Arc;

use lirtup_config::legacy::{self, ANIME4K_EFFECT, CRT_EFFECT, LUMA_SHARPEN_EFFECT};
use lirtup_config::{Config, ConfigWarning, Mapping, Value};
use lirtup_effects::ports::{CrtParams, PortEffect};
use lirtup_effects::{Pass, util};
use lirtup_gpu::{Device, Format, Instance, InstanceOptions, SubmitOptions, Texture};
use lirtup_magpiefx::compiler::{HlslCompiler, SpirvCache, detect_compilers};
use lirtup_magpiefx::runtime::compile_effect_with;
use lirtup_magpiefx::spec::parse_effect_chain;
use lirtup_magpiefx::{Catalog, CompileOptions, EffectSpec, MagpieEffect};

// ---------------------------------------------------------------------------
// The translation of the legacy keys names what the ports name
// ---------------------------------------------------------------------------

/// A mapping of legacy fields, as an old mode file would have them.
fn legacy_map(fields: &[(&str, Value)]) -> Mapping {
    let mut map = Mapping::new();
    for (key, value) in fields {
        map.insert(Value::from(*key), value.clone());
    }
    map
}

fn float(v: f64) -> Value {
    Value::from(v)
}

fn int(v: u64) -> Value {
    Value::from(v)
}

/// What the chain must hold for a port: its catalog name and the
/// parameters that differ from the effect's defaults.
fn expected(port: &PortEffect) -> EffectSpec {
    let defaults = port.with_defaults().magpie_params();
    let params: BTreeMap<String, f64> = port
        .magpie_params()
        .into_iter()
        .filter(|(name, value)| defaults.get(name) != Some(value))
        .collect();
    EffectSpec {
        name: port.magpie_name().to_owned(),
        params,
    }
}

/// Translate `map` and read its chain back the way a session does.
fn translated_chain(mut map: Mapping) -> Vec<EffectSpec> {
    let mut warnings: Vec<ConfigWarning> = Vec::new();
    assert!(
        legacy::translate_native(&mut map, &mut warnings),
        "nothing translated"
    );
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert!(!legacy::has_legacy_keys(&map), "legacy keys left: {map:?}");
    let (config, warnings) = Config::from_mapping(&map);
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    parse_effect_chain(&config.scaling.magpie_effects).expect("a valid chain")
}

/// LumaSharpen with every parameter off its default (values exact in f32,
/// so the port's `f32` fields and the YAML's `f64` agree).
fn luma_tuned() -> (PortEffect, Vec<(&'static str, Value)>) {
    let port = PortEffect::LumaSharpen {
        strength: 1.5,
        clamp: 0.25,
        pattern: 2,
        offset_bias: 2.0,
    };
    let fields = vec![
        ("luma_sharpen_enabled", Value::from(true)),
        ("luma_sharpen_strength", float(1.5)),
        ("luma_sharpen_clamp", float(0.25)),
        ("luma_sharpen_pattern", int(2)),
        ("luma_sharpen_offset_bias", float(2.0)),
    ];
    (port, fields)
}

/// CRT-Easymode with all 17 parameters off their defaults.
fn crt_tuned() -> (PortEffect, Vec<(&'static str, Value)>) {
    let port = PortEffect::CrtEasymode(CrtParams {
        sharpness_h: 0.75,
        sharpness_v: 0.5,
        mask_strength: 0.5,
        mask_dot_width: 2,
        mask_dot_height: 3,
        mask_stagger: 1,
        mask_size: 2,
        scanline_strength: 0.5,
        scanline_beam_width_min: 2.0,
        scanline_beam_width_max: 2.5,
        scanline_bright_min: 0.25,
        scanline_bright_max: 0.75,
        scanline_cutoff: 500,
        gamma_input: 2.5,
        gamma_output: 2.25,
        bright_boost: 1.5,
        dilation: 0,
    });
    let fields = vec![
        ("crt_enabled", Value::from(true)),
        ("crt_sharpness_h", float(0.75)),
        ("crt_sharpness_v", float(0.5)),
        ("crt_mask_strength", float(0.5)),
        ("crt_mask_dot_width", int(2)),
        ("crt_mask_dot_height", int(3)),
        ("crt_mask_stagger", int(1)),
        ("crt_mask_size", int(2)),
        ("crt_scanline_strength", float(0.5)),
        ("crt_scanline_beam_width_min", float(2.0)),
        ("crt_scanline_beam_width_max", float(2.5)),
        ("crt_scanline_bright_min", float(0.25)),
        ("crt_scanline_bright_max", float(0.75)),
        ("crt_scanline_cutoff", int(500)),
        ("crt_gamma_input", float(2.5)),
        ("crt_gamma_output", float(2.25)),
        ("crt_bright_boost", float(1.5)),
        ("crt_dilation", int(0)),
    ];
    (port, fields)
}

#[test]
fn translation_uses_the_port_names() {
    assert_eq!(PortEffect::Anime4kS.magpie_name(), ANIME4K_EFFECT);

    // Anime4K: no parameters, the short form.
    let map = legacy_map(&[("anime4k_enabled", Value::from(true))]);
    let mut short = map.clone();
    assert!(legacy::translate_native(&mut short, &mut Vec::new()));
    let list = short
        .get("magpie_effects")
        .and_then(Value::as_sequence)
        .expect("a magpie_effects list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].as_str(), Some(ANIME4K_EFFECT));
    assert_eq!(translated_chain(map), [expected(&PortEffect::Anime4kS)]);

    // Every parameter off its default: all of them, with the port's names.
    let (luma, fields) = luma_tuned();
    assert_eq!(luma.magpie_name(), LUMA_SHARPEN_EFFECT);
    let want = expected(&luma);
    assert_eq!(want.params.len(), 4);
    assert_eq!(translated_chain(legacy_map(&fields)), [want]);

    let (crt, fields) = crt_tuned();
    assert_eq!(crt.magpie_name(), CRT_EFFECT);
    let want = expected(&crt);
    assert_eq!(want.params.len(), 17);
    assert_eq!(translated_chain(legacy_map(&fields)), [want]);

    // One parameter changed: only that one goes out.
    let luma = PortEffect::LumaSharpen {
        strength: 1.25,
        clamp: 0.035,
        pattern: 1,
        offset_bias: 1.0,
    };
    let fields = [
        ("luma_sharpen_enabled", Value::from(true)),
        ("luma_sharpen_strength", float(1.25)),
    ];
    let want = expected(&luma);
    assert_eq!(want.params.keys().collect::<Vec<_>>(), ["sharpStrength"]);
    assert_eq!(translated_chain(legacy_map(&fields)), [want]);

    // The three together: the native order (Anime4K, LumaSharpen, CRT).
    let (luma, luma_fields) = luma_tuned();
    let (crt, crt_fields) = crt_tuned();
    let mut fields = crt_fields;
    fields.extend(luma_fields);
    fields.push(("anime4k_enabled", Value::from(true)));
    assert_eq!(
        translated_chain(legacy_map(&fields)),
        [expected(&PortEffect::Anime4kS), expected(&luma), expected(&crt)]
    );
}

// ---------------------------------------------------------------------------
// GPU: each port against its MagpieFX twin
// ---------------------------------------------------------------------------

/// The test image of `make_input` in `scripts/rust-parity-refs.py` (and of
/// the parity tests), read as BGRA bytes: what the capture uploads.
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
            // BGRA order.
            out.extend_from_slice(&[b as u8, g as u8, r as u8, 255]);
        }
    }
    out
}

fn device() -> Arc<Device> {
    let instance = Instance::new(&InstanceOptions {
        app_name: "lirtup-ports".into(),
        debug: false,
    })
    .expect("Vulkan instance");
    let gpu = std::env::var("LIRTUP_GPU").ok();
    let index = instance.select_adapter(gpu.as_deref()).expect("a Vulkan adapter");
    instance.create_device(index).expect("Vulkan device")
}

/// Tight 8-bit bytes of an R8G8B8A8 texture.
fn download_rgba8(tex: &Texture) -> Result<Vec<u8>, String> {
    if tex.format() != Format::R8G8B8A8Unorm {
        return Err(format!("output format {:?} is not compared", tex.format()));
    }
    let (w, h) = (tex.width() as usize, tex.height() as usize);
    let data = tex.download().map_err(|e| format!("download: {e}"))?;
    let pitch = (tex.row_pitch() as usize).max(w * 4);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        out.extend_from_slice(&data[y * pitch..y * pitch + w * 4]);
    }
    Ok(out)
}

fn run(device: &Arc<Device>, pass: &dyn Pass) -> Result<(), String> {
    let mut recorder = device.recorder().map_err(|e| format!("recorder: {e}"))?;
    pass.record(&mut recorder).map_err(|e| format!("record: {e}"))?;
    device
        .submit(recorder, &SubmitOptions::default())
        .map_err(|e| format!("submit: {e}"))?
        .wait(10_000_000_000)
        .map_err(|e| format!("wait: {e}"))?;
    Ok(())
}

/// What the GPU comparison needs besides the case.
struct Gpu<'a> {
    device: &'a Arc<Device>,
    src: &'a Texture,
    target: (u32, u32),
    catalog: &'a Catalog,
    compilers: &'a [Box<dyn HlslCompiler>],
    cache: &'a SpirvCache,
}

/// Run the port and its MagpieFX twin over the same input; the largest
/// difference of one channel.
fn max_difference(gpu: &Gpu<'_>, port: &PortEffect) -> Result<u8, String> {
    let input = (gpu.src.width(), gpu.src.height());

    let pass = port
        .create(gpu.device, gpu.src, port.output_size(input, gpu.target))
        .map_err(|e| format!("port: {e}"))?;
    run(gpu.device, pass.as_ref())?;

    // The chain of the app: BGRA in and out, the swap folded into the
    // shaders; FP32 like the port.
    let options = CompileOptions {
        fp16: false,
        swap_input: true,
        swap_output: true,
    };
    let entry = gpu
        .catalog
        .resolve(port.magpie_name())
        .map_err(|e| format!("catalog: {e}"))?;
    let compiled = compile_effect_with(&entry, gpu.compilers, gpu.cache, &options)
        .map_err(|e| format!("compile: {e}"))?;
    if !compiled.input_swapped() || !compiled.output_swapped() {
        return Err("the red/blue swap was not folded into the shaders".to_owned());
    }
    let size = MagpieEffect::output_size(&compiled, input, gpu.target).map_err(|e| format!("size: {e}"))?;
    let effect = MagpieEffect::new(gpu.device, &compiled, gpu.src, size, &port.magpie_params())
        .map_err(|e| format!("bind: {e}"))?;
    run(gpu.device, &effect)?;

    let (a, b) = (pass.output(), effect.output());
    if (a.width(), a.height()) != (b.width(), b.height()) {
        return Err(format!(
            "port {}x{}, MagpieFX {}x{}",
            a.width(),
            a.height(),
            b.width(),
            b.height()
        ));
    }
    let (a, b) = (download_rgba8(a)?, download_rgba8(b)?);
    Ok(a.iter().zip(&b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0))
}

#[test]
#[ignore = "needs a Vulkan GPU and an HLSL compiler or the seed"]
fn ports_match_magpiefx() {
    let (w, h) = (160, 96);
    let device = device();
    let src = util::texture(&device, w, h, Format::R8G8B8A8Unorm, "ports input").expect("input texture");
    util::upload_texture(&device, &src, &input_image(w, h)).expect("upload");

    // No compiler is fine when the seed has the bundled effects; a compile
    // error then fails the case with its message.
    let compilers = detect_compilers();
    let cache_dir = tempfile::tempdir().expect("temporary folder");
    let cache = SpirvCache::new(cache_dir.path().to_owned());
    let catalog = Catalog::scan(None);
    let gpu = Gpu {
        device: &device,
        src: &src,
        // CRT renders at the size it is given: 2x, like the parity case.
        target: (320, 192),
        catalog: &catalog,
        compilers: &compilers,
        cache: &cache,
    };

    let cases: Vec<(&str, PortEffect, u8)> = vec![
        ("Anime4K_Upscale_S", PortEffect::Anime4kS, 4),
        ("LumaSharpen", PortEffect::LUMA_SHARPEN_DEFAULT, 2),
        ("LumaSharpen tuned", luma_tuned().0, 2),
        (
            "LumaSharpen pattern 0",
            PortEffect::LumaSharpen {
                strength: 1.0,
                clamp: 0.05,
                pattern: 0,
                offset_bias: 1.0,
            },
            2,
        ),
        ("CRT_Easymode", PortEffect::CrtEasymode(CrtParams::default()), 2),
        ("CRT_Easymode tuned", crt_tuned().0, 2),
    ];
    let only = std::env::var("LIRTUP_PORTS_CASE").ok();

    let mut failures = Vec::new();
    for (label, port, allowed) in &cases {
        if only.as_deref().is_some_and(|o| o != *label) {
            continue;
        }
        match max_difference(&gpu, port) {
            Ok(diff) => {
                println!("{label}: max difference {diff} (allowed {allowed})");
                if diff > *allowed {
                    failures.push(format!("{label}: max difference {diff} > {allowed}"));
                }
            }
            Err(e) => failures.push(format!("{label}: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn input_image_is_bgra() {
    let img = input_image(160, 96);
    let px = |x: usize, y: usize| &img[(y * 160 + x) * 4..][..4];
    assert_eq!(px(0, 0), &[255, 255, 255, 255]);
    // RGB (1, 0, 7) stored as BGRA.
    assert_eq!(px(1, 0), &[7, 0, 1, 255]);
    assert_eq!(px(124, 48), &[64, 64, 64, 255]);
}
