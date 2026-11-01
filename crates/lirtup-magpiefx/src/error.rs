//! Errors of MagpieFX.

/// An effect could not be found, parsed, compiled or run.
#[derive(Debug, thiserror::Error)]
pub enum MagpieError {
    /// No effect with that name or path.
    #[error("effect {0:?} not found")]
    NotFound(String),
    /// A directive or expression is malformed (`MagpieFXError` in 1.x).
    #[error("{effect}: {message}")]
    Parse {
        /// Effect name.
        effect: String,
        /// What is wrong (with the line when known).
        message: String,
    },
    /// No HLSL compiler (`dxc`, `glslangValidator`) found.
    #[error("no HLSL compiler found (install dxc or glslang)")]
    NoCompiler,
    /// The compiler failed.
    #[error("{effect} pass {pass}: {compiler} failed: {log}")]
    Compile {
        /// Effect name.
        effect: String,
        /// Pass (1-based).
        pass: u32,
        /// `dxc` or `glslangValidator`.
        compiler: String,
        /// Compiler output.
        log: String,
    },
    /// A `.dds` (or other) lookup texture could not be read.
    #[error("texture {path}: {message}")]
    Texture {
        /// File.
        path: String,
        /// What is wrong.
        message: String,
    },
    /// I/O.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// GPU or pass error.
    #[error(transparent)]
    Effect(#[from] lirtup_effects::EffectError),
}

/// Result of this crate.
pub type Result<T, E = MagpieError> = std::result::Result<T, E>;
