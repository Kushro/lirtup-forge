//! Recording and submitting work: the 1.x `dispatch_sequence` (copies, a
//! chain of dispatches, the transition for presentation, timestamps) as a
//! builder.

use std::cell::Cell;
use std::marker::PhantomData;

use std::sync::Arc;

use ash::vk;

use crate::device::{Frame, Keep, TIMESTAMP_QUERIES};
use crate::error::vk_err;
use crate::format::NumericKind;
use crate::resource::color_range;
use crate::{Buffer, ComputePipeline, Device, ExternalSemaphore, GpuError, Result, Texture, TextureUsage};

/// One dispatch: pipeline, workgroup counts, push constants.
#[derive(Clone, Copy)]
pub struct Dispatch<'a> {
    /// Pipeline.
    pub pipeline: &'a ComputePipeline,
    /// Workgroup counts.
    pub groups: [u32; 3],
    /// Push constant bytes (at most the pipeline's push size).
    pub push: &'a [u8],
    /// Name of the timestamp of this dispatch (stats panel: «ms por etapa»);
    /// `None` = no timestamp.
    pub timestamp: Option<&'a str>,
}

/// A rectangle copied between a buffer and a texture, or two textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CopyRegion {
    /// Source byte offset (buffer → texture) or source x.
    pub src_offset: u64,
    /// Source row length in texels (buffer → texture; 0 = tightly packed).
    pub src_row_length: u32,
    /// Source y (texture → texture).
    pub src_y: u32,
    /// Destination x.
    pub dst_x: u32,
    /// Destination y.
    pub dst_y: u32,
    /// Destination array layer.
    pub dst_slice: u32,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
}

/// A command buffer being recorded. `Send`, not `Sync`. Barriers between
/// steps are inserted by the recorder (resources are tracked per recording,
/// as the 1.x C++ does).
///
/// Textures live in `GENERAL` layout, so a step only needs a memory barrier
/// from what the previous step wrote to what it reads and writes (the first
/// step waits on everything submitted before). Mistakes (a resource of
/// another device, a region out of bounds, too many push constant bytes)
/// skip the step and are returned by [`Device::submit`].
pub struct CommandRecorder {
    device: Arc<Device>,
    /// `None` only after `submit` took it.
    frame: Option<Frame>,
    steps: usize,
    /// Stage and writes of the previous step.
    last: (vk::PipelineStageFlags, vk::AccessFlags),
    timestamp_names: Vec<String>,
    timestamps_full: bool,
    error: Option<GpuError>,
    _not_sync: PhantomData<Cell<()>>,
}

impl CommandRecorder {
    pub(crate) fn new(device: Arc<Device>) -> Result<CommandRecorder> {
        let frame = device.acquire_frame()?;
        Ok(CommandRecorder {
            device,
            frame: Some(frame),
            steps: 0,
            last: (
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::AccessFlags::MEMORY_WRITE,
            ),
            timestamp_names: Vec::new(),
            timestamps_full: false,
            error: None,
            _not_sync: PhantomData,
        })
    }

    fn frame(&mut self) -> &mut Frame {
        self.frame.as_mut().expect("recorder frame taken before submit")
    }

    fn cmd(&self) -> vk::CommandBuffer {
        self.frame
            .as_ref()
            .expect("recorder frame taken before submit")
            .cmd
    }

    /// Keep the first error; later steps still record.
    fn fail(&mut self, error: GpuError) {
        tracing::warn!("command recorder: {error}");
        self.error.get_or_insert(error);
    }

    fn same_device(&self, device: &Arc<Device>) -> bool {
        Arc::ptr_eq(device, &self.device)
    }

    fn keep(&mut self, keep: Keep) {
        self.frame().keep.push(keep);
    }

