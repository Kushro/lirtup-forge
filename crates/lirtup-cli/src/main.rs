//! `lirtup`: the command line of LirtUp Forge 2.0.
//!
//! - `lirtup run [OPTIONS] [-- PROGRAM ARGS...]`: scale a window, as 1.x
//!   `upscale` (same options).
//! - `lirtup serve`: the scaler and daemon for the GUI, on the Unix socket.
//! - `lirtup ctl <COMMAND>`: send one protocol command to a running
//!   `lirtup serve` and print the events (a test client, front K).
//! - `lirtup list <windows|models|effects|gpus|monitors>`: what 1.x
//!   `config/listing.py` prints.
//! - `lirtup bench --json <REQUEST>`: «Medir en mi GPU» in a child process
//!   of `lirtup serve` (see `lirtup_pipeline::bench`): prints one
//!   `Envelope<Event>` line per progress step and a final `BenchResult`,
//!   saves `bench.json`, and stops cleanly on SIGTERM.
//!
//! Logs go to stderr (`-v`, `-q`, or `RUST_LOG`); stdout is for what a
//! subcommand prints (`ctl` and `bench --json` print protocol lines).

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use lirtup_config::cli::ConfigArgs;
use lirtup_config::{Model, OverlayMode};
use lirtup_core::broadcast::error::RecvError;
use lirtup_core::{Client, Engine, EngineOptions};
use lirtup_gpu::{Instance, InstanceOptions};
use lirtup_proto::{
    BenchRequest, Command, EffectOrigin, Event, Level, PROTOCOL_VERSION, SessionKind, StopReason, Target,
    encode_line, socket_path,
};

/// How long a session gets to stop on the way out.
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

