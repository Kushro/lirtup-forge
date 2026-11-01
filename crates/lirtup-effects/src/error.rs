//! Errors of the passes.

/// A pass could not be created or recorded.
#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    /// GPU error.
    #[error(transparent)]
    Gpu(#[from] lirtup_gpu::GpuError),
    /// A model or shader asset is missing or malformed (`model.json`).
    #[error("asset {name}: {message}")]
    Asset {
        /// Asset name.
        name: String,
        /// What is wrong.
        message: String,
    },
    /// The device lacks something the pass needs (FP16 variant, ...).
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// A font could not be loaded.
    #[error("font: {0}")]
    Font(String),
}

/// Result of this crate.
pub type Result<T, E = EffectError> = std::result::Result<T, E>;
