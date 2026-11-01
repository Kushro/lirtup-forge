//! Command-line options of `lirtup run` (the 1.x `upscale`, `config/args.py`).
//!
//! The binary (`lirtup-cli`) flattens [`ConfigArgs`] into its `run`
//! subcommand; the options become one more layer for
//! [`crate::resolve::resolve`].
//!
//! Flags keep the 1.x names. Two differences: `-q`/`-v` belong to the binary
//! (global), so logging here is `--debug`, `--log-level` and `--log-file`; and
//! the listings (`--list-gpus`, ...) are `lirtup list`.

use std::path::PathBuf;

use clap::Args;

use crate::{Mapping, Value};

/// Options that pick the files and the profile.
#[derive(Debug, Clone, Default, Args)]
pub struct ConfigArgs {
    /// Config file (default: ~/.config/linux-rt-upscaler/config.yaml).
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Profile to apply (default: the first one whose match fits the window).
    #[arg(short = 'p', long = "profile", value_name = "NAME")]
    pub profile: Option<String>,

    /// Scaling mode to use instead of the profile's.
    #[arg(long = "mode", value_name = "ID")]
    pub mode: Option<String>,

    /// One flag per `Config` field, as `args.py` (`--model 4x12`,
    /// `--no-tile-processing`, ...).
    #[command(flatten)]
    pub options: OptionArgs,
}

const MODELS: [&str; 10] = [
    "none", "veryfast", "faster", "fast", "3x12", "4x12", "4x16", "4x24", "4x32", "8x32",
];
/// `LUT_PRESETS` of `shaders/effects/lut/presets.py`.
const LUT_PRESETS: [&str; 12] = [
    "identity", "warm", "cool", "split", "vivid", "pastel", "lofi", "bleach", "film", "noir", "sepia",
    "cyano",
];

