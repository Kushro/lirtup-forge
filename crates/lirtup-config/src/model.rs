//! The typed configuration: [`Config`], the scaling part shared with modes
//! ([`ModeSettings`]) and the enums of its fields.
//!
//! Field names, YAML spellings and defaults are those of
//! `src/upscaler/config/models.py` (1.3.0). Durations keep the 1.x units
//! (seconds as `f64`, milliseconds or nanoseconds as integers) so the files are
//! read unchanged.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::color::BackgroundColor;
use crate::hotkeys::default_hotkeys;
use crate::{ConfigWarning, Mapping, Value};

// -------------------------------------------------------------------------
// Enums
// -------------------------------------------------------------------------

/// SRCNN model (CuNNy), from fastest to best.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Model {
    /// No neural pass: only the final resampler.
    #[serde(rename = "none")]
    None,
    /// CuNNy veryfast.
    #[serde(rename = "veryfast")]
    VeryFast,
    /// CuNNy faster.
    #[serde(rename = "faster")]
    Faster,
    /// CuNNy fast (default).
    #[default]
    #[serde(rename = "fast")]
    Fast,
    /// CuNNy 3x12.
    #[serde(rename = "3x12")]
    M3x12,
    /// CuNNy 4x12.
    #[serde(rename = "4x12")]
    M4x12,
    /// CuNNy 4x16.
    #[serde(rename = "4x16")]
    M4x16,
    /// CuNNy 4x24.
    #[serde(rename = "4x24")]
    M4x24,
    /// CuNNy 4x32.
    #[serde(rename = "4x32")]
    M4x32,
    /// CuNNy 8x32.
    #[serde(rename = "8x32")]
    M8x32,
}

impl Model {
    /// All models in the order of the GUI slider and of `cycle_model`.
    pub const ALL: [Model; 10] = [
        Model::None,
        Model::VeryFast,
        Model::Faster,
        Model::Fast,
        Model::M3x12,
        Model::M4x12,
        Model::M4x16,
        Model::M4x24,
        Model::M4x32,
        Model::M8x32,
    ];

    /// The YAML spelling (`"fast"`, `"4x12"`, ...), which is also the folder
    /// name under `srcnn/cunny/`.
    pub fn as_str(self) -> &'static str {
        match self {
            Model::None => "none",
            Model::VeryFast => "veryfast",
            Model::Faster => "faster",
            Model::Fast => "fast",
            Model::M3x12 => "3x12",
            Model::M4x12 => "4x12",
            Model::M4x16 => "4x16",
            Model::M4x24 => "4x24",
            Model::M4x32 => "4x32",
            Model::M8x32 => "8x32",
        }
    }
}

/// Final upsampling filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Upsampler {
    /// Lanczos-2 (default).
    #[default]
    Lanczos,
    /// AMD FSR 1.0 (EASU + RCAS).
    Fsr,
    /// NVIDIA Image Scaling.
    Nis,
}

/// Final downsampling filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Downsampler {
    /// Catmull-Rom (default).
    #[default]
    Catmull,
    /// Adaptive Lanczos.
    Lanczos,
}

/// Which engine runs the Magpie effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectsEngine {
    /// The hand-ported passes (Anime4K S, LumaSharpen, CRT-Easymode).
    #[default]
    Native,
    /// The MagpieFX chain of [`ModeSettings::magpie_effects`].
    Magpiefx,
}

/// Overlay window behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OverlayMode {
    /// Always on top, opaque, input forwarded to the target (default).
    #[default]
    #[serde(rename = "always-on-top")]
    AlwaysOnTop,
    /// Always on top and transparent to the mouse.
    #[serde(rename = "top-transparent")]
    TopTransparent,
    /// `_NET_WM_STATE_FULLSCREEN` on the chosen monitor.
    #[serde(rename = "fullscreen")]
    Fullscreen,
    /// A normal, movable window.
    #[serde(rename = "windowed")]
    Windowed,
}

/// Vulkan presentation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresentMode {
    /// `VK_PRESENT_MODE_FIFO_KHR` (default, vsync).
    #[default]
    Fifo,
    /// `VK_PRESENT_MODE_MAILBOX_KHR`.
    Mailbox,
    /// `VK_PRESENT_MODE_IMMEDIATE_KHR` (tearing).
    Immediate,
}

/// Frame generation backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameGenEngine {
    /// The lsfg-vk Vulkan layer, configured before the instance is created.
    #[default]
    Layer,
    /// lsfg built in (`lsfg-sys`), images shared by fd.
    Native,
}

/// Frame generation multiplier: `1` (off) to `5`, or `"auto"`.
///
/// `auto` only works with the native engine; it picks the multiplier from the
/// game's frame rate and the display refresh (`lirtup_fg::AutoMultiplier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "MultiplierRepr", into = "MultiplierRepr")]
pub enum FrameGenMultiplier {
    /// Fixed multiplier, 1 to 5 (1 = frame generation off).
    Fixed(u8),
    /// Chosen while running.
    Auto,
}

impl Default for FrameGenMultiplier {
    fn default() -> Self {
        FrameGenMultiplier::Fixed(1)
    }
}

