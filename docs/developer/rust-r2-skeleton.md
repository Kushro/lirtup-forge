# Ronda 2: el esqueleto del revisor

Rama **`rust/r2-skeleton`**, que sale de `rust/r1-integration`. Fecha: 2026-11-01. Los cuatro
frentes de la ronda 2 (S, P, F, K) salen de esta rama. Sin push y sin merge a `main`.

`cargo check --workspace --all-targets` (rustc 1.99.1): **pasa, sin avisos**.

Notas anteriores: [rust-r0-contract.md](rust-r0-contract.md) (las reglas de siempre siguen) y
[rust-r1-integration.md](rust-r1-integration.md) (lo que pasó a esta ronda).

## Qué escribí

- **`lirtup-pipeline`**: las firmas públicas de los 13 módulos, con documentación y `todo!()`. El
  doc de `lib.rs` tiene el diagrama de quién habla con quién.
  - **Dos módulos nuevos:**
    - **`error`**: `PipelineError` con su `stage()`. Ya está implementado y es del revisor.
    - **`surface`**: el hilo de la superposición y el swapchain sobre ella. Es del frente K.
  - `#![allow(unused_variables)]` en `lib.rs`. Lo saco yo en la integración, porque los frentes
    no tocan `lib.rs`.
- **`lirtup-x11::overlay`**: la conexión de la superposición pasó a `Arc<XCBConnection>`, y hay un
  tipo nuevo, **`OverlaySurface`** (la conexión más el id de la ventana; `Send + Sync`), que se saca
  con `OverlayWindow::surface()`. Ya está implementado. Este es el punto de conexión con
  `lirtup-gpu`.
- **`lirtup-pipeline::surface::PresentSurface::create`**, ya implementado: es el único `unsafe` del
  crate y convierte un `OverlaySurface` en `XcbTarget`. Es seguro porque `PresentSurface` guarda una
  copia del `OverlaySurface` y la suelta después del swapchain (por el orden de los campos).
- **`lirtup-core::engine`**, en el doc del módulo:
  - el orden de arranque, con `layer::prepare` antes de todo;
  - qué usa de `lirtup-pipeline`;
  - una tabla de qué hace cada `Command`.

  Al `Cargo.toml` le agregué `lirtup-gpu` y `lirtup-magpiefx`.
- **`lirtup-cli`**: el subcomando `lirtup bench --json <REQUEST> [--gpu]` (su cuerpo es de K). Al
  `Cargo.toml` le agregué `lirtup-pipeline`, `lirtup-proto`, `lirtup-gpu` y `serde_json`.

## Cómo encajan las piezas

```text
 Engine (K, lirtup-core) ──SessionSetup──▶ Session (P, un hilo por sesión)
       │ PipelineCommand                     │ es dueño de: Capture, Upscaler, Presenter,
       ▼                                     │   Osd, StatsPanel, Stats, MenuBarDetector
 proto::Event ◀─── crossbeam Sender<Event> ──┤
                                             ├─▶ OverlayThread (K): OverlayWindow + InputForwarder
                                             │     OverlayCommand ─▶ ◀─ OverlayNotice
                                             └─▶ Presenter (P) ──▶ SharedSurface = Arc<Mutex<PresentSurface>> (K)
                                                    │                     ▲
                                                    └─▶ FrameGen (F) ──── hilo lsfg-present
 Cada present (real o generado) ──PresentSink──▶ Stats (S) ──▶ LatencyLog (S)
```

El flujo de un cuadro, como `_process_one_frame` de 1.x, en el hilo de la sesión (P):

1. Espera el present anterior.
2. `Capture::grab` (X).
3. `MenuBarDetector::check` (S), mientras no haya decidido.
4. `Osd::update` y `StatsPanel::update` (S).
5. Detección de cuadro sin cambios.
6. `Upscaler::process` (P).
7. `Presenter::present(&[osd.pass(), panel.pass()], timestamps)` (P). Por dentro, el presentador
   llama a `FrameGen::present` (F) o presenta directo en la `SharedSurface` (K).
8. `Stats::poll` (S), que devuelve `Event::Stats` cada 500 ms.
9. `OverlayCommand::SetMapper` (K) si cambió el rectángulo del contenido.
10. Las notificaciones de la superposición: `Configured` lleva a `Presenter::resize` y, después,
    a `Osd::rebind` y `StatsPanel::rebind` con `presenter.screen()`.

## Decisiones

1. **La superposición corre en su propio hilo** (`OverlayThread`, K), con sus eventos y el reenvío
   de entrada, como en 3.7. El hilo de la sesión nunca toca la ventana directamente: le manda
   `OverlayCommand`s y lee `OverlayNotice`s. Así el ratón no espera al cuadro.