/// One flag per [`crate::Config`] field, with the 1.x names. Flags not given
/// stay `None` / `false` and do not override the file.
#[derive(Debug, Clone, Default, Args)]
pub struct OptionArgs {
    // Target selection
    /// Pick the window to scale from a list.
    #[arg(short = 's', long = "select", help_heading = "Target selection")]
    pub select: bool,
    /// Daemon mode: scale windows that match a profile as they appear.
    #[arg(short = 'd', long = "daemon", help_heading = "Target selection")]
    pub daemon: bool,
    /// Scale the window whose title contains this text.
    #[arg(
        short = 't',
        long = "target-title",
        value_name = "TEXT",
        help_heading = "Target selection"
    )]
    pub target_title: Option<String>,
    /// Scale the window whose title matches this regex.
    #[arg(
        long = "target-title-regex",
        value_name = "REGEX",
        help_heading = "Target selection"
    )]
    pub target_title_regex: Option<String>,

    // Focus tracking
    /// Follow the focused window.
    #[arg(short = 'f', long = "follow-focus", help_heading = "Focus tracking")]
    pub follow_focus: bool,
    /// Keep scaling while the target is not focused.
    #[arg(long = "no-focus-pause", help_heading = "Focus tracking")]
    pub no_focus_pause: bool,

    // Intervals and window detection
    /// Seconds between daemon window scans (>= 0.1).
    #[arg(long = "daemon-poll-interval", value_name = "S", help_heading = "Intervals")]
    pub daemon_poll_interval: Option<f64>,
    /// Seconds between focus checks (>= 0.01).
    #[arg(long = "focus-poll-interval", value_name = "S", help_heading = "Intervals")]
    pub focus_poll_interval: Option<f64>,
    /// Seconds between pipeline idle checks (>= 0.01).
    #[arg(long = "pipeline-poll-interval", value_name = "S", help_heading = "Intervals")]
    pub pipeline_poll_interval: Option<f64>,
    /// Seconds to wait before taking the active window after launching.
    #[arg(long = "target-delay", value_name = "S", help_heading = "Window detection")]
    pub target_delay: Option<f64>,
    /// Seconds of the PID detection phase.
    #[arg(long = "pid-timeout", value_name = "S", help_heading = "Window detection")]
    pub pid_timeout: Option<f64>,
    /// Seconds of the WM_CLASS detection phase.
    #[arg(long = "class-timeout", value_name = "S", help_heading = "Window detection")]
    pub class_timeout: Option<f64>,
    /// Maximum seconds to wait for a window.
    #[arg(long = "total-timeout", value_name = "S", help_heading = "Window detection")]
    pub total_timeout: Option<f64>,
    /// Detection phase to try first (1 = PID, 2 = class).
    #[arg(long = "starting-phase", value_parser = clap::value_parser!(u8).range(1..=2),
          help_heading = "Window detection")]
    pub starting_phase: Option<u8>,

    // Upscaling and samplers
    /// SRCNN model (none, veryfast, faster, fast, 3x12, 4x12, 4x16, 4x24, 4x32, 8x32).
    #[arg(short = 'm', long = "model", value_parser = MODELS, help_heading = "Upscaling")]
    pub model: Option<String>,
    /// Chain two 2x passes for 4x.
    #[arg(short = '2', long = "double-upscale", help_heading = "Upscaling")]
    pub double_upscale: bool,
    /// Final upsampling filter.
    #[arg(short = 'U', long = "upsampler", value_parser = ["lanczos", "fsr", "nis"], help_heading = "Upscaling")]
    pub upsampler: Option<String>,
    /// Final downsampling filter.
    #[arg(short = 'D', long = "downsampler", value_parser = ["catmull", "lanczos"], help_heading = "Upscaling")]
    pub downsampler: Option<String>,
    /// Kernel width of the final resampling (0 to 2).
    #[arg(long = "blur", help_heading = "Sampler")]
    pub blur: Option<f32>,
    /// Anti-ringing strength (0 to 1).
    #[arg(long = "antiring-strength", help_heading = "Sampler")]
    pub antiring_strength: Option<f32>,
    /// Turn off tight anti-ringing.
    #[arg(long = "no-tight-antiring", help_heading = "Sampler")]
    pub no_tight_antiring: bool,
    /// Override of the automatic Lanczos radius (2 to 10).
    #[arg(long = "kernel-radius", help_heading = "Sampler")]
    pub kernel_radius: Option<u32>,

    // GPU and display
    /// GPU index or name substring (see `lirtup list gpus`).
    #[arg(short = 'g', long = "gpu", help_heading = "GPU and display")]
    pub gpu: Option<String>,
    /// Monitor: primary, all, an output name or an index (see `lirtup list monitors`).
    #[arg(long = "monitor", help_heading = "GPU and display")]
    pub monitor: Option<String>,
    /// Manual scale factor.
    #[arg(long = "scale-factor", help_heading = "GPU and display")]
    pub scale_factor: Option<f64>,

    // Overlay and presentation
    /// Overlay mode (always-on-top, top-transparent, fullscreen, windowed).
    #[arg(long = "overlay-mode", value_parser = ["always-on-top", "top-transparent", "fullscreen", "windowed"],
          help_heading = "Overlay")]
    pub overlay_mode: Option<String>,
    /// Hide the cursor after this many milliseconds without movement.
    #[arg(long = "hide-cursor", value_name = "MS", help_heading = "Overlay")]
    pub hide_cursor: Option<u32>,
    /// Minimum overlay opacity (0 to 1).
    #[arg(long = "overlay-opacity-min", help_heading = "Overlay")]
    pub overlay_opacity_min: Option<f32>,
    /// Maximum overlay opacity (0 to 1).
    #[arg(long = "overlay-opacity-max", help_heading = "Overlay")]
    pub overlay_opacity_max: Option<f32>,
    /// Output geometry (fit, stretch, cover, 150%, 1920x, x1080, 1920x1080, ...).
    #[arg(
        short = 'o',
        long = "output-geometry",
        visible_alias = "geometry",
        help_heading = "Presentation"
    )]
    pub output_geometry: Option<String>,
    /// Pixels cropped from the top.
    #[arg(long = "crop-top", help_heading = "Presentation")]
    pub crop_top: Option<u32>,
    /// Pixels cropped from the bottom.
    #[arg(long = "crop-bottom", help_heading = "Presentation")]
    pub crop_bottom: Option<u32>,
    /// Pixels cropped from the left.
    #[arg(long = "crop-left", help_heading = "Presentation")]
    pub crop_left: Option<u32>,
    /// Pixels cropped from the right.
    #[arg(long = "crop-right", help_heading = "Presentation")]
    pub crop_right: Option<u32>,
    /// Detect and crop a Wine menu bar (experimental).
    #[arg(long = "auto-crop-menu-bar", help_heading = "Presentation")]
    pub auto_crop_menu_bar: bool,
    /// Color of the letterbox bars (CSS: black, #RRGGBB, rgba(...), ...).
    #[arg(long = "background-color", help_heading = "Presentation")]
    pub background_color: Option<String>,
    /// Horizontal content offset in pixels.
    #[arg(
        long = "offset-x",
        allow_negative_numbers = true,
        help_heading = "Presentation"
    )]
    pub offset_x: Option<i32>,
    /// Vertical content offset in pixels.
    #[arg(
        long = "offset-y",
        allow_negative_numbers = true,
        help_heading = "Presentation"
    )]
    pub offset_y: Option<i32>,

    // Pre-processing and Magpie effects
    /// Debanding before scaling.
    #[arg(long = "enable-deband", help_heading = "Pre-processing")]
    pub enable_deband: bool,
    /// Debanding intensity (0 to 1).
    #[arg(long = "deband-strength", help_heading = "Pre-processing")]
    pub deband_strength: Option<f32>,
    /// Add a MagpieFX effect to the chain (repeatable): `Name` or `Name:param=value,...`.
    #[arg(
        long = "magpie-effect",
        value_name = "EFFECT",
        help_heading = "Pre-processing"
    )]
    pub magpie_effects: Vec<String>,
    /// Log the GPU time of the MagpieFX chain.
    #[arg(long = "magpie-profile", help_heading = "Pre-processing")]
    pub magpie_profile: bool,
    /// Engine for the Magpie effects.
    #[arg(long = "effects-engine", value_parser = ["native", "magpiefx"], help_heading = "Magpie effects")]
    pub effects_engine: Option<String>,
    /// Anime4K_Upscale_S (2x) before scaling.
    #[arg(long = "enable-anime4k", help_heading = "Magpie effects")]
    pub enable_anime4k: bool,
    /// LumaSharpen.
    #[arg(long = "enable-luma-sharpen", help_heading = "Magpie effects")]
    pub enable_luma_sharpen: bool,
    /// LumaSharpen strength (0 to 3).
    #[arg(long = "luma-sharpen-strength", help_heading = "Magpie effects")]
    pub luma_sharpen_strength: Option<f32>,
    /// Maximum sharpening per pixel (0 to 1).
    #[arg(long = "luma-sharpen-clamp", help_heading = "Magpie effects")]
    pub luma_sharpen_clamp: Option<f32>,
    /// Sample pattern (0 to 3).
    #[arg(long = "luma-sharpen-pattern", value_parser = clap::value_parser!(u32).range(0..=3),
          help_heading = "Magpie effects")]
    pub luma_sharpen_pattern: Option<u32>,
    /// Sample pattern radius (0 to 6).
    #[arg(long = "luma-sharpen-offset-bias", help_heading = "Magpie effects")]
    pub luma_sharpen_offset_bias: Option<f32>,
    /// CRT-Easymode (replaces the final scaler).
    #[arg(long = "enable-crt", help_heading = "Magpie effects")]
    pub enable_crt: bool,
    /// Horizontal sharpness (0 to 1).
    #[arg(long = "crt-sharpness-h", help_heading = "CRT-Easymode")]
    pub crt_sharpness_h: Option<f32>,
    /// Vertical sharpness (0 to 1).
    #[arg(long = "crt-sharpness-v", help_heading = "CRT-Easymode")]
    pub crt_sharpness_v: Option<f32>,
    /// RGB mask strength (0 to 1).
    #[arg(long = "crt-mask-strength", help_heading = "CRT-Easymode")]
    pub crt_mask_strength: Option<f32>,
    /// Mask dot width (1 to 100).
    #[arg(long = "crt-mask-dot-width", help_heading = "CRT-Easymode")]
    pub crt_mask_dot_width: Option<u32>,
    /// Mask dot height (1 to 100).
    #[arg(long = "crt-mask-dot-height", help_heading = "CRT-Easymode")]
    pub crt_mask_dot_height: Option<u32>,
    /// Mask stagger (0 to 100).
    #[arg(long = "crt-mask-stagger", help_heading = "CRT-Easymode")]
    pub crt_mask_stagger: Option<u32>,
    /// Mask size (1 to 100).
    #[arg(long = "crt-mask-size", help_heading = "CRT-Easymode")]
    pub crt_mask_size: Option<u32>,
    /// Scanline strength (0 to 1).
    #[arg(long = "crt-scanline-strength", help_heading = "CRT-Easymode")]
    pub crt_scanline_strength: Option<f32>,
    /// Min scanline beam width (0.5 to 5).
    #[arg(long = "crt-scanline-beam-width-min", help_heading = "CRT-Easymode")]
    pub crt_scanline_beam_width_min: Option<f32>,
    /// Max scanline beam width (0.5 to 5).
    #[arg(long = "crt-scanline-beam-width-max", help_heading = "CRT-Easymode")]
    pub crt_scanline_beam_width_max: Option<f32>,
    /// Min scanline brightness (0 to 1).
    #[arg(long = "crt-scanline-bright-min", help_heading = "CRT-Easymode")]
    pub crt_scanline_bright_min: Option<f32>,
    /// Max scanline brightness (0 to 1).
    #[arg(long = "crt-scanline-bright-max", help_heading = "CRT-Easymode")]
    pub crt_scanline_bright_max: Option<f32>,
    /// No scanlines when the source height is at least this (1 to 1000).
    #[arg(long = "crt-scanline-cutoff", help_heading = "CRT-Easymode")]
    pub crt_scanline_cutoff: Option<u32>,
    /// Input gamma (0.1 to 5).
    #[arg(long = "crt-gamma-input", help_heading = "CRT-Easymode")]
    pub crt_gamma_input: Option<f32>,
    /// Output gamma (0.1 to 5).
    #[arg(long = "crt-gamma-output", help_heading = "CRT-Easymode")]
    pub crt_gamma_output: Option<f32>,
    /// Brightness boost (1 to 2).
    #[arg(long = "crt-bright-boost", help_heading = "CRT-Easymode")]
    pub crt_bright_boost: Option<f32>,
    /// Dilation (0 or 1).
    #[arg(long = "crt-dilation", value_parser = clap::value_parser!(u32).range(0..=1),
          help_heading = "CRT-Easymode")]
    pub crt_dilation: Option<u32>,

    // Post-processing
    /// Contrast Adaptive Sharpening.
    #[arg(long = "enable-cas", help_heading = "Post-processing")]
    pub enable_cas: bool,
    /// CAS intensity (0 to 1).
    #[arg(long = "cas-strength", help_heading = "Post-processing")]
    pub cas_strength: Option<f32>,
    /// Bloom.
    #[arg(long = "enable-bloom", help_heading = "Post-processing")]
    pub enable_bloom: bool,
    /// Bloom intensity (0 to 1).
    #[arg(long = "bloom-strength", help_heading = "Post-processing")]
    pub bloom_strength: Option<f32>,
    /// Brightness threshold for bloom (0 to 1).
    #[arg(long = "bloom-threshold", help_heading = "Post-processing")]
    pub bloom_threshold: Option<f32>,
    /// Bloom blur radius in pixels (1 to 16).
    #[arg(long = "bloom-radius", help_heading = "Post-processing")]
    pub bloom_radius: Option<u32>,
    /// Vignette.
    #[arg(long = "enable-vignette", help_heading = "Post-processing")]
    pub enable_vignette: bool,
    /// Vignette intensity (0 to 1).
    #[arg(long = "vignette-strength", help_heading = "Post-processing")]
    pub vignette_strength: Option<f32>,
    /// Distance from the center where the vignette starts (0 to 2).
    #[arg(long = "vignette-radius", help_heading = "Post-processing")]
    pub vignette_radius: Option<f32>,
    /// Softness of the vignette transition (0.1 to 10).
    #[arg(long = "vignette-falloff", help_heading = "Post-processing")]
    pub vignette_falloff: Option<f32>,
    /// Film grain.
    #[arg(long = "enable-grain", help_heading = "Post-processing")]
    pub enable_grain: bool,
    /// Grain intensity (0 to 1).
    #[arg(long = "grain-strength", help_heading = "Post-processing")]
    pub grain_strength: Option<f32>,
    /// Apparent grain size (1 to 10).
    #[arg(long = "grain-size", help_heading = "Post-processing")]
    pub grain_size: Option<f32>,
    /// 3D color LUT.
    #[arg(long = "enable-lut", help_heading = "Post-processing")]
    pub enable_lut: bool,
    /// Blend between the original and the graded image (0 to 1).
    #[arg(long = "lut-intensity", help_heading = "Post-processing")]
    pub lut_intensity: Option<f32>,
    /// Built-in LUT preset.
    #[arg(long = "lut-preset", value_parser = LUT_PRESETS, help_heading = "Post-processing")]
    pub lut_preset: Option<String>,

    // Screenshots, OSD, stats
    /// Folder for screenshots.
    #[arg(long = "screenshot-dir", value_name = "DIR", help_heading = "Screenshots")]
    pub screenshot_dir: Option<PathBuf>,
    /// Screenshot file name template (`{timestamp:%Y%m%d_%H%M%S}`).
    #[arg(
        long = "screenshot-filename",
        value_name = "TEMPLATE",
        help_heading = "Screenshots"
    )]
    pub screenshot_filename: Option<String>,
    /// No on-screen messages.
    #[arg(long = "no-osd", help_heading = "OSD and stats")]
    pub no_osd: bool,
    /// Seconds an OSD message stays.
    #[arg(long = "osd-duration", value_name = "S", help_heading = "OSD and stats")]
    pub osd_duration: Option<f32>,
    /// Start with the stats panel visible.
    #[arg(long = "show-stats", help_heading = "OSD and stats")]
    pub show_stats: bool,
    /// Write the latency of every presented frame to this CSV.
    #[arg(long = "latency-log", value_name = "FILE", help_heading = "OSD and stats")]
    pub latency_log: Option<PathBuf>,

    // Vulkan and frame generation
    /// Vulkan presentation mode.
    #[arg(long = "vulkan-present-mode", value_parser = ["fifo", "mailbox", "immediate"], help_heading = "Vulkan")]
    pub vulkan_present_mode: Option<String>,
    /// Maximum pipeline frames per second.
    #[arg(long = "max-fps", value_parser = clap::value_parser!(u32).range(1..), help_heading = "Vulkan")]
    pub max_fps: Option<u32>,
    /// Skip presents while the target has no new frame.
    #[arg(long = "present-on-change-only", help_heading = "Vulkan")]
    pub present_on_change_only: bool,
    /// GPU fence timeout in nanoseconds.
    #[arg(long = "frame-timeout", value_name = "NS", help_heading = "Vulkan")]
    pub frame_timeout: Option<u64>,
    /// Frame generation multiplier (1 to 5, or auto).
    #[arg(long = "frame-gen", value_parser = parse_multiplier, help_heading = "Frame generation")]
    pub frame_gen_multiplier: Option<String>,
    /// Optical flow resolution (0.25 to 1).
    #[arg(long = "frame-gen-flow-scale", help_heading = "Frame generation")]
    pub frame_gen_flow_scale: Option<f32>,
    /// Lighter lsfg model.
    #[arg(long = "frame-gen-performance", help_heading = "Frame generation")]
    pub frame_gen_performance: bool,
    /// Folder with lsfg-vk.dll, or Lossless.dll for the native engine.
    #[arg(
        long = "frame-gen-dll",
        value_name = "PATH",
        help_heading = "Frame generation"
    )]
    pub frame_gen_dll: Option<PathBuf>,
    /// Frame generation engine.
    #[arg(long = "frame-gen-engine", value_parser = ["layer", "native"], help_heading = "Frame generation")]
    pub frame_gen_engine: Option<String>,

    // Tiles and errors
    /// Process the whole frame every time.
    #[arg(long = "no-tile-processing", help_heading = "Tiles")]
    pub no_tile_processing: bool,
    /// Transfer the whole frame instead of the damaged regions.
    #[arg(long = "no-damage-tracking", help_heading = "Tiles")]
    pub no_damage_tracking: bool,
    /// Tile interior size in pixels.
    #[arg(long = "tile-size", value_parser = clap::value_parser!(u32).range(1..), help_heading = "Tiles")]
    pub tile_size: Option<u32>,
    /// Border pixels added to each tile.
    #[arg(long = "tile-context-margin", help_heading = "Tiles")]
    pub tile_context_margin: Option<u32>,
    /// Maximum tiles per frame.
    #[arg(long = "max-tile-layers", help_heading = "Tiles")]
    pub max_tile_layers: Option<u32>,
    /// Dirty area fraction that forces a full frame (0 to 1).
    #[arg(long = "area-threshold", help_heading = "Tiles")]
    pub area_threshold: Option<f32>,
    /// Consecutive capture failures before stopping.
    #[arg(long = "max-capture-failures", help_heading = "Error recovery")]
    pub max_capture_failures: Option<u32>,
    /// Seconds to wait after a capture failure.
    #[arg(
        long = "capture-failure-delay",
        value_name = "S",
        help_heading = "Error recovery"
    )]
    pub capture_failure_delay: Option<f64>,
    /// Minimum seconds between swapchain recreations.
    #[arg(
        long = "swapchain-debounce",
        value_name = "S",
        help_heading = "Error recovery"
    )]
    pub swapchain_debounce: Option<f64>,

    // Logging
    /// Log at DEBUG level.
    #[arg(long = "debug", help_heading = "Logging")]
    pub debug: bool,
    /// Log level (DEBUG, INFO, WARNING, ERROR).
    #[arg(long = "log-level", value_parser = ["DEBUG", "INFO", "WARNING", "ERROR", "debug", "info", "warning", "error"],
          help_heading = "Logging")]
    pub log_level: Option<String>,
    /// Write logs to this file.
    #[arg(long = "log-file", value_name = "FILE", help_heading = "Logging")]
    pub log_file: Option<PathBuf>,

    /// Generic override, repeatable: `--set key=value` (YAML value; dotted
    /// keys for hotkeys: `--set hotkeys.zoom_in=Ctrl+Plus`).
    #[arg(long = "set", value_name = "KEY=VALUE")]
    pub set: Vec<String>,
}

