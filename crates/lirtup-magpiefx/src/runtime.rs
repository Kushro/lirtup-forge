//! Running an effect on the GPU (`effect.py`: `MagpieEffect`,
//! `compile_effect`).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use lirtup_effects::util::{self, CbWriter};
use lirtup_gpu::{AddressMode, Buffer, CommandRecorder, ComputePipeline, Device, Dispatch, Filter, Format, Texture};

use crate::compiler::{CompileRequest, HlslCompiler, SpirvCache, Transform};
use crate::dds::{self, DdsImage};
use crate::parser::{EffectPass, ParamKind};
use crate::{CatalogEntry, EffectDesc, MagpieError, Result, codegen, expr, formats, swizzle};

/// An effect parsed and compiled, not yet bound to textures. Cheap to keep
/// between sessions (the GUI's `CompileEffect` fills a cache of these in
/// `lirtup serve`).
#[derive(Clone)]
pub struct CompiledEffect {
    /// The description.
    pub desc: Arc<EffectDesc>,
    /// SPIR-V per pass.
    pub spirv: Arc<Vec<Vec<u8>>>,
    /// Whether the FP16 variant was compiled.
    pub fp16: bool,
    /// `//!SOURCE` textures by texture index, read at compile time.
    sources: Arc<BTreeMap<usize, DdsImage>>,
    /// The requested INPUT swap is folded into the shaders.
    input_swapped: bool,
    /// The requested OUTPUT swap is folded into the shaders.
    output_swapped: bool,
}

impl CompiledEffect {
    /// Whether reads of INPUT swap red and blue (asked for with
    /// [`CompileOptions::swap_input`] and possible for this effect).
    pub fn input_swapped(&self) -> bool {
        self.input_swapped
    }

    /// Whether stores to OUTPUT swap red and blue.
    pub fn output_swapped(&self) -> bool {
        self.output_swapped
    }

    /// The `//!SOURCE` lookup textures, by texture index.
    pub fn sources(&self) -> &BTreeMap<usize, DdsImage> {
        &self.sources
    }
}

/// How to compile (see [`compile_effect_with`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompileOptions {
    /// Use 16-bit `MF` types if the effect declares `//!CAPABILITY FP16`
    /// (falls back to FP32 when that does not compile).
    pub fp16: bool,
    /// Read INPUT as BGRA (the first effect of a chain over captured frames).
    pub swap_input: bool,
    /// Store OUTPUT as BGRA (the last effect of the chain).
    pub swap_output: bool,
}

/// Parse and compile every pass of an entry (blocking: run it with
/// `spawn_blocking` or on the pipeline thread before the first frame).
pub fn compile_effect(
    entry: &CatalogEntry,
    compilers: &[Box<dyn HlslCompiler>],
    cache: &SpirvCache,
    fp16: bool,
) -> Result<CompiledEffect> {
    compile_effect_with(entry, compilers, cache, &CompileOptions { fp16, ..CompileOptions::default() })
}

/// [`compile_effect`] with the red/blue swap folded into the shaders where
/// the effect allows it (check [`CompiledEffect::input_swapped`] and
/// [`CompiledEffect::output_swapped`]; a side that could not be folded needs
/// a [`compile_swap_rb`] pass).
pub fn compile_effect_with(
    entry: &CatalogEntry,
    compilers: &[Box<dyn HlslCompiler>],
    cache: &SpirvCache,
    options: &CompileOptions,
) -> Result<CompiledEffect> {
    let source = crate::catalog::read_source(&entry.path)?;
    compile_source(&entry.stem(), &source, entry.path.parent(), compilers, cache, options)
}

/// The separate swap-R/B effect (`internal/SwapRB.hlsl`), for the sides a
/// chain could not fold.
pub fn compile_swap_rb(compilers: &[Box<dyn HlslCompiler>], cache: &SpirvCache) -> Result<CompiledEffect> {
    compile_source("SwapRB", swizzle::swap_rb_source(), None, compilers, cache, &CompileOptions::default())
}

