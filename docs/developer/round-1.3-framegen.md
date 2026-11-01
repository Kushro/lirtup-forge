# Ronda 1.3, frente B: generación de cuadros y Vulkan

Rama: `claude/round13-framegen-9d0e13` (desde `main`, 1.2.0). Fecha: 2026-11-01.

Todo está **escrito y compilado, sin probar**: no se corrió la app, ni pytest, ni nada en la
GPU. Las extensiones compilan (`python3 setup.py build_ext --inplace`, sin avisos nuevos) y los
módulos tocados importan con `~/.cache/lirtup-test-venv` (`QT_QPA_PLATFORM=offscreen`).
Nota: ese venv no tiene `setuptools`; la compilación se hizo con el `python3` del sistema
(3.14, igual que el venv).

## Lo hecho

| Commit | Qué |
| --- | --- |
| `183e6615` | Mutex de la cola y `present` sin el GIL |
| `132131ea` | `robustImageAccess` |
| `fae8b130` | Aviso de GPU con el intervalo real del juego |
| `7e9b0860` | Latencia en `StatsHUD` y CSV (`--latency-log`) |
| `b2ab60d4` | Multiplicador automático; cableado de la latencia en el pipeline y en el motor integrado; log de `robustImageAccess` |
| `51c19ddd` | Tests nuevos y ajustados, sin correr |
| `ac049286` | Secciones *Stats panel* y *Frame generation* |

### 1. `fifo` sin frenar al pipeline (`src/upscaler/vulkan/lib`)

- `vk_Device` tiene `queue_mutex`. Lo toman todos los `vkQueueSubmit`, `vkQueuePresentKHR`,
  `vkQueueWaitIdle` y `vkDeviceWaitIdle` del módulo: `vk_utils.cpp`
  (`vk_execute_command_buffer`, `vk_queue_submit_and_wait`), `vk_device.cpp` (`wait_idle`,
  `dealloc`), `vk_external.cpp` (`destroy_semaphore`, `copy_texture_signal`,
  `wait_semaphore`) y `vk_swapchain.cpp` (`present`, `recreate`, `dealloc`).
- `vk_QueueLock` (en `vk_common.h`) es el cierre para quien tiene el GIL: si la cola está tomada,
  suelta el GIL mientras espera. Regla para no trabarse: **nunca se toma el GIL con la cola
  tomada.** Quien no tiene el GIL usa `std::lock_guard` sobre `queue_mutex`.
- `Swapchain.present` suelta el GIL en `vkAcquireNextImageKHR`, en la espera de la fence por
  imagen y en submit + present (con la cola tomada). La grabación del command buffer sigue con
  el GIL, porque los command buffers salen del pool compartido del dispositivo.
- Dos `present` sobre el mismo swapchain no pueden ir a la vez: ya los separa el lock de
  `SwapchainManager`. Está documentado en el comentario de `vk_Swapchain_present`.

### 2. `robustImageAccess` (`vk_device.cpp`)

Con la instancia en Vulkan 1.2 viene de `VK_EXT_image_robustness`. Si la extensión está y la
característica también, se activa. `Device.robust_image_access` lo informa. Al arrancar, el
pipeline escribe en el log `Vulkan robustImageAccess: on` o `not available`.

### 3. Aviso de GPU (`pipeline/stats.py`)

`gpu_budget_warning(effects, game_fps, polled_fps)`:

- Si no se pierden cuadros, el presupuesto es el intervalo real del juego (41,7 ms con 24 fps).
- Si se pierden, sigue con el mínimo de 30 fps (33,3 ms), como en la 1.2.

Para saber si se pierden cuadros (`frames_lost`) se usa lo que el panel ya mide: si el bucle
casi nunca sondea la ventana sin encontrar un cuadro nuevo (menos de 0,25 sondeos vacíos por
cuadro nuevo), nunca espera al juego. Entonces el juego dibuja más rápido de lo que se procesa,
y la tasa medida es la del pipeline. Es la versión práctica de «comparar cuadros nuevos con lo
que la ventana dibuja»: XDamage no dice cuántos cuadros dibujó el juego entre dos capturas.

### 4. Latencia

- Definición: desde que empieza la captura de un cuadro del juego (`perf_counter` antes de
  `grab()`) hasta que lirtup entrega a la presentación un cuadro hecho con él, real o generado
  (cuando vuelve `vkQueuePresentKHR`). No incluye la pantalla. Con la capa, tampoco lo que
  lsfg-vk agrega después.
