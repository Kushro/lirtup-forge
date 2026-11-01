//! Compute pipelines with their resources bound (`vk_compute.cpp`, `Compute`
//! in `vulkan/__init__.py`).
//!
//! Binding numbers follow the 1.x convention for SPIR-V from dxc (the
//! `-fvk-{b,t,u,s}-shift` offsets used by `scripts/hlsl-to-spv.fish` and by
//! the MagpieFX compiler): constant buffers, then SRVs, then UAVs, then
//! samplers, in the order given here.

use std::ffi::CString;
use std::sync::Arc;

use ash::vk;

use crate::error::vk_err;
use crate::{Buffer, Device, Format, GpuError, Result, Sampler, Texture, spirv};

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

/// First binding of each register class (the dxc `-fvk-*-shift` values).
pub(crate) const CBV_BASE: u32 = 0;
pub(crate) const SRV_BASE: u32 = 1024;
pub(crate) const UAV_BASE: u32 = 2048;
pub(crate) const SAMPLER_BASE: u32 = 3072;

impl Binding {
    fn device(&self) -> &Arc<Device> {
        match self {
            Binding::Buffer(b) => b.device(),
            Binding::Texture(t) => t.device(),
        }
    }

    /// Descriptor type in an `srv` (`uav` = false) or `uav` slot, as 1.x
    /// `vk_create_compute_descriptor_set_layout` picks it. Structured SRV
    /// buffers (stride > 0) are storage buffers, as dxc declares them.
    fn descriptor_type(&self, uav: bool) -> vk::DescriptorType {
        match (self, uav) {
            (Binding::Texture(_), false) => vk::DescriptorType::SAMPLED_IMAGE,
            (Binding::Texture(_), true) => vk::DescriptorType::STORAGE_IMAGE,
            (Binding::Buffer(b), false) if b.raw_view() != vk::BufferView::null() => {
                vk::DescriptorType::UNIFORM_TEXEL_BUFFER
            }
            (Binding::Buffer(b), false) if b.desc().stride > 0 => vk::DescriptorType::STORAGE_BUFFER,
            (Binding::Buffer(_), false) => vk::DescriptorType::UNIFORM_BUFFER,
            (Binding::Buffer(b), true) if b.raw_view() != vk::BufferView::null() => {
                vk::DescriptorType::STORAGE_TEXEL_BUFFER
            }
            (Binding::Buffer(_), true) => vk::DescriptorType::STORAGE_BUFFER,
        }
    }
}

/// What a descriptor write points to.
enum WriteInfo {
    Buffer(vk::DescriptorBufferInfo),
    Image(vk::DescriptorImageInfo),
    TexelView(vk::BufferView),
}

/// One descriptor write before the `VkWriteDescriptorSet` is built.
struct PendingWrite {
    binding: u32,
    element: u32,
    ty: vk::DescriptorType,
    info: WriteInfo,
}

fn write_for(binding: u32, element: u32, ty: vk::DescriptorType, resource: &Binding) -> PendingWrite {
    let info = match resource {
        Binding::Texture(t) => WriteInfo::Image(
            vk::DescriptorImageInfo::default()
                .image_view(t.raw_view())
                .image_layout(vk::ImageLayout::GENERAL),
        ),
        Binding::Buffer(b)
            if b.raw_view() != vk::BufferView::null() && ty != vk::DescriptorType::UNIFORM_BUFFER =>
        {
            WriteInfo::TexelView(b.raw_view())
        }
        Binding::Buffer(b) => WriteInfo::Buffer(
            vk::DescriptorBufferInfo::default()
                .buffer(b.raw())
                .offset(0)
                .range(b.size()),
        ),
    };
    PendingWrite {
        binding,
        element,
        ty,
        info,
    }
}

