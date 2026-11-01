//! Every `Command` and every `Event` survives `encode_line` → `decode_line`,
//! and the wire format is the one the protocol docs show.

use lirtup_config::{Crop, FrameGenEngine, FrameGenMultiplier, HotkeyAction};
use lirtup_proto::*;

fn window() -> WindowRef {
    WindowRef {
        xid: 0x0420_0007,
        title: "Ys VIII — Lacrimosa of DANA".into(),
        class: Some("ys8.exe".into()),
        pid: Some(4242),
    }
}

fn framegen() -> FrameGenStatus {
    FrameGenStatus {
        engine: FrameGenEngine::Native,
        configured: FrameGenMultiplier::Auto,
        state: FrameGenState::Active,
        active: Some(3),
        layer_installed: true,
        dll_found: true,
        native_available: true,
        env_locked: false,
        reason_code: None,
        reason: None,
    }
}

fn gpu() -> GpuInfo {
    GpuInfo {
        index: 0,
        name: "AMD Radeon RX 7800 XT".into(),
        vendor_id: 0x1002,
        device_id: 0x747e,
        discrete: true,
        driver: "radv Mesa 26.2.1".into(),
        fp16: true,
        external_fd: true,
        selected: true,
    }
}

fn session() -> SessionView {
    SessionView {
        window: window(),
        profile: Some("Ys VIII".into()),
        mode: Some("calidad".into()),
        kind: SessionKind::Fullscreen,
        src: Size {
            width: 1280,
            height: 720,
        },
        dst: Size {
            width: 2560,
            height: 1440,
        },
        chain: vec!["deband".into(), "CuNNy 4x12".into(), "lanczos".into()],
        started_at_ms: 1_793_000_000_123,
    }
}

fn commands() -> Vec<Command> {
    vec![
        Command::Hello {
            version: PROTOCOL_VERSION,
            client: "lirtup ctl".into(),
        },
        Command::Start {
            target: Target::Focused,
            kind: SessionKind::Fullscreen,
            delay_s: Some(3.0),
            profile: None,
        },
        Command::Start {
            target: Target::Window(0x0420_0007),
            kind: SessionKind::Windowed,
            delay_s: None,
            profile: Some("Ys VIII".into()),
        },
        Command::Start {
            target: Target::Title("mpv".into()),
            kind: SessionKind::Fullscreen,
            delay_s: Some(0.25),
            profile: None,
        },
        Command::Start {
            target: Target::Pid(4242),
            kind: SessionKind::Windowed,
            delay_s: None,
            profile: None,
        },
        Command::Stop,
        Command::Restart,
        Command::ReloadConfig,
        Command::SetDaemon {
            on: true,
            keep_after_exit: false,
        },
        Command::Hotkey {
            action: HotkeyAction::ToggleStats,
        },
        Command::Hotkey {
            action: HotkeyAction::StartWindowed,
        },
        Command::PickWindow,
        Command::ListEffects,
        Command::CompileEffect {
            name: "Anime4K/Anime4K_Upscale_GAN_x2_S".into(),
        },
        Command::ListDevices,
        Command::FrameGenProbe,
        Command::Bench {
            req: BenchRequest {
                src: Size {
                    width: 1280,
                    height: 720,
                },
                dst: Size {
                    width: 2560,
                    height: 1440,
                },
                mode: Some("calidad".into()),
                effects: vec!["FSR/FSR_EASU".into(), "CRT/CRT_Geom".into()],
                framegen: true,
                frames: 60,
            },
        },
        Command::CancelBench,
        Command::Diagnostics,
        Command::WatchGpuTimes { on: true },
        Command::Quit,
    ]
}

