//! Effect chain and presentation (`presenter.py`). Front P, round 2.
//!
//! The presenter takes the upscaled texture and runs, in one recording:
//! MagpieFX chain or native effects (the `lirtup-effects` passes, in the 1.x
//! order), the scaler into the screen texture (`output_size`, letterbox,
//! zoom and offset), `Delinearize` or `Dither`, then the extra passes the
//! session thread gives it (the OSD and the stats panel of front S, which
//! blend onto [`Presenter::screen`]). Then it presents the screen texture:
//! through the [`FrameGen`] of front F when there is one, else straight to
//! the [`SharedSurface`] of front K.
//!
//! Signatures used by other fronts: [`Presenter::screen`] (S rebinds its
//! blends to it), [`PresentOutcome`] (the session thread), the
//! `framegen`/`surface` arguments of [`Presenter::new`]. The rest is front
//! P's to reshape.
//!
//! **CPU/GPU races of `lirtup-effects`** (`set_params`, `set_frame`,
//! `set_dst_rect`, `OverlayBlend::set_image` write `Upload` buffers): the
//! presenter calls them only after the previous frame's submission has
//! finished (the wait at the top of every frame, as 1.x
//! `wait_for_last_present`), so no per-frame ring is needed. If profiling
//! shows that wait costs a frame of latency, the ring goes in round 4.
//!
//! In this implementation the wait is at the end of [`Presenter::present`]:
//! the chain is submitted, waited for (which also reads its timestamps back),
//! and only then handed to the swapchain or to frame generation. The
//! swapchain copy comes after the chain on the same queue anyway, so the
//! present is not later than in 1.x, and between two presents nothing of the
//! presenter is in flight: the session thread may call any setter (and
//! S may call `set_image`) at any time.
//!
//! # The chain
//!
//! ```text
//! source ─▶ [deband] ─▶ [Anime4K, LumaSharpen, CRT | MagpieFX] ─▶ scaler (RGBA16F)
//!        ─▶ [CAS, bloom, vignette] ─▶ Delinearize | Dither (8 bit) ─copy─▶ screen
//!        ─▶ [LUT, grain] ─▶ OSD, stats panel ─▶ swapchain | FrameGen
//! ```
//!
//! The scaler and the converters bind their output at creation, so the
//! stable `screen` texture (what the OSD and the panel blend onto) gets a
//! copy of the converter's output. Asked for in the branch note: a
//! `Conversion::create_into(output)` removes that copy.

use std::sync::Arc;
use std::time::Instant;

use lirtup_config::{Config, Downsampler, EffectsEngine, Geometry, Upsampler};
use lirtup_effects::convert::Conversion;
use lirtup_effects::effects::NativeEffect;
use lirtup_effects::scalers::{Scaler, ScalerKind, ScalerParams};
use lirtup_effects::{ParamValues, Pass, util};
use lirtup_gpu::{Device, Format, PresentOptions, PresentStatus, SubmitOptions, Texture, Timestamps};
use lirtup_magpiefx::compiler::{HlslCompiler, SpirvCache};
use lirtup_magpiefx::spec::EffectSpec;
use lirtup_magpiefx::{Catalog, CompileOptions, CompiledEffect, MagpieEffect};
use lirtup_proto::Stage;
use lirtup_x11::{CoordinateMapper, Rect, RgbaImage};

use crate::framegen::FrameGen;
use crate::surface::SharedSurface;
use crate::{PipelineError, Result};

/// What a present did.
#[derive(Debug, Clone, PartialEq)]
pub struct PresentOutcome {
    /// GPU time per pass of this frame (`None` when timestamps were not
    /// asked for: stats panel hidden and no GUI listening).
    pub timestamps: Option<Timestamps>,
    /// The swapchain must be recreated (out of date or suboptimal): the
    /// session thread calls [`Presenter::resize`] with the overlay size.
    pub recreate: bool,
}

// -------------------------------------------------------------------------
// Geometry (`utils/geometry.py`)
// -------------------------------------------------------------------------

/// How the content fills its area (`scale_mode` of 1.x).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScaleMode {
    /// Keep the aspect ratio inside the area (letterbox).
    Fit,
    /// Fill the area.
    Stretch,
    /// Keep the aspect ratio, cover the area (crops).
    Cover,
}

