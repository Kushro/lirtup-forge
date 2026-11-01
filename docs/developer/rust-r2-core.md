# Ronda 2, frente K: core, socket, CLI y la superposición

Rama **`claude/rust-r2-core-1b530a`**, que sale de `rust/r2-skeleton` (`caf74486`). Fecha:
2026-11-01. Sin push y sin merge.

`cargo check --workspace --all-targets` (rustc 1.99.1): **pasa, sin avisos**. También pasa por
separado en `lirtup-proto`, `lirtup-x11`, `lirtup-pipeline`, `lirtup-core` y `lirtup-cli`.
`Cargo.lock` no cambió. No corrí `cargo build`, `cargo test` ni la app: los tests están escritos,
pero no se corrieron.

Notas anteriores: [rust-r2-skeleton.md](rust-r2-skeleton.md) y
[rust-r0-contract.md](rust-r0-contract.md).

## Commits

1. `lirtup-proto`: test de ida y vuelta de cada `Command` y `Event`, el formato en el cable y el
   error de versión.
2. `lirtup-core`: `Engine`, el servidor y el cliente del socket, los listados y las señales.
3. `OverlayThread`: el hilo de la superposición, con el reenvío de entrada y los atajos.
   `set_mode` ahora informa la geometría que pone el gestor de ventanas.
4. `lirtup`: `run`, `serve`, `ctl`, `list` y `bench`. `Engine` cierra con `SessionStopped` los
   arranques fallidos.
5. `lirtup serve` rechaza un segundo servidor antes de crear el `Engine`, y `ctl restart` espera
   el arranque.
6. Superposición: `OverlayCommand::SetTarget` y `HideCursorAfter`, que pidió P.

## Qué hice

### El test del protocolo (`crates/lirtup-proto/tests/roundtrip.rs`)

- **Ida y vuelta.** Cada variante de `Command` (16) y de `Event` (18) pasa por `encode_line` y
  `decode_line` y vuelve igual. Cubre todos los valores de `Target`, `StopReason`, `Stage`,
  `FrameGenState`, `Level`, `BenchKind`, `ParamKind` y `EffectOrigin`, además de los `Option` en
  `Some` y en `None`.
- **Variantes nuevas.** Un `match` exhaustivo deja de compilar si se agrega una variante y no se
  la suma a las listas.
- **Formato en el cable.** Compara contra líneas escritas a mano: las del doc del crate,
  `Target::Window` como `{"window":…}`, `v` al final de la línea y `Hotkey` en snake_case.
  Verifica que se codifique exacto `Quit`, `SessionStopped` y `BenchProgress`.
- **Versión.** Una `v` distinta da `CodecError::Version`, en `Command` y en `Event`.
- **Líneas malas.** Varias líneas inválidas dan `CodecError::Json`.
- **Ruta del socket.** `socket_path` termina en `lirtup/lirtup.sock` o `lirtup-<uid>/lirtup.sock`.

El riesgo que dejó la ronda 0, `flatten` sobre un enum con etiqueta interna, queda cubierto en
cuanto se corra el test.

### `lirtup-core`

