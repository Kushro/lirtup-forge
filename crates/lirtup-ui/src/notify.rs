//! Desktop notifications (round 4, front Y): the session started, failed or
//! was started by the daemon, with a «Detener» action (`notify-rust`, the
//! freedesktop portal of notifications). Only while the window is hidden or
//! unfocused; never one per `Stats`.

use dioxus::prelude::*;

use crate::state::{Action, Store};

/// Watch the store and notify (called once by `app.rs`). Until front Y
/// writes it, nothing is shown.
pub fn use_notifications(store: Signal<Store>, dispatch: Callback<Action>) {
    let _ = (store, dispatch);
}