impl FrameGenMultiplier {
    /// Whether frame generation is on (`Auto` or a fixed value above 1).
    pub fn is_on(self) -> bool {
        !matches!(self, FrameGenMultiplier::Fixed(0 | 1))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum MultiplierRepr {
    Number(u8),
    Text(String),
}

impl TryFrom<MultiplierRepr> for FrameGenMultiplier {
    type Error = String;

    fn try_from(value: MultiplierRepr) -> Result<Self, Self::Error> {
        match value {
            MultiplierRepr::Number(n @ 1..=5) => Ok(FrameGenMultiplier::Fixed(n)),
            MultiplierRepr::Number(n) => Err(format!("frame_gen_multiplier must be 1 to 5, got {n}")),
            MultiplierRepr::Text(s) if s.trim().eq_ignore_ascii_case("auto") => Ok(FrameGenMultiplier::Auto),
            MultiplierRepr::Text(s) => match s.trim().parse::<u8>() {
                Ok(n @ 1..=5) => Ok(FrameGenMultiplier::Fixed(n)),
                _ => Err(format!(
                    "frame_gen_multiplier must be 1 to 5 or \"auto\", got {s:?}"
                )),
            },
        }
    }
}

impl From<FrameGenMultiplier> for MultiplierRepr {
    fn from(value: FrameGenMultiplier) -> Self {
        match value {
            FrameGenMultiplier::Fixed(n) => MultiplierRepr::Number(n),
            FrameGenMultiplier::Auto => MultiplierRepr::Text("auto".into()),
        }
    }
}

/// Logging verbosity, as in the 1.x files (`DEBUG`, `INFO`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    /// Everything.
    #[serde(alias = "debug")]
    Debug,
    /// Default.
    #[default]
    #[serde(alias = "info")]
    Info,
    /// Warnings and errors.
    #[serde(alias = "warning", alias = "WARN", alias = "warn")]
    Warning,
    /// Errors only.
    #[serde(alias = "error")]
    Error,
}

/// Monitor to cover: `primary`, `all`, a RandR output name, or an index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MonitorSelector {
    /// Monitor index, as listed by `lirtup_x11::X11::monitors`.
    Index(u32),
    /// `primary`, `all`, or an output name (`DP-1`, ...).
    Name(String),
}

impl Default for MonitorSelector {
    fn default() -> Self {
        MonitorSelector::Name("primary".into())
    }
}

/// GPU to use: an index into the adapter list or a name substring.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GpuSelector {
    /// Index into `lirtup_gpu::Instance::adapters`.
    Index(u32),
    /// Case-insensitive substring of the adapter name.
    Name(String),
}

/// Output geometry string, kept as written (`fit`, `stretch`, `cover`,
/// `150%`, `1920x`, `x1080`, `1920x1080!`, ...); [`OutputGeometry::parse`]
/// turns it into a [`Geometry`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OutputGeometry(pub String);

impl Default for OutputGeometry {
    fn default() -> Self {
        OutputGeometry("fit".into())
    }
}

/// Parsed form of [`OutputGeometry`] (the grammar of `validate_geometry` in
/// `config/validators.py`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Geometry {
    /// Keep the aspect ratio, fit inside the monitor.
    Fit,
    /// Fill the monitor, ignoring the aspect ratio.
    Stretch,
    /// Keep the aspect ratio, cover the monitor (crops).
    Cover,
    /// Scale by a percentage; `force` is the trailing `!`.
    Percent {
        /// Percentage, 100 = source size.
        percent: f32,
        /// Ignore the monitor bounds.
        force: bool,
    },
    /// Fixed width (`1920x`).
    Width {
        /// Width in pixels.
        width: u32,
        /// Trailing `!`.
        force: bool,
    },
    /// Fixed height (`x1080`).
    Height {
        /// Height in pixels.
        height: u32,
        /// Trailing `!`.
        force: bool,
    },
    /// Fixed size (`1920x1080`, with `!` to ignore the aspect ratio or `^` to
    /// cover).
    Size {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// Trailing `!` or `^`.
        flag: Option<char>,
    },
}

impl OutputGeometry {
    /// Parse the geometry string.
    pub fn parse(&self) -> Result<Geometry, crate::ConfigError> {
        let s = self.0.trim();
        let invalid = || crate::ConfigError::Invalid {
            key: "output_geometry".into(),
            message: format!(
                "invalid geometry {s:?} (allowed: stretch, fit, cover, 50%, 50%!, 1920x, 1920x!, \
                 x1080, x1080!, 1920x1080, 1920x1080!, 1920x1080^)"
            ),
        };
        let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
        let int = |t: &str| digits(t).then(|| t.parse::<u32>().ok()).flatten();
        match s {
            "fit" => return Ok(Geometry::Fit),
            "stretch" => return Ok(Geometry::Stretch),
            "cover" => return Ok(Geometry::Cover),
            _ => {}
        }
        let (body, bang) = match s.strip_suffix('!') {
            Some(body) => (body, true),
            None => (s, false),
        };
        if let Some(pct) = body.strip_suffix('%') {
            let (whole, frac) = pct.split_once('.').unwrap_or((pct, "0"));
            if digits(whole) && digits(frac) {
                let percent = pct.parse::<f32>().map_err(|_| invalid())?;
                return Ok(Geometry::Percent { percent, force: bang });
            }
            return Err(invalid());
        }
        if let Some(height) = body.strip_prefix('x') {
            let height = int(height).ok_or_else(invalid)?;
            return Ok(Geometry::Height { height, force: bang });
        }
        if let Some(width) = body.strip_suffix('x') {
            let width = int(width).ok_or_else(invalid)?;
            return Ok(Geometry::Width { width, force: bang });
        }
        let (body, flag) = match s.strip_suffix(['!', '^']) {
            Some(body) => (body, s.chars().last()),
            None => (s, None),
        };
        let (w, h) = body.split_once('x').ok_or_else(invalid)?;
        Ok(Geometry::Size {
            width: int(w).ok_or_else(invalid)?,
            height: int(h).ok_or_else(invalid)?,
            flag,
        })
    }
}

