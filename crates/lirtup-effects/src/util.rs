//! Helpers shared by the passes (and by `lirtup-magpiefx`): constant buffer
//! packing, resource creation shorthands and one-off texture uploads.

use std::sync::Arc;

use lirtup_gpu::{
    AddressMode, Binding, Buffer, BufferDesc, ComputeDesc, ComputePipeline, CopyRegion, Device, Filter,
    Format, MemoryLocation, Sampler, SamplerDesc, SubmitOptions, Texture, TextureDesc, TextureUsage,
};

use crate::{EffectError, Result};

/// Little-endian packer for constant buffers. Every field of the 1.x
/// `struct.pack` layouts is a 4-byte word, so packing is sequential; the
/// result is padded to 16 bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CbWriter {
    bytes: Vec<u8>,
}

impl CbWriter {
    /// Empty buffer.
    pub fn new() -> CbWriter {
        CbWriter::default()
    }

    /// Append a `uint`.
    pub fn u32(mut self, v: u32) -> CbWriter {
        self.bytes.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// Append an `int`.
    pub fn i32(mut self, v: i32) -> CbWriter {
        self.bytes.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// Append a `float`.
    pub fn f32(mut self, v: f32) -> CbWriter {
        self.bytes.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// Append several floats.
    pub fn f32s(mut self, vs: &[f32]) -> CbWriter {
        for v in vs {
            self.bytes.extend_from_slice(&v.to_le_bytes());
        }
        self
    }

    /// Bytes written so far, unpadded.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Nothing written.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The bytes, padded with zeros to a multiple of 16.
    pub fn finish(mut self) -> Vec<u8> {
        let padded = self.bytes.len().div_ceil(16).max(1) * 16;
        self.bytes.resize(padded, 0);
        self.bytes
    }
}

/// A host-visible constant buffer of `size` bytes (rounded up to 16).
pub fn constant_buffer(device: &Arc<Device>, size: usize, label: &str) -> Result<Buffer> {
    let size = size.div_ceil(16).max(1) * 16;
    Ok(device.create_buffer(&BufferDesc {
        size: size as u64,
        stride: 0,
        format: None,
        location: MemoryLocation::Upload,
        label: label.to_owned(),
    })?)
}

/// A constant buffer holding `data` (padded by [`CbWriter::finish`]).
pub fn constant_buffer_with(device: &Arc<Device>, data: CbWriter, label: &str) -> Result<Buffer> {
    let bytes = data.finish();
    let cb = constant_buffer(device, bytes.len(), label)?;
    cb.write(0, &bytes)?;
    Ok(cb)
}

/// A plain 2D texture with the default usage.
pub fn texture(device: &Arc<Device>, width: u32, height: u32, format: Format, label: &str) -> Result<Texture> {
    texture_array(device, width, height, 1, false, format, label)
}

/// A texture array (`slices` layers).
pub fn texture_array(
    device: &Arc<Device>,
    width: u32,
    height: u32,
    slices: u32,
    force_array_view: bool,
    format: Format,
    label: &str,
) -> Result<Texture> {
    if width == 0 || height == 0 {
        return Err(EffectError::Unsupported(format!("{label}: zero-sized texture {width}x{height}")));
    }
    Ok(device.create_texture(&TextureDesc {
        width,
        height,
        format,
        slices,
        force_array_view,
        usage: TextureUsage::default(),
        label: label.to_owned(),
    })?)
}

/// A clamp-to-edge sampler with one filter (the 1.x `Sampler(filter_min=..,
/// filter_mag=..)`).
pub fn sampler(device: &Arc<Device>, filter: Filter) -> Result<Sampler> {
    sampler_with(device, filter, AddressMode::Clamp)
}

/// A sampler with one filter and one address mode on all axes.
pub fn sampler_with(device: &Arc<Device>, filter: Filter, address: AddressMode) -> Result<Sampler> {
    Ok(device.create_sampler(&SamplerDesc { address: [address; 3], min: filter, mag: filter })?)
}

/// What [`compute`] binds.
pub struct Bindings<'a> {
    /// Constant buffer (`b0`), if any.
    pub cb: Option<&'a Buffer>,
    /// Textures read (`t0..`).
    pub srv: &'a [&'a Texture],
    /// Textures written (`u0..`).
    pub uav: &'a [&'a Texture],
    /// Samplers (`s0..`).
    pub samplers: &'a [&'a Sampler],
    /// Push constant size.
    pub push_size: u32,
}

