# Ronda 4, frente T: la barra sobre el juego

Rama **`claude/rust-r4-toolbar-827f6a`**, que sale de `rust/r4-skeleton` (`27b7d08b`). Fecha:
2026-11-01. Sin push y sin merge.

- `cargo clippy --workspace --all-targets` (rustc 1.99.1): **sin avisos en los archivos de T**.
  Quedan solo los `todo!()` de M e Y.
- `cargo test --workspace`: **407 pasan**, 0 fallan y 5 ignorados. El esqueleto tenía 390; los 17
  nuevos son de T.
- El test con GPU `toolbar::tests::bar_on_a_4k_screen` (ignorado sin `--ignored`): **pasa** en la
  RX 9070 XT.
- Target en `/home` (`CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r4-t`).
- No toqué `Cargo.toml`, los archivos de bloqueo, la versión ni el `CHANGELOG.md`, ni borré Python
  ni moví datos.

## Commits

| Commit | Qué |
| --- | --- |
| `62f68973` | `gen-tokens-icons.py`: las dos instancias `.ttf` de Material Symbols; `crop` entra en `icons.json` y `tokens.rs` |
| `bf8397fd` | `ui_draw.rs` (`UiDraw`) y `OverlayBlend::with_label` en `osd.rs` de efectos |
| `9918ccfe` | `overlay_input.rs`: `ToolbarRouter`, `begin_move`, `X11::pointer_position` |
| `9caf89c2` | `tokens.rs` (`color`, `ease`) y `toolbar.rs` (`Toolbar`), con el test con GPU |
| `80f6f701` | OSD y panel de stats con los tokens y las fuentes nuevas |
| `e2145ce9` | La conexión en `surface.rs` y `pipeline.rs`, y las filas de paridad |
| (este) | Esta nota |

## Qué hice

### La barra (`toolbar.rs`)

Sigue la mesa de trabajo «Superposición» del lienzo y las clases de `lf.css` (`.ftb.glass`,
`.ibtn`, `.btn-xs`):

- **Medidas.** 64 px de alto, redondeada del todo, a 16 px del borde de arriba y centrada. Botones
  redondos de 40 px con íconos de 24 px, separados 2 px. El divisor mide 1×28 y lleva 6 px de
  margen.
- **Multiplicador.** Botón tonal de 32 px en `tertiary` al 18 %.
- **Lectura de fps.** JetBrains Mono 15/600: «60 → 120 fps» mientras se generan cuadros y «144 fps»
  sin generación. El ancho alcanza para tres cifras, así la barra no se mueve cuando cambian los
  números.
- **Escala.** Las medidas se multiplican por `ui_scale`: 1 hasta 1440 líneas, y después en
  proporción, hasta 2.
- **Estados.**
  - Fijar en `primary` con `shape-md` y el ícono relleno (FILL 1).
  - Stats activo con `primary` al 18 %.
  - Hover y pulsado con `state-hover` y `state-pressed`. Al pulsar, el botón se cuadra.
  - Detener en `error`.
- **Animaciones.** Entra con el resorte `fast-spatial` (baja desde arriba, se pasa un poco y se
  asienta) y sale con el fundido `effects`.
- **Cuándo aparece y cuándo se va.**
  - Aparece cuando el puntero toca la banda de 6 px del borde de arriba, o con `ToggleToolbar`
    (Alt+Shift+D).
  - Se va [`HIDE_AFTER`] (2 s) después de que el puntero sale, salvo que esté fijada o que haya
    un botón pulsado.
  - Si se muestra con el atajo, también se va a los 2 s, salvo que el puntero pase por encima.
  - «Ocultar» la esconde hasta que el puntero vuelva al borde, aunque esté fijada.
- **Arrastre.** En modo ventana, apretar sobre el fondo de la barra mueve la ventana: el fondo
  incluye la lectura de fps, los divisores y el relleno. Devuelve `ToolbarAction::Move`, y la
  sesión manda `OverlayCommand::BeginMove` con coordenadas de la raíz.