/// Parse `source` and compile its passes; `include_dir` is the effect's
/// folder (`#include` and `//!SOURCE`).
pub fn compile_source(
    name: &str,
    source: &str,
    include_dir: Option<&Path>,
    compilers: &[Box<dyn HlslCompiler>],
    cache: &SpirvCache,
    options: &CompileOptions,
) -> Result<CompiledEffect> {
    let desc = crate::parser::parse_effect(source, name)?;
    for w in &desc.warnings {
        tracing::warn!("MagpieFX {name}: {w}");
    }
    let sources = load_sources(&desc, include_dir)?;

    let mut fp16 = options.fp16 && desc.supports_fp16;
    let compiled = if fp16 {
        match compile_passes(&desc, include_dir, compilers, cache, options, true) {
            Ok(result) => Ok(result),
            Err(e) => {
                tracing::warn!("MagpieFX {name}: FP16 compilation failed, using 32-bit floats:\n{e}");
                fp16 = false;
                compile_passes(&desc, include_dir, compilers, cache, options, false)
            }
        }
    } else {
        compile_passes(&desc, include_dir, compilers, cache, options, false)
    };
    let (spirv, input_swapped, output_swapped) = compiled?;
    if fp16 {
        tracing::info!("MagpieFX {name}: using FP16");
    }
    Ok(CompiledEffect {
        desc: Arc::new(desc),
        spirv: Arc::new(spirv),
        fp16,
        sources: Arc::new(sources),
        input_swapped,
        output_swapped,
    })
}

/// Read the `//!SOURCE` files of `desc`. A texture without `//!FORMAT` takes
/// the file's format; one with it must match, as in Magpie.
fn load_sources(desc: &EffectDesc, effect_dir: Option<&Path>) -> Result<BTreeMap<usize, DdsImage>> {
    let mut sources = BTreeMap::new();
    for (i, tex) in desc.textures.iter().enumerate().filter(|(_, t)| !t.source.is_empty()) {
        let err = |message: String| MagpieError::Parse { effect: desc.name.clone(), message };
        let dir = effect_dir.ok_or_else(|| err(format!("lookup texture '{}' needs the effect's folder", tex.source)))?;
        let path = dds::resolve_source_path(dir, &tex.source)?;
        if !path.is_file() {
            return Err(err(format!("lookup texture '{}' of '{}' not found", tex.source, tex.name)));
        }
        let image = dds::load_image(&path)?;
        let format = dds::source_format(&tex.format, &image)
            .map_err(|e| err(format!("texture '{}': {e}", tex.name)))?;
        if formats::lookup(&format).is_none() {
            return Err(err(format!("unsupported lookup texture format {format}")));
        }
        sources.insert(i, image);
    }
    Ok(sources)
}

/// Every pass; returns `(spirv, input_swapped, output_swapped)`. Tries the
/// requested swaps, dropping a side whose rewrite is unsupported.
fn compile_passes(
    desc: &EffectDesc,
    include_dir: Option<&Path>,
    compilers: &[Box<dyn HlslCompiler>],
    cache: &SpirvCache,
    options: &CompileOptions,
    fp16: bool,
) -> Result<(Vec<Vec<u8>>, bool, bool)> {
    let (si, so) = (options.swap_input, options.swap_output);
    let mut attempts = vec![(si, so)];
    if si && so {
        attempts.extend([(true, false), (false, true)]);
    }
    if si || so {
        attempts.push((false, false));
    }
    'attempts: for (sin, sout) in attempts {
        let mut spirv = Vec::with_capacity(desc.passes.len());
        for (i, p) in desc.passes.iter().enumerate() {
            match compile_pass(desc, i, p, include_dir, compilers, cache, sin, sout, fp16)? {
                Some(code) => spirv.push(code),
                None => {
                    tracing::debug!("MagpieFX {}: swap rewrite limited (pass {})", desc.name, p.index);
                    continue 'attempts;
                }
            }
        }
        return Ok((spirv, sin, sout));
    }
    unreachable!("the last attempt has no swap and cannot be declined")
}

