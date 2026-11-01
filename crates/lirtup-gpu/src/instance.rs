//! Vulkan instance and adapter selection (`vk_instance.cpp`, and
//! `VulkanContext` in `vulkan/__init__.py`).

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, Mutex};

use ash::vk;

use crate::error::vk_err;
use crate::{Device, GpuError, Result};

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

/// Validation messages, written by the debug callback from any thread.
pub(crate) type DebugSink = Mutex<Vec<String>>;

/// The loaded Vulkan library and instance.
pub struct Instance {
    entry: ash::Entry,
    raw: ash::Instance,
    /// `apiVersion` the instance was created with (1.2 or 1.3).
    api_version: u32,
    surface: Option<ash::khr::surface::Instance>,
    xcb_surface: Option<ash::khr::xcb_surface::Instance>,
    debug_utils: Option<ash::ext::debug_utils::Instance>,
    messenger: vk::DebugUtilsMessengerEXT,
    /// Boxed so its address, passed to the callback, never moves.
    debug_sink: Box<DebugSink>,
    physical: Vec<vk::PhysicalDevice>,
    adapters: Vec<AdapterInfo>,
}

/// Collects warnings and errors (`vk_debug_callback` in `vk_instance.cpp`).
unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user_data: *mut c_void,
) -> vk::Bool32 {
    if data.is_null() || user_data.is_null() {
        return vk::FALSE;
    }
    // SAFETY: the loader passes valid callback data for the duration of the
    // call; `user_data` is the `DebugSink` boxed in `Instance`, which outlives
    // the messenger (destroyed first in `Drop`).
    let (message, sink) = unsafe {
        let data = &*data;
        let message = if data.p_message.is_null() {
            String::new()
        } else {
            CStr::from_ptr(data.p_message).to_string_lossy().into_owned()
        };
        (message, &*(user_data as *const DebugSink))
    };
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        tracing::error!(target: "vulkan", "{message}");
    } else {
        tracing::warn!(target: "vulkan", "{message}");
    }
    if let Ok(mut messages) = sink.lock() {
        messages.push(message);
    }
    vk::FALSE
}

fn has_extension(list: &[vk::ExtensionProperties], name: &CStr) -> bool {
    list.iter().any(|e| e.extension_name_as_c_str() == Ok(name))
}

fn version_string(v: u32) -> String {
    format!(
        "{}.{}.{}",
        vk::api_version_major(v),
        vk::api_version_minor(v),
        vk::api_version_patch(v)
    )
}