- **Acciones** (`Runner::toolbar_action`):
  - **Fijar** cambia el estado de la barra y manda `SaveSetting` con la clave `toolbar_pinned`
    (`PINNED_KEY`), que va a Predeterminado. Al empezar la sesión se lee de `Config::extra`.
  - **Stats** hace lo mismo que `ToggleStats`.
  - **Recorte**: si hay una barra de menú recortada, vuelve al recorte del perfil; si no, busca
    la barra de menú otra vez con `MenuBarDetector`. Avisa por el OSD.
  - **Captura** hace lo mismo que `Screenshot`.
  - **Multiplicador**:
    - recorre No → ×2 → ×3 → ×4 → Auto → No;
    - aplica el cambio en vivo con `Presenter::update_config`, avisa por el OSD y manda
      `SaveSetting` con `frame_gen_multiplier`, que es una clave global;
    - el botón solo aparece si la sesión tiene generación de cuadros;
    - con la capa, avisa que el multiplicador se lee al lanzar el juego y no cambia nada.
  - **A ventana / pantalla completa** reconstruye la superposición con el otro `SessionKind`,
    sobre el mismo objetivo, y manda `SessionStarted` otra vez con la vista nueva.
  - **Detener** termina la sesión con `StopReason::User`.

### El dibujo (`ui_draw.rs`)

- **Cómo dibuja.**
  - La escena (rectángulos redondeados, texto e íconos) se rasteriza en la CPU, en una sola
    imagen del tamaño de su caja.
  - La imagen se sube solo cuando cambia y la mezcla `OverlayBlend`, el shader de la 1.x, en un
    solo dispatch sobre esa caja.
  - Los glifos se guardan por fuente, glifo y tamaño. Redibujar, por ejemplo por un fps nuevo o
    por un cuadro de la animación, solo compone máscaras que ya están en la caché.
  - Los bordes se suavizan con distancia con signo.
- **Fuentes** (`UiFonts::from_static`):
  - Google Sans Flex a wght 500 y 600, y JetBrains Mono a wght 400 y 600: son instancias de las
    fuentes variables, hechas con `VariableFont::set_variation`;
  - las dos instancias de Material Symbols;
  - las cuatro van embebidas con `include_bytes!` desde `lirtup-ui/assets/fonts/`
    (`toolbar::ui_fonts`) y se leen una vez por proceso.
- **Tamaños.** Son de CSS: `size` es el em, como `font-size`, no el alto de `ab_glyph`.
- **Error corregido: el alfa se aplicaba dos veces.**
  - `overlay_blend.hlsl` calcula `rgb · a + screen · (1 − a)`. Su cabecera dice «premultiplied»,
    pero esa cuenta es la de alfa directo.
  - El `Canvas` de S entregaba la imagen premultiplicada, así que el OSD y el panel salían más
    oscuros en los bordes y en el fondo: el cristal al 86 % daba 35 en vez de 38.
  - `ui_draw` entrega alfa directo, y el test con GPU comprueba el color del cristal.

### Cambios a las firmas del esqueleto

Son todos en archivos de T:

| Firma | Cambio | Por qué |
| --- | --- | --- |
| `tokens::color` | Devuelve sRGB codificado (0 a 1), no lineal | La textura `screen` es `R8G8B8A8_UNORM` con los bytes de la captura tal cual, y la copia al swapchain no convierte. Un valor lineal saldría más oscuro que en la GUI. Agregué `tokens::with_alpha` |
| `overlay_input::route(zone, &x11rb::Event)` | Ahora es `ToolbarRouter::route(&mut self, &OverlayEvent)`, y `Routed` suma `Both` | El hilo de la superposición ya tiene los `OverlayEvent` traducidos. Además hace falta estado: un botón apretado sobre la barra sigue siendo de la barra hasta que se suelta, y la salida de la barra se avisa una sola vez. El movimiento en la banda va al juego y a la barra |
| `UiDraw::new` y `rebind` | Suman `max_size` (el búfer de subida) y `label` (el nombre en los tiempos de GPU). Agregué `metrics` e `is_shown` | El OSD, el panel y la barra usan `UiDraw`, cada uno con su caja y su nombre |
| `UiFonts` | Lleva `FontArc` ya leídas (`from_static`), no `Arc<Vec<u8>>` | Los clones salen gratis y no se copian 4 MB por instancia |
| `UiFont` | Suma `TextStrong` y `MonoStrong` | Las negritas del diseño (OSD, fps) |
| `ToolbarAction` | Suma `Move { button, x, y }` | El arrastre en modo ventana |
| `ToolbarState` | Suma `capture_fps` y `multiplier_label` | La lectura «60 → 120» y la etiqueta ya traducida |
| `OverlayCommand` | Suma `SetDimmedOpacity(Option<f32>)` | Fila 9 |

`toolbar.rs` también exporta `ICON_NAMES`, `PINNED_KEY`, `ui_fonts` y `ui_scale`, y `pipeline.rs`
exporta `WINDOWED_POSITION_KEY`.

