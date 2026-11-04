//! The MagpieFX plumbing shared by the two chains (fx-unify): the
//! presenter's, after the model (`magpie_effects`, on the upscaled frame),
//! and the upscaler's, before the model (`magpie_effects_before_model`, on
//! the captured frame).
//!
//! Both chains read and write BGRA bytes in R8G8B8A8 textures, as the rest
//! of the pipeline does: the first effect swaps red and blue on read and the
//! last one on write (folded into the shaders, or a separate `SwapRB` pass
//! where the effect does not allow it), so everything in between is RGBA as
//! MagpieFX effects expect.

use std::sync::Arc;
use std::time::Instant;

use lirtup_config::MagpieEffectSpec;
use lirtup_effects::{ParamValues, Pass};
use lirtup_gpu::{Device, Texture};
use lirtup_magpiefx::compiler::{HlslCompiler, SpirvCache};
use lirtup_magpiefx::spec::EffectSpec;
use lirtup_magpiefx::{Catalog, CompileOptions, CompiledEffect, MagpieEffect};

use crate::{PipelineError, Result};

/// The label of the separate red/blue swap pass.
pub(crate) const SWAP_RB_LABEL: &str = "SwapRB";

/// The MagpieFX compilers, catalog and cache, created on first use.
pub(crate) struct MagpieContext {
    catalog: Catalog,
    compilers: Vec<Box<dyn HlslCompiler>>,
    cache: SpirvCache,
    fp16: bool,
    /// Compiled effects by `(name, swap input, swap output)`.
    compiled: Vec<((String, bool, bool), CompiledEffect)>,
    swap_rb: Option<CompiledEffect>,
}

impl MagpieContext {
    pub(crate) fn new(device: &Device) -> MagpieContext {
        let user = lirtup_config::paths::user_effects_dir();
        MagpieContext {
            catalog: Catalog::scan(Some(&user)),
            compilers: lirtup_magpiefx::compiler::detect_compilers(),
            cache: SpirvCache::new(lirtup_config::paths::magpiefx_cache_dir()),
            fp16: lirtup_magpiefx::runtime::fp16_wanted(device),
            compiled: Vec::new(),
            swap_rb: None,
        }
    }

    /// Compile (or take from the cache) one effect for its place in a chain:
    /// `swap_input` for the first effect, `swap_output` for the last.
    pub(crate) fn compile(
        &mut self,
        name: &str,
        swap_input: bool,
        swap_output: bool,
    ) -> Result<CompiledEffect> {
        let key = (name.to_owned(), swap_input, swap_output);
        if let Some((_, c)) = self.compiled.iter().find(|(k, _)| *k == key) {
            return Ok(c.clone());
        }
        let started = Instant::now();
        let entry = self.catalog.resolve(name)?;
        let compiled = lirtup_magpiefx::runtime::compile_effect_with(
            &entry,
            &self.compilers,
            &self.cache,
            &CompileOptions {
                fp16: self.fp16,
                swap_input,
                swap_output,
            },
        )?;
        tracing::info!(
            "MagpieFX effect '{}' loaded ({} passes, {} ms)",
            compiled.desc.name,
            compiled.desc.passes.len(),
            started.elapsed().as_millis()
        );
        self.compiled.push((key, compiled.clone()));
        Ok(compiled)
    }

    /// The separate swap pass, for a side an effect could not fold.
    pub(crate) fn swap_rb(&mut self) -> Result<CompiledEffect> {
        if self.swap_rb.is_none() {
            self.swap_rb = Some(lirtup_magpiefx::runtime::compile_swap_rb(
                &self.compilers,
                &self.cache,
            )?);
        }
        Ok(self.swap_rb.clone().expect("just compiled"))
    }

