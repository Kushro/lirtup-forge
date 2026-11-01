//! «Medir en mi GPU» (`bench.py`). Front S, round 2.
//!
//! **It runs in a child process**, as in 1.x: `lirtup-core` answers
//! `Command::Bench` by starting `lirtup bench --json <request>` (the same
//! binary; the subcommand is front K's) and relaying what it prints. Each
//! line the child prints is an `Envelope<Event>`
//! (`lirtup_proto::encode_line`): `Event::BenchProgress` per case and one
//! `Event::BenchResult` at the end. `Command::CancelBench` is SIGTERM to the
//! child, which then prints a `BenchResult` with `cancelled: true`.
//!
//! Why a process: lsfg keeps global state and allows one engine per process
//! (`lirtup_fg::native::LsfgEngine`), so the frame generation cases cannot
//! run next to a session; and a driver crash while measuring does not take
//! the scaler down.
//!
//! The cases run headless on a synthetic test image, straight on
//! `lirtup-effects`, `lirtup-magpiefx` and `lirtup-fg` (not through the
//! presenter): native effects and MagpieFX with Vulkan timestamps, frame
//! generation timed on the CPU per cycle, like 1.x. Results are kept per GPU
//! in `bench.json` next to the configuration (same file and shape as 1.x).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lirtup_config::{EffectsEngine, ModeSettings, ModeStore, paths};
use lirtup_effects::Pass;
use lirtup_effects::effects::NativeEffect;
use lirtup_fg::dll::{DllKind, find_dll};
use lirtup_fg::native::{EngineOptions, LsfgContext, LsfgEngine};
use lirtup_gpu::{
    Device, ExternalSemaphore, ExternalTexture, Format, SubmitOptions, Texture, TextureDesc, TextureUsage,
};
use lirtup_magpiefx::compiler::{HlslCompiler, SpirvCache, detect_compilers};
use lirtup_magpiefx::runtime::{compile_effect_with, fp16_wanted};
use lirtup_magpiefx::spec::parse_effect_spec;
use lirtup_magpiefx::{Catalog, CompileOptions, MagpieEffect};
use lirtup_proto::{BenchEntry, BenchKind, BenchReport, BenchRequest, Size, Stage};
use serde_json::{Map, Value, json};

use crate::{PipelineError, Result};

/// Native effects of a run without a mode, under their 1.x (MagpieFX)
/// names, with the label of the pass they are.
const NATIVE_EFFECTS: [(&str, &str); 3] = [
    ("LumaSharpen", "luma_sharpen"),
    ("Anime4K_Upscale_S", "anime4k"),
    ("CRT_Easymode", "crt_easymode"),
];

/// Frame generation settings measured (1.x order: model, flow scale,
/// multiplier).
const FRAMEGEN_MODELS: [bool; 2] = [false, true];
const FRAMEGEN_FLOW_SCALES: [f32; 2] = [1.0, 0.5];
const FRAMEGEN_MULTIPLIERS: [u32; 3] = [2, 3, 4];

/// `VK_FORMAT_R8G8B8A8_UNORM`, for lsfg.
const VK_FORMAT_R8G8B8A8_UNORM: u32 = 37;
/// A generated frame later than this fails the case.
const LSFG_TIMEOUT_NS: u64 = 2_000_000_000;
/// A measured frame later than this fails the case.
const FRAME_TIMEOUT_NS: u64 = 5_000_000_000;

/// `bench.json` next to the configuration.
const RESULTS_FILE: &str = "bench.json";
const RESULTS_VERSION: u64 = 1;

/// One measurement to make.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchCase {
    /// Native, MagpieFX or frame generation.
    pub kind: BenchKind,
    /// Effect name, or the frame generation key (`x2 flow 1.0 quality`, as
    /// `framegen_key` of 1.x).
    ///
    /// The key is spelled as 1.x writes it in `bench.json`
    /// (`x2 flow 1.0 quality`), so the stored results stay readable by
    /// both versions.
    pub name: String,
}

/// Limits of a run (`BenchSettings` of 1.x).
#[derive(Debug, Clone, PartialEq)]
pub struct BenchSettings {
    /// Warm-up frames per case, not measured.
    pub warmup: u32,
    /// Measured frames per case, at least.
    pub min_frames: u32,
    /// Measured frames per case, at most.
    pub max_frames: u32,
    /// Stop measuring a case after this (once `min_frames` are done).
    pub budget: Duration,
}

impl Default for BenchSettings {
    fn default() -> BenchSettings {
        BenchSettings {
            warmup: 5,
            min_frames: 10,
            max_frames: 100,
            budget: Duration::from_millis(1500),
        }
    }
}