2. **El swapchain se comparte como `Arc<Mutex<PresentSurface>>`.** Lo usan el hilo de la sesión y
   el hilo `lsfg-present` de F, que es lo mismo que hace 1.x. Se toma el lock solo alrededor de un
   present o de un `recreate`.
3. **Los atajos globales no pasan por la superposición.** `GrabKey` sobre la raíz gana aunque la
   superposición tenga el foco del teclado. `Engine` reparte los atajos:
   - `StartFullscreen`, `StartWindowed` y `ExitApp` los maneja él;
   - el resto va como `PipelineCommand::Hotkey`.
4. **Los presents se informan por canal** (`PresentSink`, que es `Clone + Send` y nunca bloquea),
   desde cualquier hilo y con el tiempo de captura. `Stats` lo vacía en el hilo de la sesión y
   escribe el CSV de latencia.
5. **La medición corre en un proceso hijo** (`lirtup bench --json`), como en 1.x. Hay dos motivos:
   lsfg admite un solo motor por proceso (no puede medir con una sesión abierta), y si el driver se
   cae mientras mide, no se lleva puesto al escalador. Cada línea del hijo es un `Envelope<Event>`
   (`BenchProgress`, `BenchResult`); `Engine` las reenvía, y `CancelBench` le manda SIGTERM. La
   medición usa `lirtup-effects`, `lirtup-magpiefx` y `lirtup-fg` directamente, no el presentador.
   Así S no depende de P.
6. **El tipo de sesión elige el modo de la superposición:**
   - `Windowed` siempre usa `windowed`;
   - `Fullscreen` usa el `overlay_mode` configurado, o `fullscreen` si el configurado es
     `windowed`.
7. **Los eventos salen del pipeline como `proto::Event`** por un `crossbeam_channel::Sender`.
   `Engine` los pasa al `broadcast`. El pipeline no sabe nada del socket.
8. **Los errores llevan su etapa**: `PipelineError::stage()` da el `Stage` de `SessionError`.
9. **`lirtup_fg::layer::prepare` va primero.** Lo llama `Engine::new` antes del runtime de tokio,
   de los hilos y de cualquier `Instance::new`. `prepare` ya pone `DISABLE_LSFGVK=1` cuando no se
   usa la capa, así que el aviso del cargador que vimos en la ronda 1 desaparece en la app. Solo
   queda cuando la capa está en uso, y ahí es a propósito.

## Lo que pasó de la ronda 1 y a quién le toca

| Pendiente | Dónde se resuelve | Frente |
| --- | --- | --- |
| `DISABLE_LSFGVK` antes de `Instance::new` | `Engine::new`: `layer::prepare` es el paso 2, antes de tokio, los hilos y la GPU (decisión 9). En `lirtup run` igual | K |
| Carreras entre CPU y GPU de E (`set_params`, `set_frame`, `set_dst_rect`, `set_image`) | **Sin anillo por cuadro.** El presentador llama a esos métodos solo después de esperar el envío anterior (la espera del principio de cada cuadro, como 1.x). Está en el doc de `presenter`. Si al medir esa espera cuesta latencia, el anillo pasa a la ronda 4 | P |
| Paridad de la salida de E con Python | Tests `#[ignore]` nuevos en `crates/lirtup-effects/tests/parity.rs` y `crates/lirtup-magpiefx/tests/parity.rs`. Comparan contra PNG de referencia hechos por un script nuevo, `scripts/rust-parity-refs.py`, que corre la 1.x: SRCNN, los escaladores, los efectos nativos y los 86 efectos verificados, sobre la imagen RGB de prueba. Le toca a S porque ya arma la imagen de prueba y corre efectos sin ventana para la medición. Los corro yo en la integración | S |
| Swapchain sin probar | Se prueba en la integración, con la superposición de K y el presentador de P | K y P |
| XTest en `InputForwarder` | **Por ahora solo `SendEvent`.** XTest mueve el puntero real; solo hace falta si un juego ignora los eventos sintéticos. Si pasa en las pruebas de esta ronda, K lo agrega con una opción; si no, queda en la lista de paridad de la ronda 4 | K |
| Test de ida y vuelta del protocolo | `crates/lirtup-proto/tests/roundtrip.rs`: cada `Command` y cada `Event` pasan por `encode_line` y `decode_line` y quedan iguales; además, una versión distinta da `CodecError::Version` | K |
| `todo!()` de `lirtup-cli` y `lirtup-core` | Son el entregable de K | K |
| `glxgears` sin PID ni clase | **No es un error.** Lo verifiqué con `xprop`: `glxgears` no publica `WM_CLASS` ni `_NET_WM_PID` | — |

## Reparto de archivos (la tabla de 5.4, ajustada)

Cambios respecto de 5.4:

