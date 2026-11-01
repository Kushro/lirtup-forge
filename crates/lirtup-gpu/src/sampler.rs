//! Samplers (`vk_sampler.cpp`).

use std::sync::Arc;

use ash::vk;

use crate::error::vk_err;
use crate::{Device, Result};

/// Filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Filter {
    /// Nearest (the 1.x `SAMPLER_FILTER_POINT`).
    #[default]
    Point,
    /// Linear.
    Linear,
}

/// Address mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AddressMode {
    /// Repeat.
    Wrap,
    /// Mirrored repeat.
    Mirror,
    /// Clamp to edge (default).
    #[default]
    Clamp,
}

/// How to create a [`Sampler`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SamplerDesc {
    /// U, V, W address modes.
    pub address: [AddressMode; 3],
    /// Minification filter.
    pub min: Filter,
    /// Magnification filter.
    pub mag: Filter,
}

impl AddressMode {
    fn to_vk(self) -> vk::SamplerAddressMode {
        match self {
            AddressMode::Wrap => vk::SamplerAddressMode::REPEAT,
            AddressMode::Mirror => vk::SamplerAddressMode::MIRRORED_REPEAT,
            AddressMode::Clamp => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        }
    }
}

impl Filter {
    fn to_vk(self) -> vk::Filter {
        match self {
            Filter::Point => vk::Filter::NEAREST,
            Filter::Linear => vk::Filter::LINEAR,
        }
    }
}

impl SamplerDesc {
    /// `vk_Device_create_sampler`: point/point is the only nearest mip mode.
    fn to_vk(self) -> vk::SamplerCreateInfo<'static> {
        let mipmap = if self.min == Filter::Point && self.mag == Filter::Point {
            vk::SamplerMipmapMode::NEAREST
        } else {
            vk::SamplerMipmapMode::LINEAR
        };
        vk::SamplerCreateInfo::default()
            .address_mode_u(self.address[0].to_vk())
            .address_mode_v(self.address[1].to_vk())
            .address_mode_w(self.address[2].to_vk())
            .min_filter(self.min.to_vk())
            .mag_filter(self.mag.to_vk())
            .mipmap_mode(mipmap)
    }
}

struct SamplerInner {
    device: Arc<Device>,
    raw: vk::Sampler,
}

/// A sampler. Cheap to clone.
#[derive(Clone)]
pub struct Sampler {
    inner: Arc<SamplerInner>,
}

impl Sampler {
    pub(crate) fn create(device: &Arc<Device>, desc: &SamplerDesc) -> Result<Sampler> {
        // SAFETY: valid device and create info.
        let raw =
            unsafe { device.raw().create_sampler(&desc.to_vk(), None) }.map_err(vk_err("vkCreateSampler"))?;
        Ok(Sampler {
            inner: Arc::new(SamplerInner {
                device: device.clone(),
                raw,
            }),
        })
    }

    /// Raw handle.
    pub fn raw(&self) -> vk::Sampler {
        self.inner.raw
    }
}

impl Drop for SamplerInner {
    fn drop(&mut self) {
        // SAFETY: last handle; pipelines that bind it hold a clone.
        unsafe { self.device.raw().destroy_sampler(self.raw, None) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_and_modes() {
        let info = SamplerDesc {
            address: [AddressMode::Wrap, AddressMode::Mirror, AddressMode::Clamp],
            min: Filter::Linear,
            mag: Filter::Point,
        }
        .to_vk();
        assert_eq!(info.address_mode_u, vk::SamplerAddressMode::REPEAT);
        assert_eq!(info.address_mode_v, vk::SamplerAddressMode::MIRRORED_REPEAT);
        assert_eq!(info.address_mode_w, vk::SamplerAddressMode::CLAMP_TO_EDGE);
        assert_eq!(info.min_filter, vk::Filter::LINEAR);
        assert_eq!(info.mag_filter, vk::Filter::NEAREST);
        assert_eq!(info.mipmap_mode, vk::SamplerMipmapMode::LINEAR);
        assert_eq!(
            SamplerDesc::default().to_vk().mipmap_mode,
            vk::SamplerMipmapMode::NEAREST
        );
    }
}