/// Progress of a run.
#[derive(Debug, Clone, PartialEq)]
pub enum BenchProgress {
    /// A case is about to run (`index` from 0).
    Started {
        /// Index of the case.
        index: u32,
        /// Number of cases.
        total: u32,
        /// The case.
        case: BenchCase,
    },
    /// A case finished (measured or failed).
    Done {
        /// Cases done.
        done: u32,
        /// Number of cases.
        total: u32,
        /// Its result.
        entry: BenchEntry,
    },
}

/// Name of a frame generation case (1.x `framegen_key`).
fn framegen_key(multiplier: u32, flow_scale: f32, performance: bool) -> String {
    let model = if performance { "performance" } else { "quality" };
    format!("x{multiplier} flow {flow_scale:.1} {model}")
}

/// The settings of a frame generation key.
fn parse_framegen_key(key: &str) -> Option<(u32, f32, bool)> {
    let mut parts = key.split(' ');
    let multiplier = parts.next()?.strip_prefix('x')?.parse().ok()?;
    if parts.next()? != "flow" {
        return None;
    }
    let flow = parts.next()?.parse().ok()?;
    let performance = match parts.next()? {
        "performance" => true,
        "quality" => false,
        _ => return None,
    };
    parts.next().is_none().then_some((multiplier, flow, performance))
}

/// The mode a request names, if it exists.
fn load_mode(id: &str) -> Option<ModeSettings> {
    match ModeStore::load(&paths::modes_dir()) {
        Ok((store, _)) => store.get(id).map(|m| m.settings.clone()),
        Err(e) => {
            tracing::warn!("bench: cannot read the modes: {e}");
            None
        }
    }
}

/// The cases of a request, in the order they are measured (`build_cases`).
///
/// - Without a mode: the three native effects of 1.x (LumaSharpen,
///   Anime4K_Upscale_S, CRT_Easymode). With a mode: its native passes (by
///   pass label) or, with the MagpieFX engine, its chain.
/// - Then `effects` (MagpieFX), without repeating one of the mode.
/// - Then, with `framegen`, ×2 to ×4 at flow 1.0 and 0.5, quality and
///   performance model.
pub fn build_cases(request: &BenchRequest) -> Vec<BenchCase> {
    let mut cases = Vec::new();
    let mut magpie: Vec<String> = Vec::new();
    match request.mode.as_deref() {
        None => cases.extend(NATIVE_EFFECTS.iter().map(|(name, _)| BenchCase {
            kind: BenchKind::Native,
            name: (*name).to_owned(),
        })),
        Some(id) => match load_mode(id) {
            Some(mode) => {
                cases.extend(NativeEffect::from_mode(&mode).iter().map(|e| BenchCase {
                    kind: BenchKind::Native,
                    name: e.label().to_owned(),
                }));
                if mode.effects_engine == EffectsEngine::Magpiefx {
                    for item in &mode.magpie_effects {
                        match parse_effect_spec(item) {
                            Ok(spec) => magpie.push(spec.name),
                            Err(e) => tracing::warn!("bench: {e}"),
                        }
                    }
                }
            }
            None => tracing::warn!("bench: mode '{id}' not found"),
        },
    }
    magpie.extend(request.effects.iter().cloned());
    let mut seen = std::collections::HashSet::new();
    cases.extend(
        magpie
            .into_iter()
            .filter(|name| seen.insert(name.clone()))
            .map(|name| BenchCase {
                kind: BenchKind::Magpiefx,
                name,
            }),
    );
    if request.framegen {
        for performance in FRAMEGEN_MODELS {
            for flow in FRAMEGEN_FLOW_SCALES {
                for multiplier in FRAMEGEN_MULTIPLIERS {
                    cases.push(BenchCase {
                        kind: BenchKind::Framegen,
                        name: framegen_key(multiplier, flow, performance),
                    });
                }
            }
        }
    }
    cases
}

