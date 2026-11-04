//! The split comparison of live tuning (block 3): two images of the same
//! size side by side, A left of a vertical split and B right of it, with a
//! thin line and a handle on the split (`misc/split_compose`). The presenter
//! renders A (the previous settings) and B (the current ones) once on a
//! frozen frame and composes them into its screen texture on every present;
//! the OSD, the stats panel and the bar then blend over the result.

use std::sync::Arc;

use lirtup_gpu::{Buffer, Device, Dispatch, Texture};

use crate::util::{self, Bindings, CbWriter};
use crate::{EffectError, Pass, Result};

const SHADER: &str = "misc/split_compose/split_compose.spv";

/// Width of the line at 1080 lines of output, in pixels.
pub const LINE_WIDTH: f32 = 2.0;
/// Size of the handle at 1080 lines of output, in pixels (width, height).
pub const HANDLE_SIZE: (f32, f32) = (24.0, 48.0);
/// Opacity of the line and of the handle.
pub const LINE_ALPHA: f32 = 0.7;

/// A split position as a fraction of the width: clamped to `0..=1`, and
/// `0.5` (the middle) when it is not a number.
pub fn clamp_split(x: f32) -> f32 {
    if x.is_nan() { 0.5 } else { x.clamp(0.0, 1.0) }
}

/// UI scale of the line and the handle for an output of `height` lines
/// (1.0 up to 1080p, then proportional: 2.0 at 4K).
pub fn ui_scale(height: u32) -> f32 {
    (height as f32 / 1080.0).max(1.0)
}

/// The constant buffer of the shader (`SplitParams`, 32 bytes) for an
/// output of `size` with the split at the fraction `split`.
pub fn split_params(size: (u32, u32), split: f32) -> CbWriter {
    let scale = ui_scale(size.1);
    let (hw, hh) = (HANDLE_SIZE.0 * scale, HANDLE_SIZE.1 * scale);
    CbWriter::new()
        .u32(size.0)
        .u32(size.1)
        .f32(clamp_split(split) * size.0 as f32)
        .f32(LINE_WIDTH * scale)
        .f32(hw)
        .f32(hh)
        .f32(hw * 0.5)
        .f32(LINE_ALPHA)
}

/// Writes `a` left of the split and `b` right of it into `out`, with the
/// split line and its handle on top.
pub struct SplitCompose {
    out: Texture,
    cb: Buffer,
    pipeline: lirtup_gpu::ComputePipeline,
    size: (u32, u32),
    split: f32,
}

impl SplitCompose {
    /// Compose `a` and `b` (same size as `out`, or larger: only `out`'s
    /// area is read) into `out` (RGBA8), with the split in the middle.
    pub fn new(device: &Arc<Device>, a: &Texture, b: &Texture, out: &Texture) -> Result<SplitCompose> {
        let size = (out.width(), out.height());
        if a.width() < size.0 || a.height() < size.1 || b.width() < size.0 || b.height() < size.1 {
            return Err(EffectError::Unsupported(format!(
                "split compose: images of {}x{} and {}x{} for an output of {}x{}",
                a.width(),
                a.height(),
                b.width(),
                b.height(),
                size.0,
                size.1
            )));
        }
        let spirv = crate::assets::native_spirv(SHADER).ok_or_else(|| EffectError::Asset {
            name: SHADER.to_owned(),
            message: "not embedded".to_owned(),
        })?;
        let split = 0.5;
        let cb = util::constant_buffer_with(device, split_params(size, split), "compare cb")?;
        let pipeline = util::compute(
            device,
            spirv,
            "main",
            &Bindings {
                cb: Some(&cb),
                srv: &[a, b],
                uav: &[out],
                samplers: &[],
                push_size: 0,
            },
            "compare",
        )?;
        Ok(SplitCompose {
            out: out.clone(),
            cb,
            pipeline,
            size,
            split,
        })
    }

    /// Move the split (fraction of the width, see [`clamp_split`]). Writes
    /// an `Upload` buffer: call it only when no recording of this pass is
    /// in flight (the presenter waits for every frame).
    pub fn set_split(&mut self, x: f32) -> Result<()> {
        let split = clamp_split(x);
        if split != self.split {
            self.cb.write(0, &split_params(self.size, split).finish())?;
            self.split = split;
        }
        Ok(())
    }

    /// The split, as a fraction of the width.
    pub fn split(&self) -> f32 {
        self.split
    }
}

impl Pass for SplitCompose {
    fn label(&self) -> &str {
        "compare"
    }

