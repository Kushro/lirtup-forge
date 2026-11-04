//! The retired native effects engine (fx-unify round): its keys and their
//! translation to MagpieFX effects.
//!
//! Up to 2.1 a mode chose `effects_engine: native` (the default: the hand
//! ports of Anime4K_Upscale_S, LumaSharpen and CRT_Easymode, driven by 24
//! loose fields) or `effects_engine: magpiefx` (the `magpie_effects`
//! chain); only the chosen one ran. Since fx-unify the MagpieFX chain is the
//! only one. Files written before keep working without migrating: the
//! resolver calls [`translate_native`] on the merged mapping of the file
//! layers (top level, mode, profile), before the command line goes on top
//! and before it becomes a [`crate::Config`]. `lirtup migrate` (and the
//! GUI's migration dialog) rewrite the files with the same function.
//!
//! The rule and the parameter table are in
//! `docs/developer/fx-unify-contract.md` (§2.2).

use std::collections::BTreeMap;

use crate::util::display;
use crate::{ConfigWarning, MagpieEffectSpec, Mapping, Value};

/// Every key of the native engine, in the old `MODE_KEYS` order:
/// `effects_engine`, then the 24 fields of the three ports.
pub const LEGACY_KEYS: [&str; 25] = [
    "effects_engine",
    "anime4k_enabled",
    "luma_sharpen_enabled",
    "luma_sharpen_strength",
    "luma_sharpen_clamp",
    "luma_sharpen_pattern",
    "luma_sharpen_offset_bias",
    "crt_enabled",
    "crt_sharpness_h",
    "crt_sharpness_v",
    "crt_mask_strength",
    "crt_mask_dot_width",
    "crt_mask_dot_height",
    "crt_mask_stagger",
    "crt_mask_size",
    "crt_scanline_strength",
    "crt_scanline_beam_width_min",
    "crt_scanline_beam_width_max",
    "crt_scanline_bright_min",
    "crt_scanline_bright_max",
    "crt_scanline_cutoff",
    "crt_gamma_input",
    "crt_gamma_output",
    "crt_bright_boost",
    "crt_dilation",
];

/// The MagpieFX catalog names the native effects become (what the GUI's
/// picker writes, so the chain cards find their catalog entry).
pub const ANIME4K_EFFECT: &str = "Anime4K/Anime4K_Upscale_S";
/// See [`ANIME4K_EFFECT`].
pub const LUMA_SHARPEN_EFFECT: &str = "Sharpen/LumaSharpen";
/// See [`ANIME4K_EFFECT`].
pub const CRT_EFFECT: &str = "CRT/CRT_Easymode";

/// Whether `map` holds any key of [`LEGACY_KEYS`] (with any value, `null`
/// included).
pub fn has_legacy_keys(map: &Mapping) -> bool {
    map.keys()
        .filter_map(Value::as_str)
        .any(|k| LEGACY_KEYS.contains(&k))
}

/// How a native field is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// An old `f32` field: any YAML number.
    Float,
    /// An old `u32` field: a non-negative integer.
    Int,
}

/// One native field and the MagpieFX parameter it becomes: same default and
/// same (inclusive) range on both sides.
#[derive(Debug, Clone, Copy)]
struct Param {
    /// The native field (and the key of its warnings).
    field: &'static str,
    /// The `//!PARAMETER` of the MagpieFX effect.
    param: &'static str,
    kind: Kind,
    default: f64,
    min: f64,
    max: f64,
}

const fn param(
    field: &'static str,
    param: &'static str,
    kind: Kind,
    default: f64,
    min: f64,
    max: f64,
) -> Param {
    Param {
        field,
        param,
        kind,
        default,
        min,
        max,
    }
}

/// The parameters of `Sharpen/LumaSharpen`.
#[rustfmt::skip]
const LUMA_PARAMS: [Param; 4] = [
    param("luma_sharpen_strength", "sharpStrength", Kind::Float, 0.65, 0.0, 3.0),
    param("luma_sharpen_clamp", "sharpClamp", Kind::Float, 0.035, 0.0, 1.0),
    param("luma_sharpen_pattern", "pattern", Kind::Int, 1.0, 0.0, 3.0),
    param("luma_sharpen_offset_bias", "offsetBias", Kind::Float, 1.0, 0.0, 6.0),
];

