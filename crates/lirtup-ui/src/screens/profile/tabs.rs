//! The five tabs of a profile page. Each is a list of groups of rows; the
//! keys are those of [`crate::state::store::tab_keys`] (a test checks that
//! every key of a tab has a row).

use dioxus::prelude::*;

use super::framegen::FrameGenSection;
use super::rows::{Control, GeometryRow, GpuRow, ModeRow, MonitorRow, Num, Row, RowSpec};
use crate::components::SettingGroup;
use crate::i18n::tr;
use crate::route::ProfileTab;
use crate::state::Scope;

const SECONDS: Num = Num::float(0.0, 10.0, 0.05);

const FOCUS: [RowSpec; 2] = [
    RowSpec::new(
        "follow_focus",
        "center_focus_strong",
        "profile-follow-focus",
        Control::Switch,
    )
    .sub("profile-follow-focus-sub"),
    RowSpec::new(
        "pause_on_focus_loss",
        "pause_circle",
        "profile-pause-focus",
        Control::Switch,
    )
    .sub("profile-pause-focus-sub"),
];

const AUTO: [RowSpec; 1] = [RowSpec::new(
    "daemon_exclude",
    "block",
    "profile-daemon-exclude",
    Control::Switch,
)
.sub("profile-daemon-exclude-sub")];

const OVERLAY_MODES: [(&str, &str); 4] = [
    ("always-on-top", "profile-overlay-always-on-top"),
    ("top-transparent", "profile-overlay-top-transparent"),
    ("fullscreen", "profile-overlay-fullscreen"),
    ("windowed", "profile-overlay-windowed"),
];

const WINDOW_TOP: [RowSpec; 1] = [RowSpec::new(
    "overlay_mode",
    "web_asset",
    "profile-overlay-mode",
    Control::Select(&OVERLAY_MODES),
)];

const WINDOW_BOTTOM: [RowSpec; 3] = [
    RowSpec::new(
        "auto_crop_menu_bar",
        "wine_bar",
        "profile-menu-bar",
        Control::Switch,
    )
    .sub("profile-menu-bar-sub")
    .experimental(),
    RowSpec::new(
        "hide_cursor",
        "mouse",
        "profile-hide-cursor",
        Control::Optional(Num::float(0.5, 10.0, 0.5).scaled(1000.0).starting(2.0)),
    )
    .sub("profile-hide-cursor-sub"),
    RowSpec::new(
        "background_color",
        "format_color_fill",
        "profile-background",
        Control::Color,
    )
    .sub("profile-background-sub"),
];

const CROP: Num = Num::int(0.0, 1000.0, 1.0);
const OFFSET: Num = Num::int(-2000.0, 2000.0, 1.0);

const CROP_ROWS: [RowSpec; 6] = [
    RowSpec::new(
        "crop_top",
        "border_top",
        "profile-crop-top",
        Control::Number(CROP),
    )
    .sub("profile-crop-sub"),
    RowSpec::new(
        "crop_bottom",
        "border_bottom",
        "profile-crop-bottom",
        Control::Number(CROP),
    )
    .sub("profile-crop-sub"),
    RowSpec::new(
        "crop_left",
        "border_left",
        "profile-crop-left",
        Control::Number(CROP),
    )
    .sub("profile-crop-sub"),
    RowSpec::new(
        "crop_right",
        "border_right",
        "profile-crop-right",
        Control::Number(CROP),
    )
    .sub("profile-crop-sub"),
    RowSpec::new(
        "offset_x",
        "swap_horiz",
        "profile-offset-x",
        Control::Number(OFFSET),
    )
    .sub("profile-crop-sub"),
    RowSpec::new(
        "offset_y",
        "swap_vert",
        "profile-offset-y",
        Control::Number(OFFSET),
    )
    .sub("profile-crop-sub"),
];

const PRESENT_MODES: [(&str, &str); 3] = [
    ("fifo", "profile-present-fifo"),
    ("mailbox", "profile-present-mailbox"),
    ("immediate", "profile-present-immediate"),
];

const VSYNC: [RowSpec; 3] = [
    RowSpec::new(
        "vulkan_present_mode",
        "sync",
        "profile-present-mode",
        Control::Select(&PRESENT_MODES),
    )
    .sub("profile-present-mode-sub"),
    RowSpec::new(
        "max_fps",
        "speed",
        "profile-max-fps",
        Control::Optional(Num::int(10.0, 360.0, 1.0).starting(60.0)),
    ),
    RowSpec::new(
        "present_on_change_only",
        "difference",
        "profile-present-on-change",
        Control::Switch,
    )
    .sub("profile-present-on-change-sub"),
];

