//! Line framing and the socket location.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One line on the wire: `{"v": 1, "type": ..., ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Envelope<T> {
    /// Protocol version of the sender.
    pub v: u32,
    /// The message (its `type` and fields are flattened next to `v`).
    #[serde(flatten)]
    pub msg: T,
}

/// A line could not be decoded.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    /// Not valid JSON for this message type.
    #[error("bad message: {0}")]
    Json(#[from] serde_json::Error),
    /// A different protocol version.
    #[error("protocol version {got}, expected {expected}")]
    Version {
        /// Version in the line.
        got: u32,
        /// [`crate::PROTOCOL_VERSION`].
        expected: u32,
    },
}

/// Encode a message as one line, with the trailing `\n`.
pub fn encode_line<T: Serialize>(msg: &T) -> String {
    let envelope = Envelope {
        v: crate::PROTOCOL_VERSION,
        msg,
    };
    // Commands and events only hold strings, numbers, bools and maps with
    // string keys, which always serialize.
    let mut line = serde_json::to_string(&envelope).expect("protocol messages always serialize");
    line.push('\n');
    line
}

/// Decode one line (with or without the `\n`). A different `v` is
/// [`CodecError::Version`]; the server still answers a `Hello` of another
/// version (front K: decode it as `serde_json::Value` first) so the GUI can
/// show the mismatch.
pub fn decode_line<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T, CodecError> {
    #[derive(Deserialize)]
    struct Version {
        v: u32,
    }
    let line = line.trim_end_matches(['\n', '\r']);
    let Version { v } = serde_json::from_str(line)?;
    if v != crate::PROTOCOL_VERSION {
        return Err(CodecError::Version {
            got: v,
            expected: crate::PROTOCOL_VERSION,
        });
    }
    let envelope: Envelope<T> = serde_json::from_str(line)?;
    Ok(envelope.msg)
}

/// `$XDG_RUNTIME_DIR/lirtup/lirtup.sock` (`/tmp/lirtup-$UID/lirtup.sock`
/// without `XDG_RUNTIME_DIR`).
pub fn socket_path() -> PathBuf {
    use std::os::unix::fs::MetadataExt;

    let base = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("lirtup"),
        _ => {
            let uid = std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0);
            PathBuf::from(format!("/tmp/lirtup-{uid}"))
        }
    };
    base.join("lirtup.sock")
}