/// One pass; `None` when the swap rewrite does not apply.
#[allow(clippy::too_many_arguments)]
fn compile_pass(
    desc: &EffectDesc,
    index: usize,
    p: &EffectPass,
    include_dir: Option<&Path>,
    compilers: &[Box<dyn HlslCompiler>],
    cache: &SpirvCache,
    swap_input: bool,
    swap_output: bool,
    fp16: bool,
) -> Result<Option<Vec<u8>>> {
    let source = codegen::generate_pass_source(desc, index, fp16);
    let request = CompileRequest { effect: &desc.name, pass: p.index, source: &source, entry: "__M", include_dir, fp16 };
    let sin = swap_input && p.inputs.contains(&0);
    let sout = swap_output && p.outputs.contains(&1);
    if !sin && !sout {
        return cache.compile(compilers, &request).map(Some);
    }
    let rewrite = move |text: &str| swizzle::rewrite(text, sin, sout).ok().map(|r| r.0);
    let key = format!("swaprb:{}{}", sin as u8, sout as u8);
    cache.compile_with(compilers, &request, Some(&Transform { key: &key, apply: &rewrite }))
}

/// Whether FP16 variants should be used on this device (`fp16_wanted`).
///
/// `UPSCALER_MAGPIEFX_FP16` decides: `0`/`off`/`false`/`no` always uses
/// 32-bit floats; anything else (`auto`, the default) uses 16-bit floats when
/// the device has `shaderFloat16`.
pub fn fp16_wanted(device: &Device) -> bool {
    fp16_policy(std::env::var("UPSCALER_MAGPIEFX_FP16").ok().as_deref(), device.info().supports_fp16)
}

fn fp16_policy(env: Option<&str>, supported: bool) -> bool {
    let mode = env.unwrap_or("auto").trim().to_lowercase();
    if matches!(mode.as_str(), "0" | "off" | "false" | "no") {
        return false;
    }
    if !supported && matches!(mode.as_str(), "1" | "on" | "true" | "yes") {
        tracing::info!("MagpieFX: the GPU has no FP16 support, using 32-bit floats");
    }
    supported
}

/// A compiled effect bound to an input, with its intermediate textures,
/// lookup textures and samplers. Implements `lirtup_effects::Pass`.
pub struct MagpieEffect {
    desc: Arc<EffectDesc>,
    /// `[INPUT, OUTPUT, ...]`, as `desc.textures`.
    textures: Vec<Texture>,
    pipelines: Vec<(ComputePipeline, [u32; 3])>,
    cb: Buffer,
    params: BTreeMap<String, f64>,
    frame: u64,
}

fn size_vars(input: (u32, u32), output: Option<(u32, u32)>) -> BTreeMap<String, f64> {
    let mut vars = BTreeMap::from([
        ("INPUT_WIDTH".to_owned(), input.0 as f64),
        ("INPUT_HEIGHT".to_owned(), input.1 as f64),
    ]);
    if let Some((w, h)) = output {
        vars.insert("OUTPUT_WIDTH".to_owned(), w as f64);
        vars.insert("OUTPUT_HEIGHT".to_owned(), h as f64);
    }
    vars
}

fn named(desc: &EffectDesc, e: MagpieError) -> MagpieError {
    match e {
        MagpieError::Parse { effect, message } if effect.is_empty() => MagpieError::Parse { effect: desc.name.clone(), message },
        other => other,
    }
}

