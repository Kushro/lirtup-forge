//! The MagpieFX chain of a mode: one card per effect, reordered by
//! dragging (HTML5 drag and drop in the webview) or with the keyboard
//! (arrows on the handle, or the up and down buttons), its cost, its
//! parameters as controls generated from the catalog's `EffectParamInfo`,
//! and the effects the running session disabled (`EffectCompile { session:
//! true }`) with their error.
//!
//! Every change writes the whole `magpie_effects` list as one edit. Items
//! that did not change keep the spelling they were read with (`"Name"`,
//! `"Name:p=1"` or the long form).

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lirtup_config::MagpieEffectSpec;
use lirtup_proto::{EffectInfo, EffectOrigin, EffectParamInfo, ParamKind};

use super::costs::Costs;
use super::widgets::{CostBadge, fmt_num, use_value, yaml};
use crate::app::{use_dispatch, use_store};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, IconButton, LoadingIndicator, Slider,
};
use crate::route::ModeId;
use crate::state::{Action, CompileState, Dialog, Scope};

/// The YAML key of the chain.
pub(crate) const CHAIN_KEY: &str = "magpie_effects";

/// One effect of the chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChainItem {
    /// As read (written back unchanged if the item is not edited).
    pub spec: MagpieEffectSpec,
    /// Catalog name or `.hlsl` path.
    pub name: String,
    /// Parameter overrides.
    pub params: BTreeMap<String, f64>,
}

impl ChainItem {
    /// From a spec: `"Name"`, `"Name:p=1,q=2"` or `{name, params}`.
    pub fn parse(spec: &MagpieEffectSpec) -> ChainItem {
        let (name, params) = match spec {
            MagpieEffectSpec::Full { name, params } => (name.clone(), params.clone()),
            MagpieEffectSpec::Short(text) => match text.split_once(':') {
                Some((name, rest)) if rest.contains('=') => {
                    let params = rest
                        .split(',')
                        .filter_map(|kv| {
                            let (k, v) = kv.split_once('=')?;
                            Some((k.trim().to_owned(), v.trim().parse().ok()?))
                        })
                        .collect();
                    (name.trim().to_owned(), params)
                }
                _ => (text.trim().to_owned(), BTreeMap::new()),
            },
        };
        ChainItem {
            spec: spec.clone(),
            name,
            params,
        }
    }

    /// A new item for `name` without overrides.
    pub fn new(name: &str) -> ChainItem {
        ChainItem::parse(&MagpieEffectSpec::Short(name.to_owned()))
    }

    /// With parameter `param` set to `value` (removed when it is the
    /// default), re-spelled: the short name alone, or the long form.
    pub fn with_param(&self, param: &str, value: f64, default: f64) -> ChainItem {
        let mut params = self.params.clone();
        if (value - default).abs() < 1e-9 {
            params.remove(param);
        } else {
            params.insert(param.to_owned(), value);
        }
        ChainItem::from_parts(self.name.clone(), params)
    }

    /// Without overrides.
    pub fn reset(&self) -> ChainItem {
        ChainItem::from_parts(self.name.clone(), BTreeMap::new())
    }

    fn from_parts(name: String, params: BTreeMap<String, f64>) -> ChainItem {
        let spec = if params.is_empty() {
            MagpieEffectSpec::Short(name.clone())
        } else {
            MagpieEffectSpec::Full {
                name: name.clone(),
                params: params.clone(),
            }
        };
        ChainItem { spec, name, params }
    }

    /// The name to show: a path shows its file stem.
    pub fn display_name(&self) -> String {
        if self.name.ends_with(".hlsl") {
            std::path::Path::new(&self.name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&self.name)
                .to_owned()
        } else {
            self.name.clone()
        }
    }
}

/// `items` with the one at `from` moved to `to`.
pub(crate) fn moved<T: Clone>(items: &[T], from: usize, to: usize) -> Vec<T> {
    let mut out = items.to_vec();
    if from < out.len() && to < out.len() && from != to {
        let item = out.remove(from);
        out.insert(to, item);
    }
    out
}

/// The chain's names, for costs and the bench.
pub(crate) fn chain_names(specs: &[MagpieEffectSpec]) -> Vec<String> {
    specs.iter().map(|s| ChainItem::parse(s).name).collect()
}

