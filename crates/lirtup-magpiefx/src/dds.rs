//! `//!SOURCE` lookup textures: `.dds` (and PNG/BMP/... through `image` in
//! front E's choice) (`dds.py`).
//!
//! Magpie loads these with DirectXTK's DDS loader (and WIC for the rest).
//! Only what MagpieFX effects can use is read:
//!
//! - uncompressed DDS with the legacy header (masks or a D3DFMT FourCC) or
//!   the `DX10` header, in one of the intermediate formats of
//!   [`crate::formats`] (plus `B8G8R8A8_UNORM`, turned into RGBA here);
//! - only the top mip of a single 2D texture;
//! - `.png`, `.bmp`, `.jpg` and `.tif` images, as `R8G8B8A8_UNORM`.
//!
//! Block-compressed files (BC1-BC7) are rejected: none of Magpie's effects
//! use them (RAVU and NIS use `R16G16B16A16_FLOAT`, SMAA `R8G8B8A8_UNORM` and
//! `R8_UNORM`). The texels keep their file format, so the runtime copies them
//! straight into a texture of that format (1.x expanded them to floats and
//! ran a compute pass; [`to_rgba_floats`] stays for tests and tools).

use std::path::{Path, PathBuf};

use crate::{MagpieError, Result};

/// A decoded lookup texture.
#[derive(Debug, Clone, PartialEq)]
pub struct DdsImage {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// DXGI format name of the data (`R8G8B8A8_UNORM`, `R16G16B16A16_FLOAT`, ...).
    pub format: String,
    /// Texel bytes, top mip only.
    pub data: Vec<u8>,
}

const HEADER_SIZE: usize = 124;
const DX10_SIZE: usize = 20;

/// `DDS_PIXELFORMAT` flags.
pub const DDPF_ALPHAPIXELS: u32 = 0x1;
/// Alpha only.
pub const DDPF_ALPHA: u32 = 0x2;
/// FourCC code.
pub const DDPF_FOURCC: u32 = 0x4;
/// RGB masks.
pub const DDPF_RGB: u32 = 0x40;
/// Luminance masks.
pub const DDPF_LUMINANCE: u32 = 0x20000;
/// Signed (bump map) masks.
pub const DDPF_BUMPDUDV: u32 = 0x80000;

const DDSCAPS2_CUBEMAP: u32 = 0x200;
const DDSCAPS2_VOLUME: u32 = 0x20_0000;
const DIMENSION_TEXTURE2D: u32 = 3;

/// Extensions read through `image`.
pub const IMAGE_EXTENSIONS: [&str; 6] = ["png", "bmp", "jpg", "jpeg", "tif", "tiff"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Float,
    Unorm,
    Snorm,
    /// R10G10B10A2.
    Packed,
}

/// How the texels of a format are stored: components, bytes per component,
/// kind.
fn layout(format: &str) -> Option<(usize, usize, Kind)> {
    use Kind::*;
    Some(match format {
        "R32G32B32A32_FLOAT" => (4, 4, Float),
        "R32G32_FLOAT" => (2, 4, Float),
        "R32_FLOAT" => (1, 4, Float),
        "R16G16B16A16_FLOAT" => (4, 2, Float),
        "R16G16_FLOAT" => (2, 2, Float),
        "R16_FLOAT" => (1, 2, Float),
        "R16G16B16A16_UNORM" => (4, 2, Unorm),
        "R16G16_UNORM" => (2, 2, Unorm),
        "R16_UNORM" => (1, 2, Unorm),
        "R16G16B16A16_SNORM" => (4, 2, Snorm),
        "R16G16_SNORM" => (2, 2, Snorm),
        "R16_SNORM" => (1, 2, Snorm),
        "R8G8B8A8_UNORM" => (4, 1, Unorm),
        "R8G8_UNORM" => (2, 1, Unorm),
        "R8_UNORM" => (1, 1, Unorm),
        "R8G8B8A8_SNORM" => (4, 1, Snorm),
        "R8G8_SNORM" => (2, 1, Snorm),
        "R8_SNORM" => (1, 1, Snorm),
        "R10G10B10A2_UNORM" => (4, 0, Packed),
        _ => return None,
    })
}

