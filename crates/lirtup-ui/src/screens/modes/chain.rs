//! The MagpieFX chain of a mode, in two segments: «Antes del modelo»
//! (`magpie_effects_before_model`, on the capture at the game's
//! resolution) and «Después del modelo» (`magpie_effects`, on the model's
//! output), with a row for the model between them. One card per effect,
//! reordered by dragging (HTML5 drag and drop in the webview, also between
//! the segments) or with the keyboard (arrows on the handle, or the up and
//! down buttons), moved to the other segment with its `swap_vert` button,
//! its cost, its parameters as controls generated from the catalog's
//! `EffectParamInfo`, and the effects the running session disabled
//! (`EffectCompile { session: true }`) with their error. Those badges match
//! by name, so an effect in both lists shows it on both.
//!
//! Every change writes whole lists ([`apply_edit`]): moving an effect to
//! the other segment writes both, the one that loses it first. Items that
//! did not change keep the spelling they were read with (`"Name"`,
//! `"Name:p=1"` or the long form).
//!
//! A mode file that still has settings of the retired native engine shows
//! its chain read-only, as the read-time translation runs it
//! ([`translated_lists`]): an edit would be overridden by that translation
//! until the files are migrated.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lirtup_config::{MagpieEffectSpec, Mapping, Model, legacy};
use lirtup_proto::{EffectInfo, EffectOrigin, EffectParamInfo, ParamKind};

use super::costs::{Costs, model_scale, post_factor};
use super::widgets::{CostBadge, fmt_num, use_value, yaml};
use crate::app::{use_dispatch, use_store};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon, IconButton, LoadingIndicator, Slider,
};
use crate::route::ModeId;
use crate::state::{Action, CompileState, Dialog, Scope};

/// The YAML key of the chain after the model.
pub(crate) const CHAIN_KEY: &str = "magpie_effects";

/// The YAML key of the chain before the model.
pub(crate) const PRE_CHAIN_KEY: &str = "magpie_effects_before_model";

/// The key of one segment of the chain.
pub(crate) fn chain_key(before_model: bool) -> &'static str {
    if before_model { PRE_CHAIN_KEY } else { CHAIN_KEY }
}

/// Both lists of a mode.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ChainLists {
    /// `magpie_effects_before_model`.
    pub pre: Vec<MagpieEffectSpec>,
    /// `magpie_effects`.
    pub post: Vec<MagpieEffectSpec>,
}

impl ChainLists {
    /// The lists of a mode file: [`translated_lists`] when it still has
    /// native-engine keys, else `pre` and `post` as given.
    pub fn of(raw: Option<&Mapping>, pre: &[MagpieEffectSpec], post: &[MagpieEffectSpec]) -> ChainLists {
        raw.and_then(translated_lists).unwrap_or_else(|| ChainLists {
            pre: pre.to_vec(),
            post: post.to_vec(),
        })
    }
}

/// What the read-time translation makes of a mode file with keys of the
/// retired native engine (`lirtup_config::legacy::translate_native` on a
/// copy); `None` when it has none.
pub(crate) fn translated_lists(raw: &Mapping) -> Option<ChainLists> {
    if !legacy::has_legacy_keys(raw) {
        return None;
    }
    let mut raw = raw.clone();
    let mut warnings = Vec::new();
    // The GUI shows no translation warnings: the session reports them.
    let _ = legacy::translate_native(&mut raw, &mut warnings);
    let list = |key: &str| {
        raw.get(key)
            .cloned()
            .and_then(|v| serde_yaml_ng::from_value::<Vec<MagpieEffectSpec>>(v).ok())
            .unwrap_or_default()
    };
    Some(ChainLists {
        pre: list(PRE_CHAIN_KEY),
        post: list(CHAIN_KEY),
    })
}

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

/// Item `index` of one list (`from_pre`: the list before the model) moved
/// to position `to` of the other one (past its end: appended). Returns the
/// new `(pre, post)`; an `index` out of range changes nothing.
pub(crate) fn move_between_at<T: Clone>(
    pre: &[T],
    post: &[T],
    from_pre: bool,
    index: usize,
    to: usize,
) -> (Vec<T>, Vec<T>) {
    let (mut src, mut dst) = if from_pre {
        (pre.to_vec(), post.to_vec())
    } else {
        (post.to_vec(), pre.to_vec())
    };
    if index < src.len() {
        let item = src.remove(index);
        dst.insert(to.min(dst.len()), item);
    }
    if from_pre { (src, dst) } else { (dst, src) }
}

