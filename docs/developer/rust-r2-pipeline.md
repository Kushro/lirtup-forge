# Ronda 2, frente P: el pipeline

Rama **`claude/rust-r2-pipeline-e64c8b`**, que sale de `rust/r2-skeleton`. Fecha: 2026-11-01.
Sin push y sin merge.

`cargo check -p lirtup-pipeline --all-targets` (rustc 1.99.1): **pasa, sin avisos**. No corrí
`cargo build` ni `cargo test`: los tests están escritos pero no se ejecutaron.

Toqué solo `crates/lirtup-pipeline/src/{pipeline,tiles,upscale,presenter}.rs` y esta nota.

## Qué hice

### `tiles`: `TileProcessor`

- **2×**: cada tile sucio se recorta del cuadro capturado con su margen, repitiendo el píxel del
  borde (`extract_expanded`, como `extract_expanded_tiles`). Se escribe en una capa de un buffer de
  subida y se copia al arreglo de entrada. El último pase de SRCNN lee el residuo de la textura 1×
  completa, que es la entrada del `Upscaler`.
- **4×**: la primera etapa corre sobre el cuadro completo y solo la segunda va por tiles. Las
  regiones 2× se copian en la GPU desde la salida de la primera etapa. Si un tile de borde
  necesitaría recorte, ese cuadro va completo, igual que `_process_4x`.
- Usa `Srcnn::new_tiled` y `TilePlacement::from_grid`. El margen del shader se duplica en 4×.
- `should_use` sigue a `should_use_tile_mode`: hace falta daño, menos celdas sucias que capas y un
  área expandida por debajo de `area_threshold`.
- Tests: `dirty_tiles`, `interior_origin` y la réplica de bordes de `extract_expanded`.

### `upscale`: `Upscaler`

- Para `model: none` solo sube el cuadro (`PassthroughUpscaler`). Si no, corre una o dos etapas
  de SRCNN de cuadro completo (`double_upscale`) y, con `use_tile_processing`, el `TileProcessor`.
- La subida pasa por un buffer que replica el cuadro sin relleno entre filas. Solo se escriben las
  filas dañadas, y cuando el daño es chico solo se copian sus rectángulos
  (`use_damage_tracking`). Un `Upscaler` nuevo (otro modelo u otro tamaño) arranca con el cuadro
  entero.
- En modo tiles, la salida de los tiles es la salida del upscaler. Un cuadro completo (el primero,
  o un respaldo) se copia ahí, porque `Srcnn` fija su salida al crearse (ver «Pedidos»).
- `process(..., timestamps)` envía su propio trabajo. `finish()` lo espera y devuelve sus tiempos
  de GPU: la sesión los junta con los del presentador, así el panel muestra también SRCNN.
- `next_model` recorre `Model::ALL` para `CycleModel`.

### `presenter`: `Presenter`

- La cadena sigue el orden de 1.x:
  1. `deband`;
  2. los efectos nativos (Anime4K → LumaSharpen → CRT) o la cadena MagpieFX;
  3. el escalador: copia a 1:1, `upsampler` si algún lado crece y `downsampler` si no;
  4. CAS, bloom y viñeta sobre la luz lineal;
  5. `Delinearize` o `Dither`;
  6. la copia a `screen`;
  7. LUT y grano sobre los 8 bits;
  8. el OSD y el panel, que llegan como `overlays`;
  9. el swapchain o `FrameGen`.
- **MagpieFX**: compila al crear el presentador, en el hilo de la sesión, como en 1.x. Guarda los
  efectos compilados por `(nombre, swap de entrada, swap de salida)`. Hay un pase `SwapRB` en los
  extremos que no se pueden plegar en el shader. Si un efecto falla, se saca y la cadena se rearma
  sin él (`_build_magpie_chain`). Si solo cambian los parámetros, usa `set_params` sin recompilar.
- **`update_config`** rehace solo lo que cambió:
  - si solo cambiaron parámetros, usa `set_params`;
  - un preset de LUT nuevo, el CRT o un escalador nuevo reconstruyen esa parte;
  - un cambio de geometría reconstruye lo que va antes del escalador (CRT y MagpieFX renderizan
    al tamaño del contenido).
- **Las carreras de CPU y GPU**: la decisión del esqueleto se cumple esperando el envío al final de
  `present`, no al principio del cuadro siguiente. La cadena se envía, se espera (así se leen los
  timestamps de *este* cuadro) y recién después se presenta. La copia del swapchain va detrás en la
  misma cola, así que el present no sale más tarde que en 1.x. Entre dos presents no hay nada del
  presentador en vuelo, y cualquier `set_*` (también el `set_image` de S) es seguro.