fn texel_size(format: &str) -> Option<usize> {
    layout(format).map(|(n, size, kind)| if kind == Kind::Packed { 4 } else { n * size })
}

/// DXGI numbers a lookup texture may use.
fn dxgi_format(n: u32) -> Option<&'static str> {
    Some(match n {
        2 => "R32G32B32A32_FLOAT",
        10 => "R16G16B16A16_FLOAT",
        11 => "R16G16B16A16_UNORM",
        13 => "R16G16B16A16_SNORM",
        16 => "R32G32_FLOAT",
        24 => "R10G10B10A2_UNORM",
        26 => "R11G11B10_FLOAT",
        28 => "R8G8B8A8_UNORM",
        31 => "R8G8B8A8_SNORM",
        34 => "R16G16_FLOAT",
        35 => "R16G16_UNORM",
        37 => "R16G16_SNORM",
        41 => "R32_FLOAT",
        49 => "R8G8_UNORM",
        51 => "R8G8_SNORM",
        54 => "R16_FLOAT",
        55 => "R16_UNORM",
        58 => "R16_SNORM",
        61 => "R8_UNORM",
        63 => "R8_SNORM",
        87 => "B8G8R8A8_UNORM",
        _ => return None,
    })
}

/// Block-compressed DXGI numbers (typeless, unorm and sRGB variants).
fn bc_format(n: u32) -> Option<&'static str> {
    Some(match n {
        70..=72 => "BC1",
        73..=75 => "BC2",
        76..=78 => "BC3",
        79..=81 => "BC4",
        82..=84 => "BC5",
        94..=96 => "BC6H",
        97..=99 => "BC7",
        _ => return None,
    })
}

/// D3DFMT values used as FourCC by D3DX/DirectXTex.
fn d3dfmt_fourcc(n: u32) -> Option<&'static str> {
    Some(match n {
        36 => "R16G16B16A16_UNORM",
        110 => "R16G16B16A16_SNORM",
        111 => "R16_FLOAT",
        112 => "R16G16_FLOAT",
        113 => "R16G16B16A16_FLOAT",
        114 => "R32_FLOAT",
        115 => "R32G32_FLOAT",
        116 => "R32G32B32A32_FLOAT",
        _ => return None,
    })
}

fn compressed_fourcc(code: &[u8; 4]) -> Option<&'static str> {
    Some(match code {
        b"DXT1" => "BC1",
        b"DXT2" | b"DXT3" => "BC2",
        b"DXT4" | b"DXT5" => "BC3",
        b"ATI1" | b"BC4U" | b"BC4S" => "BC4",
        b"ATI2" | b"BC5U" | b"BC5S" => "BC5",
        _ => return None,
    })
}

/// `(flags, bits, r, g, b, a)` masks → `(format, stored as BGRA)`.
fn mask_format(kind: u32, bits: u32, m: [u32; 4]) -> Option<(&'static str, bool)> {
    Some(match (kind, bits, m) {
        (DDPF_RGB, 32, [0xFF, 0xFF00, 0xFF_0000, 0xFF00_0000]) => ("R8G8B8A8_UNORM", false),
        (DDPF_RGB, 32, [0xFF_0000, 0xFF00, 0xFF, 0xFF00_0000]) => ("R8G8B8A8_UNORM", true),
        (DDPF_RGB, 32, [0xFFFF, 0xFFFF_0000, 0, 0]) => ("R16G16_UNORM", false),
        (DDPF_RGB, 32, [0xFFFF_FFFF, 0, 0, 0]) => ("R32_FLOAT", false),
        (DDPF_RGB, 32, [0x3FF, 0xF_FC00, 0x3FF0_0000, 0xC000_0000]) => ("R10G10B10A2_UNORM", false),
        // D3DX writes R10G10B10A2 with the red and blue masks swapped
        (DDPF_RGB, 32, [0x3FF0_0000, 0xF_FC00, 0x3FF, 0xC000_0000]) => ("R10G10B10A2_UNORM", false),
        (DDPF_LUMINANCE, 8, [0xFF, 0, 0, 0]) => ("R8_UNORM", false),
        (DDPF_LUMINANCE, 16, [0xFFFF, 0, 0, 0]) => ("R16_UNORM", false),
        (DDPF_LUMINANCE, 16, [0xFF, 0, 0, 0xFF00]) => ("R8G8_UNORM", false),
        (DDPF_BUMPDUDV, 16, [0xFF, 0xFF00, 0, 0]) => ("R8G8_SNORM", false),
        (DDPF_BUMPDUDV, 32, [0xFF, 0xFF00, 0xFF_0000, 0xFF00_0000]) => ("R8G8B8A8_SNORM", false),
        (DDPF_BUMPDUDV, 32, [0xFFFF, 0xFFFF_0000, 0, 0]) => ("R16G16_SNORM", false),
        _ => return None,
    })
}