/// Pixels cropped from each side of the captured window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Crop {
    /// From the top.
    pub top: u32,
    /// From the bottom.
    pub bottom: u32,
    /// From the left.
    pub left: u32,
    /// From the right.
    pub right: u32,
}

// -------------------------------------------------------------------------
// Scaling mode settings
// -------------------------------------------------------------------------

/// Effect in [`ModeSettings::magpie_effects`]: `"Name"`,
/// `"Name:param=value,..."`, a path to an `.hlsl`, or
/// `{name: ..., params: {...}}`. `lirtup_magpiefx::spec` parses the short form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MagpieEffectSpec {
    /// Short form.
    Short(String),
    /// Long form.
    Full {
        /// Effect name or `.hlsl` path.
        name: String,
        /// Parameter overrides.
        #[serde(default)]
        params: BTreeMap<String, f64>,
    },
}

/// Everything a scaling mode holds (6.6): model, samplers, effects engine and
/// chain, final touch-up. Flattened into [`Config`], and the body of
/// `modes/<id>.yaml` ([`crate::Mode`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModeSettings {
    /// SRCNN model.
    pub model: Model,
    /// Chain two 2x passes for 4x.
    pub double_upscale: bool,

    /// Final upsampling filter.
    pub upsampler: Upsampler,
    /// Final downsampling filter.
    pub downsampler: Downsampler,
    /// Kernel width of the final resampling.
    pub blur: f32,
    /// Anti-ringing strength.
    pub antiring_strength: f32,
    /// Tight anti-ringing.
    pub tight_antiring: bool,
    /// Override of the automatic Lanczos radius.
    pub kernel_radius: Option<u32>,

    /// Debanding before scaling.
    pub deband_enabled: bool,
    /// Debanding intensity.
    pub deband_strength: f32,

    /// Engine for the Magpie effects; only the selected one runs.
    pub effects_engine: EffectsEngine,
    /// MagpieFX chain, run on the frame before scaling.
    pub magpie_effects: Vec<MagpieEffectSpec>,
    /// Log the GPU time of the MagpieFX chain.
    pub magpie_profile: bool,

    /// Contrast Adaptive Sharpening.
    pub cas_enabled: bool,
    /// CAS intensity.
    pub cas_strength: f32,

    /// Bloom.
    pub bloom_enabled: bool,
    /// Bloom intensity.
    pub bloom_strength: f32,
    /// Brightness threshold for bloom.
    pub bloom_threshold: f32,
    /// Bloom blur radius in pixels.
    pub bloom_radius: u32,

    /// Vignette.
    pub vignette_enabled: bool,
    /// Vignette intensity.
    pub vignette_strength: f32,
    /// Distance from the center where the vignette starts.
    pub vignette_radius: f32,
    /// Softness of the vignette transition.
    pub vignette_falloff: f32,

    /// 3D color LUT.
    pub lut_enabled: bool,
    /// Blend between the original and the graded image.
    pub lut_intensity: f32,
    /// Built-in LUT preset.
    pub lut_preset: String,

    /// Film grain.
    pub grain_enabled: bool,
    /// Grain intensity.
    pub grain_strength: f32,
    /// Apparent grain size.
    pub grain_size: f32,

    /// Anime4K_Upscale_S (2x) before scaling (native engine).
    pub anime4k_enabled: bool,

    /// LumaSharpen (native engine).
    pub luma_sharpen_enabled: bool,
    /// LumaSharpen strength.
    pub luma_sharpen_strength: f32,
    /// Maximum sharpening per pixel.
    pub luma_sharpen_clamp: f32,
    /// Sample pattern, 0 to 3.
    pub luma_sharpen_pattern: u32,
    /// Sample pattern radius.
    pub luma_sharpen_offset_bias: f32,

    /// CRT-Easymode (native engine; replaces the final scaler).
    pub crt_enabled: bool,
    /// See Magpie's CRT_Easymode.
    pub crt_sharpness_h: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_sharpness_v: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_mask_strength: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_mask_dot_width: u32,
    /// See Magpie's CRT_Easymode.
    pub crt_mask_dot_height: u32,
    /// See Magpie's CRT_Easymode.
    pub crt_mask_stagger: u32,
    /// See Magpie's CRT_Easymode.
    pub crt_mask_size: u32,
    /// See Magpie's CRT_Easymode.
    pub crt_scanline_strength: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_scanline_beam_width_min: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_scanline_beam_width_max: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_scanline_bright_min: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_scanline_bright_max: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_scanline_cutoff: u32,
    /// See Magpie's CRT_Easymode.
    pub crt_gamma_input: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_gamma_output: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_bright_boost: f32,
    /// See Magpie's CRT_Easymode.
    pub crt_dilation: u32,
}

