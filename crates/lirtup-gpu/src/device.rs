//! Logical device, queue mutex and resource creation (`vk_device.cpp`).

use std::ffi::c_char;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, MutexGuard};

use ash::vk;
use gpu_allocator::vulkan::{
    Allocation, AllocationCreateDesc, AllocationScheme, Allocator, AllocatorCreateDesc,
};

use crate::error::vk_err;
use crate::{
    AdapterInfo, Buffer, BufferDesc, CommandRecorder, ComputeDesc, ComputePipeline, ExternalSemaphore,
    ExternalTexture, GpuError, Instance, Result, Sampler, SamplerDesc, Submission, SubmitOptions, Swapchain,
    SwapchainDesc, Texture, TextureDesc,
};

/// Queries in each timestamp pool (the 1.x pool size).
pub(crate) const TIMESTAMP_QUERIES: u32 = 128;

/// Something a submission keeps alive until the GPU is done with it.
#[derive(Clone)]
#[allow(dead_code)] // held only for its `Drop`
pub(crate) enum Keep {
    Buffer(Buffer),
    Texture(Texture),
    Pipeline(ComputePipeline),
    Semaphore(ExternalSemaphore),
}

/// A command pool with one command buffer and its fence: what one recording
/// uses. Each has its own pool, so recordings on different threads never
/// share one (`VkCommandPool` is externally synchronized).
pub(crate) struct Frame {
    pub pool: vk::CommandPool,
    pub cmd: vk::CommandBuffer,
    pub fence: vk::Fence,
    pub query_pool: Option<vk::QueryPool>,
    pub keep: Vec<Keep>,
}

#[derive(Default)]
struct FramePool {
    free: Vec<Frame>,
    /// Submitted, dropped before their fence signalled.
    pending: Vec<Frame>,
}

/// Optional features that were enabled.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeviceCaps {
    /// `shaderStorageImageReadWithoutFormat`.
    pub read_without_format: bool,
    /// Descriptor indexing (1.x `supports_bindless`).
    pub bindless: bool,
    /// Nanoseconds per tick, when timestamps work on the queue.
    pub timestamp_period: Option<f32>,
    /// Mask of the valid timestamp bits.
    pub timestamp_mask: u64,
}

