# Ronda 4, frente Y: bandeja, daemon, avisos y ciclo de vida

Rama **`claude/rust-r4-tray-ab3db0`**, que sale de `rust/r4-skeleton` (`27b7d08b`). Fecha:
2026-11-01. Sin push y sin merge.

Compilé con `CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r4-y`.
- `cargo clippy -p lirtup-core -p lirtup-cli -p lirtup-fg -p lirtup-ui --all-targets`: sin avisos
  en lo mío (quedan los `todo!()` de M y T).
- `cargo test --workspace`: **404 pasan**, 0 fallan, 4 ignorados (los de siempre). Hay 13
  tests nuevos: 8 en `lirtup-core` (`daemon` y `lifecycle`) y 5 en `lirtup-ui` (`tray`, `notify`
  y la demora de Inicio).
- `scripts/check-ftl-keys.py`: 594 claves en cada idioma, ninguna falta.
- `dx build -p lirtup-ui` compila. Después de cada build devolví `assets/tailwind.css` a la
  versión de git.

## Commits

| Commit | Qué |
| --- | --- |
| `cf02409d` | Núcleo: `daemon.rs`, `lifecycle.rs`, su conexión en `engine.rs` y `server.rs`, `restart_self` con variables, `serve --exit-with-clients` y `run -d` |
| `a8cefb6d` | GUI: `tray.rs`, `notify.rs`, cerrar y minimizar, instancia única, filas de Ajustes, `start_delay_s`, `PauseHotkeys` |
| `61371f48` | Arreglos de la prueba en vivo con Plasma (tres de ellos importantes, ver «Hallazgos») |

## Lo hecho

### Núcleo

| Pieza | Qué hace |
| --- | --- |
| `daemon.rs` | Un hilo con su conexión X que cada `daemon_poll_interval` lista las ventanas y, sin sesión ni arranque en curso, toma la primera que coincide con un perfil vigilado (con reglas y sin `daemon_exclude`). Como la 1.x, recorre primero los perfiles en orden y después las ventanas. Enfoca la ventana, manda `DaemonMatch` y arranca con ese perfil, en su modo de superposición, después de `target_delay`. Una ventana cuya sesión terminó sin cerrarse (la detuvo la persona o falló) no se vuelve a escalar hasta que se cierra; tampoco la que la persona escalaba cuando se encendió el daemon. Nunca toma ventanas de LirtUp Forge. `watched` y `pick` son puras y tienen tests |
| `lifecycle.rs` | `Lifecycle`: un `serve` lanzado por la GUI (`--exit-with-clients`) sale `LINGER` (5 s) después de que se va el último cliente, o del arranque si nadie se conecta. No sale si el daemon está encendido con `keep_after_exit` o si hay una sesión. `Resume` (`encode`/`decode`/`take_from_env`). `launch`/`Launched`: el programa de `lirtup run -- prog`, con X11 forzado, que se termina al salir (filas 2 y 3) |
| `engine.rs` | `SetDaemon` enciende y apaga el hilo; el daemon arranca con `--daemon`, `daemon: true` en el nivel superior o lo que trae un `Restart`. `ReloadConfig` reinicia el hilo si cambia `daemon_poll_interval`. Un `Start` sobre una ventana dada (no `Focused`) la enfoca primero (fila 4). `Target::Pid` busca también por clase, con el nombre del ejecutable como pista (fila 1). `Restart` con la capa pasa la sesión (`LIRTUP_RESUME`) y el daemon (`LIRTUP_RESUME_DAEMON`) al proceso nuevo, y `Engine::new` los toma. Métodos nuevos: `session_running` y `daemon_keeps` |
| `server.rs` | Cuenta las conexiones y le pregunta a `Lifecycle` cada segundo. Sin clientes conectados, avisa con `notify-send` cuando una sesión empieza (con «Detener», que manda `Stop`) o falla. Es el aviso de 3.8 para la GUI cerrada; el texto sale en español o en inglés según `LANG`. `serve` y `run` reciben el `Lifecycle` |
| `lirtup-fg/src/layer.rs` | `restart_self(env)`: las variables del proceso nuevo |
| `lirtup-cli/src/serve.rs` | `serve(…, exit_with_clients)` y `daemon_foreground` (`run -d`, fila 22) |