const SCALE: [RowSpec; 1] = [RowSpec::new(
    "scale_factor",
    "zoom_out_map",
    "profile-scale-factor",
    Control::Optional(Num::float(0.25, 8.0, 0.05).starting(2.0)),
)
.sub("profile-scale-factor-sub")];

const SCREENSHOTS: [RowSpec; 2] = [
    RowSpec::new(
        "screenshot_dir",
        "folder",
        "profile-screenshot-dir",
        Control::Path(true),
    ),
    RowSpec::new(
        "screenshot_filename",
        "label",
        "profile-screenshot-filename",
        Control::Text,
    )
    .sub("profile-screenshot-filename-sub"),
];

const OSD: [RowSpec; 4] = [
    RowSpec::new("show_osd", "chat_bubble", "profile-show-osd", Control::Switch),
    RowSpec::new(
        "osd_duration",
        "timer",
        "profile-osd-duration",
        Control::Number(Num::float(0.5, 10.0, 0.1)),
    )
    .sub("profile-osd-duration-sub"),
    RowSpec::new("show_stats", "monitoring", "profile-show-stats", Control::Switch)
        .sub("profile-show-stats-sub")
        .experimental(),
    RowSpec::new(
        "latency_log",
        "query_stats",
        "profile-latency-log",
        Control::Path(false),
    )
    .sub("profile-latency-log-sub"),
];

const TILES: [RowSpec; 6] = [
    RowSpec::new(
        "use_tile_processing",
        "grid_view",
        "profile-tiles",
        Control::Switch,
    )
    .sub("profile-tiles-sub"),
    RowSpec::new(
        "use_damage_tracking",
        "difference",
        "profile-damage",
        Control::Switch,
    ),
    RowSpec::new(
        "tile_size",
        "crop_square",
        "profile-tile-size",
        Control::Number(Num::int(16.0, 512.0, 16.0)),
    ),
    RowSpec::new(
        "tile_context_margin",
        "padding",
        "profile-tile-margin",
        Control::Number(Num::int(0.0, 128.0, 1.0)),
    ),
    RowSpec::new(
        "max_tile_layers",
        "layers",
        "profile-tile-layers",
        Control::Number(Num::int(1.0, 256.0, 1.0)),
    ),
    RowSpec::new(
        "area_threshold",
        "percent",
        "profile-area-threshold",
        Control::Number(Num::float(0.0, 1.0, 0.05)),
    )
    .sub("profile-area-threshold-sub"),
];

const TIMING: [RowSpec; 4] = [
    RowSpec::new(
        "frame_timeout",
        "timer",
        "profile-frame-timeout",
        Control::Number(Num::int(10.0, 10_000.0, 10.0).scaled(1_000_000.0)),
    )
    .sub("profile-frame-timeout-sub"),
    RowSpec::new(
        "focus_poll_interval",
        "center_focus_weak",
        "profile-focus-poll",
        Control::Number(SECONDS),
    )
    .sub("profile-seconds"),
    RowSpec::new(
        "pipeline_poll_interval",
        "hourglass_empty",
        "profile-pipeline-poll",
        Control::Number(SECONDS),
    )
    .sub("profile-seconds"),
    RowSpec::new(
        "daemon_poll_interval",
        "radar",
        "profile-daemon-poll",
        Control::Number(Num::float(0.5, 30.0, 0.5)),
    )
    .sub("profile-seconds"),
];

const RECOVERY: [RowSpec; 3] = [
    RowSpec::new(
        "max_capture_failures",
        "healing",
        "profile-capture-failures",
        Control::Number(Num::int(1.0, 1000.0, 1.0)),
    ),
    RowSpec::new(
        "capture_failure_delay",
        "hourglass_bottom",
        "profile-capture-delay",
        Control::Number(Num::float(0.0, 5.0, 0.01)),
    )
    .sub("profile-seconds"),
    RowSpec::new(
        "swapchain_debounce",
        "autorenew",
        "profile-swapchain-debounce",
        Control::Number(Num::float(0.0, 10.0, 0.1)),
    )
    .sub("profile-seconds"),
];

