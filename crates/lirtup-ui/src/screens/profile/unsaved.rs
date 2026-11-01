//! «Cambios sin guardar» (front C): Guardar y cambiar / Descartar y cambiar
//! / Cancelar; the answer goes back as `Action::Unsaved`.

use dioxus::prelude::*;

use crate::app::{use_dispatch, use_store};
use crate::components::{Button, ButtonVariant, Dialog};
use crate::i18n::{arg, tr, tr_args};
use crate::route::{ProfileRef, Route};
use crate::state::{Action, UnsavedChoice};

/// The dialog (`Dialog::Unsaved`).
#[component]
pub fn UnsavedDialog(to: Route) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let open = use_signal(|| true);
    // `to` is where the person goes; the text names the page being left.
    let _ = to;
    let name = {
        let s = store.read();
        match &s.route {
            Route::Profile {
                id: ProfileRef::Named(name),
                ..
            } => name.clone(),
            Route::Modes { id: Some(id) } => s
                .modes
                .store
                .get(id)
                .map_or_else(|| id.clone(), |m| m.name.clone()),
            _ => tr("nav-default-profile"),
        }
    };
    let body = tr_args("unsaved-body", &[("name", arg(name))]);
    let answer = move |choice: UnsavedChoice| move |_| dispatch(Action::Unsaved(choice));
    rsx! {
        Dialog {
            open,
            icon: "edit_note",
            icon_color: "tertiary",
            title: tr("unsaved-title"),
            on_close: move |_| dispatch(Action::Unsaved(UnsavedChoice::Cancel)),
            actions: rsx! {
                Button { label: tr("unsaved-cancel"), variant: ButtonVariant::Text, onclick: answer(UnsavedChoice::Cancel) }
                Button { label: tr("unsaved-discard"), variant: ButtonVariant::Outlined, onclick: answer(UnsavedChoice::DiscardAndGo) }
                Button { label: tr("unsaved-save"), icon: "save", onclick: answer(UnsavedChoice::SaveAndGo) }
            },
            p { class: "t-bm", "{body}" }
        }
    }
}
