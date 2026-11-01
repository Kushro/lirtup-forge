//! Small pieces: badges, chips, icons and the loading indicator.
// Front A: remove when the bodies are written.
#![allow(unused_variables)]

use dioxus::prelude::*;

/// [`Badge`] colors (`.badge.*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeVariant {
    /// Surface container («Hereda», «Global», «sin perfil»).
    #[default]
    Neutral,
    /// Primary container («×2 Auto»).
    Primary,
    /// Tertiary container, live («Escalando»).
    Tertiary,
    /// Success container (a measured cost: «0,9 ms»).
    Success,
    /// Error container («Necesita dxc»).
    Error,
}

/// A small label (`h-[22px] rounded-full`).
#[component]
pub fn Badge(label: String, #[props(default)] variant: BadgeVariant, icon: Option<&'static str>) -> Element {
    todo!()
}

/// A filter or rule chip (`.chip`); selectable when `onclick` is set.
#[component]
pub fn Chip(
    label: String,
    icon: Option<&'static str>,
    #[props(default)] selected: bool,
    /// A trailing × that removes it (rule chips of the profile header).
    on_remove: Option<EventHandler<()>>,
    onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    todo!()
}

/// A Material Symbols Rounded icon (the bundled subset, by ligature name).
/// Decorative (`aria-hidden`) unless `label` is set.
#[component]
pub fn Icon(
    name: &'static str,
    /// FILL 1, weight 500 (selected states).
    #[props(default)]
    fill: bool,
    /// 18, 20, 24 (default) or 32 px.
    #[props(default = 24)]
    size: u8,
    label: Option<String>,
) -> Element {
    todo!()
}

/// The M3 Expressive loading indicator (a morphing shape); a plain spinner
/// under `prefers-reduced-motion`.
#[component]
pub fn LoadingIndicator(
    /// 24, 38 (default) or 48 px.
    #[props(default = 38)]
    size: u8,
    /// `aria-label` (translated).
    label: String,
) -> Element {
    todo!()
}
