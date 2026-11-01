//! Compute pipelines with their resources bound (`vk_compute.cpp`, `Compute`
//! in `vulkan/__init__.py`).
//!
//! Binding numbers follow the 1.x convention for SPIR-V from dxc (the
//! `-fvk-{b,t,u,s}-shift` offsets used by `scripts/hlsl-to-spv.fish` and by
//! the MagpieFX compiler): constant buffers, then SRVs, then UAVs, then
//! samplers, in the order given here.

use crate::{Buffer, Sampler, Texture};

/// A resource bound to a pipeline slot.
#[derive(Clone)]
pub enum Binding {
    /// A buffer (constant, structured, typed).
    Buffer(Buffer),
    /// A texture.
    Texture(Texture),
}

/// How to create a [`ComputePipeline`].
#[derive(Clone, Default)]
pub struct ComputeDesc<'a> {
    /// SPIR-V bytecode (from `include_bytes!` or the MagpieFX compiler).
    pub spirv: &'a [u8],
    /// Entry point (`main` for dxc output).
    pub entry: &'a str,
    /// Constant buffers (`b` registers).
    pub cbv: Vec<Binding>,
    /// Shader resource views (`t` registers).
    pub srv: Vec<Binding>,
    /// Unordered access views (`u` registers).
    pub uav: Vec<Binding>,
    /// Samplers (`s` registers).
    pub samplers: Vec<Sampler>,
    /// Push constant block size in bytes (0 = none).
    pub push_size: u32,
    /// Bindless slots (0 = off), as 1.x `max_bindless`.
    pub bindless_max: u32,
    /// Debug name.
    pub label: &'a str,
}

/// A compute pipeline and its descriptor set. Cheap to clone; keeps its
/// resources alive.
#[derive(Clone)]
pub struct ComputePipeline {
    _private: std::sync::Arc<()>,
}

impl ComputePipeline {
    /// Workgroup size from the SPIR-V (`LocalSize`), to compute dispatch
    /// counts.
    pub fn workgroup_size(&self) -> [u32; 3] {
        todo!("front G")
    }

    /// Push constant size it was created with.
    pub fn push_size(&self) -> u32 {
        todo!("front G")
    }

    /// Debug name.
    pub fn label(&self) -> &str {
        todo!("front G")
    }
}