/// Overlay size, content size and scale mode of a geometry for a source of
/// `src` on a monitor of `base` (`parse_output_geometry`).
pub fn output_geometry(
    geometry: &Geometry,
    src: (u32, u32),
    base: (u32, u32),
) -> ((u32, u32), (u32, u32), ScaleMode) {
    let (sw, sh) = (f64::from(src.0.max(1)), f64::from(src.1.max(1)));
    let fit = |ow: u32, oh: u32| {
        let s = (f64::from(ow) / sw).min(f64::from(oh) / sh);
        ((sw * s) as u32, (sh * s) as u32)
    };
    let cover = |ow: u32, oh: u32| {
        let s = (f64::from(ow) / sw).max(f64::from(oh) / sh);
        ((sw * s) as u32, (sh * s) as u32)
    };
    let mode = |force: bool| if force { ScaleMode::Stretch } else { ScaleMode::Fit };
    match *geometry {
        Geometry::Stretch => (base, base, ScaleMode::Stretch),
        Geometry::Fit => (base, fit(base.0, base.1), ScaleMode::Fit),
        Geometry::Cover => (base, cover(base.0, base.1), ScaleMode::Cover),
        Geometry::Percent { percent, force } => {
            let p = f64::from(percent) / 100.0;
            let o = ((f64::from(base.0) * p) as u32, (f64::from(base.1) * p) as u32);
            let content = if force { o } else { fit(o.0, o.1) };
            (o, content, mode(force))
        }
        Geometry::Width { width, force } => {
            let o = (width, (sh * f64::from(width) / sw) as u32);
            (o, o, mode(force))
        }
        Geometry::Height { height, force } => {
            let o = ((sw * f64::from(height) / sh) as u32, height);
            (o, o, mode(force))
        }
        Geometry::Size { width, height, flag } => {
            let o = (width, height);
            match flag {
                Some('!') => (o, o, ScaleMode::Stretch),
                Some('^') => (o, cover(width, height), ScaleMode::Cover),
                _ => (o, fit(width, height), ScaleMode::Fit),
            }
        }
    }
}

/// Where a `src` image goes inside a `dst` area (`calculate_scaling_rect`):
/// `(x, y, width, height)`, centered.
pub fn scaling_rect(src: (u32, u32), dst: (u32, u32), mode: ScaleMode) -> (i32, i32, u32, u32) {
    let (sw, sh) = (f64::from(src.0.max(1)), f64::from(src.1.max(1)));
    let (dw, dh) = (f64::from(dst.0), f64::from(dst.1));
    let s = match mode {
        ScaleMode::Stretch => return (0, 0, dst.0, dst.1),
        ScaleMode::Cover => (dw / sw).max(dh / sh),
        ScaleMode::Fit => (dw / sw).min(dh / sh),
    };
    let (w, h) = ((sw * s) as u32, (sh * s) as u32);
    (
        (i64::from(dst.0) - i64::from(w)).div_euclid(2) as i32,
        (i64::from(dst.1) - i64::from(h)).div_euclid(2) as i32,
        w,
        h,
    )
}

/// The content rectangle inside the screen: the area of the geometry
/// (or of the zoom), centered, plus the offset (`_compute_scaling_params`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// Size of the captured (cropped) frame.
    pub capture: (u32, u32),
    /// Monitor size (the base of percentages and of `fit`).
    pub monitor: (u32, u32),
    /// Screen texture size (the overlay).
    pub screen: (u32, u32),
    /// `output_geometry`, parsed.
    pub geometry: Geometry,
    /// Zoom (1.0 = the geometry; else `fit` at that percentage of the
    /// monitor, as 1.x `ZoomIn` switching to `N%`).
    pub zoom: f32,
    /// Offset in screen pixels.
    pub offset: (i32, i32),
}

impl Layout {
    /// Content area and its scale mode.
    pub fn content(&self) -> ((u32, u32), ScaleMode) {
        let geometry = if (self.zoom - 1.0).abs() > f32::EPSILON {
            Geometry::Percent {
                percent: self.zoom * 100.0,
                force: false,
            }
        } else {
            self.geometry
        };
        let (_, content, mode) = output_geometry(&geometry, self.capture, self.monitor);
        (content, mode)
    }

    /// Where an image of `src` is drawn in the screen.
    pub fn dst_rect(&self, src: (u32, u32)) -> (i32, i32, u32, u32) {
        let (content, mode) = self.content();
        let (rx, ry, rw, rh) = scaling_rect(src, content, mode);
        let cx = (i64::from(self.screen.0) - i64::from(content.0)).div_euclid(2) as i32;
        let cy = (i64::from(self.screen.1) - i64::from(content.1)).div_euclid(2) as i32;
        (cx + rx + self.offset.0, cy + ry + self.offset.1, rw, rh)
    }
}

// -------------------------------------------------------------------------
// Presenter
// -------------------------------------------------------------------------

/// The MagpieFX compilers, catalog and cache, created on first use.
struct MagpieContext {
    catalog: Catalog,
    compilers: Vec<Box<dyn HlslCompiler>>,
    cache: SpirvCache,
    fp16: bool,
    /// Compiled effects by `(name, swap input, swap output)`.
    compiled: Vec<((String, bool, bool), CompiledEffect)>,
    swap_rb: Option<CompiledEffect>,
}