/// Item `index` of one list moved to the end of the other one (the card's
/// «Mover antes/después del modelo»). Returns the new `(pre, post)`.
pub(crate) fn move_between<T: Clone>(
    pre: &[T],
    post: &[T],
    from_pre: bool,
    index: usize,
) -> (Vec<T>, Vec<T>) {
    move_between_at(pre, post, from_pre, index, usize::MAX)
}

/// A position in the chain: a list and an index in it ([`END`]: after its
/// last item).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Slot {
    /// In `magpie_effects_before_model`.
    pub before_model: bool,
    pub index: usize,
}

/// The index of a [`Slot`] after the last item (the segment's title takes
/// drops there).
pub(crate) const END: usize = usize::MAX;

/// One edit of the chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ChainEdit {
    /// Reorder inside one list: the item at `from` goes to index `to`.
    Move { from: Slot, to: usize },
    /// Replace (`Some`) or remove (`None`) the item at `at`.
    Change { at: Slot, item: Option<ChainItem> },
    /// The item at the slot goes to the end of the other list.
    Swap(Slot),
    /// A dragged card dropped on `to` (the same list: it takes that index;
    /// the other list: it goes before that card, or last for [`END`]).
    Drop { from: Slot, to: Slot },
}

/// The lists an edit writes, `(before_model, items)`, the one that loses an
/// item first; lists that do not change are left out.
pub(crate) fn apply_edit(
    pre: &[ChainItem],
    post: &[ChainItem],
    edit: ChainEdit,
) -> Vec<(bool, Vec<ChainItem>)> {
    let list = |before_model: bool| if before_model { pre } else { post };
    let across = |from: Slot, (new_pre, new_post): (Vec<ChainItem>, Vec<ChainItem>)| {
        let (src, dst) = if from.before_model {
            (new_pre, new_post)
        } else {
            (new_post, new_pre)
        };
        vec![(from.before_model, src), (!from.before_model, dst)]
    };
    let writes = match edit {
        ChainEdit::Move { from, to } => {
            vec![(from.before_model, moved(list(from.before_model), from.index, to))]
        }
        ChainEdit::Change { at, item } => {
            let mut next = list(at.before_model).to_vec();
            if at.index < next.len() {
                match item {
                    Some(item) => next[at.index] = item,
                    None => {
                        next.remove(at.index);
                    }
                }
            }
            vec![(at.before_model, next)]
        }
        ChainEdit::Swap(from) => across(from, move_between(pre, post, from.before_model, from.index)),
        ChainEdit::Drop { from, to } if from.before_model == to.before_model => {
            let items = list(from.before_model);
            let to = to.index.min(items.len().saturating_sub(1));
            vec![(from.before_model, moved(items, from.index, to))]
        }
        ChainEdit::Drop { from, to } => across(
            from,
            move_between_at(pre, post, from.before_model, from.index, to.index),
        ),
    };
    writes
        .into_iter()
        .filter(|(before_model, items)| items.as_slice() != list(*before_model))
        .collect()
}

/// The catalog entry of a chain item, as the session's catalog resolves it
/// (`Catalog::find`, without the path step): the exact catalog name, else
/// the last part of the name as a file stem (the person's effects first),
/// so a 1.x spelling like `LumaSharpen` finds `Sharpen/LumaSharpen`. A
/// `.hlsl` path has no entry.
pub(crate) fn find_effect<'a>(effects: &'a [EffectInfo], name: &str) -> Option<&'a EffectInfo> {
    if let Some(info) = effects.iter().find(|e| e.name == name) {
        return Some(info);
    }
    if name.ends_with(".hlsl") {
        return None;
    }
    let base = name.rsplit('/').next().unwrap_or(name);
    let by_stem = |origin: EffectOrigin| {
        effects
            .iter()
            .find(|e| e.origin == origin && e.name.rsplit('/').next() == Some(base))
    };
    by_stem(EffectOrigin::User).or_else(|| by_stem(EffectOrigin::Builtin))
}

/// The chain's names, for costs and the bench.
pub(crate) fn chain_names(specs: &[MagpieEffectSpec]) -> Vec<String> {
    specs.iter().map(|s| ChainItem::parse(s).name).collect()
}

