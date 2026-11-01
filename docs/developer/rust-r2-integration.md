# Ronda 2: integración, builds y pruebas

Rama **`rust/r2-integration`**, que sale de `rust/r2-skeleton`. Fecha: 2026-11-01. Sin push y
sin merge a `main`.

Notas relacionadas: el esqueleto ([rust-r2-skeleton.md](rust-r2-skeleton.md)) y las notas de
cada frente:
- [rust-r2-stats.md](rust-r2-stats.md);
- [rust-r2-pipeline.md](rust-r2-pipeline.md);
- [rust-r2-framegen.md](rust-r2-framegen.md);
- [rust-r2-core.md](rust-r2-core.md).

**Máquina de prueba:** la misma de la ronda 1.
- GPU: RX 9070 XT, con RADV y Mesa 26.2.5.
- Sesión: KWin sobre Wayland, con XWayland y monitores de 144 y 60 Hz.
- Están instalados las capas de validación, lsfg-vk 2.0 (capa implícita), `Lossless.dll` y la
  1.3.0 (pipx).

## Resumen

| | Build 1 | Build 2 |
| --- | --- | --- |
| `cargo build --workspace --release` | pasa, 0 avisos | pasa, 0 avisos |
| `cargo clippy --workspace --all-targets` | pasa, 0 avisos | pasa, 0 avisos |
| `cargo test --workspace` | 302 de 302 (3 ignorados) | 302 de 302 (3 ignorados) |
| Tests ignorados (`--ignored`, en GPU) | paridad de E: 2 de 2; lsfg con fds: **falla** (el aviso del cargador de Vulkan) | **3 de 3** |
| Ejemplo `copy` de G | falla sin `DISABLE_LSFGVK=1` (el mismo aviso) | pasa sin variables |
| `cargo fmt --check` | no (P, F y S no lo pasaron) | pasa |

Ya no queda ningún `todo!()` en el workspace.

**Criterios de 5.8 de la ronda 2:** se cumplen todos los que se pueden verificar desde la sesión.
- El reenvío del ratón y los atajos de teclado quedan para el guion de pruebas a mano: en esta
  sesión KWin no entrega a XWayland las teclas ni los clics que inyecta `xdotool`.
- FG por capa da un error de validación, pero lo provoca la propia capa lsfg-vk (detalle
  abajo).

## Merges

Los merges fueron en el orden S → P → F → K, con `--no-ff`, y **no hubo conflictos**. Cada frente
tocó solo sus archivos.

| Frente | Rama | Commits |
| --- | --- | --- |
| S | `claude/rust-r2-stats-46ef59` | 7 |
| P | `claude/rust-r2-pipeline-e64c8b` | 5 |
| F | `claude/rust-r2-framegen-f4e51e` | 4 |
| K | `claude/rust-r2-core-1b530a` | 7 |

Después de los merges hay 9 commits míos:
1. La preparación de la integración:
   - `libc` en `lirtup-core`, que pidió K;
   - fuera el `#![allow(unused_variables)]` del `lib.rs` del pipeline;
   - las referencias de paridad en el `.gitignore`.
2. `lirtup-gpu`: los mensajes `GENERAL` del cargador ya no cuentan como validación.
3. El panel de stats con el contenido de 1.x (proto, `stats`, `framegen`, `pipeline`).
4. El panel y el OSD en orden BGRA.
5. `lirtup serve --config`, y EPIPE termina sin error.
6. Dos correcciones de documentación.
7. `cargo fmt` sobre los archivos de los frentes.
8. `CancelBench` con `libc::kill` (pedido de K).
9. Esta nota.

## Revisión contra el esqueleto

Ninguna firma que cruza frentes cambió. Los cambios que anotaron los frentes:

- **S:**
  - agregó `serde_json` a `lirtup-pipeline` e `image` a las dev-dependencies de `lirtup-effects`,
    con el OK del orquestador;
  - `PanelContext`, que es suyo, lo reemplacé yo por la versión de 1.x (abajo);
  - la clave de FG en `bench.json` es la de 1.x (`x2 flow 1.0 quality`), y el doc ya lo dice.
- **P:** solo cambió firmas internas, que el esqueleto le dejaba libres:
  - `Presenter::new` con el tamaño del monitor;
  - `TileProcessor::record`;
  - `Upscaler::process(…, timestamps)` y `finish()`.

  **Las acepto.**
- **F:** rehízo `pacing` en dos mitades, `FrameInterval` y `FramePacer`, y es interno. En
  `lirtup-fg` agregó `EngineStart`, `EngineOptions::from_config` y `AutoMultiplier::chosen`.
  **Lo acepto.**