impl MagpieContext {
    fn new(device: &Device) -> MagpieContext {
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

    fn compile(&mut self, name: &str, swap_input: bool, swap_output: bool) -> Result<CompiledEffect> {
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

    fn swap_rb(&mut self) -> Result<CompiledEffect> {
        if self.swap_rb.is_none() {
            self.swap_rb = Some(lirtup_magpiefx::runtime::compile_swap_rb(
                &self.compilers,
                &self.cache,
            )?);
        }
        Ok(self.swap_rb.clone().expect("just compiled"))
    }
}

/// What the effects of a config are, to tell what an update changes.
#[derive(Debug, Clone, PartialEq)]
struct EffectPlan {
    /// Before scaling, on the upscaled frame: deband, then the native
    /// effects (MagpieFX is in `magpie`).
    pre: Vec<NativeEffect>,
    /// The MagpieFX chain (empty with the native engine).
    magpie: Vec<EffectSpec>,
    /// After scaling, on linear light: CAS, bloom, vignette.
    linear: Vec<NativeEffect>,
    /// On the 8-bit screen: LUT, grain (1.x order).
    screen: Vec<NativeEffect>,
}

impl EffectPlan {
    fn from_config(config: &Config) -> EffectPlan {
        let mut plan = EffectPlan {
            pre: Vec::new(),
            magpie: Vec::new(),
            linear: Vec::new(),
            screen: Vec::new(),
        };
        let mut lut = None;
        for effect in NativeEffect::from_mode(&config.scaling) {
            match effect {
                NativeEffect::Deband { .. }
                | NativeEffect::Anime4kS
                | NativeEffect::LumaSharpen { .. }
                | NativeEffect::CrtEasymode(_) => plan.pre.push(effect),
                NativeEffect::Cas { .. } | NativeEffect::Bloom { .. } | NativeEffect::Vignette { .. } => {
                    plan.linear.push(effect)
                }
                NativeEffect::Lut { .. } => lut = Some(effect),
                NativeEffect::Grain { .. } => plan.screen.push(effect),
            }
        }
        if let Some(lut) = lut {
            plan.screen.insert(0, lut);
        }
        if config.scaling.effects_engine == EffectsEngine::Magpiefx {
            plan.magpie = match lirtup_magpiefx::spec::parse_effect_chain(&config.scaling.magpie_effects) {
                Ok(specs) => specs,
                Err(e) => {
                    tracing::error!("invalid MagpieFX configuration: {e}");
                    Vec::new()
                }
            };
        }
        plan
    }
}

/// The parameters of a native effect as `Pass::set_params` names (`None`
/// when a change needs a new pass: CRT, a LUT preset, Anime4K).
fn native_params(effect: &NativeEffect) -> Option<ParamValues> {
    let p = |pairs: &[(&str, f64)]| Some(pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect());
    match *effect {
        NativeEffect::Cas { strength } => p(&[("cas_strength", f64::from(strength))]),
        NativeEffect::Deband { strength } => p(&[("deband_strength", f64::from(strength))]),
        NativeEffect::Bloom {
            strength,
            threshold,
            radius,
        } => p(&[
            ("bloom_strength", f64::from(strength)),
            ("bloom_threshold", f64::from(threshold)),
            ("bloom_radius", f64::from(radius)),
        ]),
        NativeEffect::Vignette {
            strength,
            radius,
            falloff,
        } => p(&[
            ("vignette_strength", f64::from(strength)),
            ("vignette_radius", f64::from(radius)),
            ("vignette_falloff", f64::from(falloff)),
        ]),
        NativeEffect::Grain { strength, size } => p(&[
            ("grain_strength", f64::from(strength)),
            ("grain_size", f64::from(size)),
        ]),
        NativeEffect::Lut { intensity, .. } => p(&[("lut_intensity", f64::from(intensity))]),
        NativeEffect::LumaSharpen {
            strength,
            clamp,
            pattern,
            offset_bias,
        } => p(&[
            ("luma_sharpen_strength", f64::from(strength)),
            ("luma_sharpen_clamp", f64::from(clamp)),
            ("luma_sharpen_pattern", f64::from(pattern)),
            ("luma_sharpen_offset_bias", f64::from(offset_bias)),
        ]),
        NativeEffect::Anime4kS | NativeEffect::CrtEasymode(_) => None,
    }
}

/// Whether `new` can be applied to passes built for `old` with
/// `set_params` alone; then applies it.
fn update_native(passes: &mut [Box<dyn Pass>], old: &[NativeEffect], new: &[NativeEffect]) -> Result<bool> {
    if old.len() != new.len() {
        return Ok(false);
    }
    for (a, b) in old.iter().zip(new) {
        let same_kind = std::mem::discriminant(a) == std::mem::discriminant(b);
        let lut_preset = |e: &NativeEffect| match e {
            NativeEffect::Lut { preset, .. } => Some(*preset),
            _ => None,
        };
        if !same_kind || lut_preset(a) != lut_preset(b) || (a != b && native_params(b).is_none()) {
            return Ok(false);
        }
    }
    for ((pass, a), b) in passes.iter_mut().zip(old).zip(new) {
        if a != b
            && let Some(params) = native_params(b)
        {
            pass.set_params(&params)?;
        }
    }
    Ok(true)
}

fn scaler_params(config: &Config) -> ScalerParams {
    let s = &config.scaling;
    ScalerParams {
        blur: s.blur,
        antiring_strength: s.antiring_strength,
        tight_antiring: s.tight_antiring,
        kernel_radius: s.kernel_radius,
    }
}

/// The scaler for drawing `src` into a `rect` (`present` of 1.x): copy at
/// 1:1, the upsampler when either side grows, else the downsampler.
fn scaler_kind(config: &Config, src: (u32, u32), rect: (u32, u32)) -> ScalerKind {
    if rect == src {
        ScalerKind::Copy
    } else if rect.0 >= src.0 || rect.1 >= src.1 {
        match config.scaling.upsampler {
            Upsampler::Lanczos => ScalerKind::Lanczos,
            Upsampler::Fsr => ScalerKind::Fsr,
            Upsampler::Nis => ScalerKind::Nis,
        }
    } else {
        match config.scaling.downsampler {
            Downsampler::Catmull => ScalerKind::CatmullRom,
            Downsampler::Lanczos => ScalerKind::Lanczos,
        }
    }
}

/// The letterbox color: the frames are BGRA bytes, so the shader gets
/// `[b, g, r, a]` (1.x `color_string_to_float4`).
fn background(config: &Config) -> [f32; 4] {
    let [r, g, b, a] = config.background_color.to_rgba();
    [b, g, r, a]
}

/// Everything bound to one scaler: the scaler, the linear effects on its
/// output and the converter.
struct ScaleStage {
    kind: ScalerKind,
    /// Size of the texture it reads.
    input: (u32, u32),
    rect: (i32, i32, u32, u32),
    scaler: Scaler,
    linear: Vec<Box<dyn Pass>>,
    convert: Box<dyn Pass>,
}

/// The effect chain and the screen texture of one session.
pub struct Presenter {
    device: Arc<Device>,
    config: Config,
    plan: EffectPlan,
    layout: Layout,
    surface: SharedSurface,
    framegen: Option<FrameGen>,
    magpie: Option<MagpieContext>,
    /// The MagpieFX effects actually in the chain (failed ones dropped).
    magpie_active: Vec<EffectSpec>,
    /// The upscaled frame.
    source: Option<Texture>,
    /// Passes before scaling (deband, native or MagpieFX effects).
    pre: Vec<Box<dyn Pass>>,
    /// Rebuild `pre` (and everything after it) before the next present.
    pre_stale: bool,
    scale: Option<ScaleStage>,
    /// The final 8-bit texture (swapchain size).
    screen: Texture,
    /// LUT and grain, in place on `screen`.
    screen_passes: Vec<Box<dyn Pass>>,
    frame: u64,
    started: Instant,
}

fn poisoned() -> PipelineError {
    PipelineError::other(Stage::Swapchain, "the swapchain lock is poisoned")
}

impl Presenter {
    /// Build the chain for `config` (compiling MagpieFX effects, blocking)
    /// for a captured frame of `src_size` (after crop) shown on a monitor of
    /// `monitor` (the base of `fit` and percentages), presenting to
    /// `surface` (or through `framegen`). The screen texture takes the
    /// surface's size.
    pub fn new(
        device: &Arc<Device>,
        config: &Config,
        src_size: (u32, u32),
        monitor: (u32, u32),
        surface: SharedSurface,
        framegen: Option<FrameGen>,
    ) -> Result<Presenter> {
        let screen_size = surface.lock().map_err(|_| poisoned())?.size();
        let geometry = config.output_geometry.parse().unwrap_or_else(|e| {
            tracing::warn!("{e}; using fit");
            Geometry::Fit
        });
        let screen = new_screen(device, screen_size)?;
        let plan = EffectPlan::from_config(config);
        let mut presenter = Presenter {
            device: device.clone(),
            config: config.clone(),
            layout: Layout {
                capture: src_size,
                monitor,
                screen: screen_size,
                geometry,
                zoom: 1.0,
                offset: (0, 0),
            },
            surface,
            framegen,
            magpie: None,
            magpie_active: Vec::new(),
            source: None,
            pre: Vec::new(),
            pre_stale: true,
            scale: None,
            screen,
            screen_passes: Vec::new(),
            frame: 0,
            started: Instant::now(),
            plan,
        };
        presenter.build_screen_passes()?;
        if !presenter.plan.magpie.is_empty() {
            // Compile now (blocking), so the first frame does not stall.
            let mut ctx = MagpieContext::new(device);
            for (i, spec) in presenter.plan.magpie.iter().enumerate() {
                let last = presenter.plan.magpie.len() - 1;
                if let Err(e) = ctx.compile(&spec.name, i == 0, i == last) {
                    tracing::error!("MagpieFX effect '{}' disabled: {e}", spec.name);
                }
            }
            presenter.magpie = Some(ctx);
        }
        Ok(presenter)
    }

    /// The final 8-bit screen texture (swapchain size). The OSD and the
    /// stats panel blend onto it; it changes on [`Presenter::resize`].
    pub fn screen(&self) -> &Texture {
        &self.screen
    }

    /// The upscaled frame to present next. A different texture than the
    /// last one (a new upscaler) rebuilds the chain on the next present.
    pub fn set_source(&mut self, texture: &Texture) {
        let same = self.source.as_ref().is_some_and(|t| t.raw() == texture.raw());
        if !same {
            self.source = Some(texture.clone());
            self.pre_stale = true;
        }
    }

    /// The captured size changed (new window size, menu bar crop).
    pub fn set_capture_size(&mut self, size: (u32, u32)) {
        if self.layout.capture != size {
            self.layout.capture = size;
            self.pre_stale = true;
        }
    }

    /// The monitor changed.
    pub fn set_monitor(&mut self, size: (u32, u32)) {
        if self.layout.monitor != size {
            self.layout.monitor = size;
            self.pre_stale = true;
        }
    }

    /// Run the chain, then the `overlays` (in order), and present. With
    /// frame generation, the [`FrameGen`] presents (and paces) instead.
    /// `timestamps`: time every pass on the GPU.
    pub fn present(&mut self, overlays: &[&dyn Pass], timestamps: bool) -> Result<PresentOutcome> {
        self.prepare()?;
        let frame = self.frame;
        let time = self.started.elapsed().as_secs_f32();
        for pass in self.pre.iter_mut().chain(self.screen_passes.iter_mut()) {
            pass.set_frame(frame, time);
        }

        let mut recorder = self.device.recorder()?;
        for pass in &self.pre {
            pass.record(&mut recorder)?;
        }
        let stage = self.scale.as_ref().expect("prepare builds the scaler");
        stage.scaler.record(&mut recorder)?;
        for pass in &stage.linear {
            pass.record(&mut recorder)?;
        }
        stage.convert.record(&mut recorder)?;
        recorder.copy_texture(stage.convert.output(), &self.screen, &[]);
        for pass in &self.screen_passes {
            pass.record(&mut recorder)?;
        }
        for pass in overlays {
            pass.record(&mut recorder)?;
        }
        recorder.prepare_present(&self.screen);
        let submission = self.device.submit(
            recorder,
            &SubmitOptions {
                timestamps,
                ..SubmitOptions::default()
            },
        )?;
        // The wait of the decision in the module docs: nothing in flight
        // after this, so every setter is safe until the next present.
        let times = submission.wait(self.config.frame_timeout)?;
        self.frame += 1;

        let recreate = match self.framegen.as_mut() {
            Some(fg) => {
                fg.present(&self.screen)?;
                self.surface.lock().map_err(|_| poisoned())?.needs_recreation()
            }
            None => self.present_screen()?,
        };
        Ok(PresentOutcome {
            timestamps: times,
            recreate,
        })
    }

    /// Present the last screen texture again (`present_on_change_only:
    /// false` and nothing changed). Not called while frame generation is
    /// active.
    pub fn present_unchanged(&mut self) -> Result<PresentOutcome> {
        let recreate = self.present_screen()?;
        Ok(PresentOutcome {
            timestamps: None,
            recreate,
        })
    }

    fn present_screen(&mut self) -> Result<bool> {
        let mut surface = self.surface.lock().map_err(|_| poisoned())?;
        let status = surface
            .present(&self.screen, &PresentOptions::default())
            .map_err(|e| PipelineError::other(Stage::Swapchain, e.to_string()))?;
        Ok(status != PresentStatus::Ok || surface.needs_recreation())
    }

    /// The swapchain (overlay) has a new size: recreate it, the screen
    /// texture and the scaler output. The caller then rebinds the OSD and
    /// the panel to [`Presenter::screen`].
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        let (width, height) = (width.max(1), height.max(1));
        self.surface
            .lock()
            .map_err(|_| poisoned())?
            .recreate(width, height)
            .map_err(|e| PipelineError::other(Stage::Swapchain, e.to_string()))?;
        if self.layout.screen != (width, height) {
            self.layout.screen = (width, height);
            self.screen = new_screen(&self.device, (width, height))?;
            self.scale = None;
            self.build_screen_passes()?;
            if let Some(fg) = self.framegen.as_mut() {
                fg.resize((width, height));
            }
        }
        Ok(())
    }

    /// New effect parameters, chain or scaler (`ReloadConfig`, a geometry
    /// change; `CycleModel` does not come here: that is the upscaler).
    /// Rebuilds only what changed.
    pub fn update_config(&mut self, config: &Config) -> Result<()> {
        let plan = EffectPlan::from_config(config);
        let old_config = std::mem::replace(&mut self.config, config.clone());
        let old = std::mem::replace(&mut self.plan, plan);

        match config.output_geometry.parse() {
            Ok(geometry) if geometry != self.layout.geometry => {
                self.layout.geometry = geometry;
                // CRT and MagpieFX render at the content size.
                self.pre_stale = true;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("{e}; keeping the geometry"),
        }

        // Before scaling.
        if (old.pre != self.plan.pre || old.magpie != self.plan.magpie) && !self.pre_stale {
            let updated = if old.magpie.is_empty() && self.plan.magpie.is_empty() {
                // Native only: one pass per effect, in plan order.
                self.pre.len() == old.pre.len() && update_native(&mut self.pre, &old.pre, &self.plan.pre)?
            } else {
                self.update_magpie_params(&old)?
            };
            if !updated {
                self.pre_stale = true;
            }
        }

        // Scaler and linear effects.
        if let Some(stage) = self.scale.as_mut() {
            let params_changed = scaler_params(&old_config) != scaler_params(config)
                || old_config.scaling.upsampler != config.scaling.upsampler
                || old_config.scaling.downsampler != config.scaling.downsampler;
            if params_changed || !update_native(&mut stage.linear, &old.linear, &self.plan.linear)? {
                self.scale = None;
            } else if old_config.background_color != config.background_color {
                stage.scaler.set_background(background(config));
            }
        }

        // On the screen.
        if !update_native(&mut self.screen_passes, &old.screen, &self.plan.screen)? {
            self.build_screen_passes()?;
        }

        if let Some(fg) = self.framegen.as_mut() {
            fg.update_config(config);
        }
        Ok(())
    }

    /// Same MagpieFX effects with new parameter values: `set_params`, no
    /// recompile (the GUI sliders). `false` when the chain must be rebuilt.
    fn update_magpie_params(&mut self, old: &EffectPlan) -> Result<bool> {
        let names = |specs: &[EffectSpec]| specs.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        if old.pre != self.plan.pre
            || self.plan.magpie.is_empty()
            || names(&old.magpie) != names(&self.plan.magpie)
            || self.magpie_active != old.magpie
            || self.pre_stale
        {
            return Ok(false);
        }
        // `pre` is: [deband], [SwapRB], effects..., [SwapRB].
        let effects: Vec<&mut Box<dyn Pass>> = self
            .pre
            .iter_mut()
            .filter(|p| p.label() != "deband" && p.label() != "SwapRB")
            .collect();
        if effects.len() != self.plan.magpie.len() {
            return Ok(false);
        }
        for (pass, spec) in effects.into_iter().zip(&self.plan.magpie) {
            pass.set_params(&spec.params)?;
        }
        self.magpie_active = self.plan.magpie.clone();
        Ok(true)
    }

    /// Zoom (1.0 = the configured geometry) and offset in output pixels
    /// (`ZoomIn`, `OffsetLeft`, `RestoreView`, ...). The offset is the whole
    /// content offset: the session adds `offset_x`/`offset_y` outside
    /// windowed mode.
    pub fn set_view(&mut self, zoom: f32, offset: (i32, i32)) {
        let zoom = if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 };
        if (zoom - self.layout.zoom).abs() > f32::EPSILON {
            self.layout.zoom = zoom;
            // CRT and MagpieFX render at the content size.
            if self
                .plan
                .pre
                .iter()
                .any(|e| matches!(e, NativeEffect::CrtEasymode(_)))
                || !self.plan.magpie.is_empty()
            {
                self.pre_stale = true;
            }
        }
        self.layout.offset = offset;
    }

    /// Where the content is drawn in the overlay, for the input mapping
    /// (`OverlayCommand::SetMapper`). `crop_offset` and `source_size` are
    /// filled by the session thread.
    pub fn mapper(&self) -> CoordinateMapper {
        let src = self
            .pre
            .last()
            .map(|p| p.output())
            .or(self.source.as_ref())
            .map(|t| (t.width(), t.height()))
            .unwrap_or(self.layout.capture);
        let (x, y, width, height) = self.layout.dst_rect(src);
        CoordinateMapper {
            content: Rect { x, y, width, height },
            source_size: (0, 0),
            crop_offset: (0, 0),
        }
    }

    /// Labels of the chain, in order (`SessionView.chain`, the stats panel).
    /// The SRCNN passes are the upscaler's; the session puts them first.
    pub fn chain(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if self.pre_stale {
            out.extend(
                self.plan
                    .pre
                    .iter()
                    .filter(|e| matches!(e, NativeEffect::Deband { .. }))
                    .map(|e| e.label().to_owned()),
            );
            out.extend(self.plan.magpie.iter().map(|s| s.name.clone()));
            out.extend(
                self.plan
                    .pre
                    .iter()
                    .filter(|e| !matches!(e, NativeEffect::Deband { .. }))
                    .map(|e| e.label().to_owned()),
            );
        } else {
            out.extend(
                self.pre
                    .iter()
                    .filter(|p| p.label() != "SwapRB")
                    .map(|p| p.label().to_owned()),
            );
        }
        match &self.scale {
            Some(stage) => {
                out.push(stage.scaler.label().to_owned());
                out.extend(stage.linear.iter().map(|p| p.label().to_owned()));
            }
            None => out.extend(self.plan.linear.iter().map(|e| e.label().to_owned())),
        }
        out.extend(self.screen_passes.iter().map(|p| p.label().to_owned()));
        out
    }

    /// The frame generation engine, if any (status, multiplier).
    pub fn framegen(&self) -> Option<&FrameGen> {
        self.framegen.as_ref()
    }

    /// Mutable access, for `update_config` and `set_refresh_hz`.
    pub fn framegen_mut(&mut self) -> Option<&mut FrameGen> {
        self.framegen.as_mut()
    }

    /// Read back the screen texture (`Screenshot`): RGBA8, swapchain size.
    pub fn screenshot(&self) -> Result<RgbaImage> {
        let mut data = self.screen.download()?;
        // The frames are BGRA bytes.
        for px in data.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
            px[3] = 255;
        }
        Ok(RgbaImage {
            width: self.screen.width(),
            height: self.screen.height(),
            data,
        })
    }

