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
use crate::{ConfigWarning, Mapping};

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
        todo!("front C: port validate_geometry")
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
        todo!("front C")
    }

    /// The keys that differ from [`Config::default`], as `to_dict(diff_only=True)`
    /// does: skips `program`, `log_level`, `log_file`, `config_file`; colors as
    /// hex; `hotkeys` only with the bindings that differ.
    pub fn to_diff_mapping(&self) -> Mapping {
        todo!("front C")
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
        todo!("front C")
    }

    /// Check value ranges (the 1.x `validators.py`), clamping where 1.x
    /// clamps.
    pub fn validate(&mut self) -> Vec<ConfigWarning> {
        todo!("front C")
    }
}