fn texture_error(message: impl Into<String>) -> MagpieError {
    MagpieError::Texture {
        path: String::new(),
        message: message.into(),
    }
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().expect("4 bytes"))
}

fn legacy_format(flags: u32, fourcc: u32, bits: u32, masks: [u32; 4]) -> Result<(&'static str, bool)> {
    if flags & DDPF_FOURCC != 0 {
        let code = fourcc.to_le_bytes();
        if let Some(bc) = compressed_fourcc(&code) {
            return Err(texture_error(format!(
                "block-compressed DDS ({bc}) is not supported"
            )));
        }
        if let Some(fmt) = d3dfmt_fourcc(fourcc) {
            return Ok((fmt, false));
        }
        let shown = if code.iter().all(u8::is_ascii_alphanumeric) {
            String::from_utf8_lossy(&code).into_owned()
        } else {
            fourcc.to_string()
        };
        return Err(texture_error(format!("unsupported DDS FourCC '{shown}'")));
    }
    let kind = flags & (DDPF_RGB | DDPF_LUMINANCE | DDPF_BUMPDUDV | DDPF_ALPHA);
    mask_format(kind, bits, masks).ok_or_else(|| {
        texture_error(format!(
            "unsupported DDS pixel format (flags 0x{flags:x}, {bits} bits, masks 0x{:x}/0x{:x}/0x{:x}/0x{:x})",
            masks[0], masks[1], masks[2], masks[3]
        ))
    })
}

/// Parse a `.dds` file. BGRA data (`B8G8R8A8_UNORM` or swapped masks) comes
/// back as `R8G8B8A8_UNORM` with the bytes reordered.
pub fn parse_dds(data: &[u8]) -> Result<DdsImage> {
    if data.len() < 4 + HEADER_SIZE || &data[..4] != b"DDS " {
        return Err(texture_error("not a DDS file"));
    }
    let size = u32_at(data, 4);
    let height = u32_at(data, 12);
    let width = u32_at(data, 16);
    let depth = u32_at(data, 24);
    if size as usize != HEADER_SIZE {
        return Err(texture_error(format!("invalid DDS header size {size}")));
    }
    let pf_size = u32_at(data, 76);
    if pf_size != 32 {
        return Err(texture_error(format!("invalid DDS pixel format size {pf_size}")));
    }
    let (pf_flags, fourcc, bits) = (u32_at(data, 80), u32_at(data, 84), u32_at(data, 88));
    let masks = [
        u32_at(data, 92),
        u32_at(data, 96),
        u32_at(data, 100),
        u32_at(data, 104),
    ];
    let caps2 = u32_at(data, 112);
    if caps2 & DDSCAPS2_CUBEMAP != 0 {
        return Err(texture_error("cube map DDS textures are not supported"));
    }
    if caps2 & DDSCAPS2_VOLUME != 0 || depth > 1 {
        return Err(texture_error("volume DDS textures are not supported"));
    }

    let mut offset = 4 + HEADER_SIZE;
    let (format, swap_rb) = if pf_flags & DDPF_FOURCC != 0 && &fourcc.to_le_bytes() == b"DX10" {
        if data.len() < offset + DX10_SIZE {
            return Err(texture_error("truncated DDS DX10 header"));
        }
        let (dxgi, dimension, misc, array_size) = (
            u32_at(data, offset),
            u32_at(data, offset + 4),
            u32_at(data, offset + 8),
            u32_at(data, offset + 12),
        );
        offset += DX10_SIZE;
        if dimension != DIMENSION_TEXTURE2D || misc & 0x4 != 0 {
            return Err(texture_error("only 2D DDS textures are supported"));
        }
        if array_size > 1 {
            return Err(texture_error("DDS texture arrays are not supported"));
        }
        if let Some(bc) = bc_format(dxgi) {
            return Err(texture_error(format!(
                "block-compressed DDS ({bc}) is not supported"
            )));
        }
        match dxgi_format(dxgi) {
            Some("B8G8R8A8_UNORM") => ("R8G8B8A8_UNORM", true),
            Some(fmt) => (fmt, false),
            None => {
                return Err(texture_error(format!(
                    "unsupported DXGI format {dxgi} in DDS file"
                )));
            }
        }
    } else {
        legacy_format(pf_flags, fourcc, bits, masks)?
    };

    let texel = texel_size(format)
        .ok_or_else(|| texture_error(format!("DDS format {format} cannot be used as a lookup texture")))?;
    if width == 0 || height == 0 {
        return Err(texture_error("DDS texture has a zero size"));
    }
    let needed = width as usize * height as usize * texel;
    if data.len() < offset + needed {
        return Err(texture_error(format!(
            "truncated DDS data ({} bytes, need {needed})",
            data.len() - offset
        )));
    }
    let mut texels = data[offset..offset + needed].to_vec();
    if swap_rb {
        for px in texels.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
    }
    Ok(DdsImage {
        width,
        height,
        format: format.to_owned(),
        data: texels,
    })
}

