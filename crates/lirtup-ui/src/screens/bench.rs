//! «Medir en mi GPU» (front D): `Command::Bench`, progress from
//! `BenchProgress`, results from `BenchResult`, Cancelar (`CancelBench`).
//!
//! Three steps in one dialog: what to measure (sizes, frame generation),
//! the progress, and the result per pass with the mode's total. The core
//! reads the mode from disk, so unsaved edits of the mode are not measured
//! (the dialog says so). The core also saves the result in `bench.json`,
//! where the mode editor reads the cost of each pass.

use dioxus::prelude::*;
use lirtup_proto::{BenchEntry, BenchKind, BenchReport, BenchRequest, Size};

use crate::app::{use_dispatch, use_store};
use crate::components::{Button, ButtonVariant, Connected, Dialog as DialogBox, Icon, Switch};
use crate::route::ModeId;
use crate::screens::modes::widgets::{fmt_ms, fmt_num};
use crate::state::{Action, Scope};

/// Source → output sizes offered (1.x `bench.py` measures 720p → 1440p).
pub(crate) const SIZES: [(Size, Size); 3] = [
    (size(960, 540), size(1920, 1080)),
    (size(1280, 720), size(2560, 1440)),
    (size(1920, 1080), size(3840, 2160)),
];

const fn size(width: u32, height: u32) -> Size {
    Size { width, height }
}

/// «720p → 1440p».
pub(crate) fn sizes_label(src: Size, dst: Size) -> String {
    format!("{}p → {}p", src.height, dst.height)
}

/// What the dialog asks the core for.
pub(crate) fn request(mode: Option<ModeId>, sizes: usize, framegen: bool) -> BenchRequest {
    let (src, dst) = SIZES[sizes.min(SIZES.len() - 1)];
    BenchRequest {
        src,
        dst,
        mode,
        effects: Vec::new(),
        framegen,
        // 0: the bench's own budget per case.
        frames: 0,
    }
}

/// The mode's cost in a report: its native and MagpieFX passes that
/// finished (frame generation is not part of a mode).
pub(crate) fn report_total(report: &BenchReport) -> Option<f32> {
    let passes: Vec<f32> = report
        .entries
        .iter()
        .filter(|e| e.kind != BenchKind::Framegen && e.error.is_none())
        .filter_map(|e| e.ms)
        .collect();
    (!passes.is_empty()).then(|| passes.iter().sum())
}