    /// Release everything (closes the [`FrameGen`] first).
    pub fn close(mut self) {
        if let Some(fg) = self.framegen.take() {
            fg.close();
        }
        if let Err(e) = self.device.wait_idle() {
            tracing::warn!("presenter close: {e}");
        }
    }

    // ---------------------------------------------------------------------
    // Building the chain
    // ---------------------------------------------------------------------

    /// Bring the chain up to date with the source, the config and the view.
    fn prepare(&mut self) -> Result<()> {
        let Some(source) = self.source.clone() else {
            return Err(PipelineError::other(
                Stage::Runtime,
                "present without a source texture",
            ));
        };
        if self.pre_stale {
            self.build_pre(&source)?;
            self.pre_stale = false;
            self.scale = None;
        }
        let input = self.pre.last().map(|p| p.output().clone()).unwrap_or(source);
        let size = (input.width(), input.height());
        let rect = self.layout.dst_rect(size);
        let kind = scaler_kind(&self.config, size, (rect.2, rect.3));
        match self.scale.as_mut() {
            Some(stage) if stage.kind == kind && stage.input == size => {
                if stage.rect != rect {
                    stage.scaler.set_dst_rect(rect);
                    stage.rect = rect;
                }
            }
            _ => self.scale = Some(self.build_scale(&input, kind, rect)?),
        }
        Ok(())
    }

