//! Window capture: XShm + XDamage + tile hashes (`capture/lib/*.c`).
//!
//! One grab:
//!
//! 1. `GetGeometry` of the window: gone → [`X11Error::NoWindow`], other
//!    size → [`X11Error::Resized`].
//! 2. With damage tracking, `DamageSubtract` into a region and fetch its
//!    bounds; nothing damaged → a clean frame, `out` untouched.
//! 3. The bounds are grown to the tile grid (so every touched tile is read
//!    whole) and read with `ShmGetImage` into a `memfd` segment (plain
//!    `GetImage` when MIT-SHM is missing).
//! 4. The read region is copied into `out`, the xxh64 of each tile in it is
//!    compared with the [`TileCache`], and the tiles whose hash changed are
//!    the damage. Over [`FULL_FRAME_TILE_FRACTION`] of the tiles, the frame is
//!    reported as full (the 1.x `CAPTURE_TILE_THRESHOLD`, 30 %).

use std::fs::File;
use std::os::fd::{FromRawFd, OwnedFd};

use memmap2::MmapMut;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::ErrorKind;
use x11rb::protocol::damage::{self, ConnectionExt as _};
use x11rb::protocol::shm::{self, ConnectionExt as _};
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat};
use x11rb::rust_connection::RustConnection;

use crate::tiles::{Tile, TileCache};
use crate::{Result, X11Error};

/// Smallest tile edge (`MIN_TILE_SIZE` of 1.x).
pub const MIN_TILE_SIZE: u32 = 16;

/// Share of changed tiles above which a grab reports a full frame
/// (`DEFAULT_THRESHOLD` of `capture.h`).
pub const FULL_FRAME_TILE_FRACTION: f32 = 0.30;

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
///
/// With damage tracking, a grab writes only the part of `out` that may have
/// changed: pass the same buffer every time (it must still hold the previous
/// frame). Without it, every grab writes the whole frame.
pub struct Capture {
    conn: RustConnection,
    window: u32,
    crop: lirtup_config::Crop,
    /// Window size when the capture started.
    window_size: (u32, u32),
    /// Cropped size.
    width: u32,
    height: u32,
    tile_size: u32,
    damage: Option<Damage>,
    shm: Option<ShmSegment>,
    cache: TileCache,
    /// Next grab reads and reports the whole frame.
    need_full: bool,
    sequence: u64,
    candidates: Vec<DamageRect>,
    tiles: Vec<Tile>,
}

/// The XDamage object and the region its damage is moved into.
struct Damage {
    damage: damage::Damage,
    region: xfixes::Region,
}

/// A `memfd` attached to the server with `ShmAttachFd`.
struct ShmSegment {
    seg: shm::Seg,
    map: MmapMut,
}

impl ShmSegment {
    fn new(conn: &RustConnection, len: usize) -> Result<ShmSegment> {
        // SAFETY: `memfd_create` takes a NUL-terminated name and flags; it
        // returns a new fd or -1.
        let fd = unsafe { libc::memfd_create(c"lirtup-capture".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(X11Error::Shm(format!(
                "memfd_create: {}",
                std::io::Error::last_os_error()
            )));
        }
        // SAFETY: `fd` is a fresh descriptor nobody else owns.
        let file = unsafe { File::from_raw_fd(fd) };
        file.set_len(len as u64)
            .map_err(|e| X11Error::Shm(format!("ftruncate: {e}")))?;
        // SAFETY: the memfd is private to this process and the X server; the
        // server only writes into it during `ShmGetImage`, whose reply we wait
        // for before reading the map.
        let map = unsafe { MmapMut::map_mut(&file) }.map_err(|e| X11Error::Shm(format!("mmap: {e}")))?;
        let dup = file.try_clone().map_err(|e| X11Error::Shm(format!("dup: {e}")))?;
        let seg = conn.generate_id()?;
        conn.shm_attach_fd(seg, OwnedFd::from(dup), false)?.check()?;
        Ok(ShmSegment { seg, map })
    }
}

impl Capture {
    /// Start capturing. Fails if the cropped size is empty.
    pub fn new(options: &CaptureOptions) -> Result<Capture> {
        let (conn, _) = RustConnection::connect(None)?;
        let window = options.window;
        let geometry = conn.get_geometry(window)?.reply().map_err(|e| match &e {
            e if crate::error::is_gone(e) => X11Error::NoWindow(window),
            _ => e.into(),
        })?;
        let window_size = (u32::from(geometry.width), u32::from(geometry.height));
        let (width, height) = cropped_size(window_size, &options.crop);
        if width == 0 || height == 0 {
            return Err(X11Error::Request(format!(
                "window 0x{window:x} is {}x{}: nothing left after cropping",
                window_size.0, window_size.1
            )));
        }

        let shm = match shm_available(&conn) {
            true => match ShmSegment::new(&conn, width as usize * height as usize * 4) {
                Ok(seg) => Some(seg),
                Err(e) => {
                    tracing::warn!("MIT-SHM setup failed, using plain GetImage: {e}");
                    None
                }
            },
            false => {
                tracing::warn!("MIT-SHM (1.2) not available, using plain GetImage");
                None
            }
        };

        let damage = if options.damage_tracking {
            match create_damage(&conn, window) {
                Ok(d) => Some(d),
                Err(e) => {
                    tracing::warn!("damage tracking off: {e}");
                    None
                }
            }
        } else {
            None
        };

        let tile_size = options.tile_size.max(MIN_TILE_SIZE);
        tracing::debug!(
            "capture 0x{window:x}: {width}x{height}, tile {tile_size}, shm {}, damage {}",
            shm.is_some(),
            damage.is_some()
        );
        Ok(Capture {
            conn,
            window,
            crop: options.crop,
            window_size,
            width,
            height,
            tile_size,
            damage,
            shm,
            cache: TileCache::new(width, height, tile_size),
            need_full: true,
            sequence: 0,
            candidates: Vec::new(),
            tiles: Vec::new(),
        })
    }

