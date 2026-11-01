//! A profile page (front C): header with the rules as chips, the five tabs
//! (`tabs/` is C's to split as it likes), inheritance per row, Restablecer
//! and the save bar's content. Also the dialogs of profiles.

pub mod unsaved;

pub use unsaved::UnsavedDialog;

use dioxus::prelude::*;

use crate::route::{ProfileName, ProfileRef, ProfileTab};

/// The profile page (`Route::Profile`).
#[component]
pub fn ProfilePage(id: ProfileRef, tab: ProfileTab) -> Element {
    todo!()
}

/// «Eliminar perfil» confirmation (`Dialog::DeleteProfile`).
#[component]
pub fn DeleteProfileDialog(name: ProfileName) -> Element {
    todo!()
}

/// What the save bar shows for the current page: the edits count and the
/// scope Restablecer acts on. `app.rs` draws the bar (it floats over every
/// page) and asks this; `None` hides it.
pub fn save_bar_scope(store: &crate::state::Store) -> Option<(crate::state::Scope, usize)> {
    use crate::route::Route;
    use crate::state::Scope;
    let scope = match &store.route {
        Route::Profile { id, .. } => Scope::Profile(id.clone()),
        Route::Modes { id: Some(id) } => Scope::Mode(id.clone()),
        _ => return None,
    };
    let count = store.page_edits(&store.route);
    (count > 0).then_some((scope, count))
}
