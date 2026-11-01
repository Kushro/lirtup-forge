# Ronda 3: el esqueleto del revisor

Rama **`rust/r3-skeleton`**, que sale de `rust/r2-integration`. Fecha: 2026-11-01. Los cuatro
frentes de la GUI (A, B, C, D) salen de esta rama. Sin push y sin merge a `main`.

- `cargo check --workspace --all-targets` (rustc 1.99.1): **pasa, sin avisos**.
- `cargo clippy -p lirtup-ui`: solo los 63 avisos de `todo!()` de las firmas, que se van con los
  cuerpos. El resto del workspace sigue sin avisos.
- `cargo test`:
  - `lirtup-ui` pasa sus 4 tests: rutas, traducciones (con plurales) y `CoreLink` contra un socket
    de prueba;
  - `lirtup-proto`, `lirtup-pipeline`, `lirtup-core` y `lirtup-cli` siguen en verde con los
    cambios de contrato.
- **`npm run tailwind:build`** (Tailwind 4.3.3, en lugar de `bun`, que no está instalado):
  **compila sin avisos**; el `@theme` genera `bg-surface-container-low`, `t-ts`, `press-morph`,
  etc. Lo mismo corre con `bun run tailwind:build`.

Notas anteriores: [rust-r0-contract.md](rust-r0-contract.md) (las reglas de siempre siguen) y
[rust-r2-integration.md](rust-r2-integration.md) (los pendientes que pasaron a esta ronda).

## Antes de lanzar los frentes: disco y `dx`

- **El disco DATOS está lleno.** Hoy quedan unos 4,6 GB libres, después de que borré la caché
  incremental de mi `target/`. Cada worktree compila todo el workspace, y la GUI suma WebKitGTK y
  Dioxus. Cuatro frentes en paralelo lo llenan. **Que cada frente use un `target` en `/home`**
  (287 GB libres):

  ```bash
  export CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r3-<frente>
  ```

  `npm install` también falló por falta de espacio. Instalé Tailwind en mi scratchpad y lo enlacé
  con un symlink `node_modules`, que está en `.gitignore`. Los frentes pueden hacer lo mismo o
  instalar en `/home`.
- **`dx` no está instalado.** Con `cargo run -p lirtup-ui` la ventana arranca. Pero los `asset!`
  (el CSS) los empaqueta `dx`, y sin él lo más probable es que la página salga sin estilos (no lo
  probé). Para que A vea su galería, y para el criterio de 5.8 (`dx build`), conviene instalar una
  sola vez, antes de lanzar los frentes:

  ```bash
  CARGO_TARGET_DIR=$HOME/.cache/dx-build cargo install dioxus-cli --version 0.7.10 --locked
  ```

  `dx` 0.7 encuentra el `tailwind.css` de la raíz del crate y lo compila solo a
  `assets/tailwind.css`, la misma ruta que usa `package.json`.

## Qué escribí

### `crates/lirtup-ui`