    fn output(&self) -> &Texture {
        &self.out
    }

    fn record(&self, recorder: &mut lirtup_gpu::CommandRecorder) -> Result<()> {
        recorder.dispatch(&Dispatch {
            pipeline: &self.pipeline,
            groups: util::groups(self.size, (16, 16)),
            push: &[],
            timestamp: Some("compare"),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_is_clamped_and_nan_is_the_middle() {
        assert_eq!(clamp_split(0.25), 0.25);
        assert_eq!(clamp_split(-1.0), 0.0);
        assert_eq!(clamp_split(3.0), 1.0);
        assert_eq!(clamp_split(f32::NAN), 0.5);
        assert_eq!(clamp_split(f32::INFINITY), 1.0);
        assert_eq!(clamp_split(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn line_and_handle_grow_with_the_output() {
        assert_eq!(ui_scale(720), 1.0);
        assert_eq!(ui_scale(1080), 1.0);
        assert_eq!(ui_scale(2160), 2.0);
    }

    #[test]
    fn params_match_the_shader_layout() {
        let bytes = split_params((3840, 2160), 0.25).finish();
        assert_eq!(bytes.len(), 32);
        let word = |i: usize| [bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3]];
        let f = |i: usize| f32::from_le_bytes(word(i));
        assert_eq!(u32::from_le_bytes(word(0)), 3840);
        assert_eq!(u32::from_le_bytes(word(1)), 2160);
        assert_eq!(f(2), 960.0);
        assert_eq!(f(3), 4.0);
        assert_eq!((f(4), f(5)), (48.0, 96.0));
        assert_eq!(f(6), 24.0);
        assert_eq!(f(7), LINE_ALPHA);
        // Out of range splits are clamped before they reach the GPU.
        let bytes = split_params((100, 100), 7.0).finish();
        assert_eq!(
            f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            100.0
        );
    }

    /// Composes a solid red A and a solid blue B (BGRA bytes do not matter
    /// here: the shader only picks and blends) and reads back both sides
    /// and the line.
    #[test]
    #[ignore = "needs a Vulkan GPU"]
    fn composes_two_solid_images() {
        use lirtup_gpu::{Format, Instance, InstanceOptions, SubmitOptions};

        let instance = Instance::new(&InstanceOptions {
            app_name: "lirtup-compose-test".into(),
            debug: std::env::var_os("LIRTUP_VULKAN_DEBUG").is_some(),
        })
        .expect("Vulkan instance");
        let gpu = std::env::var("LIRTUP_GPU").ok();
        let index = instance.select_adapter(gpu.as_deref()).expect("a Vulkan adapter");
        let device = instance.create_device(index).expect("Vulkan device");

        let (w, h) = (256u32, 128u32);
        let solid = |rgba: [u8; 4], label: &str| {
            let t = util::texture(&device, w, h, Format::R8G8B8A8Unorm, label).expect("texture");
            util::upload_texture(&device, &t, &rgba.repeat((w * h) as usize)).expect("upload");
            t
        };
        let a = solid([255, 0, 0, 255], "a");
        let b = solid([0, 0, 255, 255], "b");
        let out = util::texture(&device, w, h, Format::R8G8B8A8Unorm, "out").expect("out");
        let mut pass = SplitCompose::new(&device, &a, &b, &out).expect("compose");
        pass.set_split(0.25).expect("split");
        assert_eq!(pass.split(), 0.25);

        let mut recorder = device.recorder().expect("recorder");
        pass.record(&mut recorder).expect("record");
        device
            .submit(recorder, &SubmitOptions::default())
            .expect("submit")
            .wait(10_000_000_000)
            .expect("wait");
        let data = out.download().expect("download");
        let px = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [data[i], data[i + 1], data[i + 2], data[i + 3]]
        };
        // Far from the line and the handle: A on the left, B on the right.
        assert_eq!(px(4, 4), [255, 0, 0, 255]);
        assert_eq!(px(w - 4, 4), [0, 0, 255, 255]);
        assert_eq!(px(w - 4, h / 2), [0, 0, 255, 255]);
        // On the line (x = 64), near the top: 70 % white over A or B.
        let [r, g, bl, _] = px(64, 4);
        assert!(g > 150 && r > 150 && bl > 150, "line pixel {:?}", px(64, 4));
        // A wrong-sized output is refused.
        let small = util::texture(&device, w * 2, h, Format::R8G8B8A8Unorm, "big out").expect("big");
        assert!(SplitCompose::new(&device, &a, &b, &small).is_err());
    }
}