- **K:**
  - Agregó `OverlayCommand::{SetTarget, HideCursorAfter}`, que pidió P.
  - Volvió a poner `OverlayNotice::Hotkey`, que yo había sacado del esqueleto. Lo acepto como
    respaldo: solo salta cuando una tecla de atajo llega a la superposición en vez de a la captura
    global, que es lo que pasa si otro programa ya tiene esa combinación. P ya la maneja.
  - Superficie pública nueva en `lirtup-core`: `client`, `listing`, `signals`, `prepare_headless`
    y `EngineOptions::profile`.
  - El test de ida y vuelta del protocolo ya está y pasa. Cubre también el riesgo de la ronda 0
    (`flatten` sobre un enum con etiqueta).

## Paridad de E con Python (pendiente de la ronda 1)

```text
python scripts/rust-parity-refs.py        # con el Python de pipx de la 1.3.0, sin PYTHONPATH=src
cargo test --release -p lirtup-effects -p lirtup-magpiefx --test parity -- --ignored
```

- Las referencias las generó la 1.3.0 instalada en esta GPU. No uso el `src/` del repositorio
  porque no tiene las extensiones compiladas.
- **Pasan los 111 casos con una diferencia máxima de 1** por canal, sin tolerancias por efecto. Son:
  - **`lirtup-effects` (25 casos):** los 10 efectos nativos, los 6 escaladores y los 9 modelos
    SRCNN.
  - **`lirtup-magpiefx` (86 casos):** todos los efectos verificados, incluidos los 3 Denoise
    compilados con glslang.
- Los PNG (11,6 MB) **no van a git**: dependen de la GPU, así que se generan en cada máquina con
  el script.

Queda cumplida la mitad pendiente del criterio de E de la ronda 1.

## Lo que falló y cómo se arregló

1. **El test de lsfg con fds reales y el ejemplo `copy` fallaban** con el aviso del cargador
   «Failed to find `vkGetInstanceProcAddr` in layer liblsfg-vk-layer.so». Es la capa implícita de
   lsfg-vk instalada en el sistema, no un hallazgo de validación.
   - **Corrección:** `lirtup-gpu` sigue logueando los mensajes de tipo `GENERAL` (del cargador o
     de una capa sobre sí misma), pero ya no los junta en `Device::debug_messages`. Solo cuentan
     `VALIDATION` y `PERFORMANCE`.
   - lsfg genera bien: el píxel central de un cuadro intermedio entre rojo y azul sale violeta.
2. **Las etiquetas del panel salían naranjas en vez de celestes.**
   - La textura `screen` de P lleva los bytes BGRA de la captura tal cual, como en 1.x.
   - El panel y el OSD subían RGBA.
   - **Corrección:** `Canvas::finish` ahora entrega BGRA, como S había previsto en su nota.
3. **El panel no tenía el contenido de 1.x.** S pidió el cambio de contrato y lo hice:
   - **`StatsSample`** suma `poll_fps`, `render_ms` y `latency_by_kind`, con `serde(default)`;
   - **`PanelContext`** lleva el recorte, ×2/×4, el motor de efectos, `output_geometry` con los
     escaladores, «Post», «Present» y una `FrameGenLine`;
   - **`FrameGen::panel_line`** da el texto de 1.x, por ejemplo «x2 built-in FP16 ~124 fps,
     pacing 15.4 ms»;
   - **las filas** son las de `_stats_rows`, más «Mode» (nuevo en 2.0) y los tiles del último
     cuadro en «Upscale».

   Saqué las filas «Window» y «GPU ms/frame», que 1.x no tenía.
4. **`lirtup serve` no aceptaba `--config`**, y `run` sí. Ahora acepta `-c/--config`.
5. **`lirtup ctl … | head`** terminaba con «Broken pipe»; ahora sale sin error.
6. **`CancelBench`** lanzaba `kill(1)`. Ahora usa `libc::kill`, y la ranura del pid se vacía antes
   de esperar al hijo. Lo probé: cancelar a 1,5 s da `BenchResult { cancelled: true }`.
7. **Documentación:**
   - `Geometry::Percent` es un porcentaje del monitor, no de la fuente (pedido de P);
   - `SessionError` siempre va seguido de `SessionStopped` (pedido de K).

## Criterios de 5.8, ronda 2

