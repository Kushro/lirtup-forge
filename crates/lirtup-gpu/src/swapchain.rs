//! Presentation to an XCB window (`vk_swapchain.cpp`).

use std::cell::Cell;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::{ExternalSemaphore, Format, Result, Texture};

/// Vulkan present mode (mirrors `lirtup_config::PresentMode`; this crate does
/// not depend on the config).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PresentMode {
    /// FIFO (vsync).
    #[default]
    Fifo,
    /// Mailbox.
    Mailbox,
    /// Immediate.
    Immediate,
}

/// An XCB window to present to: the overlay window of `lirtup-x11`.
#[derive(Debug, Clone, Copy)]
pub struct XcbTarget {
    connection: NonNull<c_void>,
    window: u32,
}

// SAFETY: the pointer is only passed to vkCreateXcbSurfaceKHR, which may be
// called from any thread; `XcbTarget::new` makes the caller keep the
// connection alive.
unsafe impl Send for XcbTarget {}

impl XcbTarget {
    /// Wrap an `xcb_connection_t*` and a window id.
    ///
    /// # Safety
    ///
    /// `connection` must be a live XCB connection that outlives every
    /// swapchain created from this target.
    pub unsafe fn new(connection: NonNull<c_void>, window: u32) -> XcbTarget {
        XcbTarget { connection, window }
    }

    /// The connection pointer.
    pub fn connection(&self) -> NonNull<c_void> {
        self.connection
    }

    /// The window id.
    pub fn window(&self) -> u32 {
        self.window
    }
}

/// How to create a [`Swapchain`].
#[derive(Debug, Clone)]
pub struct SwapchainDesc {
    /// Window.
    pub target: XcbTarget,
    /// Preferred format (falls back to what the surface offers).
    pub format: Format,
    /// Images (2 or 3).
    pub buffers: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Present mode (falls back to FIFO).
    pub present_mode: PresentMode,
}

/// How to present one texture.
#[derive(Default)]
pub struct PresentOptions<'a> {
    /// Top-left position of the texture inside the swapchain image.
    pub offset: (i32, i32),
    /// Color of what the texture does not cover (letterbox), RGBA.
    pub clear: Option<[f32; 4]>,
    /// Wait on this semaphore before copying (a generated lsfg frame).
    pub wait: Option<&'a ExternalSemaphore>,
    /// Block until the present's fence signals (1.x `wait_for_fence`).
    pub wait_fence: bool,
}

/// Result of a present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresentStatus {
    /// Presented.
    Ok,
    /// Presented, but the swapchain should be recreated soon.
    Suboptimal,
    /// Not presented; recreate ([`Swapchain::recreate`]).
    OutOfDate,
}

/// A swapchain. `Send`, not `Sync`.
pub struct Swapchain {
    _private: (),
    _not_sync: PhantomData<Cell<()>>,
}

impl Swapchain {
    /// Current width.
    pub fn width(&self) -> u32 {
        todo!("front G")
    }

    /// Current height.
    pub fn height(&self) -> u32 {
        todo!("front G")
    }

    /// The format actually chosen.
    pub fn format(&self) -> Format {
        todo!("front G")
    }

    /// The present mode actually chosen.
    pub fn present_mode(&self) -> PresentMode {
        todo!("front G")
    }

    /// Acquire, copy `texture` in (with `options`), present. Queue locked for
    /// the submit and the present.
    pub fn present(&mut self, texture: &Texture, options: &PresentOptions<'_>) -> Result<PresentStatus> {
        todo!("front G")
    }

    /// Recreate at a new size (after `OutOfDate`, or a resize debounced by
    /// `swapchain_debounce`).
    pub fn recreate(&mut self, width: u32, height: u32) -> Result<()> {
        todo!("front G")
    }

    /// Whether the last present asked for a recreation.
    pub fn needs_recreation(&self) -> bool {
        todo!("front G")
    }
}
