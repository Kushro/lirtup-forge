//! Errors of the X11 layer.

/// Something failed talking to the X server.
#[derive(Debug, thiserror::Error)]
pub enum X11Error {
    /// No display (`$DISPLAY` unset, or the server refused).
    #[error("cannot connect to the X server: {0}")]
    Connect(String),
    /// The connection broke.
    #[error("X11 connection error: {0}")]
    Connection(String),
    /// A request returned an X error.
    #[error("X11 request failed: {0}")]
    Request(String),
    /// A required extension is missing (MIT-SHM, DAMAGE, XFIXES, Composite,
    /// RandR).
    #[error("X11 extension {0} is not available")]
    MissingExtension(&'static str),
    /// The window does not exist (any more).
    #[error("window 0x{0:x} does not exist")]
    NoWindow(u32),
    /// The window changed size; recreate the capture.
    #[error("window resized to {width}x{height}")]
    Resized {
        /// New width.
        width: u32,
        /// New height.
        height: u32,
    },
    /// Waiting for a window timed out.
    #[error("no window found within {0:.1} s")]
    Timeout(f64),
    /// A key name with no keysym, or a combination already grabbed.
    #[error("hotkey {0}: {1}")]
    Hotkey(String, String),
    /// Shared memory setup failed.
    #[error("shared memory: {0}")]
    Shm(String),
}

/// Result of this crate.
pub type Result<T, E = X11Error> = std::result::Result<T, E>;