- **`Engine`** (`engine.rs`). El orden de arranque es el del doc:
  1. lee la configuración;
  2. llama a `layer::prepare`, antes de cualquier hilo;
  3. crea el hilo que reenvía los eventos de la sesión;
  4. arranca los atajos globales.

  La GPU se abre con la primera sesión y queda abierta hasta el cierre. `handle` no espera ni a X11
  ni a la GPU ni a un compilador:
  - `Start`, `Restart`, `PickWindow`, `ListEffects`, `CompileEffect`, `ListDevices`,
    `Diagnostics` y `Bench` corren en hilos propios;
  - sí espera a que pare la sesión en `Stop`, en `Quit` y cuando un `Start` la reemplaza.

  Cómo se comporta:
  - **El reenvío de eventos.** Los eventos de la sesión pasan por un solo hilo,
    `lirtup-events`. Ese hilo actualiza la foto (`SessionStarted` → `view`, `SessionError` →
    `last_error`, `FrameGen`, `DaemonState`) y después hace el `broadcast`.
  - **`Start`.** Busca la ventana:
    - `Focused` usa `active_after(delay)`;
    - `Window` usa `window_info`;
    - `Title` usa `find_by_title`;
    - `Pid` usa `wait_for_pid`, con los tiempos de la configuración.

    Después resuelve el perfil (el que se nombró, si no el de `EngineOptions::profile`, y si no
    `ProfileChoice::Auto`) y manda `SessionStarting`. Por último abre el dispositivo, para la
    sesión anterior con `Replaced` y llama a `Session::start`.

    Cada `Start` y cada `Stop` suben un número de generación, así que un `Start` que todavía
    busca su ventana se cancela solo. Si el arranque falla antes de que exista la sesión, manda
    `SessionError` y después `SessionStopped { Error }`, igual que una sesión que falla.
  - **Atajos.**
    - `StartFullscreen` y `StartWindowed` arrancan con la ventana enfocada. Si ya hay una sesión,
      la paran, porque en ese momento la superposición tiene el foco.
    - `ExitApp` para la sesión.
    - El resto va como `PipelineCommand::Hotkey`.
  - **`Restart`.** Con la capa lsfg-vk, si cambiaron el motor, el multiplicador, el flujo, el modo
    de rendimiento o la DLL desde `prepare`:
    1. para la sesión;
    2. suelta los atajos;
    3. cancela la medición;
    4. llama a `layer::restart_self`.

    Si no, vuelve a arrancar la última ventana (por XID), con el mismo tipo y el mismo perfil.
  - **`ReloadConfig`.** Vuelve a leer `config.yaml` y `modes/` y hace `Hotkeys::rebind`. Si hay
    sesión, le manda `UpdateConfig` con la configuración resuelta otra vez para esa ventana. Si no
    hay sesión, manda `FrameGen` cuando cambia el estado. Si el cambio pide un `exec`, lo avisa
    con un `Log`.
  - **`SetDaemon`.** Solo guarda el estado y manda `DaemonState`, con `watching` igual a los
    perfiles con `match` que no están excluidos. El daemon es de la ronda 4.
  - **`Bench`.** Lanza `lirtup bench --json <req> [--gpu …]` (el mismo ejecutable) y reenvía cada
    línea. Si el hijo termina sin `BenchResult`, manda un `Log` y un `BenchResult { cancelled:
    true }` vacío.

    `CancelBench` le manda SIGTERM con `kill(1)`, porque `lirtup-core` no tiene `libc` (ver
    «Dependencias»).
  - **`Diagnostics`.** Junta las GPU, la versión de Vulkan del adaptador elegido, X11 o XWayland,
    el estado de FG, `Lossless.dll` con `dll::find_dll`, las versiones de dxc y glslang, y la
    carpeta de configuración.
  - **`shutdown`.** Se puede llamar más de una vez. Para la sesión (`Quit`), cancela la medición,
    suelta los atajos, vacía los eventos pendientes y une los hilos. `Drop` de `Engine` lo llama.
- **Superficie pública nueva** (crate de K, nadie más la usa):
  - `EngineOptions::profile`: es el `-p` de `lirtup run`. Entra en la configuración de arranque,
    así la FG del perfil cuenta para `prepare`.
  - `Engine::config` y `Engine::quit_signal`.
  - `prepare_headless`, para `bench` y `list gpus`: corre `prepare` con la FG apagada, para que la
    capa no entre en esos procesos.
  - `pub use tokio::sync::broadcast`, para la CLI, que no depende de tokio.
  - `CoreError::{Gpu, Magpie, Protocol, Disconnected}`.
- **`server`.**
  - **Antes de escuchar.** Crea la carpeta con 0700 y pone el socket en 0600. Si el socket ya
    existe y responde, devuelve `AlreadyRunning`; si no responde, lo borra.
  - **Clientes.** Atiende una tarea por cliente:
    - con `Hello`, se suscribe y manda el `Snapshot`, en ese orden, así no se pierde ningún
      evento, aunque alguno puede llegar dos veces;
    - los comandos van a `spawn_blocking(handle)`;
    - si un comando falla, ese cliente recibe un `Log { Error }`;
    - una línea mala da un `Log { Warn }`;
    - un `Hello` de otra versión recibe el `Snapshot` y se cierra la conexión;
    - un cliente atrasado (`Lagged`) recibe un `Snapshot` nuevo.
  - **Cierre.** Termina con `Quit`, SIGINT o SIGTERM. Antes de terminar borra el socket y llama a
    `shutdown`.
  - **Funciones públicas.** `server::run` arma el runtime multihilo para `lirtup serve`, y
    `ensure_not_running` revisa el socket antes de crear el `Engine`.
- **`client`.** Es un cliente bloqueante (`connect`, `hello`, `send`, `recv`, `recv_timeout`,
  `is_closed`) que usan `lirtup ctl` y los tests. La GUI va a tener el suyo, asíncrono.