/// A Vulkan device with its single queue.
///
/// `Send + Sync`. Shared as `Arc<Device>`; every resource keeps one.
pub struct Device {
    instance: Arc<Instance>,
    info: AdapterInfo,
    physical: vk::PhysicalDevice,
    raw: ash::Device,
    queue: Mutex<vk::Queue>,
    queue_family: u32,
    allocator: Mutex<ManuallyDrop<Allocator>>,
    pipeline_cache: vk::PipelineCache,
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    caps: DeviceCaps,
    swapchain_fn: Option<ash::khr::swapchain::Device>,
    memory_fd_fn: Option<ash::khr::external_memory_fd::Device>,
    semaphore_fd_fn: Option<ash::khr::external_semaphore_fd::Device>,
    debug_utils_fn: Option<ash::ext::debug_utils::Device>,
    frames: Mutex<FramePool>,
    query_pools: Mutex<Vec<vk::QueryPool>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding one of these leaves plain handles behind; the
    // data is still usable.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Device {
    /// `vk_Device_get_initialized`: queue, extensions, features, allocator.
    pub(crate) fn new(
        instance: Arc<Instance>,
        mut info: AdapterInfo,
        pd: vk::PhysicalDevice,
    ) -> Result<Arc<Device>> {
        let caps = instance.query_caps(pd);
        if caps.device_api < vk::API_VERSION_1_2 {
            return Err(GpuError::Unsupported(format!(
                "{} supports Vulkan {} (1.2 needed)",
                info.name, info.api_version
            )));
        }
        let queue_family = caps
            .queue_family
            .ok_or_else(|| GpuError::Unsupported(format!("{}: no graphics+compute queue", info.name)))?;
        let (_, vk_instance) = instance.raw();

        // SAFETY: `pd` comes from `instance`; each query chain points to
        // locals alive for the call and is unlinked before reuse.
        let (core, mut f11, mut f12, mut robustness, memory_properties) = unsafe {
            let core = vk_instance.get_physical_device_features(pd);
            let mut f11 = vk::PhysicalDeviceVulkan11Features::default();
            let mut f12 = vk::PhysicalDeviceVulkan12Features::default();
            {
                let mut q = vk::PhysicalDeviceFeatures2::default().push_next(&mut f11);
                vk_instance.get_physical_device_features2(pd, &mut q);
            }
            {
                let mut q = vk::PhysicalDeviceFeatures2::default().push_next(&mut f12);
                vk_instance.get_physical_device_features2(pd, &mut q);
            }
            let mut robustness = vk::PhysicalDeviceImageRobustnessFeatures::default();
            if caps.robust_image_access {
                let mut q = vk::PhysicalDeviceFeatures2::default().push_next(&mut robustness);
                vk_instance.get_physical_device_features2(pd, &mut q);
            }
            f11.p_next = std::ptr::null_mut();
            f12.p_next = std::ptr::null_mut();
            robustness.p_next = std::ptr::null_mut();
            (
                core,
                f11,
                f12,
                robustness,
                vk_instance.get_physical_device_memory_properties(pd),
            )
        };
        let bindless = f12.descriptor_indexing == vk::TRUE
            && f12.shader_sampled_image_array_non_uniform_indexing == vk::TRUE
            && f12.shader_storage_image_array_non_uniform_indexing == vk::TRUE
            && f12.shader_uniform_buffer_array_non_uniform_indexing == vk::TRUE
            && f12.shader_storage_buffer_array_non_uniform_indexing == vk::TRUE;

        // What 1.x enables, plus 16-bit integers (dxc's min16 types).
        let enabled_core = vk::PhysicalDeviceFeatures::default()
            .shader_storage_image_read_without_format(
                core.shader_storage_image_read_without_format == vk::TRUE,
            )
            .shader_storage_image_write_without_format(
                core.shader_storage_image_write_without_format == vk::TRUE,
            )
            .shader_int16(core.shader_int16 == vk::TRUE);

        let mut extensions: Vec<*const c_char> = Vec::new();
        if caps.swapchain {
            extensions.push(ash::khr::swapchain::NAME.as_ptr());
        }
        if caps.external_fd {
            extensions.push(ash::khr::external_memory_fd::NAME.as_ptr());
            extensions.push(ash::khr::external_semaphore_fd::NAME.as_ptr());
        }
        if caps.robust_needs_extension {
            extensions.push(ash::ext::image_robustness::NAME.as_ptr());
        }

        let priorities = [1.0f32];
        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&priorities)];
        let mut create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_info)
            .enabled_extension_names(&extensions)
            .enabled_features(&enabled_core)
            .push_next(&mut f11)
            .push_next(&mut f12);
        if caps.robust_image_access {
            create_info = create_info.push_next(&mut robustness);
        }
        // SAFETY: the create info and its chain point to locals alive here;
        // only supported extensions and features are enabled.
        let raw =
            unsafe { vk_instance.create_device(pd, &create_info, None) }.map_err(vk_err("vkCreateDevice"))?;

        // From here on `raw` must be destroyed on error.
        let destroy = |raw: &ash::Device| {
            // SAFETY: nothing was created from the device yet.
            unsafe { raw.destroy_device(None) }
        };

        let allocator = match Allocator::new(&AllocatorCreateDesc {
            instance: vk_instance.clone(),
            device: raw.clone(),
            physical_device: pd,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        }) {
            Ok(a) => a,
            Err(e) => {
                destroy(&raw);
                return Err(GpuError::Allocation(e.to_string()));
            }
        };

        // SAFETY: valid device; the queue exists (one was requested).
        let (queue, pipeline_cache) = unsafe {
            let queue = raw.get_device_queue(queue_family, 0);
            let cache = raw
                .create_pipeline_cache(&vk::PipelineCacheCreateInfo::default(), None)
                .unwrap_or(vk::PipelineCache::null());
            (queue, cache)
        };

        let timestamp_mask = match caps.timestamp_valid_bits {
            0 => 0,
            64.. => u64::MAX,
            bits => (1u64 << bits) - 1,
        };
        // What was actually enabled wins over the adapter listing.
        info.supports_fp16 = f12.shader_float16 == vk::TRUE;
        info.supports_external_fd = caps.external_fd;
        info.robust_image_access = caps.robust_image_access;
        info.timestamp_period_ns = caps.timestamp_period;

        let device = Device {
            swapchain_fn: caps
                .swapchain
                .then(|| ash::khr::swapchain::Device::new(vk_instance, &raw)),
            memory_fd_fn: caps
                .external_fd
                .then(|| ash::khr::external_memory_fd::Device::new(vk_instance, &raw)),
            semaphore_fd_fn: caps
                .external_fd
                .then(|| ash::khr::external_semaphore_fd::Device::new(vk_instance, &raw)),
            debug_utils_fn: instance
                .debug_enabled()
                .then(|| ash::ext::debug_utils::Device::new(vk_instance, &raw)),
            caps: DeviceCaps {
                read_without_format: core.shader_storage_image_read_without_format == vk::TRUE,
                bindless,
                timestamp_period: caps.timestamp_period,
                timestamp_mask,
            },
            instance: instance.clone(),
            info,
            physical: pd,
            raw,
            queue: Mutex::new(queue),
            queue_family,
            allocator: Mutex::new(ManuallyDrop::new(allocator)),
            pipeline_cache,
            memory_properties,
            frames: Mutex::new(FramePool::default()),
            query_pools: Mutex::new(Vec::new()),
        };
        tracing::info!(
            "Vulkan device: {} ({}, Vulkan {}, fp16 {}, external fd {}, robustImageAccess {}, timestamps {})",
            device.info.name,
            device.info.driver,
            device.info.api_version,
            device.info.supports_fp16,
            device.info.supports_external_fd,
            device.info.robust_image_access,
            device.caps.timestamp_period.is_some()
        );
        Ok(Arc::new(device))
    }

    /// The adapter this device runs on.
    pub fn info(&self) -> &AdapterInfo {
        &self.info
    }

    /// The instance.
    pub fn instance(&self) -> &Arc<Instance> {
        &self.instance
    }

    /// The raw `ash` device, for calls this layer does not wrap.
    pub fn raw(&self) -> &ash::Device {
        &self.raw
    }

    /// Run `f` with the queue locked. Raw `vkQueue*` calls made outside this
    /// layer must go through here.
    pub fn with_queue<R>(&self, f: impl FnOnce(vk::Queue) -> R) -> R {
        let queue = lock(&self.queue);
        f(*queue)
    }

    /// Queue family index of the queue.
    pub fn queue_family(&self) -> u32 {
        self.queue_family
    }

    /// `vkDeviceWaitIdle` (with the queue locked).
    pub fn wait_idle(&self) -> Result<()> {
        // SAFETY: the queue mutex is held, so no other thread uses the queue.
        self.with_queue(|_| unsafe { self.raw.device_wait_idle() })
            .map_err(vk_err("vkDeviceWaitIdle"))?;
        self.reclaim_frames();
        Ok(())
    }

    /// Validation messages collected since the last call (debug instances).
    pub fn debug_messages(&self) -> Vec<String> {
        self.instance.take_debug_messages()
    }

    /// Create a buffer.
    pub fn create_buffer(self: &Arc<Self>, desc: &BufferDesc) -> Result<Buffer> {
        Buffer::create(self, desc)
    }

    /// Create a texture (2D, optionally an array).
    pub fn create_texture(self: &Arc<Self>, desc: &TextureDesc) -> Result<Texture> {
        Texture::create(self, desc, false)
    }

    /// Create a texture in its own exportable allocation (opaque fd, first
    /// `DEVICE_LOCAL` type, outside the allocator), for lsfg.
    pub fn create_external_texture(self: &Arc<Self>, desc: &TextureDesc) -> Result<ExternalTexture> {
        self.require_external()?;
        Ok(ExternalTexture::new(Texture::create(self, desc, true)?))
    }

    /// Create an exportable binary semaphore.
    pub fn create_external_semaphore(self: &Arc<Self>) -> Result<ExternalSemaphore> {
        self.require_external()?;
        ExternalSemaphore::create(self)
    }

    /// Create a sampler.
    pub fn create_sampler(self: &Arc<Self>, desc: &SamplerDesc) -> Result<Sampler> {
        Sampler::create(self, desc)
    }

    /// Create a compute pipeline with its resources bound.
    pub fn create_compute(self: &Arc<Self>, desc: &ComputeDesc<'_>) -> Result<ComputePipeline> {
        ComputePipeline::create(self, desc)
    }

    /// Start recording work.
    pub fn recorder(self: &Arc<Self>) -> Result<CommandRecorder> {
        CommandRecorder::new(self.clone())
    }

    /// Submit recorded work (queue locked only for the submit). The
    /// [`Submission`] waits for it and reads its timestamps.
    pub fn submit(&self, recorder: CommandRecorder, options: &SubmitOptions<'_>) -> Result<Submission> {
        recorder.submit(options)
    }

    /// Create a swapchain on an XCB window.
    pub fn create_swapchain(self: &Arc<Self>, desc: &SwapchainDesc) -> Result<Swapchain> {
        Swapchain::create(self, desc)
    }
}