/// Create a compute pipeline from precompiled SPIR-V (entry `main`).
pub fn compute(
    device: &Arc<Device>,
    spirv: &[u8],
    entry: &str,
    bindings: &Bindings<'_>,
    label: &str,
) -> Result<ComputePipeline> {
    Ok(device.create_compute(&ComputeDesc {
        spirv,
        entry,
        cbv: bindings.cb.map(|b| vec![Binding::Buffer(b.clone())]).unwrap_or_default(),
        srv: bindings.srv.iter().map(|t| Binding::Texture((*t).clone())).collect(),
        uav: bindings.uav.iter().map(|t| Binding::Texture((*t).clone())).collect(),
        samplers: bindings.samplers.iter().map(|s| (*s).clone()).collect(),
        push_size: bindings.push_size,
        bindless_max: 0,
        label,
    })?)
}

/// Workgroups that cover `size` with `block`-sized groups.
pub fn groups(size: (u32, u32), block: (u32, u32)) -> [u32; 3] {
    [size.0.div_ceil(block.0).max(1), size.1.div_ceil(block.1).max(1), 1]
}

/// Upload tightly packed texels to every slice of `texture` (one slice after
/// another in `data`) and wait for the copy. For lookup tables, LUTs and the
/// blue noise, done once at creation.
pub fn upload_texture(device: &Arc<Device>, texture: &Texture, data: &[u8]) -> Result<()> {
    let (w, h, slices) = (texture.width(), texture.height(), texture.slices().max(1));
    let slice_bytes = (w * h * texture.format().bytes_per_texel()) as usize;
    if data.len() < slice_bytes * slices as usize {
        return Err(EffectError::Asset {
            name: texture.desc().label.clone(),
            message: format!("{} bytes for {w}x{h}x{slices} texels, need {}", data.len(), slice_bytes * slices as usize),
        });
    }
    let staging = device.create_buffer(&BufferDesc {
        size: (slice_bytes * slices as usize) as u64,
        stride: 0,
        format: None,
        location: MemoryLocation::Upload,
        label: format!("{} staging", texture.desc().label),
    })?;
    staging.write(0, &data[..slice_bytes * slices as usize])?;
    let regions: Vec<CopyRegion> = (0..slices)
        .map(|slice| CopyRegion {
            src_offset: (slice as usize * slice_bytes) as u64,
            dst_slice: slice,
            width: w,
            height: h,
            ..CopyRegion::default()
        })
        .collect();
    let mut recorder = device.recorder()?;
    recorder.copy_buffer_to_texture(&staging, texture, &regions);
    device.submit(recorder, &SubmitOptions::default())?.wait(u64::MAX)?;
    Ok(())
}

/// Clamp helper (`max(lo, min(v, hi))` of 1.x).
pub(crate) fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
    v.min(hi).max(lo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cb_writer_pads_to_16() {
        let bytes = CbWriter::new().f32(1.0).u32(2).i32(-1).finish();
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[0..4], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[4..8], &2u32.to_le_bytes());
        assert_eq!(&bytes[8..12], &(-1i32).to_le_bytes());
        assert_eq!(&bytes[12..16], &[0; 4]);
        assert_eq!(CbWriter::new().finish().len(), 16);
        assert_eq!(CbWriter::new().f32s(&[0.0; 5]).finish().len(), 32);
    }

    #[test]
    fn groups_round_up() {
        assert_eq!(groups((17, 16), (16, 16)), [2, 1, 1]);
        assert_eq!(groups((0, 0), (8, 8)), [1, 1, 1]);
    }
}
