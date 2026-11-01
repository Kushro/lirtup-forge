//! Errors of the engine.

/// The engine or the server failed.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Configuration.
    #[error(transparent)]
    Config(#[from] lirtup_config::ConfigError),
    /// X11.
    #[error(transparent)]
    X11(#[from] lirtup_x11::X11Error),
    /// Frame generation.
    #[error(transparent)]
    FrameGen(#[from] lirtup_fg::FgError),
    /// Vulkan instance or device.
    #[error(transparent)]
    Gpu(#[from] lirtup_gpu::GpuError),
    /// MagpieFX catalog or compiler.
    #[error(transparent)]
    Magpie(#[from] lirtup_magpiefx::MagpieError),
    /// Socket or other I/O.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A line on the socket could not be decoded.
    #[error(transparent)]
    Protocol(#[from] lirtup_proto::CodecError),
    /// The server closed the connection.
    #[error("lirtup serve closed the connection")]
    Disconnected,
    /// Another `lirtup serve` already listens on the socket.
    #[error("another lirtup serve is running ({0})")]
    AlreadyRunning(std::path::PathBuf),
    /// The session failed at a stage.
    #[error("{stage:?}: {message}")]
    Session {
        /// Stage.
        stage: lirtup_proto::Stage,
        /// Message.
        message: String,
    },
}

/// Result of this crate.
pub type Result<T, E = CoreError> = std::result::Result<T, E>;
