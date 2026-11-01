//! Errors of the GPU layer.

/// Something failed in Vulkan or in this layer.
#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    /// `libvulkan.so.1` could not be loaded.
    #[error("cannot load Vulkan: {0}")]
    Loading(String),
    /// No adapter fits (or the requested index does not exist).
    #[error("no suitable Vulkan device: {0}")]
    NoDevice(String),
    /// A required extension or feature is missing.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// A Vulkan call failed.
    #[error("{call}: {result}")]
    Vk {
        /// The call (`vkCreateImage`, ...).
        call: &'static str,
        /// The result code.
        result: ash::vk::Result,
    },
    /// Device memory allocation failed.
    #[error("allocation failed: {0}")]
    Allocation(String),
    /// A fence or semaphore wait timed out.
    #[error("GPU wait timed out")]
    Timeout,
    /// The device was lost (driver reset); everything must be recreated.
    #[error("device lost")]
    DeviceLost,
    /// The swapchain is out of date (resize) and must be recreated.
    #[error("swapchain out of date")]
    OutOfDate,
    /// Bad arguments (sizes, formats, binding counts, ...).
    #[error("invalid argument: {0}")]
    Invalid(String),
}

/// Result of this crate.
pub type Result<T, E = GpuError> = std::result::Result<T, E>;