    /// Compile every effect of a chain now (blocking), for its place in the
    /// chain, so the first frame does not stall. The ones that fail are
    /// pushed to `disabled` (`(name, error)`) and logged; binding drops
    /// them later.
    pub(crate) fn precompile(&mut self, specs: &[EffectSpec], disabled: &mut Vec<(String, String)>) {
        let Some(last) = specs.len().checked_sub(1) else {
            return;
        };
        for (i, spec) in specs.iter().enumerate() {
            if let Err(e) = self.compile(&spec.name, i == 0, i == last) {
                tracing::error!("MagpieFX effect '{}' disabled: {e}", spec.name);
                disabled.push((spec.name.clone(), e.to_string()));
            }
        }
    }
}

/// The parsed chain of a config list; an invalid entry empties it (logged),
/// as the presenter always did.
pub(crate) fn parse_chain(items: &[MagpieEffectSpec]) -> Vec<EffectSpec> {
    match lirtup_magpiefx::spec::parse_effect_chain(items) {
        Ok(specs) => specs,
        Err(e) => {
            tracing::error!("invalid MagpieFX configuration: {e}");
            Vec::new()
        }
    }
}

/// Whether two chains hold the same effects in the same order (parameters
/// aside): then new values go live through [`set_chain_params`] instead of a
/// rebuild.
pub(crate) fn same_names(a: &[EffectSpec], b: &[EffectSpec]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.name == y.name)
}

/// The effect names of a config list, in order; `None` when an entry is
/// invalid (the chain is then empty, [`parse_chain`]). Quiet: for comparing
/// two configurations without logging the error again.
pub(crate) fn chain_names(items: &[MagpieEffectSpec]) -> Option<Vec<String>> {
    lirtup_magpiefx::spec::parse_effect_chain(items)
        .ok()
        .map(|specs| specs.into_iter().map(|s| s.name).collect())
}

/// The size each effect of a chain renders at when it does not fix its
/// OUTPUT size itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutSize {
    /// This size for every effect (the presenter: the content size).
    Target((u32, u32)),
    /// The size of the effect's own input (the upscaler's chain before the
    /// model: what the model takes is what the capture gave, unless an
    /// effect fixes another size).
    Input,
}

impl OutSize {
    fn for_input(self, input: &Texture) -> (u32, u32) {
        match self {
            OutSize::Target(size) => size,
            OutSize::Input => (input.width(), input.height()),
        }
    }
}

/// The MagpieFX chain on `input`: the first effect reads BGRA and the
/// last writes it (folded into the shaders, or a `SwapRB` pass where the
/// effect does not allow it). An effect that fails is dropped (and pushed to
/// `disabled`) and the chain is rebuilt without it, so the swaps move to the
/// new ends (`_build_magpie_chain`). Returns the passes and the effects
/// actually bound.
pub(crate) fn build_magpie_chain(
    device: &Arc<Device>,
    magpie: &mut Option<MagpieContext>,
    input: &Texture,
    specs: &[EffectSpec],
    out_size: OutSize,
    disabled: &mut Vec<(String, String)>,
) -> (Vec<Box<dyn Pass>>, Vec<EffectSpec>) {
    let ctx = magpie.get_or_insert_with(|| MagpieContext::new(device));
    let mut active = specs.to_vec();
    while !active.is_empty() {
        match bind_magpie(device, ctx, input, &active, out_size) {
            Ok(passes) => return (passes, active),
            Err((i, e)) => {
                tracing::error!("MagpieFX effect '{}' disabled: {e}", active[i].name);
                let spec = active.remove(i);
                disabled.push((spec.name, e.to_string()));
            }
        }
    }
    (Vec::new(), active)
}

