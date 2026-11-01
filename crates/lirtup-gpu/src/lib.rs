//! Vulkan for LirtUp Forge, on `ash`: a thin safe layer with the shape of the
//! 1.x `upscaler.vulkan` module (`src/upscaler/vulkan/lib/*.cpp`).
//!
//! - [`Instance`] loads `libvulkan` at run time (`ash::Entry::load`), lists the
//!   adapters and creates a [`Device`]. When the lsfg-vk layer is used, its
//!   environment is set **before** [`Instance::new`] (`lirtup_fg::layer`).
//! - [`Device`] owns the `VkDevice`, the memory allocator, the timestamp pool
//!   and the queue. Every `vkQueueSubmit`, `vkQueuePresentKHR` and
//!   `vkQueueWaitIdle` goes through its queue mutex: the pipeline thread and
//!   the frame generation thread share it (what the GIL did in 1.x).
//! - Resources ([`Buffer`], [`Texture`], [`Sampler`]) are cheap `Clone`
//!   handles (`Arc` inside) that keep the device alive.
//! - [`ComputePipeline`] binds its resources at creation, like 1.x `Compute`.
//!   Work is recorded into a [`CommandRecorder`] and submitted with
//!   [`Device::submit`]; GPU times come back as [`Timestamps`].
//! - [`ExternalTexture`] and [`ExternalSemaphore`] export opaque fds for lsfg.
//! - [`Swapchain`] presents to an XCB window ([`XcbTarget`]).
//!
//! Thread rules: `Device`, resources and pipelines are `Send + Sync`;
//! `CommandRecorder` and `Swapchain` are `Send` only (one thread at a time).
//!
//! `unsafe` is allowed in this crate; every block has a `// SAFETY:` comment.

#![allow(unsafe_code)]

pub mod compute;
pub mod device;
pub mod error;
pub mod external;
pub mod format;
pub mod instance;
pub mod recorder;
pub mod resource;
pub mod sampler;
mod spirv;
pub mod swapchain;

pub use ash::vk;

pub use compute::{Binding, ComputeDesc, ComputePipeline};
pub use device::Device;
pub use error::{GpuError, Result};
pub use external::{ExternalSemaphore, ExternalTexture};
pub use format::Format;
pub use instance::{AdapterInfo, AdapterKind, Instance, InstanceOptions};
pub use recorder::{CommandRecorder, CopyRegion, Dispatch, Submission, SubmitOptions, Timestamps};
pub use resource::{Buffer, BufferDesc, MemoryLocation, Texture, TextureDesc, TextureUsage};
pub use sampler::{AddressMode, Filter, Sampler, SamplerDesc};
pub use swapchain::{PresentMode, PresentOptions, PresentStatus, Swapchain, SwapchainDesc, XcbTarget};
