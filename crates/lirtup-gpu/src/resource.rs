//! Buffers and textures (`vk_resource.cpp`, `Buffer` and `Texture2D` in
//! `vulkan/__init__.py`). The 1.x explicit heaps become a memory location;
//! `gpu-allocator` places them.

use bitflags::bitflags;

use crate::{Format, Result};

/// Where a resource lives (the 1.x `HEAP_DEFAULT`, `HEAP_UPLOAD`,
/// `HEAP_READBACK`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MemoryLocation {
    /// Device local (default).
    #[default]
    GpuOnly,
    /// Host visible, written by the CPU (staging for capture uploads).
    Upload,
    /// Host visible and cached, read by the CPU (screenshots, tests).
    Readback,
}

/// How to create a [`Buffer`].
#[derive(Debug, Clone, PartialEq)]
pub struct BufferDesc {
    /// Size in bytes.
    pub size: u64,
    /// Element stride for structured buffers (0 = raw).
    pub stride: u32,
    /// Texel format for typed buffers.
    pub format: Option<Format>,
    /// Memory location.
    pub location: MemoryLocation,
    /// Debug name.
    pub label: String,
}

/// A buffer. Cheap to clone; keeps the device alive.
#[derive(Clone)]
pub struct Buffer {
    _private: std::sync::Arc<()>,
}

impl Buffer {
    /// Size in bytes.
    pub fn size(&self) -> u64 {
        todo!("front G")
    }

    /// The description it was created with.
    pub fn desc(&self) -> &BufferDesc {
        todo!("front G")
    }

    /// Copy bytes in at `offset` (host-visible buffers only).
    pub fn write(&self, offset: u64, data: &[u8]) -> Result<()> {
        todo!("front G")
    }

    /// Write straight into the mapped memory (host-visible buffers only);
    /// capture grabs into this to avoid a copy. The closure gets the whole
    /// buffer.
    pub fn write_with<R>(&self, f: impl FnOnce(&mut [u8]) -> R) -> Result<R> {
        todo!("front G")
    }

    /// Copy bytes out (readback buffers only).
    pub fn read(&self, offset: u64, out: &mut [u8]) -> Result<()> {
        todo!("front G")
    }

    /// Raw handle.
    pub fn raw(&self) -> ash::vk::Buffer {
        todo!("front G")
    }
}

bitflags! {
    /// What a texture is used for.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TextureUsage: u32 {
        /// Read in shaders (SRV).
        const SAMPLED = 1 << 0;
        /// Written in shaders (UAV).
        const STORAGE = 1 << 1;
        /// Copy source.
        const COPY_SRC = 1 << 2;
        /// Copy destination.
        const COPY_DST = 1 << 3;
        /// Copied to a swapchain image.
        const PRESENT_SRC = 1 << 4;
    }
}

impl Default for TextureUsage {
    fn default() -> Self {
        TextureUsage::SAMPLED | TextureUsage::STORAGE | TextureUsage::COPY_SRC | TextureUsage::COPY_DST
    }
}

/// How to create a [`Texture`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextureDesc {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Format.
    pub format: Format,
    /// Array layers (tiles use one layer per tile); 1 = plain 2D.
    pub slices: u32,
    /// Create a 2D-array view even with one slice (shaders that declare
    /// `Texture2DArray`).
    pub force_array_view: bool,
    /// Usage.
    pub usage: TextureUsage,
    /// Debug name.
    pub label: String,
}

/// A 2D texture or texture array. Cheap to clone; keeps the device alive.
#[derive(Clone)]
pub struct Texture {
    _private: std::sync::Arc<()>,
}

impl Texture {
    /// Width in pixels.
    pub fn width(&self) -> u32 {
        todo!("front G")
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        todo!("front G")
    }

    /// Array layers.
    pub fn slices(&self) -> u32 {
        todo!("front G")
    }

    /// Format.
    pub fn format(&self) -> Format {
        todo!("front G")
    }

    /// The description it was created with.
    pub fn desc(&self) -> &TextureDesc {
        todo!("front G")
    }

    /// Bytes per row of a tightly packed copy.
    pub fn row_pitch(&self) -> u32 {
        todo!("front G")
    }

    /// Read the whole texture (slice 0) back to the CPU, tightly packed.
    /// Blocks; for screenshots and tests, not per frame.
    pub fn download(&self) -> Result<Vec<u8>> {
        todo!("front G")
    }

    /// Raw image handle.
    pub fn raw(&self) -> ash::vk::Image {
        todo!("front G")
    }

    /// Raw image view.
    pub fn raw_view(&self) -> ash::vk::ImageView {
        todo!("front G")
    }
}
