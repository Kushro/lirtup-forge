//! Configuration of LirtUp Forge.
//!
//! Reads and writes the same files as the 1.x Python app, without migration:
//!
//! - `~/.config/linux-rt-upscaler/config.yaml`: top-level options (the
//!   *Predeterminado* profile of the new GUI) plus `profiles:`.
//! - `~/.config/linux-rt-upscaler/modes/<id>.yaml`: scaling modes (new in 2.0,
//!   see [`mode`]).
//! - `~/.config/linux-rt-upscaler/gui-config.yaml`: GUI settings ([`gui`]).
//!
//! # Layering
//!
//! Options are layered as raw YAML mappings and only then turned into a typed
//! [`Config`] ([`resolve::resolve`]):
//!
//! 1. defaults ([`Config::default`]);
//! 2. top-level options of `config.yaml`;
//! 3. the mode named by `mode:` (profile first, then top level), if any;
//! 4. the options of the selected profile (loose mode fields here win over
//!    the mode, as in 6.6 of the analysis);
//! 5. command-line overrides ([`cli::ConfigArgs::overrides`]).
//!
//! `hotkeys` is merged key by key at every layer instead of replaced. Unknown
//! keys are kept (in [`Config::extra`] and in the raw mappings) so a save never
//! drops what another version wrote.

pub mod cli;
pub mod color;
pub mod error;
pub mod file;
pub mod gui;
pub mod hotkeys;
pub mod mode;
pub mod model;
pub mod paths;
pub mod profile;
pub mod resolve;
mod util;

pub use color::BackgroundColor;
pub use error::{ConfigError, ConfigWarning};
pub use file::ConfigFile;
pub use gui::GuiConfig;
pub use hotkeys::{Hotkey, HotkeyAction, Modifiers};
pub use mode::{Mode, ModeStore};
pub use model::*;
pub use profile::{Interval, Match, Profile};

/// A raw YAML mapping: one layer of options, as written in the file.
pub type Mapping = serde_yaml_ng::Mapping;

/// A raw YAML value.
pub type Value = serde_yaml_ng::Value;
