//! Buffers and textures (`vk_resource.cpp`, `Buffer` and `Texture2D` in
//! `vulkan/__init__.py`). The 1.x explicit heaps become a memory location;
//! `gpu-allocator` places them.

use std::sync::{Arc, Mutex};

use ash::vk;
use bitflags::bitflags;
use gpu_allocator::vulkan::Allocation;

use crate::error::vk_err;
use crate::{Device, Format, GpuError, Result};

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

impl MemoryLocation {
    fn allocator_location(self) -> gpu_allocator::MemoryLocation {
        match self {
            MemoryLocation::GpuOnly => gpu_allocator::MemoryLocation::GpuOnly,
            MemoryLocation::Upload => gpu_allocator::MemoryLocation::CpuToGpu,
            MemoryLocation::Readback => gpu_allocator::MemoryLocation::GpuToCpu,
        }
    }
}

pub(crate) struct BufferInner {
    device: Arc<Device>,
    raw: vk::Buffer,
    view: vk::BufferView,
    /// Locked for host access (`write_with` hands out `&mut [u8]`).
    allocation: Mutex<Option<Allocation>>,
    desc: BufferDesc,
}

/// A buffer. Cheap to clone; keeps the device alive.
#[derive(Clone)]
pub struct Buffer {
    inner: Arc<BufferInner>,
}

impl Buffer {
    /// `vk_Device_create_buffer` (no heaps, no sparse).
    pub(crate) fn create(device: &Arc<Device>, desc: &BufferDesc) -> Result<Buffer> {
        if desc.size == 0 {
            return Err(GpuError::Invalid(format!("buffer '{}': size 0", desc.label)));
        }
        // Everything 1.x asked for: constant, structured, typed, copies.
        let usage = vk::BufferUsageFlags::TRANSFER_SRC
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::UNIFORM_BUFFER
            | vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER
            | vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER
            | vk::BufferUsageFlags::INDIRECT_BUFFER;
        let raw_device = device.raw();
        let info = vk::BufferCreateInfo::default().size(desc.size).usage(usage);
        // SAFETY: valid device and create info.
        let raw = unsafe { raw_device.create_buffer(&info, None) }.map_err(vk_err("vkCreateBuffer"))?;
        let mut inner = BufferInner {
            device: device.clone(),
            raw,
            view: vk::BufferView::null(),
            allocation: Mutex::new(None),
            desc: desc.clone(),
        };
        // SAFETY: `raw` is a live buffer of this device.
        let requirements = unsafe { raw_device.get_buffer_memory_requirements(raw) };
        let allocation = device.allocate(
            &desc.label,
            requirements,
            desc.location.allocator_location(),
            true,
        )?;
        // SAFETY: fresh allocation large enough for `raw`, bound once.
        unsafe { raw_device.bind_buffer_memory(raw, allocation.memory(), allocation.offset()) }
            .map_err(vk_err("vkBindBufferMemory"))?;
        *inner.allocation.get_mut().unwrap_or_else(|e| e.into_inner()) = Some(allocation);
        if let Some(format) = desc.format {
            let view_info = vk::BufferViewCreateInfo::default()
                .buffer(raw)
                .format(format.to_vk())
                .range(vk::WHOLE_SIZE);
            // SAFETY: the buffer is bound to memory.
            inner.view = unsafe { raw_device.create_buffer_view(&view_info, None) }
                .map_err(vk_err("vkCreateBufferView"))?;
        }
        device.set_name(raw, &desc.label);
        Ok(Buffer {
            inner: Arc::new(inner),
        })
    }

    /// Size in bytes.
    pub fn size(&self) -> u64 {
        self.inner.desc.size
    }

    /// The description it was created with.
    pub fn desc(&self) -> &BufferDesc {
        &self.inner.desc
    }

    /// Copy bytes in at `offset` (host-visible buffers only).
    pub fn write(&self, offset: u64, data: &[u8]) -> Result<()> {
        let end = offset
            .checked_add(data.len() as u64)
            .filter(|&e| e <= self.size())
            .ok_or_else(|| {
                GpuError::Invalid(format!(
                    "buffer '{}': write of {} bytes at {offset} past {}",
                    self.inner.desc.label,
                    data.len(),
                    self.size()
                ))
            })?;
        self.write_with(|mapped| mapped[offset as usize..end as usize].copy_from_slice(data))
    }