    /// Barrier from the previous step to one at `stage` touching `access`.
    fn begin_step(&mut self, stage: vk::PipelineStageFlags, access: vk::AccessFlags) {
        let (src_stage, src_access) = self.last;
        let barrier = [vk::MemoryBarrier::default()
            .src_access_mask(src_access)
            .dst_access_mask(access)];
        // SAFETY: the command buffer is recording on this thread.
        unsafe {
            self.device.raw().cmd_pipeline_barrier(
                self.cmd(),
                src_stage,
                stage,
                vk::DependencyFlags::empty(),
                &barrier,
                &[],
                &[],
            );
        }
    }

    fn end_step(&mut self, stage: vk::PipelineStageFlags, writes: vk::AccessFlags) {
        self.last = (stage, writes);
        self.steps += 1;
    }

    /// Bring a new texture from `UNDEFINED` to `GENERAL` (1.x
    /// `vk_cmd_transition_for_compute` at creation).
    pub(crate) fn init_layout(&mut self, texture: &Texture) {
        let barrier = [vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(texture.raw())
            .subresource_range(color_range(0, texture.slices()))];
        // SAFETY: the command buffer is recording; the image is alive (kept).
        unsafe {
            self.device.raw().cmd_pipeline_barrier(
                self.cmd(),
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barrier,
            );
        }
        self.keep(Keep::Texture(texture.clone()));
        self.end_step(
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::AccessFlags::MEMORY_WRITE,
        );
    }

    /// Copy regions of a buffer (capture staging) into a texture.
    ///
    /// No regions = the whole of slice 0, tightly packed from offset 0.
    pub fn copy_buffer_to_texture(&mut self, src: &Buffer, dst: &Texture, regions: &[CopyRegion]) {
        if !self.same_device(src.device()) || !self.same_device(dst.device()) {
            return self.fail(GpuError::Invalid(
                "copy_buffer_to_texture: resources of another device".into(),
            ));
        }
        if !dst.has_usage(TextureUsage::COPY_DST) {
            return self.fail(GpuError::Invalid(format!(
                "'{}' was not created with COPY_DST",
                dst.desc().label
            )));
        }
        let full = [CopyRegion {
            width: dst.width(),
            height: dst.height(),
            ..Default::default()
        }];
        let regions = if regions.is_empty() { &full[..] } else { regions };
        let mut copies = Vec::with_capacity(regions.len());
        for r in regions {
            if let Err(e) = check_buffer_region(r, src.size(), dst) {
                return self.fail(e);
            }
            if r.width == 0 || r.height == 0 {
                continue;
            }
            copies.push(
                vk::BufferImageCopy::default()
                    .buffer_offset(r.src_offset)
                    .buffer_row_length(r.src_row_length)
                    .buffer_image_height(0)
                    .image_subresource(layers(r.dst_slice))
                    .image_offset(vk::Offset3D {
                        x: r.dst_x as i32,
                        y: r.dst_y as i32,
                        z: 0,
                    })
                    .image_extent(vk::Extent3D {
                        width: r.width,
                        height: r.height,
                        depth: 1,
                    }),
            );
        }
        if copies.is_empty() {
            return;
        }
        self.begin_step(
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE,
        );
        // SAFETY: recording; both resources are alive (kept) and the regions
        // were checked against their sizes.
        unsafe {
            self.device.raw().cmd_copy_buffer_to_image(
                self.cmd(),
                src.raw(),
                dst.raw(),
                vk::ImageLayout::GENERAL,
                &copies,
            );
        }
        self.keep(Keep::Buffer(src.clone()));
        self.keep(Keep::Texture(dst.clone()));
        self.end_step(vk::PipelineStageFlags::TRANSFER, vk::AccessFlags::TRANSFER_WRITE);
    }

