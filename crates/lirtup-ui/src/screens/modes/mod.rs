//! Modos de escalado (front D): the mode list with total cost and use, and
//! the editor (model, double upscale, sampling, engine, the effect chain
//! with drag and drop, MagpieFX parameters, the shared mode banner, final
//! touches). Also the effect picker and deleting a mode.

use dioxus::prelude::*;

use crate::route::ModeId;

/// The list (`id: None`) or one mode's editor (`Route::Modes`).
#[component]
pub fn ModesPage(id: Option<ModeId>) -> Element {
    todo!()
}

/// «Agregar efecto» (`Dialog::EffectPicker`): the catalog of `ListEffects`,
/// with «Sin probar» on the unverified ones.
#[component]
pub fn EffectPickerDialog(mode: ModeId) -> Element {
    todo!()
}

/// «Eliminar modo» confirmation (`Dialog::DeleteMode`).
#[component]
pub fn DeleteModeDialog(id: ModeId) -> Element {
    todo!()
}