/// The synthetic test frame (`make_test_image`): RGBA8, gradients, fine
/// lines, flat areas and noise; `shift` moves the pattern to make a second,
/// different frame.
///
/// Same layout as 1.x: red is a vertical gradient, green a radial one from
/// the center, blue noise around 128; white vertical lines in the top
/// third, black horizontal lines in the middle third, a red box in the
/// bottom third. The noise is a fixed pseudo-random sequence (1.x used
/// PIL's), so two runs time the same image.
pub fn test_image(width: u32, height: u32, shift: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut data = vec![0u8; w * h * 4];
    let mut seed: u32 = 0x9e37_79b9;
    let mut noise = move || {
        // Sum of four uniforms ≈ Gaussian; sigma 40 around 128.
        let mut sum = 0.0;
        for _ in 0..4 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            sum += seed as f32 / u32::MAX as f32;
        }
        (128.0 + (sum - 2.0) * 40.0 / 0.577).clamp(0.0, 255.0) as u8
    };
    for y in 0..h {
        let r = (y * 255 / h.saturating_sub(1).max(1)) as u8;
        for x in 0..w {
            let dx = (x as f32 + 0.5) / w as f32 - 0.5;
            let dy = (y as f32 + 0.5) / h as f32 - 0.5;
            let g = ((dx * dx + dy * dy).sqrt() * 2.0 * 255.0).min(255.0) as u8;
            let i = (y * w + x) * 4;
            data[i..i + 4].copy_from_slice(&[r, g, noise(), 255]);
        }
    }
    let mut put = |x: usize, y: usize, rgb: [u8; 3]| {
        if x < w && y < h {
            let i = (y * w + x) * 4;
            data[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    };
    let step = (w / 64).max(8);
    for x in ((shift as usize % step)..w).step_by(step) {
        for y in 0..=h / 3 {
            put(x, y, [255, 255, 255]);
        }
    }
    for y in (h / 3..2 * h / 3).step_by((step / 2).max(4)) {
        for x in 0..w {
            put(x, y, [0, 0, 0]);
        }
    }
    let x0 = w / 2 + shift as usize;
    for y in 2 * h / 3..h {
        for x in x0..=(x0 + w / 4) {
            put(x, y, [200, 60, 40]);
        }
    }
    data
}

/// RGBA → BGRA in place (a capture holds BGRA, as 1.x uploaded the source).
fn swap_rb(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
}

/// GPU key of the results file (1.x `gpu_key`: `vendor:device:name`).
fn gpu_key(device: &Device) -> String {
    let info = device.info();
    format!("{:04x}:{:04x}:{}", info.vendor_id, info.device_id, info.name)
}

/// One prepared case: each step runs a frame and returns its ms.
trait Runner {
    fn step(&mut self) -> Result<f64>;
}

/// A native pass or a MagpieFX effect, timed with Vulkan timestamps (GPU
/// time from the first dispatch to the end of the last one).
struct PassRunner {
    device: Arc<Device>,
    pass: Box<dyn Pass>,
    frame: u64,
}

impl Runner for PassRunner {
    fn step(&mut self) -> Result<f64> {
        self.frame += 1;
        self.pass.set_frame(self.frame, self.frame as f32 / 60.0);
        let mut recorder = self.device.recorder()?;
        self.pass.record(&mut recorder)?;
        let options = SubmitOptions {
            timestamps: true,
            ..SubmitOptions::default()
        };
        let times = self.device.submit(recorder, &options)?.wait(FRAME_TIMEOUT_NS)?;
        times
            .map(|t| t.total_ms)
            .ok_or_else(|| PipelineError::other(Stage::Device, "the GPU has no compute timestamps"))
    }
}

/// One lsfg configuration: an engine, a context at the target size, and
/// cycles timed on the CPU, from the copy of a new real frame into the
/// shared input until every generated frame is signalled (1.x
/// `_LsfgRunner`).
struct LsfgRunner {
    device: Arc<Device>,
    frames: [Texture; 2],
    inputs: [ExternalTexture; 2],
    _outputs: Vec<ExternalTexture>,
    input_ready: ExternalSemaphore,
    outputs_ready: Vec<ExternalSemaphore>,
    /// Dropped before the textures: lsfg deletes the context, and the
    /// engine is finalized with the last reference.
    context: LsfgContext,
    frame: usize,
}

impl LsfgRunner {
    fn new(device: &Arc<Device>, frames: &[Texture; 2], dll: &Path, case: &BenchCase) -> Result<LsfgRunner> {
        let (multiplier, flow_scale, performance) = parse_framegen_key(&case.name).ok_or_else(|| {
            PipelineError::other(
                Stage::FrameGen,
                format!("bad frame generation case {}", case.name),
            )
        })?;
        let info = device.info();
        if !info.supports_external_fd {
            return Err(PipelineError::other(
                Stage::FrameGen,
                "the GPU driver cannot share memory by fd (VK_KHR_external_memory_fd)",
            ));
        }
        let generated = multiplier - 1;
        let engine = LsfgEngine::initialize(&EngineOptions {
            dll: dll.to_owned(),
            vendor_id: info.vendor_id,
            device_id: info.device_id,
            generated,
            flow_scale,
            performance,
            hdr: false,
            fp16: info.supports_fp16,
        })?;
        let (w, h) = (frames[0].width(), frames[0].height());
        let desc = |label: &str| TextureDesc {
            width: w,
            height: h,
            format: Format::R8G8B8A8Unorm,
            slices: 1,
            force_array_view: false,
            usage: TextureUsage::default(),
            label: label.to_owned(),
        };
        let inputs = [
            device.create_external_texture(&desc("bench lsfg input 0"))?,
            device.create_external_texture(&desc("bench lsfg input 1"))?,
        ];
        let outputs = (0..generated)
            .map(|i| device.create_external_texture(&desc(&format!("bench lsfg output {i}"))))
            .collect::<lirtup_gpu::Result<Vec<_>>>()?;
        let input_ready = device.create_external_semaphore()?;
        let outputs_ready = (0..generated)
            .map(|_| device.create_external_semaphore())
            .collect::<lirtup_gpu::Result<Vec<_>>>()?;
        let context = engine.create_context(
            [inputs[0].export_fd()?, inputs[1].export_fd()?],
            outputs
                .iter()
                .map(ExternalTexture::export_fd)
                .collect::<lirtup_gpu::Result<_>>()?,
            w,
            h,
            VK_FORMAT_R8G8B8A8_UNORM,
        )?;
        Ok(LsfgRunner {
            device: device.clone(),
            frames: frames.clone(),
            inputs,
            _outputs: outputs,
            input_ready,
            outputs_ready,
            context,
            frame: 0,
        })
    }
}

impl Runner for LsfgRunner {
    fn step(&mut self) -> Result<f64> {
        let start = Instant::now();
        let slot = self.frame % 2;
        let mut recorder = self.device.recorder()?;
        recorder.copy_texture(&self.frames[slot], self.inputs[slot].texture(), &[]);
        let copy = self.device.submit(
            recorder,
            &SubmitOptions {
                signal: &[&self.input_ready],
                ..SubmitOptions::default()
            },
        )?;
        self.context.present(
            self.input_ready.export_fd()?,
            self.outputs_ready
                .iter()
                .map(ExternalSemaphore::export_fd)
                .collect::<lirtup_gpu::Result<_>>()?,
        )?;
        // An empty submission that waits on every generated frame.
        let waits: Vec<&ExternalSemaphore> = self.outputs_ready.iter().collect();
        let done = self.device.submit(
            self.device.recorder()?,
            &SubmitOptions {
                wait: &waits,
                ..SubmitOptions::default()
            },
        )?;
        copy.wait(LSFG_TIMEOUT_NS)?;
        done.wait(LSFG_TIMEOUT_NS).map_err(|e| {
            PipelineError::other(
                Stage::FrameGen,
                format!("lsfg did not finish a generated frame in time ({e})"),
            )
        })?;
        self.frame += 1;
        Ok(start.elapsed().as_secs_f64() * 1000.0)
    }
}

/// The MagpieFX catalog, compilers and SPIR-V cache of a run.
type MagpieTools = (Catalog, Vec<Box<dyn HlslCompiler>>, SpirvCache);

/// What the runners share across cases: the source frame, the lsfg
/// frames, the MagpieFX compilers.
struct Backend<'a> {
    device: &'a Arc<Device>,
    request: &'a BenchRequest,
    mode: ModeSettings,
    source: Option<Texture>,
    lsfg_frames: Option<[Texture; 2]>,
    magpie: Option<MagpieTools>,
    dll: Option<Option<PathBuf>>,
}