    /// Copy regions of one texture into another (tiles, lsfg inputs).
    ///
    /// For each region, `src_offset` is the source x and `src_y` the source
    /// y (slice 0); no regions = the common size, from (0, 0) to (0, 0).
    pub fn copy_texture(&mut self, src: &Texture, dst: &Texture, regions: &[CopyRegion]) {
        if !self.same_device(src.device()) || !self.same_device(dst.device()) {
            return self.fail(GpuError::Invalid(
                "copy_texture: resources of another device".into(),
            ));
        }
        if !src.has_usage(TextureUsage::COPY_SRC) && !src.has_usage(TextureUsage::PRESENT_SRC) {
            return self.fail(GpuError::Invalid(format!(
                "'{}' was not created with COPY_SRC",
                src.desc().label
            )));
        }
        if !dst.has_usage(TextureUsage::COPY_DST) {
            return self.fail(GpuError::Invalid(format!(
                "'{}' was not created with COPY_DST",
                dst.desc().label
            )));
        }
        if src.format().bytes_per_texel() != dst.format().bytes_per_texel() {
            return self.fail(GpuError::Invalid(format!(
                "copy_texture: {:?} and {:?} differ in size",
                src.format(),
                dst.format()
            )));
        }
        let full = [CopyRegion {
            width: src.width().min(dst.width()),
            height: src.height().min(dst.height()),
            ..Default::default()
        }];
        let regions = if regions.is_empty() { &full[..] } else { regions };
        let mut copies = Vec::with_capacity(regions.len());
        for r in regions {
            if let Err(e) = check_texture_region(r, src, dst) {
                return self.fail(e);
            }
            if r.width == 0 || r.height == 0 {
                continue;
            }
            copies.push(
                vk::ImageCopy::default()
                    .src_subresource(layers(0))
                    .src_offset(vk::Offset3D {
                        x: r.src_offset as i32,
                        y: r.src_y as i32,
                        z: 0,
                    })
                    .dst_subresource(layers(r.dst_slice))
                    .dst_offset(vk::Offset3D {
                        x: r.dst_x as i32,
                        y: r.dst_y as i32,
                        z: 0,
                    })
                    .extent(vk::Extent3D {
                        width: r.width,
                        height: r.height,
                        depth: 1,
                    }),
            );
        }
        if copies.is_empty() {
            return;
        }
        self.begin_step(
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE,
        );
        // SAFETY: recording; both images are alive (kept), in GENERAL, and
        // the regions were checked.
        unsafe {
            self.device.raw().cmd_copy_image(
                self.cmd(),
                src.raw(),
                vk::ImageLayout::GENERAL,
                dst.raw(),
                vk::ImageLayout::GENERAL,
                &copies,
            );
        }
        self.keep(Keep::Texture(src.clone()));
        self.keep(Keep::Texture(dst.clone()));
        self.end_step(vk::PipelineStageFlags::TRANSFER, vk::AccessFlags::TRANSFER_WRITE);
    }

    /// Copy a texture into a buffer (screenshots, readback).
    pub fn copy_texture_to_buffer(&mut self, src: &Texture, dst: &Buffer) {
        if !self.same_device(src.device()) || !self.same_device(dst.device()) {
            return self.fail(GpuError::Invalid(
                "copy_texture_to_buffer: resources of another device".into(),
            ));
        }
        if !src.has_usage(TextureUsage::COPY_SRC) && !src.has_usage(TextureUsage::PRESENT_SRC) {
            return self.fail(GpuError::Invalid(format!(
                "'{}' was not created with COPY_SRC",
                src.desc().label
            )));
        }
        if dst.size() < src.slice_size() {
            return self.fail(GpuError::Invalid(format!(
                "copy_texture_to_buffer: '{}' has {} bytes, {} needed",
                dst.desc().label,
                dst.size(),
                src.slice_size()
            )));
        }
        let copy = [vk::BufferImageCopy::default()
            .image_subresource(layers(0))
            .image_extent(vk::Extent3D {
                width: src.width(),
                height: src.height(),
                depth: 1,
            })];
        self.begin_step(
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE,
        );
        // SAFETY: recording; both resources are alive (kept) and the buffer
        // holds the whole slice.
        unsafe {
            self.device.raw().cmd_copy_image_to_buffer(
                self.cmd(),
                src.raw(),
                vk::ImageLayout::GENERAL,
                dst.raw(),
                &copy,
            );
        }
        self.keep(Keep::Texture(src.clone()));
        self.keep(Keep::Buffer(dst.clone()));
        // Host reads after the fence: make the writes visible to the host.
        self.end_step(vk::PipelineStageFlags::TRANSFER, vk::AccessFlags::TRANSFER_WRITE);
        self.begin_step(vk::PipelineStageFlags::HOST, vk::AccessFlags::HOST_READ);
        self.last = (vk::PipelineStageFlags::TRANSFER, vk::AccessFlags::TRANSFER_WRITE);
    }

