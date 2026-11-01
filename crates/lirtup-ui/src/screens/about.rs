//! Acerca de (front B): version, links, third-party licenses and
//! *Diagnóstico* (`Command::Diagnostics`, copy to the clipboard).

use dioxus::prelude::*;
use lirtup_proto::DiagReport;

use crate::app::{use_dispatch, use_store};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Icon, IconButton, SettingGroup, SettingRow,
};
use crate::state::{Action, LinkState, Origin};

/// The GUI's version.
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Source code.
const REPO_URL: &str = "https://github.com/Kushro/lirtup-forge";
/// The project LirtUp Forge forks.
const UPSTREAM_URL: &str = "https://github.com/baronsmv/linux-rt-upscaler";
/// Documentation.
const DOCS_URL: &str = "https://baronsmv.github.io/linux-rt-upscaler/";

/// Third-party work shipped or used, with its license.
const THIRD_PARTY: &[(&str, &str, &str)] = &[
    ("CuNNy", "LGPL-3.0", "https://github.com/funnyplanter/CuNNy"),
    ("Magpie", "GPL-3.0", "https://github.com/Blinue/Magpie"),
    ("lsfg-vk", "MIT", "https://github.com/PancakeTAS/lsfg-vk"),
    ("pe-parse", "MIT", "https://github.com/trailofbits/pe-parse"),
    ("volk", "MIT", "https://github.com/zeux/volk"),
    (
        "Dioxus",
        "MIT / Apache-2.0",
        "https://github.com/DioxusLabs/dioxus",
    ),
    (
        "Google Sans Flex",
        "OFL-1.1",
        "https://github.com/google/fonts/tree/main/ofl/googlesansflex",
    ),
    (
        "JetBrains Mono",
        "OFL-1.1",
        "https://github.com/JetBrains/JetBrainsMono",
    ),
    (
        "Material Symbols",
        "Apache-2.0",
        "https://github.com/google/material-design-icons",
    ),
];

/// The Acerca de page.
#[component]
pub fn About() -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let mut licenses = use_signal(|| false);
    let mut copied = use_signal(|| false);

    let s = store.read();
    let connected = matches!(s.link, LinkState::Connected { .. });
    let report = s.diagnostics.clone();
    drop(s);

    // Ask once the scaler answers.
    let mut asked = use_signal(|| false);
    use_effect(move || {
        let connected = matches!(store.read().link, LinkState::Connected { .. });
        if connected && !*asked.peek() && store.peek().diagnostics.is_none() {
            asked.set(true);
            dispatch(Action::RequestDiagnostics);
        }
    });

    let licenses_icon = if licenses() { "expand_less" } else { "expand_more" };
    let copy_icon = if copied() { "check" } else { "content_copy" };
    let text = report.as_ref().map(|r| diagnostics_text(r, VERSION));
    let copy_text = text.clone();

    rsx! {
        div { class: "flex flex-col items-start gap-4 w-full max-w-[720px] px-10 pt-8 pb-12",
            span { class: "grid place-items-center size-[72px] rounded-[24px] bg-primary text-on-primary",
                Icon { name: "diamond", fill: true, size: 32 }
            }
            h1 { class: "t-hm m-0", {t!("app-title")} }
            Badge { label: t!("about-version", version = VERSION), variant: BadgeVariant::Primary }
            p { class: "t-bm m-0 text-on-surface-variant", {t!("about-description")} }

            div { class: "w-full",
                SettingGroup {
                    SettingRow {
                        icon: "code",
                        title: t!("about-source"),
                        subtitle: REPO_URL.to_string(),
                        origin: Origin::Inherited,
                        IconButton { icon: "open_in_new", label: t!("about-open-link"), onclick: move |_| open_url(REPO_URL) }
                    }
                    SettingRow {
                        icon: "fork_right",
                        title: t!("about-upstream"),
                        subtitle: t!("about-upstream-subtitle"),
                        origin: Origin::Inherited,
                        IconButton { icon: "open_in_new", label: t!("about-open-link"), onclick: move |_| open_url(UPSTREAM_URL) }
                    }
                    SettingRow {
                        icon: "menu_book",
                        title: t!("about-docs"),
                        subtitle: DOCS_URL.to_string(),
                        origin: Origin::Inherited,
                        IconButton { icon: "open_in_new", label: t!("about-open-link"), onclick: move |_| open_url(DOCS_URL) }
                    }
                    SettingRow {
                        icon: "gavel",
                        title: t!("about-licenses"),
                        subtitle: t!("about-licenses-subtitle"),
                        origin: Origin::Inherited,
                        IconButton {
                            icon: licenses_icon,
                            label: t!("about-licenses-toggle"),
                            selected: licenses(),
                            onclick: move |_| licenses.toggle(),
                        }
                    }
                }
            }
            if licenses() {
                ul { class: "w-full m-0 p-0 list-none flex flex-col gap-0.5",
                    li { class: "flex items-center gap-3 px-5 py-3 rounded-xs bg-surface-container-low",
                        span { class: "t-bl flex-1", {t!("app-title")} }
                        Badge { label: "GPL-3.0-or-later".to_string() }
                    }
                    for (name, license, url) in THIRD_PARTY.iter().copied() {
                        li { key: "{name}", class: "flex items-center gap-3 px-5 py-3 rounded-xs bg-surface-container-low",
                            span { class: "t-bl flex-1", "{name}" }
                            Badge { label: license.to_string() }
                            IconButton { icon: "open_in_new", label: t!("about-open-link"), size: 32, onclick: move |_| open_url(url) }
                        }
                    }
                }
            }

            section { class: "flex flex-col gap-3 w-full mt-4",
                div { class: "flex flex-wrap items-center gap-3",
                    h2 { class: "t-tm m-0 flex-1", {t!("about-diagnostics")} }
                    Button {
                        label: t!("about-diagnostics-refresh"),
                        variant: ButtonVariant::Text,
                        icon: "refresh",
                        disabled: !connected,
                        onclick: move |_| {
                            copied.set(false);
                            dispatch(Action::RequestDiagnostics);
                        },
                    }
                    Button {
                        label: if copied() { t!("about-diagnostics-copied") } else { t!("about-diagnostics-copy") },
                        variant: ButtonVariant::Tonal,
                        icon: copy_icon,
                        disabled: copy_text.is_none(),
                        onclick: move |_| {
                            if let Some(text) = &copy_text {
                                super::home::copy_to_clipboard(text);
                                copied.set(true);
                            }
                        },
                    }
                }
                p { class: "t-bs m-0 text-on-surface-variant", {t!("about-diagnostics-subtitle")} }
                match text {
                    Some(text) => rsx! {
                        pre { class: "mono t-bs m-0 p-4 rounded-lg bg-surface-container whitespace-pre-wrap break-words select-text",
                            "{text}"
                        }
                    },
                    None => rsx! {
                        p { class: "t-bm m-0 p-4 rounded-lg bg-surface-container text-on-surface-variant",
                            if connected { {t!("about-diagnostics-waiting")} } else { {t!("about-diagnostics-offline")} }
                        }
                    },
                }
            }
        }
    }
}

