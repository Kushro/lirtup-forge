//! A plain RGBA image (icons, thumbnails), so this crate needs no image crate.

/// 8-bit RGBA, rows tightly packed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RgbaImage {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// `width * height * 4` bytes, R, G, B, A.
    pub data: Vec<u8>,
}

/// Resize to `width × height`: box filter when shrinking (thumbnails, big
/// icons), bilinear when growing (small icons).
pub(crate) fn resize(src: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let (sw, sh) = (src.width as usize, src.height as usize);
    let (dw, dh) = (width.max(1) as usize, height.max(1) as usize);
    if sw == 0 || sh == 0 {
        return RgbaImage {
            width: dw as u32,
            height: dh as u32,
            data: vec![0; dw * dh * 4],
        };
    }
    if (sw, sh) == (dw, dh) {
        return src.clone();
    }
    let mut data = vec![0u8; dw * dh * 4];
    if dw <= sw && dh <= sh {
        for y in 0..dh {
            let y0 = y * sh / dh;
            let y1 = ((y + 1) * sh / dh).max(y0 + 1);
            for x in 0..dw {
                let x0 = x * sw / dw;
                let x1 = ((x + 1) * sw / dw).max(x0 + 1);
                let mut acc = [0u32; 4];
                for sy in y0..y1 {
                    let row = &src.data[(sy * sw + x0) * 4..(sy * sw + x1) * 4];
                    for px in row.as_chunks::<4>().0 {
                        for c in 0..4 {
                            acc[c] += u32::from(px[c]);
                        }
                    }
                }
                let n = ((y1 - y0) * (x1 - x0)) as u32;
                let o = (y * dw + x) * 4;
                for c in 0..4 {
                    data[o + c] = ((acc[c] + n / 2) / n) as u8;
                }
            }
        }
    } else {
        let fx = sw as f32 / dw as f32;
        let fy = sh as f32 / dh as f32;
        for y in 0..dh {
            let syf = ((y as f32 + 0.5) * fy - 0.5).clamp(0.0, (sh - 1) as f32);
            let sy0 = syf as usize;
            let sy1 = (sy0 + 1).min(sh - 1);
            let ty = syf - sy0 as f32;
            for x in 0..dw {
                let sxf = ((x as f32 + 0.5) * fx - 0.5).clamp(0.0, (sw - 1) as f32);
                let sx0 = sxf as usize;
                let sx1 = (sx0 + 1).min(sw - 1);
                let tx = sxf - sx0 as f32;
                let p = |xx: usize, yy: usize, c: usize| f32::from(src.data[(yy * sw + xx) * 4 + c]);
                let o = (y * dw + x) * 4;
                for c in 0..4 {
                    let top = p(sx0, sy0, c) * (1.0 - tx) + p(sx1, sy0, c) * tx;
                    let bottom = p(sx0, sy1, c) * (1.0 - tx) + p(sx1, sy1, c) * tx;
                    data[o + c] = (top * (1.0 - ty) + bottom * ty).round() as u8;
                }
            }
        }
    }
    RgbaImage {
        width: dw as u32,
        height: dh as u32,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> RgbaImage {
        RgbaImage {
            width: w,
            height: h,
            data: px.repeat((w * h) as usize),
        }
    }

    #[test]
    fn shrink_averages() {
        let mut img = solid(2, 1, [0, 0, 0, 255]);
        img.data[4..8].copy_from_slice(&[200, 100, 50, 255]);
        let out = resize(&img, 1, 1);
        assert_eq!(out.data, vec![100, 50, 25, 255]);
    }

    #[test]
    fn grow_keeps_solid_color() {
        let out = resize(&solid(2, 2, [10, 20, 30, 40]), 5, 3);
        assert_eq!((out.width, out.height), (5, 3));
        assert!(out.data.as_chunks::<4>().0.iter().all(|p| *p == [10, 20, 30, 40]));
    }
}