    /// Deband, then the native effects or the MagpieFX chain.
    fn build_pre(&mut self, source: &Texture) -> Result<()> {
        self.pre.clear();
        let content_rect = |layout: &Layout, src: (u32, u32)| {
            let (content, mode) = layout.content();
            let (_, _, w, h) = scaling_rect(src, content, mode);
            (w.max(1), h.max(1))
        };
        let mut input = source.clone();
        let plan = self.plan.clone();
        for effect in plan
            .pre
            .iter()
            .filter(|e| matches!(e, NativeEffect::Deband { .. }))
        {
            let pass = effect.create(&self.device, &input, (input.width(), input.height()))?;
            input = pass.output().clone();
            self.pre.push(pass);
        }
        if !plan.magpie.is_empty() {
            let out_size = content_rect(&self.layout, (input.width(), input.height()));
            let (passes, active) = self.build_magpie(&input, &plan.magpie, out_size);
            self.pre.extend(passes);
            self.magpie_active = active;
        }
        for effect in plan
            .pre
            .iter()
            .filter(|e| !matches!(e, NativeEffect::Deband { .. }))
        {
            let size = (input.width(), input.height());
            let pass = effect.create(&self.device, &input, content_rect(&self.layout, size))?;
            input = pass.output().clone();
            self.pre.push(pass);
        }
        Ok(())
    }

