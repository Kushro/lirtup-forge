//! The drawer and the tabs.
// Front A: remove when the bodies are written.
#![allow(unused_variables)]

use dioxus::prelude::*;

/// The navigation drawer: 288 px, or an 80 px rail below 1100 px of window
/// width (labels become tooltips).
#[component]
pub fn NavDrawer(
    /// Rail mode.
    #[props(default)]
    rail: bool,
    /// The top part (scrolls): Inicio, Modos, the profiles.
    children: Element,
    /// Pinned at the bottom: Acerca de, Ajustes.
    footer: Element,
) -> Element {
    todo!()
}

/// A section title in the drawer («Perfiles»).
#[component]
pub fn NavHeader(label: String) -> Element {
    todo!()
}

/// What a [`NavItem`] shows before its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavIcon {
    /// A Material Symbols name.
    Symbol(&'static str),
    /// A profile: its icon (`/icon/profile/<name>` asset) or, without one,
    /// its initials on a color derived from the name.
    Profile {
        /// Initials («BS»).
        initials: String,
        /// Background when there is no image (`#3D6FB6`).
        color: String,
        /// The icon's URL, if the profile has one.
        image: Option<String>,
    },
}

/// One entry of the drawer (`h-13 rounded-full`).
#[component]
pub fn NavItem(
    label: String,
    icon: NavIcon,
    #[props(default)] selected: bool,
    /// Amber dot: unsaved changes.
    #[props(default)]
    dirty: bool,
    /// Amber ▶: this profile is scaling now.
    #[props(default)]
    running: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    todo!()
}

/// One tab of [`Tabs`].
#[derive(Debug, Clone, PartialEq)]
pub struct TabItem<T> {
    /// Returned by `on_select`.
    pub value: T,
    /// Text (translated).
    pub label: String,
    /// Material Symbols name.
    pub icon: &'static str,
    /// Amber dot: unsaved changes in the tab.
    pub dot: bool,
}

/// Tabs (`.tabs`; `role="tablist"`, arrows move between them).
#[component]
pub fn Tabs<T: Clone + PartialEq + 'static>(
    tabs: Vec<TabItem<T>>,
    selected: T,
    on_select: EventHandler<T>,
) -> Element {
    todo!()
}