/// Load a lookup texture from disk (by extension). Errors name the file.
pub fn load_image(path: &Path) -> Result<DdsImage> {
    let shown = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let with_path = |e: MagpieError| match e {
        MagpieError::Texture { message, .. } => MagpieError::Texture {
            path: shown.clone(),
            message,
        },
        other => other,
    };
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext == "dds" {
        let data = std::fs::read(path).map_err(|e| MagpieError::Texture {
            path: path.display().to_string(),
            message: format!("cannot read: {e}"),
        })?;
        return parse_dds(&data).map_err(with_path);
    }
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        let img = image::open(path)
            .map_err(|e| MagpieError::Texture {
                path: path.display().to_string(),
                message: format!("cannot read: {e}"),
            })?
            .to_rgba8();
        return Ok(DdsImage {
            width: img.width(),
            height: img.height(),
            format: "R8G8B8A8_UNORM".to_owned(),
            data: img.into_raw(),
        });
    }
    Err(MagpieError::Texture {
        path: shown,
        message: format!("unsupported lookup texture type '.{ext}'"),
    })
}

/// The format to create the texture with, from the declared `//!FORMAT` and
/// the file (`source_format`): a declared format must match the file.
pub fn source_format(declared: &str, image: &DdsImage) -> Result<String> {
    if !declared.is_empty() && declared != image.format {
        return Err(texture_error(format!(
            "//!FORMAT {declared} does not match the file ({})",
            image.format
        )));
    }
    Ok(image.format.clone())
}

/// Path of a `//!SOURCE` file, relative to the effect's folder. Absolute
/// paths and paths that leave the folder are rejected (Magpie only looks
/// next to the effect). `\` separators are accepted.
pub fn resolve_source_path(effect_dir: &Path, source: &str) -> Result<PathBuf> {
    let bad = |message: String| MagpieError::Texture {
        path: source.to_owned(),
        message,
    };
    let normalized = source.replace('\\', "/");
    if source.trim().is_empty() || Path::new(&normalized).is_absolute() {
        return Err(bad("invalid //!SOURCE path".to_owned()));
    }
    let base = effect_dir
        .canonicalize()
        .unwrap_or_else(|_| effect_dir.to_path_buf());
    let joined = base.join(&normalized);
    let path = joined
        .canonicalize()
        .unwrap_or_else(|_| lexical_normalize(&joined));
    if !path.starts_with(&base) {
        return Err(bad("//!SOURCE path leaves the effect's directory".to_owned()));
    }
    Ok(path)
}

/// Resolve `.` and `..` without touching the disk (for files that do not
/// exist, where `canonicalize` fails).
fn lexical_normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// IEEE half → f32.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1F) as u32;
    let mant = (h & 0x3FF) as u32;
    let bits = match (exp, mant) {
        (0, 0) => sign,
        (0, m) => {
            // subnormal: normalize
            let shift = m.leading_zeros() - 21;
            let m = (m << shift) & 0x3FF;
            sign | ((113 - shift) << 23) | (m << 13)
        }
        (0x1F, m) => sign | 0x7F80_0000 | (m << 13),
        (e, m) => sign | ((e + 112) << 23) | (m << 13),
    };
    f32::from_bits(bits)
}