    /// Cropped size `(width, height)`.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Bytes `grab` writes: `stride * height`.
    pub fn buffer_len(&self) -> usize {
        self.stride() as usize * self.height as usize
    }

    fn stride(&self) -> u32 {
        self.width * 4
    }

    /// Copy the window into `out` (at least [`Capture::buffer_len`] bytes,
    /// [`crate::PIXEL_FORMAT`]) and fill `damage` (cleared first) with what
    /// changed. Returns [`crate::X11Error::Resized`] when the window size
    /// changed, [`crate::X11Error::NoWindow`] when it is gone.
    pub fn grab(&mut self, out: &mut [u8], damage: &mut Vec<DamageRect>) -> Result<Frame> {
        damage.clear();
        if out.len() < self.buffer_len() {
            return Err(X11Error::Request(format!(
                "capture buffer has {} bytes, needs {}",
                out.len(),
                self.buffer_len()
            )));
        }
        self.sequence += 1;
        let mut frame = Frame {
            width: self.width,
            height: self.height,
            stride: self.stride(),
            dirty: false,
            full: false,
            sequence: self.sequence,
        };

        let geometry = self
            .conn
            .get_geometry(self.window)?
            .reply()
            .map_err(|e| match &e {
                e if crate::error::is_gone(e) => X11Error::NoWindow(self.window),
                _ => e.into(),
            })?;
        let size = (u32::from(geometry.width), u32::from(geometry.height));
        if size != self.window_size {
            return Err(X11Error::Resized {
                width: size.0,
                height: size.1,
            });
        }

        // Damage notifications only wake nobody; drop them.
        while self.conn.poll_for_event()?.is_some() {}

        // Region to read, in cropped coordinates: (x, y, w, h).
        let tracking = self.damage.is_some();
        let region = match &self.damage {
            Some(d) => {
                self.conn.damage_subtract(d.damage, x11rb::NONE, d.region)?;
                let fetched = self.conn.xfixes_fetch_region(d.region)?.reply()?;
                if self.need_full {
                    Some((0, 0, self.width, self.height))
                } else {
                    let e = fetched.extents;
                    let rect = DamageRect {
                        x: i32::from(e.x) - self.crop.left as i32,
                        y: i32::from(e.y) - self.crop.top as i32,
                        width: i32::from(e.width),
                        height: i32::from(e.height),
                        hash: 0,
                    };
                    align_to_tiles(&rect, self.tile_size, self.width, self.height)
                }
            }
            None => Some((0, 0, self.width, self.height)),
        };
        let Some((rx, ry, rw, rh)) = region else {
            return Ok(frame);
        };

        // Read the region.
        let src_stride = rw as usize * 4;
        let dst_stride = self.stride() as usize;
        let read = read_region(
            &self.conn,
            self.window,
            &self.crop,
            self.shm.as_ref(),
            (rx, ry, rw, rh),
        );
        let pixels: std::borrow::Cow<'_, [u8]> = match read {
            Ok(p) => p,
            Err(ReadError::NotViewable) => {
                // Unmapped, minimized or off screen without a compositor:
                // keep the last frame and read everything once it is back.
                self.need_full = true;
                return Ok(frame);
            }
            Err(ReadError::Gone) => return Err(X11Error::NoWindow(self.window)),
            Err(ReadError::Other(e)) => return Err(e),
        };

        // Copy into `out`.
        for row in 0..rh as usize {
            let src = &pixels[row * src_stride..row * src_stride + src_stride];
            let dst_off = (ry as usize + row) * dst_stride + rx as usize * 4;
            out[dst_off..dst_off + src_stride].copy_from_slice(src);
        }