/// The chain editor of mode `id`: «Antes del modelo», the model, «Después
/// del modelo». With `read_only` (a mode with native-engine keys) it shows
/// those lists and offers no edits.
#[component]
pub(crate) fn EffectChain(id: ModeId, costs: Costs, read_only: Option<ChainLists>) -> Element {
    let dispatch = use_dispatch();
    let scope = Scope::Mode(id.clone());
    let pre_specs = use_value::<Vec<MagpieEffectSpec>>(scope.clone(), PRE_CHAIN_KEY.into());
    let post_specs = use_value::<Vec<MagpieEffectSpec>>(scope.clone(), CHAIN_KEY.into());
    let model = use_value::<Model>(scope.clone(), "model".into());
    let double = use_value::<bool>(scope.clone(), "double_upscale".into());
    // The card being dragged and the slot under the pointer.
    let dragging = use_signal(|| None::<Slot>);
    let over = use_signal(|| None::<Slot>);

    let locked = read_only.is_some();
    let lists = read_only.unwrap_or_else(|| ChainLists {
        pre: pre_specs(),
        post: post_specs(),
    });
    let pre: Vec<ChainItem> = lists.pre.iter().map(ChainItem::parse).collect();
    let post: Vec<ChainItem> = lists.post.iter().map(ChainItem::parse).collect();
    let (model, double) = (model(), double());
    let factor = post_factor(model, double);
    let model_text = if model == Model::None {
        t!("modes-chain-no-model")
    } else {
        t!(
            "modes-chain-model",
            model = model.as_str().to_owned(),
            scale = model_scale(model, double)
        )
    };

    let on_edit = {
        let (pre, post) = (pre.clone(), post.clone());
        move |edit: ChainEdit| {
            for (before_model, items) in apply_edit(&pre, &post, edit) {
                let specs: Vec<MagpieEffectSpec> = items.into_iter().map(|i| i.spec).collect();
                dispatch(Action::SetField {
                    scope: scope.clone(),
                    key: chain_key(before_model).into(),
                    value: yaml(specs),
                });
            }
        }
    };
    rsx! {
        div { class: "flex flex-col gap-3", role: "group", "aria-label": t!("modes-chain-label"),
            ChainSegment {
                mode: id.clone(),
                before_model: true,
                items: pre,
                costs: costs.clone(),
                factor: 1.0,
                read_only: locked,
                dragging,
                over,
                on_edit: on_edit.clone(),
            }
            div { class: "flex items-center gap-3 px-5 py-2 rounded-lg-inc bg-surface-container-high t-bm",
                Icon { name: "neurology", size: 20 }
                span { "{model_text}" }
            }
            ChainSegment {
                mode: id.clone(),
                before_model: false,
                items: post,
                costs,
                factor,
                read_only: locked,
                dragging,
                over,
                on_edit,
            }
        }
    }
}