/// Expand the texels to `width * height * 4` floats (RGBA, row-major).
/// Missing channels read as 0 (green, blue) and 1 (alpha), as a texture load
/// of that format would return.
pub fn to_rgba_floats(image: &DdsImage) -> Result<Vec<f32>> {
    let (components, size, kind) = layout(&image.format)
        .ok_or_else(|| texture_error(format!("format {} cannot be expanded", image.format)))?;
    let n = image.width as usize * image.height as usize;
    let d = &image.data;
    let mut out = Vec::with_capacity(n * 4);
    if kind == Kind::Packed {
        for w in d.as_chunks::<4>().0.iter().take(n) {
            let w = u32::from_le_bytes(*w);
            out.extend_from_slice(&[
                (w & 0x3FF) as f32 / 1023.0,
                ((w >> 10) & 0x3FF) as f32 / 1023.0,
                ((w >> 20) & 0x3FF) as f32 / 1023.0,
                (w >> 30) as f32 / 3.0,
            ]);
        }
        return Ok(out);
    }
    let value = |i: usize| -> f32 {
        let b = &d[i * size..(i + 1) * size];
        match (kind, size) {
            (Kind::Float, 4) => f32::from_le_bytes(b.try_into().expect("4 bytes")),
            (Kind::Float, _) => f16_to_f32(u16::from_le_bytes(b.try_into().expect("2 bytes"))),
            (Kind::Unorm, 1) => b[0] as f32 / 255.0,
            (Kind::Unorm, _) => u16::from_le_bytes(b.try_into().expect("2 bytes")) as f32 / 65535.0,
            (Kind::Snorm, 1) => (b[0] as i8 as f32 / 127.0).max(-1.0),
            (Kind::Snorm, _) => {
                (i16::from_le_bytes(b.try_into().expect("2 bytes")) as f32 / 32767.0).max(-1.0)
            }
            (Kind::Packed, _) => unreachable!(),
        }
    };
    for t in 0..n {
        let mut px = [0.0, 0.0, 0.0, 1.0];
        for (c, slot) in px.iter_mut().enumerate().take(components) {
            *slot = value(t * components + c);
        }
        out.extend_from_slice(&px);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DDS file (`make_dds` of 1.x `test_magpiefx_dds.py`); `dx10` is
    /// `(dxgi, dimension, misc, array)`.
    struct Dds {
        width: u32,
        height: u32,
        flags: u32,
        fourcc: u32,
        bits: u32,
        masks: [u32; 4],
        dx10: Option<[u32; 4]>,
        caps2: u32,
        depth: u32,
    }

    impl Dds {
        fn new(width: u32, height: u32) -> Dds {
            Dds {
                width,
                height,
                flags: 0,
                fourcc: 0,
                bits: 0,
                masks: [0; 4],
                dx10: None,
                caps2: 0,
                depth: 0,
            }
        }

        fn fourcc(mut self, code: u32) -> Dds {
            self.flags = DDPF_FOURCC;
            self.fourcc = code;
            self
        }

        fn masks(mut self, flags: u32, bits: u32, masks: [u32; 4]) -> Dds {
            self.flags = flags;
            self.bits = bits;
            self.masks = masks;
            self
        }

        fn dx10(self, dxgi: u32) -> Dds {
            self.dx10_full([dxgi, 3, 0, 1])
        }

        fn dx10_full(mut self, dx10: [u32; 4]) -> Dds {
            self = self.fourcc(u32::from_le_bytes(*b"DX10"));
            self.dx10 = Some(dx10);
            self
        }

        fn build(&self, data: &[u8]) -> Vec<u8> {
            let mut out = b"DDS ".to_vec();
            let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
            for v in [124, 0x100F, self.height, self.width, 0, self.depth, 1] {
                put(v);
            }
            for _ in 0..11 {
                put(0);
            }
            for v in [32, self.flags, self.fourcc, self.bits] {
                put(v);
            }
            for v in self.masks {
                put(v);
            }
            for v in [0x1000, self.caps2, 0, 0, 0] {
                put(v);
            }
            if let Some(dx10) = self.dx10 {
                for v in dx10 {
                    put(v);
                }
                put(0);
            }
            assert!(out.len() == 128 || out.len() == 148);
            out.extend_from_slice(data);
            out
        }
    }

    fn halves(values: &[u16]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn floats(img: &DdsImage) -> Vec<f32> {
        to_rgba_floats(img).unwrap()
    }

    fn approx(a: &[f32], b: &[f32]) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-6, "{a:?} != {b:?}");
        }
    }

    fn message(r: Result<DdsImage>) -> String {
        match r {
            Err(MagpieError::Texture { message, .. }) => message,
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn half_floats() {
        assert_eq!(f16_to_f32(0x3C00), 1.0);
        assert_eq!(f16_to_f32(0xC000), -2.0);
        assert_eq!(f16_to_f32(0x7BFF), 65504.0);
        assert_eq!(f16_to_f32(0x0001), 5.960_464_5e-8);
        assert_eq!(f16_to_f32(0x8000).to_bits(), 0x8000_0000);
        assert!(f16_to_f32(0x7C00).is_infinite());
        assert!(f16_to_f32(0x7E00).is_nan());
    }

    #[test]
    fn fourcc_113_is_rgba16_float() {
        // 0.5, -1, 2, 1, 0.25, 0, 65504, -0.125
        let data = halves(&[0x3800, 0xBC00, 0x4000, 0x3C00, 0x3400, 0x0000, 0x7BFF, 0xB000]);
        let img = parse_dds(&Dds::new(2, 1).fourcc(113).build(&data)).unwrap();
        assert_eq!(
            (img.width, img.height, img.format.as_str()),
            (2, 1, "R16G16B16A16_FLOAT")
        );
        assert_eq!(img.data, data);
        assert_eq!(floats(&img), [0.5, -1.0, 2.0, 1.0, 0.25, 0.0, 65504.0, -0.125]);
    }

    #[test]
    fn rgba8_masks() {
        let dds = Dds::new(1, 1).masks(
            DDPF_RGB | DDPF_ALPHAPIXELS,
            32,
            [0xFF, 0xFF00, 0xFF_0000, 0xFF00_0000],
        );
        let img = parse_dds(&dds.build(&[10, 20, 30, 40])).unwrap();
        assert_eq!(img.format, "R8G8B8A8_UNORM");
        approx(
            &floats(&img),
            &[10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 40.0 / 255.0],
        );
    }

    #[test]
    fn bgra8_masks_are_read_as_rgba() {
        let dds = Dds::new(1, 1).masks(
            DDPF_RGB | DDPF_ALPHAPIXELS,
            32,
            [0xFF_0000, 0xFF00, 0xFF, 0xFF00_0000],
        );
        let img = parse_dds(&dds.build(&[10, 20, 30, 40])).unwrap();
        assert_eq!(img.format, "R8G8B8A8_UNORM");
        assert_eq!(img.data, [30, 20, 10, 40]);
        approx(
            &floats(&img),
            &[30.0 / 255.0, 20.0 / 255.0, 10.0 / 255.0, 40.0 / 255.0],
        );
    }

    #[test]
    fn luminance_and_bump_formats() {
        let l8 = parse_dds(
            &Dds::new(2, 1)
                .masks(DDPF_LUMINANCE, 8, [0xFF, 0, 0, 0])
                .build(&[0, 255]),
        )
        .unwrap();
        assert_eq!(l8.format, "R8_UNORM");
        approx(&floats(&l8), &[0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0]);

        let la = Dds::new(1, 1).masks(DDPF_LUMINANCE | DDPF_ALPHAPIXELS, 16, [0xFF, 0, 0, 0xFF00]);
        let la = parse_dds(&la.build(&[255, 0])).unwrap();
        assert_eq!(la.format, "R8G8_UNORM");
        approx(&floats(&la), &[1.0, 0.0, 0.0, 1.0]);

        // -128 clamps to -1
        let uv = parse_dds(
            &Dds::new(1, 1)
                .masks(DDPF_BUMPDUDV, 16, [0xFF, 0xFF00, 0, 0])
                .build(&[0x80, 0x7F]),
        )
        .unwrap();
        assert_eq!(uv.format, "R8G8_SNORM");
        approx(&floats(&uv), &[-1.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn dx10_formats() {
        let r16 = parse_dds(&Dds::new(2, 1).dx10(55).build(&[0, 0, 0xFF, 0xFF])).unwrap();
        assert_eq!(r16.format, "R16_UNORM");
        approx(&floats(&r16), &[0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0]);

        let r32 = parse_dds(&Dds::new(1, 1).dx10(41).build(&3.5f32.to_le_bytes())).unwrap();
        assert_eq!(
            (r32.format.as_str(), floats(&r32)),
            ("R32_FLOAT", vec![3.5, 0.0, 0.0, 1.0])
        );

        let rg: Vec<u8> = [1.5f32, -2.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        let rg32 = parse_dds(&Dds::new(1, 1).dx10(16).build(&rg)).unwrap();
        assert_eq!(
            (rg32.format.as_str(), floats(&rg32)),
            ("R32G32_FLOAT", vec![1.5, -2.0, 0.0, 1.0])
        );

        let bgra = parse_dds(&Dds::new(1, 1).dx10(87).build(&[0, 0, 255, 255])).unwrap();
        assert_eq!(bgra.format, "R8G8B8A8_UNORM");
        approx(&floats(&bgra), &[1.0, 0.0, 0.0, 1.0]);

        let word: u32 = 1023 | (512 << 20) | (3 << 30);
        let packed = parse_dds(&Dds::new(1, 1).dx10(24).build(&word.to_le_bytes())).unwrap();
        assert_eq!(packed.format, "R10G10B10A2_UNORM");
        approx(&floats(&packed), &[1.0, 0.0, 512.0 / 1023.0, 1.0]);
    }

    #[test]
    fn only_the_top_mip_is_kept() {
        let top = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut data = top.to_vec();
        data.extend_from_slice(&[9, 9]);
        let img = parse_dds(&Dds::new(2, 1).dx10(28).build(&data)).unwrap();
        assert_eq!(img.data, top);
    }

    #[test]
    fn rejected_files() {
        for code in [b"DXT1", b"DXT5", b"ATI2", b"BC4U"] {
            let msg = message(parse_dds(
                &Dds::new(4, 4).fourcc(u32::from_le_bytes(*code)).build(&[0; 64]),
            ));
            assert!(msg.contains("block-compressed"), "{msg}");
        }
        for dxgi in [71, 77, 98] {
            assert!(
                message(parse_dds(&Dds::new(4, 4).dx10(dxgi).build(&[0; 64]))).contains("block-compressed")
            );
        }
        assert_eq!(message(parse_dds(b"nope")), "not a DDS file");
        assert_eq!(
            message(parse_dds(&Dds::new(1, 1).build(&[])[..100])),
            "not a DDS file"
        );
        assert!(message(parse_dds(&Dds::new(4, 4).dx10(28).build(&[0; 8]))).contains("truncated DDS data"));

        let mut cube = Dds::new(1, 1).dx10(28);
        cube.caps2 = 0x200;
        assert!(message(parse_dds(&cube.build(&[0; 4]))).contains("cube"));
        let mut volume = Dds::new(1, 1).dx10(28);
        volume.depth = 2;
        assert!(message(parse_dds(&volume.build(&[0; 4]))).contains("volume"));
        assert!(
            message(parse_dds(&Dds::new(1, 1).dx10_full([28, 3, 0, 2]).build(&[0; 8]))).contains("arrays")
        );
        assert!(message(parse_dds(&Dds::new(1, 1).dx10_full([28, 3, 4, 1]).build(&[0; 4]))).contains("2D"));
        assert!(message(parse_dds(&Dds::new(1, 1).dx10_full([28, 2, 0, 1]).build(&[0; 4]))).contains("2D"));

        assert!(
            message(parse_dds(
                &Dds::new(1, 1)
                    .masks(DDPF_RGB, 24, [0xFF, 0xFF00, 0xFF_0000, 0])
                    .build(&[0; 4])
            ))
            .contains("unsupported DDS pixel format")
        );
        assert!(
            message(parse_dds(
                &Dds::new(1, 1).fourcc(u32::from_le_bytes(*b"ABCD")).build(&[0; 4])
            ))
            .contains("FourCC 'ABCD'")
        );
        assert!(message(parse_dds(&Dds::new(1, 1).dx10(90).build(&[0; 4]))).contains("DXGI format 90"));
        // R11G11B10 is an intermediate format but not a lookup texture format
        assert!(message(parse_dds(&Dds::new(1, 1).dx10(26).build(&[0; 4]))).contains("cannot be used"));
        assert!(message(parse_dds(&Dds::new(0, 1).dx10(28).build(&[]))).contains("zero size"));
    }

    #[test]
    fn source_format_must_match() {
        let img = DdsImage {
            width: 1,
            height: 1,
            format: "R8_UNORM".into(),
            data: vec![0],
        };
        assert_eq!(source_format("", &img).unwrap(), "R8_UNORM");
        assert_eq!(source_format("R8_UNORM", &img).unwrap(), "R8_UNORM");
        assert!(message(source_format("R16_FLOAT", &img).map(|_| img.clone())).contains("does not match"));
    }

    #[test]
    fn load_image_dds_png_and_errors() {
        let dir = tempfile::tempdir().unwrap();
        let dds = dir.path().join("lut.dds");
        std::fs::write(&dds, Dds::new(1, 1).fourcc(113).build(&[0; 8])).unwrap();
        assert_eq!(load_image(&dds).unwrap().format, "R16G16B16A16_FLOAT");

        let png = dir.path().join("a.png");
        image::RgbImage::from_pixel(2, 1, image::Rgb([255, 0, 0]))
            .save(&png)
            .unwrap();
        // the extension is matched without case
        let upper = dir.path().join("b.PNG");
        std::fs::copy(&png, &upper).unwrap();
        assert_eq!(load_image(&upper).unwrap().format, "R8G8B8A8_UNORM");
        let img = load_image(&png).unwrap();
        assert_eq!(
            (img.width, img.height, img.format.as_str()),
            (2, 1, "R8G8B8A8_UNORM")
        );
        assert_eq!(img.data, [255, 0, 0, 255, 255, 0, 0, 255]);

        let bad = dir.path().join("bad.dds");
        std::fs::write(&bad, b"nope").unwrap();
        match load_image(&bad) {
            Err(MagpieError::Texture { path, .. }) => assert_eq!(path, "bad.dds"),
            other => panic!("{other:?}"),
        }
        assert!(load_image(&dir.path().join("x.gif")).is_err());
        assert!(load_image(&dir.path().join("missing.dds")).is_err());
    }

    #[test]
    fn source_paths_stay_in_the_effect_folder() {
        let dir = tempfile::tempdir().unwrap();
        let effect_dir = dir.path().join("fx");
        std::fs::create_dir_all(effect_dir.join("sub")).unwrap();
        std::fs::write(effect_dir.join("lut.dds"), b"").unwrap();
        let base = effect_dir.canonicalize().unwrap();
        assert_eq!(
            resolve_source_path(&effect_dir, "lut.dds").unwrap(),
            base.join("lut.dds")
        );
        assert!(
            resolve_source_path(&effect_dir, "sub\\lut.dds")
                .unwrap()
                .ends_with("sub/lut.dds")
        );
        assert!(resolve_source_path(&effect_dir, "../x.dds").is_err());
        assert!(resolve_source_path(&effect_dir, "/etc/passwd").is_err());
        assert!(resolve_source_path(&effect_dir, " ").is_err());
    }

    #[test]
    fn bundled_dds_files_are_readable() {
        let mut count = 0;
        for entry in walk(&crate::catalog::builtin_dir()) {
            if entry.extension().is_some_and(|e| e == "dds") {
                let img = load_image(&entry).unwrap_or_else(|e| panic!("{}: {e}", entry.display()));
                assert!(crate::formats::lookup(&img.format).is_some(), "{}", img.format);
                assert_eq!(
                    to_rgba_floats(&img).unwrap().len(),
                    (img.width * img.height * 4) as usize
                );
                count += 1;
            }
        }
        assert_eq!(count, 17);
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path))
            } else {
                out.push(path)
            }
        }
        out
    }
}