        let whole = DamageRect {
            x: 0,
            y: 0,
            width: self.width as i32,
            height: self.height as i32,
            hash: 0,
        };
        if !tracking {
            frame.dirty = true;
            frame.full = true;
            damage.push(whole);
            return Ok(frame);
        }

        // Hash the tiles of the region (all whole, thanks to the alignment).
        let ts = self.tile_size;
        self.candidates.clear();
        for row in ry / ts..(ry + rh).div_ceil(ts) {
            for col in rx / ts..(rx + rw).div_ceil(ts) {
                let mut t = self.cache.tile_rect(Tile { col, row });
                t.hash = tile_hash(
                    &pixels,
                    src_stride,
                    t.x as u32 - rx,
                    t.y as u32 - ry,
                    t.width as u32,
                    t.height as u32,
                );
                self.candidates.push(t);
            }
        }
        if self.need_full {
            self.cache.reset();
        }
        self.cache.changed_tiles(&self.candidates, &mut self.tiles);

        let (cols, rows) = self.cache.grid();
        let total = (cols * rows).max(1) as f32;
        if self.need_full || self.tiles.len() as f32 > total * FULL_FRAME_TILE_FRACTION {
            frame.dirty = true;
            frame.full = true;
            damage.push(whole);
        } else if !self.tiles.is_empty() {
            frame.dirty = true;
            // Candidates are row-major over the region's tiles.
            let region_cols = (rx + rw).div_ceil(ts) - rx / ts;
            for t in &self.tiles {
                let idx = (t.row - ry / ts) * region_cols + (t.col - rx / ts);
                damage.push(self.candidates[idx as usize]);
            }
        }
        self.need_full = false;
        Ok(frame)
    }
}

/// Read `w × h` at `(x, y)` of the cropped frame, tightly packed BGRA8.
fn read_region<'a>(
    conn: &RustConnection,
    window: u32,
    crop: &lirtup_config::Crop,
    shm: Option<&'a ShmSegment>,
    (x, y, w, h): (u32, u32, u32, u32),
) -> std::result::Result<std::borrow::Cow<'a, [u8]>, ReadError> {
    let len = w as usize * h as usize * 4;
    let sx = (crop.left + x) as i16;
    let sy = (crop.top + y) as i16;
    if let Some(seg) = shm {
        let reply = conn
            .shm_get_image(
                window,
                sx,
                sy,
                w as u16,
                h as u16,
                !0,
                ImageFormat::Z_PIXMAP.into(),
                seg.seg,
                0,
            )
            .map_err(|e| ReadError::Other(e.into()))?
            .reply()
            .map_err(ReadError::from)?;
        if reply.size as usize != len {
            return Err(ReadError::Other(unsupported_depth(reply.depth)));
        }
        Ok(std::borrow::Cow::Borrowed(&seg.map[..len]))
    } else {
        let reply = conn
            .get_image(ImageFormat::Z_PIXMAP, window, sx, sy, w as u16, h as u16, !0)
            .map_err(|e| ReadError::Other(e.into()))?
            .reply()
            .map_err(ReadError::from)?;
        if reply.data.len() != len {
            return Err(ReadError::Other(unsupported_depth(reply.depth)));
        }
        Ok(std::borrow::Cow::Owned(reply.data))
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(d) = &self.damage {
            let _ = self.conn.damage_destroy(d.damage);
            let _ = self.conn.xfixes_destroy_region(d.region);
        }
        if let Some(seg) = &self.shm {
            let _ = self.conn.shm_detach(seg.seg);
        }
        let _ = self.conn.flush();
    }
}

enum ReadError {
    NotViewable,
    Gone,
    Other(X11Error),
}

impl From<x11rb::errors::ReplyError> for ReadError {
    fn from(e: x11rb::errors::ReplyError) -> Self {
        match &e {
            x11rb::errors::ReplyError::X11Error(x) if x.error_kind == ErrorKind::Match => {
                ReadError::NotViewable
            }
            e if crate::error::is_gone(e) => ReadError::Gone,
            _ => ReadError::Other(e.into()),
        }
    }
}

fn unsupported_depth(depth: u8) -> X11Error {
    X11Error::Request(format!(
        "window depth {depth} is not 32 bits per pixel; only 24/32-bit windows are supported"
    ))
}

fn shm_available(conn: &RustConnection) -> bool {
    if !matches!(conn.extension_information(shm::X11_EXTENSION_NAME), Ok(Some(_))) {
        return false;
    }
    match conn.shm_query_version().ok().and_then(|c| c.reply().ok()) {
        Some(v) => (v.major_version, v.minor_version) >= (1, 2),
        None => false,
    }
}