impl Instance {
    /// Load `libvulkan`, create the instance (`VK_KHR_surface`,
    /// `VK_KHR_xcb_surface`, optional debug utils) and enumerate adapters.
    pub fn new(options: &InstanceOptions) -> Result<Arc<Instance>> {
        // SAFETY: loading libvulkan runs its initializers; the loader is the
        // system one and the entry is kept alive as long as the instance.
        let entry = unsafe { ash::Entry::load() }.map_err(|e| GpuError::Loading(e.to_string()))?;

        // SAFETY: plain queries on a loaded entry.
        let (loader_version, extensions, layers) = unsafe {
            (
                entry
                    .try_enumerate_instance_version()
                    .ok()
                    .flatten()
                    .unwrap_or(vk::API_VERSION_1_0),
                entry
                    .enumerate_instance_extension_properties(None)
                    .map_err(vk_err("vkEnumerateInstanceExtensionProperties"))?,
                entry.enumerate_instance_layer_properties().unwrap_or_default(),
            )
        };
        if loader_version < vk::API_VERSION_1_2 {
            return Err(GpuError::Unsupported(format!(
                "Vulkan loader {} (1.2 needed)",
                version_string(loader_version)
            )));
        }
        // 1.x asked for 1.2; 1.3 gives robustImageAccess without the EXT.
        let api_version = loader_version.min(vk::API_VERSION_1_3);

        let mut enabled: Vec<*const c_char> = Vec::new();
        let has_surface = has_extension(&extensions, ash::khr::surface::NAME);
        let has_xcb = has_surface && has_extension(&extensions, ash::khr::xcb_surface::NAME);
        if has_surface {
            enabled.push(ash::khr::surface::NAME.as_ptr());
        }
        if has_xcb {
            enabled.push(ash::khr::xcb_surface::NAME.as_ptr());
        }
        let debug = options.debug && has_extension(&extensions, ash::ext::debug_utils::NAME);
        if debug {
            enabled.push(ash::ext::debug_utils::NAME.as_ptr());
        }
        let validation = c"VK_LAYER_KHRONOS_validation";
        let mut enabled_layers: Vec<*const c_char> = Vec::new();
        if options.debug {
            if layers.iter().any(|l| l.layer_name_as_c_str() == Ok(validation)) {
                enabled_layers.push(validation.as_ptr());
            } else {
                tracing::warn!("debug requested but VK_LAYER_KHRONOS_validation is not installed");
            }
        }

        let app_name = CString::new(options.app_name.replace('\0', "")).unwrap_or_default();
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 2, 0, 0))
            .engine_name(c"lirtup")
            .engine_version(vk::make_api_version(0, 2, 0, 0))
            .api_version(api_version);
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&enabled)
            .enabled_layer_names(&enabled_layers);
        // SAFETY: every pointer in `create_info` points to locals alive here.
        let raw = unsafe { entry.create_instance(&create_info, None) }.map_err(vk_err("vkCreateInstance"))?;

        let debug_sink: Box<DebugSink> = Box::new(Mutex::new(Vec::new()));
        let mut messenger = vk::DebugUtilsMessengerEXT::null();
        let debug_utils = if debug {
            let utils = ash::ext::debug_utils::Instance::new(&entry, &raw);
            let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(debug_callback))
                .user_data(&*debug_sink as *const DebugSink as *mut c_void);
            // SAFETY: the instance was created with VK_EXT_debug_utils; the
            // user data outlives the messenger (see `Drop`).
            match unsafe { utils.create_debug_utils_messenger(&info, None) } {
                Ok(m) => messenger = m,
                Err(e) => tracing::warn!("vkCreateDebugUtilsMessengerEXT: {e}"),
            }
            Some(utils)
        } else {
            None
        };

        let mut instance = Instance {
            surface: has_surface.then(|| ash::khr::surface::Instance::new(&entry, &raw)),
            xcb_surface: has_xcb.then(|| ash::khr::xcb_surface::Instance::new(&entry, &raw)),
            entry,
            raw,
            api_version,
            debug_utils,
            messenger,
            debug_sink,
            physical: Vec::new(),
            adapters: Vec::new(),
        };
        // SAFETY: valid instance.
        let physical = unsafe { instance.raw.enumerate_physical_devices() }
            .map_err(vk_err("vkEnumeratePhysicalDevices"))?;
        instance.adapters = physical
            .iter()
            .enumerate()
            .map(|(index, &pd)| instance.describe(index, pd))
            .collect();
        instance.physical = physical;
        for a in &instance.adapters {
            tracing::debug!(
                "Vulkan adapter {}: {} ({:?}, {} MiB, fp16 {}, external fd {}, robust {})",
                a.index,
                a.name,
                a.kind,
                a.dedicated_video_memory >> 20,
                a.supports_fp16,
                a.supports_external_fd,
                a.robust_image_access
            );
        }
        Ok(Arc::new(instance))
    }

    /// Fill an [`AdapterInfo`] (`vk_get_discovered_devices` plus what
    /// `vk_Device_get_initialized` checks).
    fn describe(&self, index: usize, pd: vk::PhysicalDevice) -> AdapterInfo {
        let caps = self.query_caps(pd);
        // SAFETY: `pd` comes from this instance.
        let (props, mem) = unsafe {
            (
                self.raw.get_physical_device_properties(pd),
                self.raw.get_physical_device_memory_properties(pd),
            )
        };
        let name = props
            .device_name_as_c_str()
            .map(|c| c.to_string_lossy().into_owned())
            .unwrap_or_default();
        let kind = match props.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => AdapterKind::Discrete,
            vk::PhysicalDeviceType::INTEGRATED_GPU => AdapterKind::Integrated,
            vk::PhysicalDeviceType::VIRTUAL_GPU => AdapterKind::Virtual,
            vk::PhysicalDeviceType::CPU => AdapterKind::Cpu,
            _ => AdapterKind::Other,
        };
        let (video, rebar, shared) = memory_totals(&mem);
        AdapterInfo {
            index,
            name,
            vendor_id: props.vendor_id,
            device_id: props.device_id,
            driver: caps.driver,
            api_version: version_string(props.api_version),
            kind,
            dedicated_video_memory: video,
            dedicated_system_memory: rebar,
            shared_system_memory: shared,
            supports_fp16: caps.fp16,
            supports_external_fd: caps.external_fd,
            robust_image_access: caps.robust_image_access,
            timestamp_period_ns: caps.timestamp_period,
        }
    }

    /// Optional features and extensions of one adapter.
    pub(crate) fn query_caps(&self, pd: vk::PhysicalDevice) -> AdapterCaps {
        // SAFETY: `pd` comes from this instance; the pNext chains point to
        // locals alive for each call.
        unsafe {
            let props = self.raw.get_physical_device_properties(pd);
            let device_api = props.api_version.min(self.api_version);
            let extensions = self
                .raw
                .enumerate_device_extension_properties(pd)
                .unwrap_or_default();

            let mut f12 = vk::PhysicalDeviceVulkan12Features::default();
            let mut features2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut f12);
            self.raw.get_physical_device_features2(pd, &mut features2);
            let fp16 = f12.shader_float16 == vk::TRUE;

            let robust_ext = has_extension(&extensions, ash::ext::image_robustness::NAME);
            let robust_core = device_api >= vk::API_VERSION_1_3;
            let mut robust_image_access = false;
            if robust_ext || robust_core {
                let mut robustness = vk::PhysicalDeviceImageRobustnessFeatures::default();
                let mut query = vk::PhysicalDeviceFeatures2::default().push_next(&mut robustness);
                self.raw.get_physical_device_features2(pd, &mut query);
                robust_image_access = robustness.robust_image_access == vk::TRUE;
            }

            let mut driver = vk::PhysicalDeviceDriverProperties::default();
            let mut props2 = vk::PhysicalDeviceProperties2::default().push_next(&mut driver);
            self.raw.get_physical_device_properties2(pd, &mut props2);
            let driver_name = driver
                .driver_name_as_c_str()
                .map(|c| c.to_string_lossy().into_owned())
                .unwrap_or_default();
            let driver_info = driver
                .driver_info_as_c_str()
                .map(|c| c.to_string_lossy().into_owned())
                .unwrap_or_default();
            let driver = match (driver_name.is_empty(), driver_info.is_empty()) {
                (true, _) => format!("driver {:#x}", props.driver_version),
                (false, true) => driver_name,
                (false, false) => format!("{driver_name} {driver_info}"),
            };

            let families = self.raw.get_physical_device_queue_family_properties(pd);
            let queue_family = pick_queue_family(&families);
            let timestamp_period = queue_family
                .filter(|&q| {
                    props.limits.timestamp_compute_and_graphics == vk::TRUE
                        && families[q as usize].timestamp_valid_bits > 0
                })
                .map(|_| props.limits.timestamp_period);

            AdapterCaps {
                device_api,
                fp16,
                external_fd: has_extension(&extensions, ash::khr::external_memory_fd::NAME)
                    && has_extension(&extensions, ash::khr::external_semaphore_fd::NAME),
                robust_image_access,
                robust_needs_extension: robust_image_access && !robust_core,
                swapchain: self.surface.is_some() && has_extension(&extensions, ash::khr::swapchain::NAME),
                queue_family,
                timestamp_period,
                timestamp_valid_bits: queue_family.map_or(0, |q| families[q as usize].timestamp_valid_bits),
                driver,
            }
        }
    }

    /// The adapters, in enumeration order.
    pub fn adapters(&self) -> &[AdapterInfo] {
        &self.adapters
    }

    /// The 1.x `get_best_device` choice: discrete first, then most video
    /// memory, then FP16.
    pub fn best_adapter(&self) -> Option<usize> {
        // 1.x honors VULKAN_DEVICE (an index) first.
        if let Some(i) = std::env::var("VULKAN_DEVICE")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            && i < self.adapters.len()
        {
            return Some(i);
        }
        best_of(&self.adapters)
    }

    /// Resolve the `gpu:` option: a decimal index or a case-insensitive name
    /// substring. `None` or no match → [`Instance::best_adapter`].
    pub fn select_adapter(&self, selector: Option<&str>) -> Option<usize> {
        match selector.map(str::trim).filter(|s| !s.is_empty()) {
            None => self.best_adapter(),
            Some(s) => match match_selector(&self.adapters, s) {
                Some(i) => Some(i),
                None => {
                    tracing::warn!("GPU '{s}' not found; falling back to automatic");
                    self.best_adapter()
                }
            },
        }
    }

    /// Create the logical device on an adapter (one compute+graphics queue
    /// that can present, the 1.x extensions and features, a 128-entry
    /// timestamp pool when supported).
    pub fn create_device(self: &Arc<Self>, adapter: usize) -> Result<Arc<Device>> {
        let pd = *self
            .physical
            .get(adapter)
            .ok_or_else(|| GpuError::NoDevice(format!("adapter {adapter} does not exist")))?;
        Device::new(self.clone(), self.adapters[adapter].clone(), pd)
    }

    /// The `ash` entry and instance, for code that needs raw calls.
    pub fn raw(&self) -> (&ash::Entry, &ash::Instance) {
        (&self.entry, &self.raw)
    }

    pub(crate) fn surface_fn(&self) -> Option<&ash::khr::surface::Instance> {
        self.surface.as_ref()
    }

    pub(crate) fn xcb_surface_fn(&self) -> Option<&ash::khr::xcb_surface::Instance> {
        self.xcb_surface.as_ref()
    }

    pub(crate) fn debug_enabled(&self) -> bool {
        self.debug_utils.is_some()
    }

    /// Drain the validation messages.
    pub(crate) fn take_debug_messages(&self) -> Vec<String> {
        self.debug_sink
            .lock()
            .map(|mut m| std::mem::take(&mut *m))
            .unwrap_or_default()
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: every device holds an `Arc<Instance>`, so none is left; the
        // messenger goes before the instance (and before `debug_sink`).
        unsafe {
            if let Some(utils) = &self.debug_utils
                && self.messenger != vk::DebugUtilsMessengerEXT::null()
            {
                utils.destroy_debug_utils_messenger(self.messenger, None);
            }
            self.raw.destroy_instance(None);
        }
    }
}