    /// Fill a texture with one color (every slice). Needs `COPY_DST`.
    pub fn clear(&mut self, texture: &Texture, rgba: [f32; 4]) {
        if !self.same_device(texture.device()) {
            return self.fail(GpuError::Invalid("clear: texture of another device".into()));
        }
        if !texture.has_usage(TextureUsage::COPY_DST) {
            return self.fail(GpuError::Invalid(format!(
                "'{}' was not created with COPY_DST",
                texture.desc().label
            )));
        }
        let color = clear_value(texture.format().numeric_kind(), rgba);
        let range = [color_range(0, texture.slices())];
        self.begin_step(vk::PipelineStageFlags::TRANSFER, vk::AccessFlags::TRANSFER_WRITE);
        // SAFETY: recording; the image is alive (kept) and in GENERAL.
        unsafe {
            self.device.raw().cmd_clear_color_image(
                self.cmd(),
                texture.raw(),
                vk::ImageLayout::GENERAL,
                &color,
                &range,
            );
        }
        self.keep(Keep::Texture(texture.clone()));
        self.end_step(vk::PipelineStageFlags::TRANSFER, vk::AccessFlags::TRANSFER_WRITE);
    }

    /// Next free pair of timestamp queries, resetting the pool on first use.
    fn timestamp_pair(&mut self) -> Option<(vk::QueryPool, u32)> {
        if self.timestamps_full {
            return None;
        }
        let index = self.timestamp_names.len() as u32 * 2;
        if index + 2 > TIMESTAMP_QUERIES {
            tracing::debug!("more than {} timestamps in one submission", TIMESTAMP_QUERIES / 2);
            self.timestamps_full = true;
            return None;
        }
        let pool = match self.frame().query_pool {
            Some(pool) => pool,
            None => {
                let Some(pool) = self.device.acquire_query_pool() else {
                    self.timestamps_full = true;
                    return None;
                };
                self.frame().query_pool = Some(pool);
                // SAFETY: recording; the pool is ours until the frame is
                // recycled.
                unsafe {
                    self.device
                        .raw()
                        .cmd_reset_query_pool(self.cmd(), pool, 0, TIMESTAMP_QUERIES)
                };
                pool
            }
        };
        Some((pool, index))
    }