fn create_damage(conn: &RustConnection, window: u32) -> Result<Damage> {
    if conn.extension_information(damage::X11_EXTENSION_NAME)?.is_none() {
        return Err(X11Error::MissingExtension("DAMAGE"));
    }
    if conn.extension_information(xfixes::X11_EXTENSION_NAME)?.is_none() {
        return Err(X11Error::MissingExtension("XFIXES"));
    }
    // Both extensions refuse requests before the version handshake.
    conn.xfixes_query_version(5, 0)?.reply()?;
    conn.damage_query_version(1, 1)?.reply()?;
    let region = conn.generate_id()?;
    conn.xfixes_create_region(region, &[])?;
    let id = conn.generate_id()?;
    // NonEmpty: one event when damage appears; we poll, events are not needed.
    conn.damage_create(id, window, damage::ReportLevel::NON_EMPTY)?
        .check()?;
    Ok(Damage { damage: id, region })
}

/// Window size minus the crop (saturating).
pub(crate) fn cropped_size(window: (u32, u32), crop: &lirtup_config::Crop) -> (u32, u32) {
    (
        window.0.saturating_sub(crop.left).saturating_sub(crop.right),
        window.1.saturating_sub(crop.top).saturating_sub(crop.bottom),
    )
}

/// Grow a rectangle (cropped coordinates, may stick out) to whole tiles,
/// clamped to the frame. `None` when it misses the frame.
pub(crate) fn align_to_tiles(
    r: &DamageRect,
    tile: u32,
    width: u32,
    height: u32,
) -> Option<(u32, u32, u32, u32)> {
    let x0 = i64::from(r.x).clamp(0, i64::from(width)) as u32;
    let y0 = i64::from(r.y).clamp(0, i64::from(height)) as u32;
    let x1 = (i64::from(r.x) + i64::from(r.width)).clamp(0, i64::from(width)) as u32;
    let y1 = (i64::from(r.y) + i64::from(r.height)).clamp(0, i64::from(height)) as u32;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let ax0 = x0 / tile * tile;
    let ay0 = y0 / tile * tile;
    let ax1 = (x1.div_ceil(tile) * tile).min(width);
    let ay1 = (y1.div_ceil(tile) * tile).min(height);
    Some((ax0, ay0, ax1 - ax0, ay1 - ay0))
}

/// xxh64 (seed 0) of a `w × h` block at `(x, y)` of a BGRA8 buffer, row by
/// row (`tile_compute_hash`). Never 0: 0 means "no hash" in [`DamageRect`].
pub(crate) fn tile_hash(buf: &[u8], stride: usize, x: u32, y: u32, w: u32, h: u32) -> u64 {
    let mut hasher = xxhash_rust::xxh64::Xxh64::new(0);
    for row in y..y + h {
        let start = row as usize * stride + x as usize * 4;
        hasher.update(&buf[start..start + w as usize * 4]);
    }
    hasher.digest().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_saturates() {
        let crop = lirtup_config::Crop {
            top: 30,
            bottom: 10,
            left: 5,
            right: 5,
        };
        assert_eq!(cropped_size((800, 600), &crop), (790, 560));
        assert_eq!(cropped_size((8, 6), &crop), (0, 0));
    }

    #[test]
    fn align_grows_to_tile_grid() {
        let r = DamageRect {
            x: 70,
            y: 10,
            width: 5,
            height: 60,
            hash: 0,
        };
        assert_eq!(align_to_tiles(&r, 64, 100, 100), Some((64, 0, 36, 100)));
        let r = DamageRect {
            x: -20,
            y: -20,
            width: 10,
            height: 10,
            hash: 0,
        };
        assert_eq!(align_to_tiles(&r, 64, 100, 100), None);
        let r = DamageRect {
            x: -20,
            y: 90,
            width: 30,
            height: 100,
            hash: 0,
        };
        assert_eq!(align_to_tiles(&r, 64, 100, 100), Some((0, 64, 64, 36)));
    }

    #[test]
    fn hash_depends_on_block_only() {
        // 4x2 frame, stride 16: the left 2x2 block is the same in both.
        let a: Vec<u8> = (0..32).collect();
        let mut b = a.clone();
        b[8..16].fill(0xFF);
        b[24..32].fill(0xFF);
        assert_eq!(tile_hash(&a, 16, 0, 0, 2, 2), tile_hash(&b, 16, 0, 0, 2, 2));
        assert_ne!(tile_hash(&a, 16, 2, 0, 2, 2), tile_hash(&b, 16, 2, 0, 2, 2));
        // Matches a one-shot xxh64 of the packed rows.
        let packed: Vec<u8> = [&a[0..8], &a[16..24]].concat();
        assert_eq!(
            tile_hash(&a, 16, 0, 0, 2, 2),
            xxhash_rust::xxh64::xxh64(&packed, 0).max(1)
        );
    }
}