/// `--frame-gen`: 1 to 5, or auto (any case).
fn parse_multiplier(text: &str) -> Result<String, String> {
    let t = text.trim().to_ascii_lowercase();
    match t.as_str() {
        "auto" => Ok(t),
        _ => match t.parse::<u8>() {
            Ok(1..=5) => Ok(t),
            _ => Err(format!(
                "invalid frame generation multiplier {text:?} (1 to 5, or auto)"
            )),
        },
    }
}

/// A number when the text is one (`--gpu 1` is an index), text otherwise.
fn index_or_name(text: &str) -> Value {
    match text.trim().parse::<u32>() {
        Ok(n) => Value::from(n),
        Err(_) => Value::String(text.to_owned()),
    }
}

struct Layer(Mapping);

impl Layer {
    fn put(&mut self, key: &str, value: Option<impl Into<Value>>) {
        if let Some(value) = value {
            self.0.insert(key.into(), value.into());
        }
    }

    fn flag(&mut self, key: &str, given: bool, value: bool) {
        if given {
            self.0.insert(key.into(), Value::Bool(value));
        }
    }

    fn float(&mut self, key: &str, value: Option<f32>) {
        // Through the shortest f32 spelling, so `--blur 0.3` is 0.3.
        self.put(key, value.and_then(|v| v.to_string().parse::<f64>().ok()));
    }