- K toma `surface.rs` (nuevo), `lirtup-x11/src/input.rs` (además de `overlay.rs`) y los tests de
  `lirtup-proto`.
- S toma los tests de paridad de E.
- `lib.rs` y `error.rs` de `lirtup-pipeline` son del revisor.

También actualicé la tabla en `rust-refactor-analysis.md`.

| Frente | Toca (solo esto) | No toca |
| --- | --- | --- |
| **S. Stats y OSD** | `crates/lirtup-pipeline/src/{stats,osd,latency,menubar,bench}.rs`; los archivos nuevos `crates/lirtup-effects/tests/parity.rs`, `crates/lirtup-magpiefx/tests/parity.rs` y `crates/*/tests/fixtures/parity/**`; `scripts/rust-parity-refs.py` | Los `src/` de `lirtup-effects` y `lirtup-magpiefx`; el resto de `lirtup-pipeline` |
| **P. Pipeline** | `crates/lirtup-pipeline/src/{pipeline,tiles,upscale,presenter}.rs` | `surface.rs`, `framegen.rs`, los módulos de S, `lib.rs`, `error.rs` |
| **F. FG en el pipeline** | `crates/lirtup-pipeline/src/{framegen,pacing}.rs`, `crates/lirtup-fg/**` | El resto de `lirtup-pipeline`; `lsfg-sys` (si su API tiene que cambiar, lo pide en su nota) |
| **K. Core, socket y CLI** | `crates/lirtup-core/**`, `crates/lirtup-cli/**`, `crates/lirtup-pipeline/src/surface.rs`, `crates/lirtup-x11/src/{overlay,input}.rs`, `crates/lirtup-proto/tests/**` | `crates/lirtup-proto/src/**` (el contrato: los cambios se piden), el resto de `lirtup-x11` y de `lirtup-pipeline` |
| **Revisor** | `Cargo.toml`, `Cargo.lock`, `crates/lirtup-pipeline/src/{lib,error}.rs`, `lirtup-proto/src`, y lo que pidan las notas | — |

Nadie es dueño de `lirtup-gpu`, `lirtup-config`, `lirtup-effects/src` ni `lirtup-magpiefx/src` en
esta ronda. Un cambio ahí se pide en la nota de la rama y lo hace el revisor en la integración.
Orden de merge: **S → P → F → K**.

Las firmas que cruzan frentes no cambian sin pedirlo. Las internas de un frente (las de
`upscale`, `tiles` y `pacing`, y lo que el doc marque como libre) las puede cambiar su dueño.
Mientras el otro frente no termina, sus funciones son `todo!()`: el código compila igual, y
anda después del merge.

## Por frente

### S: stats, OSD, latencia, barra de menú y medición

**Implementa:**
- `stats`: `FrameKind`, `PresentRecord`, `PresentSink::record`, `StatsOptions`, `Stats` (`new`,
  `sink`, `record_capture`, `record_render`, `record_dirty`, `set_gpu_times`, `set_multiplier`,
  `set_refresh_hz`, `poll`, `latest`, `close`), `gpu_budget_warning`, `PanelContext` y
  `StatsPanel` (`new`, `visible`, `set_visible`, `update`, `rebind`, `pass`).
- `latency`: `HEADER`, `LatencyLog` (`create`, `write`, `flush`).
- `osd`: `Osd` (`new`, `show`, `update`, `update_config`, `rebind`, `pass`).
- `menubar`: `detect_menu_bar`, `MenuBarCheck`, `MenuBarDetector` (`new`, `check`).
- `bench`: `BenchCase`, `BenchSettings`, `BenchProgress`, `build_cases`, `test_image`, `run`,
  `results_path`, `save_results`, `results_for_gpu`.
- La paridad de E (tabla de arriba).

**Usa:**
- `lirtup_effects::osd::{TextRenderer, OverlayBlend}` y `Pass`;
- `lirtup_gpu::{Timestamps, Device, Texture}`;
- `lirtup_proto::{StatsSample, StageTime, GpuWarning, Bench*}`;
- `lirtup_effects`, `lirtup_magpiefx` y `lirtup_fg::native` para la medición;
- `lirtup_config::paths` para `bench.json`.

No depende de nada de P, F ni K.

**Lo usan:** P (todo lo de stats, OSD y barra de menú en el bucle), F (`PresentSink`) y K (`bench`
en el subcomando).

**Criterios de 5.8:**
- panel de stats y CSV de latencia iguales en contenido a los de 1.x;
- los tests de paridad de E en verde contra las referencias de Python.

### P: el pipeline

**Implementa:**
- `pipeline`: `SessionSetup`, `PipelineCommand`, `Session` (`start`, `send`, `view`,
  `is_finished`, `stop`), el bucle y el controlador de 1.x (zoom, desplazamiento, modelo,
  geometría, captura de pantalla, pausa, foco, `follow_focus`, `max_fps`,
  `present_on_change_only`).