/// The parameters of `CRT/CRT_Easymode`.
#[rustfmt::skip]
const CRT_PARAMS: [Param; 17] = [
    param("crt_sharpness_h", "sharpnessH", Kind::Float, 0.5, 0.0, 1.0),
    param("crt_sharpness_v", "sharpnessV", Kind::Float, 1.0, 0.0, 1.0),
    param("crt_mask_strength", "maskStrength", Kind::Float, 0.3, 0.0, 1.0),
    param("crt_mask_dot_width", "maskDotWidth", Kind::Int, 1.0, 1.0, 100.0),
    param("crt_mask_dot_height", "maskDotHeight", Kind::Int, 1.0, 1.0, 100.0),
    param("crt_mask_stagger", "maskStagger", Kind::Int, 0.0, 0.0, 100.0),
    param("crt_mask_size", "maskSize", Kind::Int, 1.0, 1.0, 100.0),
    param("crt_scanline_strength", "scanlineStrength", Kind::Float, 1.0, 0.0, 1.0),
    param("crt_scanline_beam_width_min", "scanlineBeamWidthMin", Kind::Float, 1.5, 0.5, 5.0),
    param("crt_scanline_beam_width_max", "scanlineBeamWidthMax", Kind::Float, 1.5, 0.5, 5.0),
    param("crt_scanline_bright_min", "scanlineBrightMin", Kind::Float, 0.35, 0.0, 1.0),
    param("crt_scanline_bright_max", "scanlineBrightMax", Kind::Float, 0.65, 0.0, 1.0),
    param("crt_scanline_cutoff", "scanlineCutoff", Kind::Int, 400.0, 1.0, 1000.0),
    param("crt_gamma_input", "gammaInput", Kind::Float, 2.0, 0.1, 5.0),
    param("crt_gamma_output", "gammaOutput", Kind::Float, 1.8, 0.1, 5.0),
    param("crt_bright_boost", "brightBoost", Kind::Float, 1.2, 1.0, 2.0),
    param("crt_dilation", "dilation", Kind::Int, 1.0, 0.0, 1.0),
];

/// The native effects in the order the native engine ran them: the field
/// that enables each one, its catalog name and its parameters.
const EFFECTS: [(&str, &str, &[Param]); 3] = [
    ("anime4k_enabled", ANIME4K_EFFECT, &[]),
    ("luma_sharpen_enabled", LUMA_SHARPEN_EFFECT, &LUMA_PARAMS),
    ("crt_enabled", CRT_EFFECT, &CRT_PARAMS),
];

/// `map[key]`, with `null` read as absent.
fn present<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(key).filter(|v| !v.is_null())
}

/// Whether the native engine was the one running (absent, `null`,
/// `native`, or a bad value with a warning).
fn native_engine(map: &Mapping, warnings: &mut Vec<ConfigWarning>) -> bool {
    match present(map, "effects_engine") {
        None => true,
        Some(Value::String(s)) if s == "native" => true,
        Some(Value::String(s)) if s == "magpiefx" => false,
        Some(other) => {
            warnings.push(ConfigWarning {
                key: "effects_engine".to_owned(),
                message: format!("invalid value {}, using native", display(other)),
            });
            true
        }
    }
}

/// An old boolean field: only YAML booleans count.
fn read_bool(map: &Mapping, field: &str, warnings: &mut Vec<ConfigWarning>) -> bool {
    match present(map, field) {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => {
            warnings.push(ConfigWarning {
                key: field.to_owned(),
                message: format!("invalid value {}, using the default false", display(other)),
            });
            false
        }
    }
}

/// An old numeric field, checked like the old validators: the default when
/// absent, of the wrong type (with a warning) or out of range (with a
/// warning).
fn read_param(map: &Mapping, p: &Param, warnings: &mut Vec<ConfigWarning>) -> f64 {
    let Some(value) = present(map, p.field) else {
        return p.default;
    };
    let read = match (p.kind, value) {
        (Kind::Float, Value::Number(n)) => n.as_f64(),
        (Kind::Int, Value::Number(n)) => n.as_u64().map(|v| v as f64),
        _ => None,
    };
    let Some(v) = read else {
        warnings.push(ConfigWarning {
            key: p.field.to_owned(),
            message: format!(
                "invalid value {}, using the default {}",
                display(value),
                p.default
            ),
        });
        return p.default;
    };
    if !(p.min..=p.max).contains(&v) {
        warnings.push(ConfigWarning {
            key: p.field.to_owned(),
            message: format!(
                "must be {} to {}, got {v}; using the default {}",
                p.min, p.max, p.default
            ),
        });
        return p.default;
    }
    v
}

