//! `lirtup`: the command line of LirtUp Forge 2.0.
//!
//! - `lirtup run [OPTIONS] [-- PROGRAM ARGS...]`: scale a window, as 1.x
//!   `upscale` (same options).
//! - `lirtup serve`: the scaler and daemon for the GUI, on the Unix socket.
//! - `lirtup ctl <COMMAND>`: send one protocol command to a running
//!   `lirtup serve` and print the events (a test client, front K).
//! - `lirtup list <windows|models|effects|gpus|monitors>`: what 1.x
//!   `config/listing.py` prints.
//!
//! Front K writes the bodies in round 2.

// Round 0 stubs: the bodies are `todo!()`. Front K removes this.
#![allow(unused_variables)]

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use lirtup_config::cli::ConfigArgs;

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
        config: ConfigArgs,

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
    /// List windows, models, effects, GPUs or monitors.
    List {
        /// What to list.
        what: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        CliCommand::Run { config, program } => run(config, program),
        CliCommand::Serve { socket, daemon } => serve(socket, daemon),
        CliCommand::Ctl { command, follow } => ctl(&command, follow),
        CliCommand::List { what } => list(&what),
    }
}

fn run(config: ConfigArgs, program: Vec<String>) -> anyhow::Result<()> {
    todo!("front K")
}

fn serve(socket: Option<PathBuf>, daemon: bool) -> anyhow::Result<()> {
    todo!("front K")
}

fn ctl(command: &str, follow: bool) -> anyhow::Result<()> {
    todo!("front K")
}

fn list(what: &str) -> anyhow::Result<()> {
    todo!("front K")
}