    /// The MagpieFX chain on `input`: the first effect reads BGRA and the
    /// last writes it (folded into the shaders, or a `SwapRB` pass where the
    /// effect does not allow it). An effect that fails is dropped and the
    /// chain is rebuilt without it, so the swaps move to the new ends
    /// (`_build_magpie_chain`).
    fn build_magpie(
        &mut self,
        input: &Texture,
        specs: &[EffectSpec],
        out_size: (u32, u32),
    ) -> (Vec<Box<dyn Pass>>, Vec<EffectSpec>) {
        let ctx = self
            .magpie
            .get_or_insert_with(|| MagpieContext::new(&self.device));
        let mut active = specs.to_vec();
        while !active.is_empty() {
            match bind_magpie(&self.device, ctx, input, &active, out_size) {
                Ok(passes) => return (passes, active),
                Err((i, e)) => {
                    tracing::error!("MagpieFX effect '{}' disabled: {e}", active[i].name);
                    active.remove(i);
                }
            }
        }
        (Vec::new(), active)
    }

    fn build_scale(
        &self,
        input: &Texture,
        kind: ScalerKind,
        rect: (i32, i32, u32, u32),
    ) -> Result<ScaleStage> {
        let mut scaler = Scaler::new(
            &self.device,
            kind,
            input,
            self.layout.screen,
            rect,
            &scaler_params(&self.config),
        )?;
        scaler.set_background(background(&self.config));
        let mut linear = Vec::with_capacity(self.plan.linear.len());
        for effect in &self.plan.linear {
            linear.push(effect.create(&self.device, scaler.output(), self.layout.screen)?);
        }
        let conversion = if scaler.linear_output() {
            Conversion::Delinearize
        } else {
            Conversion::Dither
        };
        let convert = conversion.create(&self.device, scaler.output(), Format::R8G8B8A8Unorm)?;
        Ok(ScaleStage {
            kind,
            input: (input.width(), input.height()),
            rect,
            scaler,
            linear,
            convert,
        })
    }

