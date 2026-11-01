//! Inicio (front B): the session card, activation, the daemon switch and
//! the window grid with its filters, tile menu and keyboard navigation.
//!
//! | Module | Piece of 6.1 |
//! | --- | --- |
//! | [`session`] | `SessionCard`: starting, running, failed (Reintentar) |
//! | [`activation`] | `ActivationGroup`: the two hotkeys, «Empezar» with a delay, the daemon |
//! | [`grid`] | `WindowsSection`: header, quick filter, search, `WindowGrid`, `TileMenu` |

mod activation;
mod grid;
mod session;

use dioxus::prelude::*;
use lirtup_proto::{SessionKind, Target};

use crate::route::ProfileName;
use crate::state::Action;

/// The last start asked from Inicio, for «Reintentar» of a failed session
/// (kept here, not in the store: only the card uses it).
static LAST_START: GlobalSignal<Option<Action>> = Signal::global(|| None);

/// The Inicio page. Reads the store (`use_store`), sends actions
/// (`use_dispatch`); the thumbnails come from [`crate::thumbs::Thumbs`] (a
/// context `app.rs` provides).
#[component]
pub fn Home() -> Element {
    rsx! {
        div { class: "flex flex-col gap-7 w-full max-w-[1040px] px-10 pt-8 pb-12",
            h1 { class: "t-hl m-0", {t!("home-title")} }
            session::SessionCard {}
            activation::ActivationGroup {}
            grid::WindowsSection {}
        }
    }
}

/// Start scaling and remember it for «Reintentar».
fn start(
    dispatch: Callback<Action>,
    target: Target,
    kind: SessionKind,
    delay_s: Option<f32>,
    profile: Option<ProfileName>,
) {
    let action = Action::StartScaling {
        target,
        kind,
        delay_s,
        profile,
    };
    *LAST_START.write() = Some(action.clone());
    dispatch(action);
}

/// Copy `text` to the clipboard through the webview (the async Clipboard
/// API, else a hidden textarea and `execCommand`).
pub(crate) fn copy_to_clipboard(text: &str) {
    let text = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
    let script = format!(
        r#"
        const text = {text};
        const fallback = () => {{
            const area = document.createElement("textarea");
            area.value = text;
            area.style.position = "fixed";
            area.style.opacity = "0";
            document.body.appendChild(area);
            area.select();
            document.execCommand("copy");
            area.remove();
        }};
        if (navigator.clipboard && navigator.clipboard.writeText) {{
            navigator.clipboard.writeText(text).catch(fallback);
        }} else {{
            fallback();
        }}
        "#
    );
    document::eval(&script);
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::collections::HashMap;

    use fluent_templates::fluent_bundle::FluentValue;

    use crate::i18n::{Lang, tr_in};

    /// The message ids of a Fluent file (`id = …` at the start of a line).
    fn keys(ftl: &str) -> Vec<&str> {
        ftl.lines()
            .filter(|l| l.starts_with(|c: char| c.is_ascii_lowercase()))
            .filter_map(|l| l.split_once(" =").map(|(k, _)| k.trim()))
            .collect()
    }

    #[test]
    fn home_and_about_texts_in_both_languages() {
        for (es, en) in [
            (
                include_str!("../../../i18n/es/home.ftl"),
                include_str!("../../../i18n/en/home.ftl"),
            ),
            (
                include_str!("../../../i18n/es/about.ftl"),
                include_str!("../../../i18n/en/about.ftl"),
            ),
        ] {
            // Same ids in both files, and every id resolves with the
            // arguments the screens pass (`has_key` does not take messages
            // with variables yet).
            let args: HashMap<Cow<'static, str>, FluentValue<'static>> = [
                "title", "stage", "gpu", "budget", "seconds", "count", "ms", "version",
            ]
            .into_iter()
            .map(|name| (Cow::Borrowed(name), FluentValue::from(1)))
            .collect();
            assert_eq!(keys(es), keys(en));
            for key in keys(es) {
                assert_ne!(tr_in(Lang::Es, key, Some(&args)), key, "es: {key}");
                assert_ne!(tr_in(Lang::En, key, Some(&args)), key, "en: {key}");
            }
        }
    }
}