impl Backend<'_> {
    fn src_size(&self) -> (u32, u32) {
        (self.request.src.width.max(1), self.request.src.height.max(1))
    }

    fn dst_size(&self) -> (u32, u32) {
        (self.request.dst.width.max(1), self.request.dst.height.max(1))
    }

    /// The input-size frame, BGRA bytes like a capture.
    fn source(&mut self) -> Result<Texture> {
        if let Some(src) = &self.source {
            return Ok(src.clone());
        }
        let (w, h) = self.src_size();
        let mut data = test_image(w, h, 0);
        swap_rb(&mut data);
        let tex = self.upload(w, h, &data, "bench source")?;
        self.source = Some(tex.clone());
        Ok(tex)
    }

    fn upload(&self, w: u32, h: u32, data: &[u8], label: &str) -> Result<Texture> {
        let tex = lirtup_effects::util::texture(self.device, w, h, Format::R8G8B8A8Unorm, label)?;
        lirtup_effects::util::upload_texture(self.device, &tex, data)?;
        Ok(tex)
    }

    fn runner(&mut self, case: &BenchCase) -> Result<Box<dyn Runner>> {
        match case.kind {
            BenchKind::Native => self.native(&case.name),
            BenchKind::Magpiefx => self.magpiefx(&case.name),
            BenchKind::Framegen => self.framegen(case),
        }
    }

    fn native(&mut self, name: &str) -> Result<Box<dyn Runner>> {
        let label = NATIVE_EFFECTS
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(name, |(_, label)| *label);
        let effect = all_native(&self.mode)
            .into_iter()
            .find(|e| e.label() == label)
            .ok_or_else(|| PipelineError::other(Stage::Effects, format!("{name} is not a native effect")))?;
        let src = self.source()?;
        let size = effect.output_size((src.width(), src.height()), self.dst_size());
        let pass = effect.create(self.device, &src, size)?;
        Ok(Box::new(PassRunner {
            device: self.device.clone(),
            pass,
            frame: 0,
        }))
    }

    fn magpiefx(&mut self, name: &str) -> Result<Box<dyn Runner>> {
        let src = self.source()?;
        let (catalog, compilers, cache) = self.magpie.get_or_insert_with(|| {
            (
                Catalog::scan(Some(&paths::user_effects_dir())),
                detect_compilers(),
                SpirvCache::new(paths::magpiefx_cache_dir()),
            )
        });
        if compilers.is_empty() {
            return Err(PipelineError::other(
                Stage::Effects,
                "no HLSL compiler found (dxc or glslang)",
            ));
        }
        let entry = catalog.resolve(name)?;
        // As the presenter runs it: BGRA input, the swap folded in.
        let options = CompileOptions {
            fp16: fp16_wanted(self.device),
            swap_input: true,
            swap_output: true,
        };
        let compiled = compile_effect_with(&entry, compilers, cache, &options)?;
        let size = MagpieEffect::output_size(&compiled, (src.width(), src.height()), self.dst_size())?;
        let pass = MagpieEffect::new(self.device, &compiled, &src, size, &Default::default())?;
        Ok(Box::new(PassRunner {
            device: self.device.clone(),
            pass: Box::new(pass),
            frame: 0,
        }))
    }

    fn framegen(&mut self, case: &BenchCase) -> Result<Box<dyn Runner>> {
        let dll = self
            .dll
            .get_or_insert_with(|| find_dll(DllKind::Lossless, None))
            .clone()
            .ok_or_else(|| PipelineError::other(Stage::FrameGen, "Lossless.dll not found"))?;
        if self.lsfg_frames.is_none() {
            let (w, h) = self.dst_size();
            let a = self.upload(w, h, &test_image(w, h, 0), "bench lsfg frame 0")?;
            let b = self.upload(w, h, &test_image(w, h, (w / 160).max(4)), "bench lsfg frame 1")?;
            self.lsfg_frames = Some([a, b]);
        }
        let frames = self.lsfg_frames.as_ref().expect("just created");
        Ok(Box::new(LsfgRunner::new(self.device, frames, &dll, case)?))
    }
}

