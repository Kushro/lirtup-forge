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

mod serve;

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

        /// Program to launch and scale, with its arguments (`lirtup run prog
        /// args`, as 1.x; `--` before it also works).
        #[arg(trailing_var_arg = true)]
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

        /// Exit once the last client has gone (lirtup-gui starts it so),
        /// unless the daemon keeps running after exit or a session runs.
        #[arg(long)]
        exit_with_clients: bool,
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
    /// Move the loose 1.x mode fields of config.yaml into scaling modes
    /// (backup in config.yaml.1x.bak).
    Migrate {
        /// Only print what would change.
        #[arg(long)]
        dry_run: bool,

        /// Config file (default: ~/.config/linux-rt-upscaler/config.yaml).
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbose, cli.quiet, &log_settings(&cli.command));
    let result = match cli.command {
        CliCommand::Run { config, program } => run(config, program),
        CliCommand::Serve {
            socket,
            daemon,
            config,
            exit_with_clients,
        } => serve::serve(socket, daemon, config, exit_with_clients),
        CliCommand::Ctl { command, follow } => ctl(&command, follow),
        CliCommand::Bench { request, json, gpu } => bench(&request, json, gpu.as_deref()),
        CliCommand::List { what } => list(&what),
        CliCommand::Migrate { dry_run, config } => migrate(dry_run, config),
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
            // 1.x: 2 for a configuration error, 1 for the rest.
            if e.chain()
                .any(|c| c.downcast_ref::<lirtup_config::ConfigError>().is_some())
                || e.chain().any(|c| {
                    c.downcast_ref::<lirtup_core::CoreError>()
                        .is_some_and(|core| matches!(core, lirtup_core::CoreError::Config(_)))
                })
            {
                ExitCode::from(EXIT_CONFIG)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

/// Exit code of a configuration error (1.x `cli.py`).
const EXIT_CONFIG: u8 = 2;

/// Where and how much to log beyond stderr: `--debug`, `--log-level` and
/// `--log-file` of `run`, else `log_level` and `log_file` of the top level of
/// `config.yaml` (1.x `config/logging.py`).
struct LogSettings {
    level: Option<String>,
    file: Option<PathBuf>,
}

fn log_settings(command: &CliCommand) -> LogSettings {
    let (config_path, args) = match command {
        CliCommand::Run { config, .. } => (config.config.clone(), Some(config)),
        CliCommand::Serve { config, .. } | CliCommand::Migrate { config, .. } => (config.clone(), None),
        _ => (None, None),
    };
    let path = config_path.unwrap_or_else(lirtup_config::paths::config_path);
    let general = lirtup_config::ConfigFile::load(&path)
        .map(|(f, _)| f.general)
        .unwrap_or_default();
    let from_yaml = |key: &str| general.get(key).and_then(|v| v.as_str()).map(str::to_owned);
    let level = args
        .and_then(|a| {
            if a.options.debug {
                Some("DEBUG".to_owned())
            } else {
                a.options.log_level.clone()
            }
        })
        .or_else(|| from_yaml("log_level"));
    let file = args
        .and_then(|a| a.options.log_file.clone())
        .or_else(|| from_yaml("log_file").map(PathBuf::from))
        .map(|p| lirtup_config::paths::expand_tilde(&p));
    LogSettings { level, file }
}

/// `-q` → warnings, nothing → info, `-v` → debug, `-vv` → trace; `RUST_LOG`
/// wins. Never spawns a thread (`layer::prepare` comes later).
fn init_logging(verbose: u8, quiet: bool, settings: &LogSettings) {
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;
    use tracing_subscriber::{EnvFilter, Layer as _, fmt};

    let flags = match (quiet, verbose) {
        (true, _) => None,
        (false, 0) => None,
        (false, 1) => Some("debug"),
        (false, _) => Some("trace"),
    };
    // `-v`/`-q` win over `log_level` (the 1.x names, upper case).
    let level = match (quiet, flags, settings.level.as_deref()) {
        (true, _, _) => "warn".to_owned(),
        (false, Some(f), _) => f.to_owned(),
        (false, None, Some(l)) => match l.to_ascii_lowercase().as_str() {
            "warning" => "warn".to_owned(),
            other => other.to_owned(),
        },
        (false, None, None) => "info".to_owned(),
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&level));
    let stderr = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_filter(filter);
    // The log file takes everything at DEBUG, with time (1.x).
    let file = settings.file.as_ref().and_then(|path| {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::OpenOptions::new().create(true).append(true).open(path) {
            Ok(f) => Some(
                fmt::layer()
                    .with_writer(std::sync::Mutex::new(f))
                    .with_ansi(false)
                    .with_filter(EnvFilter::new("debug")),
            ),
            Err(e) => {
                eprintln!("lirtup: cannot open the log file {}: {e}", path.display());
                None
            }
        }
    });
    tracing_subscriber::registry().with(stderr).with(file).init();
}

// ---- run -------------------------------------------------------------------

/// `program:` of the configuration (the top level, or the `-p` profile), as
/// 1.x launches it when the command line names none.
fn configured_program(config: &ConfigArgs) -> Vec<String> {
    use lirtup_config::resolve::{ProfileChoice, resolve};
    use lirtup_config::{ConfigFile, ModeStore, paths};

    let path = config.config.clone().unwrap_or_else(paths::config_path);
    let modes_dir = path
        .parent()
        .map(|d| d.join("modes"))
        .unwrap_or_else(paths::modes_dir);
    let (Ok((file, _)), Ok((modes, _))) = (ConfigFile::load(&path), ModeStore::load(&modes_dir)) else {
        return Vec::new();
    };
    let choice = match &config.profile {
        Some(name) => ProfileChoice::Named(name.clone()),
        None => ProfileChoice::None,
    };
    let overrides = config.overrides().unwrap_or_default();
    resolve(&file, &modes, &choice, &overrides)
        .ok()
        .and_then(|r| r.config.program)
        .unwrap_or_default()
}

fn run(config: Box<ConfigArgs>, program: Vec<String>) -> anyhow::Result<ExitCode> {
    let program = if program.is_empty() {
        configured_program(&config)
    } else {
        program
    };
    // The program first, with the user's environment (X11 forced, as 1.x):
    // `Engine::new` sets the lsfg-vk variables for this process only. It is
    // terminated when the run ends (`Launched` drops it).
    // 1.x order (`acquisition.py`): a title or a title regex wins over
    // `--select` and over the program, which is then not launched.
    let by_title = config.options.target_title.is_some() || config.options.target_title_regex.is_some();
    if by_title && !program.is_empty() {
        tracing::warn!("a target title was given: {} is not launched", program[0]);
    }
    let launched = match program
        .split_first()
        .filter(|_| !by_title && !config.options.select)
    {
        Some((exe, args)) => {
            let child =
                lirtup_core::lifecycle::launch(exe, args).with_context(|| format!("cannot start {exe}"))?;
            tracing::info!("started {exe} (pid {})", child.id());
            Some(child)
        }
        None => None,
    };
    let launched_pid = launched.as_ref().map(lirtup_core::lifecycle::Launched::id);
    let launched = Arc::new(std::sync::Mutex::new(launched));

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
        return serve::daemon_foreground(engine);
    }

    let (target, delay_s) = if let Some(regex) = base.target_title_regex.as_deref() {
        let Some(window) = lirtup_core::listing::find_window(None, Some(regex))? else {
            eprintln!("lirtup: no window title matches {regex:?}");
            return Ok(ExitCode::FAILURE);
        };
        (Target::Window(window.xid), None)
    } else if let Some(title) = base.target_title.clone() {
        (Target::Title(title), None)
    } else if base.select {
        match select_window()? {
            Some(xid) => (Target::Window(xid), None),
            None => return Ok(ExitCode::SUCCESS),
        }
    } else if let Some(pid) = launched_pid {
        (Target::Pid(pid), None)
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
        let launched = Arc::clone(&launched);
        lirtup_core::signals::on_terminate(move || {
            engine.shutdown(STOP_TIMEOUT);
            // `exit` runs no destructors: terminate the program here.
            drop(launched.lock().ok().and_then(|mut l| l.take()));
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
    let mut config_failed = false;
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
            Event::SessionError { stage, .. } => {
                failed = true;
                config_failed |= stage == lirtup_proto::Stage::Config;
            }
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
    drop(launched.lock().ok().and_then(|mut l| l.take()));
    Ok(if config_failed {
        ExitCode::from(EXIT_CONFIG)
    } else if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// `--select`: number the windows and read a choice from stdin.
fn select_window() -> anyhow::Result<Option<u32>> {
    // As 1.x (`acquisition.py`): sorted by title, numbered from 0, `q`
    // cancels, a bad answer asks again.
    let mut windows = lirtup_core::listing::windows()?;
    anyhow::ensure!(!windows.is_empty(), "no windows to scale");
    windows.sort_by_key(|w| w.title.to_lowercase());
    for (i, w) in windows.iter().enumerate() {
        eprintln!("{i:>3}  {}x{}  {}", w.rect.width, w.rect.height, w.title);
    }
    loop {
        eprint!("Window to scale [0-{}, q to cancel]: ", windows.len() - 1);
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let answer = line.trim();
        if answer.eq_ignore_ascii_case("q") {
            return Ok(None);
        }
        match answer.parse::<usize>().ok().and_then(|n| windows.get(n)) {
            Some(window) => return Ok(Some(window.xid)),
            None => eprintln!("lirtup: {answer:?} is not a window number"),
        }
    }
}

// ---- migrate ---------------------------------------------------------------

fn migrate(dry_run: bool, config: Option<PathBuf>) -> anyhow::Result<ExitCode> {
    use lirtup_config::{ConfigFile, ModeStore, migrate, paths};

    let path = config.unwrap_or_else(paths::config_path);
    let modes_dir = path
        .parent()
        .map(|d| d.join("modes"))
        .unwrap_or_else(paths::modes_dir);
    let (mut file, _) = ConfigFile::load(&path)?;
    let (mut modes, _) = ModeStore::load(&modes_dir)?;
    let plan = migrate::plan(&file, &modes);
    if plan.is_empty() {
        println!("nothing to migrate: {} has no loose mode fields", path.display());
        return Ok(ExitCode::SUCCESS);
    }
    for (id, raw) in &plan.modes {
        println!("mode {id}: {} fields", raw.len().saturating_sub(1));
    }
    if let Some(top) = &plan.top_mode {
        println!("top level: mode {top}");
    }
    for (profile, mode) in &plan.profiles {
        println!("profile {profile}: {}", mode.as_deref().unwrap_or("(inherits)"));
    }
    if dry_run {
        return Ok(ExitCode::SUCCESS);
    }
    let backup = migrate::apply(&mut file, &mut modes, &plan)?;
    println!(
        "{} fields moved; backup in {}",
        plan.moved_fields,
        backup.display()
    );
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
        | Command::PauseHotkeys { .. }
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
    if let Err(e) = bench::save_results(
        &bench::results_path(),
        &report,
        request.src,
        request.dst,
        selector.as_deref(),
    ) {
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
