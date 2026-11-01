# Ronda 4: el esqueleto del revisor

Rama **`rust/r4-skeleton`**, que sale de `rust/r3-integration`. Fecha: 2026-11-01. Los cuatro
frentes de la ronda 4 (M, T, Y, P) salen de esta rama. Sin push y sin merge a `main`.

Compilé con `CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r3-integration`.
- `cargo check --workspace --all-targets` (rustc 1.99.1): **pasa, sin avisos**.
- `cargo test --workspace`: **390 pasan**, 0 fallan. Hay un test nuevo, `save_setting_from_the_bar`.
- `scripts/check-ftl-keys.py`: 570 claves en cada idioma, ninguna falta.

Notas anteriores: [rust-r3-integration.md](rust-r3-integration.md) (lo que pasa a esta ronda) y
[rust-r0-contract.md](rust-r0-contract.md) (las reglas de siempre).

## Decisiones que necesita la persona

1. **¿La 2.0 reemplaza a la app de Python o se publica aparte?** Hoy la 1.x se publica en PyPI
   (`.github/workflows/build_wheels.yml`) con `setup.py` y `pyproject.toml`. De esto dependen:
   - el nombre del paquete de AUR (el mismo que la 1.x o uno nuevo);
   - si se retira Python del repositorio, con `src/upscaler/`, `setup.py`, `pyproject.toml` y
     el workflow de wheels;
   - **si se mueven los datos**: los `.spv`, los modelos CuNNy, los 117 efectos de Magpie y el
     C++ de lsfg siguen en `src/upscaler/…` y los crates los leen de ahí (regla 5 del contrato de
     la ronda 0). Se mueven solo si se retira Python.

   Mientras no se decida, P arma el paquete de AUR y la AppImage de la 2.0 con otro nombre y no
   toca nada de Python.
2. **La versión.** `2.0.0-alpha.0` hoy. Propongo `2.0.0-beta.1` al cerrar esta ronda y `2.0.0`
   cuando la persona pruebe el paquete en una máquina limpia. La versión y el `CHANGELOG.md`
   son míos, no de P.

## Qué escribí (`9de59450`)

### Cambios de contrato, ya implementados

| Cambio | Dónde | Qué |
| --- | --- | --- |
| `Command::PauseHotkeys { on }` | proto, core (engine y server) | Lo cuenta el servidor por conexión, como `WatchGpuTimes`; una conexión que se cierra deja de contar. Mientras la cuenta no sea 0, el motor suelta todos los atajos globales (`rebind(&[])`) y después los vuelve a tomar. Un `ReloadConfig` que llega en pausa no los toma. Resuelve que `HotkeyCapture` no pudiera capturar Alt+Shift+A con el escalador corriendo |
| `Event::SaveSetting { profile, key, value }` | proto, GUI (`store.rs`) | La barra sobre el juego cambia algo en vivo (el multiplicador, «fijar») y le pide a la GUI que lo guarde, porque la GUI es la única que escribe YAML. Una clave global va a Predeterminado. Una edición sin guardar de la misma clave se descarta: gana el valor en vivo. Sin GUI conectada, el cambio dura lo que la sesión. Tiene test |
| `GuiConfig` | `lirtup-config/src/gui.rs` | `show_tray_icon` (sí), `minimize_to_tray` (no), `start_delay_s` (3, de 0 a 60) y `migration_declined` (no) |
| `HotkeyAction::ToggleToolbar` | config, pipeline, Ajustes ▸ Atajos | Alt+Shift+D, en el grupo Sesión. En la sesión, por ahora, solo deja una línea en el log |
| `lirtup migrate [--dry-run] [-c config]` | `lirtup-cli/src/main.rs` | Muestra el plan y lo aplica con `lirtup_config::migrate` (de M) |
| `Effect::Migrate` | GUI (`app.rs`) | Aplica la migración en un hilo bloqueante, despacha `Migrated` y vuelve a leer los archivos |
| `serve` | `lirtup-cli/src/serve.rs` | Salió de `main.rs`, para que Y trabaje en un archivo propio |

Para la barra quedaron declarados en `surface.rs` dos comandos nuevos de la superposición
(`OverlayCommand::SetToolbarZone` y `OverlayCommand::BeginMove`) y un aviso
(`OverlayNotice::Pointer`). El hilo de la superposición guarda la zona y delega el movimiento en
`OverlayWindow::begin_move`. El ruteo de los eventos es de T.