/// The dialog (`Dialog::Bench`).
#[component]
pub fn BenchDialog(mode: Option<ModeId>) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let open = use_signal(|| true);
    let sizes = use_signal(|| 1usize);
    let framegen = use_signal(|| false);
    // A measurement already running when the dialog opens is shown, not
    // asked again.
    let mut started = use_signal(|| {
        let bench = &store.peek().bench;
        bench.request.is_some() && bench.report.is_none()
    });
    use_hook(move || {
        if store.peek().gpus.is_empty() {
            dispatch(Action::ListDevices);
        }
    });

    let s = store.read();
    let bench = s.bench.clone();
    let mode_name = mode.as_ref().map(|id| {
        s.modes
            .store
            .get(id)
            .map_or_else(|| id.clone(), |m| m.name.clone())
    });
    let unsaved = mode
        .as_ref()
        .is_some_and(|id| s.dirty_count(&Scope::Mode(id.clone())) > 0);
    let gpu = s
        .gpus
        .iter()
        .find(|g| g.selected)
        .or(s.gpus.first())
        .map(|g| g.name.clone());
    drop(s);

    let running = started() && bench.report.is_none();
    let done = started() && bench.report.is_some();
    let what = match &mode_name {
        Some(name) => t!("bench-what-mode", name = name.clone()),
        None => t!("bench-what-native"),
    };
    let close = move || {
        if running {
            dispatch(Action::CancelBench);
        }
        dispatch(Action::CloseDialog);
    };
    let run_mode = mode.clone();
    let start = move |_| {
        dispatch(Action::RunBench(request(run_mode.clone(), sizes(), framegen())));
        started.set(true);
    };
    let again = move |_| started.set(false);

    let actions = if running {
        rsx! {
            Button { label: t!("bench-cancel"), variant: ButtonVariant::Outlined, onclick: move |_| dispatch(Action::CancelBench) }
        }
    } else if done {
        rsx! {
            Button { label: t!("bench-again"), variant: ButtonVariant::Text, icon: "replay", onclick: again }
            Button { label: t!("bench-done"), onclick: move |_| close() }
        }
    } else {
        rsx! {
            Button { label: t!("bench-cancel"), variant: ButtonVariant::Outlined, onclick: move |_| close() }
            Button { label: t!("bench-start"), icon: "speed", onclick: start }
        }
    };

    rsx! {
        DialogBox {
            open,
            icon: "speed",
            title: t!("bench-title"),
            on_close: move |_| close(),
            actions,
            div { class: "flex flex-col gap-4 min-w-0",
                p { class: "t-bm m-0", "{what}" }
                if let Some(gpu) = &gpu {
                    p { class: "t-bs m-0 text-on-surface-variant flex items-center gap-2",
                        Icon { name: "memory", size: 18 }
                        "{gpu}"
                    }
                }
                if running {
                    Progress { done: bench.done, total: bench.total }
                } else if let (true, Some(report)) = (done, &bench.report) {
                    Results { report: report.clone(), request: bench.request.clone() }
                } else {
                    Setup { sizes, framegen }
                    if unsaved {
                        p { class: "t-bs m-0 px-4 py-3 rounded-md bg-tertiary-container text-on-tertiary-container",
                            {t!("bench-unsaved")}
                        }
                    }
                }
            }
        }
    }
}

/// Sizes and frame generation.
#[component]
fn Setup(sizes: Signal<usize>, framegen: Signal<bool>) -> Element {
    let options: Vec<(usize, String)> = SIZES
        .iter()
        .enumerate()
        .map(|(i, (src, dst))| (i, sizes_label(*src, *dst)))
        .collect();
    rsx! {
        div { class: "flex flex-col gap-2",
            span { class: "t-lm text-on-surface-variant", {t!("bench-sizes")} }
            Connected { options, value: sizes, label: t!("bench-sizes") }
        }
        label { class: "flex items-center gap-3",
            div { class: "flex-1 flex flex-col",
                span { class: "t-bl", {t!("bench-framegen")} }
                span { class: "t-bs text-on-surface-variant", {t!("bench-framegen-help")} }
            }
            Switch { value: framegen, label: t!("bench-framegen") }
        }
    }
}

