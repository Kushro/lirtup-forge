//! Window capture: XShm + XDamage + tile hashes (`capture/lib/*.c`).

use crate::Result;

/// How to capture.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureOptions {
    /// Window to capture.
    pub window: u32,
    /// Pixels cropped from each side.
    pub crop: lirtup_config::Crop,
    /// Tile size for the damage hashes (`tile_size`, the 1.x
    /// `CAPTURE_TILE_SIZE`).
    pub tile_size: u32,
    /// Use XDamage; off = every grab is a full frame.
    pub damage_tracking: bool,
}

/// A damaged rectangle of the cropped frame, with the xxh64 of its pixels
/// (unchanged hashes let the pipeline skip tiles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct DamageRect {
    /// Left, in frame pixels.
    pub x: i32,
    /// Top, in frame pixels.
    pub y: i32,
    /// Width.
    pub width: i32,
    /// Height.
    pub height: i32,
    /// xxh64 of the rectangle's pixels.
    pub hash: u64,
}

/// What one grab produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Frame {
    /// Cropped width.
    pub width: u32,
    /// Cropped height.
    pub height: u32,
    /// Bytes per row in the output buffer.
    pub stride: u32,
    /// Anything changed since the last grab.
    pub dirty: bool,
    /// The whole frame was copied (first grab, damage overflow, tracking
    /// off): ignore the rectangles.
    pub full: bool,
    /// Grab number, from 1.
    pub sequence: u64,
}

/// Capture of one window. `Send`; owns its own connection and shared
/// memory segment (`memfd` + `shm::attach_fd`).
pub struct Capture {
    _private: (),
}

impl Capture {
    /// Start capturing. Fails if the cropped size is empty.
    pub fn new(options: &CaptureOptions) -> Result<Capture> {
        todo!("front X")
    }

    /// Cropped size `(width, height)`.
    pub fn size(&self) -> (u32, u32) {
        todo!("front X")
    }

    /// Bytes `grab` writes: `stride * height`.
    pub fn buffer_len(&self) -> usize {
        todo!("front X")
    }

    /// Copy the window into `out` (at least [`Capture::buffer_len`] bytes,
    /// [`crate::PIXEL_FORMAT`]) and fill `damage` (cleared first) with what
    /// changed. Returns [`crate::X11Error::Resized`] when the window size
    /// changed, [`crate::X11Error::NoWindow`] when it is gone.
    pub fn grab(&mut self, out: &mut [u8], damage: &mut Vec<DamageRect>) -> Result<Frame> {
        todo!("front X")
    }
}
