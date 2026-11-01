//! Pieces the mode editor and Ajustes share: a setting of the store bound
//! to a control, the rows built from it, and number formatting.
//!
//! A row reads its value and origin through memos over the store, so it
//! renders again only when its own setting changes. The control edits a
//! signal the row owns; the signal follows the store (Revert, Inherit,
//! another window saving) and a change goes back as an `Action`.

use dioxus::prelude::*;
use lirtup_config::Value;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::app::{use_dispatch, use_store};
use crate::components::{Badge, BadgeVariant, SettingRow, Slider, Switch};
use crate::i18n::{LANG, Lang, tr};
use crate::state::{Action, Origin, Scope};

/// The value a scope gives a key, after its layers (a memo over the store).
pub(crate) fn use_value<T>(scope: Scope, key: String) -> Memo<T>
where
    T: Clone + PartialEq + DeserializeOwned + 'static,
{
    let store = use_store();
    use_memo(move || store.read().field::<T>(&scope, &key).value())
}

/// Where the value of a key comes from (a memo over the store).
pub(crate) fn use_origin(scope: Scope, key: String) -> Memo<Origin> {
    let store = use_store();
    use_memo(move || store.read().origin(&scope, &key))
}

/// A signal for a control that follows `value`, mapped with `map`.
pub(crate) fn use_control<T, U>(value: Memo<T>, map: fn(&T) -> U) -> Signal<U>
where
    T: Clone + PartialEq + 'static,
    U: Clone + PartialEq + 'static,
{
    let mut control = use_signal(|| map(&value.peek()));
    use_effect(move || {
        let v = map(&value.read());
        if *control.peek() != v {
            control.set(v);
        }
    });
    control
}

/// `v` as YAML.
pub(crate) fn yaml(v: impl Serialize) -> Value {
    serde_yaml_ng::to_value(v).unwrap_or(Value::Null)
}

/// A slider value as YAML: an integer for integer settings, else rounded to
/// 4 decimals so a step of 0,05 does not write `0.30000000000000004`.
pub(crate) fn number(v: f64, int: bool) -> Value {
    if int {
        yaml(v.round() as i64)
    } else {
        yaml((v * 10_000.0).round() / 10_000.0)
    }
}

/// `v` with `decimals` and the decimal separator of the language.
pub(crate) fn fmt_num(v: f64, decimals: usize) -> String {
    fmt_num_in(*LANG.read(), v, decimals)
}

/// [`fmt_num`] in `lang`, without the signal.
pub(crate) fn fmt_num_in(lang: Lang, v: f64, decimals: usize) -> String {
    let text = format!("{v:.decimals$}");
    match lang {
        Lang::Es => text.replace('.', ","),
        Lang::En => text,
    }
}

/// A cost in ms per frame: `0,9 ms`.
pub(crate) fn fmt_ms(ms: f32) -> String {
    let decimals = if ms < 10.0 { 1 } else { 0 };
    format!("{} ms", fmt_num(ms as f64, decimals))
}

/// The cost badge of an effect or a mode, if it was measured.
#[component]
pub(crate) fn CostBadge(ms: Option<f32>, #[props(default)] strong: bool) -> Element {
    let Some(ms) = ms else {
        return rsx! {};
    };
    let variant = if strong {
        BadgeVariant::Success
    } else {
        BadgeVariant::Neutral
    };
    rsx! { Badge { label: fmt_ms(ms), variant } }
}

/// A numeric setting: YAML key, Fluent key of its title, range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Num {
    /// YAML key.
    pub key: &'static str,
    /// Material Symbols name.
    pub icon: &'static str,
    /// Fluent key of the title.
    pub title: &'static str,
    /// Minimum.
    pub min: f64,
    /// Maximum.
    pub max: f64,
    /// Step.
    pub step: f64,
    /// Written as an integer.
    pub int: bool,
}