impl MagpieEffect {
    /// Bind. `output_size` is the size the chain wants after this effect
    /// (`OUTPUT_WIDTH`/`OUTPUT_HEIGHT` in the expressions).
    pub fn new(
        device: &Arc<Device>,
        effect: &CompiledEffect,
        input: &Texture,
        output_size: (u32, u32),
        params: &lirtup_effects::ParamValues,
    ) -> Result<MagpieEffect> {
        let desc = effect.desc.clone();
        if effect.spirv.len() != desc.passes.len() {
            return Err(MagpieError::Parse { effect: desc.name.clone(), message: "SPIR-V count does not match the passes".to_owned() });
        }
        let in_size = (input.width(), input.height());
        let out = MagpieEffect::output_size(effect, in_size, output_size)?;
        let vars = size_vars(in_size, Some(out));

        let mut textures = vec![
            input.clone(),
            util::texture(device, out.0, out.1, Format::R8G8B8A8Unorm, &format!("{} OUTPUT", desc.name))
                .map_err(MagpieError::from)?,
        ];
        for (i, tex) in desc.textures.iter().enumerate().skip(2) {
            let label = format!("{} {}", desc.name, tex.name);
            if !tex.source.is_empty() {
                let image = effect.sources.get(&i).ok_or_else(|| MagpieError::Parse {
                    effect: desc.name.clone(),
                    message: format!("texture '{}' ('{}') was not loaded", tex.name, tex.source),
                })?;
                textures.push(upload_lookup(device, image, &label)?);
                continue;
            }
            let (w, h) = if tex.width_expr.is_empty() {
                out
            } else {
                (
                    expr::evaluate_size(&tex.width_expr, &vars).map_err(|e| named(&desc, e))?,
                    expr::evaluate_size(&tex.height_expr, &vars).map_err(|e| named(&desc, e))?,
                )
            };
            let format = formats::lookup(&tex.format)
                .ok_or_else(|| MagpieError::Parse { effect: desc.name.clone(), message: format!("unknown format '{}'", tex.format) })?
                .format;
            textures.push(util::texture(device, w, h, format, &label)?);
        }

        let samplers = desc
            .samplers
            .iter()
            .map(|s| {
                let filter = if s.filter == "LINEAR" { Filter::Linear } else { Filter::Point };
                let address = if s.address == "WRAP" { AddressMode::Wrap } else { AddressMode::Clamp };
                util::sampler_with(device, filter, address)
            })
            .collect::<lirtup_effects::Result<Vec<_>>>()?;
        let sampler_refs: Vec<_> = samplers.iter().collect();

        let mut fx = MagpieEffect {
            cb: util::constant_buffer(device, codegen::constant_buffer_layout(&desc).len() * 8, &format!("{} cb", desc.name))?,
            desc: desc.clone(),
            textures,
            pipelines: Vec::new(),
            params: BTreeMap::new(),
            frame: 0,
        };
        for (p, spirv) in desc.passes.iter().zip(effect.spirv.iter()) {
            let srv: Vec<&Texture> = p.inputs.iter().map(|i| &fx.textures[*i as usize]).collect();
            let uav: Vec<&Texture> = p.outputs.iter().map(|i| &fx.textures[*i as usize]).collect();
            let entry = util::spirv_entry_point(spirv).unwrap_or_else(|| "__M".to_owned());
            let pipeline = util::compute(
                device,
                spirv,
                &entry,
                &util::Bindings { cb: Some(&fx.cb), srv: &srv, uav: &uav, samplers: &sampler_refs, push_size: 0 },
                &format!("{} pass {}", desc.name, p.index),
            )?;
            let first = &fx.textures[p.outputs[0] as usize];
            let groups = util::groups((first.width(), first.height()), p.block_size);
            fx.pipelines.push((pipeline, groups));
        }
        fx.apply_params(params);
        fx.upload_constants()?;
        tracing::debug!(
            "MagpieFX {}: {}x{} -> {}x{}, {} passes",
            desc.name,
            in_size.0,
            in_size.1,
            out.0,
            out.1,
            fx.pipelines.len()
        );
        Ok(fx)
    }

    /// Output size the effect's expressions give for an input size (to plan
    /// the chain before binding). Effects that do not fix OUTPUT take
    /// `output` (the size the content is shown at, like Magpie's
    /// scaling-mode default).
    pub fn output_size(effect: &CompiledEffect, input: (u32, u32), output: (u32, u32)) -> Result<(u32, u32)> {
        let out = effect.desc.output();
        if out.width_expr.is_empty() {
            return Ok(output);
        }
        let vars = size_vars(input, None);
        let named = |e| named(&effect.desc, e);
        Ok((
            expr::evaluate_size(&out.width_expr, &vars).map_err(named)?,
            expr::evaluate_size(&out.height_expr, &vars).map_err(named)?,
        ))
    }

    /// The description.
    pub fn desc(&self) -> &EffectDesc {
        &self.desc
    }

    /// Current value of a parameter (its default when not overridden).
    pub fn param_value(&self, name: &str) -> f64 {
        self.params.get(name).copied().or_else(|| self.desc.param(name).map(|p| p.default)).unwrap_or(0.0)
    }