/// The chain editor of mode `id`.
#[component]
pub(crate) fn EffectChain(id: ModeId, costs: Costs) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let scope = Scope::Mode(id.clone());
    let specs = use_value::<Vec<MagpieEffectSpec>>(scope.clone(), CHAIN_KEY.into());
    let items: Vec<ChainItem> = specs.read().iter().map(ChainItem::parse).collect();
    let catalog_loaded = !store.read().effects.is_empty();
    // The card being dragged and the one under the pointer.
    let mut dragging = use_signal(|| None::<usize>);
    let mut over = use_signal(|| None::<usize>);

    let write = {
        let scope = scope.clone();
        move |items: Vec<ChainItem>| {
            let specs: Vec<MagpieEffectSpec> = items.into_iter().map(|i| i.spec).collect();
            dispatch(Action::SetField {
                scope: scope.clone(),
                key: CHAIN_KEY.into(),
                value: yaml(specs),
            });
        }
    };
    let on_move = {
        let write = write.clone();
        let items = items.clone();
        move |(from, to): (usize, usize)| write(moved(&items, from, to))
    };
    let on_change = {
        let write = write.clone();
        let items = items.clone();
        move |(index, item): (usize, Option<ChainItem>)| {
            let mut next = items.clone();
            match item {
                Some(item) => next[index] = item,
                None => {
                    next.remove(index);
                }
            }
            write(next);
        }
    };
    let count = items.len();
    rsx! {
        div { class: "flex flex-col gap-0.5", role: "list", "aria-label": t!("modes-chain-label"),
            if items.is_empty() {
                div { class: "flex items-center gap-3 min-h-16 px-5 rounded-lg-inc bg-surface-container t-bm text-on-surface-variant",
                    {t!("modes-chain-empty")}
                }
            }
            for (index, item) in items.into_iter().enumerate() {
                EffectCard {
                    key: "{index}:{item.name}",
                    index,
                    count,
                    info: store.read().effects.iter().find(|e| e.name == item.name).cloned(),
                    compile: store.read().compiling.get(&item.name).cloned(),
                    cost: costs.magpie(&item.name),
                    catalog_loaded,
                    item,
                    dragging: dragging(),
                    over: over() == Some(index),
                    on_drag: move |state: DragState| match state {
                        DragState::Start => dragging.set(Some(index)),
                        DragState::Over => over.set(Some(index)),
                        DragState::End => {
                            dragging.set(None);
                            over.set(None);
                        }
                    },
                    on_move: on_move.clone(),
                    on_change: on_change.clone(),
                }
            }
        }
        div { class: "flex flex-wrap items-center gap-2",
            Button {
                label: t!("modes-add-effect"),
                variant: ButtonVariant::Outlined,
                icon: "add",
                onclick: move |_| dispatch(Action::OpenDialog(Dialog::EffectPicker { mode: id.clone() })),
            }
        }
    }
}

/// What a card tells the chain while dragging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DragState {
    /// This card started being dragged.
    Start,
    /// Something is dragged over this card.
    Over,
    /// The drag finished (dropped or cancelled).
    End,
}

