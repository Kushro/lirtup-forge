//! «Agregar efecto»: the MagpieFX catalog of `ListEffects` (the bundled
//! effects and the person's own in `~/.config/linux-rt-upscaler/effects`),
//! searchable, with «Sin probar» on the effects not checked against Magpie.
//! Adding one appends it to the chain and compiles it ahead of time.

use dioxus::prelude::*;
use lirtup_config::MagpieEffectSpec;
use lirtup_proto::{EffectInfo, EffectOrigin};

use super::chain::{CHAIN_KEY, ChainItem};
use super::widgets::{CostBadge, yaml};
use crate::app::{use_dispatch, use_store};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Chip, Dialog as DialogBox, LoadingIndicator,
    TextField,
};
use crate::route::ModeId;
use crate::state::{Action, Scope};

/// Which effects the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Filter {
    /// All.
    #[default]
    All,
    /// Checked against Magpie.
    Verified,
    /// From the person's folder.
    Mine,
}

/// Whether `info` passes the filter and the search text.
pub(crate) fn matches(info: &EffectInfo, filter: Filter, query: &str) -> bool {
    let kind = match filter {
        Filter::All => true,
        Filter::Verified => info.verified,
        Filter::Mine => info.origin == EffectOrigin::User,
    };
    let query = query.trim().to_lowercase();
    kind && (query.is_empty()
        || info.name.to_lowercase().contains(&query)
        || info.sort_name.to_lowercase().contains(&query))
}

/// «Agregar efecto» (`Dialog::EffectPicker`): the catalog of `ListEffects`,
/// with «Sin probar» on the unverified ones.
#[component]
pub fn EffectPickerDialog(mode: ModeId) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let open = use_signal(|| true);
    let query = use_signal(String::new);
    let mut filter = use_signal(Filter::default);
    let costs = super::costs::use_costs();
    use_hook(move || {
        if store.peek().effects.is_empty() {
            dispatch(Action::ListEffects);
        }
    });

    let scope = Scope::Mode(mode.clone());
    let s = store.read();
    let chain: Vec<MagpieEffectSpec> = s.field::<Vec<MagpieEffectSpec>>(&scope, CHAIN_KEY).value();
    let in_chain: Vec<String> = chain.iter().map(|spec| ChainItem::parse(spec).name).collect();
    let mut effects: Vec<EffectInfo> = s
        .effects
        .iter()
        .filter(|e| matches(e, filter(), &query.read()))
        .cloned()
        .collect();
    effects.sort_by_key(|e| e.sort_name.to_lowercase());
    let total = s.effects.len();
    let loaded = total > 0;
    drop(s);

    let add = move |name: String| {
        let mut next = chain.clone();
        next.push(MagpieEffectSpec::Short(name.clone()));
        dispatch(Action::SetField {
            scope: scope.clone(),
            key: CHAIN_KEY.into(),
            value: yaml(next),
        });
        dispatch(Action::CompileEffect(name));
        dispatch(Action::CloseDialog);
    };
    let filters = [
        (Filter::All, t!("modes-picker-all", count = total)),
        (Filter::Verified, t!("modes-picker-verified")),
        (Filter::Mine, t!("modes-picker-mine")),
    ];
    let shown = effects.len();
    rsx! {
        DialogBox {
            open,
            icon: "add",
            title: t!("modes-picker-title"),
            wide: true,
            on_close: move |_| dispatch(Action::CloseDialog),
            actions: rsx! {
                Button { label: t!("modes-close"), variant: ButtonVariant::Text, onclick: move |_| dispatch(Action::CloseDialog) }
            },
            div { class: "flex flex-col gap-3 min-h-0",
                TextField {
                    value: query,
                    label: t!("modes-picker-search"),
                    leading_icon: "search",
                    autofocus: true,
                }
                div { class: "flex flex-wrap gap-2", role: "group", "aria-label": t!("modes-picker-filter"),
                    for (value, label) in filters {
                        Chip { key: "{label}", label, selected: filter() == value, onclick: move |_| filter.set(value) }
                    }
                }
                p { class: "t-bs text-on-surface-variant m-0", {t!("modes-picker-help")} }
                if !loaded {
                    div { class: "flex items-center gap-3 py-6",
                        LoadingIndicator { size: 38, label: t!("modes-picker-loading") }
                        span { class: "t-bm", {t!("modes-picker-loading")} }
                    }
                } else if shown == 0 {
                    p { class: "t-bm m-0 py-6 text-on-surface-variant", {t!("modes-picker-empty")} }
                } else {
                    ul { class: "flex flex-col gap-0.5 max-h-[50vh] overflow-auto m-0 p-0 list-none",
                        for info in effects {
                            PickerItem {
                                key: "{info.name}",
                                added: in_chain.contains(&info.name),
                                cost: costs.read().magpie(&info.name),
                                on_add: add.clone(),
                                info,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One catalog entry.
#[component]
fn PickerItem(info: EffectInfo, added: bool, cost: Option<f32>, on_add: EventHandler<String>) -> Element {
    let name = info.name.clone();
    let mut details = vec![t!("modes-effect-passes", count = info.passes)];
    if !info.params.is_empty() {
        details.push(t!("modes-effect-params", count = info.params.len()));
    }
    if info.fp16 {
        details.push("FP16".to_owned());
    }
    let details = details.join(" · ");
    let broken = info.error.is_some();
    rsx! {
        li { class: "flex flex-wrap items-center gap-3 min-h-14 px-4 py-2 rounded-xs first:rounded-t-lg-inc last:rounded-b-lg-inc bg-surface-container",
            div { class: "flex-1 min-w-48 flex flex-col",
                div { class: "flex flex-wrap items-center gap-2",
                    span { class: "t-ts", "{info.name}" }
                    if !info.verified {
                        Badge { label: t!("modes-untested"), icon: "science" }
                    }
                    if info.origin == EffectOrigin::User {
                        Badge { label: t!("modes-effect-user"), variant: BadgeVariant::Primary }
                    }
                    if added {
                        Badge { label: t!("modes-picker-in-chain"), icon: "check" }
                    }
                }
                span { class: "t-bs text-on-surface-variant", "{details}" }
                if let Some(error) = &info.error {
                    span { class: "t-bs text-error break-words", {t!("modes-effect-parse-error", error = error.clone())} }
                }
            }
            CostBadge { ms: cost }
            Button {
                label: t!("modes-picker-add"),
                variant: ButtonVariant::Tonal,
                size: ButtonSize::Xs,
                icon: "add",
                disabled: broken,
                onclick: move |_| on_add.call(name.clone()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, verified: bool, origin: EffectOrigin) -> EffectInfo {
        EffectInfo {
            name: name.into(),
            sort_name: name.into(),
            passes: 1,
            params: Vec::new(),
            verified,
            origin,
            fp16: false,
            error: None,
        }
    }

    #[test]
    fn filters_and_search() {
        let a = info("Anime4K_Upscale_S", true, EffectOrigin::Builtin);
        let b = info("CRT_Geom", false, EffectOrigin::Builtin);
        let c = info("Mine", false, EffectOrigin::User);
        assert!(matches(&a, Filter::All, ""));
        assert!(matches(&a, Filter::Verified, "anime"));
        assert!(!matches(&b, Filter::Verified, ""));
        assert!(matches(&c, Filter::Mine, " MIN "));
        assert!(!matches(&a, Filter::Mine, ""));
        assert!(!matches(&a, Filter::All, "crt"));
    }
}