- **`listing`.** Convierte a los tipos del protocolo las ventanas, los monitores, las GPU (con la
  elegida marcada), el catálogo de efectos ya leído (un archivo que no se puede leer aparece con
  `error`) y las versiones de los compiladores. Lo usan el `Engine` y `lirtup list`.
- **`signals::on_terminate`.** Atiende SIGINT y SIGTERM en un hilo con un runtime de un solo hilo.
  Una segunda señal sale con 130.

### La superposición (`surface.rs`, `overlay.rs`, `input.rs`)

- **`OverlayThread`.**
  - **El hilo.** Crea la ventana y el `InputForwarder` en el hilo `lirtup-overlay`, y `spawn`
    vuelve cuando la ventana existe.
  - **El bucle.** Toma los comandos, después espera eventos X en tramos de 4 ms
    (`EVENT_SLICE`), así un comando espera como mucho un tramo.
  - **Los eventos.**
    - El puntero y las teclas se reenvían con `SendEvent`.
    - Una tecla que coincide con un atajo de la sesión da `OverlayNotice::Hotkey`, y no se
      reenvían ni la tecla ni su suelta.
    - `Configure`, `Expose` y `Close` pasan a notificaciones.
    - Si se cae la conexión, manda `Failed`.
  - **El cierre.** `close` y `Drop` mandan `Close` y esperan con límite de tiempo.
- **`SetMode` siempre responde `Configured`.** El esqueleto lo prometía, pero `OverlayWindow`
  guardaba el rectángulo nuevo antes del `ConfigureNotify` y el evento se filtraba. Ahora:
  - `set_mode` deja el rectángulo viejo hasta que llega el `ConfigureNotify`;
  - las pistas de tamaño usan el rectángulo pedido;
  - si el gestor de ventanas no mueve nada en 250 ms, el hilo informa la geometría tal como está.
- **Nuevas, a pedido de P** (son variantes que se agregan, no cambian nada de lo que ya hay):
  - `OverlayCommand::SetTarget(u32)` cambia de ventana sin rehacer la superposición. Usa
    `OverlayWindow::set_target` (`WM_TRANSIENT_FOR`) y `InputForwarder::set_target`, que antes le
    manda el `LeaveNotify` a la ventana anterior.
  - `OverlayCommand::HideCursorAfter(Option<Duration>)` es `hide_cursor`. Esconde el cursor tras
    N sin movimiento y lo muestra con el siguiente movimiento. `SetCursorVisible(false)` gana.
- **`lirtup_x11::input::HotkeyMatcher`.** Se arma con `InputForwarder::hotkey_matcher(bindings)`:
  lee el mapa del teclado una vez e ignora Lock, NumLock y ScrollLock, igual que los atajos
  globales. Tiene un test unitario.
- **XTest.** Sigue sin estar, como decidió el esqueleto. Solo hay `SendEvent`, y el doc de
  `InputForwarder` lo dice.

### `lirtup` (`crates/lirtup-cli/src/main.rs`)

- **Logs.** Van a stderr: `-q` muestra avisos, sin bandera hay info, `-v` muestra debug y `-vv`
  muestra trace. `RUST_LOG` gana. stdout queda para lo que imprime cada subcomando.
- **`run`.**
  1. Si hay `-- PROGRAMA`, lo lanza **antes** de `Engine::new`, así el juego hereda el entorno del
     usuario y no el `DISABLE_LSFGVK` del escalador.
  2. Crea el `Engine`, que corre `prepare`.
  3. Elige la ventana, en este orden:
     - el PID del programa;
     - `--select`, que muestra una lista numerada y lee la elección por stdin;
     - `target_title_regex`, que resuelve el XID en ese momento;
     - `target_title`;
     - si no hay nada de lo anterior, la ventana enfocada después de `target_delay` (5 s por
       defecto, con un aviso en stderr).
  4. Elige el tipo: `Windowed` si `overlay_mode` es `windowed`, y `Fullscreen` si no.
  5. Imprime los eventos hasta `SessionStopped`.

  Además:
  - el código de salida es 1 si hubo un `SessionError`;
  - Ctrl+C hace `shutdown` y sale con 130;
  - `--daemon` todavía no anda: es de la ronda 4.
- **`serve [--socket P] [--daemon]`.** Hace `ensure_not_running`, después `Engine::new`, después
  `server::run`.