/// Layout bindings and writes for `desc` (1.x
/// `vk_create_compute_descriptor_set_layout` and
/// `vk_allocate_and_write_descriptor_set`).
fn plan_descriptors(
    desc: &ComputeDesc<'_>,
) -> (Vec<vk::DescriptorSetLayoutBinding<'static>>, Vec<PendingWrite>) {
    let mut layout = Vec::new();
    let mut writes = Vec::new();
    let mut add = |binding: u32, ty: vk::DescriptorType, count: u32| {
        layout.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(ty)
                .descriptor_count(count)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        );
    };
    if desc.bindless_max == 0 {
        for (i, b) in desc.cbv.iter().enumerate() {
            let slot = CBV_BASE + i as u32;
            add(slot, vk::DescriptorType::UNIFORM_BUFFER, 1);
            writes.push(write_for(slot, 0, vk::DescriptorType::UNIFORM_BUFFER, b));
        }
        for (i, b) in desc.srv.iter().enumerate() {
            let slot = SRV_BASE + i as u32;
            let ty = b.descriptor_type(false);
            add(slot, ty, 1);
            writes.push(write_for(slot, 0, ty, b));
        }
        for (i, b) in desc.uav.iter().enumerate() {
            let slot = UAV_BASE + i as u32;
            let ty = b.descriptor_type(true);
            add(slot, ty, 1);
            writes.push(write_for(slot, 0, ty, b));
        }
    } else {
        // Three arrays; only constant buffers and textures are written, as
        // in 1.x.
        add(CBV_BASE, vk::DescriptorType::UNIFORM_BUFFER, desc.bindless_max);
        add(SRV_BASE, vk::DescriptorType::SAMPLED_IMAGE, desc.bindless_max);
        add(UAV_BASE, vk::DescriptorType::STORAGE_IMAGE, desc.bindless_max);
        for (i, b) in desc.cbv.iter().enumerate().take(desc.bindless_max as usize) {
            writes.push(write_for(
                CBV_BASE,
                i as u32,
                vk::DescriptorType::UNIFORM_BUFFER,
                b,
            ));
        }
        for (i, b) in desc.srv.iter().enumerate().take(desc.bindless_max as usize) {
            if matches!(b, Binding::Texture(_)) {
                writes.push(write_for(
                    SRV_BASE,
                    i as u32,
                    vk::DescriptorType::SAMPLED_IMAGE,
                    b,
                ));
            }
        }
        for (i, b) in desc.uav.iter().enumerate().take(desc.bindless_max as usize) {
            if matches!(b, Binding::Texture(_)) {
                writes.push(write_for(
                    UAV_BASE,
                    i as u32,
                    vk::DescriptorType::STORAGE_IMAGE,
                    b,
                ));
            }
        }
    }
    for (i, s) in desc.samplers.iter().enumerate() {
        let slot = SAMPLER_BASE + i as u32;
        add(slot, vk::DescriptorType::SAMPLER, 1);
        writes.push(PendingWrite {
            binding: slot,
            element: 0,
            ty: vk::DescriptorType::SAMPLER,
            info: WriteInfo::Image(vk::DescriptorImageInfo::default().sampler(s.raw())),
        });
    }
    (layout, writes)
}