    /// Write straight into the mapped memory (host-visible buffers only);
    /// capture grabs into this to avoid a copy. The closure gets the whole
    /// buffer.
    pub fn write_with<R>(&self, f: impl FnOnce(&mut [u8]) -> R) -> Result<R> {
        let mut guard = self.inner.allocation.lock().unwrap_or_else(|e| e.into_inner());
        let size = self.size() as usize;
        let mapped = guard
            .as_mut()
            .and_then(|a| a.mapped_slice_mut())
            .ok_or_else(|| self.not_mapped())?;
        // gpu-allocator maps host memory coherent: no flush needed.
        Ok(f(&mut mapped[..size]))
    }

    /// Copy bytes out (readback buffers only).
    pub fn read(&self, offset: u64, out: &mut [u8]) -> Result<()> {
        let end = offset
            .checked_add(out.len() as u64)
            .filter(|&e| e <= self.size())
            .ok_or_else(|| {
                GpuError::Invalid(format!(
                    "buffer '{}': read of {} bytes at {offset} past {}",
                    self.inner.desc.label,
                    out.len(),
                    self.size()
                ))
            })?;
        let guard = self.inner.allocation.lock().unwrap_or_else(|e| e.into_inner());
        let mapped = guard
            .as_ref()
            .and_then(|a| a.mapped_slice())
            .ok_or_else(|| self.not_mapped())?;
        out.copy_from_slice(&mapped[offset as usize..end as usize]);
        Ok(())
    }

    fn not_mapped(&self) -> GpuError {
        GpuError::Invalid(format!(
            "buffer '{}' is not host visible ({:?})",
            self.inner.desc.label, self.inner.desc.location
        ))
    }

    /// Raw handle.
    pub fn raw(&self) -> vk::Buffer {
        self.inner.raw
    }

    /// Typed-buffer view (null for raw and structured buffers).
    pub(crate) fn raw_view(&self) -> vk::BufferView {
        self.inner.view
    }

    pub(crate) fn device(&self) -> &Arc<Device> {
        &self.inner.device
    }
}

