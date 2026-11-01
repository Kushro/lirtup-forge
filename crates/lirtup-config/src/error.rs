//! Errors and warnings of the configuration.

use std::path::PathBuf;

/// A configuration file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// I/O on a config file.
    #[error("{path}: {source}")]
    Io {
        /// File involved.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// The file is not valid YAML, or not a mapping at the top.
    #[error("{path}: {message}")]
    Yaml {
        /// File involved.
        path: PathBuf,
        /// Parser message.
        message: String,
    },
    /// A named profile does not exist.
    #[error("unknown profile {0:?}")]
    UnknownProfile(String),
    /// A named mode does not exist.
    #[error("unknown scaling mode {0:?}")]
    UnknownMode(String),
    /// A value that cannot be used (geometry, interval, hotkey, ...).
    #[error("{key}: {message}")]
    Invalid {
        /// YAML key (or `profiles.<name>.match.width`, ...).
        key: String,
        /// What is wrong.
        message: String,
    },
}

/// A value that was ignored or corrected while loading; the load goes on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigWarning {
    /// YAML key, dotted for nested ones (`hotkeys.zoom_in`).
    pub key: String,
    /// What happened (`"invalid value 'x', using the default"`).
    pub message: String,
}
