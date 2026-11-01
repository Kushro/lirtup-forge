//! `magpie_effects:` items (`spec.py`).

use std::collections::BTreeMap;

use lirtup_config::MagpieEffectSpec;

use crate::Result;

/// One effect of the chain with its parameter overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectSpec {
    /// Name or `.hlsl` path.
    pub name: String,
    /// Overrides.
    pub params: BTreeMap<String, f64>,
}

/// Parse one item (`"Name"`, `"Name:a=1,b=2"`, or the long form).
pub fn parse_effect_spec(item: &MagpieEffectSpec) -> Result<EffectSpec> {
    todo!("front E")
}

/// Parse the whole chain.
pub fn parse_effect_chain(items: &[MagpieEffectSpec]) -> Result<Vec<EffectSpec>> {
    items.iter().map(parse_effect_spec).collect()
}
