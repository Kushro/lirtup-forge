//! Layering of defaults, file, mode, profile and command line into a
//! [`Config`] (the order is in the crate docs).

use crate::{Config, ConfigError, ConfigFile, ConfigWarning, Mapping, ModeStore};

/// Which profile to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileChoice {
    /// Only the top-level options.
    None,
    /// A profile by name (`-p`, the GUI, the daemon); unknown names are an
    /// error.
    Named(String),
    /// The first profile whose `match` fits this window, if any.
    Auto {
        /// Window title.
        title: String,
        /// Window width.
        width: u32,
        /// Window height.
        height: u32,
    },
}

/// What [`resolve`] produced.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The final config.
    pub config: Config,
    /// Profile applied, if any.
    pub profile: Option<String>,
    /// Mode applied, if any.
    pub mode: Option<String>,
    /// Everything ignored or corrected on the way.
    pub warnings: Vec<ConfigWarning>,
}

/// Merge `top` over `base` key by key; `hotkeys` merges one level deeper.
pub fn merge_layer(base: &mut Mapping, top: &Mapping) {
    todo!("front C")
}

/// Build the config of one session.
///
/// `cli` holds the command-line overrides ([`crate::cli::ConfigArgs::overrides`];
/// empty for the GUI and the daemon).
pub fn resolve(
    file: &ConfigFile,
    modes: &ModeStore,
    profile: &ProfileChoice,
    cli: &Mapping,
) -> Result<Resolved, ConfigError> {
    todo!("front C")
}