/// Pool sizes for a layout.
fn pool_sizes(layout: &[vk::DescriptorSetLayoutBinding<'_>]) -> Vec<vk::DescriptorPoolSize> {
    let mut sizes: Vec<vk::DescriptorPoolSize> = Vec::new();
    for b in layout {
        match sizes.iter_mut().find(|s| s.ty == b.descriptor_type) {
            Some(s) => s.descriptor_count += b.descriptor_count,
            None => sizes.push(vk::DescriptorPoolSize {
                ty: b.descriptor_type,
                descriptor_count: b.descriptor_count,
            }),
        }
    }
    sizes
}

/// UAV slots holding a BGRA texture (they need `NonReadable` when the device
/// cannot read storage images without a format).
fn bgra_uav_bindings(uav: &[Binding]) -> Vec<u32> {
    uav.iter()
        .enumerate()
        .filter(|(_, b)| {
            matches!(b, Binding::Texture(t) if matches!(t.format(), Format::B8G8R8A8Unorm | Format::B8G8R8A8UnormSrgb))
        })
        .map(|(i, _)| UAV_BASE + i as u32)
        .collect()
}

struct PipelineInner {
    device: Arc<Device>,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    push_size: u32,
    workgroup: [u32; 3],
    label: String,
    /// Kept alive while the descriptor set points to them.
    _bindings: Vec<Binding>,
    _samplers: Vec<Sampler>,
}

/// A compute pipeline and its descriptor set. Cheap to clone; keeps its
/// resources alive.
#[derive(Clone)]
pub struct ComputePipeline {
    inner: Arc<PipelineInner>,
}

impl ComputePipeline {
    /// `vk_Device_create_compute_impl`.
    pub(crate) fn create(device: &Arc<Device>, desc: &ComputeDesc<'_>) -> Result<ComputePipeline> {
        let label = if desc.label.is_empty() {
            "compute"
        } else {
            desc.label
        };
        let invalid = |what: String| GpuError::Invalid(format!("pipeline '{label}': {what}"));
        let foreign = desc
            .cbv
            .iter()
            .chain(&desc.srv)
            .chain(&desc.uav)
            .any(|b| !Arc::ptr_eq(b.device(), device));
        if foreign {
            return Err(invalid("a resource belongs to another device".into()));
        }
        if desc.push_size % 4 != 0 {
            return Err(invalid(format!(
                "push size {} is not a multiple of 4",
                desc.push_size
            )));
        }
        if desc.cbv.iter().any(|b| matches!(b, Binding::Texture(_))) {
            return Err(invalid("a texture in a constant buffer slot".into()));
        }
        if desc.bindless_max > 0 && !device.caps().bindless {
            return Err(GpuError::Unsupported(
                "bindless descriptors (descriptor indexing)".into(),
            ));
        }

        let mut words = spirv::words(desc.spirv)?;
        if !device.caps().read_without_format {
            for binding in bgra_uav_bindings(&desc.uav) {
                if let Some(patched) = spirv::patch_non_readable(&words, binding) {
                    words = patched;
                }
            }
        }
        let (entry, entry_id) = spirv::compute_entry(&words, desc.entry)
            .ok_or_else(|| invalid(format!("no GLCompute entry point '{}'", desc.entry)))?;
        let workgroup = spirv::workgroup_size(&words, entry_id).unwrap_or_else(|| {
            tracing::warn!("pipeline '{label}': no workgroup size in the SPIR-V, assuming 1x1x1");
            [1, 1, 1]
        });

        let (layout_bindings, writes) = plan_descriptors(desc);
        let update_after_bind = desc.bindless_max > 0;
        let flags: Vec<vk::DescriptorBindingFlags> = if update_after_bind {
            vec![
                vk::DescriptorBindingFlags::PARTIALLY_BOUND | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND;
                layout_bindings.len()
            ]
        } else {
            Vec::new()
        };
        let mut flags_info = vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&flags);

        let raw = device.raw();
        let mut inner = PipelineInner {
            device: device.clone(),
            pipeline: vk::Pipeline::null(),
            layout: vk::PipelineLayout::null(),
            set_layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            push_size: desc.push_size,
            workgroup,
            label: label.to_owned(),
            _bindings: desc
                .cbv
                .iter()
                .chain(&desc.srv)
                .chain(&desc.uav)
                .cloned()
                .collect(),
            _samplers: desc.samplers.clone(),
        };

        // SAFETY: every create info points to locals alive for its call;
        // objects created so far are destroyed by `PipelineInner::drop` if a
        // later step fails.
        unsafe {
            let mut set_layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&layout_bindings);
            if update_after_bind {
                set_layout_info = set_layout_info
                    .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                    .push_next(&mut flags_info);
            }
            inner.set_layout = raw
                .create_descriptor_set_layout(&set_layout_info, None)
                .map_err(vk_err("vkCreateDescriptorSetLayout"))?;

            let set_layouts = [inner.set_layout];
            let push_ranges = [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::COMPUTE,
                offset: 0,
                size: desc.push_size,
            }];
            let mut layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
            if desc.push_size > 0 {
                layout_info = layout_info.push_constant_ranges(&push_ranges);
            }
            inner.layout = raw
                .create_pipeline_layout(&layout_info, None)
                .map_err(vk_err("vkCreatePipelineLayout"))?;

            let module = raw
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
                .map_err(vk_err("vkCreateShaderModule"))?;
            let entry_c = CString::new(entry).map_err(|_| invalid("entry point name".into()))?;
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(module)
                .name(&entry_c);
            let info = [vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(inner.layout)];
            let created = raw.create_compute_pipelines(device.pipeline_cache(), &info, None);
            raw.destroy_shader_module(module, None);
            inner.pipeline = created.map_err(|(_, e)| GpuError::vk("vkCreateComputePipelines", e))?[0];

            let mut sizes = pool_sizes(&layout_bindings);
            if sizes.is_empty() {
                // A set with no bindings still has to be bound; a pool needs
                // at least one size.
                sizes.push(vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLER,
                    descriptor_count: 1,
                });
            }
            let mut pool_info = vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&sizes);
            if update_after_bind {
                pool_info = pool_info.flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND);
            }
            inner.pool = raw
                .create_descriptor_pool(&pool_info, None)
                .map_err(vk_err("vkCreateDescriptorPool"))?;
            let alloc = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(inner.pool)
                .set_layouts(&set_layouts);
            inner.set = raw
                .allocate_descriptor_sets(&alloc)
                .map_err(vk_err("vkAllocateDescriptorSets"))?[0];

            let vk_writes: Vec<vk::WriteDescriptorSet<'_>> = writes
                .iter()
                .map(|w| {
                    let base = vk::WriteDescriptorSet::default()
                        .dst_set(inner.set)
                        .dst_binding(w.binding)
                        .dst_array_element(w.element)
                        .descriptor_type(w.ty);
                    match &w.info {
                        WriteInfo::Buffer(info) => base.buffer_info(std::slice::from_ref(info)),
                        WriteInfo::Image(info) => base.image_info(std::slice::from_ref(info)),
                        WriteInfo::TexelView(view) => base.texel_buffer_view(std::slice::from_ref(view)),
                    }
                })
                .collect();
            raw.update_descriptor_sets(&vk_writes, &[]);
        }
        device.set_name(inner.pipeline, label);
        Ok(ComputePipeline {
            inner: Arc::new(inner),
        })
    }

    /// Workgroup size from the SPIR-V (`LocalSize`), to compute dispatch
    /// counts.
    pub fn workgroup_size(&self) -> [u32; 3] {
        self.inner.workgroup
    }

    /// Push constant size it was created with.
    pub fn push_size(&self) -> u32 {
        self.inner.push_size
    }

    /// Debug name.
    pub fn label(&self) -> &str {
        &self.inner.label
    }

    pub(crate) fn raw(&self) -> vk::Pipeline {
        self.inner.pipeline
    }

    pub(crate) fn layout(&self) -> vk::PipelineLayout {
        self.inner.layout
    }

    pub(crate) fn descriptor_set(&self) -> vk::DescriptorSet {
        self.inner.set
    }

    pub(crate) fn device(&self) -> &Arc<Device> {
        &self.inner.device
    }
}