impl Drop for BufferInner {
    fn drop(&mut self) {
        let device = self.device.raw();
        // SAFETY: last handle; submissions keep their buffers alive until
        // their fence signals, so the GPU no longer uses it.
        unsafe {
            if self.view != vk::BufferView::null() {
                device.destroy_buffer_view(self.view, None);
            }
            device.destroy_buffer(self.raw, None);
        }
        if let Some(a) = self
            .allocation
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            self.device.free(a);
        }
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

impl TextureUsage {
    fn to_vk(self) -> vk::ImageUsageFlags {
        let mut usage = vk::ImageUsageFlags::empty();
        if self.contains(TextureUsage::SAMPLED) {
            usage |= vk::ImageUsageFlags::SAMPLED;
        }
        if self.contains(TextureUsage::STORAGE) {
            usage |= vk::ImageUsageFlags::STORAGE;
        }
        if self.intersects(TextureUsage::COPY_SRC | TextureUsage::PRESENT_SRC) {
            usage |= vk::ImageUsageFlags::TRANSFER_SRC;
        }
        if self.contains(TextureUsage::COPY_DST) {
            usage |= vk::ImageUsageFlags::TRANSFER_DST;
        }
        usage
    }
}

pub(crate) enum TextureMemory {
    None,
    Allocated(Allocation),
    /// Own exportable allocation (`ExternalTexture`).
    Dedicated(vk::DeviceMemory),
}

pub(crate) struct TextureInner {
    device: Arc<Device>,
    raw: vk::Image,
    view: vk::ImageView,
    memory: TextureMemory,
    desc: TextureDesc,
    row_pitch: u32,
}

/// A 2D texture or texture array. Cheap to clone; keeps the device alive.
///
/// Textures stay in `VK_IMAGE_LAYOUT_GENERAL` for their whole life (as in
/// 1.x): compute, copies and clears all use it, so only memory barriers are
/// needed between steps.
#[derive(Clone)]
pub struct Texture {
    inner: Arc<TextureInner>,
}

/// First memory type in `type_bits` with `DEVICE_LOCAL` (`vk_external.cpp`).
pub(crate) fn device_local_type(props: &vk::PhysicalDeviceMemoryProperties, type_bits: u32) -> Option<u32> {
    (0..props.memory_type_count).find(|&i| {
        type_bits & (1 << i) != 0
            && props.memory_types[i as usize]
                .property_flags
                .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
    })
}

impl Texture {
    /// `vk_Device_create_texture2d`, or `create_texture2d_external` with
    /// `external`.
    pub(crate) fn create(device: &Arc<Device>, desc: &TextureDesc, external: bool) -> Result<Texture> {
        if desc.width == 0 || desc.height == 0 || desc.slices == 0 {
            return Err(GpuError::Invalid(format!(
                "texture '{}': {}x{}x{}",
                desc.label, desc.width, desc.height, desc.slices
            )));
        }
        if external && desc.slices != 1 {
            return Err(GpuError::Invalid(format!(
                "external texture '{}': one slice only",
                desc.label
            )));
        }
        let format = desc.format.to_vk();
        let mut usage = desc.usage.to_vk();
        let (_, instance) = device.instance().raw();
        // SAFETY: plain query on this device's adapter.
        let features = unsafe { instance.get_physical_device_format_properties(device.physical(), format) }
            .optimal_tiling_features;
        if usage.contains(vk::ImageUsageFlags::STORAGE)
            && !features.contains(vk::FormatFeatureFlags::STORAGE_IMAGE)
        {
            // sRGB formats cannot be storage images; 1.x asked anyway.
            tracing::debug!(
                "texture '{}': {:?} has no storage support, dropping STORAGE",
                desc.label,
                desc.format
            );
            usage &= !vk::ImageUsageFlags::STORAGE;
        }
        if usage.contains(vk::ImageUsageFlags::SAMPLED)
            && !features.contains(vk::FormatFeatureFlags::SAMPLED_IMAGE)
        {
            return Err(GpuError::Unsupported(format!(
                "{:?} cannot be sampled on this device",
                desc.format
            )));
        }
        if usage.is_empty() {
            return Err(GpuError::Invalid(format!("texture '{}': no usage", desc.label)));
        }

        let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        let mut info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: desc.width,
                height: desc.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(desc.slices)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        if external {
            info = info.push_next(&mut external_info);
        }
        let raw_device = device.raw();
        // SAFETY: valid device; the chain points to a local alive here.
        let raw = unsafe { raw_device.create_image(&info, None) }.map_err(vk_err("vkCreateImage"))?;
        let mut inner = TextureInner {
            device: device.clone(),
            raw,
            view: vk::ImageView::null(),
            memory: TextureMemory::None,
            desc: desc.clone(),
            row_pitch: desc.width * desc.format.bytes_per_texel(),
        };
        // SAFETY: `raw` is a live image of this device.
        let requirements = unsafe { raw_device.get_image_memory_requirements(raw) };
        let (memory, offset) = if external {
            let type_index = device_local_type(device.memory_properties(), requirements.memory_type_bits)
                .ok_or_else(|| {
                    GpuError::Allocation("no device-local memory type for an external texture".into())
                })?;
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(raw);
            let mut export = vk::ExportMemoryAllocateInfo::default()
                .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
            let alloc = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(type_index)
                .push_next(&mut export)
                .push_next(&mut dedicated);
            // SAFETY: valid device; the chain points to locals alive here.
            let memory =
                unsafe { raw_device.allocate_memory(&alloc, None) }.map_err(vk_err("vkAllocateMemory"))?;
            inner.memory = TextureMemory::Dedicated(memory);
            (memory, 0)
        } else {
            let a = device.allocate(
                &desc.label,
                requirements,
                gpu_allocator::MemoryLocation::GpuOnly,
                false,
            )?;
            // SAFETY: the handle is only bound to the image; the allocation
            // owns it and is freed after the image is destroyed.
            let r = (unsafe { a.memory() }, a.offset());
            inner.memory = TextureMemory::Allocated(a);
            r
        };
        // SAFETY: fresh memory large enough for the image, bound once.
        unsafe { raw_device.bind_image_memory(raw, memory, offset) }.map_err(vk_err("vkBindImageMemory"))?;

        let view_type = if desc.slices > 1 || desc.force_array_view {
            vk::ImageViewType::TYPE_2D_ARRAY
        } else {
            vk::ImageViewType::TYPE_2D
        };
        let view_info = vk::ImageViewCreateInfo::default()
            .image(raw)
            .view_type(view_type)
            .format(format)
            .subresource_range(color_range(0, desc.slices));
        // SAFETY: the image is bound to memory.
        inner.view =
            unsafe { raw_device.create_image_view(&view_info, None) }.map_err(vk_err("vkCreateImageView"))?;
        device.set_name(raw, &desc.label);

        let texture = Texture {
            inner: Arc::new(inner),
        };
        device.run_now(|r| r.init_layout(&texture))?;
        Ok(texture)
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.inner.desc.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.inner.desc.height
    }