    /// Record a dispatch.
    pub fn dispatch(&mut self, dispatch: &Dispatch<'_>) {
        let pipeline = dispatch.pipeline;
        if !self.same_device(pipeline.device()) {
            return self.fail(GpuError::Invalid(format!(
                "dispatch '{}': pipeline of another device",
                pipeline.label()
            )));
        }
        if dispatch.push.len() as u32 > pipeline.push_size() || !dispatch.push.len().is_multiple_of(4) {
            return self.fail(GpuError::Invalid(format!(
                "dispatch '{}': {} push constant bytes (pipeline takes {})",
                pipeline.label(),
                dispatch.push.len(),
                pipeline.push_size()
            )));
        }
        self.begin_step(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::UNIFORM_READ,
        );
        let timestamp = match dispatch.timestamp {
            Some(name) => self.timestamp_pair().map(|q| (name, q)),
            None => None,
        };
        let raw = self.device.raw();
        let cmd = self.cmd();
        let [x, y, z] = dispatch.groups;
        // SAFETY: recording; the pipeline (and through it its resources) is
        // kept alive; the push range was checked against the layout.
        unsafe {
            if let Some((_, (pool, index))) = timestamp {
                raw.cmd_write_timestamp(cmd, vk::PipelineStageFlags::COMPUTE_SHADER, pool, index);
            }
            raw.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline.raw());
            raw.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                pipeline.layout(),
                0,
                &[pipeline.descriptor_set()],
                &[],
            );
            if !dispatch.push.is_empty() {
                raw.cmd_push_constants(
                    cmd,
                    pipeline.layout(),
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    dispatch.push,
                );
            }
            raw.cmd_dispatch(cmd, x, y, z);
            if let Some((_, (pool, index))) = timestamp {
                raw.cmd_write_timestamp(cmd, vk::PipelineStageFlags::COMPUTE_SHADER, pool, index + 1);
            }
        }
        if let Some((name, _)) = timestamp {
            self.timestamp_names.push(name.to_owned());
        }
        self.keep(Keep::Pipeline(pipeline.clone()));
        self.end_step(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
        );
    }

    /// Leave `texture` ready to be copied to a swapchain image.
    pub fn prepare_present(&mut self, texture: &Texture) {
        if !self.same_device(texture.device()) {
            return self.fail(GpuError::Invalid(
                "prepare_present: texture of another device".into(),
            ));
        }
        // The swapchain copies from GENERAL in a later submission; make this
        // recording's writes available to transfers.
        self.begin_step(vk::PipelineStageFlags::TRANSFER, vk::AccessFlags::TRANSFER_READ);
        self.keep(Keep::Texture(texture.clone()));
        self.end_step(vk::PipelineStageFlags::TRANSFER, vk::AccessFlags::empty());
    }

    /// Number of steps recorded.
    pub fn len(&self) -> usize {
        self.steps
    }

    /// Nothing recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `Device::submit`.
    pub(crate) fn submit(mut self, options: &SubmitOptions<'_>) -> Result<Submission> {
        let device = self.device.clone();
        let mut frame = self.frame.take().expect("recorder frame taken before submit");
        if let Some(e) = self.error.take() {
            device.release_frame(frame, false);
            return Err(e);
        }
        let foreign = options
            .wait
            .iter()
            .chain(options.signal)
            .any(|s| !Arc::ptr_eq(s.device(), &device));
        if foreign {
            device.release_frame(frame, false);
            return Err(GpuError::Invalid("submit: semaphore of another device".into()));
        }
        // SAFETY: the command buffer is recording on this thread.
        if let Err(e) = unsafe { device.raw().end_command_buffer(frame.cmd) } {
            device.release_frame(frame, false);
            return Err(GpuError::vk("vkEndCommandBuffer", e));
        }
        let waits: Vec<vk::Semaphore> = options.wait.iter().map(|s| s.raw()).collect();
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
        let signals: Vec<vk::Semaphore> = options.signal.iter().map(|s| s.raw()).collect();
        frame.keep.extend(
            options
                .wait
                .iter()
                .chain(options.signal)
                .map(|s| Keep::Semaphore((*s).clone())),
        );
        let cmds = [frame.cmd];
        let info = vk::SubmitInfo::default()
            .wait_semaphores(&waits)
            .wait_dst_stage_mask(&stages)
            .command_buffers(&cmds)
            .signal_semaphores(&signals);
        if let Err(e) = device.queue_submit(&[info], frame.fence) {
            device.release_frame(frame, false);
            return Err(e);
        }
        Ok(Submission {
            device,
            frame: Some(frame),
            names: std::mem::take(&mut self.timestamp_names),
            want_timestamps: options.timestamps,
        })
    }
}

impl Drop for CommandRecorder {
    fn drop(&mut self) {
        // Never submitted: the pool is reset when the frame is reused.
        if let Some(frame) = self.frame.take() {
            self.device.release_frame(frame, false);
        }
    }
}

fn layers(slice: u32) -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        mip_level: 0,
        base_array_layer: slice,
        layer_count: 1,
    }
}

fn clear_value(kind: NumericKind, rgba: [f32; 4]) -> vk::ClearColorValue {
    match kind {
        NumericKind::Float => vk::ClearColorValue { float32: rgba },
        NumericKind::Uint => vk::ClearColorValue {
            uint32: rgba.map(|c| c.max(0.0) as u32),
        },
        NumericKind::Sint => vk::ClearColorValue {
            int32: rgba.map(|c| c as i32),
        },
    }
}