- **Geometría**, con funciones puras y testeadas:
  - `output_geometry` = `parse_output_geometry`;
  - `scaling_rect` = `calculate_scaling_rect`;
  - `Layout::dst_rect` = `_compute_scaling_params`;
  - el zoom es `fit` al N % del monitor, como cuando `ZoomIn` pasa a `N%` en 1.x.
- `mapper()` da el rectángulo del contenido; la sesión completa `source_size` y `crop_offset`.
- `screenshot()` lee `screen` y cambia BGRA por RGBA.
- El fondo va como BGRA, como `color_string_to_float4`.

### `pipeline`: `Session` y el hilo

- `Session`:
  - un hilo `lirtup-session` por sesión, con un canal de comandos;
  - `view()` sale de un `ArcSwapOption`;
  - `stop` espera con un plazo y suelta el hilo si no termina;
  - `Drop` para la sesión con `StopReason::User`;
  - un guardia marca la sesión como terminada aunque haya un pánico, y en ese caso informa
    `SessionError { Runtime }` y `SessionStopped { Error }`.
- **Armado**: X11, el monitor y su refresco, las stats, la captura, la superposición (el modo
  según el tipo de sesión), el swapchain (`B8G8R8A8_SRGB`, 2 imágenes, el `vulkan_present_mode`),
  `FrameGen::new`, el `Upscaler`, el `Presenter`, el OSD y el panel.
- **El bucle** sigue el orden de la nota del esqueleto:
  - tope de fps y captura;
  - `NoWindow` termina con `WindowClosed`; `Resized` rehace la captura y el upscaler (y mueve la
    superposición en modo ventana); los fallos seguidos cortan después de `max_capture_failures`;
  - la barra de menú, mientras no se decidió: recorta, rehace la captura y manda
    `MenuBarDetected`;
  - el OSD y el panel;
  - el cuadro sin cambios: con `present_on_change_only` o con FG activo espera un intervalo de
    refresco y si no, `present_unchanged`;
  - el escalado y el present;
  - los tiempos de GPU de las dos partes;
  - el `PresentRecord` cuando no hay FG (con FG los informa F);
  - `SetMapper` si cambió;
  - la recreación del swapchain;
  - `SessionStarted` después del primer present;
  - `Stats::poll` y `Event::Stats`.
- **Entre cuadros**: los comandos, los avisos de la superposición (`Configured` lleva a `resize` y
  a los `rebind` de S; `Exposed`, `Hotkey`, `Closed` y `Failed`), `follow_focus` y
  `pause_on_focus_loss`. En pausa espera con `Select::ready_timeout` sobre los dos canales.
- **Atajos** (`controller.py`):
  - `ToggleScaling` pausa y oculta la superposición;
  - `Screenshot` guarda un PNG en RGB en otro hilo, con la plantilla `screenshot_filename`;
  - `ToggleStats`;
  - `CycleModel` rehace el upscaler;
  - `CycleGeometry` recorre `fit` → `stretch` → `cover`;
  - `RestoreView`;
  - `ZoomIn` y `ZoomOut` van de 50 a 800 % en pasos de 25, con vuelta;
  - `Offset*` mueve 25 px con los signos de 1.x (`OffsetLeft` suma a `offset_x`).

  Los textos del OSD salen en español o inglés según `LANG`.
- **`UpdateConfig`**: si cambia el recorte, la captura o los tiles, rehace la captura o el
  upscaler. Si cambia `overlay_mode`, `monitor` o el modo de presentación, rehace todo lo que
  depende de la ventana. Lo demás va a `Presenter::update_config`, `Osd::update_config`, la
  visibilidad del panel, la opacidad y el cursor. El zoom y el desplazamiento de los atajos se
  conservan.
- **`SwitchWindow`** (y `follow_focus`): desarma y vuelve a armar todo lo que depende de la ventana,
  superposición incluida, porque el reenvío de entrada tiene un solo destino. Conserva las stats,
  la vista y la configuración.
- Tests: el modo de superposición según el tipo de sesión, el rectángulo de la superposición, los
  pasos de zoom, el recorte, la plantilla de capturas (con fecha UTC) y la unión de timestamps.

### Comportamiento que difiere de 1.x a propósito

- **Centrado vertical en modo ventana**: 1.x centra la superposición con el *ancho* del monitor
  (`(phys_base_w - overlay_h) // 2`). Acá se usa el alto.
- **La fecha de las capturas sale en UTC**. 1.x usa la hora local. Ver «Dependencias».
- **El cursor**: `hide_cursor` (ms de inactividad) solo manda `SetCursorVisible(false)` al empezar.
  No hay temporizador, ver «Pedidos».

## Cambios de firma

### Internos de P (libres según la nota del esqueleto)

- `Presenter::new` recibe además `monitor: (u32, u32)`, la base de `fit` y de los porcentajes.
  K no la llama. `framegen` y `surface` no cambian.