    /// Array layers.
    pub fn slices(&self) -> u32 {
        self.inner.desc.slices
    }

    /// Format.
    pub fn format(&self) -> Format {
        self.inner.desc.format
    }

    /// The description it was created with.
    pub fn desc(&self) -> &TextureDesc {
        &self.inner.desc
    }

    /// Bytes per row of a tightly packed copy.
    pub fn row_pitch(&self) -> u32 {
        self.inner.row_pitch
    }

    /// Bytes of one tightly packed slice.
    pub(crate) fn slice_size(&self) -> u64 {
        self.inner.row_pitch as u64 * self.inner.desc.height as u64
    }

    /// Read the whole texture (slice 0) back to the CPU, tightly packed.
    /// Blocks; for screenshots and tests, not per frame.
    pub fn download(&self) -> Result<Vec<u8>> {
        let device = &self.inner.device;
        let readback = device.create_buffer(&BufferDesc {
            size: self.slice_size(),
            stride: 0,
            format: None,
            location: MemoryLocation::Readback,
            label: format!("{} readback", self.inner.desc.label),
        })?;
        // A bad copy is reported by the submit.
        device.run_now(|r| r.copy_texture_to_buffer(self, &readback))?;
        let mut out = vec![0; self.slice_size() as usize];
        readback.read(0, &mut out)?;
        Ok(out)
    }

    /// Raw image handle.
    pub fn raw(&self) -> vk::Image {
        self.inner.raw
    }

    /// Raw image view.
    pub fn raw_view(&self) -> vk::ImageView {
        self.inner.view
    }

    pub(crate) fn device(&self) -> &Arc<Device> {
        &self.inner.device
    }

    /// The dedicated memory of an external texture.
    pub(crate) fn dedicated_memory(&self) -> Option<vk::DeviceMemory> {
        match self.inner.memory {
            TextureMemory::Dedicated(m) => Some(m),
            _ => None,
        }
    }

    /// Whether the image was created with `usage` (after format checks).
    pub(crate) fn has_usage(&self, usage: TextureUsage) -> bool {
        self.inner.desc.usage.contains(usage)
    }
}

/// Color aspect, mip 0, `count` layers from `base`.
pub(crate) fn color_range(base: u32, count: u32) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: base,
        layer_count: count,
    }
}

impl Drop for TextureInner {
    fn drop(&mut self) {
        let device = self.device.raw();
        // SAFETY: last handle; submissions keep their textures alive until
        // their fence signals, so the GPU no longer uses it.
        unsafe {
            if self.view != vk::ImageView::null() {
                device.destroy_image_view(self.view, None);
            }
            device.destroy_image(self.raw, None);
        }
        match std::mem::replace(&mut self.memory, TextureMemory::None) {
            TextureMemory::None => {}
            TextureMemory::Allocated(a) => self.device.free(a),
            // SAFETY: the image bound to it is destroyed; exported fds hold
            // their own reference in the importing driver.
            TextureMemory::Dedicated(m) => unsafe { device.free_memory(m, None) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_flags() {
        let u = TextureUsage::default().to_vk();
        assert!(u.contains(
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST
        ));
        assert_eq!(
            TextureUsage::PRESENT_SRC.to_vk(),
            vk::ImageUsageFlags::TRANSFER_SRC
        );
        assert!(TextureUsage::empty().to_vk().is_empty());
    }

    #[test]
    fn device_local_memory_type() {
        let mut props = vk::PhysicalDeviceMemoryProperties {
            memory_type_count: 3,
            ..Default::default()
        };
        props.memory_types[0].property_flags = vk::MemoryPropertyFlags::HOST_VISIBLE;
        props.memory_types[1].property_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;
        props.memory_types[2].property_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;
        assert_eq!(device_local_type(&props, 0b111), Some(1));
        assert_eq!(device_local_type(&props, 0b101), Some(2));
        assert_eq!(device_local_type(&props, 0b001), None);
    }

    fn assert_send_sync<T: Send + Sync + Clone>() {}

    #[test]
    fn resources_are_send_sync() {
        assert_send_sync::<Buffer>();
        assert_send_sync::<Texture>();
    }
}
