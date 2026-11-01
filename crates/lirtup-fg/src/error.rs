//! Errors of frame generation.

/// Frame generation could not be set up or failed while running.
#[derive(Debug, thiserror::Error)]
pub enum FgError {
    /// `lsfg-vk.dll` / `Lossless.dll` not found.
    #[error("{0} not found")]
    DllNotFound(&'static str),
    /// The lsfg-vk layer is not installed.
    #[error("the lsfg-vk layer is not installed")]
    NoLayer,
    /// An lsfg call failed: its code and `lsfg_last_error()`.
    #[error("lsfg error {code}: {message}")]
    Lsfg {
        /// Negative `LSFG_ERR_*` code.
        code: i32,
        /// Message.
        message: String,
    },
    /// Bad arguments (multiplier, fd counts, ...).
    #[error("invalid argument: {0}")]
    Invalid(String),
    /// I/O (writing `lsfg-vk.toml`, reading `libraryfolders.vdf`).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result of this crate.
pub type Result<T, E = FgError> = std::result::Result<T, E>;
