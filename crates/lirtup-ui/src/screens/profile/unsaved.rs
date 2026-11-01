//! «Cambios sin guardar» (front C): Guardar y cambiar / Descartar y cambiar
//! / Cancelar; the answer goes back as `Action::Unsaved`.

use dioxus::prelude::*;

use crate::route::Route;

/// The dialog (`Dialog::Unsaved`).
#[component]
pub fn UnsavedDialog(to: Route) -> Element {
    todo!()
}