- Sin FG o con la capa: la registra el pipeline después de `presenter.present()`. Los
  `present_unchanged` no cuentan, porque re-presentan una captura vieja.
- Motor integrado: el pipeline llama a `NativeFrameGen.mark_capture(t)` antes de
  `presenter.present()`. El tiempo viaja en el `_Cycle`, y el hilo `lsfg-present` informa cada
  generado y cada real por `on_present`. Así no hizo falta tocar `presenter.py` (ver
  integración).
- Panel: fila *Latency* con media y máximo del último segundo por tipo (`real`, `generated`).
  Con la capa termina en `+ lsfg-vk`.
- CSV (`--latency-log PATH`, `latency_log` en la config): una fila por present, con
  `capture_s,present_s,latency_ms,kind,multiplier`. Los cuadros del mismo ciclo comparten
  `capture_s`. Se vacía al disco una vez por segundo y al cerrar.

### 5. Multiplicador automático

- Config: `frame_gen_multiplier` acepta `auto` (YAML), `--frame-gen auto` (CLI), validador
  `validate_frame_gen_multiplier`. `framegen.parse_multiplier`, `multiplier_on`,
  `active_setting()` (lo que el proceso arrancó: 1, 2..5 o `auto`) y `AUTO_START_MULTIPLIER`.
  `NativeSettings` tiene un campo nuevo, `auto`.
- Con la capa, `auto` corre a ×2 y lo avisa: la capa fija el multiplicador al cargarse.
- Elección (`ideal_multiplier`): el mayor de ×2 a ×5 cuya salida entra en el refresco, con 2 %
  de tolerancia. A 144 Hz da ×5 con 24 fps, ×4 con 30 (también con 29,97 y 30,5) y ×2 con 60.
- Histéresis (`AutoMultiplier`):
  - Baja en cuanto la salida pasa el refresco (más el 2 %).
  - Sube solo si la salida mayor entra en el 97 % del refresco. A 144 Hz, entre 27,9 y
    29,3 fps no se mueve ni de ×4 ni de ×5.
  - Una elección nueva tiene que durar 2 s y entre cambios pasan al menos 5 s.
  - Con menos de 5 fps (imagen quieta) no cambia.
- Arranque: los primeros 15 cuadros se presentan reales, sin lsfg, para medir. Después lsfg
  arranca directo con la elección, sin un reinicio extra.
- Cambio: con el hilo de presentación ya libre, se borra el contexto y se liberan las texturas
  compartidas. `finalize` + `initialize` corren en un hilo `lsfg-restart` (lsfg suelta el GIL).
  Mientras tanto se presentan los reales solos: la imagen no se corta, pero hay un tramo sin
  generados. El cuadro siguiente al reinicio abre un contexto nuevo, y ese ciclo tampoco tiene
  generados. Si el reinicio falla, la FG se apaga para la sesión, como cualquier otro error.
- Refresco: `_refresh_rate(overlay)` en `pipeline.py` (`QScreen.refreshRate`, 60 si no lo
  informa), pasado con `NativeFrameGen.set_refresh_rate`.
- GUI: opción *Auto* en la pestaña Presentación y en la bandeja. En la bandeja solo se puede
  elegir con el motor integrado, y debajo aparece la nota «Auto needs the built-in engine».
  Estados nuevos en la pestaña: corriendo con multiplicador automático, y «Auto» con la capa.
- Panel: `Frame gen  auto x4 built-in FP16 …`, o `restarting lsfg` mientras reinicia.

### 6. Tests (sin correr)

- `tests/test_native_framegen.py`:
  - Ajusté los `_generate` simulados a la firma nueva (`capture_time`).
  - Nuevos: informes de latencia, `ideal_multiplier`, histéresis, tiempos de espera, arranque
    con medición, reinicio en su hilo, reinicio fallido y `close` que espera al reinicio.
- `tests/test_stats.py`: presupuesto con y sin cuadros perdidos, `frames_lost`, medias de
  latencia, vencimiento de las muestras y CSV.
- `tests/test_framegen.py`: `parse_multiplier`, `auto` con el motor integrado y con la capa,
  `prepare_from_options` y el validador.

## Para la integración (archivos de otros o sin dueño)

1. **`src/upscaler/gui/main.py`**, `restart_with_frame_gen`: el log usa
   `"Restarting with frame generation x%d"` y desde la bandeja ahora puede llegar `"auto"`.
   Cambiar a `%s` y la anotación `multiplier: int` a `framegen.Multiplier`. No rompe: logging
   atrapa el error de formato, pero escribe una traza.
