//! The error of the pipeline crate (reviewer). Every module returns it, so a
//! failure anywhere becomes one `Event::SessionError` with its stage.

use lirtup_proto::Stage;

/// Something went wrong in a session, a bench run or one of their threads.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// Vulkan (device, resources, submit, swapchain).
    #[error("GPU: {0}")]
    Gpu(#[from] lirtup_gpu::GpuError),
    /// X11 (capture, overlay, input).
    #[error("X11: {0}")]
    X11(#[from] lirtup_x11::X11Error),
    /// Native effects, SRCNN, scalers, OSD drawing.
    #[error("effects: {0}")]
    Effect(#[from] lirtup_effects::EffectError),
    /// MagpieFX parsing or compiling.
    #[error("MagpieFX: {0}")]
    Magpie(#[from] lirtup_magpiefx::MagpieError),
    /// Frame generation (lsfg).
    #[error("frame generation: {0}")]
    FrameGen(#[from] lirtup_fg::FgError),
    /// Files (screenshots, latency CSV, bench results).
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Anything else, with the stage it belongs to.
    #[error("{message}")]
    Other {
        /// Where it happened.
        stage: Stage,
        /// What happened.
        message: String,
    },
    /// Stopped on request (bench cancel, session stop during setup).
    #[error("cancelled")]
    Cancelled,
}

impl PipelineError {
    /// An error of `stage` with a message.
    pub fn other(stage: Stage, message: impl Into<String>) -> PipelineError {
        PipelineError::Other {
            stage,
            message: message.into(),
        }
    }

    /// The stage shown by the GUI (`Event::SessionError.stage`).
    ///
    /// The mapping is by source; a module that knows better (a GPU error
    /// while creating the swapchain is `Stage::Swapchain`) wraps it with
    /// [`PipelineError::other`].
    pub fn stage(&self) -> Stage {
        match self {
            PipelineError::Gpu(_) => Stage::Device,
            PipelineError::X11(_) => Stage::Capture,
            PipelineError::Effect(_) | PipelineError::Magpie(_) => Stage::Effects,
            PipelineError::FrameGen(_) => Stage::FrameGen,
            PipelineError::Io(_) | PipelineError::Cancelled => Stage::Runtime,
            PipelineError::Other { stage, .. } => *stage,
        }
    }
}

/// Result of the pipeline crate.
pub type Result<T, E = PipelineError> = std::result::Result<T, E>;