impl Drop for PipelineInner {
    fn drop(&mut self) {
        let raw = self.device.raw();
        // SAFETY: last handle; submissions keep their pipelines alive until
        // their fence signals. Null handles are skipped.
        unsafe {
            if self.pipeline != vk::Pipeline::null() {
                raw.destroy_pipeline(self.pipeline, None);
            }
            if self.layout != vk::PipelineLayout::null() {
                raw.destroy_pipeline_layout(self.layout, None);
            }
            if self.pool != vk::DescriptorPool::null() {
                raw.destroy_descriptor_pool(self.pool, None);
            }
            if self.set_layout != vk::DescriptorSetLayout::null() {
                raw.destroy_descriptor_set_layout(self.set_layout, None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_sizes_add_up() {
        let b = |binding, ty, count| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(ty)
                .descriptor_count(count)
        };
        let layout = [
            b(0, vk::DescriptorType::UNIFORM_BUFFER, 1),
            b(1024, vk::DescriptorType::SAMPLED_IMAGE, 1),
            b(1025, vk::DescriptorType::SAMPLED_IMAGE, 1),
            b(2048, vk::DescriptorType::STORAGE_IMAGE, 8),
        ];
        let sizes = pool_sizes(&layout);
        assert_eq!(sizes.len(), 3);
        let count = |ty| sizes.iter().find(|s| s.ty == ty).map(|s| s.descriptor_count);
        assert_eq!(count(vk::DescriptorType::SAMPLED_IMAGE), Some(2));
        assert_eq!(count(vk::DescriptorType::STORAGE_IMAGE), Some(8));
    }

    #[test]
    fn empty_desc_has_no_bindings() {
        let (layout, writes) = plan_descriptors(&ComputeDesc::default());
        assert!(layout.is_empty());
        assert!(writes.is_empty());
    }

    fn assert_send_sync<T: Send + Sync + Clone>() {}

    #[test]
    fn pipeline_is_send_sync() {
        assert_send_sync::<ComputePipeline>();
    }
}
