//! Vulkan instance and adapter selection (`vk_instance.cpp`, and
//! `VulkanContext` in `vulkan/__init__.py`).

use std::sync::Arc;

use crate::{Device, Result};

/// How to create the instance.
#[derive(Debug, Clone)]
pub struct InstanceOptions {
    /// Application name for `VkApplicationInfo` (the lsfg-vk layer matches on
    /// the process name, not on this).
    pub app_name: String,
    /// Enable `VK_EXT_debug_utils` and the validation layer when present;
    /// messages are kept and logged ([`Device::debug_messages`]).
    pub debug: bool,
}

impl Default for InstanceOptions {
    fn default() -> Self {
        InstanceOptions {
            app_name: "lirtup".into(),
            debug: false,
        }
    }
}

/// Kind of physical device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterKind {
    /// Discrete GPU.
    Discrete,
    /// Integrated GPU.
    Integrated,
    /// Virtual GPU.
    Virtual,
    /// CPU (llvmpipe, lavapipe).
    Cpu,
    /// Anything else.
    Other,
}

/// What the app needs to know about one physical device.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterInfo {
    /// Index in [`Instance::adapters`].
    pub index: usize,
    /// `deviceName`.
    pub name: String,
    /// PCI vendor id (lsfg needs it).
    pub vendor_id: u32,
    /// PCI device id (lsfg needs it).
    pub device_id: u32,
    /// Driver name and version (`VkPhysicalDeviceDriverProperties`), for
    /// Diagnostics.
    pub driver: String,
    /// `apiVersion`, as `major.minor.patch`.
    pub api_version: String,
    /// Kind.
    pub kind: AdapterKind,
    /// Device-local memory in bytes.
    pub dedicated_video_memory: u64,
    /// Host-visible device-local memory in bytes (ReBAR).
    pub dedicated_system_memory: u64,
    /// Host memory usable by the device in bytes.
    pub shared_system_memory: u64,
    /// `shaderFloat16` (CuNNy and MagpieFX FP16 variants).
    pub supports_fp16: bool,
    /// `VK_KHR_external_memory_fd` and `VK_KHR_external_semaphore_fd` (the
    /// native frame generation engine needs both).
    pub supports_external_fd: bool,
    /// `robustImageAccess` (Vulkan 1.3 or `VK_EXT_image_robustness`).
    pub robust_image_access: bool,
    /// Nanoseconds per timestamp tick, `None` if compute timestamps are not
    /// supported.
    pub timestamp_period_ns: Option<f32>,
}

/// The loaded Vulkan library and instance.
pub struct Instance {
    _private: (),
}

impl Instance {
    /// Load `libvulkan`, create the instance (`VK_KHR_surface`,
    /// `VK_KHR_xcb_surface`, optional debug utils) and enumerate adapters.
    pub fn new(options: &InstanceOptions) -> Result<Arc<Instance>> {
        todo!("front G")
    }

    /// The adapters, in enumeration order.
    pub fn adapters(&self) -> &[AdapterInfo] {
        todo!("front G")
    }

    /// The 1.x `get_best_device` choice: discrete first, then most video
    /// memory, then FP16.
    pub fn best_adapter(&self) -> Option<usize> {
        todo!("front G")
    }

    /// Resolve the `gpu:` option: a decimal index or a case-insensitive name
    /// substring. `None` or no match → [`Instance::best_adapter`].
    pub fn select_adapter(&self, selector: Option<&str>) -> Option<usize> {
        todo!("front G")
    }

    /// Create the logical device on an adapter (one compute+graphics queue
    /// that can present, the 1.x extensions and features, a 128-entry
    /// timestamp pool when supported).
    pub fn create_device(self: &Arc<Self>, adapter: usize) -> Result<Arc<Device>> {
        todo!("front G")
    }

    /// The `ash` entry and instance, for code that needs raw calls.
    pub fn raw(&self) -> (&ash::Entry, &ash::Instance) {
        todo!("front G")
    }
}