// Crate-internal helpers.
impl Device {
    pub(crate) fn physical(&self) -> vk::PhysicalDevice {
        self.physical
    }

    pub(crate) fn caps(&self) -> &DeviceCaps {
        &self.caps
    }

    pub(crate) fn pipeline_cache(&self) -> vk::PipelineCache {
        self.pipeline_cache
    }

    pub(crate) fn memory_properties(&self) -> &vk::PhysicalDeviceMemoryProperties {
        &self.memory_properties
    }

    pub(crate) fn swapchain_fn(&self) -> Result<&ash::khr::swapchain::Device> {
        self.swapchain_fn
            .as_ref()
            .ok_or_else(|| GpuError::Unsupported("VK_KHR_swapchain (no surface support)".into()))
    }

    pub(crate) fn memory_fd_fn(&self) -> Result<&ash::khr::external_memory_fd::Device> {
        self.memory_fd_fn.as_ref().ok_or_else(external_unsupported)
    }

    pub(crate) fn semaphore_fd_fn(&self) -> Result<&ash::khr::external_semaphore_fd::Device> {
        self.semaphore_fd_fn.as_ref().ok_or_else(external_unsupported)
    }

    fn require_external(&self) -> Result<()> {
        if self.memory_fd_fn.is_some() && self.semaphore_fd_fn.is_some() {
            Ok(())
        } else {
            Err(external_unsupported())
        }
    }