- **`ctl <COMANDO> [--follow]`.**
  - **Qué hace.** Conecta, manda `Hello` y después el comando, e imprime cada evento como una línea
    del protocolo.
  - **Cuándo termina.** Al llegar la respuesta de ese comando, o al vencer un tiempo de espera:
    - `PickWindow` y `Bench` esperan sin límite;
    - `Start` espera 120 s;
    - los comandos sin respuesta esperan 1 s.

    Con `--follow` no termina nunca.
  - **Cómo se escribe el comando.**
    - Como JSON de un `Command`.
    - O como un nombre (en cualquier caso, con `_` o `-`) seguido de `clave=valor`, donde el valor
      es JSON si se puede leer como JSON, y texto si no.
    - Hay atajos: `window=0x…`, `pid=`, `title=`, `windowed` y `focused` en `start`, y el
      argumento suelto de `hotkey` y de `compile_effect`.
    - `start` sin nada usa la ventana enfocada, en pantalla completa, ya.
    - `ctl hello` imprime el `Snapshot`.
  - **Código de salida.** Es 1 si llega un `Log { Error }` o un `SessionError`.
  - **Tests.** Hay tests unitarios del parser.
- **`list windows|models|effects|gpus|monitors`.** Muestra lo mismo que `listing.py`, en tablas.
  `gpus` corre `prepare_headless` antes de crear la instancia.
- **`bench <REQ> [--json] [--gpu G]`.**
  1. Corre `prepare_headless`.
  2. Elige el dispositivo con `--gpu`, o con `gpu:` de la configuración.
  3. SIGTERM y SIGINT ponen la bandera de cancelación.
  4. Corre `bench::run`. Con `--json`, imprime un `BenchProgress` al empezar y uno por caso, y un
     `BenchResult` al final.
  5. Guarda `bench.json` con `save_results`.

  Sin `--json`, imprime una tabla.

## Lo que depende de otros frentes

Con S, P y F sin mergear, esto compila pero entra en `todo!()` al correr:

| Llamada | Frente | Dónde |
| --- | --- | --- |
| `framegen::status_from_setup` | F | `Engine::new` (todo `run` y `serve`), `FrameGenProbe`, `ReloadConfig` sin sesión |
| `Session::{start, send, is_finished, stop}` | P | cualquier `Start` |
| `bench::{run, save_results, results_path}` | S | `lirtup bench` |

El F que está en la carpeta de notas ya trae `status_from_setup`. Lo que no depende de nadie
(`list`, `ctl` y el socket) anda en cuanto `Engine::new` pasa ese punto.

Lo que asumo de P (está en su doc; lo anoto para la integración):

- Tras un `SessionError` manda `SessionStopped { Error }`, y en cualquier otro final manda
  `SessionStopped`.
- `is_finished()` pasa a `true` cuando el hilo termina. El `Engine` deja de mostrar la sesión en
  la foto (`snapshot().session`) en cuanto eso pasa, aunque el `SessionStopped` llegue después.
- Para `SwitchWindow` y `hide_cursor` puede usar los comandos nuevos (`SetTarget`,
  `HideCursorAfter`). Después de un `SetMode` llega siempre un `Configured`.

## Dependencias que pido

- **`libc` en `lirtup-core`** (ya está en el workspace), para mandar SIGTERM al hijo de la medición
  con `libc::kill` en lugar de lanzar `kill(1)`. Es una línea `#[allow(unsafe_code)]` con su
  `SAFETY`. No la agregué porque cambia `Cargo.lock`.

Nada más: tokio, crossbeam, arc-swap, serde_json y clap alcanzan. La CLI no necesita tokio, porque
usa `lirtup_core::broadcast` y `lirtup_core::signals`.

## Cambios de firma o de contrato que pido

- **Contrato (`lirtup-proto`)**: ninguno obligatorio. Una aclaración para el doc de
  `Event::SessionError`: siempre la sigue un `SessionStopped { Error }`, también cuando el arranque
  falla antes de que exista la sesión. Así lo hace ahora el `Engine`, y la GUI puede contar con eso.
- **Al revisor, en `lirtup-x11/src/lib.rs`** (no es mío): reexportar `input::HotkeyMatcher` en la
  raíz, opcional.
- **Pedido de P «avisar si hay una GUI escuchando»**: es `SessionSetup` o `PipelineCommand`, que
  son de P. Por mi lado, `Engine` lo sabe con `events.receiver_count()`. Si P agrega
  `PipelineCommand::SetTimestamps(bool)`, lo mando al conectarse y desconectarse el primer y el
  último cliente. Queda para la integración.
- **`lirtup_fg::layer::restart_self`** no permite agregar variables al `exec`. Para que el
  escalador reiniciado retome la sesión (hoy la GUI tiene que mandar `Start` otra vez), haría falta
  `restart_self_with(env: &[(&str, &str)])`. Es de F (o del revisor). No es urgente.