### Firmas con `todo!()`

| Frente | Archivo | Firmas |
| --- | --- | --- |
| T | `lirtup-pipeline/src/tokens.rs` | Incluye el `tokens.rs` que genera `gen-tokens` en `lirtup-ui/design/` (un módulo con `#[path]`, así hay una sola copia); `Scheme`, `color(role, scheme)`, `icon(name)` (ya implementado), `ease(motion, t)` |
| T | `lirtup-pipeline/src/toolbar.rs` | `Toolbar` (`new`, `rebind`, `zone`, `pointer`, `set_shown`, `update`, `pass`), `ToolbarAction`, `ToolbarState`, `HIDE_AFTER` |
| T | `lirtup-x11/src/overlay_input.rs` | `ToolbarZone`, `PointerEvent`, `Routed`, `route(zone, event)`, `begin_move(...)` |
| T | `lirtup-effects/src/ui_draw.rs` | `UiDraw` (`new`, `set_scene`, `rebind`, `pass`, `measure`), `Shape` (rectángulo redondeado, texto, ícono), `UiFonts`, `UiFont` |
| Y | `lirtup-core/src/daemon.rs` | `DaemonHost` (lo implementa el motor), `Daemon::spawn` y `stop`, `watched(file)`, `pick(windows, file, ignored, is_lirtup)` |
| Y | `lirtup-core/src/lifecycle.rs` | `Lifecycle` (`connected`, `left`, `should_exit`), `LINGER`, `Resume` (`encode`, `take_from_env`) y `RESUME_ENV` |
| Y | `lirtup-ui/src/tray.rs`, `notify.rs` | `TrayStatus`, `status(store)`, `use_tray(store, dispatch)` y `use_notifications(store, dispatch)`. `app.rs` ya las llama: los hooks no hacen nada hasta Y |
| M | `lirtup-config/src/migrate.rs` | `MigrationPlan`, `needs_migration`, `plan`, `apply`, `BACKUP_SUFFIX` |
| M | `lirtup-ui/src/screens/migrate.rs`, `state` | `MigrateDialog(plan)`; `Dialog::Migrate`, `Action::Migrate`, `SkipMigration` y `Migrated` (en `update` con `todo!()`) |
| M | `lirtup-ui/src/i18n_check.rs` | Vacío, con su documentación: el test de traducciones completas |

`Runner::toggle_toolbar` y la rama de `OverlayNotice::Pointer` en `pipeline.rs` solo escriben en
el log, para que el atajo no tumbe la sesión antes de que T los complete.

## Reparto de archivos (la tabla de 5.6, ajustada)

| Frente | Toca | No toca |
| --- | --- | --- |
| **M. Migración y traducciones** | `lirtup-config/src/migrate.rs`; `lirtup-ui/src/screens/migrate.rs`; los brazos `Migrate`, `SkipMigration` y `Migrated` de `update` y el ofrecimiento después de `Loaded` en `state/store.rs`; `lirtup-ui/src/i18n_check.rs`; `scripts/ts-to-ftl*`; `i18n/*/*.ftl` (solo para completar el inglés y pasar las cadenas de la 1.x) | el resto del estado, `app.rs` |
| **T. Barra sobre el juego** | `lirtup-pipeline/src/{toolbar,tokens}.rs`; en `pipeline.rs`, lo que conecta la barra (`toggle_toolbar`, `OverlayNotice::Pointer`, la pasada en `frame`); en `surface.rs`, el ruteo en el hilo de la superposición; `lirtup-x11/src/overlay_input.rs`; `lirtup-effects/src/ui_draw*`; `lirtup-effects/src/osd.rs` y `lirtup-pipeline/src/{osd,stats}.rs` (panel y OSD con los tokens y las fuentes nuevas); `scripts/gen-tokens-icons.py` (las dos instancias estáticas de Material Symbols en `.ttf`) | la GUI, el núcleo |
| **Y. Bandeja, daemon, avisos y ciclo de vida** | `lirtup-core/src/{daemon,lifecycle}.rs`; en `engine.rs` y `server.rs`, lo que conecta el daemon, el ciclo de vida y `Resume`; `lirtup-cli/src/serve.rs`; `lirtup-fg/src/layer.rs` (`restart_self` con `Resume`); `lirtup-ui/src/{tray,notify}.rs`; en `app.rs`, solo los manejadores de cerrar y minimizar de la barra de título; las filas de bandeja de `screens/settings/mod.rs` (General); `start_delay_s` en `screens/home/activation.rs`; `i18n/*/settings.ftl` y `home.ftl`, solo las claves nuevas | `pipeline.rs`, lo de T |
| **P. Empaquetado y documentación** | `packaging/**`; `scripts/build-release-linux.sh`; `.github/workflows/**` (uno nuevo para Rust; el de wheels queda hasta la decisión 1); `README.md`; `docs/user/**`; `mkdocs.yml`; los íconos de `data/` | el código, la versión y el `CHANGELOG.md` |
| **Revisor** | Los dos `Cargo.toml` (el del workspace y el de cada crate), `package.json`, los dos archivos de bloqueo (`Cargo.lock` y `package-lock.json`), la versión, `CHANGELOG.md`, `assets/tailwind.css`, los tipos de `state/mod.rs`, `proto`, el resto de `app.rs`, y lo que esté en la lista de abajo como «Revisor» | — |