/// YAML keys that belong to a mode ([`ModeSettings`]), in declaration order.
/// Used by the resolver (loose mode fields win over the mode) and by the
/// migration of round 4.
pub const MODE_KEYS: &[&str] = &[
    "model",
    "double_upscale",
    "upsampler",
    "downsampler",
    "blur",
    "antiring_strength",
    "tight_antiring",
    "kernel_radius",
    "deband_enabled",
    "deband_strength",
    "effects_engine",
    "magpie_effects",
    "magpie_profile",
    "cas_enabled",
    "cas_strength",
    "bloom_enabled",
    "bloom_strength",
    "bloom_threshold",
    "bloom_radius",
    "vignette_enabled",
    "vignette_strength",
    "vignette_radius",
    "vignette_falloff",
    "lut_enabled",
    "lut_intensity",
    "lut_preset",
    "grain_enabled",
    "grain_strength",
    "grain_size",
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

impl Default for ModeSettings {
    fn default() -> Self {
        ModeSettings {
            model: Model::Fast,
            double_upscale: false,
            upsampler: Upsampler::Lanczos,
            downsampler: Downsampler::Catmull,
            blur: 1.0,
            antiring_strength: 0.8,
            tight_antiring: true,
            kernel_radius: None,
            deband_enabled: false,
            deband_strength: 0.3,
            effects_engine: EffectsEngine::Native,
            magpie_effects: Vec::new(),
            magpie_profile: false,
            cas_enabled: false,
            cas_strength: 0.4,
            bloom_enabled: false,
            bloom_strength: 0.03,
            bloom_threshold: 0.85,
            bloom_radius: 4,
            vignette_enabled: false,
            vignette_strength: 0.5,
            vignette_radius: 0.3,
            vignette_falloff: 2.0,
            lut_enabled: false,
            lut_intensity: 1.0,
            lut_preset: "identity".into(),
            grain_enabled: false,
            grain_strength: 0.15,
            grain_size: 1.0,
            anime4k_enabled: false,
            luma_sharpen_enabled: false,
            luma_sharpen_strength: 0.65,
            luma_sharpen_clamp: 0.035,
            luma_sharpen_pattern: 1,
            luma_sharpen_offset_bias: 1.0,
            crt_enabled: false,
            crt_sharpness_h: 0.5,
            crt_sharpness_v: 1.0,
            crt_mask_strength: 0.3,
            crt_mask_dot_width: 1,
            crt_mask_dot_height: 1,
            crt_mask_stagger: 0,
            crt_mask_size: 1,
            crt_scanline_strength: 1.0,
            crt_scanline_beam_width_min: 1.5,
            crt_scanline_beam_width_max: 1.5,
            crt_scanline_bright_min: 0.35,
            crt_scanline_bright_max: 0.65,
            crt_scanline_cutoff: 400,
            crt_gamma_input: 2.0,
            crt_gamma_output: 1.8,
            crt_bright_boost: 1.2,
            crt_dilation: 1,
        }
    }
}

// -------------------------------------------------------------------------
// Config
// -------------------------------------------------------------------------

/// The resolved configuration of one scaling session.
///
/// Same fields as the 1.x `Config` dataclass (plus [`Config::mode`]); the
/// scaling part is flattened from [`ModeSettings`], so `config.scaling.model`
/// is the YAML key `model`. Built by [`crate::resolve::resolve`]; published
/// inside `lirtup serve` as an `Arc<Config>` (`arc-swap`) and never mutated
/// in place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // Program
    /// Command and arguments to launch before scaling.
    pub program: Option<Vec<String>>,

    // Target selection
    /// Pick a window from a list (CLI).
    pub select: bool,
    /// Daemon mode: start on windows that match a profile.
    pub daemon: bool,
    /// Exclude this profile from daemon matching.
    pub daemon_exclude: bool,
    /// Substring of the target window title.
    pub target_title: Option<String>,
    /// Regex on the target window title.
    pub target_title_regex: Option<String>,

    // Focus tracking
    /// Follow the focused window.
    pub follow_focus: bool,
    /// Hide the overlay while the target is not focused.
    pub pause_on_focus_loss: bool,

    // Timing (seconds)
    /// Seconds between focus checks.
    pub focus_poll_interval: f64,
    /// Seconds between daemon window scans.
    pub daemon_poll_interval: f64,
    /// Seconds between pipeline idle checks.
    pub pipeline_poll_interval: f64,

    // Window detection (seconds)
    /// Seconds to wait before taking the active window.
    pub target_delay: f64,
    /// Seconds of the PID detection phase.
    pub pid_timeout: f64,
    /// Seconds of the WM_CLASS detection phase.
    pub class_timeout: f64,
    /// Maximum seconds to wait for a window.
    pub total_timeout: f64,
    /// Detection phase to try first (1 = PID, 2 = class).
    pub starting_phase: u8,

    /// Scaling mode named by this layer (`modes/<id>.yaml`), new in 2.0.
    pub mode: Option<String>,

    /// Model, samplers, effects (the part a mode holds).
    #[serde(flatten)]
    pub scaling: ModeSettings,

    // GPU and display
    /// GPU selection.
    pub gpu: Option<GpuSelector>,
    /// Monitor to cover.
    pub monitor: MonitorSelector,
    /// Manual scale factor.
    pub scale_factor: Option<f64>,

    // Presentation
    /// Output sizing.
    pub output_geometry: OutputGeometry,
    /// Pixels cropped from the top.
    pub crop_top: u32,
    /// Pixels cropped from the bottom.
    pub crop_bottom: u32,
    /// Pixels cropped from the left.
    pub crop_left: u32,
    /// Pixels cropped from the right.
    pub crop_right: u32,
    /// Detect and crop a Wine menu bar (experimental).
    pub auto_crop_menu_bar: bool,
    /// Color of the letterbox bars.
    pub background_color: BackgroundColor,
    /// Horizontal content offset in pixels.
    pub offset_x: i32,
    /// Vertical content offset in pixels.
    pub offset_y: i32,

    // Overlay
    /// Overlay window behavior.
    pub overlay_mode: OverlayMode,
    /// Milliseconds of inactivity before hiding the cursor.
    pub hide_cursor: Option<u32>,
    /// Minimum overlay opacity.
    pub overlay_opacity_min: f32,
    /// Maximum overlay opacity.
    pub overlay_opacity_max: f32,

    // Screenshots
    /// Folder for screenshots (`~` is expanded when used).
    pub screenshot_dir: PathBuf,
    /// Filename template (`{timestamp:%Y%m%d_%H%M%S}`).
    pub screenshot_filename: String,

    // OSD and stats
    /// Show on-screen messages.
    pub show_osd: bool,
    /// Seconds an OSD message stays.
    pub osd_duration: f32,
    /// Start with the stats panel visible.
    pub show_stats: bool,
    /// CSV with the latency of every presented frame.
    pub latency_log: Option<PathBuf>,

    // Vulkan
    /// Maximum pipeline frames per second.
    pub max_fps: Option<u32>,
    /// Skip presents while the target has no new frame.
    pub present_on_change_only: bool,

    // Frame generation (read once at startup)
    /// Frames per captured frame, or auto.
    pub frame_gen_multiplier: FrameGenMultiplier,
    /// Optical flow resolution, 0.25 to 1.0.
    pub frame_gen_flow_scale: f32,
    /// Lighter lsfg model.
    pub frame_gen_performance: bool,
    /// Folder with `lsfg-vk.dll`, or `Lossless.dll` for the native engine
    /// (`None` = search Steam).
    pub frame_gen_dll: Option<PathBuf>,
    /// Layer or native.
    pub frame_gen_engine: FrameGenEngine,
    /// Vulkan presentation mode.
    pub vulkan_present_mode: PresentMode,
    /// GPU fence timeout in nanoseconds.
    pub frame_timeout: u64,

    // Tiles
    /// Tile-based processing.
    pub use_tile_processing: bool,
    /// Transfer only damaged regions.
    pub use_damage_tracking: bool,
    /// Tile interior size in pixels.
    pub tile_size: u32,
    /// Border pixels added to each tile.
    pub tile_context_margin: u32,
    /// Maximum tiles per frame.
    pub max_tile_layers: u32,
    /// Dirty area fraction that forces a full frame.
    pub area_threshold: f32,

    // Errors
    /// Consecutive capture failures before stopping.
    pub max_capture_failures: u32,
    /// Seconds to wait after a capture failure.
    pub capture_failure_delay: f64,
    /// Minimum seconds between swapchain recreations.
    pub swapchain_debounce: f64,

    // Logging
    /// Log verbosity.
    pub log_level: LogLevel,
    /// Log file.
    pub log_file: Option<PathBuf>,

    /// Path of the loaded file (internal, never written).
    #[serde(skip)]
    pub config_file: Option<PathBuf>,

    /// Action name → hotkey string (`"Alt+Shift+S"`). Kept as strings so
    /// unknown actions survive; [`Config::hotkey_bindings`] parses them.
    pub hotkeys: BTreeMap<String, String>,

    /// Keys this version does not know, kept for saving.
    #[serde(flatten)]
    pub extra: Mapping,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            program: None,
            select: false,
            daemon: false,
            daemon_exclude: false,
            target_title: None,
            target_title_regex: None,
            follow_focus: false,
            pause_on_focus_loss: true,
            focus_poll_interval: 0.2,
            daemon_poll_interval: 2.0,
            pipeline_poll_interval: 0.1,
            target_delay: 5.0,
            pid_timeout: 5.0,
            class_timeout: 5.0,
            total_timeout: 60.0,
            starting_phase: 1,
            mode: None,
            scaling: ModeSettings::default(),
            gpu: None,
            monitor: MonitorSelector::default(),
            scale_factor: None,
            output_geometry: OutputGeometry::default(),
            crop_top: 0,
            crop_bottom: 0,
            crop_left: 0,
            crop_right: 0,
            auto_crop_menu_bar: false,
            background_color: BackgroundColor::default(),
            offset_x: 0,
            offset_y: 0,
            overlay_mode: OverlayMode::AlwaysOnTop,
            hide_cursor: None,
            overlay_opacity_min: 0.2,
            overlay_opacity_max: 1.0,
            screenshot_dir: crate::paths::default_screenshot_dir(),
            screenshot_filename: "Screenshot_{timestamp:%Y%m%d_%H%M%S}.png".into(),
            show_osd: true,
            osd_duration: 1.5,
            show_stats: false,
            latency_log: None,
            max_fps: None,
            present_on_change_only: false,
            frame_gen_multiplier: FrameGenMultiplier::Fixed(1),
            frame_gen_flow_scale: 1.0,
            frame_gen_performance: false,
            frame_gen_dll: None,
            frame_gen_engine: FrameGenEngine::Layer,
            vulkan_present_mode: PresentMode::Fifo,
            frame_timeout: 1_000_000_000,
            use_tile_processing: true,
            use_damage_tracking: true,
            tile_size: 64,
            tile_context_margin: 16,
            max_tile_layers: 16,
            area_threshold: 0.3,
            max_capture_failures: 10,
            capture_failure_delay: 0.05,
            swapchain_debounce: 1.0,
            log_level: LogLevel::Info,
            log_file: None,
            config_file: None,
            hotkeys: default_hotkeys(),
            extra: Mapping::new(),
        }
    }
}