    /// Name an object for the validation layer and debuggers (debug
    /// instances only).
    pub(crate) fn set_name<H: vk::Handle>(&self, handle: H, label: &str) {
        let Some(utils) = &self.debug_utils_fn else { return };
        if label.is_empty() {
            return;
        }
        let Ok(name) = std::ffi::CString::new(label) else {
            return;
        };
        let info = vk::DebugUtilsObjectNameInfoEXT::default()
            .object_handle(handle)
            .object_name(&name);
        // SAFETY: `handle` is a live object of this device.
        let _ = unsafe { utils.set_debug_utils_object_name(&info) };
    }

    /// Sub-allocate memory with `gpu-allocator`.
    pub(crate) fn allocate(
        &self,
        name: &str,
        requirements: vk::MemoryRequirements,
        location: gpu_allocator::MemoryLocation,
        linear: bool,
    ) -> Result<Allocation> {
        lock(&self.allocator)
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location,
                linear,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| GpuError::Allocation(format!("{name}: {e}")))
    }

    pub(crate) fn free(&self, allocation: Allocation) {
        if let Err(e) = lock(&self.allocator).free(allocation) {
            tracing::warn!("freeing GPU memory: {e}");
        }
    }

    /// A frame ready to record into (command buffer begun).
    pub(crate) fn acquire_frame(&self) -> Result<Frame> {
        self.reclaim_frames();
        let frame = lock(&self.frames).free.pop();
        let frame = match frame {
            Some(f) => f,
            None => self.new_frame()?,
        };
        // SAFETY: the frame is not in use (fresh, or its fence signalled);
        // its pool belongs to this thread until it is submitted.
        let begun = unsafe {
            self.raw
                .reset_command_pool(frame.pool, vk::CommandPoolResetFlags::empty())
                .and_then(|_| {
                    self.raw.begin_command_buffer(
                        frame.cmd,
                        &vk::CommandBufferBeginInfo::default()
                            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                    )
                })
        };
        if let Err(e) = begun {
            self.destroy_frame(frame);
            return Err(GpuError::vk("vkBeginCommandBuffer", e));
        }
        Ok(frame)
    }

    fn new_frame(&self) -> Result<Frame> {
        // SAFETY: plain object creation on a valid device; partial results
        // are destroyed on error.
        unsafe {
            let pool = self
                .raw
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(self.queue_family)
                        .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                    None,
                )
                .map_err(vk_err("vkCreateCommandPool"))?;
            let cmd = match self.raw.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            ) {
                Ok(v) => v[0],
                Err(e) => {
                    self.raw.destroy_command_pool(pool, None);
                    return Err(GpuError::vk("vkAllocateCommandBuffers", e));
                }
            };
            let fence = match self.raw.create_fence(&vk::FenceCreateInfo::default(), None) {
                Ok(f) => f,
                Err(e) => {
                    self.raw.destroy_command_pool(pool, None);
                    return Err(GpuError::vk("vkCreateFence", e));
                }
            };
            Ok(Frame {
                pool,
                cmd,
                fence,
                query_pool: None,
                keep: Vec::new(),
            })
        }
    }

    fn destroy_frame(&self, frame: Frame) {
        // SAFETY: the frame is idle (never submitted, or its fence signalled).
        unsafe {
            self.raw.destroy_fence(frame.fence, None);
            self.raw.destroy_command_pool(frame.pool, None);
        }
        if let Some(q) = frame.query_pool {
            self.release_query_pool(q);
        }
    }

    /// Give back a frame whose work is finished (or was never submitted).
    pub(crate) fn release_frame(&self, mut frame: Frame, submitted: bool) {
        // Dropping the kept resources may destroy them: do it unlocked.
        drop(std::mem::take(&mut frame.keep));
        if let Some(q) = frame.query_pool.take() {
            self.release_query_pool(q);
        }
        if submitted {
            // SAFETY: the fence signalled; nothing waits on it.
            if unsafe { self.raw.reset_fences(&[frame.fence]) }.is_err() {
                self.destroy_frame(frame);
                return;
            }
        }
        lock(&self.frames).free.push(frame);
    }

    /// Park a submitted frame whose fence has not signalled yet.
    pub(crate) fn park_frame(&self, frame: Frame) {
        lock(&self.frames).pending.push(frame);
    }

    /// Recycle parked frames that finished.
    fn reclaim_frames(&self) {
        let done: Vec<Frame> = {
            let mut pool = lock(&self.frames);
            let pending = std::mem::take(&mut pool.pending);
            let (done, still): (Vec<_>, Vec<_>) = pending
                .into_iter()
                // SAFETY: the fence belongs to this device.
                .partition(|f| unsafe { self.raw.get_fence_status(f.fence) }.unwrap_or(false));
            pool.pending = still;
            done
        };
        for frame in done {
            self.release_frame(frame, true);
        }
    }

    /// A reset-pending 128-entry timestamp pool, if timestamps work.
    pub(crate) fn acquire_query_pool(&self) -> Option<vk::QueryPool> {
        self.caps.timestamp_period?;
        if let Some(q) = lock(&self.query_pools).pop() {
            return Some(q);
        }
        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(TIMESTAMP_QUERIES);
        // SAFETY: plain object creation on a valid device.
        unsafe { self.raw.create_query_pool(&info, None) }.ok()
    }

    pub(crate) fn release_query_pool(&self, pool: vk::QueryPool) {
        lock(&self.query_pools).push(pool);
    }

    /// Submit `submits` with the queue locked.
    pub(crate) fn queue_submit(&self, submits: &[vk::SubmitInfo<'_>], fence: vk::Fence) -> Result<()> {
        // SAFETY: the queue mutex is held; the submit infos point to live
        // command buffers and semaphores of this device.
        self.with_queue(|q| unsafe { self.raw.queue_submit(q, submits, fence) })
            .map_err(vk_err("vkQueueSubmit"))
    }

    /// Record, submit and wait (1.x `vk_execute_one_time_commands`).
    pub(crate) fn run_now(self: &Arc<Self>, record: impl FnOnce(&mut CommandRecorder)) -> Result<()> {
        let mut recorder = self.recorder()?;
        record(&mut recorder);
        self.submit(recorder, &SubmitOptions::default())?.wait(u64::MAX)?;
        Ok(())
    }
}