### La superposición (`surface.rs`)

- **Ruteo.** Los eventos del puntero pasan por `ToolbarRouter`:
  - lo que es de la barra vuelve como `OverlayNotice::Pointer` y no llega al juego;
  - sobre la barra, el cursor se ve aunque `hide_cursor` lo haya ocultado.
- **`top-transparent` sin captura del puntero.** En ese modo la ventana deja pasar el puntero:
  - mientras la barra se ve, la región de entrada es la barra (XFixes), así se puede hacer clic;
  - el puntero se consulta cada 50 ms (`POINTER_POLL`) con una conexión propia, para la banda que
    muestra la barra y para la opacidad (fila 9).
- **`BeginMove`** suelta la captura del router, porque después el gestor de ventanas se queda con
  el puntero y no llega el `Release`.

### Panel y OSD

- **Panel.** Usa el cristal oscuro (`glass`) con `shape-lg-inc` (20 px) y está en (20, 96), debajo
  de la barra.
  - Los textos van en JetBrains Mono 12 sobre líneas de 19: las etiquetas en `primary`, los
    valores en `glass-content` y los avisos en `error`.
  - Las filas y el contenido son los de la 1.x, sin cambios.
- **OSD.** Una pastilla `primary-container` de 48 px, con Google Sans Flex 16/600 en
  `on-primary-container`, centrada al 72 % de la altura (`bottom: 28%`).
  - Entra con `fast-spatial` subiendo 16 px. Pasado `osd_duration`, se va con el fundido
    `effects`.
  - Un mensaje que reemplaza a otro no vuelve a entrar.
- Los dos son siempre oscuros, como el cristal del diseño. `Scheme::Light` queda en `tokens` sin
  uso.
- La pasada de la barra se llama `toolbar` en los tiempos de GPU. El panel y el OSD siguen como
  `osd`. `stats::is_overlay` no cuenta ninguna de las dos para el presupuesto.

## Filas de paridad

| # | Estado | Cómo |
| --- | --- | --- |
| 5 | **Cerrada** | `Runner::check_minimized`, cada 500 ms: si el objetivo no es visible (una ventana minimizada queda sin mapear, ICCCM), la sesión se pausa con la superposición oculta y vuelve cuando el objetivo vuelve. Si la ventana ya no existe, termina con `WindowClosed` |
| 9 | **Cerrada** | En `top-transparent`, la opacidad es `overlay_opacity_min` con el puntero fuera del objetivo y `overlay_opacity_max` dentro, como `opacity.py`. El hilo de la superposición consulta el puntero y el rectángulo del objetivo |
| 10 | **Cerrada** | El último movimiento sobre el contenido se reenvía cada 100 ms mientras el puntero está quieto, como `HOVER_KEEPALIVE_INTERVAL_MS`. Se corta al salir, al pasar a la barra o al cambiar de objetivo. En `top-transparent` no hace falta, porque el juego recibe los eventos reales |
| 11 | **Cerrada** | En modo ventana, la posición en la que la barra deja la ventana se guarda 0,8 s después del último `ConfigureNotify` (`SaveSetting` con `windowed_overlay_position: [x, y]`, global). Al crear la superposición, al cambiar de ventana o cuando el objetivo cambia de tamaño, se usa esa posición en lugar de la centrada. Un `Configured` que no cambia el tamaño ya no recrea el swapchain |
| 15 | **Cerrada** | `{timestamp:…}` acepta todo `strftime` de C, en hora local, con `localtime_r` y `strftime`. El título se lee en el momento de la captura y el OSD avisa cuando el archivo ya se escribió. Lo probé: `shot_{timestamp:%H%M%S_%a}` dio `shot_184003_Sun.png` a las 18:40 locales (21:40 UTC) |
| `hide_cursor: 0` | **Cerrada** | `cursor_commands`: con `0`, `SetCursorVisible(false)` para siempre, como en la 1.x. Antes el cursor volvía con cada movimiento y desaparecía al siguiente corte de 4 ms. Sobre la barra se ve igual |

## Medición de GPU con la barra y sin ella (5.8: ≤ 0,1 ms)

**Cumplido, con mucho margen: la barra suma unos 0,002 ms por cuadro.**