impl Config {
    /// Build a config from one merged mapping (all layers already applied).
    ///
    /// Tolerant like the 1.x validators: a key with a bad value keeps its
    /// default and yields a [`ConfigWarning`]; the rest of the file still
    /// loads. Unknown keys go to [`Config::extra`].
    pub fn from_mapping(map: &Mapping) -> (Config, Vec<ConfigWarning>) {
        let mut map = map.clone();
        let hotkeys = map.remove("hotkeys");
        // Internal, never read from a file.
        map.remove("config_file");
        let (mut config, mut warnings) = crate::util::tolerant::<Config>(&map, "");
        config.hotkeys = default_hotkeys();
        match hotkeys {
            None | Some(Value::Null) => {}
            Some(Value::Mapping(entries)) => {
                for (action, binding) in entries {
                    let action = crate::util::key_string(&action);
                    match binding {
                        Value::String(binding) => {
                            config.hotkeys.insert(action, binding);
                        }
                        // `null` unbinds, as an empty string does.
                        Value::Null => {
                            config.hotkeys.insert(action, String::new());
                        }
                        other => warnings.push(ConfigWarning {
                            key: format!("hotkeys.{action}"),
                            message: format!(
                                "invalid value {}, using the default",
                                crate::util::display(&other)
                            ),
                        }),
                    }
                }
            }
            Some(other) => warnings.push(ConfigWarning {
                key: "hotkeys".into(),
                message: format!(
                    "expected a mapping, found {}; using the defaults",
                    crate::util::display(&other)
                ),
            }),
        }
        (config, warnings)
    }