/// Every native effect, with the parameters of `mode`.
fn all_native(mode: &ModeSettings) -> Vec<NativeEffect> {
    let mut all = mode.clone();
    all.effects_engine = EffectsEngine::Native;
    all.deband_enabled = true;
    all.anime4k_enabled = true;
    all.luma_sharpen_enabled = true;
    all.crt_enabled = true;
    all.cas_enabled = true;
    all.bloom_enabled = true;
    all.vignette_enabled = true;
    all.grain_enabled = true;
    all.lut_enabled = true;
    NativeEffect::from_mode(&all)
}

/// The first line of an error, at most 300 characters (1.x `_error_text`).
fn error_text(e: &PipelineError) -> String {
    let text = e.to_string();
    let line = text.trim().lines().next().unwrap_or("error");
    line.chars().take(300).collect()
}

/// Median (mean of the two middle values for an even count) and 95th
/// percentile of frame times (1.x `summarize`).
fn summarize(times: &[f64]) -> (f64, f64) {
    let mut ordered = times.to_vec();
    ordered.sort_by(f64::total_cmp);
    let n = ordered.len();
    let median = if n % 2 == 1 {
        ordered[n / 2]
    } else {
        (ordered[n / 2 - 1] + ordered[n / 2]) / 2.0
    };
    let rank = ((n as f64 * 0.95 + 0.5) as usize).max(1) - 1;
    (median, ordered[rank.min(n - 1)])
}

/// Measure one case. `Err(Cancelled)` when `cancel` is set; any other
/// failure is the entry's error.
fn measure(
    backend: &mut Backend<'_>,
    case: &BenchCase,
    settings: &BenchSettings,
    cancel: &AtomicBool,
) -> Result<BenchEntry> {
    let check = || {
        if cancel.load(Ordering::Acquire) {
            Err(PipelineError::Cancelled)
        } else {
            Ok(())
        }
    };
    let failed = |e: PipelineError| BenchEntry {
        kind: case.kind,
        name: case.name.clone(),
        ms: None,
        p95: None,
        frames: 0,
        error: Some(error_text(&e)),
    };
    check()?;
    let mut runner = match backend.runner(case) {
        Ok(runner) => runner,
        Err(e) => return Ok(failed(e)),
    };
    let mut times = Vec::new();
    let result = (|| {
        for _ in 0..settings.warmup {
            check()?;
            runner.step()?;
        }
        let start = Instant::now();
        while times.len() < settings.max_frames.max(1) as usize {
            check()?;
            times.push(runner.step()?);
            if times.len() >= settings.min_frames as usize && start.elapsed() >= settings.budget {
                break;
            }
        }
        Ok(())
    })();
    match result {
        Err(PipelineError::Cancelled) => Err(PipelineError::Cancelled),
        Err(e) => Ok(failed(e)),
        Ok(()) => {
            let (median, p95) = summarize(&times);
            Ok(BenchEntry {
                kind: case.kind,
                name: case.name.clone(),
                ms: Some(median as f32),
                p95: Some(p95 as f32),
                frames: times.len() as u32,
                error: None,
            })
        }
    }
}

