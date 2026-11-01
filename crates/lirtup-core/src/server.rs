//! The Unix socket server of `lirtup serve` (6.5).

use std::path::Path;
use std::sync::Arc;

use crate::{Engine, Result};

/// Listen on `socket` (creating its folder with mode 0700 and removing a
/// stale socket), serve clients until `Quit` or a signal, then shut the
/// engine down. One task per client: read lines → [`Engine::handle`], and
/// forward [`Engine::subscribe`] to the client.
pub async fn serve(engine: Arc<Engine>, socket: &Path) -> Result<()> {
    todo!("front K")
}
