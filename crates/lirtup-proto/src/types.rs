//! Payload types of [`crate::Command`] and [`crate::Event`].

use serde::{Deserialize, Serialize};

pub use lirtup_config::{Crop, FrameGenEngine, FrameGenMultiplier};

/// Which window to scale.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// By XID.
    Window(u32),
    /// The focused one (after the delay, if any).
    Focused,
    /// First title containing this (case-insensitive).
    Title(String),
    /// First window of this process.
    Pid(u32),
}

/// Fullscreen or windowed (the two activation hotkeys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    /// Overlay mode from the config (`always-on-top`, `top-transparent` or
    /// `fullscreen`).
    Fullscreen,
    /// Overlay mode `windowed`.
    Windowed,
}

/// A window as the protocol names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowRef {
    /// XID.
    pub xid: u32,
    /// Title.
    pub title: String,
    /// `WM_CLASS` class name.
    pub class: Option<String>,
    /// PID.
    pub pid: Option<u32>,
}

/// Width × height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Size {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// A running session, as the session card shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionView {
    /// Target window.
    pub window: WindowRef,
    /// Profile applied.
    pub profile: Option<String>,
    /// Mode applied.
    pub mode: Option<String>,
    /// Fullscreen or windowed.
    pub kind: SessionKind,
    /// Captured size (after crop).
    pub src: Size,
    /// Output size.
    pub dst: Size,
    /// Pass labels of the chain, in order (`deband`, `CuNNy 4x12`, `lanczos`, ...).
    pub chain: Vec<String>,
    /// Unix time in ms when it started.
    pub started_at_ms: u64,
}

/// Why a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// `Stop`, the toolbar, the tray or `exit_app`.
    User,
    /// The target window closed.
    WindowClosed,
    /// A `Start` replaced it.
    Replaced,
    /// `Restart`.
    Restart,
    /// `Quit` or a signal.
    Quit,
    /// After a `SessionError`.
    Error,
}

/// Where a session failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Reading config, profile or mode.
    Config,
    /// Finding the window.
    Window,
    /// Vulkan instance or device.
    Device,
    /// Capture.
    Capture,
    /// Building the chain (models, MagpieFX).
    Effects,
    /// Overlay window.
    Overlay,
    /// Swapchain.
    Swapchain,
    /// Frame generation.
    FrameGen,
    /// While running.
    Runtime,
}

/// GPU time of one stage, in ms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageTime {
    /// Pass label.
    pub name: String,
    /// Mean ms over the sample window.
    pub ms: f32,
}

/// Warning of the stats panel (`test_stats.py`: the GPU cannot keep up).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuWarning {
    /// Frame time budget at the target rate, ms.
    pub budget_ms: f32,
    /// GPU time per frame, ms.
    pub gpu_ms: f32,
    /// The heaviest effects `(label, ms)`.
    pub heaviest: Vec<(String, f32)>,
}

/// One stats sample (every 500 ms). The stats panel of the scaler draws
/// from the same struct, so both show the same.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StatsSample {
    /// Frames captured per second (game frames that changed).
    pub capture_fps: f32,
    /// Frames presented per second (with generated ones).
    pub present_fps: f32,
    /// Generated frames per second (0 without frame generation).
    pub generated_fps: f32,
    /// Capture → present latency, mean ms.
    pub latency_ms: f32,
    /// Latency, 95th percentile ms.
    pub latency_p95_ms: f32,
    /// GPU time per frame, ms.
    pub gpu_ms: f32,
    /// Per stage.
    pub stages: Vec<StageTime>,
    /// Fraction of the frame processed (tiles), 0 to 1.
    pub dirty_fraction: f32,
    /// Tiles processed in the last frame (`None` without tiles).
    pub tiles: Option<u32>,
    /// Frame generation multiplier in use (`None` = off).
    pub multiplier: Option<u8>,
    /// GPU warning.
    pub gpu_warning: Option<GpuWarning>,
    /// Captures per second, changed or not (1.x «N/s polled»).
    #[serde(default)]
    pub poll_fps: f32,
    /// CPU time of a full render, mean ms over the last second (`None`
    /// when nothing was rendered), without [`StatsSample::wait_ms`].
    #[serde(default)]
    pub render_ms: Option<f32>,
    /// Of each render, the time spent waiting to present, mean ms over the
    /// last second: the previous frame generation cycle (its pacing), or a
    /// blocking swapchain present. Not work: the panel shows it apart.
    #[serde(default)]
    pub wait_ms: Option<f32>,
    /// Latency per kind of frame over the last second, real first (1.x
    /// «real 12.3 ms (max 20)  generated …»).
    #[serde(default)]
    pub latency_by_kind: Vec<KindLatency>,
}