### GUI

| Pieza | Qué hace |
| --- | --- |
| `tray.rs` | El ícono con sus **4 estados**, dibujados en código a 64 px: en reposo (contorno gris), daemon vigilando (anillo primario), escalando (relleno ámbar con punto) y error (rojo hasta que se abre la ventana o se usa el menú). `status(store)` es pura y tiene test. El menú del diseño: «Escalando: …» con «fps · modo», Detener, Reiniciar, Perfil ▸ (reinicia la sesión con otro perfil), Generación de cuadros ▸ (Apagada, ×2, ×3, ×4, Auto, con el motivo si falta el motor; guarda solo `frame_gen_multiplier` en Predeterminado y reinicia la sesión cuando salió `ReloadConfig`), Modo daemon, Mostrar LirtUp Forge y Salir |
| Cerrar y minimizar | En `app.rs`, la barra de título llama a `tray::close_window` y `tray::minimize_window`; también el cierre del gestor de ventanas (Alt+F4, `WindowHides`). Sin ícono en la bandeja, cerrar sale aunque `close_to_tray` esté encendido |
| Salir | Si hay cambios sin guardar, abre `QuitDialog` (Guardar y salir / Descartar y salir / Cancelar). Después detiene la sesión y apaga el daemon, salvo que el daemon siga con «Seguir ejecutándose tras salir». Guarda la geometría y cierra. El `serve` lanzado por la GUI sale solo (fila 35) |
| Ocultarse al escalar | Al empezar una sesión, la ventana visible se oculta en la bandeja (o se minimiza sin bandeja) y vuelve al terminar (fila 33) |
| Instancia única | `gui.sock` junto al socket del escalador: una segunda `lirtup-gui` pide «show» a la primera y sale (fila 32) |
| Geometría | `window_geometry` (`x`, `y`, `width`, `height`, `maximized`) en `gui-config.yaml` (en `extra`), guardada al salir y usada al abrir (fila 40) |
| Empezar oculto | `start_hidden` con el ícono encendido |
| `notify.rs` | Avisos con `notify-rust` cuando una sesión empieza («Detener» manda `Stop`) o falla. Solo con la ventana oculta o sin foco, y solo en los cambios: la primera mirada, que es el `Snapshot`, no avisa. `notice` es pura y tiene test |
| Ajustes ▸ General | Filas nuevas: «Mostrar ícono en la bandeja» y «Minimizar a la bandeja». Sin ícono, las tres de la bandeja quedan deshabilitadas |
| Inicio | El escalado diferido usa `gui.start_delay_s` y el menú lo guarda; un valor fuera de la lista (0 a 60) aparece como una opción más. Con 0 no hay demora |
| `PauseHotkeys` | `HotkeyCapture` tiene `on_capture`. Atajos manda `PauseHotkeys { on }` mientras captura (fila 41) |
| `link.rs` | Lanza `serve --exit-with-clients` en su propio grupo de procesos (un Ctrl+C a la GUI no lo mata) |

## Hallazgos de la prueba en vivo

1. **Los clics de la bandeja no llegaban.** `muda` tiene un solo manejador global
   (`OnceCell`). dioxus-desktop 0.7.10 lo pone dos veces: primero el de la barra de menús
   (`MudaMenuEvent`) y después el de la bandeja (`TrayMenuEvent`), que se pierde. Escucho los dos.
   Si se actualiza dioxus, hay que revisarlo.
2. **Con la ventana oculta, toda la GUI se congelaba**, incluido el enlace con `serve`. Por
   ejemplo, «Detener» desde la bandeja no salía hasta mostrar la ventana. El intérprete de Dioxus
   aplica los cambios en `requestAnimationFrame` y espera a que terminen para volver a sondear las
   tareas, y WebKit no corre `requestAnimationFrame` en una ventana oculta. Con una ventana creada
   oculta, Dioxus entra en modo `headless` y aplica los cambios en el momento. Por eso `main.rs`
   crea la ventana oculta y `tray.rs` la muestra al arrancar, salvo con «Empezar oculto». El
   problema ya existía en la ronda 3 con la ventana minimizada.