/// Bounds of a buffer → texture region.
fn check_buffer_region(r: &CopyRegion, buffer_size: u64, dst: &Texture) -> Result<()> {
    let bpt = dst.format().bytes_per_texel() as u64;
    if r.dst_slice >= dst.slices()
        || r.dst_x as u64 + r.width as u64 > dst.width() as u64
        || r.dst_y as u64 + r.height as u64 > dst.height() as u64
    {
        return Err(GpuError::Invalid(format!(
            "copy region {r:?} outside '{}'",
            dst.desc().label
        )));
    }
    if r.src_row_length != 0 && r.src_row_length < r.width {
        return Err(GpuError::Invalid(format!(
            "copy region {r:?}: row length below width"
        )));
    }
    if r.width == 0 || r.height == 0 {
        return Ok(());
    }
    let row = if r.src_row_length == 0 {
        r.width
    } else {
        r.src_row_length
    } as u64;
    let needed = r.src_offset + (row * (r.height as u64 - 1) + r.width as u64) * bpt;
    if needed > buffer_size {
        return Err(GpuError::Invalid(format!(
            "copy region {r:?} reads {needed} bytes from a {buffer_size}-byte buffer"
        )));
    }
    if !r.src_offset.is_multiple_of(bpt) {
        return Err(GpuError::Invalid(format!(
            "copy region {r:?}: offset not texel aligned"
        )));
    }
    Ok(())
}

/// Bounds of a texture → texture region.
fn check_texture_region(r: &CopyRegion, src: &Texture, dst: &Texture) -> Result<()> {
    if r.src_offset + r.width as u64 > src.width() as u64
        || r.src_y as u64 + r.height as u64 > src.height() as u64
    {
        return Err(GpuError::Invalid(format!(
            "copy region {r:?} outside '{}'",
            src.desc().label
        )));
    }
    if r.dst_slice >= dst.slices()
        || r.dst_x as u64 + r.width as u64 > dst.width() as u64
        || r.dst_y as u64 + r.height as u64 > dst.height() as u64
    {
        return Err(GpuError::Invalid(format!(
            "copy region {r:?} outside '{}'",
            dst.desc().label
        )));
    }
    Ok(())
}

/// How to submit.
#[derive(Default)]
pub struct SubmitOptions<'a> {
    /// Read the dispatch timestamps back ([`Submission::wait`] returns them).
    pub timestamps: bool,
    /// Wait on these (lsfg output) before the work starts.
    pub wait: &'a [&'a ExternalSemaphore],
    /// Signal these (lsfg input ready) when the work ends.
    pub signal: &'a [&'a ExternalSemaphore],
}

/// GPU times of one submission.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timestamps {
    /// `(name, ms)` of every dispatch that asked for a timestamp, in order.
    pub dispatches: Vec<(String, f64)>,
    /// From the first to the last timestamp, in ms.
    pub total_ms: f64,
}

/// Work in flight.
pub struct Submission {
    device: Arc<Device>,
    /// `None` once recycled.
    frame: Option<Frame>,
    names: Vec<String>,
    want_timestamps: bool,
}

impl Submission {
    /// Whether it finished (does not block).
    pub fn is_done(&self) -> Result<bool> {
        let Some(frame) = &self.frame else { return Ok(true) };
        // SAFETY: the fence belongs to this device.
        unsafe { self.device.raw().get_fence_status(frame.fence) }.map_err(vk_err("vkGetFenceStatus"))
    }