    /// Store overrides: unknown names are ignored and values are clamped to
    /// `[min, max]` (1.x raised; the `Pass` contract asks to ignore), both
    /// with a warning.
    fn apply_params(&mut self, params: &lirtup_effects::ParamValues) {
        for (name, value) in params {
            let Some(p) = self.desc.param(name) else {
                tracing::warn!("MagpieFX {}: unknown parameter '{name}'", self.desc.name);
                continue;
            };
            let clamped = value.clamp(p.min, p.max);
            if clamped != *value {
                tracing::warn!("MagpieFX {}: {name}={value} outside [{}, {}]", self.desc.name, p.min, p.max);
            }
            self.params.insert(name.clone(), clamped);
        }
    }

    /// The constant buffer (`_pack_constants`), in [`codegen::constant_buffer_layout`]
    /// order.
    fn pack_constants(&self) -> Vec<u8> {
        let (in_w, in_h) = (self.textures[0].width(), self.textures[0].height());
        let (out_w, out_h) = (self.textures[1].width(), self.textures[1].height());
        let mut cb = CbWriter::new().u32(in_w).u32(in_h).u32(out_w).u32(out_h).f32s(&[
            1.0 / in_w as f32,
            1.0 / in_h as f32,
            1.0 / out_w as f32,
            1.0 / out_h as f32,
            out_w as f32 / in_w as f32,
            out_h as f32 / in_h as f32,
        ]);
        if let Some((_, rest)) = self.desc.passes.split_last() {
            for p in rest.iter().filter(|p| p.ps_style) {
                let t = &self.textures[p.outputs[0] as usize];
                cb = cb.u32(t.width()).u32(t.height()).f32(1.0 / t.width() as f32).f32(1.0 / t.height() as f32);
            }
        }
        for param in &self.desc.params {
            let value = self.param_value(&param.name);
            cb = match param.kind {
                ParamKind::Int => cb.i32(value.round() as i32),
                ParamKind::Float => cb.f32(value as f32),
            };
        }
        if self.desc.use_dynamic {
            cb = cb.u32(self.frame as u32);
        }
        cb.finish()
    }

    fn upload_constants(&self) -> Result<()> {
        self.cb.write(0, &self.pack_constants()).map_err(lirtup_effects::EffectError::from)?;
        Ok(())
    }
}

/// Create a texture with a lookup image's format and contents. The texels
/// are copied as they are (1.x expanded them to floats and wrote them with a
/// compute pass because its upload only handled 4-byte texels).
fn upload_lookup(device: &Arc<Device>, image: &DdsImage, label: &str) -> Result<Texture> {
    let format = formats::lookup(&image.format)
        .ok_or_else(|| MagpieError::Texture { path: label.to_owned(), message: format!("unsupported format {}", image.format) })?
        .format;
    let tex = util::texture(device, image.width, image.height, format, label)?;
    util::upload_texture(device, &tex, &image.data)?;
    Ok(tex)
}

impl lirtup_effects::Pass for MagpieEffect {
    fn label(&self) -> &str {
        &self.desc.name
    }

    fn output(&self) -> &Texture {
        &self.textures[1]
    }

    fn record(&self, recorder: &mut CommandRecorder) -> lirtup_effects::Result<()> {
        for (pipeline, groups) in &self.pipelines {
            recorder.dispatch(&Dispatch { pipeline, groups: *groups, push: &[], timestamp: Some(&self.desc.name) });
        }
        Ok(())
    }

    fn set_params(&mut self, params: &lirtup_effects::ParamValues) -> lirtup_effects::Result<()> {
        self.apply_params(params);
        self.cb.write(0, &self.pack_constants())?;
        Ok(())
    }

