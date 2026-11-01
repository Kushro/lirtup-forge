//! Presentation to an XCB window (`vk_swapchain.cpp`).

use std::cell::Cell;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::Arc;

use ash::vk;

use crate::error::vk_err;
use crate::resource::color_range;
use crate::{Device, ExternalSemaphore, Format, GpuError, Result, Texture, TextureUsage};

/// Vulkan present mode (mirrors `lirtup_config::PresentMode`; this crate does
/// not depend on the config).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PresentMode {
    /// FIFO (vsync).
    #[default]
    Fifo,
    /// Mailbox.
    Mailbox,
    /// Immediate.
    Immediate,
}

/// An XCB window to present to: the overlay window of `lirtup-x11`.
#[derive(Debug, Clone, Copy)]
pub struct XcbTarget {
    connection: NonNull<c_void>,
    window: u32,
}

// SAFETY: the pointer is only passed to vkCreateXcbSurfaceKHR, which may be
// called from any thread; `XcbTarget::new` makes the caller keep the
// connection alive.
unsafe impl Send for XcbTarget {}

impl XcbTarget {
    /// Wrap an `xcb_connection_t*` and a window id.
    ///
    /// # Safety
    ///
    /// `connection` must be a live XCB connection that outlives every
    /// swapchain created from this target.
    pub unsafe fn new(connection: NonNull<c_void>, window: u32) -> XcbTarget {
        XcbTarget { connection, window }
    }

    /// The connection pointer.
    pub fn connection(&self) -> NonNull<c_void> {
        self.connection
    }

    /// The window id.
    pub fn window(&self) -> u32 {
        self.window
    }
}

/// How to create a [`Swapchain`].
#[derive(Debug, Clone)]
pub struct SwapchainDesc {
    /// Window.
    pub target: XcbTarget,
    /// Preferred format (falls back to what the surface offers).
    pub format: Format,
    /// Images (2 or 3).
    pub buffers: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Present mode (falls back to FIFO).
    pub present_mode: PresentMode,
}

/// How to present one texture.
#[derive(Default)]
pub struct PresentOptions<'a> {
    /// Top-left position of the texture inside the swapchain image.
    pub offset: (i32, i32),
    /// Color of what the texture does not cover (letterbox), RGBA.
    pub clear: Option<[f32; 4]>,
    /// Wait on this semaphore before copying (a generated lsfg frame).
    pub wait: Option<&'a ExternalSemaphore>,
    /// Block until the present's fence signals (1.x `wait_for_fence`).
    pub wait_fence: bool,
}

/// Result of a present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresentStatus {
    /// Presented.
    Ok,
    /// Presented, but the swapchain should be recreated soon.
    Suboptimal,
    /// Not presented; recreate ([`Swapchain::recreate`]).
    OutOfDate,
}

impl PresentMode {
    fn to_vk(self) -> vk::PresentModeKHR {
        match self {
            PresentMode::Fifo => vk::PresentModeKHR::FIFO,
            PresentMode::Mailbox => vk::PresentModeKHR::MAILBOX,
            PresentMode::Immediate => vk::PresentModeKHR::IMMEDIATE,
        }
    }

    fn from_vk(mode: vk::PresentModeKHR) -> PresentMode {
        match mode {
            vk::PresentModeKHR::MAILBOX => PresentMode::Mailbox,
            vk::PresentModeKHR::IMMEDIATE => PresentMode::Immediate,
            _ => PresentMode::Fifo,
        }
    }
}

/// `choose_present_mode`: the wanted one if offered, else FIFO.
fn choose_present_mode(available: &[vk::PresentModeKHR], wanted: PresentMode) -> vk::PresentModeKHR {
    let wanted = wanted.to_vk();
    if available.contains(&wanted) {
        wanted
    } else {
        vk::PresentModeKHR::FIFO
    }
}

/// `choose_surface_format`: the wanted format in sRGB non-linear, else
/// `B8G8R8A8_SRGB`, else the first one this crate knows.
fn choose_surface_format(
    available: &[vk::SurfaceFormatKHR],
    wanted: Format,
) -> Option<(vk::SurfaceFormatKHR, Format)> {
    let srgb = |f: &&vk::SurfaceFormatKHR| f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR;
    let find = |vk_format: vk::Format| available.iter().filter(srgb).find(|f| f.format == vk_format);
    find(wanted.to_vk())
        .or_else(|| find(vk::Format::B8G8R8A8_SRGB))
        .or_else(|| {
            available
                .iter()
                .filter(srgb)
                .find(|f| Format::from_vk(f.format).is_some())
        })
        .or_else(|| available.iter().find(|f| Format::from_vk(f.format).is_some()))
        .and_then(|f| Format::from_vk(f.format).map(|ours| (*f, ours)))
}