fn external_unsupported() -> GpuError {
    GpuError::Unsupported(
        "sharing memory and semaphores by fd (VK_KHR_external_memory_fd, VK_KHR_external_semaphore_fd)"
            .into(),
    )
}

impl Drop for Device {
    fn drop(&mut self) {
        // Every resource, recorder and submission holds an `Arc<Device>`, so
        // nothing of ours is in flight except what the queue still runs.
        // SAFETY: last owner; after the wait the queue is idle and every
        // object below is unused.
        unsafe {
            let _ = self.raw.device_wait_idle();
            let pool = std::mem::take(&mut *lock(&self.frames));
            for frame in pool.free.into_iter().chain(pool.pending) {
                self.raw.destroy_fence(frame.fence, None);
                self.raw.destroy_command_pool(frame.pool, None);
                if let Some(q) = frame.query_pool {
                    self.raw.destroy_query_pool(q, None);
                }
            }
            for q in std::mem::take(&mut *lock(&self.query_pools)) {
                self.raw.destroy_query_pool(q, None);
            }
            if self.pipeline_cache != vk::PipelineCache::null() {
                self.raw.destroy_pipeline_cache(self.pipeline_cache, None);
            }
            ManuallyDrop::drop(&mut *lock(&self.allocator));
            self.raw.destroy_device(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn device_is_send_sync() {
        assert_send_sync::<Device>();
        assert_send_sync::<Instance>();
    }
}
