//! Safe wrapper over `lsfg-sys` (the built-in engine).
//!
//! lsfg has global state, so [`LsfgEngine`] is a process-wide singleton:
//! [`LsfgEngine::initialize`] fails while another one is alive. Contexts
//! hold an `Arc` of the engine; dropping a context deletes it in lsfg, and
//! lsfg is finalized when the last `Arc` goes. All calls are serialized inside (one mutex), so the
//! engine is `Send + Sync`.

use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::Result;

/// What a DLL offers (`lsfg_probe`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Probe {
    /// FP16 and FP32 shaders.
    Fp16,
    /// FP32 only.
    Fp32Only,
}

/// Check a `Lossless.dll` without touching Vulkan.
pub fn probe(dll: &Path) -> Result<Probe> {
    todo!("front C")
}

/// Options of [`LsfgEngine::initialize`] (see `lsfg_initialize` in
/// `lsfg_c.h`).
#[derive(Debug, Clone, PartialEq)]
pub struct EngineOptions {
    /// `Lossless.dll`.
    pub dll: PathBuf,
    /// PCI vendor of the GPU (`lirtup_gpu::AdapterInfo::vendor_id`).
    pub vendor_id: u32,
    /// PCI device of the GPU.
    pub device_id: u32,
    /// Generated frames per real frame (multiplier − 1), 1 to 4.
    pub generated: u32,
    /// Flow scale, 0.25 to 1.0.
    pub flow_scale: f32,
    /// Lighter model.
    pub performance: bool,
    /// HDR input.
    pub hdr: bool,
    /// Use FP16 shaders if available.
    pub fp16: bool,
}

/// The initialized lsfg engine.
pub struct LsfgEngine {
    _private: (),
}

impl LsfgEngine {
    /// Initialize lsfg (one engine per process).
    pub fn initialize(options: &EngineOptions) -> Result<Arc<LsfgEngine>> {
        todo!("front C")
    }

    /// Whether FP16 shaders are in use.
    pub fn fp16(&self) -> bool {
        todo!("front C")
    }

    /// The options it was created with.
    pub fn options(&self) -> &EngineOptions {
        todo!("front C")
    }

    /// Create a context: two input image fds, `generated` output image fds,
    /// size and `VkFormat` (as a number). lsfg takes the fds.
    pub fn create_context(
        self: &Arc<Self>,
        inputs: [OwnedFd; 2],
        outputs: Vec<OwnedFd>,
        width: u32,
        height: u32,
        vk_format: u32,
    ) -> Result<LsfgContext> {
        todo!("front C")
    }
}

/// One lsfg context.
pub struct LsfgContext {
    _engine: Arc<LsfgEngine>,
}

impl LsfgContext {
    /// Generate the frames for the newest input: lsfg waits on `input_ready`
    /// and signals one of `outputs_ready` per generated frame. lsfg takes the
    /// fds.
    pub fn present(&mut self, input_ready: OwnedFd, outputs_ready: Vec<OwnedFd>) -> Result<()> {
        todo!("front C")
    }
}

impl Drop for LsfgContext {
    fn drop(&mut self) {
        // front C: lsfg_delete_context
    }
}

impl Drop for LsfgEngine {
    fn drop(&mut self) {
        // front C: lsfg_finalize
    }
}