/// A group of rows from a table.
#[component]
fn Group(
    scope: Scope,
    title: &'static str,
    rows: &'static [RowSpec],
    #[props(default)] collapsible: bool,
) -> Element {
    rsx! {
        SettingGroup { title: tr(title), collapsible, collapsed: collapsible,
            for spec in rows {
                Row { key: "{spec.key}", scope: scope.clone(), spec }
            }
        }
    }
}

/// The content of one tab.
#[component]
pub fn TabContent(scope: Scope, tab: ProfileTab) -> Element {
    let s = scope.clone();
    match tab {
        ProfileTab::Scaling => rsx! {
            SettingGroup { title: tr("profile-group-mode"), ModeRow { scope: s.clone() } }
            Group { scope: s.clone(), title: "profile-group-focus", rows: &FOCUS }
            Group { scope: s, title: "profile-group-auto", rows: &AUTO }
        },
        ProfileTab::Presentation => rsx! {
            SettingGroup { title: tr("profile-group-window"),
                Row { scope: s.clone(), spec: &WINDOW_TOP[0] }
                GeometryRow { scope: s.clone() }
                for spec in WINDOW_BOTTOM.iter() {
                    Row { key: "{spec.key}", scope: s.clone(), spec }
                }
            }
            Group { scope: s.clone(), title: "profile-group-crop", rows: &CROP_ROWS }
            FrameGenSection { scope: s }
        },
        ProfileTab::Display => rsx! {
            SettingGroup { title: tr("profile-group-devices"),
                MonitorRow { scope: s.clone() }
                GpuRow { scope: s.clone() }
            }
            Group { scope: s.clone(), title: "profile-group-vsync", rows: &VSYNC }
            Group { scope: s, title: "profile-group-scale", rows: &SCALE }
        },
        ProfileTab::Extras => rsx! {
            Group { scope: s.clone(), title: "profile-group-screenshots", rows: &SCREENSHOTS }
            Group { scope: s, title: "profile-group-osd", rows: &OSD }
        },
        ProfileTab::Advanced => rsx! {
            Group { scope: s.clone(), title: "profile-group-tiles", rows: &TILES, collapsible: true }
            Group { scope: s.clone(), title: "profile-group-timing", rows: &TIMING, collapsible: true }
            Group { scope: s, title: "profile-group-recovery", rows: &RECOVERY, collapsible: true }
        },
    }
}

/// The keys each tab shows, for the test below.
#[cfg(test)]
fn shown_keys(tab: ProfileTab) -> Vec<&'static str> {
    let rows = |list: &[RowSpec]| list.iter().map(|r| r.key).collect::<Vec<_>>();
    match tab {
        ProfileTab::Scaling => [vec!["mode"], rows(&FOCUS), rows(&AUTO)].concat(),
        ProfileTab::Presentation => [
            rows(&WINDOW_TOP),
            vec!["output_geometry"],
            rows(&WINDOW_BOTTOM),
            rows(&CROP_ROWS),
            super::framegen::KEYS.to_vec(),
        ]
        .concat(),
        ProfileTab::Display => [vec!["monitor", "gpu"], rows(&VSYNC), rows(&SCALE)].concat(),
        ProfileTab::Extras => [rows(&SCREENSHOTS), rows(&OSD)].concat(),
        ProfileTab::Advanced => [rows(&TILES), rows(&TIMING), rows(&RECOVERY)].concat(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::store::tab_keys;

    #[test]
    fn every_key_of_a_tab_has_a_row() {
        for tab in ProfileTab::ALL {
            let mut shown = shown_keys(tab);
            let mut keys = tab_keys(tab).to_vec();
            shown.sort_unstable();
            keys.sort_unstable();
            assert_eq!(shown, keys, "{tab:?}");
        }
    }

    #[test]
    fn every_label_is_translated() {
        let mut keys = Vec::new();
        for list in [
            &FOCUS[..],
            &AUTO,
            &WINDOW_TOP,
            &WINDOW_BOTTOM,
            &CROP_ROWS,
            &VSYNC,
            &SCALE,
            &SCREENSHOTS,
            &OSD,
            &TILES,
            &TIMING,
            &RECOVERY,
        ] {
            for spec in list {
                keys.push(spec.title);
                keys.extend(spec.subtitle);
                if let Control::Select(options) | Control::Connected(options) = spec.control {
                    keys.extend(options.iter().map(|(_, l)| *l));
                }
            }
        }
        for lang in [crate::i18n::Lang::Es, crate::i18n::Lang::En] {
            for key in &keys {
                assert!(crate::i18n::has_key(lang, key), "{lang:?}: {key}");
            }
        }
    }
}
