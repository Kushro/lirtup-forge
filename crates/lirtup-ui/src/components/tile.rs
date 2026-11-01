//! The window tile of Inicio's grid.
// Front A: remove when the bodies are written.
#![allow(unused_variables)]

use dioxus::prelude::*;
use lirtup_x11::WindowInfo;

/// How a [`WindowTile`] looks. Hover and keyboard focus are also CSS states;
/// the prop lets the grid (which owns the arrow-key focus) and the gallery
/// force them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TileState {
    /// At rest.
    #[default]
    Idle,
    /// Pointer over it: the «Escalar» button over the preview.
    Hover,
    /// Keyboard focus of the grid.
    Focus,
    /// Its window is scaling: amber badge «Escalando · N fps».
    Running,
}

/// A window: live preview (`/thumb/{xid}?v={n}`), icon, title and its
/// profile's badge («sin perfil» when none).
#[component]
pub fn WindowTile(
    window: WindowInfo,
    /// The matching profile's name.
    profile: Option<String>,
    #[props(default)] state: TileState,
    /// The preview's URL (front B's `thumbs.rs` builds it).
    thumb: Option<String>,
    /// The window icon's URL (`/icon/{xid}`).
    icon: Option<String>,
    /// Shown fps while running.
    fps: Option<f32>,
    /// Aspect of the preview (`aspect-4/3`, `aspect-video`; Ajustes ▸ Estilo).
    #[props(default = "4/3".to_string())]
    aspect: String,
    /// Click, Enter: scale it with its profile.
    on_activate: EventHandler<()>,
    /// Right click, ⋮, the Menu key: open the tile menu at the pointer.
    on_menu: EventHandler<(f64, f64)>,
) -> Element {
    todo!()
}
