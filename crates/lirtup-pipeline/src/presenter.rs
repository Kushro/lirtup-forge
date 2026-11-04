//! Effect chain and presentation (`presenter.py`). Front P, round 2.
//!
//! The presenter takes the upscaled texture and runs, in one recording:
//! deband, the MagpieFX chain after the model (`magpie_effects`), the scaler
//! into the screen texture (`output_size`, letterbox, zoom and offset), the
//! native linear effects (CAS, bloom, vignette), `Delinearize` or `Dither`,
//! the native screen effects (LUT, grain), then the extra passes the
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
//! The order of a frame (fx-unify). The chain before the model
//! (`magpie_effects_before_model`) runs in the upscaler, on the captured
//! frame; the presenter's `source` is the upscaler's output (the model's,
//! or that chain's with `model: none`):
//!
//! ```text
//! capture ─upload─▶ [MagpieFX before the model] ─▶ model (SRCNN ×1/×2, or none)   (upscaler)
//!         ─▶ [deband] ─▶ [MagpieFX after the model] ─▶ scaler (RGBA16F)            (presenter)
//!         ─▶ [CAS, bloom, vignette] ─▶ Delinearize | Dither (8 bit) ─copy─▶ screen
//!         ─▶ [LUT, grain] ─▶ OSD, stats panel ─▶ swapchain | FrameGen
//! ```
//!
//! There is no native effects engine any more: Anime4K_Upscale_S,
//! LumaSharpen and CRT_Easymode are MagpieFX effects like any other (old
//! configs are translated by `lirtup_config::legacy`). MagpieFX effects
//! that do not fix their OUTPUT size render at the content size.
//!
//! The scaler and the converters bind their output at creation, so the
//! stable `screen` texture (what the OSD and the panel blend onto) gets a
//! copy of the converter's output. Asked for in the branch note: a
//! `Conversion::create_into(output)` removes that copy.
//!
//! # The split comparison (live tuning, block 3)
//!
//! [`Presenter::begin_compare`] renders, once and from the same source
//! texture (the frozen frame), A's chain (another [`Config`]: its deband,
//! MagpieFX chain after the model, native effects, scaler and geometry; the
//! model and the chain before it are the shared upscaler's, so they are not
//! compared) into `screen a` and this
//! presenter's own chain (B) into `screen b`; A's passes are dropped right
//! after that render. While the comparison is on, [`Presenter::present`]
//! does not run either chain: it composes `screen a | screen b` at the split
//! into `screen` (`lirtup_effects::compose::SplitCompose`), then records the
//! overlays and presents (or hands `screen` to frame generation), so the OSD,
//! the stats panel and the bar draw over the composition.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lirtup_config::{Config, Downsampler, Geometry, Upsampler};
use lirtup_effects::compose::{self, SplitCompose};
use lirtup_effects::convert::Conversion;
use lirtup_effects::effects::NativeEffect;
use lirtup_effects::scalers::{Scaler, ScalerKind, ScalerParams};
use lirtup_effects::{ParamValues, Pass, util};
use lirtup_gpu::{
    CommandRecorder, Device, Format, PresentOptions, PresentStatus, SubmitOptions, Texture, Timestamps,
};
use lirtup_magpiefx::spec::EffectSpec;
use lirtup_proto::Stage;
use lirtup_x11::{CoordinateMapper, Rect, RgbaImage};

use crate::framegen::FrameGen;
use crate::magpie::{MagpieContext, OutSize, build_magpie_chain, parse_chain, set_chain_params};
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
    /// Time spent waiting to present (the previous frame generation cycle,
    /// or a blocking swapchain present), part of the call's time.
    pub waited: Duration,
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