- `upscale`: `Upscaler`, `UpscaleOutcome`.
- `tiles`: `TileProcessor`.
- `presenter`: `Presenter` (`new`, `screen`, `set_source`, `present`, `present_unchanged`,
  `resize`, `update_config`, `set_view`, `mapper`, `chain`, `framegen`, `framegen_mut`,
  `screenshot`, `close`), `PresentOutcome`.

**Usa:**
- `lirtup_x11::Capture` (captura), `X11::active_window` (foco) y `monitor_area`;
- `lirtup_effects` (SRCNN, `Srcnn::new_tiled`, escaladores, efectos, conversiones);
- `lirtup_magpiefx` (la cadena);
- de S: `Stats`, `Osd`, `StatsPanel`, `MenuBarDetector` y `PresentSink`;
- de F: `FrameGen::new`, `mark_capture`, `present`, `is_active`, `resize`, `update_config` y
  `status`;
- de K: `OverlayThread::spawn`, `send` y `notices`, y `PresentSurface::create`.

El hilo de la sesión crea el `FrameGen` y se lo pasa a `Presenter::new`.

**Lo usa:** K (`Session`).

**Criterios de 5.8:**
- `lirtup` escala una ventana en los cuatro modos de superposición, con tiles (junto con K);
- los tiempos de GPU de la tabla del README, dentro de un 5 % de la versión de Python.

### F: generación de cuadros en el pipeline

**Implementa:**
- `framegen`: `FrameGenSetup`, `FrameGen` (`new`, `mark_capture`, `present`, `is_active`,
  `active_multiplier`, `set_refresh_hz`, `resize`, `update_config`, `status`, `close`) y
  `status_from_setup`.
- `pacing`: `FramePacer`, `sleep_until` y `MIN_INTERVAL`, que puede cambiar libremente.
- Lo que haga falta en `lirtup-fg`: el hilo `lsfg-present`, los ciclos, el reinicio en segundo
  plano y el estado `NoNative`.

**Usa:**
- `lirtup_gpu::{ExternalTexture, ExternalSemaphore, PresentOptions::wait}`;
- `lirtup_fg::{native, multiplier, layer::LayerSetup}`;
- de S: `PresentSink`;
- de K: `SharedSurface` (lock, `present`).

**Lo usan:** P (el presentador) y K (`status_from_setup`).

**Criterio de 5.8:** FG por capa y por motor integrado, a ×2 y ×4, con video y sin errores de
validación.

### K: core, socket, CLI y la superposición

**Implementa:**
- `lirtup-core`: `Engine` (`new`, `handle`, `subscribe`, `snapshot`, `shutdown`), `server::serve`
  y los errores, según la tabla de comandos del doc de `engine.rs`.
- `lirtup-cli`: `run`, `serve`, `ctl`, `list` y `bench`.
- `surface`: `OverlayThread` (`spawn`, `surface`, `rect`, `send`, `notices`, `close`),
  `OverlayCommand` y `OverlayNotice`. `PresentSurface` ya está hecho.
- `lirtup-x11`: lo que la superposición y el reenvío necesiten en `overlay.rs` e `input.rs`.
- El test de ida y vuelta del protocolo.

**Usa:**
- de P: `Session`, `SessionSetup` y `PipelineCommand`;
- de F: `status_from_setup`;
- de S: `bench::run` y `save_results`, en el subcomando;
- `lirtup_fg::layer::{prepare, restart_self}`;
- `lirtup_x11::{Hotkeys, X11, OverlayWindow, InputForwarder}`;
- `lirtup_gpu::{Instance, Device}`;
- `lirtup_magpiefx::{Catalog, compile_effect}`.

**Lo usa:** nadie; está arriba de todo.

**Criterios de 5.8:**
- reenvío de ratón y atajos;
- `lirtup run`, `lirtup serve` y `lirtup ctl` de punta a punta;
- el test del protocolo en verde.

## Dependencias

Ya agregadas en esta rama: las de `lirtup-core` y `lirtup-cli` de arriba. Si un frente necesita
otra (por ejemplo, `nix` o `signal-hook` para el SIGTERM de la medición), la pide en su nota.

## Riesgos

- **El hilo de la superposición y el swapchain usan la misma conexión XCB desde dos hilos.**
  libxcb es seguro entre hilos y x11rb lo envuelve igual, pero hay que mirarlo en la integración.
- **El presentador espera el envío anterior antes de cada cuadro.** Es lo que hace 1.x, pero
  puede sumar latencia con FG: lo dirán los números del CSV.
- **Con FG integrado, el swapchain bajo `Mutex` puede competir entre los dos hilos.** El lock solo
  dura un present, pero hay que confirmarlo en la integración.