/// Run every case of `request` on `device`. Checks `cancel` between frames
/// and returns a report with `cancelled: true` when set. `progress` is
/// called from this thread.
///
/// A case that fails is reported with its error and the run goes on.
/// `request.frames` (when not 0) caps the measured frames per case, like
/// `--max-frames` of 1.x.
pub fn run(
    device: &Arc<Device>,
    request: &BenchRequest,
    settings: &BenchSettings,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(BenchProgress),
) -> Result<BenchReport> {
    let mut settings = settings.clone();
    if request.frames > 0 {
        settings.max_frames = request.frames;
        settings.min_frames = settings.min_frames.min(request.frames);
    }
    let cases = build_cases(request);
    let total = cases.len() as u32;
    let mut backend = Backend {
        device,
        request,
        mode: request.mode.as_deref().and_then(load_mode).unwrap_or_default(),
        source: None,
        lsfg_frames: None,
        magpie: None,
        dll: None,
    };
    let mut report = BenchReport {
        gpu: gpu_key(device),
        entries: Vec::new(),
        cancelled: false,
    };
    for (index, case) in cases.into_iter().enumerate() {
        progress(BenchProgress::Started {
            index: index as u32,
            total,
            case: case.clone(),
        });
        match measure(&mut backend, &case, &settings, cancel) {
            Ok(entry) => {
                report.entries.push(entry.clone());
                progress(BenchProgress::Done {
                    done: index as u32 + 1,
                    total,
                    entry,
                });
            }
            Err(PipelineError::Cancelled) => {
                report.cancelled = true;
                break;
            }
            Err(e) => return Err(e),
        }
    }
    drop(backend);
    if let Err(e) = device.wait_idle() {
        tracing::warn!("bench: wait idle: {e}");
    }
    Ok(report)
}

/// `bench.json` next to the configuration.
pub fn results_path() -> PathBuf {
    paths::config_dir().join(RESULTS_FILE)
}

/// The whole results file (`{"version", "gpus": {key: entry},
/// "selectors"}`); a missing or broken file is an empty one.
fn load_results(path: &Path) -> Map<String, Value> {
    let empty = || {
        let mut data = Map::new();
        data.insert("version".into(), json!(RESULTS_VERSION));
        data.insert("gpus".into(), json!({}));
        data.insert("selectors".into(), json!({}));
        data
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return empty();
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(mut data)) if data.get("gpus").is_some_and(Value::is_object) => {
            if !data.get("selectors").is_some_and(Value::is_object) {
                data.insert("selectors".into(), json!({}));
            }
            data
        }
        _ => empty(),
    }
}

fn kind_key(kind: BenchKind) -> &'static str {
    match kind {
        BenchKind::Native => "native",
        BenchKind::Magpiefx => "magpiefx",
        BenchKind::Framegen => "framegen",
    }
}

/// 4 decimals, as 1.x stores times.
fn round4(v: f32) -> f64 {
    (f64::from(v) * 1e4).round() / 1e4
}