| Archivo | Estado | Qué |
| --- | --- | --- |
| `main.rs` | completo | Ventana sin marco de 1280×820 (mínimo 720×520), sin menú, fondo oscuro desde el primer cuadro, `LIRTUP_LOG` |
| `app.rs` | completo | `Store` y `CoreLink` como contextos, `use_store()` y `use_dispatch()`, el ejecutor de efectos, la barra de título, el cajón (riel por debajo de 1100 px), la página de la ruta, la barra de cambios, el snackbar, los diálogos y los atajos (Ctrl+S, N, D, F y Ctrl+Shift+G) |
| `route.rs` | completo | `Route`, `ProfileRef`, `ProfileTab`, `SettingsTab`, con un test |
| `link.rs` | completo | `CoreLink`: `Hello`, eventos como `Action::Core`, reconexión, cola de comandos mientras está caído, y arranque de `lirtup serve` con el primer comando. Tiene un test |
| `i18n.rs` | completo | `t!("clave")` y `t!("clave", n = v)` sobre `fluent-templates`, la señal global `LANG`, el español como respaldo, `has_key` para el test de la ronda 4. Tiene tests |
| `state/mod.rs` | tipos | `Store`, `Field<T>`, `Origin`, `Scope`, `ConfigDoc`, `ModesDoc`, `LinkState`, `SessionState`, `Dialog`, `Toast`, `NewProfile`, `Action` (todas las variantes) y `Effect`; `update()` delega en `store.rs` |
| `state/store.rs` | firmas, `todo!()` | Los métodos de `Field` y de `Store` (`field`, `origin`, `dirty_count`, `tab_dirty`, `profile_dirty`, `profiles`, `running_profile`, `matching_profile`, `mode_of`), `tab_keys`, `global_keys`, `apply_edits` y `update` (frente C) |
| `components/` | firmas, `todo!()` | Los componentes de 6.2 en 8 archivos, más `Gallery` (frente A) |
| `screens/` | firmas, `todo!()` | `Home`, `About` (B); `ProfilePage`, `UnsavedDialog`, `DeleteProfileDialog`, `save_bar_scope`, `NewProfileDialog` (C); `ModesPage`, `EffectPickerDialog`, `DeleteModeDialog`, `SettingsPage`, `BenchDialog` (D) |
| `windows.rs`, `thumbs.rs` | firmas que no hacen nada | `use_window_list` y `use_thumbnails` (frente B). `app.rs` las llama al arrancar, así que no pueden entrar en pánico |
| `theme.rs` | firmas que no hacen nada | `SCHEME`, `resolve` y `root_style` (frente A). `app.rs` las llama, igual que a las de B |
| `i18n/{es,en}/*.ftl` | `app.ftl` completo | Un archivo por frente; ya tienen las claves de las pestañas |
| `design/tokens.json` | completo | Los 39 roles por tema, forma, fuentes, la escala tipográfica y los resortes |
| `assets/tokens.css` | generado | Lo generé con un script mío de una sola vez. Lo reemplaza el `gen-tokens` de A |
| `tailwind.css` | completo | El `@theme inline` de las notas, con los 39 roles y no solo los del ejemplo, más `press-morph`, la escala `t-*`, `mono`, `dot`, el foco de 3 px y `prefers-reduced-motion` |
| `assets/main.css` | vacío | Para A: `@font-face` y las clases de componentes que no sean utilidades |
| `assets/tailwind.css` | generado | Va en git porque `asset!` lo exige para compilar; ver «Reparto» |
| `package.json` y `package-lock.json` | completos | `tailwind:build` y `tailwind:watch` con `@tailwindcss/cli` 4.3.3 |

**Sobre `tokens.json`: el lienzo no tiene un `tokens.json`.** Sus archivos son las 13 mesas de
trabajo y un `lf.css`, igual al `docs/design/lf.css` de la rama de diseño. El `tokens.json` lo
saqué de ese `lf.css`, que el README del diseño trata como la fuente de los tokens. Lo que el
README dejó pendiente, generar los colores desde la semilla con `material-colors` y compararlos con
el lienzo, queda para A.

Traje `docs/design/{README.md,lf.css,contrast.py}` con
`git checkout claude/design-system-1937a8 -- …`, sin merge. `contrast.py` sobre ese `lf.css` da
0 fallos de 54 en cada tema.

### Cambios de contrato, ya implementados

Esta ronda no tiene un frente del núcleo, así que estos cambios no quedan como firmas: van
completos, con tests (commit `5aacf13a`).

1. **`FrameGenStatus.reason_code: Option<FrameGenReason>`**, con `#[serde(default)]`. Los valores
   son `LayerMissing`, `LosslessDllMissing`, `LsfgVkDllMissing`, `EnvLocked`, `NoExternalFd` y
   `EngineFailed`. La GUI traduce el código, y `reason` queda como detalle en inglés (por ejemplo,
   el error de lsfg). Lo llenan `status_from_setup`, `FrameGen::new` (cuando no hay fd o falta la
   DLL) y `FrameGen::fail`.
2. **`Event::EffectCompile { …, session: bool }`**, con `#[serde(default)]`. El presentador anota
   los efectos MagpieFX que desactiva por un error de compilación o de enlace
   (`Presenter::take_disabled`). La sesión los manda en `poll_stats` como
   `EffectCompile { done: true, error, session: true }`. `lirtup ctl compile-effect` solo espera
   los eventos con `session: false`.
