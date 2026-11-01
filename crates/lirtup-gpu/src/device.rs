//! Logical device, queue mutex and resource creation (`vk_device.cpp`).

use std::sync::Arc;

use crate::{
    AdapterInfo, Buffer, BufferDesc, CommandRecorder, ComputeDesc, ComputePipeline, ExternalSemaphore,
    ExternalTexture, Instance, Result, Sampler, SamplerDesc, Submission, SubmitOptions, Swapchain,
    SwapchainDesc, Texture, TextureDesc,
};

/// A Vulkan device with its single queue.
///
/// `Send + Sync`. Shared as `Arc<Device>`; every resource keeps one.
pub struct Device {
    _private: (),
}

impl Device {
    /// The adapter this device runs on.
    pub fn info(&self) -> &AdapterInfo {
        todo!("front G")
    }

    /// The instance.
    pub fn instance(&self) -> &Arc<Instance> {
        todo!("front G")
    }

    /// The raw `ash` device, for calls this layer does not wrap.
    pub fn raw(&self) -> &ash::Device {
        todo!("front G")
    }

    /// Run `f` with the queue locked. Raw `vkQueue*` calls made outside this
    /// layer must go through here.
    pub fn with_queue<R>(&self, f: impl FnOnce(ash::vk::Queue) -> R) -> R {
        todo!("front G")
    }

    /// Queue family index of the queue.
    pub fn queue_family(&self) -> u32 {
        todo!("front G")
    }

    /// `vkDeviceWaitIdle` (with the queue locked).
    pub fn wait_idle(&self) -> Result<()> {
        todo!("front G")
    }

    /// Validation messages collected since the last call (debug instances).
    pub fn debug_messages(&self) -> Vec<String> {
        todo!("front G")
    }

    /// Create a buffer.
    pub fn create_buffer(self: &Arc<Self>, desc: &BufferDesc) -> Result<Buffer> {
        todo!("front G")
    }

    /// Create a texture (2D, optionally an array).
    pub fn create_texture(self: &Arc<Self>, desc: &TextureDesc) -> Result<Texture> {
        todo!("front G")
    }

    /// Create a texture in its own exportable allocation (opaque fd, first
    /// `DEVICE_LOCAL` type, outside the allocator), for lsfg.
    pub fn create_external_texture(self: &Arc<Self>, desc: &TextureDesc) -> Result<ExternalTexture> {
        todo!("front G")
    }

    /// Create an exportable binary semaphore.
    pub fn create_external_semaphore(self: &Arc<Self>) -> Result<ExternalSemaphore> {
        todo!("front G")
    }

    /// Create a sampler.
    pub fn create_sampler(self: &Arc<Self>, desc: &SamplerDesc) -> Result<Sampler> {
        todo!("front G")
    }

    /// Create a compute pipeline with its resources bound.
    pub fn create_compute(self: &Arc<Self>, desc: &ComputeDesc<'_>) -> Result<ComputePipeline> {
        todo!("front G")
    }

    /// Start recording work.
    pub fn recorder(self: &Arc<Self>) -> Result<CommandRecorder> {
        todo!("front G")
    }

    /// Submit recorded work (queue locked only for the submit). The
    /// [`Submission`] waits for it and reads its timestamps.
    pub fn submit(&self, recorder: CommandRecorder, options: &SubmitOptions<'_>) -> Result<Submission> {
        todo!("front G")
    }

    /// Create a swapchain on an XCB window.
    pub fn create_swapchain(self: &Arc<Self>, desc: &SwapchainDesc) -> Result<Swapchain> {
        todo!("front G")
    }
}
