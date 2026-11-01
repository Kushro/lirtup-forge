//! X11 for LirtUp Forge, on `x11rb` (no Xlib).
//!
//! - [`X11`]: a connection for window queries: list, info, icon, focus,
//!   monitors, activation, picking ([`window`], [`monitor`], [`pick`],
//!   [`acquire`]). The GUI also uses it, without the GPU (6.4).
//! - [`Capture`]: XShm + XDamage capture of one window into a caller buffer
//!   (the pipeline passes a mapped upload buffer of `lirtup-gpu`), with the
//!   xxh64 tile cache of [`tiles`].
//! - [`Hotkeys`]: global `GrabKey` hotkeys on their own thread.
//! - [`OverlayWindow`]: the ARGB, borderless overlay window and its events;
//!   [`InputForwarder`] replays them on the target window.
//! - [`thumbs`]: XComposite thumbnails for the GUI.
//!
//! Every type opens its own connection (one connection per thread, as 1.x).
//! The overlay and the swapchain need an `xcb_connection_t*`, so the overlay
//! uses `x11rb::xcb_ffi::XCBConnection`; the rest may use `RustConnection`.
//!
//! This crate does not depend on `lirtup-gpu`: pixels are plain bytes in
//! [`PIXEL_FORMAT`] (BGRA8, what `lirtup_gpu::Format::B8G8R8A8Unorm` is).
//!
//! `unsafe` is allowed in this crate; every block has a `// SAFETY:` comment.

#![allow(unsafe_code)]
// Round 0 stubs: the bodies are `todo!()`. Front X removes this.
#![allow(unused_variables)]

pub mod acquire;
pub mod capture;
pub mod conn;
mod cursor;
pub mod error;
pub mod hotkeys;
pub mod image;
pub mod input;
pub mod monitor;
pub mod overlay;
pub mod pick;
pub mod thumbs;
pub mod tiles;
pub mod window;

pub use capture::{Capture, CaptureOptions, DamageRect, Frame};
pub use conn::X11;
pub use error::{Result, X11Error};
pub use hotkeys::Hotkeys;
pub use image::RgbaImage;
pub use input::{CoordinateMapper, InputForwarder};
pub use monitor::MonitorInfo;
pub use overlay::{OverlayEvent, OverlayOptions, OverlayWindow};
pub use window::{Rect, WindowClass, WindowInfo};

/// Pixel layout of captured frames: 4 bytes per pixel, B, G, R, X (alpha
/// undefined), rows tightly packed unless [`Frame::stride`] says otherwise.
pub const PIXEL_FORMAT: &str = "B8G8R8A8_UNORM";
