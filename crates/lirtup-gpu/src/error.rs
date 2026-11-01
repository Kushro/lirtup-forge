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

impl GpuError {
    /// Map a failed Vulkan call: `ERROR_DEVICE_LOST` becomes
    /// [`GpuError::DeviceLost`], `ERROR_OUT_OF_DATE_KHR` becomes
    /// [`GpuError::OutOfDate`], `TIMEOUT` becomes [`GpuError::Timeout`].
    pub(crate) fn vk(call: &'static str, result: ash::vk::Result) -> GpuError {
        match result {
            ash::vk::Result::ERROR_DEVICE_LOST => GpuError::DeviceLost,
            ash::vk::Result::ERROR_OUT_OF_DATE_KHR => GpuError::OutOfDate,
            ash::vk::Result::TIMEOUT => GpuError::Timeout,
            result => GpuError::Vk { call, result },
        }
    }
}

/// `.map_err(vk_err("vkCreateImage"))`.
pub(crate) fn vk_err(call: &'static str) -> impl Fn(ash::vk::Result) -> GpuError {
    move |result| GpuError::vk(call, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk;

    #[test]
    fn special_results() {
        assert!(matches!(
            GpuError::vk("x", vk::Result::ERROR_DEVICE_LOST),
            GpuError::DeviceLost
        ));
        assert!(matches!(
            GpuError::vk("x", vk::Result::ERROR_OUT_OF_DATE_KHR),
            GpuError::OutOfDate
        ));
        assert!(matches!(
            GpuError::vk("x", vk::Result::TIMEOUT),
            GpuError::Timeout
        ));
        assert!(matches!(
            GpuError::vk("vkCreateImage", vk::Result::ERROR_OUT_OF_DEVICE_MEMORY),
            GpuError::Vk {
                call: "vkCreateImage",
                ..
            }
        ));
    }
}