impl Num {
    /// A float setting.
    pub const fn float(
        key: &'static str,
        icon: &'static str,
        title: &'static str,
        min: f64,
        max: f64,
        step: f64,
    ) -> Num {
        Num {
            key,
            icon,
            title,
            min,
            max,
            step,
            int: false,
        }
    }

    /// An integer setting.
    pub const fn int(key: &'static str, icon: &'static str, title: &'static str, min: f64, max: f64) -> Num {
        Num {
            key,
            icon,
            title,
            min,
            max,
            step: 1.0,
            int: true,
        }
    }
}

/// A slider row of one numeric setting of `scope`.
#[component]
pub(crate) fn NumberRow(scope: Scope, num: Num, #[props(default)] disabled: bool) -> Element {
    let dispatch = use_dispatch();
    let value = use_value::<f64>(scope.clone(), num.key.into());
    let origin = use_origin(scope.clone(), num.key.into());
    let control = use_control(value, |v| *v);
    let set_scope = scope.clone();
    let title = tr(num.title);
    rsx! {
        SettingRow {
            icon: num.icon,
            title: title.clone(),
            origin: origin(),
            on_inherit: inherit(scope, num.key),
            Slider {
                value: control,
                min: num.min,
                max: num.max,
                step: num.step,
                label: title,
                disabled,
                on_change: move |v: f64| {
                    dispatch(Action::SetField {
                        scope: set_scope.clone(),
                        key: num.key.into(),
                        value: number(v, num.int),
                    })
                },
            }
        }
    }
}

/// A switch row of one boolean setting of `scope`, with an optional cost
/// badge before the switch.
#[component]
pub(crate) fn SwitchRow(
    scope: Scope,
    /// YAML key.
    setting: &'static str,
    icon: &'static str,
    /// Fluent key of the title.
    title: &'static str,
    /// Fluent key of the help text.
    subtitle: Option<&'static str>,
    cost: Option<f32>,
    #[props(default)] disabled: bool,
) -> Element {
    let dispatch = use_dispatch();
    let value = use_value::<bool>(scope.clone(), setting.into());
    let origin = use_origin(scope.clone(), setting.into());
    let control = use_control(value, |v| *v);
    let set_scope = scope.clone();
    let title = tr(title);
    rsx! {
        SettingRow {
            icon,
            title: title.clone(),
            subtitle: subtitle.map(tr),
            origin: origin(),
            on_inherit: inherit(scope, setting),
            CostBadge { ms: cost }
            Switch {
                value: control,
                label: title,
                disabled,
                on_change: move |on: bool| {
                    dispatch(Action::SetField { scope: set_scope.clone(), key: setting.into(), value: yaml(on) })
                },
            }
        }
    }
}

/// «Volver a heredar» for a row: `Inherit` of its key.
pub(crate) fn inherit(scope: Scope, key: &'static str) -> EventHandler<()> {
    let dispatch = use_dispatch();
    EventHandler::new(move |()| {
        dispatch(Action::Inherit {
            scope: scope.clone(),
            key: key.into(),
        })
    })
}

/// Run `f` with a Dioxus runtime, so `t!` can read the language signal.
#[cfg(test)]
pub(crate) fn in_runtime<R>(f: impl FnOnce() -> R) -> R {
    let dom = VirtualDom::new(|| rsx! {});
    dom.in_runtime(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_use_the_language_separator() {
        assert_eq!(fmt_num_in(Lang::Es, 1.375, 2), "1,38");
        assert_eq!(fmt_num_in(Lang::En, 1.375, 2), "1.38");
        assert_eq!(fmt_num_in(Lang::Es, 12.0, 0), "12");
    }

    #[test]
    fn slider_values_are_clean_yaml() {
        assert_eq!(number(0.1 + 0.2, false), yaml(0.3));
        assert_eq!(number(3.6, true), yaml(4));
        assert!(number(4.0, true).is_i64());
    }
}
