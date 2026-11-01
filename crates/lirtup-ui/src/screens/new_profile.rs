//! «Nuevo perfil» (front C): icon picker (capture from a window, a file,
//! none), name with inline validation, mode and «Copiar ajustes de», the
//! rule editor, the match preview («Coincide con N ventanas», computed here
//! from the window list) and the side panel *Capturar ventana* with
//! «Señalar ventana» (`Command::PickWindow`).

use dioxus::prelude::*;
use lirtup_x11::WindowInfo;

/// The dialog (`Dialog::NewProfile`).
#[component]
pub fn NewProfileDialog(from: Option<WindowInfo>) -> Element {
    todo!()
}