/// `choose_image_count`: within the surface limits.
fn choose_image_count(caps: &vk::SurfaceCapabilitiesKHR, wanted: u32) -> u32 {
    let mut count = wanted.max(1);
    if caps.max_image_count > 0 {
        count = count.min(caps.max_image_count);
    }
    count.max(caps.min_image_count)
}

/// The extent: the requested size clamped to the surface limits, or the
/// window's current size when 0 is asked for.
fn choose_extent(caps: &vk::SurfaceCapabilitiesKHR, width: u32, height: u32) -> vk::Extent2D {
    if (width == 0 || height == 0) && caps.current_extent.width != u32::MAX {
        return caps.current_extent;
    }
    let (w, h) = if width == 0 || height == 0 {
        (800, 600)
    } else {
        (width, height)
    };
    vk::Extent2D {
        width: w.clamp(
            caps.min_image_extent.width,
            caps.max_image_extent.width.max(caps.min_image_extent.width),
        ),
        height: h.clamp(
            caps.min_image_extent.height,
            caps.max_image_extent.height.max(caps.min_image_extent.height),
        ),
    }
}

/// Source offset, destination offset and size of a copy of a `src`-sized
/// texture placed at `offset` in a `dst`-sized image (clipped on every side).
/// `None` when nothing is visible.
pub(crate) fn clip_copy(
    src: (u32, u32),
    dst: (u32, u32),
    offset: (i32, i32),
) -> Option<([u32; 2], [u32; 2], [u32; 2])> {
    let axis = |len: u32, room: u32, at: i32| -> Option<(u32, u32, u32)> {
        let skip = at.min(0).unsigned_abs();
        let start = at.max(0) as u32;
        if skip >= len || start >= room {
            return None;
        }
        Some((skip, start, (len - skip).min(room - start)))
    };
    let (sx, dx, w) = axis(src.0, dst.0, offset.0)?;
    let (sy, dy, h) = axis(src.1, dst.1, offset.1)?;
    Some(([sx, sy], [dx, dy], [w, h]))
}

/// One frame in flight: its acquire semaphore, fence and command buffer.
struct Slot {
    acquired: vk::Semaphore,
    fence: vk::Fence,
    cmd: vk::CommandBuffer,
}

/// A swapchain. `Send`, not `Sync`.
pub struct Swapchain {
    device: Arc<Device>,
    surface: vk::SurfaceKHR,
    raw: vk::SwapchainKHR,
    surface_format: vk::SurfaceFormatKHR,
    format: Format,
    present_mode: vk::PresentModeKHR,
    wanted_images: u32,
    extent: vk::Extent2D,
    images: Vec<vk::Image>,
    /// One per image: signalled by the copy, waited by the present.
    rendered: Vec<vk::Semaphore>,
    command_pool: vk::CommandPool,
    slots: Vec<Slot>,
    next_slot: usize,
    needs_recreation: bool,
    /// Keeps the last presented texture alive until its copy is done.
    in_flight: Vec<Option<Texture>>,
    _not_sync: PhantomData<Cell<()>>,
}