Orden de integración: **M → T → Y → P**, como en 5.6. Ya agregué `notify-rust` al `Cargo.toml`
de la GUI, para Y. La bandeja usa `dioxus::desktop::trayicon` y no necesita dependencias nuevas.

## Lo que pasa a esta ronda y a quién le toca

### De la nota de la ronda 3

| Pendiente | Quién |
| --- | --- |
| Hallazgo 1: Vulkan da negro en XWayland con KWin, igual que la 1.x. Documentarlo | P (README y `docs/user/`) |
| Hallazgos 2 y 4: ciclos de FG medidos con el reloj de la CPU y latencia media de FG integrado | T (mide la GPU en el mismo camino; si hace falta tocar `framegen.rs`, lo anota y lo hago yo en la integración) |
| `PauseHotkeys` | **Hecho** en el núcleo. La GUI lo manda mientras `HotkeyCapture` captura: Y, porque es un cambio en un componente de la ronda 3 y en `hotkeys.rs` de Ajustes |
| Ícono propio del perfil en el cajón (con caché o `use_asset_handler`) | Revisor, en la integración |
| Deshacer restaura solo `config.yaml`, no los modos | Revisor, en la integración |
| `Route::Modes { from }` para «Duplicar para este perfil» | Revisor, en la integración |
| `gui.start_delay_s` | Campo **hecho**; Y lo conecta en Inicio |
| `Action::Toast` para «Copiado» | Revisor, en la integración |
| Campos de la bandeja en `GuiConfig` | Campos **hechos**; Y los usa y los muestra en Ajustes |
| Versión de Tailwind de `dx` (4.1.5) contra la de `package.json` (4.3.3) | P: en CI, `dx build` con el CSS de `package.json` (`[application] tailwind_input` vacío o `--skip-assets`), o fijar la versión en `Dioxus.toml` |
| Test de traducciones completas y `.ts` → `.ftl` | M |

### De rondas anteriores

| Pendiente | Quién |
| --- | --- |
| El anillo por cuadro (las carreras de `set_params` y `set_frame`) | T lo mide al medir la barra: si la espera del principio de cada cuadro cuesta latencia, lo anota y lo hago yo. Si no, se cierra |
| `Conversion::create_into` y `Srcnn::new_into` (unos 0,1 ms a 4K) | Revisor, en la integración, si la medición de T lo justifica |
| `swapchain_buffers` en la configuración (hoy fijo en 2) | Revisor, en la integración |
| `restart_self` con variables, para retomar la sesión | Y (`Resume`) |
| `{timestamp}` de las capturas en hora local | Ver la lista de paridad |
| Mover los datos de `src/upscaler/…` | Revisor, solo si la decisión 1 retira Python |
| La lista de paridad con la 1.x | Abajo |

## Lista de paridad con la 1.x

LISTA_DE_PARIDAD

## Por frente

### M: migración y traducciones

- **Implementa**:
  - `migrate::{needs_migration, plan, apply}` (reglas en el doc del módulo);
  - el diálogo `MigrateDialog`;
  - en `update`: el ofrecimiento después de `Loaded` (si `needs_migration` y no
    `migration_declined`), `Migrate` → `Effect::Migrate`, `SkipMigration` → `SetGui` con
    `migration_declined`, y `Migrated` con un aviso;
  - `scripts/ts-to-ftl*`, para pasar las 526 cadenas de los `.ts` de la 1.x y completar las que
    falten;
  - el test de `i18n_check.rs`;
  - el inglés completo.
