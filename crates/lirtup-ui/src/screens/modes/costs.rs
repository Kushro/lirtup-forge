//! The cost of each pass in ms per frame, from the last measurements:
//! `bench.json` next to the configuration (what `lirtup bench` and «Medir
//! en mi GPU» save, per GPU) with the last `BenchResult` of this run on top.
//!
//! The native passes are keyed by their label in the bench (`cas`,
//! `anime4k`, `crt_easymode`...), MagpieFX effects by catalog name. The
//! SRCNN model is not measured on its own, so a mode's total is the sum of
//! its effects.

use std::collections::HashMap;
use std::path::Path;

use dioxus::prelude::*;
use lirtup_config::{EffectsEngine, ModeSettings};
use lirtup_proto::{BenchEntry, BenchKind, BenchReport, GpuInfo};
use serde_json::Value as Json;

use crate::app::use_store;

/// `bench.json` (`lirtup_pipeline::bench::results_path`).
const RESULTS_FILE: &str = "bench.json";

/// Measured costs for one GPU.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Costs {
    /// Native passes by bench label.
    pub native: HashMap<String, f32>,
    /// MagpieFX effects by name.
    pub magpie: HashMap<String, f32>,
    /// The GPU's name, for «Total ≈ … · RX 9070 XT».
    pub gpu: Option<String>,
}

impl Costs {
    /// A native pass.
    pub fn native(&self, label: &str) -> Option<f32> {
        self.native.get(label).copied()
    }

    /// A MagpieFX effect.
    pub fn magpie(&self, name: &str) -> Option<f32> {
        self.magpie.get(name).copied()
    }

    fn add(&mut self, entries: &[BenchEntry]) {
        for e in entries {
            let Some(ms) = e.ms.filter(|_| e.error.is_none()) else {
                continue;
            };
            match e.kind {
                BenchKind::Native => {
                    self.native.insert(e.name.clone(), ms);
                }
                BenchKind::Magpiefx => {
                    self.magpie.insert(e.name.clone(), ms);
                }
                BenchKind::Framegen => {}
            }
        }
    }

    /// What a mode costs: its enabled passes, `None` if one of them was
    /// never measured (or it has none measured at all).
    pub fn mode(&self, mode: &ModeSettings, chain: &[String]) -> Option<f32> {
        let mut total = 0.0;
        let mut any = false;
        for label in native_passes(mode) {
            total += self.native(label)?;
            any = true;
        }
        if mode.effects_engine == EffectsEngine::Magpiefx {
            for name in chain {
                total += self.magpie(name)?;
                any = true;
            }
        }
        any.then_some(total)
    }
}

/// The bench labels of the native passes a mode runs (the order of
/// `NativeEffect::from_mode`).
pub(crate) fn native_passes(mode: &ModeSettings) -> Vec<&'static str> {
    let native = mode.effects_engine == EffectsEngine::Native;
    [
        (mode.deband_enabled, "deband"),
        (native && mode.anime4k_enabled, "anime4k"),
        (native && mode.luma_sharpen_enabled, "luma_sharpen"),
        (native && mode.crt_enabled, "crt_easymode"),
        (mode.cas_enabled, "cas"),
        (mode.bloom_enabled, "bloom"),
        (mode.vignette_enabled, "vignette"),
        (mode.grain_enabled, "grain"),
        (mode.lut_enabled, "lut"),
    ]
    .into_iter()
    .filter_map(|(on, label)| on.then_some(label))
    .collect()
}

/// The key `bench.json` files a GPU under (`vendor:device:name`).
pub(crate) fn gpu_key(gpu: &GpuInfo) -> String {
    format!("{:04x}:{:04x}:{}", gpu.vendor_id, gpu.device_id, gpu.name)
}