/// One segment of the chain: its title and help, its cards (or its empty
/// state) and its «Agregar efecto».
#[component]
fn ChainSegment(
    mode: ModeId,
    before_model: bool,
    items: Vec<ChainItem>,
    costs: Costs,
    /// Times the measured cost of an effect (the model's area after it).
    factor: f32,
    read_only: bool,
    dragging: Signal<Option<Slot>>,
    over: Signal<Option<Slot>>,
    on_edit: EventHandler<ChainEdit>,
) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let (mut dragging, mut over) = (dragging, over);
    let catalog_loaded = !store.read().effects.is_empty();
    let (title, help, empty) = if before_model {
        (
            t!("modes-chain-before"),
            t!("modes-chain-before-help"),
            t!("modes-chain-before-empty"),
        )
    } else {
        (
            t!("modes-chain-after"),
            t!("modes-chain-after-help"),
            t!("modes-chain-empty"),
        )
    };
    let end = Slot {
        before_model,
        index: END,
    };
    // The title (and the empty state) take drops too: last in this list.
    // Only the classes of the committed tailwind.css can be used here.
    let end_mark = if over() == Some(end) && dragging().is_some_and(|d| d.before_model != before_model) {
        "shadow-[inset_0_3px_0_var(--primary)]"
    } else {
        ""
    };
    let drop_end = move |e: DragEvent| {
        e.prevent_default();
        if let Some(from) = dragging() {
            on_edit.call(ChainEdit::Drop { from, to: end });
        }
        dragging.set(None);
        over.set(None);
    };
    let count = items.len();
    rsx! {
        div { class: "flex flex-col gap-2",
            div {
                class: "flex flex-col px-1 pb-1 {end_mark}",
                ondragover: move |e: DragEvent| {
                    e.prevent_default();
                    over.set(Some(end));
                },
                ondrop: drop_end,
                span { class: "t-ls text-on-surface", "{title}" }
                span { class: "t-bs text-on-surface-variant", "{help}" }
            }
            div { class: "flex flex-col gap-0.5", role: "list", "aria-label": "{title}",
                if items.is_empty() {
                    div {
                        class: "flex items-center gap-3 min-h-16 px-5 rounded-lg-inc bg-surface-container t-bm text-on-surface-variant {end_mark}",
                        ondragover: move |e: DragEvent| {
                            e.prevent_default();
                            over.set(Some(end));
                        },
                        ondrop: drop_end,
                        "{empty}"
                    }
                }
                for (index, item) in items.into_iter().enumerate() {
                    EffectCard {
                        key: "{index}:{item.name}",
                        slot: Slot { before_model, index },
                        count,
                        info: find_effect(&store.read().effects, &item.name).cloned(),
                        compile: store.read().compiling.get(&item.name).cloned(),
                        cost: costs.magpie(&item.name).map(|ms| ms * factor),
                        catalog_loaded,
                        item,
                        read_only,
                        lifted: dragging() == Some(Slot { before_model, index }),
                        drop_mark: over() == Some(Slot { before_model, index })
                            && dragging().is_some_and(|d| d != Slot { before_model, index }),
                        on_drag: move |state: DragState| {
                            let slot = Slot { before_model, index };
                            match state {
                                DragState::Start => dragging.set(Some(slot)),
                                DragState::Over => over.set(Some(slot)),
                                DragState::Drop => {
                                    if let Some(from) = dragging() {
                                        on_edit.call(ChainEdit::Drop { from, to: slot });
                                    }
                                    dragging.set(None);
                                    over.set(None);
                                }
                                DragState::End => {
                                    dragging.set(None);
                                    over.set(None);
                                }
                            }
                        },
                        on_edit,
                    }
                }
            }
            if !read_only {
                div { class: "flex flex-wrap items-center gap-2",
                    Button {
                        label: t!("modes-add-effect"),
                        variant: ButtonVariant::Outlined,
                        icon: "add",
                        onclick: move |_| dispatch(Action::OpenDialog(Dialog::EffectPicker { mode: mode.clone(), before_model })),
                    }
                }
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
    /// Something was dropped on this card.
    Drop,
    /// The drag finished (dropped elsewhere or cancelled).
    End,
}

/// One effect of the chain.
#[component]
fn EffectCard(
    /// Its list and index.
    slot: Slot,
    /// Items in its list.
    count: usize,
    item: ChainItem,
    /// The catalog entry (`None`: not loaded yet, or not in the catalog).
    info: Option<EffectInfo>,
    compile: Option<CompileState>,
    cost: Option<f32>,
    catalog_loaded: bool,
    /// No handle, no buttons: a mode with native-engine keys.
    read_only: bool,
    /// This card is being dragged.
    lifted: bool,
    /// Another card is dragged over this one.
    drop_mark: bool,
    on_drag: EventHandler<DragState>,
    on_edit: EventHandler<ChainEdit>,
) -> Element {
    // Only the handle starts a drag, so sliders inside keep working.
    let mut grab = use_signal(|| false);
    let mut open = use_signal(|| false);
    let index = slot.index;
    let name = item.display_name();
    let params = info.as_ref().map(|i| i.params.clone()).unwrap_or_default();
    let has_params = !params.is_empty() && !read_only;
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
        if !info.params.is_empty() {
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
    let drop_mark = if drop_mark {
        "shadow-[inset_0_3px_0_var(--primary)]"
    } else {
        ""
    };
    let lifted = if lifted { "opacity-60" } else { "" };
    let up = index.checked_sub(1);
    let down = (index + 1 < count).then_some(index + 1);
    let swap_label = if slot.before_model {
        t!("modes-effect-to-after")
    } else {
        t!("modes-effect-to-before")
    };
    let item_for_params = item.clone();
    let item_for_reset = item.clone();

    rsx! {
        div {
            class: "flex flex-col gap-2 px-4 py-3 min-h-16 rounded-xs first:rounded-t-lg-inc last:rounded-b-lg-inc {tone} {drop_mark} {lifted}",
            role: "listitem",
            draggable: if grab() && !read_only { "true" } else { "false" },
            ondragstart: move |_| on_drag.call(DragState::Start),
            ondragover: move |e: DragEvent| {
                e.prevent_default();
                on_drag.call(DragState::Over);
            },
            ondrop: move |e: DragEvent| {
                e.prevent_default();
                on_drag.call(DragState::Drop);
                grab.set(false);
            },
            ondragend: move |_| {
                on_drag.call(DragState::End);
                grab.set(false);
            },
            div { class: "flex items-center gap-3",
                if !read_only {
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
                                on_edit.call(ChainEdit::Move { from: slot, to });
                            }
                        },
                        IconButton { icon: "drag_indicator", label: t!("modes-effect-move", name = name.clone()), size: 32 }
                    }
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
                if !read_only {
                    IconButton {
                        icon: "swap_vert",
                        label: swap_label,
                        size: 32,
                        onclick: move |_| on_edit.call(ChainEdit::Swap(slot)),
                    }
                    IconButton {
                        icon: "arrow_upward",
                        label: t!("modes-effect-up"),
                        size: 32,
                        disabled: up.is_none(),
                        onclick: move |_| if let Some(to) = up { on_edit.call(ChainEdit::Move { from: slot, to }) },
                    }
                    IconButton {
                        icon: "arrow_downward",
                        label: t!("modes-effect-down"),
                        size: 32,
                        disabled: down.is_none(),
                        onclick: move |_| if let Some(to) = down { on_edit.call(ChainEdit::Move { from: slot, to }) },
                    }
                    IconButton {
                        icon: "close",
                        label: t!("modes-effect-remove", name = name.clone()),
                        size: 32,
                        onclick: move |_| on_edit.call(ChainEdit::Change { at: slot, item: None }),
                    }
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
                                move |v: f64| on_edit.call(ChainEdit::Change {
                                    at: slot,
                                    item: Some(item.with_param(&param.name, v, param.default)),
                                })
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
                                onclick: move |_| on_edit.call(ChainEdit::Change { at: slot, item: Some(item_for_reset.reset()) }),
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
    fn moving_between_the_lists() {
        let (pre, post) = (["a", "b"], ["x", "y", "z"]);
        // The card's button: to the end of the other list.
        assert_eq!(move_between(&pre, &post, true, 0), (vec!["b"], vec!["x", "y", "z", "a"]));
        assert_eq!(move_between(&pre, &post, false, 1), (vec!["a", "b", "y"], vec!["x", "z"]));
        // A drop on a card of the other list: before that card.
        assert_eq!(move_between_at(&pre, &post, true, 1, 0), (vec!["a"], vec!["b", "x", "y", "z"]));
        assert_eq!(move_between_at(&pre, &post, false, 2, 1), (vec!["a", "z", "b"], vec!["x", "y"]));
        // Out of range: nothing moves.
        assert_eq!(move_between(&pre, &post, true, 5), (pre.to_vec(), post.to_vec()));
        let empty: [&str; 0] = [];
        assert_eq!(move_between(&empty, &post, false, 0), (vec!["x"], vec!["y", "z"]));
    }

    fn items(names: &[&str]) -> Vec<ChainItem> {
        names.iter().map(|n| ChainItem::new(n)).collect()
    }

    fn names(items: &[ChainItem]) -> Vec<&str> {
        items.iter().map(|i| i.name.as_str()).collect()
    }

    #[test]
    fn edits_write_the_lists_that_change_removal_first() {
        let (pre, post) = (items(&["A"]), items(&["X", "Y"]));
        let at = |before_model, index| Slot { before_model, index };

        let writes = apply_edit(&pre, &post, ChainEdit::Swap(at(false, 1)));
        let got: Vec<(bool, Vec<&str>)> = writes.iter().map(|(b, i)| (*b, names(i))).collect();
        assert_eq!(got, vec![(false, vec!["X"]), (true, vec!["A", "Y"])]);

        let writes = apply_edit(&pre, &post, ChainEdit::Drop { from: at(true, 0), to: at(false, 1) });
        let got: Vec<(bool, Vec<&str>)> = writes.iter().map(|(b, i)| (*b, names(i))).collect();
        assert_eq!(got, vec![(true, vec![]), (false, vec!["X", "A", "Y"])]);

        // Dropped on the other segment's title: last.
        let writes = apply_edit(&pre, &post, ChainEdit::Drop { from: at(true, 0), to: at(false, END) });
        assert_eq!(names(&writes[1].1), vec!["X", "Y", "A"]);

        // Inside one list: only that list.
        let writes = apply_edit(&pre, &post, ChainEdit::Drop { from: at(false, 0), to: at(false, END) });
        let got: Vec<(bool, Vec<&str>)> = writes.iter().map(|(b, i)| (*b, names(i))).collect();
        assert_eq!(got, vec![(false, vec!["Y", "X"])]);
        let writes = apply_edit(&pre, &post, ChainEdit::Move { from: at(false, 1), to: 0 });
        assert_eq!(names(&writes[0].1), vec!["Y", "X"]);

        // Remove and replace.
        let writes = apply_edit(&pre, &post, ChainEdit::Change { at: at(true, 0), item: None });
        assert_eq!(writes, vec![(true, Vec::new())]);
        let edited = post[0].with_param("p", 2.0, 1.0);
        let change = ChainEdit::Change {
            at: at(false, 0),
            item: Some(edited.clone()),
        };
        let writes = apply_edit(&pre, &post, change);
        assert_eq!(writes, vec![(false, vec![edited, post[1].clone()])]);

        // Nothing changes, nothing is written.
        assert!(apply_edit(&pre, &post, ChainEdit::Drop { from: at(true, 0), to: at(true, 0) }).is_empty());
        assert!(apply_edit(&pre, &post, ChainEdit::Move { from: at(false, 0), to: 0 }).is_empty());
        assert!(apply_edit(&pre, &post, ChainEdit::Swap(at(true, 3))).is_empty());
    }

    #[test]
    fn effects_are_found_like_the_session_finds_them() {
        let info = |name: &str, origin| EffectInfo {
            name: name.into(),
            sort_name: name.into(),
            passes: 1,
            params: Vec::new(),
            verified: true,
            origin,
            fp16: false,
            error: None,
        };
        let effects = vec![
            info("Sharpen/LumaSharpen", EffectOrigin::Builtin),
            info("CRT/CRT_Easymode", EffectOrigin::Builtin),
            info("Mine/CRT_Easymode", EffectOrigin::User),
        ];
        let found = |name: &str| find_effect(&effects, name).map(|e| e.name.as_str());
        assert_eq!(found("Sharpen/LumaSharpen"), Some("Sharpen/LumaSharpen"));
        assert_eq!(found("LumaSharpen"), Some("Sharpen/LumaSharpen"));
        assert_eq!(found("CRT/CRT_Easymode"), Some("CRT/CRT_Easymode"));
        // By stem, the person's own effect first.
        assert_eq!(found("CRT_Easymode"), Some("Mine/CRT_Easymode"));
        assert_eq!(found("/home/me/LumaSharpen.hlsl"), None);
        assert_eq!(found("Nope"), None);
    }

    fn mapping(text: &str) -> Mapping {
        serde_yaml_ng::from_str(text).unwrap()
    }

    #[test]
    fn modes_with_native_keys_show_the_translated_chain() {
        let plain = mapping("name: A\nmagpie_effects: [X]\n");
        assert_eq!(translated_lists(&plain), None);
        let given = [MagpieEffectSpec::Short("X".into())];
        assert_eq!(ChainLists::of(Some(&plain), &[], &given).post, given.to_vec());
        assert_eq!(ChainLists::of(None, &given, &[]).pre, given.to_vec());

        // The native engine (the default): its effects become the chain
        // after the model, the inactive list goes, the list before stays.
        let native = mapping(
            "name: CRT\ncrt_enabled: true\nmagpie_effects: [X]\nmagpie_effects_before_model: [Y]\n",
        );
        let lists = translated_lists(&native).unwrap();
        assert_eq!(chain_names(&lists.post), vec![legacy::CRT_EFFECT]);
        assert_eq!(chain_names(&lists.pre), vec!["Y"]);
        assert_eq!(ChainLists::of(Some(&native), &[], &given), lists);

        // `effects_engine: magpiefx` keeps the chain.
        let magpie = mapping("effects_engine: magpiefx\nmagpie_effects: [X]\n");
        let lists = translated_lists(&magpie).unwrap();
        assert_eq!(chain_names(&lists.post), vec!["X"]);
        assert!(lists.pre.is_empty());
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