impl Swapchain {
    /// `vk_Device_create_swapchain_impl`.
    pub(crate) fn create(device: &Arc<Device>, desc: &SwapchainDesc) -> Result<Swapchain> {
        let instance = device.instance();
        let surface_fn = instance
            .surface_fn()
            .ok_or_else(|| GpuError::Unsupported("VK_KHR_surface".into()))?;
        let xcb_fn = instance
            .xcb_surface_fn()
            .ok_or_else(|| GpuError::Unsupported("VK_KHR_xcb_surface".into()))?;
        device.swapchain_fn()?;
        let info = vk::XcbSurfaceCreateInfoKHR::default()
            .connection(desc.target.connection().as_ptr())
            .window(desc.target.window());
        // SAFETY: `XcbTarget::new` makes the caller keep the connection alive
        // for the life of every swapchain made from it.
        let surface =
            unsafe { xcb_fn.create_xcb_surface(&info, None) }.map_err(vk_err("vkCreateXcbSurfaceKHR"))?;
        let destroy_surface = || {
            // SAFETY: nothing was created on the surface yet.
            unsafe { surface_fn.destroy_surface(surface, None) }
        };

        // SAFETY: valid surface and adapter.
        let queried = unsafe {
            let pd = device.physical();
            surface_fn
                .get_physical_device_surface_support(pd, device.queue_family(), surface)
                .and_then(|ok| Ok((ok, surface_fn.get_physical_device_surface_formats(pd, surface)?)))
                .and_then(|(ok, formats)| {
                    Ok((
                        ok,
                        formats,
                        surface_fn.get_physical_device_surface_present_modes(pd, surface)?,
                    ))
                })
        };
        let (supported, formats, modes) = match queried {
            Ok(q) => q,
            Err(e) => {
                destroy_surface();
                return Err(GpuError::vk("vkGetPhysicalDeviceSurface*", e));
            }
        };
        if !supported {
            destroy_surface();
            return Err(GpuError::Unsupported(
                "the queue family cannot present to this window".into(),
            ));
        }
        let Some((surface_format, format)) = choose_surface_format(&formats, desc.format) else {
            destroy_surface();
            return Err(GpuError::Unsupported(format!(
                "no usable surface format in {formats:?}"
            )));
        };
        let present_mode = choose_present_mode(&modes, desc.present_mode);
        if present_mode != desc.present_mode.to_vk() {
            tracing::info!("present mode {:?} not offered, using FIFO", desc.present_mode);
        }

        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(device.queue_family())
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        // SAFETY: valid device.
        let command_pool = match unsafe { device.raw().create_command_pool(&pool_info, None) } {
            Ok(p) => p,
            Err(e) => {
                destroy_surface();
                return Err(GpuError::vk("vkCreateCommandPool", e));
            }
        };

        let mut swapchain = Swapchain {
            device: device.clone(),
            surface,
            raw: vk::SwapchainKHR::null(),
            surface_format,
            format,
            present_mode,
            wanted_images: desc.buffers,
            extent: vk::Extent2D {
                width: desc.width,
                height: desc.height,
            },
            images: Vec::new(),
            rendered: Vec::new(),
            command_pool,
            slots: Vec::new(),
            next_slot: 0,
            needs_recreation: false,
            in_flight: Vec::new(),
            _not_sync: PhantomData,
        };
        // From here `Drop` cleans up.
        swapchain.recreate(desc.width, desc.height)?;
        tracing::info!(
            "swapchain {}x{} {:?} {:?}, {} images",
            swapchain.extent.width,
            swapchain.extent.height,
            swapchain.format,
            swapchain.present_mode,
            swapchain.images.len()
        );
        Ok(swapchain)
    }

    /// Current width.
    pub fn width(&self) -> u32 {
        self.extent.width
    }

    /// Current height.
    pub fn height(&self) -> u32 {
        self.extent.height
    }

    /// The format actually chosen.
    pub fn format(&self) -> Format {
        self.format
    }

    /// The present mode actually chosen.
    pub fn present_mode(&self) -> PresentMode {
        PresentMode::from_vk(self.present_mode)
    }