3. **`Command::WatchGpuTimes { on }`**: el reemplazo de `PipelineCommand::SetTimestamps` como
   comando de la GUI.
   - El servidor guarda el estado de cada conexión, así que una conexión que se cierra deja de
     contar sola.
   - `Engine::watch_gpu_times` cuenta cuántas conexiones lo piden. Cuando la cuenta pasa por 0,
     manda **`PipelineCommand::SetTimestamps(bool)`** a la sesión. `SessionSetup.timestamps` da el
     estado inicial.
   - La sesión toma timestamps de Vulkan solo si alguien mira o si el panel de stats está
     visible. Al apagarlos borra los tiempos viejos (`Stats::clear_gpu_times`).
   - Consecuencia: sin nadie mirando y sin panel, `gpu_ms` vale 0 y no hay aviso de presupuesto
     de GPU. El aviso es para la tarjeta de sesión y el panel, que ya piden tiempos.

Además, `ConfigFile` y `ModeStore` derivan `PartialEq`, porque el `Store` los contiene.
`PROTOCOL_VERSION` sigue en 1: los campos nuevos tienen valor por defecto, y la GUI y el
escalador se instalan juntos.

## Decisiones

- **Sin `dioxus-router`.** La ruta es un campo del `Store`, y `app.rs` elige la página con un
  `match`. Navegar es `Action::Navigate`, así que `update()` puede frenar la navegación con el
  diálogo de cambios sin guardar, y eso se prueba sin Dioxus. Una ventana de escritorio no tiene
  URL. El «router» de 5.5 es ese `match`.
- **Un solo `Signal<Store>`.** El idioma (`i18n::LANG`) y el esquema de color
  (`theme::SCHEME`) son `GlobalSignal` aparte, como pide 6.3. Que sean globales permite usar
  `t!` en cualquier función, no solo en un componente.
- **`update()` es puro y devuelve `Option<Effect>`.** Las escrituras de YAML vuelven como
  `Action::Written`, y `ReloadConfig` sale recién después de una escritura correcta. Hay
  `Effect::Many` para los casos que necesitan varios efectos. El diálogo vive en `Store::dialog`:
  `update` lo pone directamente, sin un `Effect::Confirm` de por medio.
- **Los ajustes se identifican por su clave YAML.** `Store::field::<T>(scope, key)` arma el
  `Field<T>` de 6.3 a partir de la capa como se leyó más las ediciones
  (`IndexMap<(perfil, clave), Option<Value>>`). Así las claves que la GUI no conoce sobreviven
  igual que con `lirtup-config`, y guardar es fusionar las ediciones sobre los `Mapping`.
- **Los textos del `Store` son claves de Fluent con argumentos** (`Toast`), porque `update` no
  sabe el idioma.
- **`ModesDoc` en lugar de `Vec<ModeDoc>`.** La `ModeStore` como se leyó, las ediciones, y qué
  perfiles usan cada modo (para el aviso de modo compartido).
- **`CoreLink` arranca `lirtup serve` con el primer comando**, si no hay socket: busca `lirtup`
  junto a `lirtup-gui` y si no lo encuentra usa el del `PATH`. Mientras no hay socket guarda hasta
  32 comandos. Con la capa lsfg-vk, `Restart` hace `exec` del escalador: el socket se cae y el
  link se reconecta solo.
- **Galería**: con `LIRTUP_GUI_GALLERY=1`, la app muestra solo `Gallery`, sin store, link ni
  pantallas. Así A puede probar sus componentes antes de que existan C y las pantallas. Con la app
  normal, la galería también se abre con Ctrl+Shift+G.
- **Cerrar la ventana sale de la app** en esta ronda. Cerrar a la bandeja llega con la bandeja
  (ronda 4, frente Y).

## Lo que pasó de la ronda 2 y a quién le toca

Ningún frente de la GUI toca el núcleo. Lo que no se puede postergar lo hago yo en la integración
de la ronda 3, y el resto pasa a la ronda 4.

