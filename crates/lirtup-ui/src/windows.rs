//! The window list (front B, 6.4 of the analysis): a thread with its own
//! `lirtup-x11` connection lists the top-level windows every
//! `auto_refresh_ms` and hands them to the store (`Action::Windows`), only
//! when the list changed. The GUI's own window and the scaler's overlay
//! (`_LIRTUP_OVERLAY`) are left out.

use dioxus::prelude::*;

use crate::state::Action;

/// Start the list (called once by `app.rs`). Until front B writes it, it
/// does nothing and the list stays empty.
pub fn use_window_list(dispatch: Callback<Action>) {
    let _ = dispatch;
}