    /// Block until it finishes (`frame_timeout`, in ns) and return the
    /// timestamps if they were asked for.
    pub fn wait(mut self, timeout_ns: u64) -> Result<Option<Timestamps>> {
        let Some(frame) = &self.frame else { return Ok(None) };
        let raw = self.device.raw();
        // SAFETY: the fence belongs to this device.
        unsafe { raw.wait_for_fences(&[frame.fence], true, timeout_ns) }
            .map_err(vk_err("vkWaitForFences"))?;
        let result = match (self.want_timestamps, frame.query_pool) {
            (true, Some(pool)) if !self.names.is_empty() => {
                let mut ticks = vec![0u64; self.names.len() * 2];
                // SAFETY: the work is done; these queries were reset and
                // written by it.
                unsafe { raw.get_query_pool_results(pool, 0, &mut ticks, vk::QueryResultFlags::TYPE_64) }
                    .map_err(vk_err("vkGetQueryPoolResults"))?;
                let caps = self.device.caps();
                Some(timestamps_from_ticks(
                    &self.names,
                    &ticks,
                    caps.timestamp_period.unwrap_or(1.0),
                    caps.timestamp_mask,
                ))
            }
            (true, _) => Some(Timestamps::default()),
            _ => None,
        };
        if let Some(frame) = self.frame.take() {
            self.device.release_frame(frame, true);
        }
        Ok(result)
    }
}

impl Drop for Submission {
    fn drop(&mut self) {
        let Some(frame) = self.frame.take() else { return };
        // Resources it uses must outlive the GPU work: wait a little, then
        // park it with the device (recycled by a later recorder).
        // SAFETY: the fence belongs to this device.
        let done = unsafe {
            self.device
                .raw()
                .wait_for_fences(&[frame.fence], true, 1_000_000_000)
        }
        .is_ok();
        if done {
            self.device.release_frame(frame, true);
        } else {
            tracing::warn!("submission dropped while the GPU still runs it");
            self.device.park_frame(frame);
        }
    }
}

/// Pair up `(before, after)` ticks per name; ms with `period_ns` per tick.
fn timestamps_from_ticks(names: &[String], ticks: &[u64], period_ns: f32, mask: u64) -> Timestamps {
    let ms = |from: u64, to: u64| (to.wrapping_sub(from) & mask) as f64 * period_ns as f64 / 1e6;
    let dispatches: Vec<(String, f64)> = names
        .iter()
        .zip(ticks.as_chunks::<2>().0)
        .map(|(name, pair)| (name.clone(), ms(pair[0], pair[1])))
        .collect();
    let total_ms = match (ticks.first(), ticks.last()) {
        (Some(&first), Some(&last)) if ticks.len() >= 2 => ms(first, last),
        _ => 0.0,
    };
    Timestamps { dispatches, total_ms }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_in_ms() {
        let names = vec!["a".to_owned(), "b".to_owned()];
        // 1 tick = 10 ns.
        let t = timestamps_from_ticks(&names, &[100, 200_100, 300_000, 400_000], 10.0, u64::MAX);
        assert_eq!(t.dispatches.len(), 2);
        assert!((t.dispatches[0].1 - 2.0).abs() < 1e-9);
        assert!((t.dispatches[1].1 - 1.0).abs() < 1e-9);
        assert!((t.total_ms - 3.999).abs() < 1e-9);
    }

    #[test]
    fn timestamps_wrap_with_valid_bits() {
        let names = vec!["a".to_owned()];
        let mask = (1u64 << 36) - 1;
        let t = timestamps_from_ticks(&names, &[mask - 9, 10], 1.0, mask);
        assert!((t.dispatches[0].1 - 20e-6).abs() < 1e-12);
    }

    #[test]
    fn clear_values_follow_the_format() {
        // SAFETY: reading the union member that was written.
        unsafe {
            assert_eq!(
                clear_value(NumericKind::Float, [0.5, 0.0, 1.0, 1.0]).float32,
                [0.5, 0.0, 1.0, 1.0]
            );
            assert_eq!(
                clear_value(NumericKind::Uint, [1.0, 2.0, -1.0, 255.0]).uint32,
                [1, 2, 0, 255]
            );
            assert_eq!(
                clear_value(NumericKind::Sint, [-1.0, 2.0, 0.0, 3.0]).int32,
                [-1, 2, 0, 3]
            );
        }
    }

    fn assert_send<T: Send>() {}

    #[test]
    fn recorder_and_submission_are_send() {
        assert_send::<CommandRecorder>();
        assert_send::<Submission>();
    }
}
