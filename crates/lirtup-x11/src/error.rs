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

impl From<x11rb::errors::ConnectError> for X11Error {
    fn from(e: x11rb::errors::ConnectError) -> Self {
        X11Error::Connect(e.to_string())
    }
}

impl From<x11rb::errors::ConnectionError> for X11Error {
    fn from(e: x11rb::errors::ConnectionError) -> Self {
        X11Error::Connection(e.to_string())
    }
}

impl From<x11rb::x11_utils::X11Error> for X11Error {
    fn from(e: x11rb::x11_utils::X11Error) -> Self {
        X11Error::Request(format!(
            "{:?} (opcode {}.{})",
            e.error_kind, e.major_opcode, e.minor_opcode
        ))
    }
}

impl From<x11rb::errors::ReplyError> for X11Error {
    fn from(e: x11rb::errors::ReplyError) -> Self {
        match e {
            x11rb::errors::ReplyError::ConnectionError(e) => e.into(),
            x11rb::errors::ReplyError::X11Error(e) => e.into(),
        }
    }
}

impl From<x11rb::errors::ReplyOrIdError> for X11Error {
    fn from(e: x11rb::errors::ReplyOrIdError) -> Self {
        match e {
            x11rb::errors::ReplyOrIdError::IdsExhausted => X11Error::Request("X11 ids exhausted".into()),
            x11rb::errors::ReplyOrIdError::ConnectionError(e) => e.into(),
            x11rb::errors::ReplyOrIdError::X11Error(e) => e.into(),
        }
    }
}

/// Whether an X error means "that window (or drawable) is gone".
pub(crate) fn is_gone(e: &x11rb::errors::ReplyError) -> bool {
    use x11rb::protocol::ErrorKind;
    matches!(
        e,
        x11rb::errors::ReplyError::X11Error(x)
            if matches!(x.error_kind, ErrorKind::Window | ErrorKind::Drawable)
    )
}

/// Turn "window gone" into `Ok(None)`, any other error into `Err`.
pub(crate) fn gone_to_none<T>(r: std::result::Result<T, x11rb::errors::ReplyError>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if is_gone(&e) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
