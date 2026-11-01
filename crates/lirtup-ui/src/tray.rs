//! The tray (round 4, front Y; «Bandeja» of the design and 6.1).
//!
//! Menu: «Escalando: …» with fps and mode, Detener, Reiniciar, Perfil ▸,
//! Generación de cuadros ▸ (Apagada, ×2, ×3, ×4, Auto; with the reason when
//! the engine is missing), Modo daemon, Mostrar, Salir. The icon has four
//! states ([`TrayStatus`]). It lives in the GUI process (`tray-icon` and
//! `muda` through `dioxus::desktop::trayicon`, on the same event loop) and
//! acts only through actions, like any screen.
//!
//! Closing the window hides it to the tray when `gui.close_to_tray` and
//! `gui.show_tray_icon`; minimizing does when `gui.minimize_to_tray`;
//! `gui.start_hidden` starts in the tray. Front Y changes the title bar of
//! `app.rs` for that (only its close and minimize handlers).

use dioxus::prelude::*;

use crate::state::{Action, Store};

/// The tray icon's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayStatus {
    /// Nothing running.
    #[default]
    Idle,
    /// The daemon is watching.
    Watching,
    /// A session is scaling.
    Scaling,
    /// The last session failed, or the scaler is down.
    Error,
}

/// The state of the icon for a store (pure, for the tests).
pub fn status(store: &Store) -> TrayStatus {
    let _ = store;
    todo!()
}

/// Create the tray and keep its menu in step with the store (called once by
/// `app.rs`). Until front Y writes it, there is no tray.
pub fn use_tray(store: Signal<Store>, dispatch: Callback<Action>) {
    let _ = (store, dispatch);
}
