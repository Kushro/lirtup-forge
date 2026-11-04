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

use crate::{ConfigWarning, Mapping, Value};

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

/// Translate the native engine of `map` in place (see the module docs and
/// §2.2 of the contract): only when `map` holds a legacy key. With the
/// native engine (`effects_engine` absent, `native`, or a bad value),
/// `magpie_effects` becomes the enabled native effects in the native order
/// (Anime4K, LumaSharpen, CRT) with their non-default parameters, or goes
/// away when none is enabled; with `magpiefx` it is kept. Then every legacy
/// key is removed. Bad values (wrong type, out of the old range) keep the
/// old default and push a warning, as the old validators did.
///
/// Returns whether `map` changed.
///
/// TODO(config): implement. The stub only reports whether there is
/// anything to translate and changes nothing.
pub fn translate_native(map: &mut Mapping, warnings: &mut Vec<ConfigWarning>) -> bool {
    let _ = warnings;
    let _ = has_legacy_keys(map);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_keys_are_not_mode_keys() {
        for key in LEGACY_KEYS {
            // TODO(config): flip to `!contains` once the fields are gone.
            assert!(crate::MODE_KEYS.contains(&key), "{key}");
        }
    }

    #[test]
    fn finds_legacy_keys() {
        let map: Mapping = serde_yaml_ng::from_str("model: fast\ncrt_enabled: null\n").unwrap();
        assert!(has_legacy_keys(&map));
        let map: Mapping = serde_yaml_ng::from_str("model: fast\nmagpie_effects: [CAS]\n").unwrap();
        assert!(!has_legacy_keys(&map));
    }
}
