//! Capture one window to a PNG (the round 1 deliverable of front X).
//!
//! ```text
//! cargo run -p lirtup-x11 --example capture_png -- --list
//! cargo run -p lirtup-x11 --example capture_png -- [WINDOW] [-o FILE] [--frames N] [--pick]
//! ```
//!
//! `WINDOW` is a title substring or an XID (`0x3a00007`); without it, the
//! active window after 3 seconds. `--pick` asks for a click instead.
//! `--frames N` grabs N frames 100 ms apart (damage tracking on) and prints
//! what changed in each, then saves the last one.

use std::time::{Duration, Instant};

use lirtup_x11::{Capture, CaptureOptions, WindowInfo, X11};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut target: Option<String> = None;
    let mut out = String::from("capture.png");
    let mut frames = 1u32;
    let mut list = false;
    let mut pick = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--out" => out = args.next().ok_or("-o needs a file name")?,
            "--frames" => frames = args.next().ok_or("--frames needs a number")?.parse()?,
            "--list" => list = true,
            "--pick" => pick = true,
            "-h" | "--help" => {
                println!("capture_png [WINDOW] [-o FILE] [--frames N] [--pick] | --list");
                return Ok(());
            }
            _ => target = Some(arg),
        }
    }

    let x11 = X11::connect()?;
    println!("XWayland: {}", x11.is_xwayland());
    if list {
        for m in x11.monitors()? {
            println!(
                "monitor {} {:8} {}x{}+{}+{} {:?} Hz{}",
                m.index,
                m.name,
                m.rect.width,
                m.rect.height,
                m.rect.x,
                m.rect.y,
                m.refresh_hz,
                if m.primary { " (primary)" } else { "" }
            );
        }
        for w in x11.list_windows()? {
            print_window(&w);
        }
        return Ok(());
    }

    let window = if pick {
        println!("click a window (Escape or right click cancels)");
        x11.pick_window()?
    } else {
        match target.as_deref() {
            Some(t) if t.starts_with("0x") => x11.window_info(u32::from_str_radix(&t[2..], 16)?)?,
            Some(t) => x11.find_by_title(Some(t), None)?,
            None => {
                println!("capturing the active window in 3 s...");
                x11.active_after(Duration::from_secs(3))?
            }
        }
    };
    let window = window.ok_or("no such window")?;
    print_window(&window);

    let mut capture = Capture::new(&CaptureOptions {
        window: window.xid,
        crop: lirtup_config::Crop::default(),
        tile_size: 64,
        damage_tracking: true,
    })?;
    let (width, height) = capture.size();
    let mut buffer = vec![0u8; capture.buffer_len()];
    let mut damage = Vec::new();
    for i in 0..frames {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(100));
        }
        let start = Instant::now();
        let frame = capture.grab(&mut buffer, &mut damage)?;
        println!(
            "frame {}: {:.2} ms, dirty {}, full {}, {} rects, {:.1} % of the frame",
            frame.sequence,
            start.elapsed().as_secs_f64() * 1000.0,
            frame.dirty,
            frame.full,
            damage.len(),
            lirtup_x11::tiles::dirty_fraction(&damage, width, height) * 100.0
        );
    }

    // BGRX → RGBA.
    let stride = width as usize * 4;
    let mut rgba = Vec::with_capacity(stride * height as usize);
    for px in buffer[..stride * height as usize].as_chunks::<4>().0 {
        rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    image::save_buffer(&out, &rgba, width, height, image::ExtendedColorType::Rgba8)?;
    println!("saved {width}x{height} to {out}");

    if let Some(icon) = x11.window_icon(window.xid, 64)? {
        let icon_path = format!("{}.icon.png", out.trim_end_matches(".png"));
        image::save_buffer(
            &icon_path,
            &icon.data,
            icon.width,
            icon.height,
            image::ExtendedColorType::Rgba8,
        )?;
        println!("saved the {}x{} icon to {icon_path}", icon.width, icon.height);
    }
    Ok(())
}

fn print_window(w: &WindowInfo) {
    println!(
        "0x{:08x} {}x{}+{}+{} pid {:?} class {:?} {:?}",
        w.xid,
        w.rect.width,
        w.rect.height,
        w.rect.x,
        w.rect.y,
        w.pid,
        w.class.as_ref().map(|c| c.class.as_str()),
        w.title
    );
}