1. **Test con GPU a 4K** (`bar_on_a_4k_screen`, `--release`). La barra mostrada y fijada, con
   multiplicador, a escala 1,5 (unos 1040×96 px). La pasada `toolbar` medida con timestamps de
   Vulkan, 50 envíos: **mediana de 0,0014 ms**.
   - En la CPU, la primera rasterización (con los glifos) tardó 1,7 ms y un fps nuevo 1,1 ms. Pasa
     cada 500 ms, y en cada cuadro de una animación de 350 ms, en el hilo de la sesión.
2. **Sesión real a 2560×1440.**
   - `glxgears` a 960×540, SRCNN `fast` y Lanczos, `always-on-top` con FIFO y el panel de stats
     visible.
   - Corrí `lirtup serve` y `ctl start --follow`, dos veces sin la barra y dos con
     `toolbar_pinned: true`. Promedio de 45 muestras de `Stats` por corrida, sin las primeras 4.

   | Corrida | GPU por cuadro (`gpu_ms`) | Pasada `toolbar` |
   | --- | --- | --- |
   | Oculta 1 | 0,4376 ms | — |
   | Fijada 1 | 0,4395 ms | 0,0011 ms |
   | Oculta 2 | 0,4373 ms | — |
   | Fijada 2 | 0,4394 ms | 0,0011 ms |

   La diferencia es de **+0,002 ms**: la pasada `toolbar`, más el ruido entre corridas. La captura
   de esa sesión muestra la barra, el panel y el OSD como en el diseño.

### La espera del principio de cada cuadro («anillo por cuadro»)

**Se puede cerrar.**

- El presentador espera su envío justo después de mandarlo y recién entonces presenta.
- La copia al swapchain va en la misma cola, detrás de la cadena, así que no podría presentarse
  antes aunque hubiera un anillo.
- Lo que se espera es el tiempo de GPU de la cadena, 0,44 ms en la medición de arriba. Es lo
  máximo que un anillo podría solapar con la CPU.
- Lo que se ve en `wait_ms` (3 ms) es el `present` bloqueante de FIFO a 144 Hz, y un anillo no lo
  cambia.

## Hallazgos 2 y 4 de FG

Las dos mediciones se hicieron en esta máquina el 2026-11-01, con la 1.3.0 de pipx y en la misma
sesión. **Conclusión: ninguno de los dos hallazgos se reproduce como diferencia de la 2.0, y no
hace falta tocar `framegen.rs`.**

### Hallazgo 2: los ciclos de FG medidos con el reloj de la CPU

`lirtup bench` y `python -m upscaler.bench --section framegen`, de 1280×720 a 2560×1440 con 300
cuadros por caso. Corrí dos veces cada uno, alternados, con `XDG_CONFIG_HOME` aparte para no tocar
el `bench.json` de la persona. Los 12 casos:

| Caso | Rust (2 corridas) | 1.3.0 (2 corridas) | Diferencia de las medias |
| --- | --- | --- | --- |
| x2 flow 1.0 quality | 1,481 / 1,342 | 1,455 / 1,454 | −0,043 ms (−3,0 %) |
| x3 flow 1.0 quality | 1,681 / 1,498 | 1,619 / 1,680 | −0,060 ms (−3,6 %) |
| x4 flow 1.0 quality | 1,897 / 1,937 | 1,899 / 1,904 | +0,016 ms (+0,8 %) |
| x2 flow 0.5 quality | 0,500 / 0,499 | 0,481 / 0,480 | +0,019 ms (+4,0 %) |
| x3 flow 0.5 quality | 0,786 / 0,790 | 0,760 / 0,763 | +0,026 ms (+3,5 %) |
| x4 flow 0.5 quality | 1,080 / 1,090 | 1,053 / 1,051 | +0,033 ms (+3,1 %) |
| x2 flow 1.0 performance | 0,601 / 0,602 | 0,591 / 0,593 | +0,009 ms (+1,6 %) |
| x3 flow 1.0 performance | 0,941 / 0,938 | 0,918 / 0,917 | +0,022 ms (+2,4 %) |
| x4 flow 1.0 performance | 1,257 / 1,269 | 1,243 / 1,248 | +0,017 ms (+1,4 %) |
| x2 flow 0.5 performance | 0,435 / 0,422 | 0,410 / 0,407 | +0,020 ms (+4,9 %) |
| x3 flow 0.5 performance | 0,661 / 0,661 | 0,639 / 0,641 | +0,021 ms (+3,3 %) |
| x4 flow 0.5 performance | 0,889 / 0,899 | 0,887 / 0,877 | +0,012 ms (+1,4 %) |