fn events() -> Vec<Event> {
    vec![
        Event::Snapshot(EngineSnapshot {
            protocol: PROTOCOL_VERSION,
            version: "2.0.0-alpha.0".into(),
            session: Some(session()),
            last_error: Some((Stage::Swapchain, "VK_ERROR_SURFACE_LOST_KHR".into())),
            daemon: DaemonStatus {
                on: true,
                keep_after_exit: true,
                watching: 3,
            },
            framegen: framegen(),
        }),
        Event::Snapshot(EngineSnapshot::default()),
        Event::SessionStarting { target: window() },
        Event::SessionStarting {
            target: WindowRef {
                xid: 1,
                title: String::new(),
                class: None,
                pid: None,
            },
        },
        Event::SessionStarted(session()),
        Event::SessionStopped {
            reason: StopReason::User,
        },
        Event::SessionStopped {
            reason: StopReason::WindowClosed,
        },
        Event::SessionStopped {
            reason: StopReason::Replaced,
        },
        Event::SessionStopped {
            reason: StopReason::Restart,
        },
        Event::SessionStopped {
            reason: StopReason::Quit,
        },
        Event::SessionStopped {
            reason: StopReason::Error,
        },
        Event::SessionError {
            stage: Stage::Config,
            message: "unknown profile \"nada\"".into(),
            retry: false,
        },
        Event::SessionError {
            stage: Stage::Window,
            message: "no window".into(),
            retry: true,
        },
        Event::SessionError {
            stage: Stage::Device,
            message: String::new(),
            retry: false,
        },
        Event::SessionError {
            stage: Stage::Capture,
            message: "x".into(),
            retry: false,
        },
        Event::SessionError {
            stage: Stage::Effects,
            message: "x".into(),
            retry: false,
        },
        Event::SessionError {
            stage: Stage::Overlay,
            message: "x".into(),
            retry: false,
        },
        Event::SessionError {
            stage: Stage::FrameGen,
            message: "x".into(),
            retry: false,
        },
        Event::SessionError {
            stage: Stage::Runtime,
            message: "line 1\nline 2".into(),
            retry: false,
        },
        Event::Stats(StatsSample {
            capture_fps: 59.9,
            present_fps: 179.5,
            generated_fps: 119.75,
            latency_ms: 21.5,
            latency_p95_ms: 30.25,
            gpu_ms: 4.125,
            stages: vec![
                StageTime {
                    name: "CuNNy 4x12".into(),
                    ms: 3.5,
                },
                StageTime {
                    name: "lanczos".into(),
                    ms: 0.625,
                },
            ],
            dirty_fraction: 0.5,
            tiles: Some(12),
            multiplier: Some(3),
            gpu_warning: Some(GpuWarning {
                budget_ms: 16.5,
                gpu_ms: 18.0,
                heaviest: vec![("CuNNy 8x32".into(), 15.5), ("bloom".into(), 1.25)],
            }),
            poll_fps: 120.0,
            render_ms: Some(1.5),
            latency_by_kind: vec![
                KindLatency {
                    kind: "real".into(),
                    mean_ms: 20.5,
                    max_ms: 31.0,
                },
                KindLatency {
                    kind: "generated".into(),
                    mean_ms: 12.25,
                    max_ms: 18.0,
                },
            ],
        }),
        Event::Stats(StatsSample::default()),
        Event::FrameGen(framegen()),
        Event::FrameGen(FrameGenStatus {
            engine: FrameGenEngine::Layer,
            configured: FrameGenMultiplier::Fixed(2),
            state: FrameGenState::NoLayer,
            active: None,
            layer_installed: false,
            dll_found: true,
            native_available: false,
            env_locked: true,
            reason_code: Some(FrameGenReason::LayerMissing),
            reason: Some("lsfg-vk is not installed".into()),
        }),
        Event::FrameGen(FrameGenStatus {
            state: FrameGenState::NoDll,
            ..FrameGenStatus::default()
        }),
        Event::FrameGen(FrameGenStatus {
            state: FrameGenState::External,
            ..FrameGenStatus::default()
        }),
        Event::FrameGen(FrameGenStatus {
            state: FrameGenState::NoNative,
            reason_code: Some(FrameGenReason::EngineFailed),
            reason: Some("lsfg: present failed".into()),
            ..FrameGenStatus::default()
        }),
        Event::DaemonState {
            on: false,
            keep_after_exit: true,
            watching: 0,
        },
        Event::DaemonMatch {
            window: window(),
            profile: "Ys VIII".into(),
        },
        Event::MenuBarDetected {
            profile: Some("Ys VIII".into()),
            crop: Crop {
                top: 24,
                bottom: 0,
                left: 0,
                right: 0,
            },
        },
        Event::MenuBarDetected {
            profile: None,
            crop: Crop::default(),
        },
        Event::WindowPicked {
            window: Some(window()),
        },
        Event::WindowPicked { window: None },
        Event::Effects {
            effects: vec![
                EffectInfo {
                    name: "FSR/FSR_RCAS".into(),
                    sort_name: "FSR RCAS".into(),
                    passes: 1,
                    params: vec![EffectParamInfo {
                        name: "sharpness".into(),
                        label: "Sharpness".into(),
                        kind: ParamKind::Float,
                        default: 0.87,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                    }],
                    verified: true,
                    origin: EffectOrigin::Builtin,
                    fp16: true,
                    error: None,
                },
                EffectInfo {
                    name: "mine/broken".into(),
                    sort_name: "mine/broken".into(),
                    passes: 0,
                    params: vec![EffectParamInfo {
                        name: "steps".into(),
                        label: String::new(),
                        kind: ParamKind::Int,
                        default: 2.0,
                        min: 1.0,
                        max: 8.0,
                        step: 1.0,
                    }],
                    verified: false,
                    origin: EffectOrigin::User,
                    fp16: false,
                    error: Some("line 3: //!PASS without a number".into()),
                },
            ],
        },
        Event::Effects { effects: vec![] },
        Event::EffectCompile {
            name: "FSR/FSR_EASU".into(),
            done: false,
            error: None,
            session: false,
        },
        Event::EffectCompile {
            name: "FSR/FSR_EASU".into(),
            done: true,
            error: Some("dxc: error: undeclared identifier".into()),
            session: true,
        },
        Event::Devices {
            monitors: vec![
                MonitorInfo {
                    index: 0,
                    name: "DP-1".into(),
                    x: 0,
                    y: 0,
                    width: 2560,
                    height: 1440,
                    primary: true,
                    refresh_hz: Some(164.5),
                },
                MonitorInfo {
                    index: 1,
                    name: "HDMI-A-1".into(),
                    x: -1920,
                    y: 200,
                    width: 1920,
                    height: 1080,
                    primary: false,
                    refresh_hz: None,
                },
            ],
            gpus: vec![gpu()],
        },
        Event::BenchProgress { done: 3, total: 17 },
        Event::BenchResult(BenchReport {
            gpu: "AMD Radeon RX 7800 XT:4098:29822".into(),
            entries: vec![
                BenchEntry {
                    kind: BenchKind::Native,
                    name: "4x12".into(),
                    ms: Some(2.5),
                    p95: Some(2.75),
                    frames: 100,
                    error: None,
                },
                BenchEntry {
                    kind: BenchKind::Magpiefx,
                    name: "FSR/FSR_EASU".into(),
                    ms: None,
                    p95: None,
                    frames: 0,
                    error: Some("does not compile".into()),
                },
                BenchEntry {
                    kind: BenchKind::Framegen,
                    name: "x2-flow1.0-quality".into(),
                    ms: Some(1.5),
                    p95: None,
                    frames: 40,
                    error: None,
                },
            ],
            cancelled: true,
        }),
        Event::Diagnostics(DiagReport {
            version: "2.0.0-alpha.0".into(),
            gpus: vec![gpu()],
            vulkan: Some("1.4.321".into()),
            display: "X11".into(),
            framegen: framegen(),
            lossless_dll: Some("/games/Lossless Scaling/Lossless.dll".into()),
            dxc: Some("dxcompiler.so: 1.8.2505".into()),
            glslang: None,
            config_dir: "/home/u/.config/linux-rt-upscaler".into(),
        }),
        Event::Diagnostics(DiagReport::default()),
        Event::Log {
            level: Level::Error,
            message: "device lost".into(),
        },
        Event::Log {
            level: Level::Warn,
            message: "w".into(),
        },
        Event::Log {
            level: Level::Info,
            message: "i".into(),
        },
        Event::Log {
            level: Level::Debug,
            message: "d".into(),
        },
    ]
}