| Criterio | Estado | Cómo se verificó |
| --- | --- | --- |
| `lirtup` escala una ventana en los cuatro modos | **Cumplido** | `serve` + `ctl start` sobre una ventana X de prueba, en `always-on-top`, `top-transparent`, `fullscreen` y `windowed` (con `run --overlay-mode windowed` y con `start … windowed`). Revisé las propiedades de la superposición con `xprop`: override-redirect, `_NET_WM_STATE_FULLSCREEN`, `ABOVE` y `WM_TRANSIENT_FOR` según el modo. No hubo mensajes de validación con `LIRTUP_VULKAN_DEBUG=1`. El contenido lo verifiqué con la captura de lirtup (`hotkey screenshot`): los engranajes de `glxgears` con SRCNN 4x16 y Lanczos, y la ventana de prueba con sus bandas a los costados |
| … con tiles | **Cumplido** | Una ventana que solo cambia un cuadrado: el panel muestra «tiles (2, 3 %)», sin rastros ni bordes de tiles en la captura |
| … con reenvío de ratón y atajos | **Pendiente → a mano** | `xdotool` no llega a XWayland en esta sesión de KWin, ni con una ventana X enfocada. Los atajos por protocolo andan: `ctl 'hotkey screenshot'` guarda la captura |
| Tiempos de GPU dentro de un 5 % de Python | **Cumplido** | `lirtup bench` contra la medición de la 1.3.0, en la misma corrida y la misma GPU, de 1280×720 a 2560×1440. Los 8 casos de efectos quedan entre −3,3 % y +3,2 %: LumaSharpen, Anime4K_Upscale_S y CRT_Easymode, nativos y MagpieFX, más CuNNy-4x16 y FSR_EASU. También coinciden con la tabla del README |
| FG por motor integrado a ×2 y ×4, sin errores de validación | **Cumplido** | A ×2: 61 fps del juego, 124 mostrados (62 generados). A ×4: 35 del juego, 144 mostrados (108 generados). Cero mensajes de validación en las dos |
| FG por capa a ×2 y ×4 | **Cumplido, con un error de validación que no es nuestro** | La capa genera: nuestros presents bajan a 72/s y 36/s, como en 1.x. La validación da **un** error en `vkCreateDevice`: la cadena trae `VkPhysicalDeviceVulkan12Features` y `VkPhysicalDeviceTimelineSemaphoreFeatures` a la vez. El segundo lo agrega la capa lsfg-vk al interceptar el dispositivo (va primero en la cadena `pNext`). 1.x encadena lo mismo, así que le pasa igual. Se reporta a lsfg-vk |
| Panel de stats igual en contenido | **Cumplido** después de la corrección 3 | Filas, orden y formato de `_stats_rows`, revisados en la captura |
| CSV de latencia igual en contenido | **Cumplido** | Mismas columnas, decimales y valores de `kind` y `multiplier` que el CSV de la 1.3.0 sobre la misma ventana. Solo cambia el origen de los tiempos: segundos desde el inicio de la sesión |
| `run`, `serve`, `ctl` de punta a punta | **Cumplido** | `hello` (Snapshot), `start`, `SessionStarted`, `Stats` cada 500 ms, `hotkey`, `stop`, `quit` (que borra el socket), `Bench` y `cancel_bench` |

### Latencia con FG integrado ×2 (3 corridas de cada uno, sin los primeros segundos)

| | Real: media / p95 | Generado: media / p95 |
| --- | --- | --- |
| Rust | 29,6 / 30,5 ms | 22,0 / 22,8 ms |
| 1.3.0 | 27,2 / 31,1 ms | 19,4 / 23,3 ms |

El p95 es igual o mejor. La media de Rust queda unos 2,5 ms más alta y casi sin variación: siempre
cae cerca del p95, mientras que la de 1.x se reparte. Parece el encadenado del ritmo, que se
queda en la fase tardía.

## Hallazgos para seguir

1. **Captura de ventanas que presentan con Vulkan en XWayland y KWin.**
   - `GetImage` de una ventana `vkcube --wsi xcb` da negro. Con `glxgears` (GL) funciona.
   - La captura de 1.x usa el mismo método, así que debería pasarle igual, pero no lo comprobé.
   - **Esto puede afectar a los juegos con DXVK o VKD3D**, que presentan por Vulkan sobre X11.
   - Hay que probarlo con un juego real (guion, punto 1). Si se confirma, es un problema de
     captura (front X), anterior a la ronda 3.
   - Por la misma razón no se puede verificar la superposición capturándola: la verifiqué con la
     captura interna de lirtup.
2. **Ciclos de FG medidos en la CPU** (`lirtup bench`): entre 2 y 17 % más lentos que en 1.x. Es el
   reloj de la CPU, no tiempo de GPU. Lo más probable es que la diferencia venga de cómo se espera
   a los semáforos (un envío vacío con su fence). Es para F.
3. **«render» en el panel:** 13,7 ms con FG integrado. Incluye la espera del ciclo anterior
   (`drain`) y la del envío al final de `present`; conviene separarlas. Es para P y F.
4. La latencia media con FG integrado (tabla de arriba). Es para F.

## Lo que pasa a la ronda 3 (o después)