3. **libdbusmenu no muestra el texto nuevo de un submenú.** El menú se rehace (`set_menu`) cuando
   cambia su forma: el texto de Perfil o de Generación de cuadros, la lista de perfiles o la
   línea de detalle. Todo lo demás se actualiza en el lugar, para no cerrar un menú abierto cada
   500 ms.

## Lista de paridad: mis filas

| # | Estado |
| --- | --- |
| 1 | **Hecha.** `Target::Pid` usa como pista de clase el nombre del ejecutable (`/proc/<pid>/cmdline`, si no `comm`) |
| 2 | **Hecha.** `Launched` termina el programa al acabar `run`, también si la ventana no aparece y con SIGINT o SIGTERM |
| 3 | **Hecha.** `XDG_SESSION_TYPE=x11`, sin `WAYLAND_DISPLAY`, `MESA_VK_WSI=x11` y `RADV_DEBUG=no_wayland_wsi`. `__GLX_VENDOR_LIBRARY_NAME` queda fuera a propósito (comentado en `FORCE_X11_ENV`) |
| 4 | **Hecha.** El motor enfoca la ventana de todo `Start` que no sea `Focused`: `-t`, `--select`, la GUI y el daemon |
| 22 | **Hecha.** `run -d` corre el daemon en primer plano |
| 32 | **Hecha** (instancia única) |
| 33 | **Hecha** (se oculta al escalar y vuelve) |
| 34 | **Distinto a propósito (propuesta).** `ExitApp` solo detiene la sesión. Con la bandeja encendida, que es lo predeterminado, la 1.x tampoco cerraba la GUI. Para el caso sin bandeja haría falta un `StopReason::ExitApp` en `proto`, que es del revisor |
| 35 | **Hecha** (pregunta por los cambios sin guardar y detiene la sesión y el daemon) |
| 40 | **Hecha** (`window_geometry`). La geometría de QSettings de la 1.x es un blob de `saveGeometry` y no se migra: distinto a propósito |
| 41 | **Hecha** |
| 43 | **Hecha**, salvo «cambiar de ventana» desde la bandeja, que no está en el menú del diseño. Perfil ▸ ofrece solo los perfiles con nombre, porque el protocolo no puede pedir «Predeterminado» (`profile: None` es «el que coincida») |
| 44 | **Hecha**: intervalo, exclusión y volver a buscar al cerrarse la ventana. **Falta** la coordinación con `follow_focus`: hoy, mientras hay sesión, el daemon no actúa y la sesión sigue el foco por su cuenta (va con las filas 7 y 8 del revisor) |
| 45 | **Hecha** (`Resume`, «Seguir ejecutándose» y los avisos con «Detener») |

## Lo que pido

- **Revisor, archivos fuera de mi reparto.** Los toqué porque lo pedían mis filas. Son cambios
  mínimos y comentados:
  - `lirtup-cli/src/main.rs`: la bandera `--exit-with-clients` de `serve`, `run -d` →
    `serve::daemon_foreground`, y el programa de `run` con `lifecycle::launch`;
  - `lirtup-ui/src/main.rs`: instancia única, ventana creada oculta (hallazgo 2), geometría y
    `WindowHides`;
  - `lirtup-ui/src/link.rs`: `--exit-with-clients` y `process_group(0)`;
  - `components/hotkey.rs`: la prop `on_capture`;
  - `app.rs`: además de los manejadores, la barra de título dibuja `tray::QuitDialog`.
- **Revisor, `state`:** Generación de cuadros ▸ despacha un `Action::Core(Event::SaveSetting)`
  sintético para guardar solo el multiplicador (el camino de la barra de T). Sería más limpio un
  `Action::SaveSetting` propio, o un `restart` en `save_setting`, en vez de esperar a que
  `saving` quede vacío.
