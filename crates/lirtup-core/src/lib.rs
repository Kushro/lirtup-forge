//! The engine facade of LirtUp Forge and the server of `lirtup serve`.
//!
//! - [`Engine`]: owns the configuration (`Arc<Config>` in an `ArcSwap`), the
//!   pipeline thread, the hotkeys and the session state; takes
//!   [`proto::Command`]s and broadcasts [`proto::Event`]s. `lirtup run` uses
//!   it directly; `lirtup serve` puts the [`server`] in front of it.
//! - [`server`]: the Unix socket server (tokio), one task per client.
//! - [`client`]: a blocking client of that socket (`lirtup ctl`, tests). The
//!   GUI has its own async one (`CoreLink`) and does not link this crate.
//! - [`listing`]: monitors, GPUs, effects and windows as the protocol names
//!   them (`ListDevices`, `ListEffects`, `lirtup list`).
//! - [`signals`]: SIGINT/SIGTERM for the blocking subcommands.
//! - [`daemon`]: matching windows against profiles (round 4, front Y).

pub use lirtup_proto as proto;

pub mod client;
pub mod daemon;
pub mod engine;
pub mod error;
pub mod listing;
pub mod server;
pub mod signals;

pub use client::Client;
pub use engine::{Engine, EngineOptions, prepare_process};
pub use error::{CoreError, Result};