/// Open a link in the person's browser.
fn open_url(url: &str) {
    if let Err(e) = std::process::Command::new("xdg-open").arg(url).spawn() {
        tracing::warn!("xdg-open {url}: {e}");
    }
}

/// The report as plain text, in English (it goes to bug reports).
pub fn diagnostics_text(report: &DiagReport, gui_version: &str) -> String {
    let fg = &report.framegen;
    let none = || "-".to_string();
    let mut lines = vec![
        format!("LirtUp Forge GUI {gui_version}"),
        format!("lirtup serve {}", report.version),
        format!("Display: {}", report.display),
        format!("Vulkan: {}", report.vulkan.clone().unwrap_or_else(none)),
    ];
    for gpu in &report.gpus {
        lines.push(format!(
            "GPU {}{}: {} [{:04x}:{:04x}] {}, driver {}, fp16 {}, external fd {}",
            gpu.index,
            if gpu.selected { " (selected)" } else { "" },
            gpu.name,
            gpu.vendor_id,
            gpu.device_id,
            if gpu.discrete { "discrete" } else { "integrated" },
            gpu.driver,
            yes_no(gpu.fp16),
            yes_no(gpu.external_fd),
        ));
    }
    if report.gpus.is_empty() {
        lines.push("GPU: none".into());
    }
    lines.push(format!(
        "Frame generation: engine {:?}, configured {:?}, state {:?}, active {}",
        fg.engine,
        fg.configured,
        fg.state,
        fg.active.map_or_else(none, |m| format!("x{m}")),
    ));
    lines.push(format!(
        "  layer {}, dll {}, native {}, env locked {}",
        yes_no(fg.layer_installed),
        yes_no(fg.dll_found),
        yes_no(fg.native_available),
        yes_no(fg.env_locked),
    ));
    if let Some(code) = fg.reason_code {
        lines.push(format!(
            "  reason {code:?}: {}",
            fg.reason.clone().unwrap_or_else(none)
        ));
    } else if let Some(reason) = &fg.reason {
        lines.push(format!("  reason: {reason}"));
    }
    lines.push(format!(
        "Lossless.dll: {}",
        report.lossless_dll.clone().unwrap_or_else(none)
    ));
    lines.push(format!("dxc: {}", report.dxc.clone().unwrap_or_else(none)));
    lines.push(format!(
        "glslang: {}",
        report.glslang.clone().unwrap_or_else(none)
    ));
    lines.push(format!("Config: {}", report.config_dir));
    lines.join("\n")
}

fn yes_no(v: bool) -> &'static str {
    if v { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use lirtup_proto::GpuInfo;

    use super::*;

    #[test]
    fn diagnostics_as_text() {
        let report = DiagReport {
            version: "2.0.0".into(),
            gpus: vec![GpuInfo {
                index: 0,
                name: "AMD Radeon RX 9070 XT".into(),
                vendor_id: 0x1002,
                device_id: 0x7550,
                discrete: true,
                driver: "radv 25.2".into(),
                fp16: true,
                external_fd: true,
                selected: true,
            }],
            vulkan: Some("1.4".into()),
            display: "XWayland".into(),
            framegen: Default::default(),
            lossless_dll: None,
            dxc: Some("dxcompiler 1.8".into()),
            glslang: None,
            config_dir: "/home/u/.config/linux-rt-upscaler".into(),
        };
        let text = diagnostics_text(&report, "2.0.0-alpha.0");
        assert!(text.starts_with("LirtUp Forge GUI 2.0.0-alpha.0\nlirtup serve 2.0.0\n"));
        assert!(text.contains(
            "GPU 0 (selected): AMD Radeon RX 9070 XT [1002:7550] discrete, driver radv 25.2, fp16 yes, external fd yes"
        ));
        assert!(text.contains("Lossless.dll: -"));
        assert!(text.ends_with("Config: /home/u/.config/linux-rt-upscaler"));
    }
}