| Pendiente | De quién | Cuándo |
| --- | --- | --- |
| P use `OverlayCommand::SetTarget` en `SwitchWindow` y `follow_focus`, y `HideCursorAfter` para `hide_cursor` (hoy solo esconde el cursor al empezar). K los agregó en paralelo y P no los conecta | P y K | Ronda 3 |
| Pedir timestamps solo con una GUI escuchando (`PipelineCommand::SetTimestamps`) | P y K | Ronda 3, con la GUI |
| Un código de motivo en `FrameGenStatus` para que la GUI traduzca `NoNative` | Revisor (proto) | Esqueleto de la ronda 3 |
| `StartFullscreen` no debe tomar la GUI ni una superposición vieja (filtrar `_LIRTUP_OVERLAY` y la clase `lirtup`) | K | Ronda 3 |
| Los errores de compilación de MagpieFX del presentador van solo al log; la GUI querrá un `EffectCompile` | P y K | Ronda 3 |
| `Conversion::create_into` y `Srcnn::new_into`, para evitar la copia a `screen` (unos 0,1 ms a 4K) | Revisor (`lirtup-effects`) | Ronda 4, si la medición lo justifica |
| `swapchain_buffers` en la configuración (hoy fijo en 2) | Revisor (`lirtup-config`) | Ronda 4 |
| `restart_self` con variables, para que el escalador reiniciado retome la sesión | F | Ronda 4 |
| `{timestamp}` de las capturas en hora local (hoy UTC) | P | Lista de paridad de la ronda 4 |
| `bench` sin modo mide solo los 3 nativos de 1.x; `bench.json` sin `sizes` ni `selectors` | S y K | Cuando la GUI lo use (ronda 3) |
| XTest en `InputForwarder` | K | Solo si algún juego ignora `SendEvent` |
| Los hallazgos 1 a 4 de arriba | X, F y P | Antes de cerrar la ronda 3 |

## Guion de pruebas a mano

Antes: `cargo build --release -p lirtup-cli` y `alias lirtup=target/release/lirtup`. Para ver
los mensajes de validación: `LIRTUP_VULKAN_DEBUG=1`.

1. **Un juego con DXVK o VKD3D (Proton), el hallazgo 1.**
   - Correr `lirtup run -v -t "<título>" --show-stats`.
   - Mirar si se ve el juego o negro, y si «Game» marca fps (con negro, los fps nuevos dan 0).
   - Repetirlo con la 1.3.0 (`upscale -t …`) para saber si 1.x hace lo mismo.
2. **Atajos globales** con una sesión de `run` andando, apretando cada combinación con el teclado
   real:
   - Alt+Shift+I (panel), Alt+Shift+M (modelo, con el OSD), Alt+Shift+G (geometría), Alt+Shift+P
     (captura), Alt+Shift+Plus/Minus (zoom), las flechas con Alt+Shift (desplazamiento),
     Alt+Shift+R (restaurar), Alt+Shift+S (pausa) y Alt+Shift+Escape (salir);
   - probar cada uno con NumLock prendido y apagado;
   - con `serve`: Alt+Shift+A y Alt+Shift+Q arrancan sobre la ventana enfocada, aunque no haya
     ningún cliente conectado.
3. **Reenvío del ratón** en `fullscreen` y `windowed`:
   - mover, hacer clic, doble clic y arrastrar sobre el contenido escalado, y ver que el juego
     responde en el lugar correcto;
   - en las bandas negras, el juego no tiene que recibir nada (le llega un `LeaveNotify`);
   - la rueda;
   - en `top-transparent` los clics tienen que pasar de largo.
4. **El texto del OSD y del panel**, en un monitor de 1440p y en uno de 1080p:
   - que se lea;
   - que esté centrado (OSD) o arriba a la izquierda (panel);
   - que las etiquetas sean celestes y los avisos rojos.
5. **Cambiar el tamaño de la ventana del juego y de la superposición** (`windowed`): que llegue
   `Configured`, que el swapchain se rehaga y que no aparezcan errores de validación.
6. **FG con video real (mpv a 24 o 30 fps)**, por capa y por motor integrado, a ×2, ×3, ×4 y
   *Auto*:
   - que el movimiento se vea parejo;
   - con *Auto*, que pasar de 30 a 60 fps reinicie una sola vez y sin huecos.
7. **`ctl pick_window`:** sale el cursor en cruz; un clic devuelve la ventana y Escape cancela.
8. **`ctl restart` con FG por capa y otro multiplicador:** el PID de `serve` cambia, el socket
   vuelve y la sesión no se retoma sola (pendiente de la tabla de arriba).
9. **La GPU integrada** (`--gpu 1`): una sesión con un modelo y un efecto MagpieFX.