/// LirtUp Forge: real-time upscaler for X11 windows.
#[derive(Debug, Parser)]
#[command(name = "lirtup", version)]
struct Cli {
    /// More logging (repeatable).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Less logging.
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Scale a window (the 1.x `upscale`).
    Run {
        #[command(flatten)]
        config: Box<ConfigArgs>,

        /// Program to launch and scale.
        #[arg(last = true)]
        program: Vec<String>,
    },
    /// Run the scaler and daemon for lirtup-gui.
    Serve {
        /// Socket path (default: $XDG_RUNTIME_DIR/lirtup/lirtup.sock).
        #[arg(long)]
        socket: Option<PathBuf>,

        /// Start with the daemon on.
        #[arg(long)]
        daemon: bool,

        /// Config file (default: ~/.config/linux-rt-upscaler/config.yaml).
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Send a command to a running `lirtup serve` (JSON of a `Command`, or a
    /// bare name such as `Stop`).
    Ctl {
        /// The command.
        command: String,

        /// Keep printing events until interrupted.
        #[arg(long)]
        follow: bool,
    },
    /// Measure effects and frame generation on this GPU (run by `lirtup
    /// serve` for the GUI's «Medir en mi GPU»).
    Bench {
        /// The request: JSON of a `lirtup_proto::BenchRequest`.
        request: String,

        /// Print protocol events (one JSON line each) instead of a table.
        #[arg(long)]
        json: bool,

        /// GPU index or name (default: the configured one).
        #[arg(long)]
        gpu: Option<String>,
    },
    /// List windows, models, effects, GPUs or monitors.
    List {
        /// What to list.
        what: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbose, cli.quiet);
    let result = match cli.command {
        CliCommand::Run { config, program } => run(config, program),
        CliCommand::Serve {
            socket,
            daemon,
            config,
        } => serve(socket, daemon, config),
        CliCommand::Ctl { command, follow } => ctl(&command, follow),
        CliCommand::Bench { request, json, gpu } => bench(&request, json, gpu.as_deref()),
        CliCommand::List { what } => list(&what),
    };
    match result {
        Ok(code) => code,
        // The reader of stdout went away (`lirtup ctl --follow hello | head`).
        Err(e)
            if e.chain().any(|c| {
                c.downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
            }) =>
        {
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!("{e:#}");
            eprintln!("lirtup: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// `-q` → warnings, nothing → info, `-v` → debug, `-vv` → trace; `RUST_LOG`
/// wins. Never spawns a thread (`layer::prepare` comes later).
fn init_logging(verbose: u8, quiet: bool) {
    use tracing_subscriber::EnvFilter;

    let level = match (quiet, verbose) {
        (true, _) => "warn",
        (false, 0) => "info",
        (false, 1) => "debug",
        (false, _) => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

// ---- run -------------------------------------------------------------------

fn run(config: Box<ConfigArgs>, program: Vec<String>) -> anyhow::Result<ExitCode> {
    // The program first, with the user's environment: `Engine::new` sets
    // the lsfg-vk variables for this process only.
    let launched = match program.split_first() {
        Some((exe, args)) => {
            let child = std::process::Command::new(exe)
                .args(args)
                .spawn()
                .with_context(|| format!("cannot start {exe}"))?;
            tracing::info!("started {exe} (pid {})", child.id());
            Some(child.id())
        }
        None => None,
    };

    let options = EngineOptions {
        config_path: config.config.clone(),
        modes_dir: None,
        cli_overrides: config.overrides()?,
        daemon: false,
        profile: config.profile.clone(),
    };
    // `layer::prepare` runs in here: nothing before this line starts a
    // thread or touches Vulkan.
    let engine = Arc::new(Engine::new(options)?);
    let base = engine.config();
    if base.daemon {
        anyhow::bail!("daemon mode is not implemented yet (round 4); use `lirtup serve --daemon` later");
    }

    let (target, delay_s) = if let Some(pid) = launched {
        (Target::Pid(pid), None)
    } else if base.select {
        (Target::Window(select_window()?), None)
    } else if let Some(regex) = base.target_title_regex.as_deref() {
        let window = lirtup_core::listing::find_window(None, Some(regex))?
            .with_context(|| format!("no window title matches {regex:?}"))?;
        (Target::Window(window.xid), None)
    } else if let Some(title) = base.target_title.clone() {
        (Target::Title(title), None)
    } else {
        let delay = base.target_delay.max(0.0) as f32;
        if delay > 0.0 {
            eprintln!("lirtup: focus the window to scale; taking it in {delay:.0} s");
        }
        (Target::Focused, Some(delay).filter(|d| *d > 0.0))
    };
    let kind = if base.overlay_mode == OverlayMode::Windowed {
        SessionKind::Windowed
    } else {
        SessionKind::Fullscreen
    };

    let mut events = engine.subscribe();
    {
        let engine = Arc::clone(&engine);
        lirtup_core::signals::on_terminate(move || {
            engine.shutdown(STOP_TIMEOUT);
            std::process::exit(130);
        })?;
    }
    engine.handle(Command::Start {
        target,
        kind,
        delay_s,
        profile: None,
    })?;

    let mut failed = false;
    loop {
        let event = match events.blocking_recv() {
            Ok(event) => event,
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => break,
        };
        match event {
            Event::SessionStarting { target } => {
                tracing::info!("window 0x{:08x} {:?}", target.xid, target.title);
            }
            Event::SessionStarted(view) => {
                tracing::info!(
                    "scaling {}x{} → {}x{} ({}{})",
                    view.src.width,
                    view.src.height,
                    view.dst.width,
                    view.dst.height,
                    view.chain.join(" → "),
                    view.profile.map(|p| format!(", profile {p}")).unwrap_or_default(),
                );
            }
            Event::SessionError { .. } => failed = true,
            Event::SessionStopped { reason } => {
                tracing::info!("stopped ({reason:?})");
                failed |= reason == StopReason::Error;
                break;
            }
            Event::MenuBarDetected { profile, crop } => {
                // `lirtup run` never writes YAML (the GUI does).
                tracing::info!(
                    "menu bar cropped ({} px); to keep it, set crop_top: {} in {}",
                    crop.top,
                    crop.top,
                    profile
                        .map(|p| format!("the profile {p:?}"))
                        .unwrap_or_else(|| "config.yaml".into()),
                );
            }
            Event::Stats(sample) => tracing::trace!(
                "{:.1} fps captured, {:.1} presented, {:.2} ms GPU",
                sample.capture_fps,
                sample.present_fps,
                sample.gpu_ms
            ),
            _ => {}
        }
    }
    engine.shutdown(STOP_TIMEOUT);
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// `--select`: number the windows and read a choice from stdin.
fn select_window() -> anyhow::Result<u32> {
    let windows = lirtup_core::listing::windows()?;
    anyhow::ensure!(!windows.is_empty(), "no windows to scale");
    for (i, w) in windows.iter().enumerate() {
        eprintln!("{:>3}  {}x{}  {}", i + 1, w.rect.width, w.rect.height, w.title);
    }
    eprint!("Window to scale [1-{}]: ", windows.len());
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let n: usize = line.trim().parse().context("not a number")?;
    let window = windows
        .get(n.wrapping_sub(1))
        .with_context(|| format!("no window {n}"))?;
    Ok(window.xid)
}

// ---- serve -----------------------------------------------------------------

fn serve(socket: Option<PathBuf>, daemon: bool, config: Option<PathBuf>) -> anyhow::Result<ExitCode> {
    let socket = socket.unwrap_or_else(socket_path);
    lirtup_core::server::ensure_not_running(&socket)?;
    // The engine first (it runs `layer::prepare`), the runtime after.
    let engine = Arc::new(Engine::new(EngineOptions {
        daemon,
        config_path: config,
        ..EngineOptions::default()
    })?);
    lirtup_core::server::run(engine, &socket)?;
    Ok(ExitCode::SUCCESS)
}

// ---- ctl -------------------------------------------------------------------

/// How long `ctl` waits for the answer of a command (`None`: until it comes).
fn answer_timeout(command: &Command) -> Option<Duration> {
    let secs = match command {
        Command::PickWindow | Command::Bench { .. } => return None,
        Command::Start { .. } | Command::Restart => 120,
        Command::CompileEffect { .. } => 300,
        Command::ListEffects | Command::Diagnostics | Command::ListDevices => 60,
        Command::Quit | Command::Stop => 15,
        Command::FrameGenProbe => 10,
        Command::Hello { .. }
        | Command::ReloadConfig
        | Command::SetDaemon { .. }
        | Command::Hotkey { .. }
        | Command::WatchGpuTimes { .. }
        | Command::CancelBench => 1,
    };
    Some(Duration::from_secs(secs))
}

/// Whether `event` answers `command` (`ctl` stops there unless `--follow`).
fn answers(command: &Command, event: &Event) -> bool {
    match command {
        Command::Start { .. } => matches!(event, Event::SessionStarted(_) | Event::SessionStopped { .. }),
        // Restart stops first (`Restart`), then starts again.
        Command::Restart => matches!(
            event,
            Event::SessionStarted(_)
                | Event::SessionStopped {
                    reason: StopReason::Error
                }
        ),
        Command::Stop => matches!(event, Event::SessionStopped { .. }),
        Command::PickWindow => matches!(event, Event::WindowPicked { .. }),
        Command::ListEffects => matches!(event, Event::Effects { .. }),
        Command::CompileEffect { name } => {
            matches!(event, Event::EffectCompile { name: n, done: true, session: false, .. } if n == name)
        }
        Command::ListDevices => matches!(event, Event::Devices { .. }),
        Command::FrameGenProbe => matches!(event, Event::FrameGen(_)),
        Command::Bench { .. } => matches!(event, Event::BenchResult(_)),
        Command::Diagnostics => matches!(event, Event::Diagnostics(_)),
        _ => false,
    }
}

fn ctl(command: &str, follow: bool) -> anyhow::Result<ExitCode> {
    let command = parse_command(command)?;
    let socket = socket_path();
    let mut client = Client::connect(&socket).with_context(|| {
        format!(
            "cannot connect to {} (is `lirtup serve` running?)",
            socket.display()
        )
    })?;
    let snapshot = client.hello("lirtup ctl")?;
    let mut out = std::io::stdout().lock();
    if snapshot.protocol != PROTOCOL_VERSION {
        eprintln!(
            "lirtup: the server speaks protocol {}, this lirtup {}",
            snapshot.protocol, PROTOCOL_VERSION
        );
    }
    if matches!(command, Command::Hello { .. }) {
        out.write_all(encode_line(&Event::Snapshot(snapshot)).as_bytes())?;
        if !follow {
            return Ok(ExitCode::SUCCESS);
        }
    } else {
        client.send(&command)?;
    }

    let deadline = answer_timeout(&command).map(|t| Instant::now() + t);
    let mut failed = false;
    loop {
        let event = match (follow, deadline) {
            (true, _) | (false, None) => client.recv()?,
            (false, Some(deadline)) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                client.recv_timeout(left)?
            }
        };
        let Some(event) = event else {
            if client.is_closed() || !follow {
                break;
            }
            continue;
        };
        out.write_all(encode_line(&event).as_bytes())?;
        out.flush()?;
        failed |= matches!(
            event,
            Event::Log {
                level: Level::Error,
                ..
            } | Event::SessionError { .. }
        );
        if !follow && answers(&command, &event) {
            break;
        }
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// A command from the `ctl` argument:
///
/// - JSON of a `Command` (`{"type":"Stop"}`; a `"v"` key is ignored);
/// - or a name and `key=value` fields: `stop`, `reload_config`,
///   `start window=0x4200007 kind=windowed delay_s=3`, `start title=mpv`,
///   `hotkey toggle_stats`, `compile_effect FSR/FSR_EASU`,
///   `set_daemon on=true keep_after_exit=false`. Names may be `CamelCase`,
///   `snake_case` or `kebab-case`; values are JSON when they parse, text
///   otherwise. `start` defaults to the focused window, fullscreen, now.
fn parse_command(text: &str) -> anyhow::Result<Command> {
    use serde_json::{Map, Value, json};

    let text = text.trim();
    if text.starts_with('{') {
        return serde_json::from_str(text).context("not a protocol command");
    }
    let mut words = text.split_whitespace();
    let name = words.next().context("empty command")?;
    let name: String = name
        .split(['_', '-'])
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect();
    // Bare names are matched ignoring case: `reloadconfig` → `ReloadConfig`.
    const NAMES: [&str; 16] = [
        "Hello",
        "Start",
        "Stop",
        "Restart",
        "ReloadConfig",
        "SetDaemon",
        "Hotkey",
        "PickWindow",
        "ListEffects",
        "CompileEffect",
        "ListDevices",
        "FrameGenProbe",
        "Bench",
        "CancelBench",
        "Diagnostics",
        "Quit",
    ];
    let name = NAMES
        .iter()
        .find(|n| n.eq_ignore_ascii_case(&name))
        .with_context(|| format!("unknown command {name:?} (one of {})", NAMES.join(", ")))?;

    let mut fields = Map::new();
    fields.insert("type".into(), json!(name));
    for word in words {
        let (key, value) = match word.split_once('=') {
            Some((k, v)) => (k.to_owned(), v),
            // One bare value: the main field of the command.
            None => match *name {
                "Hotkey" => ("action".to_owned(), word),
                "CompileEffect" => ("name".to_owned(), word),
                "Start" if matches!(word, "windowed" | "fullscreen") => ("kind".to_owned(), word),
                "Start" if word == "focused" => ("target".to_owned(), word),
                _ => anyhow::bail!("{word:?}: expected key=value"),
            },
        };
        let value = match key.as_str() {
            // Shorthands for `target`.
            "window" | "xid" => ("target", json!({ "window": parse_u32(value)? })),
            "pid" => ("target", json!({ "pid": parse_u32(value)? })),
            "title" => ("target", json!({ "title": value })),
            _ => {
                let parsed = serde_json::from_str::<Value>(value).unwrap_or_else(|_| json!(value));
                fields.insert(key, parsed);
                continue;
            }
        };
        fields.insert(value.0.into(), value.1);
    }
    match *name {
        "Hello" => {
            fields.entry("version").or_insert(json!(PROTOCOL_VERSION));
            fields.entry("client").or_insert(json!("lirtup ctl"));
        }
        "Start" => {
            fields.entry("target").or_insert(json!("focused"));
            fields.entry("kind").or_insert(json!("fullscreen"));
        }
        "SetDaemon" => {
            fields.entry("keep_after_exit").or_insert(json!(false));
        }
        _ => {}
    }
    serde_json::from_value(Value::Object(fields)).with_context(|| format!("bad {name} command"))
}

/// A decimal or `0x` hexadecimal number (window ids are printed in hex).
fn parse_u32(text: &str) -> anyhow::Result<u32> {
    let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.with_context(|| format!("{text:?} is not a number"))
}

// ---- bench -----------------------------------------------------------------

fn bench(request: &str, json: bool, gpu: Option<&str>) -> anyhow::Result<ExitCode> {
    use lirtup_pipeline::bench::{self, BenchProgress, BenchSettings};

    let request: BenchRequest = serde_json::from_str(request).context("bad bench request")?;
    // Before any thread and any Vulkan call: keeps the lsfg-vk layer out.
    let config = lirtup_core::prepare_headless(&EngineOptions::default())?;

    let instance = Instance::new(&InstanceOptions::default())?;
    let selector = gpu
        .map(str::to_owned)
        .or_else(|| lirtup_core::listing::gpu_selector(&config));
    let index = instance
        .select_adapter(selector.as_deref())
        .context("no Vulkan device")?;
    let device = instance.create_device(index)?;

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let cancel = Arc::clone(&cancel);
        lirtup_core::signals::on_terminate(move || cancel.store(true, Ordering::SeqCst))?;
    }

    let mut out = std::io::stdout().lock();
    let mut print = |event: &Event| {
        // A closed pipe (the parent went away) is not worth a panic.
        let _ = out.write_all(encode_line(event).as_bytes());
        let _ = out.flush();
    };
    let report = bench::run(
        &device,
        &request,
        &BenchSettings::default(),
        &cancel,
        &mut |progress| match progress {
            BenchProgress::Started { index, total, case } => {
                if json {
                    if index == 0 {
                        print(&Event::BenchProgress { done: 0, total });
                    }
                } else {
                    eprintln!("[{}/{total}] {:?} {}", index + 1, case.kind, case.name);
                }
            }
            BenchProgress::Done { done, total, entry } => {
                if json {
                    print(&Event::BenchProgress { done, total });
                } else {
                    eprintln!("        {}", entry_line(&entry));
                }
            }
        },
    )?;
    if let Err(e) = bench::save_results(&bench::results_path(), &report) {
        tracing::warn!("cannot save {}: {e}", bench::results_path().display());
    }
    if json {
        print(&Event::BenchResult(report));
    } else {
        println!(
            "GPU: {}{}",
            report.gpu,
            if report.cancelled { " (cancelled)" } else { "" }
        );
        for entry in &report.entries {
            println!(
                "{:<10} {:<40} {}",
                format!("{:?}", entry.kind),
                entry.name,
                entry_line(entry)
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn entry_line(entry: &lirtup_proto::BenchEntry) -> String {
    match (&entry.error, entry.ms) {
        (Some(e), _) => format!("error: {e}"),
        (None, Some(ms)) => format!(
            "{ms:.3} ms (p95 {}, {} frames)",
            entry.p95.map(|p| format!("{p:.3}")).unwrap_or_else(|| "-".into()),
            entry.frames
        ),
        (None, None) => "not measured".into(),
    }
}

// ---- list ------------------------------------------------------------------

fn list(what: &str) -> anyhow::Result<ExitCode> {
    match what {
        "windows" => {
            for w in lirtup_core::listing::windows()? {
                println!(
                    "0x{:08x}  {:>5}x{:<5}  {:>7}  {:<20}  {}",
                    w.xid,
                    w.rect.width,
                    w.rect.height,
                    w.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
                    w.class.as_ref().map(|c| c.class.as_str()).unwrap_or("-"),
                    w.title
                );
            }
        }
        "models" => {
            for model in Model::ALL {
                let default = if model == Model::default() {
                    "  (default)"
                } else {
                    ""
                };
                println!("{}{default}", model.as_str());
            }
        }
        "effects" => {
            let effects = lirtup_core::listing::effects();
            for e in &effects {
                let origin = match e.origin {
                    EffectOrigin::Builtin => "builtin",
                    EffectOrigin::User => "user",
                };
                let verified = if e.verified { "verified" } else { "untested" };
                let detail = match &e.error {
                    Some(error) => format!("error: {error}"),
                    None => format!("{} passes{}", e.passes, if e.fp16 { ", fp16" } else { "" }),
                };
                println!("{:<56} {origin:<8} {verified:<9} {detail}", e.name);
            }
            eprintln!("{} effects", effects.len());
        }
        "gpus" => {
            let config = lirtup_core::prepare_headless(&EngineOptions::default())?;
            let instance = Instance::new(&InstanceOptions::default())?;
            let selector = lirtup_core::listing::gpu_selector(&config);
            for gpu in lirtup_core::listing::gpus(&instance, selector.as_deref()) {
                println!(
                    "{}{} {}  [{:04x}:{:04x}] {}{}{}",
                    if gpu.selected { "*" } else { " " },
                    gpu.index,
                    gpu.name,
                    gpu.vendor_id,
                    gpu.device_id,
                    gpu.driver,
                    if gpu.fp16 { ", fp16" } else { "" },
                    if gpu.external_fd { ", external fd" } else { "" },
                );
            }
        }
        "monitors" => {
            for m in lirtup_core::listing::monitors_now()? {
                println!(
                    "{}{} {:<10} {}x{}+{}+{}{}",
                    if m.primary { "*" } else { " " },
                    m.index,
                    m.name,
                    m.width,
                    m.height,
                    m.x,
                    m.y,
                    m.refresh_hz.map(|hz| format!("  {hz:.2} Hz")).unwrap_or_default(),
                );
            }
        }
        other => anyhow::bail!("cannot list {other:?}: windows, models, effects, gpus or monitors"),
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lirtup_config::HotkeyAction;

    #[test]
    fn bare_names_in_any_case() {
        for text in ["Stop", "stop", " STOP "] {
            assert_eq!(parse_command(text).unwrap(), Command::Stop);
        }
        assert_eq!(parse_command("reload_config").unwrap(), Command::ReloadConfig);
        assert_eq!(parse_command("reload-config").unwrap(), Command::ReloadConfig);
        assert_eq!(parse_command("ReloadConfig").unwrap(), Command::ReloadConfig);
        assert_eq!(parse_command("framegenprobe").unwrap(), Command::FrameGenProbe);
        assert!(parse_command("fly").is_err());
        assert!(parse_command("").is_err());
    }

    #[test]
    fn start_defaults_and_shorthands() {
        assert_eq!(
            parse_command("start").unwrap(),
            Command::Start {
                target: Target::Focused,
                kind: SessionKind::Fullscreen,
                delay_s: None,
                profile: None,
            }
        );
        assert_eq!(
            parse_command("start window=0x4200007 windowed delay_s=3 profile=Ys").unwrap(),
            Command::Start {
                target: Target::Window(0x0420_0007),
                kind: SessionKind::Windowed,
                delay_s: Some(3.0),
                profile: Some("Ys".into()),
            }
        );
        assert!(matches!(
            parse_command("start title=mpv").unwrap(),
            Command::Start { target: Target::Title(t), .. } if t == "mpv"
        ));
        assert!(matches!(
            parse_command("start pid=4242 kind=windowed").unwrap(),
            Command::Start {
                target: Target::Pid(4242),
                kind: SessionKind::Windowed,
                ..
            }
        ));
        assert!(parse_command("start window=zz").is_err());
    }

    #[test]
    fn positional_fields() {
        assert_eq!(
            parse_command("hotkey toggle_stats").unwrap(),
            Command::Hotkey {
                action: HotkeyAction::ToggleStats
            }
        );
        assert_eq!(
            parse_command("compile_effect FSR/FSR_EASU").unwrap(),
            Command::CompileEffect {
                name: "FSR/FSR_EASU".into()
            }
        );
        assert_eq!(
            parse_command("set_daemon on=true").unwrap(),
            Command::SetDaemon {
                on: true,
                keep_after_exit: false
            }
        );
        assert!(parse_command("stop now").is_err());
    }

    #[test]
    fn json_commands() {
        assert_eq!(parse_command(r#"{"type":"Quit"}"#).unwrap(), Command::Quit);
        assert_eq!(parse_command(r#"{"v":1,"type":"Stop"}"#).unwrap(), Command::Stop);
        assert!(matches!(
            parse_command(
                r#"{"type":"Start","target":{"window":5},"kind":"windowed","delay_s":null,"profile":null}"#
            )
            .unwrap(),
            Command::Start {
                target: Target::Window(5),
                ..
            }
        ));
        assert!(parse_command(r#"{"type":"Nope"}"#).is_err());
    }

    #[test]
    fn hex_and_decimal_ids() {
        assert_eq!(parse_u32("0x10").unwrap(), 16);
        assert_eq!(parse_u32("0X1f").unwrap(), 31);
        assert_eq!(parse_u32("42").unwrap(), 42);
        assert!(parse_u32("-1").is_err());
    }

    #[test]
    fn answers_end_ctl() {
        let stop = Command::Stop;
        assert!(answers(
            &stop,
            &Event::SessionStopped {
                reason: StopReason::User
            }
        ));
        assert!(!answers(&stop, &Event::Effects { effects: vec![] }));
        assert_eq!(answer_timeout(&Command::PickWindow), None);
        let compile = Command::CompileEffect { name: "a".into() };
        let progress = |name: &str, done| Event::EffectCompile {
            name: name.into(),
            done,
            error: None,
            session: false,
        };
        assert!(!answers(&compile, &progress("a", false)));
        assert!(!answers(&compile, &progress("b", true)));
        assert!(answers(&compile, &progress("a", true)));
    }
}