/// Latency of one kind of frame.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct KindLatency {
    /// `real` or `generated` (the CSV spelling).
    pub kind: String,
    /// Mean, ms.
    pub mean_ms: f32,
    /// Maximum, ms.
    pub max_ms: f32,
}

/// Frame generation, as the GUI shows it (Presentación, tray, toolbar).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FrameGenStatus {
    /// Configured engine.
    pub engine: FrameGenEngine,
    /// Configured multiplier.
    pub configured: FrameGenMultiplier,
    /// State (why not, when not active).
    pub state: FrameGenState,
    /// Multiplier running now (`None` = off).
    pub active: Option<u8>,
    /// The layer is installed.
    pub layer_installed: bool,
    /// The DLL the engine needs was found.
    pub dll_found: bool,
    /// The native engine can run (DLL + external fd support).
    pub native_available: bool,
    /// Set by `LSFGVK_*` variables: the GUI disables the section.
    pub env_locked: bool,
    /// Why it is not active, as a code the GUI translates (`None` when off
    /// or active).
    #[serde(default)]
    pub reason_code: Option<FrameGenReason>,
    /// Detail of the reason, in English (the log line; the GUI shows it
    /// under the translated text, e.g. the lsfg error).
    pub reason: Option<String>,
}

/// Why frame generation is not running (`FrameGenStatus.reason_code`).
/// Finer than [`FrameGenState`]: `NoDll` and `NoNative` have several causes
/// the GUI words differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameGenReason {
    /// The lsfg-vk layer is not installed (`NoLayer`).
    LayerMissing,
    /// `Lossless.dll` was not found (native engine).
    LosslessDllMissing,
    /// `lsfg-vk.dll` was not found (layer engine).
    LsfgVkDllMissing,
    /// `LSFGVK_*` variables configure it (`External`).
    EnvLocked,
    /// The driver cannot share memory by fd (`VK_KHR_external_memory_fd`).
    NoExternalFd,
    /// lsfg failed during the session and frame generation was turned off;
    /// `reason` has the error.
    EngineFailed,
}

/// What frame generation is doing, or why not. Same values as
/// `lirtup_fg::FrameGenState`, redeclared so this crate (which the GUI links)
/// does not depend on `lirtup-fg`; `lirtup-core` converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameGenState {
    /// Off in the configuration.
    #[default]
    Off,
    /// Running.
    Active,
    /// lsfg-vk layer not installed.
    NoLayer,
    /// DLL not found.
    NoDll,
    /// Set by `LSFGVK_*` variables.
    External,
    /// Native engine not available.
    NoNative,
}

/// Daemon state in the snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct DaemonStatus {
    /// On.
    pub on: bool,
    /// Keep running after the GUI quits.
    pub keep_after_exit: bool,
    /// Profiles being watched.
    pub watching: u32,
}

/// Whole state, sent after `Hello`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EngineSnapshot {
    /// [`crate::PROTOCOL_VERSION`] of the server.
    pub protocol: u32,
    /// Crate version of the server (`2.0.0`).
    pub version: String,
    /// Running session.
    pub session: Option<SessionView>,
    /// Last error of a session that is not running, if the card should show it.
    pub last_error: Option<(Stage, String)>,
    /// Daemon.
    pub daemon: DaemonStatus,
    /// Frame generation.
    pub framegen: FrameGenStatus,
}

/// Kind of a MagpieFX parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    /// Float.
    Float,
    /// Integer.
    Int,
}

