//! Samplers (`vk_sampler.cpp`).

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

/// A sampler. Cheap to clone.
#[derive(Clone)]
pub struct Sampler {
    _private: std::sync::Arc<()>,
}

impl Sampler {
    /// Raw handle.
    pub fn raw(&self) -> ash::vk::Sampler {
        todo!("front G")
    }
}