/// What [`Device`] creation needs to know about an adapter.
pub(crate) struct AdapterCaps {
    pub device_api: u32,
    pub fp16: bool,
    pub external_fd: bool,
    pub robust_image_access: bool,
    /// Enable `VK_EXT_image_robustness` (device older than 1.3).
    pub robust_needs_extension: bool,
    pub swapchain: bool,
    pub queue_family: Option<u32>,
    pub timestamp_period: Option<f32>,
    pub timestamp_valid_bits: u32,
    pub driver: String,
}

/// First family with graphics and compute (1.x took the first with
/// graphics; on desktop drivers that is the one that presents).
pub(crate) fn pick_queue_family(families: &[vk::QueueFamilyProperties]) -> Option<u32> {
    families
        .iter()
        .position(|f| {
            f.queue_flags
                .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
        })
        .map(|i| i as u32)
}

/// `(device local, host-visible device local, host)` heap sizes.
fn memory_totals(mem: &vk::PhysicalDeviceMemoryProperties) -> (u64, u64, u64) {
    let heaps = &mem.memory_heaps[..mem.memory_heap_count as usize];
    let types = &mem.memory_types[..mem.memory_type_count as usize];
    let mut video = 0;
    let mut rebar = 0;
    let mut shared = 0;
    for (i, heap) in heaps.iter().enumerate() {
        if heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL) {
            video += heap.size;
            let host_visible = types.iter().any(|t| {
                t.heap_index as usize == i
                    && t.property_flags.contains(
                        vk::MemoryPropertyFlags::DEVICE_LOCAL | vk::MemoryPropertyFlags::HOST_VISIBLE,
                    )
            });
            if host_visible {
                rebar += heap.size;
            }
        } else {
            shared += heap.size;
        }
    }
    (video, rebar, shared)
}