/// One line, `\n`-terminated, with no other newline (the framing).
fn assert_one_line(line: &str) {
    assert!(line.ends_with('\n'), "{line:?}");
    assert_eq!(line.matches('\n').count(), 1, "{line:?}");
    assert!(line.starts_with("{\"v\":1,\"type\":"), "{line:?}");
}

#[test]
fn every_command_round_trips() {
    for command in commands() {
        let line = encode_line(&command);
        assert_one_line(&line);
        let back: Command = decode_line(&line).unwrap_or_else(|e| panic!("{line}: {e}"));
        assert_eq!(back, command, "{line}");
        // Without the newline too (`decode_line` takes either).
        let back: Command = decode_line(line.trim_end()).unwrap();
        assert_eq!(back, command);
    }
}

#[test]
fn every_event_round_trips() {
    for event in events() {
        let line = encode_line(&event);
        assert_one_line(&line);
        let back: Event = decode_line(&line).unwrap_or_else(|e| panic!("{line}: {e}"));
        assert_eq!(back, event, "{line}");
    }
}

#[test]
fn every_command_and_event_variant_is_covered() {
    // A new variant must be added to the lists above; matching here fails
    // to compile until it is.
    let mut seen_commands = std::collections::BTreeSet::new();
    for c in commands() {
        seen_commands.insert(match c {
            Command::Hello { .. } => "Hello",
            Command::Start { .. } => "Start",
            Command::Stop => "Stop",
            Command::Restart => "Restart",
            Command::ReloadConfig => "ReloadConfig",
            Command::SetDaemon { .. } => "SetDaemon",
            Command::Hotkey { .. } => "Hotkey",
            Command::PickWindow => "PickWindow",
            Command::ListEffects => "ListEffects",
            Command::CompileEffect { .. } => "CompileEffect",
            Command::ListDevices => "ListDevices",
            Command::FrameGenProbe => "FrameGenProbe",
            Command::Bench { .. } => "Bench",
            Command::CancelBench => "CancelBench",
            Command::Diagnostics => "Diagnostics",
            Command::WatchGpuTimes { .. } => "WatchGpuTimes",
            Command::Quit => "Quit",
        });
    }
    assert_eq!(seen_commands.len(), 17);

    let mut seen_events = std::collections::BTreeSet::new();
    for e in events() {
        seen_events.insert(match e {
            Event::Snapshot(_) => "Snapshot",
            Event::SessionStarting { .. } => "SessionStarting",
            Event::SessionStarted(_) => "SessionStarted",
            Event::SessionStopped { .. } => "SessionStopped",
            Event::SessionError { .. } => "SessionError",
            Event::Stats(_) => "Stats",
            Event::FrameGen(_) => "FrameGen",
            Event::DaemonState { .. } => "DaemonState",
            Event::DaemonMatch { .. } => "DaemonMatch",
            Event::MenuBarDetected { .. } => "MenuBarDetected",
            Event::WindowPicked { .. } => "WindowPicked",
            Event::Effects { .. } => "Effects",
            Event::EffectCompile { .. } => "EffectCompile",
            Event::Devices { .. } => "Devices",
            Event::BenchProgress { .. } => "BenchProgress",
            Event::BenchResult(_) => "BenchResult",
            Event::Diagnostics(_) => "Diagnostics",
            Event::Log { .. } => "Log",
        });
    }
    assert_eq!(seen_events.len(), 18);
}

