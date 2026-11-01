//! `magpie_effects:` items (`spec.py`).
//!
//! An item is a string `"Name"` or `"Name:param=value,param=value"` (CLI
//! form), or `{name: Name, params: {param: value}}` (YAML form). `Name` is a
//! built-in or user effect name, or a path to a `.hlsl` file.

use std::collections::BTreeMap;

use lirtup_config::MagpieEffectSpec;

use crate::{MagpieError, Result};

/// One effect of the chain with its parameter overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectSpec {
    /// Name or `.hlsl` path.
    pub name: String,
    /// Overrides.
    pub params: BTreeMap<String, f64>,
}

fn error(effect: &str, message: String) -> MagpieError {
    MagpieError::Parse { effect: effect.to_owned(), message }
}

/// Parse one item (`"Name"`, `"Name:a=1,b=2"`, or the long form).
pub fn parse_effect_spec(item: &MagpieEffectSpec) -> Result<EffectSpec> {
    match item {
        MagpieEffectSpec::Full { name, params } => {
            if name.trim().is_empty() {
                return Err(error("", "MagpieFX effect entry needs a 'name'".to_owned()));
            }
            Ok(EffectSpec { name: name.clone(), params: params.clone() })
        }
        MagpieEffectSpec::Short(text) => parse_short(text),
    }
}

/// The CLI form. A path may contain ':' only in unusual names, so split on
/// the last ':' followed by `key=` pairs.
fn parse_short(text: &str) -> Result<EffectSpec> {
    let text = text.trim();
    if text.is_empty() {
        return Err(error("", "Invalid MagpieFX effect entry: empty".to_owned()));
    }
    let Some((name, rest)) = text.rsplit_once(':').filter(|(_, rest)| rest.contains('=')) else {
        return Ok(EffectSpec { name: text.to_owned(), params: BTreeMap::new() });
    };
    let mut params = BTreeMap::new();
    for pair in rest.split(',') {
        let Some((key, value)) = pair.split_once('=').filter(|(k, _)| !k.trim().is_empty()) else {
            return Err(error(name, format!("Invalid parameter '{pair}' in effect '{name}'")));
        };
        let key = key.trim();
        let value: f64 = value
            .trim()
            .parse()
            .map_err(|_| error(name, format!("Parameter '{key}' of effect '{name}' must be a number")))?;
        params.insert(key.to_owned(), value);
    }
    Ok(EffectSpec { name: name.to_owned(), params })
}

/// Parse the whole chain.
pub fn parse_effect_chain(items: &[MagpieEffectSpec]) -> Result<Vec<EffectSpec>> {
    items.iter().map(parse_effect_spec).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short(s: &str) -> Result<EffectSpec> {
        parse_effect_spec(&MagpieEffectSpec::Short(s.to_owned()))
    }

    #[test]
    fn short_forms() {
        assert_eq!(short(" LumaSharpen ").unwrap(), EffectSpec { name: "LumaSharpen".into(), params: BTreeMap::new() });
        let s = short("CRT/CRT_Easymode:sharpnessH=0.6, maskStrength=0.2").unwrap();
        assert_eq!(s.name, "CRT/CRT_Easymode");
        assert_eq!(s.params, BTreeMap::from([("sharpnessH".into(), 0.6), ("maskStrength".into(), 0.2)]));
        // a ':' without pairs is part of the name
        assert_eq!(short("C:/fx/a.hlsl").unwrap().name, "C:/fx/a.hlsl");
        assert_eq!(short("/a:b/fx.hlsl:x=1").unwrap().name, "/a:b/fx.hlsl");
    }

    #[test]
    fn short_errors() {
        assert!(short("").is_err());
        assert!(short("Fx:a=1,=2").is_err());
        assert!(short("Fx:a=1,b").is_err());
        assert!(short("Fx:a=x").is_err());
    }

    #[test]
    fn long_form() {
        let item = MagpieEffectSpec::Full { name: "Fx".into(), params: BTreeMap::from([("a".into(), 1.0)]) };
        assert_eq!(parse_effect_spec(&item).unwrap().params["a"], 1.0);
        let empty = MagpieEffectSpec::Full { name: " ".into(), params: BTreeMap::new() };
        assert!(parse_effect_spec(&empty).is_err());
        assert_eq!(parse_effect_chain(&[item.clone(), MagpieEffectSpec::Short("B".into())]).unwrap().len(), 2);
    }
}