    fn set_frame(&mut self, frame: u64, _time_s: f32) {
        self.frame = frame;
        if self.desc.use_dynamic
            && let Err(e) = self.cb.write(0, &self.pack_constants())
        {
            tracing::warn!("MagpieFX {}: cannot update FrameCount: {e}", self.desc.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::tests::Fake;

    const SOURCE: &str = "//!MAGPIE EFFECT\n//!VERSION 4\n{capability}\n\n//!TEXTURE\nTexture2D INPUT;\n\n//!PASS 1\n//!STYLE PS\n//!IN INPUT\n//!OUT OUTPUT\n\nMF4 Pass1(float2 pos) {\n\treturn (MF4)INPUT.Load(int3(pos * GetInputSize(), 0));\n}\n";

    fn source(fp16: bool) -> String {
        SOURCE.replace("{capability}", if fp16 { "//!CAPABILITY FP16" } else { "" })
    }

    /// Compile `source` with a fake compiler; `fail_fp16` makes FP16 passes
    /// fail. Returns the effect and the `(pass, fp16)` calls.
    fn compile(source: &str, fp16: bool, fail_fp16: bool) -> (CompiledEffect, Vec<(u32, bool)>) {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::new("dxc", move |r| {
            if r.fp16 && fail_fp16 {
                Err(Fake::failure(r, "half precision error"))
            } else {
                Ok(if r.fp16 { b"SPIRV16".to_vec() } else { b"SPIRV32".to_vec() })
            }
        }));
        struct Shared(Arc<Fake>);
        impl HlslCompiler for Shared {
            fn name(&self) -> &str {
                self.0.name()
            }
            fn version(&self) -> Option<String> {
                self.0.version()
            }
            fn compile(&self, r: &CompileRequest<'_>) -> Result<Vec<u8>> {
                self.0.compile(r)
            }
        }
        let compilers: Vec<Box<dyn HlslCompiler>> = vec![Box::new(Shared(fake.clone()))];
        let cache = SpirvCache::new(dir.path().to_path_buf());
        let options = CompileOptions { fp16, ..CompileOptions::default() };
        let effect = compile_source("Test", source, None, &compilers, &cache, &options).unwrap();
        let calls = fake.calls.lock().unwrap().clone();
        (effect, calls)
    }

    #[test]
    fn fp16_is_used_when_requested_and_declared() {
        let (fx, calls) = compile(&source(true), true, false);
        assert!(fx.fp16);
        assert_eq!(*fx.spirv, vec![b"SPIRV16".to_vec()]);
        assert_eq!(calls, [(1, true)]);
    }

    #[test]
    fn fp16_needs_the_capability() {
        let (fx, calls) = compile(&source(false), true, false);
        assert!(!fx.fp16);
        assert_eq!(*fx.spirv, vec![b"SPIRV32".to_vec()]);
        assert_eq!(calls, [(1, false)]);
    }

    #[test]
    fn fp16_not_requested() {
        let (fx, calls) = compile(&source(true), false, false);
        assert!(!fx.fp16);
        assert_eq!(calls, [(1, false)]);
    }

    #[test]
    fn fp16_falls_back_to_fp32_when_it_does_not_compile() {
        let (fx, calls) = compile(&source(true), true, true);
        assert!(!fx.fp16);
        assert_eq!(*fx.spirv, vec![b"SPIRV32".to_vec()]);
        assert_eq!(calls, [(1, true), (1, false)]);
    }

    #[test]
    fn fp16_policy_follows_the_device_and_the_switch() {
        assert!(fp16_policy(None, true));
        assert!(!fp16_policy(None, false));
        assert!(fp16_policy(Some("auto"), true));
        for off in ["0", "off", "false", "no", " OFF "] {
            assert!(!fp16_policy(Some(off), true), "{off}");
        }
        // forcing it still needs the device
        assert!(!fp16_policy(Some("1"), false));
        assert!(fp16_policy(Some("on"), true));
    }

    #[test]
    fn swap_is_folded_or_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpirvCache::new(dir.path().to_path_buf());
        let compilers: Vec<Box<dyn HlslCompiler>> =
            vec![Box::new(Fake::new("dxc", |r| Ok(r.source.as_bytes().to_vec())))];
        let both = CompileOptions { swap_input: true, swap_output: true, ..CompileOptions::default() };

        let fx = compile_source("Test", &source(false), None, &compilers, &cache, &both).unwrap();
        assert!(fx.input_swapped() && fx.output_swapped());
        let code = String::from_utf8(fx.spirv[0].clone()).unwrap();
        assert!(code.contains("__MP_SwapRB(INPUT.Load("));
        assert!(code.contains("OUTPUT[gxy] = __MP_SwapRB(Pass1(pos));"));

        // INPUT passed to a function: only OUTPUT is folded
        let fxaa = source(false).replace("return (MF4)INPUT.Load(int3(pos * GetInputSize(), 0));", "return F(INPUT, pos);");
        let fx = compile_source("Fxaa", &fxaa, None, &compilers, &cache, &both).unwrap();
        assert!(!fx.input_swapped() && fx.output_swapped());
    }

    #[test]
    fn swap_rb_effect_compiles() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpirvCache::new(dir.path().to_path_buf());
        let compilers: Vec<Box<dyn HlslCompiler>> = vec![Box::new(Fake::new("dxc", |_| Ok(vec![1, 2, 3, 4])))];
        let fx = compile_swap_rb(&compilers, &cache).unwrap();
        assert_eq!(fx.desc.name, "SwapRB");
        assert_eq!(fx.spirv.len(), 1);
    }