/// What the effects of a config are, to tell what an update changes.
#[derive(Debug, Clone, PartialEq)]
struct EffectPlan {
    /// Before scaling, on the upscaled frame: the deband (if on). The
    /// MagpieFX chain comes right after it, in `magpie`.
    pre: Vec<NativeEffect>,
    /// The MagpieFX chain after the model (`magpie_effects`).
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
                NativeEffect::Deband { .. } => plan.pre.push(effect),
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
        // The only effects chain: it always runs (fx-unify).
        plan.magpie = parse_chain(&config.scaling.magpie_effects);
        plan
    }
}

/// The parameters of a native effect as `Pass::set_params` names. Every
/// native effect takes its values live; only another LUT preset needs a new
/// pass (checked by [`update_native`]).
fn native_params(effect: &NativeEffect) -> ParamValues {
    let p = |pairs: &[(&str, f64)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
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
    }
}

/// The same MagpieFX effects in the same order (parameters aside).
fn same_names(a: &[EffectSpec], b: &[EffectSpec]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.name == y.name)
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
        if !same_kind || lut_preset(a) != lut_preset(b) {
            return Ok(false);
        }
    }
    for ((pass, a), b) in passes.iter_mut().zip(old).zip(new) {
        if a != b {
            pass.set_params(&native_params(b))?;
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

/// The split comparison on a frozen frame: both sides rendered once,
/// composed into `screen` by every present.
struct Compare {
    /// A (left of the split): the other configuration's chain, rendered.
    /// Kept for the compose pass, which reads it.
    _screen_a: Texture,
    /// B (right of the split): this presenter's chain, rendered.
    _screen_b: Texture,
    compose: SplitCompose,
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
    /// MagpieFX effects disabled since the last [`Presenter::take_disabled`]:
    /// `(name, error)`, for `Event::EffectCompile { session: true }`.
    disabled: Vec<(String, String)>,
    /// The split comparison, while it is on.
    compare: Option<Compare>,
    /// Where the split is (fraction of the width, `0..=1`).
    split: f32,
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
            disabled: Vec::new(),
            compare: None,
            split: 0.5,
        };
        presenter.build_screen_passes()?;
        if !presenter.plan.magpie.is_empty() {
            // Compile now (blocking), so the first frame does not stall.
            let mut ctx = MagpieContext::new(device);
            ctx.precompile(&presenter.plan.magpie, &mut presenter.disabled);
            presenter.magpie = Some(ctx);
        }
        Ok(presenter)
    }

    /// The MagpieFX effects disabled (compile or bind error) since the last
    /// call: `(name, error)`. The session sends each as
    /// `Event::EffectCompile { session: true }`.
    pub fn take_disabled(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.disabled)
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
    ///
    /// While the split comparison is on, the chain does not run: the two
    /// rendered sides are composed into the screen texture instead, and the
    /// overlays draw over that.
    pub fn present(&mut self, overlays: &[&dyn Pass], timestamps: bool) -> Result<PresentOutcome> {
        if self.compare.is_none() {
            self.prepare()?;
            self.set_frame();
        }

        let mut recorder = self.device.recorder()?;
        match &self.compare {
            Some(compare) => compare.compose.record(&mut recorder)?,
            None => self.record_chain(&mut recorder)?,
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

        let (recreate, waited) = match self.framegen.as_mut() {
            Some(fg) => {
                fg.present(&self.screen)?;
                let waited = fg.take_waited();
                (
                    self.surface.lock().map_err(|_| poisoned())?.needs_recreation(),
                    waited,
                )
            }
            None => {
                let started = Instant::now();
                let recreate = self.present_screen()?;
                (recreate, started.elapsed())
            }
        };
        Ok(PresentOutcome {
            timestamps: times,
            recreate,
            waited,
        })
    }

    /// Present the last screen texture again (`present_on_change_only:
    /// false` and nothing changed). Not called while frame generation is
    /// active. During the split comparison the screen texture already holds
    /// the composition (and the overlays of the last present).
    pub fn present_unchanged(&mut self) -> Result<PresentOutcome> {
        let started = Instant::now();
        let recreate = self.present_screen()?;
        Ok(PresentOutcome {
            timestamps: None,
            recreate,
            waited: started.elapsed(),
        })
    }

    fn present_screen(&mut self) -> Result<bool> {
        let mut surface = self.surface.lock().map_err(|_| poisoned())?;
        let status = surface
            .present(&self.screen, &PresentOptions::default())
            .map_err(|e| PipelineError::other(Stage::Swapchain, e.to_string()))?;
        Ok(status != PresentStatus::Ok || surface.needs_recreation())
    }

    // ---- split comparison (live tuning, block 3) -------------------------

    /// Start the split comparison against `other` (A, left of the split);
    /// this presenter's configuration is B (right). Builds A's chain over
    /// the same source texture (models are not compared: the upscaler output
    /// is shared), renders both sides once and composes them into the screen
    /// texture with the split in the middle. Called while active, it
    /// rebuilds A. On error the presenter stays out of the comparison.
    ///
    /// MagpieFX effects of A that fail to compile or bind are left out of A
    /// (logged), as the normal chain does; they were already reported when
    /// A was the current configuration.
    pub fn begin_compare(&mut self, other: &Config) -> Result<()> {
        self.end_compare();
        self.split = 0.5;
        // B: this presenter's chain, up to date with the frozen source.
        self.prepare()?;
        let source = self
            .source
            .clone()
            .ok_or_else(|| PipelineError::other(Stage::Runtime, "compare without a source texture"))?;

        // A: the other configuration over the same source.
        let layout = compare_layout(&self.layout, other);
        let plan = EffectPlan::from_config(other);
        let mut disabled = Vec::new();
        let (mut a_pre, _) = build_pre_chain(
            &self.device,
            &mut self.magpie,
            &layout,
            &plan,
            &source,
            &mut disabled,
        )?;
        for (name, error) in disabled {
            tracing::warn!("compare: MagpieFX effect '{name}' left out of A: {error}");
        }
        let input = a_pre.last().map(|p| p.output().clone()).unwrap_or(source);
        let size = (input.width(), input.height());
        let rect = layout.dst_rect(size);
        let kind = scaler_kind(other, size, (rect.2, rect.3));
        let a_scale = build_scale_stage(
            &self.device,
            other,
            &plan.linear,
            layout.screen,
            &input,
            kind,
            rect,
        )?;
        let screen_a = new_texture(&self.device, layout.screen, "screen a")?;
        let mut a_screen_passes =
            build_screen_passes_on(&self.device, &plan.screen, &screen_a, layout.screen)?;
        let screen_b = new_texture(&self.device, self.layout.screen, "screen b")?;
        let compose = SplitCompose::new(&self.device, &screen_a, &screen_b, &self.screen)?;

        // Both sides once, then the composition (so that a
        // `present_unchanged` before the next `present` shows it too).
        self.set_frame();
        let (frame, time) = (self.frame, self.started.elapsed().as_secs_f32());
        for pass in a_pre.iter_mut().chain(a_screen_passes.iter_mut()) {
            pass.set_frame(frame, time);
        }
        let mut recorder = self.device.recorder()?;
        self.record_chain(&mut recorder)?;
        recorder.copy_texture(&self.screen, &screen_b, &[]);
        record_lane(&mut recorder, &a_pre, &a_scale, &screen_a, &a_screen_passes)?;
        compose.record(&mut recorder)?;
        self.device
            .submit(recorder, &SubmitOptions::default())?
            .wait(self.config.frame_timeout)?;
        // A's passes go now (only `screen a` is needed to compose).
        drop((a_pre, a_scale, a_screen_passes));

        self.compare = Some(Compare {
            _screen_a: screen_a,
            _screen_b: screen_b,
            compose,
        });
        tracing::debug!("split comparison on");
        Ok(())
    }

    /// Where the split is, as a fraction of the output width (clamped to
    /// `0..=1`, NaN = the middle). The next `present` composes with it.
    pub fn set_split(&mut self, x: f32) {
        self.split = compose::clamp_split(x);
        if let Some(compare) = self.compare.as_mut()
            && let Err(e) = compare.compose.set_split(self.split)
        {
            tracing::warn!("compare: cannot move the split: {e}");
        }
    }

    /// The comparison is on.
    pub fn compare_active(&self) -> bool {
        self.compare.is_some()
    }

    /// End the comparison: drop A's resources, back to the normal chain
    /// (the next `present` runs it again).
    pub fn end_compare(&mut self) {
        if self.compare.take().is_some() {
            tracing::debug!("split comparison off");
        }
    }

    /// The swapchain (overlay) has a new size: recreate it, the screen
    /// texture and the scaler output. The caller then rebinds the OSD and
    /// the panel to [`Presenter::screen`].
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        self.end_compare();
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
        self.end_compare();
        let plan = EffectPlan::from_config(config);
        let old_config = std::mem::replace(&mut self.config, config.clone());
        let old = std::mem::replace(&mut self.plan, plan);

        match config.output_geometry.parse() {
            Ok(geometry) if geometry != self.layout.geometry => {
                self.layout.geometry = geometry;
                // MagpieFX effects render at the content size.
                self.pre_stale = true;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("{e}; keeping the geometry"),
        }

        // Before scaling: deband and the MagpieFX chain.
        if (old.pre != self.plan.pre || old.magpie != self.plan.magpie)
            && !self.pre_stale
            && !self.update_pre_params(&old)?
        {
            self.pre_stale = true;
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

    /// Same passes before scaling with new parameter values: the deband's
    /// strength and the MagpieFX effects' parameters through `set_params`,
    /// no recompile (the GUI sliders). `false` when the passes must be
    /// rebuilt (the deband turned on or off, other MagpieFX effects or
    /// order, an effect of the old chain was dropped).
    fn update_pre_params(&mut self, old: &EffectPlan) -> Result<bool> {
        // `pre` is: [deband], then the MagpieFX chain ([SwapRB], effects...,
        // [SwapRB]).
        let deband = old.pre.len();
        if self.pre_stale || deband != self.plan.pre.len() || self.pre.len() < deband {
            return Ok(false);
        }
        if old.magpie != self.plan.magpie {
            if !same_names(&old.magpie, &self.plan.magpie) || self.magpie_active != old.magpie {
                return Ok(false);
            }
            if !set_chain_params(&mut self.pre[deband..], &self.plan.magpie)? {
                return Ok(false);
            }
            self.magpie_active = self.plan.magpie.clone();
        }
        update_native(&mut self.pre[..deband], &old.pre, &self.plan.pre)
    }

    /// Zoom (1.0 = the configured geometry) and offset in output pixels
    /// (`ZoomIn`, `OffsetLeft`, `RestoreView`, ...). The offset is the whole
    /// content offset: the session adds `offset_x`/`offset_y` outside
    /// windowed mode.
    pub fn set_view(&mut self, zoom: f32, offset: (i32, i32)) {
        let zoom = if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 };
        if (zoom - self.layout.zoom).abs() > f32::EPSILON {
            self.layout.zoom = zoom;
            // MagpieFX effects render at the content size.
            if !self.plan.magpie.is_empty() {
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
            out.extend(self.plan.pre.iter().map(|e| e.label().to_owned()));
            out.extend(self.plan.magpie.iter().map(|s| s.name.clone()));
        } else {
            out.extend(
                self.pre
                    .iter()
                    .filter(|p| p.label() != crate::magpie::SWAP_RB_LABEL)
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
            _ => {
                self.scale = Some(build_scale_stage(
                    &self.device,
                    &self.config,
                    &self.plan.linear,
                    self.layout.screen,
                    &input,
                    kind,
                    rect,
                )?)
            }
        }
        Ok(())
    }

    /// Per-frame constants of the chain's animated passes.
    fn set_frame(&mut self) {
        let frame = self.frame;
        let time = self.started.elapsed().as_secs_f32();
        for pass in self.pre.iter_mut().chain(self.screen_passes.iter_mut()) {
            pass.set_frame(frame, time);
        }
    }

    /// Record the whole chain into `screen` (after [`Presenter::prepare`]).
    fn record_chain(&self, recorder: &mut CommandRecorder) -> Result<()> {
        let stage = self.scale.as_ref().expect("prepare builds the scaler");
        record_lane(recorder, &self.pre, stage, &self.screen, &self.screen_passes)
    }

    /// Deband, then the MagpieFX chain.
    fn build_pre(&mut self, source: &Texture) -> Result<()> {
        self.pre.clear();
        let (pre, active) = build_pre_chain(
            &self.device,
            &mut self.magpie,
            &self.layout,
            &self.plan,
            source,
            &mut self.disabled,
        )?;
        self.pre = pre;
        if let Some(active) = active {
            self.magpie_active = active;
        }
        Ok(())
    }

    fn build_screen_passes(&mut self) -> Result<()> {
        self.screen_passes.clear();
        self.screen_passes =
            build_screen_passes_on(&self.device, &self.plan.screen, &self.screen, self.layout.screen)?;
        Ok(())
    }
}

/// Record one chain (`pre`, then the scale stage) into `screen`, then the
/// passes bound to `screen` (LUT, grain).
fn record_lane(
    recorder: &mut CommandRecorder,
    pre: &[Box<dyn Pass>],
    stage: &ScaleStage,
    screen: &Texture,
    screen_passes: &[Box<dyn Pass>],
) -> Result<()> {
    for pass in pre {
        pass.record(recorder)?;
    }
    stage.scaler.record(recorder)?;
    for pass in &stage.linear {
        pass.record(recorder)?;
    }
    stage.convert.record(recorder)?;
    recorder.copy_texture(stage.convert.output(), screen, &[]);
    for pass in screen_passes {
        pass.record(recorder)?;
    }
    Ok(())
}

/// The layout of the A side of the comparison: this presenter's (capture,
/// monitor, screen, zoom, offset) with `other`'s geometry (the current one
/// if `other`'s does not parse).
fn compare_layout(layout: &Layout, other: &Config) -> Layout {
    Layout {
        geometry: other.output_geometry.parse().unwrap_or(layout.geometry),
        ..*layout
    }
}

/// The passes before scaling, and the MagpieFX effects actually bound
/// (`None` without MagpieFX).
type PreChain = (Vec<Box<dyn Pass>>, Option<Vec<EffectSpec>>);

/// The passes before scaling for `plan` on `source`: deband, then the
/// MagpieFX chain (at the content size). Also the MagpieFX effects actually
/// bound (`None` without MagpieFX); the ones that failed are pushed to
/// `disabled`.
fn build_pre_chain(
    device: &Arc<Device>,
    magpie: &mut Option<MagpieContext>,
    layout: &Layout,
    plan: &EffectPlan,
    source: &Texture,
    disabled: &mut Vec<(String, String)>,
) -> Result<PreChain> {
    let content_rect = |layout: &Layout, src: (u32, u32)| {
        let (content, mode) = layout.content();
        let (_, _, w, h) = scaling_rect(src, content, mode);
        (w.max(1), h.max(1))
    };
    let mut pre: Vec<Box<dyn Pass>> = Vec::new();
    let mut active = None;
    let mut input = source.clone();
    for effect in &plan.pre {
        let pass = effect.create(device, &input, (input.width(), input.height()))?;
        input = pass.output().clone();
        pre.push(pass);
    }
    if !plan.magpie.is_empty() {
        let out_size = content_rect(layout, (input.width(), input.height()));
        let (passes, bound) = build_magpie_chain(
            device,
            magpie,
            &input,
            &plan.magpie,
            OutSize::Target(out_size),
            disabled,
        );
        pre.extend(passes);
        active = Some(bound);
    }
    Ok((pre, active))
}

/// The scaler drawing `input` into `rect` of a `screen`-sized texture, the
/// `linear` effects on its output and the converter to 8 bits.
fn build_scale_stage(
    device: &Arc<Device>,
    config: &Config,
    linear: &[NativeEffect],
    screen: (u32, u32),
    input: &Texture,
    kind: ScalerKind,
    rect: (i32, i32, u32, u32),
) -> Result<ScaleStage> {
    let mut scaler = Scaler::new(device, kind, input, screen, rect, &scaler_params(config))?;
    scaler.set_background(background(config));
    let mut passes = Vec::with_capacity(linear.len());
    for effect in linear {
        passes.push(effect.create(device, scaler.output(), screen)?);
    }
    let conversion = if scaler.linear_output() {
        Conversion::Delinearize
    } else {
        Conversion::Dither
    };
    let convert = conversion.create(device, scaler.output(), Format::R8G8B8A8Unorm)?;
    Ok(ScaleStage {
        kind,
        input: (input.width(), input.height()),
        rect,
        scaler,
        linear: passes,
        convert,
    })
}

/// The screen effects (LUT, grain), in place on `screen`.
fn build_screen_passes_on(
    device: &Arc<Device>,
    effects: &[NativeEffect],
    screen: &Texture,
    size: (u32, u32),
) -> Result<Vec<Box<dyn Pass>>> {
    let mut passes = Vec::with_capacity(effects.len());
    for effect in effects {
        passes.push(effect.create(device, screen, size)?);
    }
    Ok(passes)
}

fn new_screen(device: &Arc<Device>, size: (u32, u32)) -> Result<Texture> {
    new_texture(device, size, "screen")
}

/// An 8-bit texture of the screen's format.
fn new_texture(device: &Arc<Device>, size: (u32, u32), label: &str) -> Result<Texture> {
    Ok(util::texture(
        device,
        size.0.max(1),
        size.1.max(1),
        Format::R8G8B8A8Unorm,
        label,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lirtup_config::MagpieEffectSpec;

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
        s.magpie_effects = vec![
            MagpieEffectSpec::Short("Sharpen/LumaSharpen".into()),
            MagpieEffectSpec::Short("CRT/CRT_Easymode".into()),
        ];
        let plan = EffectPlan::from_config(&config);
        let labels = |v: &[NativeEffect]| v.iter().map(|e| e.label()).collect::<Vec<_>>();
        assert_eq!(labels(&plan.pre), ["deband"]);
        let names = plan.magpie.iter().map(|s| s.name.as_str()).collect::<Vec<_>>();
        assert_eq!(names, ["Sharpen/LumaSharpen", "CRT/CRT_Easymode"]);
        assert_eq!(labels(&plan.linear), ["cas"]);
        assert_eq!(labels(&plan.screen), ["lut", "grain"]);
    }

    #[test]
    fn the_magpie_chain_always_runs_and_skips_the_list_before_the_model() {
        let mut config = Config::default();
        assert!(EffectPlan::from_config(&config).magpie.is_empty());
        // The list before the model is the upscaler's.
        config.scaling.magpie_effects_before_model = vec![MagpieEffectSpec::Short("CAS".into())];
        assert!(EffectPlan::from_config(&config).magpie.is_empty());
        config.scaling.magpie_effects = vec![MagpieEffectSpec::Short("Sharpen/LumaSharpen".into())];
        let plan = EffectPlan::from_config(&config);
        assert_eq!(plan.magpie.len(), 1);
        assert_eq!(plan.magpie[0].name, "Sharpen/LumaSharpen");
        assert!(plan.pre.is_empty());
    }

    #[test]
    fn same_names_ignores_the_parameters() {
        let chain = |items: &[&str]| {
            parse_chain(
                &items
                    .iter()
                    .map(|s| MagpieEffectSpec::Short((*s).to_owned()))
                    .collect::<Vec<_>>(),
            )
        };
        let a = chain(&["Sharpen/LumaSharpen:sharpStrength=0.5", "CAS"]);
        let b = chain(&["Sharpen/LumaSharpen:sharpStrength=1", "CAS"]);
        assert_ne!(a, b);
        assert!(same_names(&a, &b));
        assert!(!same_names(&a, &chain(&["CAS", "Sharpen/LumaSharpen"])));
        assert!(!same_names(&a, &chain(&["Sharpen/LumaSharpen"])));
        assert!(same_names(&[], &[]));
    }

    #[test]
    fn native_params_cover_live_changes_only() {
        let p = native_params(&NativeEffect::Cas { strength: 0.25 });
        assert_eq!(p.get("cas_strength"), Some(&0.25));
        let p = native_params(&NativeEffect::Deband { strength: 0.5 });
        assert_eq!(p.get("deband_strength"), Some(&0.5));
        let mut e = NativeEffect::Bloom {
            strength: 0.1,
            threshold: 0.2,
            radius: 3,
        };
        for (k, v) in native_params(&NativeEffect::Bloom {
            strength: 0.5,
            threshold: 0.6,
            radius: 7,
        }) {
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
    fn the_split_is_clamped_like_the_compose_pass() {
        assert_eq!(compose::clamp_split(0.3), 0.3);
        assert_eq!(compose::clamp_split(-0.1), 0.0);
        assert_eq!(compose::clamp_split(1.5), 1.0);
        assert_eq!(compose::clamp_split(f32::NAN), 0.5);
    }

    #[test]
    fn plan_a_differs_only_where_the_other_config_does() {
        let mut current = Config::default();
        current.scaling.deband_enabled = true;
        current.scaling.magpie_effects = vec![MagpieEffectSpec::Short("CAS".into())];
        current.scaling.grain_enabled = true;
        let mut other = current.clone();
        other.scaling.cas_enabled = true;
        let b = EffectPlan::from_config(&current);
        let a = EffectPlan::from_config(&other);
        assert_eq!(a.pre, b.pre);
        assert_eq!(a.screen, b.screen);
        assert_eq!(a.magpie, b.magpie);
        let labels = |v: &[NativeEffect]| v.iter().map(|e| e.label()).collect::<Vec<_>>();
        assert!(labels(&b.linear).is_empty());
        assert_eq!(labels(&a.linear), ["cas"]);
        // The same configuration gives the same plan.
        assert_eq!(EffectPlan::from_config(&current.clone()), b);
    }

    #[test]
    fn layout_a_takes_only_the_geometry_of_the_other_config() {
        let layout = Layout {
            capture: (800, 600),
            monitor: (1920, 1080),
            screen: (1920, 1080),
            geometry: Geometry::Fit,
            zoom: 1.25,
            offset: (10, -4),
        };
        let mut other = Config {
            output_geometry: lirtup_config::OutputGeometry("50%".into()),
            ..Default::default()
        };
        let a = compare_layout(&layout, &other);
        assert_eq!(
            a.geometry,
            Geometry::Percent {
                percent: 50.0,
                force: false
            }
        );
        assert_eq!(
            Layout {
                geometry: Geometry::Fit,
                ..a
            },
            layout
        );
        // A geometry that does not parse keeps the current one.
        other.output_geometry = lirtup_config::OutputGeometry("huge".into());
        assert_eq!(compare_layout(&layout, &other), layout);
    }

    #[test]
    fn background_is_bgra() {
        let config = Config::default();
        let [b, g, r, a] = background(&config);
        assert_eq!([r, g, b, a], config.background_color.to_rgba());
    }
}