/// «Midiendo 3 de 8…» and a bar.
#[component]
fn Progress(done: u32, total: u32) -> Element {
    let percent = if total > 0 {
        (done as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let text = if total > 0 {
        t!("bench-progress", done = done, total = total)
    } else {
        t!("bench-preparing")
    };
    rsx! {
        div { class: "flex flex-col gap-2", role: "status", "aria-live": "polite",
            span { class: "t-bm", "{text}" }
            div {
                class: "h-1 rounded-full bg-surface-container-highest overflow-hidden",
                role: "progressbar",
                "aria-valuemin": "0",
                "aria-valuemax": "{total}",
                "aria-valuenow": "{done}",
                "aria-label": t!("bench-title"),
                div { class: "h-full rounded-full bg-primary transition-[width]", style: "width: {percent}%" }
            }
            span { class: "t-bs text-on-surface-variant", {t!("bench-keep-open")} }
        }
    }
}

/// The table of passes and the total.
#[component]
fn Results(report: BenchReport, request: Option<BenchRequest>) -> Element {
    let total = report_total(&report);
    let sizes = request.as_ref().map(|r| sizes_label(r.src, r.dst));
    rsx! {
        if report.cancelled {
            p { class: "t-bs m-0 px-4 py-3 rounded-md bg-tertiary-container text-on-tertiary-container", {t!("bench-cancelled")} }
        }
        if report.entries.is_empty() {
            p { class: "t-bm m-0 text-on-surface-variant", {t!("bench-nothing")} }
        } else {
            div { class: "max-h-[45vh] overflow-auto rounded-lg bg-surface-container",
                table { class: "w-full t-bm border-collapse",
                    thead {
                        tr { class: "text-left text-on-surface-variant t-lm",
                            th { class: "px-4 py-2 font-semibold", {t!("bench-col-pass")} }
                            th { class: "px-4 py-2 font-semibold text-right", {t!("bench-col-median")} }
                            th { class: "px-4 py-2 font-semibold text-right", {t!("bench-col-p95")} }
                        }
                    }
                    tbody {
                        for (i, entry) in report.entries.iter().enumerate() {
                            ResultRow { key: "{i}", entry: entry.clone() }
                        }
                    }
                }
            }
        }
        if let Some(total) = total {
            p { class: "t-ts m-0 mono",
                {t!("bench-total", ms = fmt_ms(total))}
                if let Some(sizes) = sizes {
                    span { class: "t-bs text-on-surface-variant", " · {sizes}" }
                }
            }
        }
    }
}

#[component]
fn ResultRow(entry: BenchEntry) -> Element {
    let name = match entry.kind {
        BenchKind::Native => {
            let key = format!("modes-pass-{}", entry.name);
            if crate::i18n::has_key(crate::i18n::Lang::Es, &key) {
                crate::i18n::tr(&key)
            } else {
                entry.name.clone()
            }
        }
        BenchKind::Magpiefx => entry.name.clone(),
        BenchKind::Framegen => t!("bench-framegen-case", case = entry.name.clone()),
    };
    let ms = |v: Option<f32>| v.map_or_else(|| "—".to_owned(), |v| fmt_num(v as f64, 2));
    rsx! {
        tr { class: "border-t border-outline-variant",
            td { class: "px-4 py-2",
                div { class: "flex flex-col",
                    span { "{name}" }
                    if let Some(error) = &entry.error {
                        span { class: "t-bs text-error break-words", "{error}" }
                    }
                }
            }
            td { class: "px-4 py-2 text-right mono", {ms(entry.ms)} }
            td { class: "px-4 py-2 text-right mono", {ms(entry.p95)} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: BenchKind, ms: Option<f32>, error: Option<&str>) -> BenchEntry {
        BenchEntry {
            kind,
            name: "x".into(),
            ms,
            p95: ms,
            frames: 10,
            error: error.map(str::to_owned),
        }
    }

    #[test]
    fn the_total_is_the_mode_passes() {
        let report = BenchReport {
            gpu: "g".into(),
            entries: vec![
                entry(BenchKind::Native, Some(0.5), None),
                entry(BenchKind::Magpiefx, Some(1.0), None),
                entry(BenchKind::Magpiefx, Some(9.0), Some("failed")),
                entry(BenchKind::Framegen, Some(3.0), None),
            ],
            cancelled: false,
        };
        assert_eq!(report_total(&report), Some(1.5));
        let empty = BenchReport {
            entries: vec![entry(BenchKind::Framegen, Some(3.0), None)],
            ..report
        };
        assert_eq!(report_total(&empty), None);
    }

    #[test]
    fn requests() {
        let r = request(Some("anime".into()), 1, true);
        assert_eq!((r.src.height, r.dst.height), (720, 1440));
        assert_eq!(r.mode.as_deref(), Some("anime"));
        assert!(r.framegen);
        assert_eq!(request(None, 99, false).dst.height, 2160);
        assert_eq!(sizes_label(size(1280, 720), size(2560, 1440)), "720p → 1440p");
    }
}