    #[test]
    fn output_size_uses_the_expressions() {
        let (fx, _) = compile(&source(false), false, false);
        // no OUTPUT size: the chain decides
        assert_eq!(MagpieEffect::output_size(&fx, (100, 50), (300, 200)).unwrap(), (300, 200));

        let doubled = source(false).replace(
            "//!TEXTURE\nTexture2D INPUT;",
            "//!TEXTURE\nTexture2D INPUT;\n//!TEXTURE\n//!WIDTH INPUT_WIDTH * 2\n//!HEIGHT INPUT_HEIGHT * 2\nTexture2D OUTPUT;",
        );
        let (fx, _) = compile(&doubled, false, false);
        assert_eq!(MagpieEffect::output_size(&fx, (100, 50), (300, 200)).unwrap(), (200, 100));
    }

    #[test]
    fn lookup_textures_are_read_at_compile_time() {
        let dir = tempfile::tempdir().unwrap();
        let fx_dir = dir.path().join("fx");
        std::fs::create_dir_all(&fx_dir).unwrap();
        let png = fx_dir.join("lut.png");
        image::RgbaImage::from_pixel(4, 2, image::Rgba([1, 2, 3, 4])).save(&png).unwrap();
        let src = source(false)
            .replace("//!TEXTURE\nTexture2D INPUT;", "//!TEXTURE\nTexture2D INPUT;\n//!TEXTURE\n//!SOURCE lut.png\nTexture2D lut;")
            .replace("//!IN INPUT", "//!IN INPUT, lut");
        let path = fx_dir.join("Lut.hlsl");
        std::fs::write(&path, &src).unwrap();
        let cache = SpirvCache::new(dir.path().join("cache"));
        let compilers: Vec<Box<dyn HlslCompiler>> = vec![Box::new(Fake::new("dxc", |_| Ok(vec![0; 4])))];
        let entry = CatalogEntry::from_path(&path);
        let fx = compile_effect(&entry, &compilers, &cache, false).unwrap();
        assert_eq!(fx.desc.name, "Lut");
        let img = &fx.sources()[&2];
        assert_eq!((img.width, img.height, img.format.as_str()), (4, 2, "R8G8B8A8_UNORM"));

        // a declared format that does not match the file
        let bad = src.replace("//!SOURCE lut.png", "//!SOURCE lut.png\n//!FORMAT R16_FLOAT");
        std::fs::write(&path, bad).unwrap();
        assert!(compile_effect(&entry, &compilers, &cache, false).is_err());
        // a missing file
        std::fs::write(&path, src.replace("lut.png", "nope.png")).unwrap();
        assert!(compile_effect(&entry, &compilers, &cache, false).is_err());
    }

    #[test]
    fn bundled_effects_generate_every_pass() {
        // codegen over the whole catalog, in both precisions (no compiler)
        let catalog = crate::Catalog::scan(None);
        for entry in &catalog.entries {
            let desc = catalog.describe(entry).unwrap();
            for i in 0..desc.passes.len() {
                for fp16 in [false, true] {
                    let src = codegen::generate_pass_source(&desc, i, fp16);
                    assert!(src.contains("void __M("), "{} pass {}", entry.name, i + 1);
                }
            }
        }
    }
}