Las firmas que ya existían y cruzan frentes no cambiaron. Lo que se agregó en archivos de K está en
«Qué hice».

## Pendiente

- Correr los tests: `cargo test -p lirtup-proto -p lirtup-x11 -p lirtup-cli`.
- Los criterios de 5.8 de K, de punta a punta, en la integración y con S, P y F mergeados:
  - el reenvío del ratón y los atajos;
  - `run`, `serve` y `ctl`;
  - el swapchain sobre la superposición, con el presentador de P.
- **El riesgo del esqueleto sigue abierto:** la misma conexión XCB la usan el hilo de la
  superposición (`poll_for_event`) y el WSI de Vulkan desde los hilos que presentan. libxcb es
  seguro entre hilos, pero hay que mirarlo con la validación activa
  (`LIRTUP_VULKAN_DEBUG=1`).
- **El daemon** (ronda 4): hoy `SetDaemon` y `--daemon` solo guardan el estado.
- **XTest**: solo si algún juego ignora los eventos sintéticos en las pruebas.
- **`Restart` con la capa**: el proceso nuevo no retoma la sesión sola (ver el pedido de arriba).
- **El foco y `StartFullscreen`**: si la ventana enfocada es la GUI o una superposición vieja, se
  escala esa. Hace falta filtrar por `_LIRTUP_OVERLAY` y por la clase `lirtup`. Es chico; lo dejo
  para cuando se pruebe con la GUI.

## Guion para probar a mano (después de la integración)

```bash
# 0. Compilar.
cargo build -p lirtup-cli
alias lirtup=target/debug/lirtup

# 1. Listados (sin GPU salvo `gpus`).
lirtup list windows
lirtup list monitors
lirtup list gpus          # sin el aviso del cargador sobre lsfg-vk
lirtup list effects | head
lirtup list models

# 2. run: escalar la ventana enfocada (5 s para enfocarla), Ctrl+C para salir.
lirtup run -v
#    Con un programa (el juego hereda el entorno sin DISABLE_LSFGVK):
lirtup run --overlay-mode windowed -- glxgears
#    Con un título y un perfil:
lirtup run -t mpv -p "Mi perfil"
#    Probar: el ratón y el teclado llegan al juego; Alt+Shift+I (stats),
#    Alt+Shift+M (modelo), Alt+Shift+G (geometría), Alt+Shift+Escape (salir).

# 3. serve + ctl (otra terminal).
lirtup serve -v
lirtup ctl hello                                   # imprime el Snapshot
lirtup ctl list_devices
lirtup ctl frame_gen_probe
lirtup ctl 'start window=0x04200007 windowed'      # XID de `list windows`
lirtup ctl 'start delay_s=3'                       # la enfocada, en 3 s
lirtup ctl 'hotkey toggle_stats'
lirtup ctl --follow hello                          # Stats cada 500 ms; Ctrl+C
lirtup ctl reload_config
lirtup ctl restart
lirtup ctl stop
lirtup ctl pick_window                             # cursor en cruz
lirtup ctl 'compile_effect FSR/FSR_EASU'
lirtup ctl list_effects | head -c 600
lirtup ctl '{"type":"Bench","req":{"src":{"width":1280,"height":720},"dst":{"width":2560,"height":1440},"mode":null,"effects":[],"framegen":false,"frames":30}}'
lirtup ctl cancel_bench                            # desde otra terminal, durante la medición
lirtup ctl diagnostics
lirtup ctl quit                                    # serve sale y borra el socket

# 4. El socket a mano.
socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/lirtup/lirtup.sock
#   {"v":1,"type":"Hello","version":1,"client":"socat"}
#   {"v":2,"type":"Hello","version":2,"client":"viejo"}   → Snapshot y cierre
#   no json                                                → Log Warn

# 5. Casos de borde.
lirtup serve & sleep 1; lirtup serve   # «another lirtup serve is running»
kill -9 %1; lirtup serve               # borra el socket viejo y arranca
lirtup ctl 'start title=NoExiste'      # SessionError{window} + SessionStopped{error}, salida 1
```

Qué mirar en cada paso: que `SessionStarted` llegue después del primer cuadro; que `Configured`
llegue después de `cycle_geometry`; que el CSV de latencia y el panel muestren lo mismo que
`ctl --follow`; que con FG por capa `restart` reinicie el proceso (el PID cambia y el socket
vuelve).