#[test]
fn wire_format_matches_the_docs() {
    // The lines of the crate docs and of `lirtup ctl` examples, written by hand.
    let start: Command = decode_line(
        r#"{"v":1,"type":"Start","target":"focused","kind":"fullscreen","delay_s":3.0,"profile":null}"#,
    )
    .unwrap();
    assert_eq!(
        start,
        Command::Start {
            target: Target::Focused,
            kind: SessionKind::Fullscreen,
            delay_s: Some(3.0),
            profile: None,
        }
    );
    let by_xid: Command = decode_line(
        r#"{"v":1,"type":"Start","target":{"window":69206023},"kind":"windowed","delay_s":null,"profile":"Ys VIII"}"#,
    )
    .unwrap();
    assert!(matches!(
        by_xid,
        Command::Start {
            target: Target::Window(69_206_023),
            kind: SessionKind::Windowed,
            ..
        }
    ));
    let stop: Command = decode_line("{\"v\":1,\"type\":\"Stop\"}\n").unwrap();
    assert_eq!(stop, Command::Stop);
    let hotkey: Command = decode_line(r#"{"v":1,"type":"Hotkey","action":"toggle_stats"}"#).unwrap();
    assert_eq!(
        hotkey,
        Command::Hotkey {
            action: HotkeyAction::ToggleStats
        }
    );
    // Field order does not matter, `v` may come last.
    let hello: Command = decode_line(r#"{"client":"socat","type":"Hello","version":1,"v":1}"#).unwrap();
    assert_eq!(
        hello,
        Command::Hello {
            version: 1,
            client: "socat".into()
        }
    );

    assert_eq!(encode_line(&Command::Quit), "{\"v\":1,\"type\":\"Quit\"}\n");
    assert_eq!(
        encode_line(&Event::SessionStopped {
            reason: StopReason::WindowClosed
        }),
        "{\"v\":1,\"type\":\"SessionStopped\",\"reason\":\"window_closed\"}\n"
    );
    assert_eq!(
        encode_line(&Event::BenchProgress { done: 1, total: 2 }),
        "{\"v\":1,\"type\":\"BenchProgress\",\"done\":1,\"total\":2}\n"
    );
}

#[test]
fn another_version_is_a_version_error() {
    let line = r#"{"v":2,"type":"Hello","version":2,"client":"lirtup-gui 3.0.0"}"#;
    match decode_line::<Command>(line) {
        Err(CodecError::Version { got, expected }) => {
            assert_eq!(got, 2);
            assert_eq!(expected, PROTOCOL_VERSION);
        }
        other => panic!("expected a version error, got {other:?}"),
    }
    // Same for events (a newer server).
    let line = r#"{"v":0,"type":"Stop"}"#;
    assert!(matches!(
        decode_line::<Event>(line),
        Err(CodecError::Version { got: 0, .. })
    ));
}

#[test]
fn bad_lines_are_json_errors() {
    for line in [
        "",
        "not json",
        r#"{"type":"Stop"}"#,
        r#"{"v":1}"#,
        r#"{"v":1,"type":"NoSuchCommand"}"#,
        r#"{"v":1,"type":"Start","target":"focused"}"#,
        r#"{"v":1,"type":"Hotkey","action":"fly"}"#,
        r#"{"v":"1","type":"Stop"}"#,
    ] {
        assert!(
            matches!(decode_line::<Command>(line), Err(CodecError::Json(_))),
            "{line:?} should not decode"
        );
    }
}

#[test]
fn socket_path_is_under_the_runtime_dir() {
    let path = socket_path();
    assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("lirtup.sock"));
    let parent = path.parent().unwrap();
    let dir = parent.file_name().and_then(|n| n.to_str()).unwrap();
    assert!(dir == "lirtup" || dir.starts_with("lirtup-"), "{path:?}");
}