- `Presenter` suma `set_capture_size` y `set_monitor`. `set_view` toma el desplazamiento completo
  del contenido: la sesión suma `offset_x` y `offset_y` fuera del modo ventana.
- `TileProcessor::new` recibe el residuo en lugar de la salida (la salida es la de
  `Srcnn::new_tiled`). `process` pasa a ser `record(recorder, ...) -> Option<u32>`: `None` quiere
  decir «este cuadro va completo». Suma `output()`.
- `Upscaler::process` suma `timestamps: bool`. Se agregan `finish()` y `labels()`.
- Nuevas funciones públicas: `presenter::{output_geometry, scaling_rect, Layout, ScaleMode}`,
  `pipeline::{overlay_mode, overlay_rect, step_zoom, screenshot_path, ScreenshotVars}`,
  `tiles::{dirty_tiles, extract_expanded}` y `upscale::next_model`.

### Que cruzan frentes

Las de `Session`, `SessionSetup`, `PipelineCommand` y `PresentOutcome` no cambiaron.

## Pedidos

| A quién | Qué | Por qué |
| --- | --- | --- |
| Revisor (`lirtup-effects`) | `Conversion::create_into(device, input, output: &Texture)`, o que `Scaler` y `Conversion` escriban en una textura dada | Hoy el presentador copia la salida de `Delinearize`/`Dither` a `screen` en cada cuadro: son unos 0,1 ms en 4K, y cuentan para el criterio del 5 %. Con esto la copia desaparece |
| Revisor (`lirtup-effects`) | Que `Srcnn::new` acepte una salida (`new_into`) | En modo tiles, los cuadros completos (el primero y los respaldos) se copian a la salida de los tiles. 1.x la comparte con `_rebind_full_frame_output` |
| K (`surface`) | `OverlayCommand::SetTarget(u32)` | Para cambiar de ventana sin rehacer la superposición, el swapchain y el presentador. Hoy `SwitchWindow` los rehace, con un parpadeo |
| K (`surface`) | Un temporizador para `hide_cursor` (en `OverlayOptions` o con un comando) | Ocultar el cursor tras N ms sin moverlo es de la superposición, que ve el ratón |
| K (`Engine`) | Avisar a la sesión si hay una GUI escuchando (`SessionSetup::timestamps` o `PipelineCommand::SetTimestamps`) | Hoy la sesión siempre pide timestamps. Cuestan poco, pero `PresentOutcome` prevé no pedirlos |
| Revisor (`lirtup-config`) | Corregir el doc de `Geometry::Percent` («100 = source size») | En 1.x, y acá, el porcentaje es del **monitor** (`parse_output_geometry`) |
| Revisor (`lirtup-config`) | ¿Un `swapchain_buffers` en la config? | `SurfaceDesc.buffers` lo menciona, pero `Config` no lo tiene. Uso 2 |

### Pedidos de S y F

No hace falta nada nuevo. Uso sus firmas como están:
- `Stats::{new, sink, record_*, set_gpu_times, set_multiplier, set_refresh_hz, poll, latest, close}`;
- `Osd` y `StatsPanel` (`pass`, `rebind`);
- `MenuBarDetector`;
- `FrameGen::{new, mark_capture, present, is_active, active_multiplier, resize, update_config,
  close}`.

## Dependencias

Ninguna nueva para el crate: `arc-swap`, `image` y `crossbeam-channel` ya estaban. Una opcional:
**`time`** (con `local-offset`) o `chrono`, para que `{timestamp}` salga en hora local como en 1.x.

## Pendiente

- **Correr los tests** (`cargo test -p lirtup-pipeline`) en la integración.
- **Probarlo con S, F y K mergeados**:
  - los cuatro modos de superposición, con tiles 2× y 4×;
  - el reenvío del ratón con el `mapper`;
  - `Configured` y la recreación del swapchain.
- **Medir los tiempos de GPU contra la tabla del README** (criterio de 5.8), sobre todo la copia de
  `screen` y la espera después del envío.
- **Los errores de compilación de MagpieFX solo van al log.** No hay `Event::EffectCompile` porque
  el presentador no tiene el canal de eventos. Se puede pasar el `Sender` si la GUI lo quiere.
- **Las capturas se sacan de `screen`, con el OSD y el panel incluidos**, como pide el doc del
  esqueleto. 1.x sacaba la salida del upscaler, antes de la cadena.

## Riesgos

- **Esperar al final de `present` deja la CPU quieta durante la cadena.** El cuadro siguiente
  igual esperaba en 1.x, así que no debería sumar latencia. El CSV lo va a decir.
- **`SwitchWindow` rehace la superposición entera.** Con `follow_focus` puede verse un parpadeo
  hasta que exista `SetTarget`.