| Pendiente | Quién | Cuándo |
| --- | --- | --- |
| Código de motivo en `FrameGenStatus` | Revisor | **Hecho** en este esqueleto. C lo traduce en `FrameGenSection` (`profile.ftl`) |
| `EffectCompile` para los errores de compilación del presentador | Revisor | **Hecho** en este esqueleto. D lo muestra en la cadena del modo (efecto desactivado, con el error) |
| Timestamps solo con alguien mirando | Revisor (núcleo), C (GUI) | Núcleo **hecho**. C: `update` manda `WatchGpuTimes { on }` cuando cambia «ventana visible y sesión corriendo y ruta Inicio», usando `Store::gpu_times` |
| P: `OverlayCommand::SetTarget` en `SwitchWindow` y `follow_focus`; `HideCursorAfter` para `hide_cursor` | Revisor | Integración de la ronda 3 |
| `StartFullscreen` no toma la GUI ni una superposición vieja (filtrar `_LIRTUP_OVERLAY` y la clase `lirtup`) | Revisor | Integración de la ronda 3. B filtra lo mismo en `windows.rs` |
| `bench` con modos (`BenchRequest.mode`), `sizes` y `selectors` en `bench.json` | Revisor | Integración de la ronda 3, antes de probar el diálogo de D |
| Hallazgo 1: la captura de ventanas Vulkan en XWayland da negro | Revisor | Integración de la ronda 3: lo pruebo a mano contra 1.x. Si 1.x también da negro, es una limitación de KWin y va al README en la ronda 4 (frente P) |
| Hallazgo 2: los ciclos de FG medidos con el reloj de la CPU | Ronda 4, frente T | T mide el costo de GPU de la barra y toca el mismo camino |
| Hallazgo 3: separar las esperas que suma «render» en el panel | Revisor | Integración de la ronda 3 (`stats.rs` y `pipeline.rs`) |
| Hallazgo 4: latencia media de FG integrado unos 2,5 ms más alta | Ronda 4, frente T | Junto con el hallazgo 2 |
| 6.7: ajustes por perfil que hoy son globales | C | `global_keys()` y la fila con «Global» |

## Reparto de archivos (la tabla de 5.5, ajustada)

Actualicé la tabla en `rust-refactor-analysis.md`. Los cambios:

- **A** suma `tailwind.css`, `design/tokens.json`, `assets/tokens.css` (que genera su script) y la
  galería.
- **C** pasa a llamarse «Perfiles y estado». Escribe los cuerpos de `state/store.rs` y puede
  **agregar** tipos o variantes a `state/mod.rs`, pero no cambiar ni borrar los que hay, porque B
  y D programan contra ellos. Lo que agregue va en su nota.
- **Revisor**: los dos `Cargo.toml` (el del workspace y el de cada crate), `package.json` y los dos
  archivos de bloqueo (`Cargo.lock` y `package-lock.json`), `assets/tailwind.css`,
  `main/app/route/link/i18n.rs`, `screens/mod.rs`, los tipos de `state/mod.rs`, `app.ftl` y todo
  lo que no es la GUI.
- **Dependencias**: un frente que necesite una dependencia nueva de Rust o de npm la pide en su
  nota, y la agrego yo al integrar. Mientras tanto la puede agregar en su rama para probar, pero
  sin subir ese cambio.
- **`assets/tailwind.css`**: va en git porque `asset!` necesita que el archivo exista para
  compilar. Cada frente lo regenera para ver sus clases, pero no lo sube (antes de cada commit,
  `git checkout -- crates/lirtup-ui/assets/tailwind.css`). Lo regenero yo al integrar.

Ya están en el `Cargo.toml` de la GUI `dioxus` (desktop), `lirtup-config`, `lirtup-proto`,
`lirtup-x11`, `tokio`, `serde`, `serde_json`, `serde_yaml_ng`, `indexmap`, `tracing`,
`tracing-subscriber`, `fluent-templates`, `material-colors` (para A) e `image` (para las miniaturas
de B). `PathField` necesitaría `rfd` para un selector nativo: si A lo quiere, que lo pida.

## Qué puede correr cada frente

Las firmas de `components/` y de `store.rs` están en `todo!()`, así que **la app normal entra en
pánico hasta que estén integrados A y C** (orden A → C → B → D).

| Frente | Puede correr | Cómo |
| --- | --- | --- |
| A | la galería | `LIRTUP_GUI_GALLERY=1 dx serve -p lirtup-ui` (o `cargo run`, posiblemente sin CSS si no hay `dx`) |
| B | `cargo check` y sus tests | Para mirar su pantalla puede hacer merge local de A y C sin subirlo. Los tests de `windows.rs` y `thumbs.rs` corren contra X11 real |
| C | `cargo check` y **los tests de `update()`** | Es todo lógica pura; la página se ve después de integrar A |
| D | `cargo check` y sus tests | Igual que B |

Todos: `cargo check -p lirtup-ui`, `cargo clippy -p lirtup-ui` sin avisos en sus archivos, y
`cargo test -p lirtup-ui`.

## Por frente

### A: componentes y tema