/// Bind every effect of `specs` in order; on failure, the index of the
/// effect that failed.
pub(crate) fn bind_magpie(
    device: &Arc<Device>,
    ctx: &mut MagpieContext,
    input: &Texture,
    specs: &[EffectSpec],
    out_size: OutSize,
) -> std::result::Result<Vec<Box<dyn Pass>>, (usize, PipelineError)> {
    let last = specs.len() - 1;
    let mut passes: Vec<Box<dyn Pass>> = Vec::new();
    let mut current = input.clone();
    let swap = |ctx: &mut MagpieContext, current: &Texture| -> Result<Box<dyn Pass>> {
        let compiled = ctx.swap_rb()?;
        let size = (current.width(), current.height());
        Ok(Box::new(MagpieEffect::new(
            device,
            &compiled,
            current,
            size,
            &ParamValues::new(),
        )?))
    };
    for (i, spec) in specs.iter().enumerate() {
        let compiled = ctx.compile(&spec.name, i == 0, i == last).map_err(|e| (i, e))?;
        if i == 0 && !compiled.input_swapped() {
            let pass = swap(ctx, &current).map_err(|e| (i, e))?;
            current = pass.output().clone();
            passes.push(pass);
        }
        let size = out_size.for_input(&current);
        let fx =
            MagpieEffect::new(device, &compiled, &current, size, &spec.params).map_err(|e| (i, e.into()))?;
        current = fx.output().clone();
        passes.push(Box::new(fx));
        if i == last && !compiled.output_swapped() {
            let pass = swap(ctx, &current).map_err(|e| (i, e))?;
            current = pass.output().clone();
            passes.push(pass);
        }
    }
    Ok(passes)
}

/// New parameter values for a bound chain (`passes` holds that chain only:
/// its effects and its `SwapRB` passes) without recompiling. `false` when
/// the passes do not match `specs` (another chain: rebuild it).
pub(crate) fn set_chain_params(passes: &mut [Box<dyn Pass>], specs: &[EffectSpec]) -> Result<bool> {
    let mut effects: Vec<&mut Box<dyn Pass>> =
        passes.iter_mut().filter(|p| p.label() != SWAP_RB_LABEL).collect();
    if effects.len() != specs.len() {
        return Ok(false);
    }
    for (pass, spec) in effects.iter_mut().zip(specs) {
        pass.set_params(&spec.params)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invalid_entry_empties_the_chain() {
        let ok = [
            MagpieEffectSpec::Short("CAS".into()),
            MagpieEffectSpec::Short("LumaSharpen:sharpStrength=0.5".into()),
        ];
        let specs = parse_chain(&ok);
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[1].params.get("sharpStrength"), Some(&0.5));
        let bad = [
            MagpieEffectSpec::Short("CAS".into()),
            MagpieEffectSpec::Short("Lanczos:x=big".into()),
        ];
        assert!(parse_chain(&bad).is_empty());
    }

    #[test]
    fn same_names_ignores_the_parameters() {
        let a = parse_chain(&[
            MagpieEffectSpec::Short("CAS".into()),
            MagpieEffectSpec::Short("LumaSharpen:sharpStrength=0.5".into()),
        ]);
        let b = parse_chain(&[
            MagpieEffectSpec::Short("CAS".into()),
            MagpieEffectSpec::Short("LumaSharpen:sharpStrength=1.5".into()),
        ]);
        assert!(same_names(&a, &b));
        assert!(!same_names(&a, &b[..1]), "one effect less");
        let swapped: Vec<EffectSpec> = b.iter().rev().cloned().collect();
        assert!(!same_names(&a, &swapped), "another order");
        assert!(same_names(&[], &[]));
    }

    #[test]
    fn chain_names_are_quiet_about_invalid_entries() {
        assert_eq!(
            chain_names(&[
                MagpieEffectSpec::Short("CAS".into()),
                MagpieEffectSpec::Short("LumaSharpen:sharpStrength=0.5".into()),
            ]),
            Some(vec!["CAS".to_owned(), "LumaSharpen".to_owned()])
        );
        assert_eq!(chain_names(&[]), Some(Vec::new()));
        assert_eq!(
            chain_names(&[MagpieEffectSpec::Short("Lanczos:x=big".into())]),
            None
        );
    }
}