    /// The keys that differ from [`Config::default`], as `to_dict(diff_only=True)`
    /// does: skips `program`, `log_level`, `log_file`, `config_file`; colors as
    /// hex; `hotkeys` only with the bindings that differ.
    pub fn to_diff_mapping(&self) -> Mapping {
        const SKIPPED: [&str; 4] = ["program", "log_level", "log_file", "hotkeys"];
        let current = crate::util::to_mapping(self);
        let defaults = crate::util::to_mapping(&Config::default());
        let mut diff = Mapping::new();
        for (key, value) in current {
            let name = key.as_str().unwrap_or_default();
            if SKIPPED.contains(&name) {
                continue;
            }
            if name == "background_color" {
                if self.background_color.to_rgba() != Config::default().background_color.to_rgba() {
                    diff.insert(key, Value::String(self.background_color.to_hex()));
                }
                continue;
            }
            if defaults.get(&key) != Some(&value) {
                diff.insert(key, value);
            }
        }
        let hotkeys = crate::hotkeys::diff_hotkeys(&self.hotkeys);
        if !hotkeys.is_empty() {
            let map = hotkeys
                .into_iter()
                .map(|(k, v)| (Value::String(k), Value::String(v)))
                .collect();
            diff.insert("hotkeys".into(), Value::Mapping(map));
        }
        diff
    }

    /// The crop of the four `crop_*` fields.
    pub fn crop(&self) -> Crop {
        Crop {
            top: self.crop_top,
            bottom: self.crop_bottom,
            left: self.crop_left,
            right: self.crop_right,
        }
    }

    /// Parsed hotkeys. Unknown actions and unparsable strings yield warnings
    /// and are skipped.
    pub fn hotkey_bindings(&self) -> (Vec<(crate::HotkeyAction, crate::Hotkey)>, Vec<ConfigWarning>) {
        let mut bindings = Vec::new();
        let mut warnings = Vec::new();
        for (name, binding) in &self.hotkeys {
            let key = format!("hotkeys.{name}");
            let Ok(action) = name.parse::<crate::HotkeyAction>() else {
                warnings.push(ConfigWarning {
                    key,
                    message: "unknown hotkey action, ignored".into(),
                });
                continue;
            };
            if binding.trim().is_empty() {
                continue;
            }
            match binding.parse::<crate::Hotkey>() {
                Ok(hotkey) => bindings.push((action, hotkey)),
                Err(e) => warnings.push(ConfigWarning {
                    key,
                    message: format!("{e}, ignored"),
                }),
            }
        }
        bindings.sort_by_key(|(action, _)| crate::HotkeyAction::ALL.iter().position(|a| a == action));
        (bindings, warnings)
    }