- **Toca**: `src/components/**`, `src/theme.rs`, `tailwind.css`, `assets/main.css`,
  `assets/tokens.css`, `assets/fonts/**`, `design/tokens.json`, `scripts/gen-tokens*`,
  `scripts/contrast.py`, `i18n/*/components.ftl`.
- **No toca**: `app.rs`, `state/`, `screens/`, `Cargo.toml`, `package.json`.
- **Implementa**:
  - los cuerpos de `Button`, `IconButton`, `SplitButton`, `Connected<T>`, `Switch`, `Checkbox`,
    `Slider`, `Select<T>`, `TextField`, `PathField`, `ColorField`, `FontField`, `SettingRow`,
    `SettingGroup`, `NavDrawer`, `NavHeader`, `NavItem`, `Tabs<T>`, `WindowTile`, `Keycap`,
    `HotkeyCapture`, `FloatingToolbar`, `Snackbar`, `Dialog`, `Menu`, `Tooltip`, `Badge`, `Chip`,
    `Icon`, `LoadingIndicator` y `Gallery`;
  - `theme::resolve` y `theme::root_style` (con `material-colors`, desde la semilla);
  - `gen-tokens`: `tokens.json` → `assets/tokens.css` y `tokens.rs` para la ronda 4;
  - `contrast.py` leyendo `tokens.json`.
- **Usa**: `state::Origin` (en `SettingRow`), `lirtup_x11::WindowInfo` (en `WindowTile`) y
  `lirtup_config::Hotkey` (en `HotkeyCapture`).
- **Notas**:
  - Las props de las firmas son el contrato con B, C y D. Puede agregar props con valor por
    defecto, pero no cambiar las que hay sin anotarlo.
  - Las clases base de cada componente están en las notas del lienzo (mesa «Dioxus + Tailwind») y
    en `docs/design/lf.css`.
  - Quitar el `#![allow(unused_variables)]` de cada archivo al escribirlo.
- **Criterios de 5.8**:
  - `contrast.py` en verde con los tokens finales;
  - Tailwind compila sin avisos;
  - la galería muestra cada componente con sus variantes en los dos temas;
  - todo se usa con el teclado, con anillo de foco visible;
  - `prefers-reduced-motion` respetado;
  - ningún texto de `components.ftl` sin traducir al español.

### B: Inicio, Acerca de, ventanas y miniaturas

- **Toca**: `src/screens/home/**`, `src/screens/about.rs`, `src/windows.rs`, `src/thumbs.rs`,
  `i18n/*/home.ftl`, `about.ftl`.
- **No toca**: los componentes (solo los usa), `state/`, `app.rs`.
- **Implementa**:
  - `Home`: tarjeta de sesión con sus estados y error (Reintentar), activación con los atajos de
    la configuración y «Empezar» con escalado diferido, interruptor del daemon, cuadrícula con
    filtro rápido y búsqueda, menú del mosaico (en ventana, crear perfil, copiar título) y
    flechas con Intro;
  - `About`: versión, enlaces, licencias y diagnóstico copiable;
  - `use_window_list`: un hilo propio de `lirtup-x11`, cada `gui.auto_refresh_ms`, que manda
    `Action::Windows` solo si la lista cambió, sin la GUI ni la superposición (`_LIRTUP_OVERLAY`);
  - `use_thumbnails`: los `use_asset_handler` `thumb` e `icon`, el hilo de XComposite a 320 px, y
    la pausa con la ventana oculta o minimizada (eventos de `tao` con `use_wry_event_handler`).
- **Usa**:
  - `use_store`, `use_dispatch` y `FOCUS_FILTER` (Ctrl+F) de `app.rs`;
  - `Store::matching_profile`, `running_profile` y `session` de C;
  - `Action::StartScaling`, `StopScaling`, `RestartScaling`, `SetDaemon`, `RequestDiagnostics` y
    `OpenDialog(Dialog::NewProfile { from })`;
  - `WindowTile` y el resto de los componentes de A.
- **Criterios de 5.8**:
  - flujo 1 (escalar una ventana) de punta a punta contra `lirtup serve`: mosaico, atajo o
    diferido, la tarjeta, Detener;
  - la parte de Inicio del flujo 6 (el interruptor del daemon y su estado);
  - cuadrícula navegable con el teclado;
  - textos de `home.ftl` y `about.ftl` en español.

### C: perfiles y estado

- **Toca**: `src/state/store.rs`, `src/state/mod.rs` (solo para agregar),
  `src/screens/profile/**`, `src/screens/new_profile.rs`, `i18n/*/profile.ftl`.
