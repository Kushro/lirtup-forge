//! What `ListDevices`, `ListEffects`, `Diagnostics` and `lirtup list`
//! report, as the protocol names it (`config/listing.py` of 1.x).
//!
//! Everything here blocks (X11 round trips, a catalog scan, compiler
//! `--version`): the engine runs it on a thread of its own.

use lirtup_config::Config;
use lirtup_gpu::{AdapterKind, Instance};
use lirtup_magpiefx::{Catalog, CatalogEntry};
use lirtup_x11::{WindowInfo, X11};

use crate::Result;
use crate::proto::{EffectInfo, EffectOrigin, EffectParamInfo, GpuInfo, MonitorInfo, ParamKind, WindowRef};

/// A window as the protocol names it.
pub fn window_ref(window: &WindowInfo) -> WindowRef {
    WindowRef {
        xid: window.xid,
        title: window.title.clone(),
        class: window.class.as_ref().map(|c| c.class.clone()),
        pid: window.pid,
    }
}

/// The windows a user could scale (`lirtup list windows`, `--select`).
pub fn windows() -> Result<Vec<WindowInfo>> {
    Ok(X11::connect()?.list_windows()?)
}

/// The monitors (RandR).
pub fn monitors(x11: &X11) -> Result<Vec<MonitorInfo>> {
    Ok(x11
        .monitors()?
        .into_iter()
        .map(|m| MonitorInfo {
            index: m.index,
            name: m.name,
            x: m.rect.x,
            y: m.rect.y,
            width: m.rect.width,
            height: m.rect.height,
            primary: m.primary,
            refresh_hz: m.refresh_hz,
        })
        .collect())
}

/// The `gpu:` option as `Instance::select_adapter` takes it.
pub fn gpu_selector(config: &Config) -> Option<String> {
    config.gpu.as_ref().map(|g| match g {
        lirtup_config::GpuSelector::Index(i) => i.to_string(),
        lirtup_config::GpuSelector::Name(n) => n.clone(),
    })
}

/// The adapters, with the one `selector` resolves to marked. The instance
/// must exist already (created after `lirtup_fg::layer::prepare`).
pub fn gpus(instance: &Instance, selector: Option<&str>) -> Vec<GpuInfo> {
    let selected = instance.select_adapter(selector);
    instance
        .adapters()
        .iter()
        .map(|a| GpuInfo {
            index: a.index as u32,
            name: a.name.clone(),
            vendor_id: a.vendor_id,
            device_id: a.device_id,
            discrete: a.kind == AdapterKind::Discrete,
            driver: a.driver.clone(),
            fp16: a.supports_fp16,
            external_fd: a.supports_external_fd,
            selected: selected == Some(a.index),
        })
        .collect()
}

/// The MagpieFX catalog: built-in effects and the user's folder.
pub fn catalog() -> Catalog {
    Catalog::scan(Some(&lirtup_config::paths::user_effects_dir()))
}

/// One catalog entry, parsed for its passes and parameters. A file that
/// does not parse is listed with `error` set.
pub fn effect_info(catalog: &Catalog, entry: &CatalogEntry) -> EffectInfo {
    let origin = match entry.origin {
        lirtup_magpiefx::EffectOrigin::Builtin => EffectOrigin::Builtin,
        lirtup_magpiefx::EffectOrigin::User => EffectOrigin::User,
    };
    match catalog.describe(entry) {
        Ok(desc) => EffectInfo {
            name: entry.name.clone(),
            sort_name: if desc.sort_name.is_empty() {
                entry.name.clone()
            } else {
                desc.sort_name.clone()
            },
            passes: desc.passes.len() as u32,
            params: desc
                .params
                .iter()
                .map(|p| EffectParamInfo {
                    name: p.name.clone(),
                    label: p.label.clone(),
                    kind: match p.kind {
                        lirtup_magpiefx::ParamKind::Float => ParamKind::Float,
                        lirtup_magpiefx::ParamKind::Int => ParamKind::Int,
                    },
                    default: p.default,
                    min: p.min,
                    max: p.max,
                    step: p.step,
                })
                .collect(),
            verified: entry.verified,
            origin,
            fp16: desc.supports_fp16,
            error: None,
        },
        Err(e) => EffectInfo {
            name: entry.name.clone(),
            sort_name: entry.name.clone(),
            passes: 0,
            params: Vec::new(),
            verified: entry.verified,
            origin,
            fp16: false,
            error: Some(e.to_string()),
        },
    }
}

/// Every effect of the catalog, parsed.
pub fn effects() -> Vec<EffectInfo> {
    let catalog = catalog();
    catalog.entries.iter().map(|e| effect_info(&catalog, e)).collect()
}

/// `--version` of the HLSL compilers found: `(dxc, glslang)`.
pub fn compiler_versions() -> (Option<String>, Option<String>) {
    let mut dxc = None;
    let mut glslang = None;
    for compiler in lirtup_magpiefx::compiler::detect_compilers() {
        let version = compiler.version().unwrap_or_else(|| "unknown version".into());
        if compiler.name().contains("dxc") {
            dxc.get_or_insert(version);
        } else if compiler.name().contains("glslang") {
            glslang.get_or_insert(version);
        }
    }
    (dxc, glslang)
}