- **Usa**: `MODE_KEYS` y `resolve::resolve` de `lirtup-config`, para probar que resolver da lo
  mismo antes y después; `i18n::has_key`, que ya entiende variables; `lirtup migrate`, que ya
  existe.
- **Puede correr**:
  - `cargo test -p lirtup-config`;
  - `lirtup migrate --dry-run` sobre copias;
  - la GUI con `dx`, que ya funciona entera.
- **Criterios de 5.8**:
  - la migración de la configuración real de la persona (una copia) da los mismos resultados de
    escalado: un test que resuelve cada perfil antes y después y compara los `Config`;
  - inglés completo;
  - el test de claves en verde.

### T: barra sobre el juego

- **Implementa**:
  - `tokens::{color, ease}`;
  - `Toolbar` y el ruteo de `overlay_input`;
  - `UiDraw`;
  - la conexión en `pipeline.rs` (mostrar y ocultar, las acciones, `SaveSetting` del
    multiplicador, `BeginMove` en modo ventana);
  - el panel de stats y el OSD con los tokens y Google Sans Flex;
  - las dos instancias `.ttf` de Material Symbols.
- **Usa**: los tokens de A, `Event::SaveSetting` y `HotkeyAction::ToggleToolbar`.
- **Puede correr**: `lirtup run` sobre una ventana de prueba; la medición de la GPU con
  `show_stats`; `cargo test` con imágenes de referencia de la barra.
- **Criterios de 5.8**:
  - la barra no cambia los tiempos de GPU en más de 0,1 ms (medido con timestamps, con la barra
    visible y oculta);
  - aparece al acercarse al borde y se oculta a los 2 s;
  - en modo ventana, arrastrarla mueve la ventana;
  - un informe de los hallazgos 2 y 4.

### Y: bandeja, daemon, avisos y ciclo de vida

- **Implementa**:
  - `daemon` y su conexión con el motor (`SetDaemon`, `--daemon` y `DaemonState` con
    `watching` real);
  - `lifecycle` en `serve.rs` y `server.rs`;
  - `Resume` en `restart_self` y al arrancar;
  - `tray.rs` con los 4 estados y el menú del diseño;
  - `notify.rs`;
  - cerrar y minimizar a la bandeja, y empezar oculto;
  - las filas de la bandeja en Ajustes;
  - `start_delay_s` en Inicio;
  - `PauseHotkeys` desde `HotkeyCapture`.
- **Usa**: `GuiConfig` (campos nuevos), `X11::is_lirtup_window`, `find_matching_profile`.
- **Puede correr**: `lirtup serve --daemon`, la GUI con `dx` y `lirtup ctl`.
- **Criterios de 5.8**:
  - la bandeja muestra los 4 estados;
  - el daemon sigue escalando con la GUI cerrada;
  - con «Seguir ejecutándose tras salir» apagado, `serve` termina cuando sale la GUI;
  - `Restart` con la capa retoma la sesión.

### P: empaquetado y documentación

- **Implementa**:
  - en `packaging/`: el PKGBUILD de AUR, la AppImage, el `.desktop` con su metainfo y el
    lanzador con `WEBKIT_DISABLE_DMABUF_RENDERER=1`;
  - `scripts/build-release-linux.sh`;
  - el CI de Rust: fmt, clippy, test, `dx build` y `check-ftl-keys.py`;
  - el README y `docs/user/**` de la 2.0, con el hallazgo 1.
- **Usa**: los binarios `lirtup` y `lirtup-gui` (el bundle de `dx`), y los íconos de `data/`.
- **Puede correr**:
  - `makepkg` en un contenedor o una VM limpia;
  - la AppImage;
  - `act` para el CI, si está instalado.
- **Criterios de 5.8**: el paquete de AUR y la AppImage se instalan y arrancan en una máquina
  limpia (lo prueba la persona: va al guion).

## Al integrar (5.7)

Orden **M → T → Y → P**. Después de los merges:
- regenero `assets/tailwind.css` y los tokens;
- `check-ftl-keys.py` y el test de M;
- hago mi parte de la lista de arriba;
- Build 1 con `dx build --release`;
- una pasada de correcciones y Build 2;
- pruebo los criterios y escribo la nota.
