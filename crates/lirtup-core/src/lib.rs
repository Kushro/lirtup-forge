//! The engine facade of LirtUp Forge and the server of `lirtup serve`.
//!
//! - [`Engine`]: owns the configuration (`Arc<Config>` in an `ArcSwap`), the
//!   pipeline thread, the hotkeys and the session state; takes
//!   [`proto::Command`]s and broadcasts [`proto::Event`]s. `lirtup run` uses
//!   it directly; `lirtup serve` puts the [`server`] in front of it.
//! - [`server`]: the Unix socket server (tokio), one task per client.
//! - [`daemon`]: matching windows against profiles (round 4, front Y).
//!
//! Front K writes this crate in round 2.

// Round 0 stubs: the bodies are `todo!()`. Front K removes this.
#![allow(unused_variables)]

pub use lirtup_proto as proto;

pub mod daemon;
pub mod engine;
pub mod error;
pub mod server;

pub use engine::{Engine, EngineOptions};
pub use error::{CoreError, Result};