    fn path(&mut self, key: &str, value: &Option<PathBuf>) {
        self.put(key, value.as_ref().map(|p| p.to_string_lossy().into_owned()));
    }
}

impl ConfigArgs {
    /// The options given, as a mapping layer with the YAML keys of
    /// [`crate::Config`]. Flags not given are absent (they must not override
    /// the file).
    pub fn overrides(&self) -> Result<Mapping, crate::ConfigError> {
        let o = &self.options;
        let mut l = Layer(Mapping::new());
        l.put("mode", self.mode.clone());

        l.flag("select", o.select, true);
        l.flag("daemon", o.daemon, true);
        l.put("target_title", o.target_title.clone());
        l.put("target_title_regex", o.target_title_regex.clone());
        l.flag("follow_focus", o.follow_focus, true);
        l.flag("pause_on_focus_loss", o.no_focus_pause, false);

        l.put("daemon_poll_interval", o.daemon_poll_interval);
        l.put("focus_poll_interval", o.focus_poll_interval);
        l.put("pipeline_poll_interval", o.pipeline_poll_interval);
        l.put("target_delay", o.target_delay);
        l.put("pid_timeout", o.pid_timeout);
        l.put("class_timeout", o.class_timeout);
        l.put("total_timeout", o.total_timeout);
        l.put("starting_phase", o.starting_phase);

        l.put("model", o.model.clone());
        l.flag("double_upscale", o.double_upscale, true);
        l.put("upsampler", o.upsampler.clone());
        l.put("downsampler", o.downsampler.clone());
        l.float("blur", o.blur);
        l.float("antiring_strength", o.antiring_strength);
        l.flag("tight_antiring", o.no_tight_antiring, false);
        l.put("kernel_radius", o.kernel_radius);

        l.put("gpu", o.gpu.as_deref().map(index_or_name));
        l.put("monitor", o.monitor.as_deref().map(index_or_name));
        l.put("scale_factor", o.scale_factor);

        l.put("overlay_mode", o.overlay_mode.clone());
        l.put("hide_cursor", o.hide_cursor);
        l.float("overlay_opacity_min", o.overlay_opacity_min);
        l.float("overlay_opacity_max", o.overlay_opacity_max);
        l.put("output_geometry", o.output_geometry.clone());
        l.put("crop_top", o.crop_top);
        l.put("crop_bottom", o.crop_bottom);
        l.put("crop_left", o.crop_left);
        l.put("crop_right", o.crop_right);
        l.flag("auto_crop_menu_bar", o.auto_crop_menu_bar, true);
        l.put("background_color", o.background_color.clone());
        l.put("offset_x", o.offset_x);
        l.put("offset_y", o.offset_y);

        l.flag("deband_enabled", o.enable_deband, true);
        l.float("deband_strength", o.deband_strength);
        if !o.magpie_effects.is_empty() {
            let chain = o.magpie_effects.iter().cloned().map(Value::String).collect();
            l.0.insert("magpie_effects".into(), Value::Sequence(chain));
        }
        l.flag("magpie_profile", o.magpie_profile, true);
        l.put("effects_engine", o.effects_engine.clone());
        l.flag("anime4k_enabled", o.enable_anime4k, true);
        l.flag("luma_sharpen_enabled", o.enable_luma_sharpen, true);
        l.float("luma_sharpen_strength", o.luma_sharpen_strength);
        l.float("luma_sharpen_clamp", o.luma_sharpen_clamp);
        l.put("luma_sharpen_pattern", o.luma_sharpen_pattern);
        l.float("luma_sharpen_offset_bias", o.luma_sharpen_offset_bias);
        l.flag("crt_enabled", o.enable_crt, true);
        l.float("crt_sharpness_h", o.crt_sharpness_h);
        l.float("crt_sharpness_v", o.crt_sharpness_v);
        l.float("crt_mask_strength", o.crt_mask_strength);
        l.put("crt_mask_dot_width", o.crt_mask_dot_width);
        l.put("crt_mask_dot_height", o.crt_mask_dot_height);
        l.put("crt_mask_stagger", o.crt_mask_stagger);
        l.put("crt_mask_size", o.crt_mask_size);
        l.float("crt_scanline_strength", o.crt_scanline_strength);
        l.float("crt_scanline_beam_width_min", o.crt_scanline_beam_width_min);
        l.float("crt_scanline_beam_width_max", o.crt_scanline_beam_width_max);
        l.float("crt_scanline_bright_min", o.crt_scanline_bright_min);
        l.float("crt_scanline_bright_max", o.crt_scanline_bright_max);
        l.put("crt_scanline_cutoff", o.crt_scanline_cutoff);
        l.float("crt_gamma_input", o.crt_gamma_input);
        l.float("crt_gamma_output", o.crt_gamma_output);
        l.float("crt_bright_boost", o.crt_bright_boost);
        l.put("crt_dilation", o.crt_dilation);

        l.flag("cas_enabled", o.enable_cas, true);
        l.float("cas_strength", o.cas_strength);
        l.flag("bloom_enabled", o.enable_bloom, true);
        l.float("bloom_strength", o.bloom_strength);
        l.float("bloom_threshold", o.bloom_threshold);
        l.put("bloom_radius", o.bloom_radius);
        l.flag("vignette_enabled", o.enable_vignette, true);
        l.float("vignette_strength", o.vignette_strength);
        l.float("vignette_radius", o.vignette_radius);
        l.float("vignette_falloff", o.vignette_falloff);
        l.flag("grain_enabled", o.enable_grain, true);
        l.float("grain_strength", o.grain_strength);
        l.float("grain_size", o.grain_size);
        l.flag("lut_enabled", o.enable_lut, true);
        l.float("lut_intensity", o.lut_intensity);
        l.put("lut_preset", o.lut_preset.clone());

        l.path("screenshot_dir", &o.screenshot_dir);
        l.put("screenshot_filename", o.screenshot_filename.clone());
        l.flag("show_osd", o.no_osd, false);
        l.float("osd_duration", o.osd_duration);
        l.flag("show_stats", o.show_stats, true);
        l.path("latency_log", &o.latency_log);

        l.put("vulkan_present_mode", o.vulkan_present_mode.clone());
        l.put("max_fps", o.max_fps);
        l.flag("present_on_change_only", o.present_on_change_only, true);
        l.put("frame_timeout", o.frame_timeout);
        l.put(
            "frame_gen_multiplier",
            o.frame_gen_multiplier.as_deref().map(index_or_name),
        );
        l.float("frame_gen_flow_scale", o.frame_gen_flow_scale);
        l.flag("frame_gen_performance", o.frame_gen_performance, true);
        l.path("frame_gen_dll", &o.frame_gen_dll);
        l.put("frame_gen_engine", o.frame_gen_engine.clone());

        l.flag("use_tile_processing", o.no_tile_processing, false);
        l.flag("use_damage_tracking", o.no_damage_tracking, false);
        l.put("tile_size", o.tile_size);
        l.put("tile_context_margin", o.tile_context_margin);
        l.put("max_tile_layers", o.max_tile_layers);
        l.float("area_threshold", o.area_threshold);
        l.put("max_capture_failures", o.max_capture_failures);
        l.put("capture_failure_delay", o.capture_failure_delay);
        l.put("swapchain_debounce", o.swapchain_debounce);

        l.put("log_level", o.log_level.as_ref().map(|s| s.to_ascii_uppercase()));
        if o.debug {
            l.put("log_level", Some("DEBUG"));
        }
        l.path("log_file", &o.log_file);

        for item in &o.set {
            set_override(&mut l.0, item)?;
        }
        Ok(l.0)
    }
}