/// A parameter of an effect (the GUI generates its control).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectParamInfo {
    /// HLSL name (the key in `params`).
    pub name: String,
    /// Label (empty = name).
    pub label: String,
    /// Kind.
    pub kind: ParamKind,
    /// Default.
    pub default: f64,
    /// Minimum.
    pub min: f64,
    /// Maximum.
    pub max: f64,
    /// Step.
    pub step: f64,
}

/// Where an effect comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectOrigin {
    /// Shipped.
    Builtin,
    /// The user's folder.
    User,
}

/// One effect of the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectInfo {
    /// Catalog name (what goes in `magpie_effects`).
    pub name: String,
    /// `//!SORT_NAME` or the name.
    pub sort_name: String,
    /// Passes.
    pub passes: u32,
    /// Parameters.
    pub params: Vec<EffectParamInfo>,
    /// Checked against Magpie (else «Sin probar»).
    pub verified: bool,
    /// Origin.
    pub origin: EffectOrigin,
    /// Has an FP16 variant.
    pub fp16: bool,
    /// Parse error, if the file could not be read (listed anyway, disabled).
    pub error: Option<String>,
}

/// A monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// Index (`monitor: 1`).
    pub index: u32,
    /// Output name.
    pub name: String,
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Primary.
    pub primary: bool,
    /// Refresh rate.
    pub refresh_hz: Option<f32>,
}

/// A GPU.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuInfo {
    /// Index (`gpu: 0`).
    pub index: u32,
    /// Name.
    pub name: String,
    /// PCI vendor.
    pub vendor_id: u32,
    /// PCI device.
    pub device_id: u32,
    /// Discrete.
    pub discrete: bool,
    /// Driver name and version.
    pub driver: String,
    /// `shaderFloat16`.
    pub fp16: bool,
    /// External fd support (native frame generation).
    pub external_fd: bool,
    /// The one `gpu:` resolves to now.
    pub selected: bool,
}

/// Kind of a bench case (`bench.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchKind {
    /// A native pass or model.
    Native,
    /// A MagpieFX effect.
    Magpiefx,
    /// Frame generation at a multiplier.
    Framegen,
}

/// What to measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchRequest {
    /// Source size of the test image.
    pub src: Size,
    /// Output size.
    pub dst: Size,
    /// Measure the passes of this mode (`None` = every native pass and model).
    pub mode: Option<String>,
    /// Also these MagpieFX effects.
    pub effects: Vec<String>,
    /// Also frame generation.
    pub framegen: bool,
    /// Frames per case.
    pub frames: u32,
}

/// Result of one case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchEntry {
    /// Kind.
    pub kind: BenchKind,
    /// Effect name, model or frame generation key.
    pub name: String,
    /// Median ms per frame.
    pub ms: Option<f32>,
    /// 95th percentile.
    pub p95: Option<f32>,
    /// Frames measured.
    pub frames: u32,
    /// Error that stopped it.
    pub error: Option<String>,
}

/// Result of a bench.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchReport {
    /// GPU key (`name:vendor:device`) the times belong to.
    pub gpu: String,
    /// Cases.
    pub entries: Vec<BenchEntry>,
    /// Cancelled before the end.
    pub cancelled: bool,
}

/// Diagnostics for Acerca de (copied to the clipboard as text).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DiagReport {
    /// `lirtup serve` version.
    pub version: String,
    /// GPUs.
    pub gpus: Vec<GpuInfo>,
    /// Vulkan loader / API version.
    pub vulkan: Option<String>,
    /// X11 or XWayland.
    pub display: String,
    /// Frame generation.
    pub framegen: FrameGenStatus,
    /// `Lossless.dll` path, if found.
    pub lossless_dll: Option<String>,
    /// `dxc --version`.
    pub dxc: Option<String>,
    /// `glslangValidator --version`.
    pub glslang: Option<String>,
    /// Config folder.
    pub config_dir: String,
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Error.
    Error,
    /// Warning.
    Warn,
    /// Info.
    Info,
    /// Debug.
    Debug,
}