    /// Check value ranges (the 1.x `validators.py`), clamping where 1.x
    /// clamps.
    pub fn validate(&mut self) -> Vec<ConfigWarning> {
        let defaults = Config::default();
        let mut warnings = Vec::new();

        // A value outside `lo..=hi` (or the exclusive form) goes back to its
        // default; 1.x refused to start instead.
        macro_rules! range {
            ($($path:ident).+, $lo:expr, $hi:expr) => {
                range!(@check $($path).+, |v: f64| v >= $lo as f64 && v <= $hi as f64,
                    format!("must be {} to {}", $lo, $hi))
            };
            ($($path:ident).+, min $lo:expr) => {
                range!(@check $($path).+, |v: f64| v >= $lo as f64, format!("must be >= {}", $lo))
            };
            ($($path:ident).+, above $lo:expr) => {
                range!(@check $($path).+, |v: f64| v > $lo as f64, format!("must be > {}", $lo))
            };
            ($($path:ident).+, above $lo:expr, $hi:expr) => {
                range!(@check $($path).+, |v: f64| v > $lo as f64 && v <= $hi as f64,
                    format!("must be > {} and <= {}", $lo, $hi))
            };
            (@check $($path:ident).+, $ok:expr, $what:expr) => {{
                let ok = $ok;
                if !ok(self.$($path).+ as f64) {
                    let key = stringify!($($path).+).replace(" ", "").trim_start_matches("scaling.").to_owned();
                    warnings.push(ConfigWarning {
                        key,
                        message: format!("{}, got {}; using the default {}", $what, self.$($path).+,
                            defaults.$($path).+),
                    });
                    self.$($path).+ = defaults.$($path).+;
                }
            }};
        }
        macro_rules! opt_range {
            ($field:ident, $ok:expr, $what:expr) => {
                if let Some(v) = self.$field {
                    let ok = $ok;
                    if !ok(v as f64) {
                        warnings.push(ConfigWarning {
                            key: stringify!($field).into(),
                            message: format!("{}, got {v}; ignored", $what),
                        });
                        self.$field = None;
                    }
                }
            };
        }

        // Timing and window detection
        range!(daemon_poll_interval, min 0.1);
        range!(focus_poll_interval, min 0.01);
        range!(pipeline_poll_interval, min 0.01);
        range!(target_delay, min 0);
        range!(pid_timeout, min 0);
        range!(class_timeout, min 0);
        range!(total_timeout, min 0);
        range!(starting_phase, 1, 2);
        // Sampler
        range!(scaling.blur, above 0, 2);
        range!(scaling.antiring_strength, 0, 1);
        if let Some(r) = self.scaling.kernel_radius {
            if !(2..=10).contains(&r) {
                warnings.push(ConfigWarning {
                    key: "kernel_radius".into(),
                    message: format!("must be 2 to 10, got {r}; using the automatic radius"),
                });
                self.scaling.kernel_radius = None;
            }
        }
        // Effects
        range!(scaling.deband_strength, 0, 1);
        range!(scaling.cas_strength, 0, 1);
        range!(scaling.bloom_strength, 0, 1);
        range!(scaling.bloom_threshold, 0, 1);
        range!(scaling.bloom_radius, 1, 16);
        range!(scaling.vignette_strength, 0, 1);
        range!(scaling.vignette_radius, 0, 2);
        range!(scaling.vignette_falloff, 0.1, 10);
        range!(scaling.grain_strength, 0, 1);
        range!(scaling.grain_size, 1, 10);
        range!(scaling.lut_intensity, 0, 1);
        range!(scaling.luma_sharpen_strength, 0, 3);
        range!(scaling.luma_sharpen_clamp, 0, 1);
        range!(scaling.luma_sharpen_pattern, 0, 3);
        range!(scaling.luma_sharpen_offset_bias, 0, 6);
        range!(scaling.crt_sharpness_h, 0, 1);
        range!(scaling.crt_sharpness_v, 0, 1);
        range!(scaling.crt_mask_strength, 0, 1);
        range!(scaling.crt_mask_dot_width, 1, 100);
        range!(scaling.crt_mask_dot_height, 1, 100);
        range!(scaling.crt_mask_stagger, 0, 100);
        range!(scaling.crt_mask_size, 1, 100);
        range!(scaling.crt_scanline_strength, 0, 1);
        range!(scaling.crt_scanline_beam_width_min, 0.5, 5);
        range!(scaling.crt_scanline_beam_width_max, 0.5, 5);
        range!(scaling.crt_scanline_bright_min, 0, 1);
        range!(scaling.crt_scanline_bright_max, 0, 1);
        range!(scaling.crt_scanline_cutoff, 1, 1000);
        range!(scaling.crt_gamma_input, 0.1, 5);
        range!(scaling.crt_gamma_output, 0.1, 5);
        range!(scaling.crt_bright_boost, 1, 2);
        range!(scaling.crt_dilation, 0, 1);
        // Display and overlay
        opt_range!(scale_factor, |v: f64| v > 0.0, "must be > 0");
        opt_range!(max_fps, |v: f64| v >= 1.0, "must be >= 1");
        range!(overlay_opacity_min, 0, 1);
        range!(overlay_opacity_max, 0, 1);
        range!(osd_duration, min 0);
        // Frame generation
        range!(frame_gen_flow_scale, 0.25, 1);
        // Tiles and errors
        range!(tile_size, min 1);
        range!(area_threshold, 0, 1);
        range!(max_capture_failures, min 1);
        range!(capture_failure_delay, min 0);
        range!(swapchain_debounce, min 0);

        if let FrameGenMultiplier::Fixed(n) = self.frame_gen_multiplier {
            if !(1..=5).contains(&n) {
                warnings.push(ConfigWarning {
                    key: "frame_gen_multiplier".into(),
                    message: format!("must be 1 to 5 or \"auto\", got {n}; frame generation is off"),
                });
                self.frame_gen_multiplier = FrameGenMultiplier::Fixed(1);
            }
        }
        if let Err(e) = self.output_geometry.parse() {
            warnings.push(ConfigWarning {
                key: "output_geometry".into(),
                message: format!("{e}; using \"fit\""),
            });
            self.output_geometry = defaults.output_geometry.clone();
        }
        if self.background_color.try_rgba().is_none() {
            warnings.push(ConfigWarning {
                key: "background_color".into(),
                message: format!("invalid color {:?}; using black", self.background_color),
            });
            self.background_color = defaults.background_color.clone();
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(yaml: &str) -> Mapping {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    fn geometry(s: &str) -> Result<Geometry, crate::ConfigError> {
        OutputGeometry(s.into()).parse()
    }

    #[test]
    fn geometry_grammar() {
        assert_eq!(geometry("fit").unwrap(), Geometry::Fit);
        assert_eq!(geometry(" cover ").unwrap(), Geometry::Cover);
        assert_eq!(
            geometry("150%").unwrap(),
            Geometry::Percent {
                percent: 150.0,
                force: false
            }
        );
        assert_eq!(
            geometry("62.5%!").unwrap(),
            Geometry::Percent {
                percent: 62.5,
                force: true
            }
        );
        assert_eq!(
            geometry("1920x").unwrap(),
            Geometry::Width {
                width: 1920,
                force: false
            }
        );
        assert_eq!(
            geometry("x1080!").unwrap(),
            Geometry::Height {
                height: 1080,
                force: true
            }
        );
        assert_eq!(
            geometry("1920x1080^").unwrap(),
            Geometry::Size {
                width: 1920,
                height: 1080,
                flag: Some('^')
            }
        );
        assert_eq!(
            geometry("1920x1080").unwrap(),
            Geometry::Size {
                width: 1920,
                height: 1080,
                flag: None
            }
        );
        for bad in ["", "big", "x", "1920x1080x", "%", "1920^", "-5%", "1920 x 1080"] {
            assert!(geometry(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn empty_mapping_is_default() {
        let (config, warnings) = Config::from_mapping(&Mapping::new());
        assert!(warnings.is_empty());
        assert_eq!(config, Config::default());
    }

    #[test]
    fn mode_keys_are_flattened_not_extra() {
        let (config, warnings) =
            Config::from_mapping(&map("model: 4x12\ncas_enabled: true\nfuture_key: 3\n"));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.scaling.model, Model::M4x12);
        assert!(config.scaling.cas_enabled);
        assert_eq!(config.extra.len(), 1);
        assert!(config.extra.contains_key("future_key"));
        for key in MODE_KEYS {
            assert!(!config.extra.contains_key(*key));
        }
    }

    #[test]
    fn mode_keys_match_mode_settings() {
        let map = crate::util::to_mapping(&ModeSettings::default());
        let keys: Vec<&str> = map.keys().filter_map(Value::as_str).collect();
        assert_eq!(keys, MODE_KEYS);
    }

    #[test]
    fn bad_values_keep_defaults() {
        let (config, warnings) = Config::from_mapping(&map(
            "model: huge\ntile_size: -3\nshow_stats: true\nmax_fps: null\n",
        ));
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(config.scaling.model, Model::Fast);
        assert_eq!(config.tile_size, 64);
        assert!(config.show_stats);
    }

    #[test]
    fn enum_spellings() {
        let (c, w) = Config::from_mapping(&map(
            "overlay_mode: top-transparent\nlog_level: WARNING\nframe_gen_multiplier: auto\n\
             vulkan_present_mode: mailbox\ngpu: 1\nmonitor: DP-1\nframe_gen_engine: native\n",
        ));
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(c.overlay_mode, OverlayMode::TopTransparent);
        assert_eq!(c.log_level, LogLevel::Warning);
        assert_eq!(c.frame_gen_multiplier, FrameGenMultiplier::Auto);
        assert_eq!(c.vulkan_present_mode, PresentMode::Mailbox);
        assert_eq!(c.gpu, Some(GpuSelector::Index(1)));
        assert_eq!(c.monitor, MonitorSelector::Name("DP-1".into()));
        assert_eq!(c.frame_gen_engine, FrameGenEngine::Native);
        let (c, _) = Config::from_mapping(&map("frame_gen_multiplier: 6\n"));
        assert_eq!(c.frame_gen_multiplier, FrameGenMultiplier::Fixed(1));
    }

    #[test]
    fn magpie_effects_both_forms() {
        let (c, w) = Config::from_mapping(&map(
            "magpie_effects:\n- name: Lanczos\n  params: {}\n- \"CAS:sharpness=0.5\"\n",
        ));
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(c.scaling.magpie_effects.len(), 2);
        assert!(
            matches!(&c.scaling.magpie_effects[0], MagpieEffectSpec::Full { name, .. } if name == "Lanczos")
        );
    }

    #[test]
    fn hotkeys_merge_over_defaults() {
        let (c, w) = Config::from_mapping(&map("hotkeys:\n  zoom_in: Ctrl+Plus\n  zoom_out: null\n"));
        assert!(w.is_empty());
        assert_eq!(c.hotkeys["zoom_in"], "Ctrl+Plus");
        assert_eq!(c.hotkeys["zoom_out"], "");
        assert_eq!(c.hotkeys["toggle_scaling"], "Alt+Shift+S");
        let (bindings, warnings) = c.hotkey_bindings();
        assert!(warnings.is_empty());
        assert_eq!(bindings.len(), crate::HotkeyAction::ALL.len() - 1);
        assert_eq!(bindings[0].0, crate::HotkeyAction::StartFullscreen);
    }

    #[test]
    fn diff_mapping() {
        assert!(Config::default().to_diff_mapping().is_empty());
        let mut c = Config::default();
        c.scaling.bloom_strength = 0.05;
        c.scaling.model = Model::M8x32;
        c.background_color = BackgroundColor::Css("red".into());
        c.log_level = LogLevel::Debug;
        c.hotkeys.insert("zoom_in".into(), "Ctrl+Plus".into());
        let diff = c.to_diff_mapping();
        assert_eq!(diff.len(), 4, "{diff:?}");
        assert_eq!(diff["model"], Value::from("8x32"));
        assert_eq!(
            serde_yaml_ng::to_string(&diff["bloom_strength"]).unwrap().trim(),
            "0.05"
        );
        assert_eq!(diff["background_color"], Value::from("#ff0000"));
        assert_eq!(diff["hotkeys"]["zoom_in"], Value::from("Ctrl+Plus"));
        // And it reads back to the same config.
        let (back, w) = Config::from_mapping(&diff);
        assert!(w.is_empty());
        assert_eq!(back.scaling, c.scaling);
        assert_eq!(back.hotkeys, c.hotkeys);
    }

    #[test]
    fn black_variants_are_not_a_diff() {
        let mut c = Config::default();
        c.background_color = BackgroundColor::Css("#000".into());
        assert!(c.to_diff_mapping().is_empty());
    }

    #[test]
    fn validate_resets_out_of_range() {
        let mut c = Config::default();
        c.scaling.blur = 0.0;
        c.scaling.kernel_radius = Some(20);
        c.frame_gen_flow_scale = 2.0;
        c.output_geometry = OutputGeometry("huge".into());
        c.starting_phase = 3;
        let w = c.validate();
        assert_eq!(w.len(), 5, "{w:?}");
        assert_eq!(c.scaling.blur, 1.0);
        assert_eq!(c.scaling.kernel_radius, None);
        assert_eq!(c.frame_gen_flow_scale, 1.0);
        assert_eq!(c.output_geometry.0, "fit");
        assert_eq!(w[0].key, "starting_phase");
        assert!(Config::default().validate().is_empty());
    }
}