- **Revisor, decisión:** ¿el interruptor del daemon se guarda en `daemon:` de Predeterminado?
  Hoy dura lo que dura `serve`, y `daemon: true` en el YAML lo enciende al arrancar.
- **Revisor, fila 34:** confirmar «distinto a propósito» o agregar `StopReason::ExitApp`.
- **M (si integra después):** `window_geometry` ya existe; la de QSettings no se migra (es un
  blob).
- **P:** el `.desktop` puede usar `lirtup-forge` como nombre de ícono: es el que piden los avisos
  (`notify-send --icon` y `notify-rust`).
- Ningún `Cargo.toml` nuevo: `notify-rust` ya estaba y `serve` avisa con `notify-send`, sin
  dependencias.

## Pendiente o sin probar

- **Notas sobre `lirtup run`:** un `Restart` con la capa en `run -- prog` vuelve a lanzar el
  programa, porque `exec` repite los argumentos. Pasaba antes de esta ronda; `serve` no tiene el
  problema.
- La posición guardada bajo KWin con XWayland: se guarda (`x: 560, y: 355` en la prueba), pero
  no comprobé que KWin la respete al abrir. Va al guion.
- `QuitDialog` («Guardar y salir»), los avisos de `notify-rust` con su botón y los atajos reales
  durante la captura necesitan manos: van al guion.
- Mientras la ventana está oculta, la GUI corre en modo `headless` de Dioxus. Funciona en las
  pruebas, pero no medí si cambia algo en la fluidez con la ventana visible.

## Pruebas en vivo que hice (KWin con XWayland)

Con un `XDG_CONFIG_HOME` de prueba (perfil «Cubo» que coincide con `vkcube` y «Gears» con
`daemon_exclude`) y `XDG_RUNTIME_DIR=/run/user/1000/lirtup-y-test`:

| Prueba | Resultado |
| --- | --- |
| `serve --daemon` con `vkcube --wsi xcb` y `glxgears` abiertos | Escala `vkcube` en modo ventana; `glxgears` no (`daemon_exclude`) |
| `ctl Stop` con el daemon encendido | No vuelve a escalar `vkcube` mientras sigue abierta; al cerrarla y reabrirla, la escala otra vez |
| `serve --exit-with-clients` sin clientes | Sale a los 5 s |
| `… set_daemon on=true keep_after_exit=true` y el cliente se va | **Sigue escalando** pasados 12 s (criterio de 5.8) |
| Apagar el daemon y detener | Sale a los 5 s |
| `Restart` con la capa después de pasar el multiplicador de 1 a 2 | `exec`, «resuming the session», sesión otra vez en modo ventana, FG `active ×2`, daemon encendido con `keep` |
| La bandeja en Plasma (leída por D-Bus: `StatusNotifierItem` y `dbusmenu`) | Menú completo y traducido; los **4 estados** del ícono vistos en sus PNG |
| «Modo daemon» desde el menú (`dbusmenu.Event`) | La GUI lanza `serve --exit-with-clients`, el daemon escala `vkcube`, la ventana se oculta y el ícono pasa a ámbar |
| «Detener» desde el menú con la ventana oculta | Llega al núcleo; la ventana vuelve y el ícono pasa a «vigilando» |
| Error con la ventana en la bandeja (`ctl start title=no-existe`) | Ícono rojo; «Mostrar» lo devuelve a reposo |
| Segunda `lirtup-gui` | «already running; showing it» y sale |
| Cerrar a la bandeja (clic por `LIRTUP_GUI_EVAL`) | La ventana se oculta y la app sigue viva |
| Salir desde el menú | Guarda `window_geometry` y el proceso termina |
| Captura en Ajustes ▸ Atajos (clic por `LIRTUP_GUI_EVAL`, después Cancelar) | `serve`: «global hotkeys paused» y «resumed» |

## Guion de pruebas a mano: bandeja y daemon

Antes: una copia de la configuración (`XDG_CONFIG_HOME` de prueba), con un perfil cuyas reglas
coincidan con una ventana de prueba (`vkcube --wsi xcb`) y otro con `daemon_exclude: true`. Sin
otro `lirtup serve` ni `lirtup-gui` abiertos.