/// Hardware first, then discrete, then most video memory, then FP16; the
/// last of equals wins, like 1.x `sorted(...)[-1]`.
fn best_of(adapters: &[AdapterInfo]) -> Option<usize> {
    adapters
        .iter()
        .max_by_key(|a| {
            let hardware = !matches!(a.kind, AdapterKind::Cpu | AdapterKind::Other);
            (
                hardware,
                a.kind == AdapterKind::Discrete,
                a.dedicated_video_memory,
                a.supports_fp16,
            )
        })
        .map(|a| a.index)
}

/// A decimal index, else a case-insensitive name substring.
fn match_selector(adapters: &[AdapterInfo], selector: &str) -> Option<usize> {
    if let Ok(i) = selector.parse::<usize>() {
        return (i < adapters.len()).then_some(i);
    }
    let needle = selector.to_lowercase();
    adapters
        .iter()
        .find(|a| a.name.to_lowercase().contains(&needle))
        .map(|a| a.index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(index: usize, name: &str, kind: AdapterKind, vram_mib: u64, fp16: bool) -> AdapterInfo {
        AdapterInfo {
            index,
            name: name.into(),
            vendor_id: 0,
            device_id: 0,
            driver: String::new(),
            api_version: "1.3.0".into(),
            kind,
            dedicated_video_memory: vram_mib << 20,
            dedicated_system_memory: 0,
            shared_system_memory: 0,
            supports_fp16: fp16,
            supports_external_fd: true,
            robust_image_access: true,
            timestamp_period_ns: Some(1.0),
        }
    }

    fn sample() -> Vec<AdapterInfo> {
        vec![
            adapter(0, "llvmpipe (LLVM 19.1.0, 256 bits)", AdapterKind::Cpu, 0, true),
            adapter(
                1,
                "AMD Radeon Graphics (RADV RAPHAEL_MENDOCINO)",
                AdapterKind::Integrated,
                512,
                true,
            ),
            adapter(
                2,
                "AMD Radeon RX 7800 XT (RADV NAVI32)",
                AdapterKind::Discrete,
                16384,
                true,
            ),
        ]
    }

    #[test]
    fn best_prefers_discrete() {
        assert_eq!(best_of(&sample()), Some(2));
        assert_eq!(best_of(&sample()[..2]), Some(1));
        assert_eq!(best_of(&sample()[..1]), Some(0));
        assert_eq!(best_of(&[]), None);
    }

    #[test]
    fn best_breaks_ties_with_memory_then_fp16() {
        let a = vec![
            adapter(0, "a", AdapterKind::Discrete, 8192, true),
            adapter(1, "b", AdapterKind::Discrete, 4096, true),
            adapter(2, "c", AdapterKind::Discrete, 8192, false),
        ];
        assert_eq!(best_of(&a), Some(0));
    }

    #[test]
    fn selector_index_and_name() {
        let a = sample();
        assert_eq!(match_selector(&a, "1"), Some(1));
        assert_eq!(match_selector(&a, "7"), None);
        assert_eq!(match_selector(&a, "rx 7800"), Some(2));
        assert_eq!(match_selector(&a, "RADV"), Some(1));
        assert_eq!(match_selector(&a, "nvidia"), None);
    }

    #[test]
    fn queue_family_needs_graphics_and_compute() {
        let f = |flags| vk::QueueFamilyProperties {
            queue_flags: flags,
            queue_count: 1,
            ..Default::default()
        };
        let families = [
            f(vk::QueueFlags::TRANSFER),
            f(vk::QueueFlags::COMPUTE),
            f(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE | vk::QueueFlags::TRANSFER),
        ];
        assert_eq!(pick_queue_family(&families), Some(2));
        assert_eq!(pick_queue_family(&families[..2]), None);
    }

    #[test]
    fn heap_totals() {
        let mut mem = vk::PhysicalDeviceMemoryProperties {
            memory_heap_count: 2,
            memory_type_count: 3,
            ..Default::default()
        };
        mem.memory_heaps[0] = vk::MemoryHeap {
            size: 16 << 30,
            flags: vk::MemoryHeapFlags::DEVICE_LOCAL,
        };
        mem.memory_heaps[1] = vk::MemoryHeap {
            size: 32 << 30,
            flags: vk::MemoryHeapFlags::empty(),
        };
        mem.memory_types[0] = vk::MemoryType {
            property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            heap_index: 0,
        };
        mem.memory_types[1] = vk::MemoryType {
            property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL | vk::MemoryPropertyFlags::HOST_VISIBLE,
            heap_index: 0,
        };
        mem.memory_types[2] = vk::MemoryType {
            property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            heap_index: 1,
        };
        assert_eq!(memory_totals(&mem), (16 << 30, 16 << 30, 32 << 30));
    }

    #[test]
    fn versions() {
        assert_eq!(version_string(vk::make_api_version(0, 1, 3, 281)), "1.3.281");
    }
}