/// One effect of the chain.
#[component]
fn EffectCard(
    index: usize,
    count: usize,
    item: ChainItem,
    /// The catalog entry (`None`: not loaded yet, or not in the catalog).
    info: Option<EffectInfo>,
    compile: Option<CompileState>,
    cost: Option<f32>,
    catalog_loaded: bool,
    dragging: Option<usize>,
    over: bool,
    on_drag: EventHandler<DragState>,
    /// `(from, to)`.
    on_move: EventHandler<(usize, usize)>,
    /// `(index, Some(new))` or `(index, None)` to remove it.
    on_change: EventHandler<(usize, Option<ChainItem>)>,
) -> Element {
    // Only the handle starts a drag, so sliders inside keep working.
    let mut grab = use_signal(|| false);
    let mut open = use_signal(|| false);
    let name = item.display_name();
    let params = info.as_ref().map(|i| i.params.clone()).unwrap_or_default();
    let has_params = !params.is_empty();
    let overrides = item.params.len();

    let disabled_by_session = compile
        .as_ref()
        .filter(|c| c.session)
        .and_then(|c| c.error.clone());
    let compile_error = compile
        .as_ref()
        .filter(|c| !c.session && c.done)
        .and_then(|c| c.error.clone());
    let compiling = compile.as_ref().is_some_and(|c| !c.done);
    let parse_error = info.as_ref().and_then(|i| i.error.clone());
    let missing = catalog_loaded && info.is_none() && !item.name.ends_with(".hlsl");

    let mut subtitle = Vec::new();
    if let Some(info) = &info {
        subtitle.push(t!("modes-effect-passes", count = info.passes));
        if has_params {
            subtitle.push(t!("modes-effect-params", count = info.params.len()));
        }
    }
    if overrides > 0 {
        subtitle.push(t!("modes-effect-overrides", count = overrides));
    }
    let subtitle = subtitle.join(" · ");

    let error = disabled_by_session
        .as_ref()
        .map(|e| t!("modes-effect-disabled", error = e.clone()))
        .or_else(|| {
            compile_error
                .as_ref()
                .map(|e| t!("modes-effect-compile-error", error = e.clone()))
        })
        .or_else(|| {
            parse_error
                .as_ref()
                .map(|e| t!("modes-effect-parse-error", error = e.clone()))
        })
        .or_else(|| missing.then(|| t!("modes-effect-missing")));

    let tone = if disabled_by_session.is_some() || compile_error.is_some() || parse_error.is_some() {
        "bg-error-container text-on-error-container"
    } else {
        "bg-surface-container"
    };
    let drop_mark = if over && dragging.is_some_and(|d| d != index) {
        "shadow-[inset_0_3px_0_var(--primary)]"
    } else {
        ""
    };
    let lifted = if dragging == Some(index) { "opacity-60" } else { "" };
    let up = index.checked_sub(1);
    let down = (index + 1 < count).then_some(index + 1);
    let item_for_params = item.clone();
    let item_for_reset = item.clone();

    rsx! {
        div {
            class: "flex flex-col gap-2 px-4 py-3 min-h-16 rounded-xs first:rounded-t-lg-inc last:rounded-b-lg-inc {tone} {drop_mark} {lifted}",
            role: "listitem",
            draggable: if grab() { "true" } else { "false" },
            ondragstart: move |_| on_drag.call(DragState::Start),
            ondragover: move |e: DragEvent| {
                e.prevent_default();
                on_drag.call(DragState::Over);
            },
            ondrop: move |e: DragEvent| {
                e.prevent_default();
                if let Some(from) = dragging {
                    on_move.call((from, index));
                }
                on_drag.call(DragState::End);
                grab.set(false);
            },
            ondragend: move |_| {
                on_drag.call(DragState::End);
                grab.set(false);
            },
            div { class: "flex items-center gap-3",
                // The handle: drag with the mouse, arrows with the keyboard.
                span {
                    class: "cursor-grab",
                    onmousedown: move |_| grab.set(true),
                    onmouseup: move |_| grab.set(false),
                    onkeydown: move |e: KeyboardEvent| {
                        let to = match e.key() {
                            Key::ArrowUp => up,
                            Key::ArrowDown => down,
                            _ => None,
                        };
                        if let Some(to) = to {
                            e.prevent_default();
                            on_move.call((index, to));
                        }
                    },
                    IconButton { icon: "drag_indicator", label: t!("modes-effect-move", name = name.clone()), size: 32 }
                }
                span { class: "mono t-bm text-on-surface-variant w-5 text-right", "{index + 1}" }
                div { class: "flex-1 min-w-0 flex flex-col",
                    div { class: "flex flex-wrap items-center gap-2",
                        span { class: "t-ts truncate", "{name}" }
                        if info.as_ref().is_some_and(|i| !i.verified) {
                            Badge { label: t!("modes-untested"), icon: "science" }
                        }
                        if info.as_ref().is_some_and(|i| i.origin == EffectOrigin::User) {
                            Badge { label: t!("modes-effect-user"), variant: BadgeVariant::Primary }
                        }
                        if disabled_by_session.is_some() {
                            Badge { label: t!("modes-effect-disabled-badge"), variant: BadgeVariant::Error, icon: "block" }
                        }
                    }
                    if !subtitle.is_empty() {
                        span { class: "t-bs opacity-80", "{subtitle}" }
                    }
                }
                if compiling {
                    LoadingIndicator { size: 24, label: t!("modes-effect-compiling") }
                }
                CostBadge { ms: cost, strong: true }
                if has_params {
                    IconButton {
                        icon: "tune",
                        label: t!("modes-effect-params-toggle", name = name.clone()),
                        selected: open(),
                        size: 32,
                        onclick: move |_| open.toggle(),
                    }
                }
                IconButton {
                    icon: "arrow_upward",
                    label: t!("modes-effect-up"),
                    size: 32,
                    disabled: up.is_none(),
                    onclick: move |_| if let Some(to) = up { on_move.call((index, to)) },
                }
                IconButton {
                    icon: "arrow_downward",
                    label: t!("modes-effect-down"),
                    size: 32,
                    disabled: down.is_none(),
                    onclick: move |_| if let Some(to) = down { on_move.call((index, to)) },
                }
                IconButton {
                    icon: "close",
                    label: t!("modes-effect-remove", name = name.clone()),
                    size: 32,
                    onclick: move |_| on_change.call((index, None)),
                }
            }
            if let Some(error) = error {
                p { class: "t-bs pl-14 m-0 whitespace-pre-wrap break-words", role: "status", "{error}" }
            }
            if open() && has_params {
                div { class: "flex flex-col gap-1 pl-14",
                    for param in params {
                        EffectParam {
                            key: "{param.name}",
                            value: item_for_params.params.get(&param.name).copied().unwrap_or(param.default),
                            on_change: {
                                let item = item_for_params.clone();
                                let param = param.clone();
                                move |v: f64| on_change.call((index, Some(item.with_param(&param.name, v, param.default))))
                            },
                            param,
                        }
                    }
                    if overrides > 0 {
                        div {
                            Button {
                                label: t!("modes-effect-reset-params"),
                                variant: ButtonVariant::Text,
                                size: ButtonSize::Xs,
                                icon: "restart_alt",
                                onclick: move |_| on_change.call((index, Some(item_for_reset.reset()))),
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One parameter: a slider with the catalog's range and step.
#[component]
fn EffectParam(param: EffectParamInfo, value: f64, on_change: EventHandler<f64>) -> Element {
    let mut control = use_signal(|| value);
    use_effect(use_reactive((&value,), move |(value,)| {
        if *control.peek() != value {
            control.set(value);
        }
    }));
    let label = if param.label.is_empty() {
        param.name.clone()
    } else {
        param.label.clone()
    };
    let int = param.kind == ParamKind::Int;
    let step = if int { param.step.max(1.0) } else { param.step };
    let decimals = decimals_of(step);
    let changed = (value - param.default).abs() > 1e-9;
    rsx! {
        div { class: "flex flex-wrap items-center gap-3",
            span { class: "t-bs w-40 truncate", title: "{param.name}", "{label}" }
            div { class: "flex-1 min-w-48",
                Slider {
                    value: control,
                    min: param.min,
                    max: param.max,
                    step,
                    label: label.clone(),
                    format: Callback::new(move |v: f64| fmt_num(v, decimals)),
                    on_change: move |v: f64| on_change.call(if int { v.round() } else { v }),
                }
            }
            if changed {
                span { class: "t-bs text-on-surface-variant mono",
                    {t!("modes-effect-param-default", value = fmt_num(param.default, decimals))}
                }
            }
        }
    }
}

/// Decimals to show for a step (`0.05` → 2, `1` → 0), at most 4.
pub(crate) fn decimals_of(step: f64) -> usize {
    (0..4)
        .find(|&d| {
            let scaled = step * 10f64.powi(d as i32);
            (scaled - scaled.round()).abs() < 1e-6
        })
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_spellings() {
        let a = ChainItem::parse(&MagpieEffectSpec::Short("LumaSharpen".into()));
        assert_eq!(a.name, "LumaSharpen");
        assert!(a.params.is_empty());

        let b = ChainItem::parse(&MagpieEffectSpec::Short(
            "LumaSharpen:sharpStrength=0.5, pattern=2".into(),
        ));
        assert_eq!(b.name, "LumaSharpen");
        assert_eq!(b.params.get("sharpStrength"), Some(&0.5));
        assert_eq!(b.params.get("pattern"), Some(&2.0));

        let mut params = BTreeMap::new();
        params.insert("x".to_owned(), 1.0);
        let c = ChainItem::parse(&MagpieEffectSpec::Full {
            name: "CAS".into(),
            params: params.clone(),
        });
        assert_eq!((c.name.as_str(), &c.params), ("CAS", &params));

        let path = ChainItem::parse(&MagpieEffectSpec::Short("/home/me/fx/Mine.hlsl".into()));
        assert_eq!(path.name, "/home/me/fx/Mine.hlsl");
        assert_eq!(path.display_name(), "Mine");
    }

    #[test]
    fn untouched_items_keep_their_spelling() {
        let spec = MagpieEffectSpec::Short("LumaSharpen:sharpStrength=0.5".into());
        let item = ChainItem::parse(&spec);
        assert_eq!(item.spec, spec);
        // Editing re-spells it in the long form; back to the default, short.
        let edited = item.with_param("sharpStrength", 0.8, 0.65);
        assert!(
            matches!(&edited.spec, MagpieEffectSpec::Full { params, .. } if params["sharpStrength"] == 0.8)
        );
        let back = edited.with_param("sharpStrength", 0.65, 0.65);
        assert_eq!(back.spec, MagpieEffectSpec::Short("LumaSharpen".into()));
        assert_eq!(edited.reset().spec, MagpieEffectSpec::Short("LumaSharpen".into()));
    }

    #[test]
    fn moving_items() {
        let v = [1, 2, 3, 4];
        assert_eq!(moved(&v, 0, 2), vec![2, 3, 1, 4]);
        assert_eq!(moved(&v, 3, 0), vec![4, 1, 2, 3]);
        assert_eq!(moved(&v, 1, 1), v.to_vec());
        assert_eq!(moved(&v, 9, 0), v.to_vec());
    }

    #[test]
    fn decimals_follow_the_step() {
        assert_eq!(decimals_of(1.0), 0);
        assert_eq!(decimals_of(0.1), 1);
        assert_eq!(decimals_of(0.05), 2);
        assert_eq!(decimals_of(0.001), 3);
        assert_eq!(decimals_of(0.00001), 4);
    }
}