1. **Ícono y menú.** Abrir `lirtup-gui`. En el panel aparece el diamante gris. Clic derecho (o
   izquierdo): «En reposo», Detener y Reiniciar deshabilitados, Perfil ▸ con los perfiles,
   Generación de cuadros ▸ con «Apagada» marcada, Modo daemon, Mostrar LirtUp Forge, Salir.
   Cambiar el idioma en Ajustes: el menú cambia de idioma.
2. **Daemon desde la bandeja.** Marcar «Modo daemon». El ícono pasa a diamante con anillo azul y
   el encabezado dice «Daemon vigilando N perfiles». Abrir la ventana de prueba: en el intervalo
   de sondeo empieza a escalar, el ícono pasa a ámbar con punto y la ventana de LirtUp Forge se
   oculta. Llega un aviso «Escalando: «…»» con el botón «Detener».
3. **«Detener» del aviso.** Detiene la sesión; la ventana de LirtUp Forge vuelve; el daemon no
   vuelve a escalar esa ventana mientras siga abierta. Cerrarla y abrirla de nuevo: la vuelve a
   escalar.
4. **Perfil ▸ y Generación de cuadros ▸** con una sesión: elegir otro perfil reinicia la sesión
   con él (queda marcado). Elegir ×2: se guarda en Predeterminado (aviso en la GUI) y la sesión
   se reinicia con FG. Con la capa lsfg-vk, `serve` se reinicia solo y la sesión vuelve.
5. **Error.** Con la ventana en la bandeja, provocar un fallo (escalar una ventana que se cierra
   enseguida). Ícono rojo y aviso «No se pudo escalar: …». Abrir la ventana o el menú: el ícono
   vuelve a su estado.
6. **Cerrar y minimizar.** Con «Cerrar a la bandeja» encendido, la ✕ y Alt+F4 ocultan la ventana
   (la app sigue en el panel). Con «Minimizar a la bandeja», el botón minimizar la oculta en la
   bandeja, no en la barra de tareas. Apagar «Mostrar ícono en la bandeja»: el ícono desaparece,
   las otras tres filas quedan deshabilitadas y la ✕ sale de la app.
7. **Empezar oculto.** Encenderlo, salir y abrir otra vez: solo aparece el ícono; «Mostrar» abre
   la ventana. Abrir una segunda `lirtup-gui`: la primera se muestra y la segunda no queda.
8. **Salir con cambios sin guardar.** Cambiar un ajuste de un perfil sin guardar y elegir
   «Salir» en el menú. Aparece «Cambios sin guardar»: «Cancelar» deja todo como estaba;
   «Guardar y salir» guarda y cierra; «Descartar y salir» cierra sin guardar.
9. **Seguir ejecutándose tras salir.** Con el daemon encendido y la opción **apagada**, «Salir»:
   en unos 5 s no queda ningún `lirtup serve` (`pgrep -a lirtup`). Con la opción **encendida**,
   «Salir»: `lirtup serve` sigue, y una ventana que coincide se escala sola, con un aviso del
   escritorio (de `notify-send`) con «Detener». `lirtup ctl Quit` lo termina.
10. **Geometría.** Mover y redimensionar la ventana y salir. Al abrir, vuelve al mismo lugar y
    tamaño, y también maximizada si lo estaba (comprobar que KWin respete la posición).
11. **Atajos con el escalador corriendo.** En Ajustes ▸ Atajos, «Cambiar atajo» de «Escalar en
    pantalla completa» y pulsar Alt+Shift+A: la captura lo recibe (antes se lo quedaba
    `serve`). Esc cancela y el atajo global vuelve a funcionar.
12. **Escalado diferido.** En Inicio, elegir 5 s en el menú de la demora: queda guardado (sale y
    vuelve igual) y «Empezar» espera 5 s.
13. **`lirtup run`.** `lirtup run -- vkcube`: escala la ventana del programa y, al detener (Ctrl+C
    o Alt+Shift+Escape), el programa se cierra. `lirtup run -d`: queda vigilando y escala las
    ventanas que coinciden hasta Ctrl+C.