/// Apply one `--set key=value`; the value is YAML (`--set max_fps=60`,
/// `--set 'magpie_effects=[CAS]'`), dotted keys go one level deeper.
fn set_override(map: &mut Mapping, item: &str) -> Result<(), crate::ConfigError> {
    let invalid = |message: String| crate::ConfigError::Invalid {
        key: "--set".into(),
        message,
    };
    let (key, text) = item
        .split_once('=')
        .ok_or_else(|| invalid(format!("expected key=value, got {item:?}")))?;
    let key = key.trim();
    if key.is_empty() {
        return Err(invalid(format!("empty key in {item:?}")));
    }
    let value: Value = if text.trim().is_empty() {
        Value::String(String::new())
    } else {
        serde_yaml_ng::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()))
    };
    match key.split_once('.') {
        None => {
            map.insert(key.into(), value);
        }
        Some((outer, inner)) => {
            let entry = map
                .entry(outer.into())
                .or_insert_with(|| Value::Mapping(Mapping::new()));
            if !entry.is_mapping() {
                *entry = Value::Mapping(Mapping::new());
            }
            if let Value::Mapping(nested) = entry {
                nested.insert(inner.into(), value);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::*;

    #[derive(Debug, Parser)]
    struct Cli {
        #[command(flatten)]
        config: ConfigArgs,
    }

    fn parse(args: &[&str]) -> Mapping {
        let mut argv = vec!["lirtup"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).unwrap().config.overrides().unwrap()
    }

    #[test]
    fn command_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn nothing_given_is_empty() {
        assert!(parse(&[]).is_empty());
    }

    #[test]
    fn flags_become_yaml_keys() {
        let m = parse(&[
            "-m",
            "4x12",
            "-2",
            "--no-tile-processing",
            "--frame-gen",
            "AUTO",
            "-g",
            "1",
            "--monitor",
            "DP-1",
            "--blur",
            "0.3",
            "--offset-x",
            "-20",
            "-o",
            "150%",
            "--debug",
            "--magpie-effect",
            "CAS",
            "--magpie-effect",
            "Lanczos",
            "--mode",
            "anime",
        ]);
        assert_eq!(m["model"], Value::from("4x12"));
        assert_eq!(m["double_upscale"], Value::Bool(true));
        assert_eq!(m["use_tile_processing"], Value::Bool(false));
        assert_eq!(m["frame_gen_multiplier"], Value::from("auto"));
        assert_eq!(m["gpu"], Value::from(1u32));
        assert_eq!(m["monitor"], Value::from("DP-1"));
        assert_eq!(serde_yaml_ng::to_string(&m["blur"]).unwrap().trim(), "0.3");
        assert_eq!(m["offset_x"], Value::from(-20));
        assert_eq!(m["output_geometry"], Value::from("150%"));
        assert_eq!(m["log_level"], Value::from("DEBUG"));
        assert_eq!(m["magpie_effects"].as_sequence().unwrap().len(), 2);
        assert_eq!(m["mode"], Value::from("anime"));
        assert!(!m.contains_key("cas_enabled"));

        let (config, warnings) = crate::Config::from_mapping(&m);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.scaling.model, crate::Model::M4x12);
        assert_eq!(config.frame_gen_multiplier, crate::FrameGenMultiplier::Auto);
    }

    #[test]
    fn rejects_bad_values() {
        assert!(Cli::try_parse_from(["lirtup", "--model", "huge"]).is_err());
        assert!(Cli::try_parse_from(["lirtup", "--frame-gen", "7"]).is_err());
        assert!(Cli::try_parse_from(["lirtup", "--starting-phase", "3"]).is_err());
    }

    #[test]
    fn generic_set() {
        let m = parse(&[
            "--set",
            "max_fps=60",
            "--set",
            "hotkeys.zoom_in=Ctrl+Plus",
            "--set",
            "future=[1, 2]",
        ]);
        assert_eq!(m["max_fps"], Value::from(60));
        assert_eq!(m["hotkeys"]["zoom_in"], Value::from("Ctrl+Plus"));
        assert!(m["future"].is_sequence());
        let bad = Cli::try_parse_from(["lirtup", "--set", "novalue"]).unwrap();
        assert!(bad.config.overrides().is_err());
    }
}
