//! Every component with its variants and states, in one page (front A's
//! test bench and the reviewer's check against the canvas). Reached with
//! `LIRTUP_GUI_GALLERY=1` or Ctrl+Shift+G; never from the drawer.
//!
//! The texts here are sample data (the canvas's), not the app's strings.

use dioxus::prelude::*;
use lirtup_config::Hotkey;
use lirtup_x11::{Rect, WindowInfo};

use super::*;
use crate::state::Origin;
use crate::theme::{self, DEFAULT_SEED, PALETTES, Scheme};

/// Which themes the gallery shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Show {
    Dark,
    Light,
    Both,
}

/// The gallery page (`Route::Gallery`): one section per component, as the
/// canvas's «Componentes» artboard, with a theme switch at the top.
#[component]
pub fn Gallery() -> Element {
    let show = use_signal(|| match *theme::SCHEME.peek() {
        Scheme::Dark => Show::Dark,
        Scheme::Light => Show::Light,
    });
    let seed = use_signal(|| DEFAULT_SEED.to_string());
    let reduce = use_signal(|| false);
    // The app's scheme follows the switch (Both keeps the current one).
    use_effect(move || match show() {
        Show::Dark => *theme::SCHEME.write() = Scheme::Dark,
        Show::Light => *theme::SCHEME.write() = Scheme::Light,
        Show::Both => {}
    });
    let swatches: Vec<String> = std::iter::once(DEFAULT_SEED)
        .chain(PALETTES.iter().map(|(_, s)| *s))
        .map(String::from)
        .collect();
    let schemes: Vec<Scheme> = match show() {
        Show::Dark => vec![Scheme::Dark],
        Show::Light => vec![Scheme::Light],
        Show::Both => vec![Scheme::Dark, Scheme::Light],
    };
    let palette_name = PALETTES
        .iter()
        .find(|(_, s)| s.eq_ignore_ascii_case(&seed()))
        .map_or("LirtUp Forge", |(n, _)| n);
    rsx! {
        div { class: "flex flex-col min-h-full",
            header { class: "flex flex-wrap items-center gap-4 px-8 py-5 bg-surface-container-low",
                div { class: "flex flex-col mr-auto",
                    span { class: "t-lm text-primary", "LIRTUP-UI · FRENTE A" }
                    h1 { class: "t-hs m-0", "Galería de componentes" }
                }
                Connected {
                    label: "Tema".to_string(),
                    options: vec![(Show::Dark, "Oscuro".to_string()), (Show::Light, "Claro".to_string()), (Show::Both, "Ambos".to_string())],
                    icons: vec![Some("dark_mode"), Some("light_mode"), Some("contrast")],
                    value: show,
                }
                label { class: "flex items-center gap-2 t-bm",
                    Switch { value: reduce, label: "Reducir movimiento".to_string() }
                    "Reducir movimiento"
                }
            }
            div { class: "flex flex-wrap items-center gap-4 px-8 py-3 bg-surface-container-low border-t border-outline-variant",
                span { class: "t-ts", "Semilla: {palette_name}" }
                ColorField { value: seed, label: "Semilla".to_string(), swatches }
            }
            div { class: "flex flex-1 gap-0 min-h-0", class: if reduce() { "lf-reduce" },
                for scheme in schemes {
                    GalleryBody { key: "{scheme:?}", scheme, seed: seed() }
                }
            }
        }
    }
}

/// A placeholder preview: the canvas's colored blobs as an SVG.
fn sample_thumb(bg: &str, a: &str, b: &str) -> String {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 400 300'><rect width='400' height='300' fill='{bg}'/>\
         <circle cx='90' cy='80' r='90' fill='{a}' opacity='.55'/><circle cx='320' cy='230' r='120' fill='{b}' opacity='.5'/></svg>"
    );
    format!("data:image/svg+xml,{}", svg.replace('#', "%23").replace('<', "%3C").replace('>', "%3E").replace(' ', "%20"))
}