    fn build_screen_passes(&mut self) -> Result<()> {
        self.screen_passes.clear();
        for effect in &self.plan.screen {
            self.screen_passes
                .push(effect.create(&self.device, &self.screen, self.layout.screen)?);
        }
        Ok(())
    }
}

/// Bind every effect of `specs` in order; on failure, the index of the
/// effect that failed.
fn bind_magpie(
    device: &Arc<Device>,
    ctx: &mut MagpieContext,
    input: &Texture,
    specs: &[EffectSpec],
    out_size: (u32, u32),
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
        let fx = MagpieEffect::new(device, &compiled, &current, out_size, &spec.params)
            .map_err(|e| (i, e.into()))?;
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

fn new_screen(device: &Arc<Device>, size: (u32, u32)) -> Result<Texture> {
    Ok(util::texture(
        device,
        size.0.max(1),
        size.1.max(1),
        Format::R8G8B8A8Unorm,
        "screen",
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_rect_matches_1x() {
        assert_eq!(
            scaling_rect((640, 480), (1920, 1080), ScaleMode::Fit),
            (240, 0, 1440, 1080)
        );
        assert_eq!(
            scaling_rect((640, 480), (1920, 1080), ScaleMode::Stretch),
            (0, 0, 1920, 1080)
        );
        assert_eq!(
            scaling_rect((640, 480), (1920, 1080), ScaleMode::Cover),
            (0, -180, 1920, 1440)
        );
    }

    #[test]
    fn output_geometry_matches_parse_output_geometry() {
        let base = (1920, 1080);
        let src = (800, 600);
        assert_eq!(
            output_geometry(&Geometry::Fit, src, base),
            (base, (1440, 1080), ScaleMode::Fit)
        );
        assert_eq!(
            output_geometry(&Geometry::Stretch, src, base),
            (base, base, ScaleMode::Stretch)
        );
        assert_eq!(
            output_geometry(
                &Geometry::Percent {
                    percent: 50.0,
                    force: false
                },
                src,
                base
            ),
            ((960, 540), (720, 540), ScaleMode::Fit)
        );
        assert_eq!(
            output_geometry(
                &Geometry::Percent {
                    percent: 50.0,
                    force: true
                },
                src,
                base
            ),
            ((960, 540), (960, 540), ScaleMode::Stretch)
        );
        assert_eq!(
            output_geometry(
                &Geometry::Width {
                    width: 1600,
                    force: false
                },
                src,
                base
            ),
            ((1600, 1200), (1600, 1200), ScaleMode::Fit)
        );
        assert_eq!(
            output_geometry(
                &Geometry::Height {
                    height: 300,
                    force: true
                },
                src,
                base
            ),
            ((400, 300), (400, 300), ScaleMode::Stretch)
        );
        assert_eq!(
            output_geometry(
                &Geometry::Size {
                    width: 1000,
                    height: 1000,
                    flag: None
                },
                src,
                base
            ),
            ((1000, 1000), (1000, 750), ScaleMode::Fit)
        );
        assert_eq!(
            output_geometry(
                &Geometry::Size {
                    width: 1000,
                    height: 1000,
                    flag: Some('^')
                },
                src,
                base
            ),
            ((1000, 1000), (1333, 1000), ScaleMode::Cover)
        );
        assert_eq!(
            output_geometry(
                &Geometry::Size {
                    width: 1000,
                    height: 1000,
                    flag: Some('!')
                },
                src,
                base
            ),
            ((1000, 1000), (1000, 1000), ScaleMode::Stretch)
        );
    }

    #[test]
    fn layout_centers_the_content_and_adds_the_offset() {
        let mut layout = Layout {
            capture: (800, 600),
            monitor: (1920, 1080),
            screen: (1920, 1080),
            geometry: Geometry::Fit,
            zoom: 1.0,
            offset: (0, 0),
        };
        // Upscaled 2x: same aspect ratio, same rect.
        assert_eq!(layout.dst_rect((1600, 1200)), (240, 0, 1440, 1080));
        layout.offset = (25, -25);
        assert_eq!(layout.dst_rect((1600, 1200)), (265, -25, 1440, 1080));
        layout.offset = (0, 0);
        layout.zoom = 0.5;
        assert_eq!(layout.dst_rect((1600, 1200)), (600, 270, 720, 540));
    }

    #[test]
    fn scaler_kind_picks_copy_up_or_down() {
        let config = Config::default();
        assert_eq!(scaler_kind(&config, (100, 100), (100, 100)), ScalerKind::Copy);
        assert_eq!(scaler_kind(&config, (100, 100), (200, 50)), ScalerKind::Lanczos);
        assert_eq!(scaler_kind(&config, (100, 100), (50, 50)), ScalerKind::CatmullRom);
    }

    #[test]
    fn plan_orders_the_effects_like_1x() {
        let mut config = Config::default();
        let s = &mut config.scaling;
        s.deband_enabled = true;
        s.cas_enabled = true;
        s.grain_enabled = true;
        s.lut_enabled = true;
        s.anime4k_enabled = true;
        s.effects_engine = EffectsEngine::Native;
        let plan = EffectPlan::from_config(&config);
        let labels = |v: &[NativeEffect]| v.iter().map(|e| e.label()).collect::<Vec<_>>();
        assert_eq!(labels(&plan.pre), ["deband", "anime4k"]);
        assert_eq!(labels(&plan.linear), ["cas"]);
        assert_eq!(labels(&plan.screen), ["lut", "grain"]);
        assert!(plan.magpie.is_empty());
    }

    #[test]
    fn native_params_cover_live_changes_only() {
        assert!(native_params(&NativeEffect::Anime4kS).is_none());
        let p = native_params(&NativeEffect::Cas { strength: 0.25 }).unwrap();
        assert_eq!(p.get("cas_strength"), Some(&0.25));
        let mut e = NativeEffect::Bloom {
            strength: 0.1,
            threshold: 0.2,
            radius: 3,
        };
        for (k, v) in native_params(&NativeEffect::Bloom {
            strength: 0.5,
            threshold: 0.6,
            radius: 7,
        })
        .unwrap()
        {
            assert!(e.set_param(&k, v));
        }
        assert_eq!(
            e,
            NativeEffect::Bloom {
                strength: 0.5,
                threshold: 0.6,
                radius: 7
            }
        );
    }

    #[test]
    fn background_is_bgra() {
        let config = Config::default();
        let [b, g, r, a] = background(&config);
        assert_eq!([r, g, b, a], config.background_color.to_rgba());
    }
}
