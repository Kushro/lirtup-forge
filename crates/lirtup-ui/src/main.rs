//! `lirtup-gui`: the Dioxus Desktop GUI of LirtUp Forge 2.0 (section 6 of the
//! analysis).
//!
//! Round 0 only pins Dioxus and proves it builds; the reviewer writes the
//! real skeleton (`app.rs`, `route.rs`, `link.rs`, `state/`, `i18n.rs`,
//! `components/`) before round 3 (5.5 of the analysis).

use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        main { "LirtUp Forge" }
    }
}