fn sample_window(xid: u32, title: &str) -> WindowInfo {
    WindowInfo {
        xid,
        title: title.into(),
        rect: Rect { x: 0, y: 0, width: 1280, height: 720 },
        pid: None,
        class: None,
    }
}

fn card(title: &str, body: Element) -> Element {
    rsx! {
        section { class: "flex flex-col gap-4 p-6 rounded-xl bg-surface-container-low min-w-0",
            h2 { class: "t-tm m-0", "{title}" }
            {body}
        }
    }
}

fn caption(text: &str) -> Element {
    rsx! {
        h3 { class: "t-ls m-0 text-on-surface-variant uppercase", "{text}" }
    }
}

/// The gallery in one scheme.
#[component]
fn GalleryBody(scheme: Scheme, seed: String) -> Element {
    let style = theme::root_style(&seed, scheme);
    let name = match scheme {
        Scheme::Dark => "dark",
        Scheme::Light => "light",
    };
    // Inputs.
    let engine = use_signal(|| "native");
    let multiplier = use_signal(|| 4u8);
    let mut on = use_signal(|| true);
    let off = use_signal(|| false);
    let checked = use_signal(|| true);
    let unchecked = use_signal(|| false);
    let flow = use_signal(|| 0.75);
    let zoom = use_signal(|| 120.0);
    let model = use_signal(|| 3.0);
    let upscaler = use_signal(|| "lanczos2");
    let present = use_signal(|| "mailbox");
    let filter = use_signal(String::new);
    let name_field = use_signal(|| "BALDR SKY".to_string());
    let bad_field = use_signal(|| "Predeterminado".to_string());
    let lut = use_signal(|| "/home/ana/.config/linux-rt-upscaler/luts/warm.cube".to_string());
    let shots = use_signal(String::new);
    let bg = use_signal(|| "#000000".to_string());
    let font = use_signal(String::new);
    let follow = use_signal(|| false);
    let pause = use_signal(|| true);
    let daemon = use_signal(|| true);
    let fg = use_signal(|| false);
    let hk_full = use_signal(|| "Alt+Shift+A".parse::<Hotkey>().ok());
    let hk_window = use_signal(|| "Alt+Shift+Q".parse::<Hotkey>().ok());
    let hk_shot = use_signal(|| None::<Hotkey>);
    let mut tab = use_signal(|| "scaling");
    let mut chip_profile = use_signal(|| true);
    let mut chip_none = use_signal(|| false);
    let mut nav = use_signal(|| 0usize);
    // Overlays.
    let mut bar = use_signal(|| false);
    let mut snack = use_signal(|| false);
    let mut dialog = use_signal(|| false);
    let mut danger_dialog = use_signal(|| false);
    let mut menu = use_signal(|| false);
    let mut menu_at = use_signal(|| (0.0, 0.0));
    let mut last = use_signal(|| "—".to_string());

    let reset_menu = vec![
        MenuItem::new("tab", "Deshacer los cambios de esta pestaña").icon("undo"),
        MenuItem::new("overrides", "Quitar los ajustes del perfil en esta pestaña").icon("layers_clear"),
        MenuItem { divider_before: true, ..MenuItem::new("all", "Volver todo a los valores del sistema").icon("restart_alt") },
    ];
    let tile_menu = vec![
        MenuItem::new("window", "Escalar en ventana").icon("select_window"),
        MenuItem::new("new", "Crear perfil desde esta ventana").icon("person_add"),
        MenuItem::new("copy", "Copiar título").icon("content_copy"),
        MenuItem { disabled: true, ..MenuItem::new("pin", "Fijar (pronto)").icon("push_pin") },
        MenuItem { divider_before: true, danger: true, ..MenuItem::new("delete", "Eliminar perfil").icon("delete") },
    ];
    let families = vec!["Inter".to_string(), "Noto Sans".to_string(), "Cantarell".to_string(), "DejaVu Sans".to_string()];
    let pico = |initials: &str, color: &str| NavIcon::Profile { initials: initials.into(), color: color.into(), image: None };
    let tabs = vec![
        TabItem { value: "scaling", label: "Escalado".into(), icon: "tune", dot: false },
        TabItem { value: "presentation", label: "Presentación".into(), icon: "web_asset", dot: true },
        TabItem { value: "display", label: "Pantalla".into(), icon: "monitor", dot: false },
        TabItem { value: "extras", label: "Extras".into(), icon: "more_horiz", dot: false },
        TabItem { value: "advanced", label: "Avanzado".into(), icon: "settings_applications", dot: false },
    ];
    let pick = move |id: &'static str| last.set(id.to_string());

    rsx! {
        div {
            class: "lf-root flex-1 min-w-0 bg-surface text-on-surface font-brand px-8 py-8",
            "data-theme": name,
            style,
            div { class: "grid gap-4", style: "grid-template-columns: repeat(auto-fill, minmax(min(100%, 420px), 1fr))",

                {card("Button · 6 variantes", rsx! {
                    div { class: "flex flex-wrap gap-2",
                        Button { label: "Escalar", icon: "fullscreen" }
                        Button { label: "Guardar", variant: ButtonVariant::Tonal }
                        Button { label: "Cancelar", variant: ButtonVariant::Outlined }
                        Button { label: "Más tarde", variant: ButtonVariant::Text }
                        Button { label: "Detener", variant: ButtonVariant::Tertiary, icon: "stop" }
                        Button { label: "Eliminar", variant: ButtonVariant::Danger, icon: "delete" }
                        Button { label: "Guardar", disabled: true }
                        Button { label: "Guardar", title: "Guardar (Ctrl+S)".to_string(), variant: ButtonVariant::Tonal, icon: "save" }
                    }
                    {caption("Tallas XS · S · M · L")}
                    div { class: "flex flex-wrap gap-2 items-center",
                        Button { label: "XS 32", size: ButtonSize::Xs }
                        Button { label: "S 40" }
                        Button { label: "M 56", size: ButtonSize::M }
                        Button { label: "Escalar", size: ButtonSize::L, icon: "fullscreen" }
                    }
                    p { class: "t-bs m-0 text-on-surface-variant", "Redondo en reposo; al pulsar se cuadra. Con «Reducir movimiento», no cambia de forma." }
                })}

                {card("IconButton · SplitButton", rsx! {
                    div { class: "flex flex-wrap gap-2 items-center",
                        IconButton { icon: "info", label: "Información" }
                        IconButton { icon: "content_copy", label: "Duplicar", variant: IconButtonVariant::Tonal }
                        IconButton { icon: "edit", label: "Editar", variant: IconButtonVariant::Outlined }
                        IconButton { icon: "add", label: "Agregar", variant: IconButtonVariant::Filled }
                        IconButton { icon: "push_pin", label: "Fijar", selected: on(), onclick: move |_| on.toggle() }
                        IconButton { icon: "close", label: "Cerrar", size: 32 }
                        IconButton { icon: "play_arrow", label: "Escalar", size: 56, variant: IconButtonVariant::Filled }
                        IconButton { icon: "delete", label: "Eliminar", disabled: true }
                    }
                    div { class: "flex flex-wrap gap-4 items-center",
                        SplitButton {
                            label: "Restablecer",
                            variant: ButtonVariant::Tonal,
                            menu_label: "Más opciones de restablecer".to_string(),
                            on_click: move |_| last.set("main".into()),
                            menu: reset_menu.clone(),
                            on_select: pick,
                        }
                        span { class: "t-bs text-on-surface-variant", "Elegido: {last}" }
                    }
                })}

                {card("Connected · Chip · Badge", rsx! {
                    Connected {
                        label: "Motor de efectos".to_string(),
                        options: vec![("native", "Nativo".to_string()), ("magpie", "MagpieFX".to_string())],
                        value: engine,
                    }
                    Connected {
                        label: "Multiplicador".to_string(),
                        options: vec![(0, "Apagado".to_string()), (2, "×2".to_string()), (3, "×3".to_string()), (4, "Auto".to_string())],
                        icons: vec![None, None, None, Some("auto_awesome")],
                        primary: true,
                        value: multiplier,
                    }
                    Connected {
                        label: "Deshabilitado".to_string(),
                        options: vec![("a", "Uno".to_string()), ("b", "Dos".to_string())],
                        value: use_signal(|| "a"),
                        disabled: true,
                    }
                    div { class: "flex flex-wrap gap-2",
                        Chip { label: "Con perfil", selected: chip_profile(), onclick: move |_| chip_profile.toggle() }
                        Chip { label: "Sin perfil", selected: chip_none(), onclick: move |_| chip_none.toggle() }
                        Chip { label: "Wine/Proton", icon: "wine_bar" }
                        Chip { label: "título contiene «BALDR»", icon: "title", on_remove: move |_| last.set("chip ×".into()) }
                    }
                    div { class: "flex flex-wrap gap-2",
                        Badge { label: "Escalando", variant: BadgeVariant::Tertiary, icon: "play_arrow" }
                        Badge { label: "×2 Auto", variant: BadgeVariant::Primary }
                        Badge { label: "0,9 ms", variant: BadgeVariant::Success, icon: "check" }
                        Badge { label: "Hereda" }
                        Badge { label: "Necesita dxc", variant: BadgeVariant::Error }
                        Badge { label: "Global", icon: "public" }
                    }
                })}

                {card("Switch · Checkbox · Slider", rsx! {
                    div { class: "flex flex-wrap gap-4 items-center",
                        Switch { value: on, label: "Encendido" }
                        Switch { value: off, label: "Apagado" }
                        Switch { value: use_signal(|| true), label: "Deshabilitado", disabled: true }
                        Checkbox { value: checked, label: "Marcada" }
                        Checkbox { value: unchecked, label: "Sin marcar" }
                        Checkbox { value: use_signal(|| true), label: "Deshabilitada", disabled: true }
                    }
                    div { class: "flex items-center gap-3",
                        span { class: "t-bm w-28 shrink-0", "Escala de flujo" }
                        Slider { value: flow, min: 0.0, max: 1.0, step: 0.05, label: "Escala de flujo" }
                    }
                    div { class: "flex items-center gap-3",
                        span { class: "t-bm w-28 shrink-0", "Zoom" }
                        Slider {
                            value: zoom,
                            min: 50.0,
                            max: 200.0,
                            step: 5.0,
                            label: "Zoom",
                            format: Callback::new(|v: f64| format!("{v:.0} %")),
                        }
                    }
                    div { class: "flex items-center gap-3",
                        span { class: "t-bm w-28 shrink-0", "Modelo" }
                        Slider {
                            value: model,
                            min: 0.0,
                            max: 0.0,
                            step: 1.0,
                            label: "Modelo SRCNN",
                            stops: ["none", "2x8", "4x8", "4x16", "8x16", "8x32"].map(String::from).to_vec(),
                        }
                    }
                    div { class: "flex items-center gap-3",
                        span { class: "t-bm w-28 shrink-0", "Deshabilitado" }
                        Slider { value: use_signal(|| 0.3), min: 0.0, max: 1.0, step: 0.1, label: "Deshabilitado", disabled: true }
                    }
                })}

                {card("Select · TextField", rsx! {
                    div { class: "flex flex-wrap gap-2",
                        Select {
                            label: "Sobremuestreador".to_string(),
                            options: vec![("bilinear", "Bilineal".to_string()), ("lanczos2", "Lanczos-2".to_string()), ("lanczos3", "Lanczos-3".to_string()), ("fsr", "FSR 1 EASU".to_string())],
                            value: upscaler,
                        }
                        Select {
                            label: "Modo de presentación".to_string(),
                            options: vec![("fifo", "fifo".to_string()), ("mailbox", "mailbox".to_string()), ("immediate", "immediate".to_string())],
                            value: present,
                        }
                        Select { label: "Deshabilitado".to_string(), options: vec![(1, "Uno".to_string())], value: use_signal(|| 1), disabled: true }
                    }
                    TextField {
                        value: filter,
                        label: "Filtrar ventanas".to_string(),
                        placeholder: "Filtrar ventanas…".to_string(),
                        leading_icon: "search",
                        shortcut: "Ctrl F".to_string(),
                    }
                    TextField { value: name_field, label: "Nombre del perfil".to_string() }
                    TextField {
                        value: bad_field,
                        label: "Nombre del perfil".to_string(),
                        error: "Ya existe un perfil con ese nombre".to_string(),
                    }
                    TextField { value: use_signal(String::new), label: "Deshabilitado".to_string(), disabled: true }
                })}

                {card("PathField · ColorField · FontField", rsx! {
                    PathField {
                        value: lut,
                        label: "LUT".to_string(),
                        filters: vec![("LUT".to_string(), vec!["cube".to_string(), "png".to_string()])],
                    }
                    PathField { value: shots, label: "Carpeta de capturas".to_string(), directory: true }
                    ColorField { value: bg, label: "Color de fondo".to_string() }
                    FontField { value: font, label: "Fuente".to_string(), families }
                })}

                {card("SettingRow · tres estados · SettingGroup", rsx! {
                    SettingGroup { title: "Escalado".to_string(),
                        SettingRow { icon: "center_focus_strong", title: "Seguir enfoque", origin: Origin::Inherited,
                            Switch { value: follow, label: "Seguir enfoque" }
                        }
                        SettingRow {
                            icon: "pause_circle",
                            title: "Pausa al perder foco",
                            origin: Origin::Overridden,
                            on_inherit: move |_| last.set("inherit".into()),
                            Switch { value: pause, label: "Pausa al perder foco" }
                        }
                        SettingRow { icon: "auto_mode", title: "Modo daemon", origin: Origin::Dirty { before: "apagado".into() },
                            Switch { value: daemon, label: "Modo daemon" }
                        }
                        SettingRow {
                            icon: "animation",
                            title: "Generación de cuadros",
                            subtitle: "Igual para todos los perfiles".to_string(),
                            origin: Origin::Overridden,
                            global: true,
                            Switch { value: fg, label: "Generación de cuadros" }
                        }
                        SettingRow {
                            icon: "blur_on",
                            title: "LumaSharpen",
                            subtitle: "Máscara de enfoque sobre la luminancia".to_string(),
                            origin: Origin::Inherited,
                            detail: rsx! {
                                div { class: "flex items-center gap-3",
                                    span { class: "t-bs w-20 shrink-0", "Magnitud" }
                                    Slider { value: use_signal(|| 0.65), min: 0.0, max: 2.0, step: 0.05, label: "Magnitud" }
                                }
                            },
                            Badge { label: "0,2 ms" }
                        }
                    }
                    SettingGroup { title: "Tiempos y recuperación".to_string(), collapsible: true, collapsed: true,
                        SettingRow { icon: "timer", title: "Tiempo de espera", origin: Origin::Inherited,
                            Select { label: "Tiempo de espera".to_string(), options: vec![(1, "1 s".to_string()), (2, "2 s".to_string())], value: use_signal(|| 1) }
                        }
                    }
                })}

                {card("HotkeyCapture · Keycap", rsx! {
                    SettingGroup {
                        SettingRow { icon: "fullscreen", title: "Escalar en pantalla completa", origin: Origin::Inherited, subtitle: "Pulse el lápiz y luego las teclas".to_string(),
                            HotkeyCapture { value: hk_full, label: "Escalar en pantalla completa", require_modifier: true }
                        }
                        SettingRow { icon: "select_window", title: "Escalar en ventana", origin: Origin::Inherited,
                            HotkeyCapture { value: hk_window, label: "Escalar en ventana", require_modifier: true }
                        }
                        SettingRow { icon: "photo_camera", title: "Capturar pantalla", origin: Origin::Inherited,
                            HotkeyCapture { value: hk_shot, label: "Capturar pantalla", require_modifier: true }
                        }
                    }
                    div { class: "flex gap-2 items-center",
                        Keycap { label: "Ctrl" }
                        Keycap { label: "S" }
                        Keycap { label: "Esc", soft: true }
                    }
                })}

                {card("NavDrawer · NavItem · Tabs", rsx! {
                    div { class: "flex gap-3 h-[460px] rounded-lg overflow-hidden",
                        for rail in [false, true] {
                            NavDrawer {
                                key: "{rail}",
                                rail,
                                footer: rsx! {
                                    NavItem { label: "Acerca de", icon: NavIcon::Symbol("info"), onclick: move |_| nav.set(5) }
                                    NavItem { label: "Ajustes", icon: NavIcon::Symbol("settings"), selected: nav() == 6, onclick: move |_| nav.set(6) }
                                },
                                NavItem { label: "Inicio", icon: NavIcon::Symbol("home"), selected: nav() == 0, onclick: move |_| nav.set(0) }
                                NavItem { label: "Modos de escalado", icon: NavIcon::Symbol("tune"), selected: nav() == 1, onclick: move |_| nav.set(1) }
                                NavHeader { label: "Perfiles" }
                                NavItem { label: "Predeterminado", icon: NavIcon::Symbol("star"), selected: nav() == 2, onclick: move |_| nav.set(2) }
                                NavItem { label: "BALDR SKY", icon: pico("BS", "#3D6FB6"), dirty: true, selected: nav() == 3, onclick: move |_| nav.set(3) }
                                NavItem { label: "Utawarerumono", icon: pico("UT", "#B3641E"), running: true, selected: nav() == 4, onclick: move |_| nav.set(4) }
                                NavItem { label: "Nuevo perfil", icon: NavIcon::Symbol("add"), onclick: move |_| dialog.set(true) }
                            }
                        }
                    }
                    Tabs { tabs: tabs.clone(), selected: tab(), on_select: move |t| tab.set(t) }
                })}

                {card("WindowTile · estados", rsx! {
                    div { class: "grid grid-cols-2 gap-3",
                        WindowTile {
                            window: sample_window(1, "BALDR SKY"),
                            profile: "BALDR SKY".to_string(),
                            thumb: sample_thumb("#18324A", "#3D6FB6", "#7FB4CA"),
                            on_activate: move |_| last.set("tile".into()),
                            on_menu: move |p| { menu_at.set(p); menu.set(true); },
                        }
                        WindowTile {
                            window: sample_window(2, "Demons Roots"),
                            state: TileState::Hover,
                            thumb: sample_thumb("#2B2140", "#8A4FA8", "#C792EA"),
                            on_activate: move |_| last.set("tile".into()),
                            on_menu: move |p| { menu_at.set(p); menu.set(true); },
                        }
                        WindowTile {
                            window: sample_window(3, "Gin'iro, Haruka"),
                            state: TileState::Focus,
                            aspect: "video".to_string(),
                            thumb: sample_thumb("#E9E4EE", "#A8C8FF", "#FFB68B"),
                            on_activate: move |_| last.set("tile".into()),
                            on_menu: move |p| { menu_at.set(p); menu.set(true); },
                        }
                        WindowTile {
                            window: sample_window(4, "Utawarerumono"),
                            profile: "Utawarerumono".to_string(),
                            state: TileState::Running,
                            fps: 119.6,
                            aspect: "video".to_string(),
                            thumb: sample_thumb("#3A2A1E", "#B3641E", "#FFB68B"),
                            on_activate: move |_| last.set("tile".into()),
                            on_menu: move |p| { menu_at.set(p); menu.set(true); },
                        }
                    }
                    p { class: "t-bs m-0 text-on-surface-variant", "Clic derecho, ⋮ o la tecla Menú abren el menú del mosaico." }
                    Menu { open: menu, items: tile_menu, at: Some(menu_at()), on_select: pick }
                })}

                {card("FloatingToolbar · Snackbar · Dialog · Tooltip", rsx! {
                    div { class: "flex flex-wrap gap-2",
                        Button { label: if bar() { "Ocultar barra" } else { "Mostrar barra" }, variant: ButtonVariant::Tonal, onclick: move |_| bar.toggle() }
                        Button { label: "Snackbar", variant: ButtonVariant::Tonal, onclick: move |_| snack.set(true) }
                        Button { label: "Diálogo", variant: ButtonVariant::Tonal, onclick: move |_| dialog.set(true) }
                        Button { label: "Diálogo de borrar", variant: ButtonVariant::Danger, onclick: move |_| danger_dialog.set(true) }
                    }
                    div { class: "flex flex-wrap gap-6 items-center",
                        Tooltip { text: "Duplicar perfil (Ctrl+D)",
                            Button { label: "Pase el puntero", variant: ButtonVariant::Outlined }
                        }
                        span { class: "t-bs text-on-surface-variant", "Último: {last}" }
                    }
                    FloatingToolbar { open: bar(),
                        span { class: "dot" }
                        span { class: "t-ll mr-2", "3 cambios sin guardar" }
                        SplitButton {
                            label: "Restablecer",
                            variant: ButtonVariant::Outlined,
                            menu_label: "Más opciones de restablecer".to_string(),
                            on_click: move |_| last.set("reset".into()),
                            menu: reset_menu,
                            on_select: pick,
                        }
                        Button { label: "Guardar", title: "Ctrl+S".to_string(), onclick: move |_| bar.set(false) }
                    }
                    if snack() {
                        Snackbar {
                            message: "Perfil «BALDR SKY» guardado.",
                            action: "Deshacer".to_string(),
                            on_action: move |_| last.set("deshacer".into()),
                            on_close: move |_| snack.set(false),
                        }
                    }
                    Dialog {
                        open: dialog,
                        icon: "edit_note",
                        icon_color: "tertiary",
                        title: "Cambios sin guardar",
                        actions: rsx! {
                            Button { label: "Cancelar", variant: ButtonVariant::Text, onclick: move |_| dialog.set(false) }
                            Button { label: "Descartar y cambiar", variant: ButtonVariant::Text, onclick: move |_| dialog.set(false) }
                            Button { label: "Guardar y cambiar", onclick: move |_| dialog.set(false) }
                        },
                        p { class: "m-0", "Tienes cambios sin guardar en «BALDR SKY». Guárdalos antes de cambiar, descártalos, o cancela para permanecer." }
                        TextField { value: use_signal(String::new), label: "Un campo para probar el foco".to_string() }
                    }
                    Dialog {
                        open: danger_dialog,
                        icon: "delete",
                        icon_color: "error",
                        title: "¿Eliminar «BALDR SKY»?",
                        actions: rsx! {
                            Button { label: "Cancelar", variant: ButtonVariant::Text, onclick: move |_| danger_dialog.set(false) }
                            Button { label: "Eliminar", variant: ButtonVariant::Danger, onclick: move |_| danger_dialog.set(false) }
                        },
                        p { class: "m-0", "Se borra el perfil de config.yaml. Las ventanas que coincidían usarán Predeterminado." }
                    }
                })}

                {card("Icon · LoadingIndicator", rsx! {
                    div { class: "flex flex-wrap gap-4 items-center text-on-surface-variant",
                        Icon { name: "home", size: 18 }
                        Icon { name: "home", size: 20 }
                        Icon { name: "home" }
                        Icon { name: "home", size: 32 }
                        Icon { name: "home", fill: true, size: 32 }
                        Icon { name: "auto_awesome", fill: true, size: 32, label: "Automático".to_string() }
                    }
                    div { class: "flex flex-wrap gap-6 items-center",
                        LoadingIndicator { size: 24, label: "Cargando" }
                        LoadingIndicator { label: "Cargando" }
                        LoadingIndicator { size: 48, label: "Cargando" }
                    }
                })}
            }
        }
    }
}