    /// Acquire, copy `texture` in (with `options`), present. Queue locked for
    /// the submit and the present.
    pub fn present(&mut self, texture: &Texture, options: &PresentOptions<'_>) -> Result<PresentStatus> {
        if !Arc::ptr_eq(texture.device(), &self.device) {
            return Err(GpuError::Invalid("present: texture of another device".into()));
        }
        if !texture.has_usage(TextureUsage::COPY_SRC) && !texture.has_usage(TextureUsage::PRESENT_SRC) {
            return Err(GpuError::Invalid(format!(
                "'{}' was not created with PRESENT_SRC",
                texture.desc().label
            )));
        }
        if texture.format().bytes_per_texel() != self.format.bytes_per_texel() {
            return Err(GpuError::Invalid(format!(
                "present: {:?} cannot be copied into a {:?} swapchain",
                texture.format(),
                self.format
            )));
        }
        let device = self.device.clone();
        let raw = device.raw();
        let swapchain_fn = device.swapchain_fn()?;
        let slot_index = self.next_slot;
        self.next_slot = (self.next_slot + 1) % self.slots.len();
        let (acquired, fence, cmd) = {
            let s = &self.slots[slot_index];
            (s.acquired, s.fence, s.cmd)
        };

        // SAFETY: the fence and semaphore belong to this swapchain; the
        // fence wait guarantees the slot's previous use is finished.
        let index = unsafe {
            raw.wait_for_fences(&[fence], true, u64::MAX)
                .map_err(vk_err("vkWaitForFences"))?;
            match swapchain_fn.acquire_next_image(self.raw, u64::MAX, acquired, vk::Fence::null()) {
                Ok((index, suboptimal)) => {
                    self.needs_recreation |= suboptimal;
                    index
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.needs_recreation = true;
                    return Ok(PresentStatus::OutOfDate);
                }
                Err(e) => return Err(GpuError::vk("vkAcquireNextImageKHR", e)),
            }
        };
        let image = self.images[index as usize];
        let rendered = self.rendered[index as usize];

        let to_transfer = [vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(color_range(0, 1))];
        let to_present = [vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::empty())
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(color_range(0, 1))];
        // Whatever wrote the texture in earlier submissions.
        let earlier = [vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)];
        let clip = clip_copy(
            (texture.width(), texture.height()),
            (self.extent.width, self.extent.height),
            options.offset,
        );

        // SAFETY: the slot's command buffer is idle (fence waited) and only
        // this swapchain records into it; every handle is alive.
        unsafe {
            raw.reset_fences(&[fence]).map_err(vk_err("vkResetFences"))?;
            raw.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .map_err(vk_err("vkResetCommandBuffer"))?;
            raw.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .map_err(vk_err("vkBeginCommandBuffer"))?;
            raw.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &earlier,
                &[],
                &to_transfer,
            );
            if let Some(rgba) = options.clear {
                raw.cmd_clear_color_image(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue { float32: rgba },
                    &[color_range(0, 1)],
                );
                let after_clear = [vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)];
                raw.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &after_clear,
                    &[],
                    &[],
                );
            }
            if let Some((src, dst, size)) = clip {
                let region = [vk::ImageCopy::default()
                    .src_subresource(layers0())
                    .src_offset(vk::Offset3D {
                        x: src[0] as i32,
                        y: src[1] as i32,
                        z: 0,
                    })
                    .dst_subresource(layers0())
                    .dst_offset(vk::Offset3D {
                        x: dst[0] as i32,
                        y: dst[1] as i32,
                        z: 0,
                    })
                    .extent(vk::Extent3D {
                        width: size[0],
                        height: size[1],
                        depth: 1,
                    })];
                raw.cmd_copy_image(
                    cmd,
                    texture.raw(),
                    vk::ImageLayout::GENERAL,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &region,
                );
            }
            raw.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &to_present,
            );
            raw.end_command_buffer(cmd)
                .map_err(vk_err("vkEndCommandBuffer"))?;
        }

        let mut waits = vec![acquired];
        let mut stages = vec![vk::PipelineStageFlags::TRANSFER];
        if let Some(sem) = options.wait {
            if !Arc::ptr_eq(sem.device(), &self.device) {
                return Err(GpuError::Invalid("present: semaphore of another device".into()));
            }
            waits.push(sem.raw());
            stages.push(vk::PipelineStageFlags::TRANSFER);
        }
        let cmds = [cmd];
        let signals = [rendered];
        let submit = [vk::SubmitInfo::default()
            .wait_semaphores(&waits)
            .wait_dst_stage_mask(&stages)
            .command_buffers(&cmds)
            .signal_semaphores(&signals)];
        let swapchains = [self.raw];
        let indices = [index];
        let present = vk::PresentInfoKHR::default()
            .wait_semaphores(&signals)
            .swapchains(&swapchains)
            .image_indices(&indices);
        let (submitted, presented) = device.with_queue(|queue| {
            // SAFETY: the queue mutex is held; every handle is alive.
            unsafe {
                match raw.queue_submit(queue, &submit, fence) {
                    Ok(()) => (Ok(()), Some(swapchain_fn.queue_present(queue, &present))),
                    Err(e) => (Err(e), None),
                }
            }
        });
        submitted.map_err(vk_err("vkQueueSubmit"))?;
        self.in_flight[slot_index] = Some(texture.clone());

        let status = match presented {
            Some(Ok(false)) => PresentStatus::Ok,
            Some(Ok(true)) => {
                self.needs_recreation = true;
                PresentStatus::Suboptimal
            }
            Some(Err(vk::Result::ERROR_OUT_OF_DATE_KHR)) => {
                self.needs_recreation = true;
                PresentStatus::OutOfDate
            }
            Some(Err(e)) => return Err(GpuError::vk("vkQueuePresentKHR", e)),
            None => unreachable!("present only runs after a successful submit"),
        };
        if options.wait_fence {
            // SAFETY: the fence belongs to this swapchain.
            unsafe { raw.wait_for_fences(&[fence], true, u64::MAX) }.map_err(vk_err("vkWaitForFences"))?;
        }
        if self.needs_recreation && status == PresentStatus::Ok {
            // The acquire said suboptimal.
            return Ok(PresentStatus::Suboptimal);
        }
        Ok(status)
    }

    /// Recreate at a new size (after `OutOfDate`, or a resize debounced by
    /// `swapchain_debounce`).
    pub fn recreate(&mut self, width: u32, height: u32) -> Result<()> {
        let device = self.device.clone();
        let raw = device.raw();
        let surface_fn = device
            .instance()
            .surface_fn()
            .ok_or_else(|| GpuError::Unsupported("VK_KHR_surface".into()))?;
        let swapchain_fn = device.swapchain_fn()?;
        // SAFETY: valid surface and adapter.
        let caps =
            unsafe { surface_fn.get_physical_device_surface_capabilities(device.physical(), self.surface) }
                .map_err(vk_err("vkGetPhysicalDeviceSurfaceCapabilitiesKHR"))?;
        let extent = choose_extent(&caps, width, height);
        if extent.width == 0 || extent.height == 0 {
            // Minimized: keep the old one and try again later.
            self.needs_recreation = true;
            return Err(GpuError::OutOfDate);
        }

        device.wait_idle()?;
        self.destroy_sync();

        let alpha = [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::INHERIT,
        ]
        .into_iter()
        .find(|&a| caps.supported_composite_alpha.contains(a))
        .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE);
        let old = self.raw;
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(choose_image_count(&caps, self.wanted_images))
            .image_format(self.surface_format.format)
            .image_color_space(self.surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(self.present_mode)
            .clipped(true)
            .old_swapchain(old);
        // SAFETY: the device is idle; `old` is retired by this call and
        // destroyed right after.
        unsafe {
            self.raw = swapchain_fn
                .create_swapchain(&info, None)
                .map_err(vk_err("vkCreateSwapchainKHR"))?;
            if old != vk::SwapchainKHR::null() {
                swapchain_fn.destroy_swapchain(old, None);
            }
            self.images = swapchain_fn
                .get_swapchain_images(self.raw)
                .map_err(vk_err("vkGetSwapchainImagesKHR"))?;
        }
        self.extent = extent;

        let count = self.images.len();
        // SAFETY: plain object creation; `destroy_sync` frees partial sets.
        unsafe {
            let cmds = raw
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(self.command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(count as u32),
                )
                .map_err(vk_err("vkAllocateCommandBuffers"))?;
            for cmd in cmds {
                let acquired = raw
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .map_err(vk_err("vkCreateSemaphore"))?;
                let fence = match raw.create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                ) {
                    Ok(f) => f,
                    Err(e) => {
                        raw.destroy_semaphore(acquired, None);
                        return Err(GpuError::vk("vkCreateFence", e));
                    }
                };
                self.slots.push(Slot { acquired, fence, cmd });
            }
            for _ in 0..count {
                self.rendered.push(
                    raw.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                        .map_err(vk_err("vkCreateSemaphore"))?,
                );
            }
        }
        self.in_flight = vec![None; count];
        self.next_slot = 0;
        self.needs_recreation = false;
        Ok(())
    }

    /// Whether the last present asked for a recreation.
    pub fn needs_recreation(&self) -> bool {
        self.needs_recreation
    }

    /// Free the per-frame objects (the device must be idle).
    fn destroy_sync(&mut self) {
        let raw = self.device.raw();
        // SAFETY: the device is idle, so none of these is in use.
        unsafe {
            for slot in self.slots.drain(..) {
                raw.destroy_semaphore(slot.acquired, None);
                raw.destroy_fence(slot.fence, None);
                raw.free_command_buffers(self.command_pool, &[slot.cmd]);
            }
            for sem in self.rendered.drain(..) {
                raw.destroy_semaphore(sem, None);
            }
        }
        self.in_flight.clear();
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        if let Err(e) = self.device.wait_idle() {
            tracing::warn!("waiting before destroying a swapchain: {e}");
        }
        self.destroy_sync();
        let device = self.device.clone();
        // SAFETY: the device is idle; the surface outlives the swapchain and
        // goes last.
        unsafe {
            device.raw().destroy_command_pool(self.command_pool, None);
            if self.raw != vk::SwapchainKHR::null()
                && let Ok(f) = device.swapchain_fn()
            {
                f.destroy_swapchain(self.raw, None);
            }
            if let Some(f) = device.instance().surface_fn() {
                f.destroy_surface(self.surface, None);
            }
        }
    }
}

