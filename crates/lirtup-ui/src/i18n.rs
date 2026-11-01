//! Translations (3.9 of the analysis): Fluent files in
//! `crates/lirtup-ui/i18n/{es,en}/<screen>.ftl`, one per screen and front so
//! nobody edits another's, compiled in with `fluent-templates`.
//!
//! The language is the global signal [`LANG`]: a component that calls
//! [`t!`] reads it and re-renders when it changes, without restarting.
//! Spanish is the fallback: a key missing in English shows in Spanish (the
//! round 4 test checks both are complete), and a key missing in both shows
//! as the key itself and is logged once.
//!
//! ```ignore
//! rsx! { span { {t!("home-windows-title", count = windows.len())} } }
//! ```

use std::borrow::Cow;
use std::collections::HashMap;

use dioxus::prelude::*;
use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::{LanguageIdentifier, Loader, langid, static_loader};

static_loader! {
    static LOCALES = {
        locales: "./i18n",
        fallback_language: "es",
        // No Unicode isolation marks around arguments: the webview shows
        // them as boxes in some fonts.
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

static ES: LanguageIdentifier = langid!("es");
static EN: LanguageIdentifier = langid!("en");

/// A language of the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    /// Español.
    #[default]
    Es,
    /// English.
    En,
}

impl Lang {
    /// From `gui-config.yaml`; `System` reads `LC_ALL`, `LC_MESSAGES`,
    /// `LANG` (Spanish unless it starts with `en`, as 1.x).
    pub fn from_config(language: lirtup_config::gui::Language) -> Lang {
        use lirtup_config::gui::Language;
        match language {
            Language::Es => Lang::Es,
            Language::En => Lang::En,
            Language::System => {
                let env = ["LC_ALL", "LC_MESSAGES", "LANG"]
                    .iter()
                    .filter_map(|v| std::env::var(v).ok())
                    .find(|v| !v.is_empty())
                    .unwrap_or_default();
                if env.starts_with("en") { Lang::En } else { Lang::Es }
            }
        }
    }

    fn id(self) -> &'static LanguageIdentifier {
        match self {
            Lang::Es => &ES,
            Lang::En => &EN,
        }
    }
}

/// The language now. Ajustes ▸ Idioma writes it.
pub static LANG: GlobalSignal<Lang> = Signal::global(Lang::default);

/// `key` in the current language (subscribes the calling component).
pub fn tr(key: &str) -> String {
    tr_in(*LANG.read(), key, None)
}

/// `key` with arguments in the current language.
pub fn tr_args(key: &str, args: &[(&'static str, FluentValue<'static>)]) -> String {
    let map: HashMap<Cow<'static, str>, FluentValue<'static>> =
        args.iter().map(|(k, v)| (Cow::Borrowed(*k), v.clone())).collect();
    tr_in(*LANG.read(), key, Some(&map))
}

/// `key` in `lang`, without the signal (tests, the tray menu thread).
pub fn tr_in(
    lang: Lang,
    key: &str,
    args: Option<&HashMap<Cow<'static, str>, FluentValue<'static>>>,
) -> String {
    match LOCALES.try_lookup_complete(lang.id(), key, args) {
        Some(text) => text,
        None => {
            tracing::warn!("missing translation: {key}");
            key.to_owned()
        }
    }
}

/// Whether `key` exists in `lang` itself (not by fallback); for the round 4
/// test of complete translations.
pub fn has_key(lang: Lang, key: &str) -> bool {
    LOCALES
        .lookup_single_language::<&str>(lang.id(), key, None)
        .is_ok()
}

/// An argument for [`tr_args`] (numbers stay numbers, so Fluent can pick
/// plurals).
pub fn arg(value: impl Into<FluentValue<'static>>) -> FluentValue<'static> {
    value.into()
}

/// A translated string: `t!("key")` or `t!("key", name = value, ...)`.
/// Argument names are the Fluent variables (`{ $name }`).
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::tr($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr_args($key, &[$((stringify!($name), $crate::i18n::arg($value))),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_keys_exist_in_both_languages() {
        for key in [
            "app-title",
            "nav-home",
            "nav-modes",
            "nav-settings",
            "nav-about",
            "nav-new-profile",
        ] {
            assert!(has_key(Lang::Es, key), "es: {key}");
            assert!(has_key(Lang::En, key), "en: {key}");
        }
    }

    #[test]
    fn arguments_and_fallback() {
        let mut args = HashMap::new();
        args.insert(Cow::Borrowed("count"), FluentValue::from(3));
        assert_eq!(
            tr_in(Lang::Es, "savebar-changes", Some(&args)),
            "3 cambios sin guardar"
        );
        assert_eq!(
            tr_in(Lang::En, "savebar-changes", Some(&args)),
            "3 unsaved changes"
        );
        args.insert(Cow::Borrowed("count"), FluentValue::from(1));
        assert_eq!(
            tr_in(Lang::Es, "savebar-changes", Some(&args)),
            "1 cambio sin guardar"
        );
        assert_eq!(tr_in(Lang::Es, "no-such-key", None), "no-such-key");
    }
}