/// The `magpie_effects` list the native settings of `map` stand for: the
/// enabled effects in the native order, each with its non-default
/// parameters (short form when it has none, as the GUI writes it).
fn native_chain(map: &Mapping, warnings: &mut Vec<ConfigWarning>) -> Vec<MagpieEffectSpec> {
    let mut chain = Vec::new();
    for (enabled, name, params) in EFFECTS {
        let on = read_bool(map, enabled, warnings);
        // Every field is checked, enabled or not, as the old validators did.
        let mut values = BTreeMap::new();
        for p in params {
            let v = read_param(map, p, warnings);
            if v != p.default {
                values.insert(p.param.to_owned(), v);
            }
        }
        if !on {
            continue;
        }
        chain.push(if values.is_empty() {
            MagpieEffectSpec::Short(name.to_owned())
        } else {
            MagpieEffectSpec::Full {
                name: name.to_owned(),
                params: values,
            }
        });
    }
    chain
}

/// Translate the native engine of `map` in place (see the module docs and
/// §2.2 of the contract): only when `map` holds a legacy key. With the
/// native engine (`effects_engine` absent, `native`, or a bad value),
/// `magpie_effects` becomes the enabled native effects in the native order
/// (Anime4K, LumaSharpen, CRT) with their non-default parameters, or goes
/// away when none is enabled; with `magpiefx` it is kept. Then every legacy
/// key is removed; `magpie_effects_before_model` is never touched. Bad
/// values (wrong type, out of the old range) keep the old default and push
/// a warning, as the old validators did.
///
/// The key order of `map` is kept: a new `magpie_effects` goes where the old
/// one was, or else where the first legacy key was (so a rewritten mode file
/// reads as before).
///
/// Returns whether `map` changed.
pub fn translate_native(map: &mut Mapping, warnings: &mut Vec<ConfigWarning>) -> bool {
    if !has_legacy_keys(map) {
        return false;
    }
    // `None`: keep `magpie_effects` as it is (the MagpieFX engine was
    // chosen); `Some(list)`: the native engine ran, its effects replace the
    // inactive list (none: the list goes).
    let replacement: Option<Value> = if native_engine(map, warnings) {
        let chain = native_chain(map, warnings);
        Some(if chain.is_empty() {
            Value::Null
        } else {
            serde_yaml_ng::to_value(&chain).unwrap_or(Value::Null)
        })
    } else {
        None
    };
    let had_chain = map.contains_key("magpie_effects");
    let old = std::mem::take(map);
    let mut placed = false;
    for (key, value) in old {
        let name = key.as_str();
        if name == Some("magpie_effects") {
            match &replacement {
                None => {
                    map.insert(key, value);
                }
                Some(Value::Null) => {}
                Some(list) => {
                    map.insert(key, list.clone());
                }
            }
            placed = true;
            continue;
        }
        if name.is_some_and(|k| LEGACY_KEYS.contains(&k)) {
            if !had_chain
                && !placed
                && let Some(list) = replacement.as_ref().filter(|v| !v.is_null())
            {
                map.insert("magpie_effects".into(), list.clone());
                placed = true;
            }
            continue;
        }
        map.insert(key, value);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(yaml: &str) -> Mapping {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    fn translated(yaml: &str) -> (Mapping, Vec<ConfigWarning>) {
        let mut m = map(yaml);
        let mut warnings = Vec::new();
        assert!(translate_native(&mut m, &mut warnings), "{yaml}");
        assert!(!has_legacy_keys(&m), "{m:?}");
        (m, warnings)
    }

    fn chain(m: &Mapping) -> Vec<MagpieEffectSpec> {
        serde_yaml_ng::from_value(m["magpie_effects"].clone()).unwrap()
    }

    fn keys(m: &Mapping) -> Vec<&str> {
        m.keys().filter_map(Value::as_str).collect()
    }

    fn full(name: &str, params: &[(&str, f64)]) -> MagpieEffectSpec {
        MagpieEffectSpec::Full {
            name: name.to_owned(),
            params: params.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        }
    }

    #[test]
    fn legacy_keys_are_not_mode_keys() {
        for key in LEGACY_KEYS {
            assert!(!crate::MODE_KEYS.contains(&key), "{key}");
        }
    }

    #[test]
    fn finds_legacy_keys() {
        let map: Mapping = serde_yaml_ng::from_str("model: fast\ncrt_enabled: null\n").unwrap();
        assert!(has_legacy_keys(&map));
        let map: Mapping = serde_yaml_ng::from_str("model: fast\nmagpie_effects: [CAS]\n").unwrap();
        assert!(!has_legacy_keys(&map));
    }

    #[test]
    fn the_table_covers_every_native_field() {
        let mut fields: Vec<&str> = vec!["effects_engine"];
        for (enabled, _, params) in EFFECTS {
            fields.push(enabled);
            fields.extend(params.iter().map(|p| p.field));
        }
        fields.sort_unstable();
        let mut legacy = LEGACY_KEYS.to_vec();
        legacy.sort_unstable();
        assert_eq!(fields, legacy);
        for p in LUMA_PARAMS.iter().chain(&CRT_PARAMS) {
            assert!(p.min <= p.default && p.default <= p.max, "{}", p.field);
        }
    }

    #[test]
    fn nothing_legacy_is_untouched() {
        let yaml = "model: fast\nmagpie_effects: [CAS]\nmagpie_effects_before_model: [Bicubic]\n";
        let mut m = map(yaml);
        let mut warnings = Vec::new();
        assert!(!translate_native(&mut m, &mut warnings));
        assert_eq!(m, map(yaml));
        assert!(warnings.is_empty());
    }

    #[test]
    fn anime4k_becomes_the_short_form() {
        let (m, w) = translated("model: fast\nanime4k_enabled: true\n");
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(chain(&m), vec![MagpieEffectSpec::Short(ANIME4K_EFFECT.into())]);
        assert_eq!(keys(&m), ["model", "magpie_effects"]);
    }

    #[test]
    fn only_non_default_parameters_go_out() {
        let (m, w) = translated(
            "luma_sharpen_enabled: true\nluma_sharpen_strength: 1.2\nluma_sharpen_clamp: 0.035\n\
             luma_sharpen_pattern: 1\n",
        );
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            chain(&m),
            vec![full(LUMA_SHARPEN_EFFECT, &[("sharpStrength", 1.2)])]
        );
    }

    #[test]
    fn defaults_are_omitted() {
        let (m, _) = translated(
            "crt_enabled: true\ncrt_sharpness_h: 0.5\ncrt_mask_size: 1\ncrt_scanline_cutoff: 400\n\
             crt_gamma_output: 1.8\n",
        );
        assert_eq!(chain(&m), vec![MagpieEffectSpec::Short(CRT_EFFECT.into())]);
    }

    #[test]
    fn all_three_in_the_native_order() {
        let (m, w) = translated(
            "crt_enabled: true\ncrt_mask_size: 2\ncrt_scanline_strength: 0.5\nluma_sharpen_enabled: true\n\
             anime4k_enabled: true\neffects_engine: native\n",
        );
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            chain(&m),
            vec![
                MagpieEffectSpec::Short(ANIME4K_EFFECT.into()),
                MagpieEffectSpec::Short(LUMA_SHARPEN_EFFECT.into()),
                full(CRT_EFFECT, &[("maskSize", 2.0), ("scanlineStrength", 0.5)]),
            ]
        );
    }

    #[test]
    fn bad_values_warn_and_keep_the_default() {
        let (m, w) = translated(
            "crt_enabled: true\ncrt_mask_size: 2.5\ncrt_mask_stagger: -1\ncrt_sharpness_h: 3\n\
             crt_gamma_input: fast\ncrt_dilation: 1\nluma_sharpen_enabled: yes please\n\
             luma_sharpen_pattern: 7\n",
        );
        let warned: Vec<&str> = w.iter().map(|w| w.key.as_str()).collect();
        assert_eq!(
            warned,
            [
                "luma_sharpen_enabled",
                "luma_sharpen_pattern",
                "crt_sharpness_h",
                "crt_mask_stagger",
                "crt_mask_size",
                "crt_gamma_input",
            ]
        );
        let range = &w[2];
        assert_eq!(range.message, "must be 0 to 1, got 3; using the default 0.5");
        // Everything went back to its default: the CRT has no parameters,
        // and the LumaSharpen is off.
        assert_eq!(chain(&m), vec![MagpieEffectSpec::Short(CRT_EFFECT.into())]);
    }

    #[test]
    fn a_bad_engine_is_native() {
        let (m, w) = translated("effects_engine: shiny\nanime4k_enabled: true\nmagpie_effects: [CAS]\n");
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].key, "effects_engine");
        assert_eq!(chain(&m), vec![MagpieEffectSpec::Short(ANIME4K_EFFECT.into())]);
    }

    #[test]
    fn the_magpiefx_engine_keeps_its_list() {
        let (m, w) = translated(
            "model: none\neffects_engine: magpiefx\nmagpie_effects:\n- CAS:sharpness=0.5\n- name: Lanczos\n  \
             params: {}\ncrt_enabled: true\nluma_sharpen_strength: 2\n",
        );
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(keys(&m), ["model", "magpie_effects"]);
        let kept = map("x:\n- CAS:sharpness=0.5\n- name: Lanczos\n  params: {}\n");
        assert_eq!(m["magpie_effects"], kept["x"]);
        // Without a list, there is nothing to keep.
        let (m, _) = translated("effects_engine: magpiefx\ncrt_enabled: true\nmodel: fast\n");
        assert_eq!(keys(&m), ["model"]);
    }

    #[test]
    fn native_with_nothing_enabled_drops_the_inactive_list() {
        let (m, w) =
            translated("magpie_effects: [CAS]\nmodel: fast\neffects_engine: native\ncrt_mask_size: 3\n");
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(keys(&m), ["model"]);
    }

    #[test]
    fn null_values_are_absent() {
        let (m, w) = translated(
            "effects_engine: null\nanime4k_enabled: null\ncrt_enabled: true\ncrt_mask_size: null\n",
        );
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(chain(&m), vec![MagpieEffectSpec::Short(CRT_EFFECT.into())]);
        let (m, _) = translated("model: fast\ncrt_enabled: null\n");
        assert_eq!(keys(&m), ["model"]);
    }

    #[test]
    fn the_list_before_the_model_is_never_touched() {
        let (m, _) = translated(
            "magpie_effects_before_model: [Bicubic]\ncrt_enabled: true\nmagpie_effects_before_model_x: 1\n",
        );
        assert_eq!(
            keys(&m),
            [
                "magpie_effects_before_model",
                "magpie_effects",
                "magpie_effects_before_model_x"
            ]
        );
        assert_eq!(m["magpie_effects_before_model"], map("x: [Bicubic]\n")["x"]);
        let (m, _) = translated("magpie_effects_before_model: [Bicubic]\nanime4k_enabled: false\n");
        assert_eq!(keys(&m), ["magpie_effects_before_model"]);
    }

    #[test]
    fn the_key_order_is_kept() {
        // The new list goes where the old one was...
        let (m, _) =
            translated("name: CRT\nmagpie_effects: [CAS]\nmodel: none\ncrt_enabled: true\nfuture: 1\n");
        assert_eq!(keys(&m), ["name", "magpie_effects", "model", "future"]);
        // ...or where the first legacy key was.
        let (m, _) =
            translated("name: CRT\nmodel: none\neffects_engine: native\ncrt_enabled: true\nfuture: 1\n");
        assert_eq!(keys(&m), ["name", "model", "magpie_effects", "future"]);
    }

    #[test]
    fn reads_back_as_the_gui_writes_it() {
        let (m, _) = translated(
            "crt_enabled: true\ncrt_mask_dot_width: 2\nluma_sharpen_enabled: true\nanime4k_enabled: true\n",
        );
        let text = serde_yaml_ng::to_string(&m).unwrap();
        let back: Mapping = serde_yaml_ng::from_str(&text).unwrap();
        let (config, w) = crate::Config::from_mapping(&back);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            config.scaling.magpie_effects,
            vec![
                MagpieEffectSpec::Short(ANIME4K_EFFECT.into()),
                MagpieEffectSpec::Short(LUMA_SHARPEN_EFFECT.into()),
                full(CRT_EFFECT, &[("maskDotWidth", 2.0)]),
            ]
        );
        assert!(config.scaling.magpie_effects_before_model.is_empty());
        assert!(config.extra.is_empty(), "{:?}", config.extra);
    }
}