fn layers0() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        mip_level: 0,
        base_array_layer: 0,
        layer_count: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping() {
        // Fits.
        assert_eq!(
            clip_copy((100, 50), (200, 100), (10, 20)),
            Some(([0, 0], [10, 20], [100, 50]))
        );
        // Larger than the image: cut right and bottom.
        assert_eq!(
            clip_copy((300, 200), (200, 100), (0, 0)),
            Some(([0, 0], [0, 0], [200, 100]))
        );
        // Negative offset: cut left and top.
        assert_eq!(
            clip_copy((100, 100), (200, 200), (-30, -10)),
            Some(([30, 10], [0, 0], [70, 90]))
        );
        // Off screen.
        assert_eq!(clip_copy((100, 100), (200, 200), (200, 0)), None);
        assert_eq!(clip_copy((100, 100), (200, 200), (-100, 0)), None);
    }

    #[test]
    fn present_mode_fallback() {
        let modes = [vk::PresentModeKHR::FIFO, vk::PresentModeKHR::MAILBOX];
        assert_eq!(
            choose_present_mode(&modes, PresentMode::Mailbox),
            vk::PresentModeKHR::MAILBOX
        );
        assert_eq!(
            choose_present_mode(&modes, PresentMode::Immediate),
            vk::PresentModeKHR::FIFO
        );
    }

    #[test]
    fn surface_format_choice() {
        let f = |format| vk::SurfaceFormatKHR {
            format,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        };
        let formats = [
            f(vk::Format::A2R10G10B10_UNORM_PACK32),
            f(vk::Format::B8G8R8A8_SRGB),
            f(vk::Format::B8G8R8A8_UNORM),
        ];
        assert_eq!(
            choose_surface_format(&formats, Format::B8G8R8A8Unorm).map(|x| x.1),
            Some(Format::B8G8R8A8Unorm)
        );
        assert_eq!(
            choose_surface_format(&formats, Format::R16G16B16A16Float).map(|x| x.1),
            Some(Format::B8G8R8A8UnormSrgb)
        );
        assert_eq!(
            choose_surface_format(&[f(vk::Format::A2R10G10B10_UNORM_PACK32)], Format::B8G8R8A8Unorm),
            None
        );
    }

    #[test]
    fn image_count_and_extent() {
        let caps = vk::SurfaceCapabilitiesKHR {
            min_image_count: 2,
            max_image_count: 3,
            current_extent: vk::Extent2D {
                width: 1920,
                height: 1080,
            },
            min_image_extent: vk::Extent2D { width: 1, height: 1 },
            max_image_extent: vk::Extent2D {
                width: 4096,
                height: 4096,
            },
            ..Default::default()
        };
        assert_eq!(choose_image_count(&caps, 5), 3);
        assert_eq!(choose_image_count(&caps, 1), 2);
        assert_eq!(
            choose_extent(&caps, 0, 0),
            vk::Extent2D {
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            choose_extent(&caps, 1280, 720),
            vk::Extent2D {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(
            choose_extent(&caps, 9000, 720),
            vk::Extent2D {
                width: 4096,
                height: 720
            }
        );
    }

    fn assert_send<T: Send>() {}

    #[test]
    fn swapchain_is_send() {
        assert_send::<Swapchain>();
    }
}