- **No toca**: los componentes, las pantallas de B y D, `app.rs`.
- **Implementa**:
  - todo `store.rs`: `Field::value`, `origin`, `is_dirty`; `tab_keys` y `tab_of` (el reparto de
    claves de `Config` en las 5 pestañas); `global_keys`; `apply_edits` y `apply_mode_edits`; los
    métodos de `Store`; y `update` para **todas** las `Action`, también las de B y D (las de
    modos, bench y efectos son simples: mandar el comando o guardar el evento);
  - `ProfilePage` con las 5 pestañas, la cabecera con chips de reglas y «Editar perfil», y
    `FrameGenSection`, que lee `FrameGenStatus` y traduce `reason_code`;
  - `UnsavedDialog`, `DeleteProfileDialog`, `save_bar_scope` y `NewProfileDialog`, con
    `PickWindow` y la vista previa de coincidencia.
- **Reglas que tiene que cumplir `update`**:
  - navegar a otra página con cambios en la actual abre `Dialog::Unsaved`; cambiar de pestaña no;
  - `Save` escribe con `lirtup-config` (nunca a mano) y guarda en `Store::undo` el archivo de
    antes; `ReloadConfig` sale recién con `Written(Ok(Config))`;
  - `Core(MenuBarDetected)` guarda el recorte en el perfil;
  - `WatchGpuTimes` según la regla de la tabla de pendientes;
  - el resto de los eventos actualiza el `Store`.
- **Usa**: `Route` y `ProfileRef`; los componentes de A; `lirtup_config::{ConfigFile, Profile,
  Match, Mapping}`; `find_matching_profile` para `matching_profile`.
- **Criterios de 5.8**:
  - flujo 2 (crear un perfil), flujo 3 (editar y guardar con Deshacer) y flujo 5 (la generación de
    cuadros desde Presentación);
  - un perfil de la 1.x se abre y se guarda sin perder campos: el diff del YAML queda vacío salvo
    lo cambiado (con los perfiles reales de la persona, en una copia);
  - herencia y sucio correctos en fila, pestaña y cajón;
  - tests de `update()` en verde, uno por regla de arriba como mínimo.

### D: modos, Ajustes y medición

- **Toca**: `src/screens/modes/**`, `src/screens/settings/**`, `src/screens/bench.rs`,
  `i18n/*/modes.ftl`, `settings.ftl`.
- **No toca**: lo de A, B y C.
- **Implementa**:
  - `ModesPage`: la lista con costo y uso, y el editor con `ModelSlider` (un `Slider` con
    `stops`), escalado doble, muestreo con `Disclosure`, `Connected` para el motor, la cadena con
    arrastre HTML5 más subir y bajar con el teclado, el costo por efecto, los controles por
    `EffectParamInfo`, el aviso de modo compartido (`DuplicateMode`) y el retoque final;
  - `EffectPickerDialog`, con «Sin probar» según `verified` y los efectos de la persona;
  - `DeleteModeDialog`;
  - `BenchDialog` (`RunBench`, progreso, resultado, Cancelar);
  - `SettingsPage`: General (idioma, tema, semilla, bandeja, daemon), Atajos (grupos con
    `HotkeyCapture`) y Estilo. Escribe al instante con `SetGui`.
- **Usa**:
  - `Scope::Mode` con `SetField`, `Inherit` y `Revert` para editar modos;
  - `Store::modes`, `effects`, `compiling` (los errores `session: true` marcan el efecto como
    desactivado) y `bench`;
  - los componentes de A.
- **Criterios de 5.8**:
  - flujo 4 (modo y efectos: agregar, reordenar, medir, en vivo con `ReloadConfig`);
  - Ajustes navegable con el teclado y `HotkeyCapture` funcionando;
  - textos de `modes.ftl` y `settings.ftl` en español.
- **Aviso**: hasta que yo integre el `bench` con modos, `BenchRequest.mode` mide solo los 3
  nativos de 1.x. El diálogo se prueba entero después de la integración.

## Al integrar (5.7)

Orden **A → C → B → D**. Después de los merges:

- regenero `assets/tailwind.css`;
- junto los `.ftl` y paso las cadenas de los `.ts` de 1.x con un script;
- hago la pasada del núcleo de la tabla de pendientes;
- Build 1, que en esta ronda suma `dx build --release`.