- **La diferencia va de −3,6 % a +4,9 %**: dentro del 5 % del criterio de la ronda 2. El 17 % de
  la ronda 2 no se repite.
- Lo que queda es **un costo fijo de unos 0,02 ms por ciclo**, igual en los casos cortos y en los
  largos. No es tiempo de GPU: lsfg corre en su propio dispositivo y hace el mismo trabajo en las
  dos versiones.
- Ese costo encaja con la sincronización de `LsfgRunner::step`, que en Rust:
  - espera la fence de la copia (`copy.wait`), cosa que la 1.x no hace;
  - abre un `recorder` para el envío vacío.

  Del otro lado, Rust espera todos los semáforos en un solo envío, y la 1.x hace un envío y una
  fence por semáforo.
- No lo cambiaría: son 20 µs en un ciclo que dura entre 16 y 33 ms. Se cierra.

### Hallazgo 4: la latencia media de FG integrado

- **Montaje.** FG integrado ×2 sobre mpv forzado a X11
  (`av://lavfi:testsrc2` a 60 fps, ventana de 2400×1350 por la escala de KWin), salida de
  2560×1440 en `always-on-top` con FIFO a 144 Hz, la misma configuración en las dos versiones y
  `latency_log`.
- **Muestras.** Cinco corridas de 25 s de cada una, alternadas, sin los primeros 5 s. Los valores
  son media / p95, en ms.

| Corrida | Rust real | Rust generado | Rust presents/s | 1.3.0 real | 1.3.0 generado | 1.3.0 presents/s |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 32,3 / 33,2 | 23,9 / 24,8 | 120 | 42,6 / 43,6 | 31,7 / 33,3 | 91 |
| 2 | 33,1 / 33,2 | 24,8 / 24,9 | 120 | 32,5 / 35,8 | 24,5 / 26,9 | 116 |
| 3 | 35,4 / 37,8 | 26,5 / 28,3 | 112 | 35,2 / 36,2 | 26,2 / 27,0 | 110 |
| 4 | 31,9 / 33,5 | 23,5 / 25,0 | 120 | 34,6 / 35,7 | 25,8 / 26,7 | 111 |
| 5 | 33,4 / 33,8 | 24,9 / 25,3 | 119 | 35,5 / 36,6 | 26,4 / 27,3 | 109 |

La corrida 3 de Rust coincidió con una baja del video, que dio 56 fps de captura.

- **Rust queda igual o mejor.** La media real típica es de 33 ms contra 35 ms de la 1.3.0, el p95
  es más bajo, y Rust sostiene los 120 presents/s mientras la 1.3.0 pierde cuadros (de 91 a 116).
  Los +2,5 ms de la ronda 2 no aparecen.
- **Lo que sí se repite** es lo que la ronda 2 vio en la forma:
  - en Rust, el encadenado se queda fijo en una fase, con una desviación de 0,12 a 0,25 ms en las
    corridas 2 y 5, y la media cae casi en el p95;
  - en la 1.3.0, la fase se mueve (desviación de 1,3 ms).

  Es el mismo `FramePacer` que la 1.x (`CHAIN_MARGIN`, encadenar al real anterior). En Python la
  fase se mueve porque el bucle tiene más ruido. Una fase quieta no suma latencia: el p95 es igual
  o mejor.
- Los valores absolutos son más altos que en la ronda 2 (29,6 ms) porque la ventana de captura es
  más grande. Las dos versiones se midieron con la misma.
- Se cierra sin cambios.

## Pendiente

- **Lo que necesita el puntero real va al guion de abajo**, porque `xdotool` no llega a XWayland.
  Falta probar:
  - que la banda muestre la barra y que se vaya a los 2 s;
  - los clics;
  - el arrastre con KWin;
  - la región de entrada en `top-transparent`.

  Hay una señal indirecta: en una corrida en la que `toolbar_pinned` no había quedado escrito, la
  barra apareció sola a los 8 s y quedó visible. Parece que el puntero real pasó por el borde.
- **El panel no se arrastra.** El diseño dice «se mueve con arrastre» y queda fijo en (20, 96).
  Para hacerlo hace falta otra zona en el router, o que `ToolbarZone` lleve una lista de
  rectángulos.
- **El OSD no lleva ícono.** El diseño muestra `neurology` junto a «Modelo: …». `Osd::show` recibe
  solo texto; un ícono por mensaje pide cambiar las llamadas de `pipeline.rs`.