/// Merge a report into `bench.json` under its GPU (atomic write).
///
/// As 1.x (`save_results`): a case measured again replaces the old value;
/// failed cases are kept with their error; cancelled runs keep what they
/// measured. `sizes.<kind>` records the input and target sizes of the run
/// (only the target for frame generation): times at other sizes are not
/// comparable, so a kind measured at new sizes starts over. `selector` is
/// the `gpu` setting the run used (`None` = automatic, stored as `""`), so
/// `selectors` finds the entry without opening the GPU.
pub fn save_results(
    path: &Path,
    report: &BenchReport,
    input: Size,
    target: Size,
    selector: Option<&str>,
) -> Result<()> {
    let mut data = load_results(path);
    let stamp = utc_stamp(SystemTime::now());
    let name = report.gpu.splitn(3, ':').nth(2).unwrap_or(&report.gpu).to_owned();
    let gpus = data
        .entry("gpus")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("checked by load_results");
    let entry = gpus.entry(report.gpu.clone()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let entry = entry.as_object_mut().expect("just made an object");
    entry.insert("name".into(), json!(name));
    for e in &report.entries {
        let kind = kind_key(e.kind);
        let mut sizes = Map::new();
        sizes.insert("target".into(), json!([target.width, target.height]));
        if e.kind != BenchKind::Framegen {
            sizes.insert("input".into(), json!([input.width, input.height]));
        }
        let sizes = Value::Object(sizes);
        let all_sizes = entry.entry("sizes").or_insert_with(|| json!({}));
        if !all_sizes.is_object() {
            *all_sizes = json!({});
        }
        let all_sizes = all_sizes.as_object_mut().expect("just made an object");
        if all_sizes.get(kind) != Some(&sizes) {
            all_sizes.insert(kind.into(), sizes);
            entry.insert(kind.into(), json!({}));
        }
        let cases = entry.entry(kind).or_insert_with(|| json!({}));
        if !cases.is_object() {
            *cases = json!({});
        }
        cases.as_object_mut().expect("just made an object").insert(
            e.name.clone(),
            json!({
                "ms": e.ms.map(round4),
                "p95": e.p95.map(round4),
                "frames": e.frames,
                "error": e.error,
            }),
        );
        let updated = entry.entry("updated").or_insert_with(|| json!({}));
        if !updated.is_object() {
            *updated = json!({});
        }
        updated
            .as_object_mut()
            .expect("just made an object")
            .insert(kind.into(), json!(stamp));
    }
    data.entry("selectors")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("checked by load_results")
        .insert(selector.unwrap_or_default().to_owned(), json!(report.gpu));
    data.insert("version".into(), json!(RESULTS_VERSION));

    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let text = serde_json::to_string_pretty(&Value::Object(data))
        .map_err(|e| PipelineError::other(Stage::Runtime, format!("bench results: {e}")))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// The stored entries of a GPU (`results_for_gpu`), for the costs the GUI
/// shows before measuring again.
///
/// `gpu` is a GPU key (`vendor:device:name`) or, as in 1.x, the `gpu`
/// setting: the GPU last measured with it, else the first whose name
/// contains it, else the only measured GPU. Nothing found is an empty list.
pub fn results_for_gpu(path: &Path, gpu: &str) -> Result<Vec<BenchEntry>> {
    let data = load_results(path);
    let Some(gpus) = data.get("gpus").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let by_selector = data
        .get("selectors")
        .and_then(|s| s.get(gpu))
        .and_then(Value::as_str)
        .and_then(|key| gpus.get(key));
    let lower = gpu.to_lowercase();
    let by_name = || {
        (!gpu.is_empty())
            .then(|| {
                gpus.values().find(|e| {
                    e.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|n| n.to_lowercase().contains(&lower))
                })
            })
            .flatten()
    };
    let entry = gpus
        .get(gpu)
        .or(by_selector)
        .or_else(by_name)
        .or_else(|| (gpus.len() == 1).then(|| gpus.values().next()).flatten());
    let Some(entry) = entry else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for kind in [BenchKind::Native, BenchKind::Magpiefx, BenchKind::Framegen] {
        let Some(cases) = entry.get(kind_key(kind)).and_then(Value::as_object) else {
            continue;
        };
        for (name, value) in cases {
            let number = |key: &str| value.get(key).and_then(Value::as_f64).map(|v| v as f32);
            out.push(BenchEntry {
                kind,
                name: name.clone(),
                ms: number("ms"),
                p95: number("p95"),
                frames: value.get("frames").and_then(Value::as_u64).unwrap_or(0) as u32,
                error: value.get("error").and_then(Value::as_str).map(str::to_owned),
            });
        }
    }
    Ok(out)
}

/// `2026-11-01T12:34:56+00:00` (1.x `datetime.isoformat(timespec="seconds")`
/// in UTC).
fn utc_stamp(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lirtup_proto::Size;

    fn request(framegen: bool, effects: &[&str]) -> BenchRequest {
        BenchRequest {
            src: Size {
                width: 1280,
                height: 720,
            },
            dst: Size {
                width: 2560,
                height: 1440,
            },
            mode: None,
            effects: effects.iter().map(|s| (*s).to_owned()).collect(),
            framegen,
            frames: 0,
        }
    }

    #[test]
    fn cases_follow_the_1x_order() {
        let cases = build_cases(&request(true, &["CAS", "FSR", "CAS"]));
        let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            &names[..5],
            ["LumaSharpen", "Anime4K_Upscale_S", "CRT_Easymode", "CAS", "FSR"]
        );
        assert_eq!(cases.len(), 5 + 12);
        assert_eq!(names[5], "x2 flow 1.0 quality");
        assert_eq!(names[7], "x4 flow 1.0 quality");
        assert_eq!(names[8], "x2 flow 0.5 quality");
        assert_eq!(names[16], "x4 flow 0.5 performance");
        assert!(cases[5..].iter().all(|c| c.kind == BenchKind::Framegen));
    }

    #[test]
    fn framegen_keys_round_trip() {
        for (m, f, p) in [(2, 1.0, false), (4, 0.5, true)] {
            assert_eq!(parse_framegen_key(&framegen_key(m, f, p)), Some((m, f, p)));
        }
        assert_eq!(parse_framegen_key("x2 flow 1.0"), None);
    }

    #[test]
    fn native_names_map_to_passes() {
        let all = all_native(&ModeSettings::default());
        for (_, label) in NATIVE_EFFECTS {
            assert!(all.iter().any(|e| e.label() == label), "{label}");
        }
    }

    #[test]
    fn summary_matches_1x() {
        assert_eq!(summarize(&[3.0, 1.0, 2.0]), (2.0, 3.0));
        assert_eq!(summarize(&[1.0, 2.0, 3.0, 4.0]), (2.5, 4.0));
        let times: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(summarize(&times), (50.5, 95.0));
    }

    #[test]
    fn test_image_has_its_features() {
        let (w, h) = (128, 96);
        let a = test_image(w, h, 0);
        assert_eq!(a.len(), (w * h * 4) as usize);
        assert_eq!(&a[..4][..3], &[255, 255, 255]); // a line at x = 0
        let px = |x: u32, y: u32| &a[((y * w + x) * 4) as usize..][..3];
        assert_eq!(px(w / 2 + 1, h - 1), &[200, 60, 40]);
        assert_eq!(px(5, h / 3), &[0, 0, 0]);
        assert_ne!(test_image(w, h, 4), a);
    }

    #[test]
    fn utc_stamp_is_iso() {
        let t = UNIX_EPOCH + Duration::from_secs(1_793_536_496); // 2026-11-01T12:34:56Z
        assert_eq!(utc_stamp(t), "2026-11-01T12:34:56+00:00");
        assert_eq!(utc_stamp(UNIX_EPOCH), "1970-01-01T00:00:00+00:00");
    }

    #[test]
    fn results_merge_and_read_back() {
        let dir = std::env::temp_dir().join(format!("lirtup-bench-{}", std::process::id()));
        let path = dir.join("bench.json");
        let entry = |name: &str, ms: Option<f32>, error: Option<&str>| BenchEntry {
            kind: BenchKind::Magpiefx,
            name: name.to_owned(),
            ms,
            p95: ms,
            frames: if ms.is_some() { 10 } else { 0 },
            error: error.map(str::to_owned),
        };
        let gpu = "1002:73bf:AMD Radeon RX 6800".to_owned();
        let first = BenchReport {
            gpu: gpu.clone(),
            entries: vec![
                entry("CAS", Some(0.123_456), None),
                entry("Bad", None, Some("boom")),
            ],
            cancelled: false,
        };
        let input = Size {
            width: 1280,
            height: 720,
        };
        let target = Size {
            width: 2560,
            height: 1440,
        };
        save_results(&path, &first, input, target, Some("rx 6800")).unwrap();
        let again = BenchReport {
            entries: vec![entry("CAS", Some(0.5), None)],
            ..first.clone()
        };
        save_results(&path, &again, input, target, Some("rx 6800")).unwrap();

        let mut read = results_for_gpu(&path, &gpu).unwrap();
        read.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].error.as_deref(), Some("boom"));
        assert_eq!(read[1].ms, Some(0.5));
        // By name and as the only GPU.
        assert_eq!(results_for_gpu(&path, "rx 6800").unwrap().len(), 2);
        assert_eq!(results_for_gpu(&path, "").unwrap().len(), 2);

        let data: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(data["version"], 1);
        assert_eq!(data["gpus"][&gpu]["name"], "AMD Radeon RX 6800");
        assert_eq!(data["gpus"][&gpu]["magpiefx"]["CAS"]["ms"], 0.5);
        assert_eq!(
            data["gpus"][&gpu]["sizes"]["magpiefx"]["input"],
            json!([1280, 720])
        );
        assert_eq!(
            data["gpus"][&gpu]["sizes"]["magpiefx"]["target"],
            json!([2560, 1440])
        );
        assert_eq!(data["selectors"]["rx 6800"], json!(gpu));

        // Other sizes: that kind starts over.
        let smaller = Size {
            width: 1920,
            height: 1080,
        };
        save_results(&path, &again, input, smaller, None).unwrap();
        assert_eq!(results_for_gpu(&path, &gpu).unwrap().len(), 1);
        let data: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(data["selectors"][""], json!(gpu));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
