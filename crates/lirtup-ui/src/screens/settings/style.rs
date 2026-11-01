//! Estilo: zoom, font scale, font family, the window grid (columns and tile
//! shape) and the refresh intervals of the list and the previews.

use dioxus::prelude::*;
use lirtup_config::gui::TileAspect;

use super::{PlainRow, SectionTitle, use_gui, write_gui};
use crate::app::{use_dispatch, use_store};
use crate::components::{Connected, FontField, SettingGroup, Slider};

/// `fc-list : family`, one family per line (a font with several names
/// lists them comma separated: the first is kept), sorted, without
/// repeats.
pub(crate) fn families(fc_list: &str) -> Vec<String> {
    let mut out: Vec<String> = fc_list
        .lines()
        .filter_map(|line| line.split(',').next())
        .map(|name| name.trim().replace("\\-", "-"))
        .filter(|name| !name.is_empty())
        .collect();
    out.sort_by_key(|n| n.to_lowercase());
    out.dedup();
    out
}

fn read_families() -> Vec<String> {
    std::process::Command::new("fc-list")
        .args([":", "family"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| families(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// The Estilo tab.
#[component]
pub(crate) fn Style() -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let zoom = use_gui(|g| g.zoom as f64);
    let font_scale = use_gui(|g| g.font_scale as f64);
    let font = use_gui(|g| g.font_family.clone());
    let columns = use_gui(|g| g.tile_columns as f64);
    let aspect = use_gui(|g| g.tile_aspect);
    let refresh = use_gui(|g| g.auto_refresh_ms as f64);
    let preview = use_gui(|g| g.tile_preview_interval_ms as f64);
    // The font list, off the UI thread, once.
    let fonts = use_resource(|| async {
        tokio::task::spawn_blocking(read_families)
            .await
            .unwrap_or_default()
    });
    let percent = Callback::new(|v: f64| format!("{v:.0} %"));
    let ms = Callback::new(|v: f64| format!("{v:.0} ms"));
    let columns_text = Callback::new(|v: f64| {
        if v < 0.5 {
            t!("settings-columns-auto")
        } else {
            format!("{v:.0}")
        }
    });
    let interval_note = t!(
        "settings-intervals-note",
        list = refresh() as u32,
        preview = preview() as u32
    );
    rsx! {
        SectionTitle { title: t!("settings-tab-style") }
        SettingGroup {
            PlainRow { icon: "zoom_in", title: t!("settings-zoom"), subtitle: t!("settings-zoom-help"),
                div { class: "w-72",
                    Slider {
                        value: zoom,
                        min: 50.0,
                        max: 250.0,
                        step: 10.0,
                        label: t!("settings-zoom"),
                        format: percent,
                        on_change: move |v: f64| write_gui(store, dispatch, |g| g.zoom = v.round() as u32),
                    }
                }
            }
            PlainRow { icon: "format_size", title: t!("settings-font-scale"), subtitle: t!("settings-font-scale-help"),
                div { class: "w-72",
                    Slider {
                        value: font_scale,
                        min: 80.0,
                        max: 150.0,
                        step: 5.0,
                        label: t!("settings-font-scale"),
                        format: percent,
                        on_change: move |v: f64| write_gui(store, dispatch, |g| g.font_scale = v.round() as u32),
                    }
                }
            }
            PlainRow { icon: "font_download", title: t!("settings-font"), subtitle: t!("settings-font-help"),
                FontField {
                    value: font,
                    label: t!("settings-font"),
                    families: fonts.read().clone().unwrap_or_default(),
                    on_change: move |f: String| write_gui(store, dispatch, |g| g.font_family = f),
                }
            }
        }

        SectionTitle { title: t!("settings-grid") }
        SettingGroup {
            PlainRow { icon: "grid_view", title: t!("settings-aspect"), subtitle: t!("settings-aspect-help"),
                Connected {
                    options: vec![
                        (TileAspect::FourThree, "4:3".to_owned()),
                        (TileAspect::SixteenNine, "16:9".to_owned()),
                        (TileAspect::Auto, t!("settings-aspect-auto")),
                    ],
                    value: aspect,
                    label: t!("settings-aspect"),
                    on_change: move |a: TileAspect| write_gui(store, dispatch, |g| g.tile_aspect = a),
                }
            }
            PlainRow { icon: "view_column", title: t!("settings-columns"), subtitle: t!("settings-columns-help"),
                div { class: "w-72",
                    Slider {
                        value: columns,
                        min: 0.0,
                        max: 8.0,
                        step: 1.0,
                        label: t!("settings-columns"),
                        format: columns_text,
                        on_change: move |v: f64| write_gui(store, dispatch, |g| g.tile_columns = v.round() as u32),
                    }
                }
            }
        }

        SettingGroup { title: t!("settings-intervals"), collapsible: true, collapsed: true,
            PlainRow { icon: "update", title: t!("settings-refresh"), subtitle: interval_note,
                div { class: "w-72",
                    Slider {
                        value: refresh,
                        min: 250.0,
                        max: 5000.0,
                        step: 250.0,
                        label: t!("settings-refresh"),
                        format: ms,
                        on_change: move |v: f64| write_gui(store, dispatch, |g| g.auto_refresh_ms = v.round() as u32),
                    }
                }
            }
            PlainRow { icon: "slideshow", title: t!("settings-preview"), subtitle: t!("settings-preview-help"),
                div { class: "w-72",
                    Slider {
                        value: preview,
                        min: 50.0,
                        max: 1000.0,
                        step: 50.0,
                        label: t!("settings-preview"),
                        format: ms,
                        on_change: move |v: f64| write_gui(store, dispatch, |g| g.tile_preview_interval_ms = v.round() as u32),
                    }
                }
            }
        }
        p { class: "t-bs text-on-surface-variant mt-2 mx-1", {t!("settings-drawer-note")} }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_families_from_fc_list() {
        let out =
            "DejaVu Sans,DejaVu Sans Condensed\nNoto Sans\n\nnoto sans\nDejaVu Sans\nJetBrains Mono\\-Bold\n";
        assert_eq!(
            families(out),
            vec!["DejaVu Sans", "JetBrains Mono-Bold", "Noto Sans", "noto sans"]
        );
    }
}