/// The costs of `gpu` in a `bench.json` (`None` = the only GPU in it).
pub(crate) fn from_results(data: &Json, gpu: Option<&str>) -> Costs {
    let mut costs = Costs::default();
    let Some(gpus) = data.get("gpus").and_then(Json::as_object) else {
        return costs;
    };
    let entry = match gpu {
        Some(key) => gpus.get(key),
        None => None,
    }
    .or_else(|| (gpus.len() == 1).then(|| gpus.values().next()).flatten());
    let Some(entry) = entry else {
        return costs;
    };
    costs.gpu = entry.get("name").and_then(Json::as_str).map(str::to_owned);
    for (kind, map) in [("native", &mut costs.native), ("magpiefx", &mut costs.magpie)] {
        let Some(cases) = entry.get(kind).and_then(Json::as_object) else {
            continue;
        };
        for (name, case) in cases {
            if case.get("error").is_some_and(|e| !e.is_null()) {
                continue;
            }
            if let Some(ms) = case.get("ms").and_then(Json::as_f64) {
                map.insert(name.clone(), ms as f32);
            }
        }
    }
    costs
}

fn read_results(path: &Path) -> Json {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Json::Null)
}

/// The costs for the GPU in use: `bench.json` (read once per screen) and
/// this run's last report.
pub(crate) fn use_costs() -> Memo<Costs> {
    let store = use_store();
    let file = use_hook(|| {
        std::rc::Rc::new(read_results(
            &lirtup_config::paths::config_dir().join(RESULTS_FILE),
        ))
    });
    use_memo(move || {
        let s = store.read();
        let gpu = s.gpus.iter().find(|g| g.selected).or(s.gpus.first());
        let mut costs = from_results(&file, gpu.map(gpu_key).as_deref());
        if let Some(gpu) = gpu {
            costs.gpu = Some(gpu.name.clone());
        }
        if let Some(report) = &s.bench.report {
            apply_report(&mut costs, report, gpu);
        }
        costs
    })
}

/// A report on top of the file, if it is of the same GPU.
fn apply_report(costs: &mut Costs, report: &BenchReport, gpu: Option<&GpuInfo>) {
    if gpu.is_none_or(|g| gpu_key(g) == report.gpu) {
        costs.add(&report.entries);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> Json {
        serde_json::json!({
            "version": 1,
            "gpus": {
                "1002:7550:AMD Radeon RX 9070 XT": {
                    "name": "AMD Radeon RX 9070 XT",
                    "native": {
                        "cas": {"ms": 0.1, "p95": 0.12, "frames": 100, "error": null},
                        "anime4k": {"ms": 0.9, "p95": 1.0, "frames": 100, "error": null},
                        "bloom": {"ms": null, "p95": null, "frames": 0, "error": "boom"}
                    },
                    "magpiefx": {
                        "LumaSharpen": {"ms": 0.2, "p95": 0.3, "frames": 100, "error": null}
                    }
                }
            }
        })
    }

    #[test]
    fn reads_the_gpu_and_skips_errors() {
        let costs = from_results(&file(), Some("1002:7550:AMD Radeon RX 9070 XT"));
        assert_eq!(costs.native("cas"), Some(0.1));
        assert_eq!(costs.native("bloom"), None);
        assert_eq!(costs.magpie("LumaSharpen"), Some(0.2));
        assert_eq!(costs.gpu.as_deref(), Some("AMD Radeon RX 9070 XT"));
        // The only GPU when the key does not match.
        assert_eq!(from_results(&file(), Some("other")).native("cas"), Some(0.1));
        assert_eq!(from_results(&Json::Null, None), Costs::default());
    }

    #[test]
    fn a_mode_costs_the_sum_of_its_passes() {
        let costs = from_results(&file(), None);
        let mut mode = ModeSettings {
            cas_enabled: true,
            anime4k_enabled: true,
            ..ModeSettings::default()
        };
        let total = costs.mode(&mode, &[]).unwrap();
        assert!((total - 1.0).abs() < 1e-6);
        // MagpieFX: the native anime4k does not run, the chain does.
        mode.effects_engine = EffectsEngine::Magpiefx;
        let total = costs.mode(&mode, &["LumaSharpen".into()]).unwrap();
        assert!((total - 0.3).abs() < 1e-6);
        // Something never measured: unknown total.
        assert_eq!(costs.mode(&mode, &["Other".into()]), None);
        assert_eq!(costs.mode(&ModeSettings::default(), &[]), None);
    }
}
