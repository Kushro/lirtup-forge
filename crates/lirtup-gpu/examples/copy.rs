//! Headless round trip through `lirtup-gpu` (round 1 deliverable of front G):
//! upload → texture → texture copy → clear → compute dispatch (the 1.x
//! `clear.spv`) → readback, with timestamps and validation messages.
//!
//! ```text
//! cargo run -p lirtup-gpu --example copy [-- --gpu <index|name>] [--no-debug]
//! ```
//!
//! Exits non-zero if a readback does not match or the validation layer
//! reported anything.

use std::process::ExitCode;

use lirtup_gpu::{
    Binding, BufferDesc, ComputeDesc, CopyRegion, Dispatch, Format, Instance, InstanceOptions,
    MemoryLocation, SubmitOptions, TextureDesc, TextureUsage,
};

/// The 1.x clear shader: `RWTexture2D<float4>` (rgba16f) at u0 and a
/// constant buffer `{ float4 clearColor; uint2 size; }` at b0.
const CLEAR_SPV: &[u8] = include_bytes!("../../../src/upscaler/shaders/misc/clear/clear.spv");

const WIDTH: u32 = 300;
const HEIGHT: u32 = 200;

fn main() -> ExitCode {
    let mut gpu = None;
    let mut debug = true;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--gpu" => gpu = args.next(),
            "--no-debug" => debug = false,
            other => {
                eprintln!("unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }
    match run(gpu.as_deref(), debug) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(gpu: Option<&str>, debug: bool) -> lirtup_gpu::Result<bool> {
    let instance = Instance::new(&InstanceOptions {
        app_name: "lirtup-gpu-copy".into(),
        debug,
    })?;
    for a in instance.adapters() {
        println!(
            "[{}] {} ({:?}, {}, Vulkan {}, {} MiB, fp16 {}, external fd {}, robust {}, timestamps {:?})",
            a.index,
            a.name,
            a.kind,
            a.driver,
            a.api_version,
            a.dedicated_video_memory >> 20,
            a.supports_fp16,
            a.supports_external_fd,
            a.robust_image_access,
            a.timestamp_period_ns
        );
    }
    let index = instance
        .select_adapter(gpu)
        .ok_or_else(|| lirtup_gpu::GpuError::NoDevice("no Vulkan adapter".into()))?;
    let device = instance.create_device(index)?;
    println!("using [{index}] {}", device.info().name);

    let mut ok = true;
    let mut check = |what: &str, good: bool| {
        println!("{} {what}", if good { "ok  " } else { "FAIL" });
        ok &= good;
    };

    // 1. Upload a BGRA pattern, copy it texture → texture (one region at an
    //    offset, as tiles do), read both back.
    let texel = |x: u32, y: u32| [(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8, 255];
    let pattern: Vec<u8> = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).flat_map(move |x| texel(x, y)))
        .collect();
    let upload = device.create_buffer(&BufferDesc {
        size: pattern.len() as u64,
        stride: 0,
        format: None,
        location: MemoryLocation::Upload,
        label: "upload".into(),
    })?;
    upload.write_with(|mapped| mapped.copy_from_slice(&pattern))?;

    let texture = |label: &str, format: Format, width: u32, height: u32| TextureDesc {
        width,
        height,
        format,
        slices: 1,
        force_array_view: false,
        usage: TextureUsage::default(),
        label: label.into(),
    };
    let src = device.create_texture(&texture("src", Format::B8G8R8A8Unorm, WIDTH, HEIGHT))?;
    let dst = device.create_texture(&texture("dst", Format::B8G8R8A8Unorm, WIDTH, HEIGHT))?;
    let readback = device.create_buffer(&BufferDesc {
        size: pattern.len() as u64,
        stride: 0,
        format: None,
        location: MemoryLocation::Readback,
        label: "readback".into(),
    })?;

    let mut rec = device.recorder()?;
    rec.copy_buffer_to_texture(&upload, &src, &[]);
    rec.clear(&dst, [0.0, 0.0, 0.0, 0.0]);
    // The bottom-right quarter of src lands at (0, 0) of dst.
    rec.copy_texture(
        &src,
        &dst,
        &[CopyRegion {
            src_offset: (WIDTH / 2) as u64,
            src_y: HEIGHT / 2,
            width: WIDTH / 2,
            height: HEIGHT / 2,
            ..Default::default()
        }],
    );
    rec.copy_texture_to_buffer(&dst, &readback);
    println!("recorded {} steps", rec.len());
    device
        .submit(rec, &SubmitOptions::default())?
        .wait(5_000_000_000)?;

    check("src downloads as uploaded", src.download()? == pattern);
    let mut copied = vec![0u8; pattern.len()];
    readback.read(0, &mut copied)?;
    let quarter_ok = (0..HEIGHT).all(|y| {
        (0..WIDTH).all(|x| {
            let at = ((y * WIDTH + x) * 4) as usize;
            let want = if x < WIDTH / 2 && y < HEIGHT / 2 {
                texel(x + WIDTH / 2, y + HEIGHT / 2)
            } else {
                [0; 4]
            };
            copied[at..at + 4] == want
        })
    });
    check("region copy and clear", quarter_ok);

    // 2. A compute dispatch with a timestamp: clear.spv fills an rgba16f
    //    texture through a constant buffer.
    let out = device.create_texture(&texture("clear out", Format::R16G16B16A16Float, WIDTH, HEIGHT))?;
    let color = [1.0f32, 0.5, 0.25, 1.0];
    let mut constants = Vec::with_capacity(32);
    for c in color {
        constants.extend_from_slice(&c.to_le_bytes());
    }
    constants.extend_from_slice(&WIDTH.to_le_bytes());
    constants.extend_from_slice(&HEIGHT.to_le_bytes());
    constants.resize(32, 0);
    let cbuffer = device.create_buffer(&BufferDesc {
        size: constants.len() as u64,
        stride: 0,
        format: None,
        location: MemoryLocation::Upload,
        label: "clear constants".into(),
    })?;
    cbuffer.write(0, &constants)?;
    let pipeline = device.create_compute(&ComputeDesc {
        spirv: CLEAR_SPV,
        entry: "main",
        cbv: vec![Binding::Buffer(cbuffer)],
        uav: vec![Binding::Texture(out.clone())],
        label: "clear",
        ..Default::default()
    })?;
    let [gx, gy, _] = pipeline.workgroup_size();
    check(
        "workgroup size from SPIR-V is 16x16x1",
        pipeline.workgroup_size() == [16, 16, 1],
    );
    let mut rec = device.recorder()?;
    rec.dispatch(&Dispatch {
        pipeline: &pipeline,
        groups: [WIDTH.div_ceil(gx), HEIGHT.div_ceil(gy), 1],
        push: &[],
        timestamp: Some("clear"),
    });
    let times = device
        .submit(
            rec,
            &SubmitOptions {
                timestamps: true,
                ..Default::default()
            },
        )?
        .wait(5_000_000_000)?;
    match &times {
        Some(t) if !t.dispatches.is_empty() => {
            for (name, ms) in &t.dispatches {
                println!("     {name}: {ms:.4} ms");
            }
            println!("     total: {:.4} ms", t.total_ms);
        }
        _ => println!("     (no timestamps on this device)"),
    }
    // Half floats: 1.0 = 0x3c00, 0.5 = 0x3800, 0.25 = 0x3400.
    let half: Vec<u16> = out
        .download()?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    check(
        "dispatch wrote the clear color",
        half.as_chunks::<4>()
            .0
            .iter()
            .all(|t| *t == [0x3c00, 0x3800, 0x3400, 0x3c00]),
    );

    // 3. External memory and semaphores (what lsfg needs), if supported.
    if device.info().supports_external_fd {
        let shared = device.create_external_texture(&texture("shared", Format::R8G8B8A8Unorm, 64, 64))?;
        let semaphore = device.create_external_semaphore()?;
        let memory_fd = shared.export_fd();
        let semaphore_fd = semaphore.export_fd();
        check(
            "external memory and semaphore export fds",
            memory_fd.is_ok() && semaphore_fd.is_ok(),
        );
        // Signal it from a submission (as the pipeline does for lsfg's input).
        let mut rec = device.recorder()?;
        rec.clear(shared.texture(), [1.0, 0.0, 0.0, 1.0]);
        device
            .submit(
                rec,
                &SubmitOptions {
                    signal: &[&semaphore],
                    ..Default::default()
                },
            )?
            .wait(5_000_000_000)?;
        // Wait on it so it ends unsignalled.
        let rec = device.recorder()?;
        device
            .submit(
                rec,
                &SubmitOptions {
                    wait: &[&semaphore],
                    ..Default::default()
                },
            )?
            .wait(5_000_000_000)?;
        check(
            "shared texture cleared",
            shared
                .texture()
                .download()?
                .as_chunks::<4>()
                .0
                .iter()
                .all(|t| *t == [255, 0, 0, 255]),
        );
    } else {
        println!("skip external memory (not supported)");
    }

    device.wait_idle()?;
    let messages = device.debug_messages();
    for m in &messages {
        println!("validation: {m}");
    }
    check("no validation messages", messages.is_empty());
    Ok(ok)
}