2. **Traducción.** Cadenas nuevas con `self.tr`:
   - `presentation.py`: «Auto» (*Frame generation multiplier*).
   - El texto de ayuda del multiplicador cambió (*Description of a setting*).
   - «Auto needs the built-in engine; the lsfg-vk layer runs at ×{0}.»
   - «Running with the automatic multiplier.»
   - «Running with the automatic multiplier. Flow scale and model apply after a restart.»
   - `tray.py`: «Auto» (*Tray menu: automatic frame generation*) y «Auto needs the built-in
     engine» (*Tray menu note*).

   `tests/test_translations.py` puede fallar hasta que se traduzcan.
3. **`pipeline/presenter.py`** no se tocó. Más limpio sería que `Presenter.present` recibiera
   `capture_time` y lo pasara a `NativeFrameGen.present`, en vez de `mark_capture`. Queda a
   criterio de la integración.
4. **`docs/developer/frame-generation-analysis.md`**: agregar los resultados de T3.x cuando
   se corran (no es de este frente).
5. Botón *Medir en mi GPU* en Presentación (frente C): la sección de generación de cuadros está
   en `presentation.py`, después del combo de multiplicador y del de motor.

## Lo que quedó afuera

- La latencia termina cuando lirtup entrega el cuadro a la presentación, no cuando aparece en
  pantalla. Medir el momento real necesita `VK_KHR_present_wait`/`present_id` o
  `VK_GOOGLE_display_timing`: posible para la próxima ronda.
- `auto` con la capa: no se puede cambiar sin reiniciar el proceso, así que corre a ×2.
- El reinicio de lsfg deja un tramo sin cuadros generados (lo que tarde `initialize`, que
  incluye leer los shaders de la DLL y crear los pipelines). Sin medir: puede ser de cientos de
  ms.
- El panel *Shown* sigue contando solo los presents del hilo del pipeline; con el motor
  integrado, la tasa real de salida se ve en MangoHud o en el CSV.

## Riesgos a mirar al probar

- **Concurrencia nueva.** Antes el GIL serializaba todo el uso de Vulkan; ahora el hilo
  `lsfg-present` presenta sin el GIL mientras el pipeline graba y envía.
  - Los objetos que siguen dependiendo del GIL son el command pool, `ext_cmd`/`ext_fence` y la
    `internal_cmd_buffer`.
  - `wait_for_last_present` lee `last_present_fence` sin lock mientras el otro hilo puede estar
    en `present`, igual que antes.
  - Si aparece un cuelgue, revisar primero que nadie tome el GIL con `queue_mutex` tomado.
- **`vkDeviceWaitIdle` con la cola tomada** en `recreate`: con `fifo` puede esperar un vblank
  con el GIL (usa `vk_QueueLock`, que solo suelta el GIL si tiene que esperar el mutex).
- **`robustImageAccess`** puede costar algo de rendimiento en algunos drivers. Comparar con el
  panel en T3.1 y T3.9.

## Qué probar (guion de la ronda, sección 3)

| ID | Qué mira de este frente |
| --- | --- |
| T1.2 | Que corran `test_native_framegen.py`, `test_stats.py` y `test_framegen.py` (todos nuevos o ajustados) |
| T3.2 | `fifo` sin perder cuadros: con video 30, «Game 30 fps new» en el panel. Probar también sin FG |
| T3.3 | ×2 con video 60 en `mailbox` y `fifo`: ~120 fps |
| T3.4 | *Auto* con 24, 30 y 60 fps a 144 Hz: ×5, ×4 y ×2; log `Native frame generation x4 (auto)`; no salta |
| T3.5 | Video que pasa de 30 a 60: un solo cambio (`Automatic frame generation: x4 -> x2`), unos 2 s después del cambio; la imagen no se corta |
| T3.6 | `--latency-log /tmp/lat.csv` sin FG, capa ×2 e integrado ×2: el CSV tiene filas `real` (y `generated` con el integrado); la del integrado ≈ un intervalo más |
| T3.7 / T3.8 | Aviso de GPU: no con 24 fps y ~35 ms; sí con 60 fps y la cadena UL |
| T3.9 | Línea `Vulkan robustImageAccess: on` en RADV |
| T5.3 | Bandeja: *Auto* habilitado solo con el motor integrado; reinicia y retoma la ventana |
| Extra | Cerrar lirtup durante un reinicio de lsfg (cambiar el fps del video y cerrar enseguida): debe cerrar sin colgarse |