- **La pasada de rasterizado de la CPU no está optimizada.** Tarda 1,1 ms por redibujo de la barra
  en release, y la mayor parte son los rectángulos con suavizado píxel a píxel. Si molesta, se
  puede calcular el suavizado solo en las esquinas.
- **No guardé una imagen de referencia en el repositorio.** El test con GPU escribe
  `toolbar-4k.png` en `$LIRTUP_TEST_OUT` o en la carpeta temporal y comprueba píxeles: el juego
  intacto fuera de la barra, el color del cristal y el `primary` de «fijar».
- **`TextRenderer`** (en `osd.rs` de efectos) ya no lo usa nadie. Lo dejé porque es API pública.

## Lo que pido

| Para | Qué |
| --- | --- |
| Revisor | `lirtup-pipeline` no depende de `libc`, así que `strftime`, `localtime_r` y `gmtime_r` están declarados a mano (`mod c` en `pipeline.rs`, con `struct tm` de glibc y musl). Si se agrega `libc` al `Cargo.toml` del crate, se cambian por los de `libc` |
| Revisor | `toolbar_pinned` y `windowed_overlay_position` se leen de `Config::extra`. Si se prefieren campos de `Config`, con su fila en Ajustes, se pasan sin cambiar el YAML |
| Revisor (GUI) | `save_setting` muestra un aviso por cada `SaveSetting`. Para «fijar» y la posición de la ventana sobra: conviene un guardado silencioso para esas dos claves |
| Revisor | «A ventana» manda un segundo `SessionStarted` con `kind` nuevo. Hay que confirmar que la GUI y el motor lo toman como una actualización |
| M | Fila 29: la posición de la superposición en QSettings de la 1.x puede ir a `windowed_overlay_position: [x, y]` en el nivel de arriba de `config.yaml` |
| P | El tooltip del diseño dice «Ocultar (Alt+Shift+S)», que es `ToggleScaling`. Seguí el esqueleto: el botón oculta la barra, no pausa el escalado. Para `docs/user/`: la barra, la banda del borde, fijar, el arrastre en modo ventana y Alt+Shift+D |
| Revisor | `overlay_blend.hlsl` dice «premultiplied» en la cabecera, pero mezcla alfa directo. Hay que corregir el comentario cuando los shaders pasen a los crates |

No hace falta tocar `framegen.rs` (ver los hallazgos).

## Guion de pruebas a mano

Antes: `cargo build --release -p lirtup-cli`. Una ventana de prueba, por ejemplo
`glxgears -geometry 960x540`, o mpv forzado a X11:
`env -u WAYLAND_DISPLAY mpv --gpu-context=x11egl --title=prueba av://lavfi:testsrc2=size=960x540:rate=60`.

1. **Aparecer y ocultarse.** `lirtup run -t glxgears`.
   - Llevar el puntero al borde de arriba: la barra baja con un pequeño rebote.
   - Sacar el puntero: a los 2 s se va con un fundido.
   - Con el puntero encima, no se va.
2. **Botones.**
   - Fijar: queda azul, con el ícono relleno, y no se va.
   - Stats: muestra y oculta el panel.
   - Captura: el OSD «Captura guardada» aparece después de guardar.
   - Recorte: avisa por el OSD.
   - Ocultar: se va y vuelve al tocar el borde.
   - Detener: termina la sesión.
   - Con la GUI abierta, «fijar» se guarda en Predeterminado.
3. **Multiplicador**, con `frame_gen_engine: native`.
   - Recorre ×2 → ×3 → ×4 → Auto → No, con el OSD.
   - La lectura cambia a «60 → 120 fps».
   - Con la GUI abierta, el valor queda en el YAML.
4. **Modo ventana.** `lirtup ctl 'start title=glxgears windowed'`, con `serve`.
   - Arrastrar la barra por la lectura de fps mueve la ventana.
   - Cerrar y volver a empezar en modo ventana: aparece donde quedó.
   - «A pantalla completa» y vuelta.
5. **`top-transparent`** (`--set overlay_mode=top-transparent`).
   - La barra aparece al tocar el borde y se puede hacer clic.
   - Con el puntero fuera del objetivo, la superposición baja a `overlay_opacity_min`.
6. **Hover (fila 10).** Un juego con resaltado al pasar el puntero (un menú) mantiene el resaltado
   con el puntero quieto.
7. **Minimizar (fila 5).** Minimizar el objetivo: la superposición se oculta y la sesión se pausa.
   Al restaurarlo, vuelve.
8. **`hide_cursor: 0`.** El cursor no aparece sobre el juego, pero sí sobre la barra.
