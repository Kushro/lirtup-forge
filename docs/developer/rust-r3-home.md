# Ronda 3, frente B: Inicio y ventanas

Rama **`claude/rust-r3-home-953724`**, que sale de `rust/r3-skeleton` (`01cd43d5`). Fecha:
2026-11-01. Sin push y sin merge.

- `cargo check -p lirtup-ui --all-targets`: **pasa**.
- `cargo clippy -p lirtup-ui --all-targets`: **sin avisos en los archivos de B**. Quedan solo los
  `todo!()` de A y C.
- `cargo test -p lirtup-ui`: **20 tests en verde**, 16 de ellos de B. Los dos que van contra X11
  real (`windows::tests::real_list_has_no_own_windows` y `thumbs::tests::real_round`) los corrí en
  la sesión XWayland de KDE. Con `glxgears` abierto, la vuelta dio 1 ventana, 1 vista previa
  JPEG de 320 px como máximo y 1 ícono. Sin `DISPLAY`, o sin ventanas, no comprueban nada y pasan.
- Target en `/home` (`CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r3-b`).
- **No vi la pantalla.** Los componentes de A y el `update` de C siguen en `todo!()`, así que la
  app entra en pánico apenas dibuja. No hice el merge local de A y C porque sus ramas todavía no
  estaban listas. Lo visual y los flujos de 5.8 quedan para después de integrar (ver el guion al
  final).

## Commits

| Commit | Qué |
| --- | --- |
| `5cbdf934` | `windows.rs`: la lista de ventanas en su propio hilo |
| `627a9611` | `thumbs.rs`: las miniaturas y los íconos |
| `ba26f83e` | `screens/home/**` y `home.ftl` |
| `69baf9cf` | `screens/about.rs` y `about.ftl` |

No toqué `Cargo.toml`, `package.json`, los archivos de bloqueo ni `assets/tailwind.css`.

## Qué hice

### `windows.rs`

- Un hilo `lirtup-windows` con su propia conexión de `lirtup-x11` llama a `list_windows` cada
  `gui.auto_refresh_ms`, acotado entre 200 y 60 000 ms. Lee el valor de un `AtomicU32` que un
  `use_effect` actualiza desde el store. El hilo manda `Action::Windows` **solo si la lista
  cambió**: pasa por un canal de tokio a una tarea de Dioxus, que llama a `dispatch`.
- Filtro: `list_windows` ya deja afuera las ventanas con `_LIRTUP_OVERLAY`. Además,
  `is_own()` descarta las de este proceso (por pid) y las de clase o instancia `lirtup` (la de la
  superposición es `lirtup`/`LirtUp`) o que empiezan con `lirtup-` (la GUI toma su clase del
  binario, `lirtup-gui`). Al integrar, el revisor puede reutilizar `windows::is_own` para el
  pendiente de `StartFullscreen`, o copiar la regla al núcleo.
- Si falla una consulta, abre otra conexión; si no puede conectar, reintenta cada 5 s. El hilo termina cuando se cierra el canal, es
  decir, cuando se cierra la app.

### `thumbs.rs`

- Un hilo `lirtup-thumbs` con su propia conexión: `enable_thumbnails` y una vuelta cada
  `gui.tile_preview_interval_ms` (entre 100 y 60 000 ms) sobre las ventanas del store.
- Cada vuelta llama a `X11::thumbnail(xid, 320)` y calcula un hash de los píxeles. **Solo
  codifica en JPEG (calidad 80) y sube la versión si el hash cambió.** Una ventana quieta no
  cambia de URL, así que el webview no vuelve a pedir la imagen. Elegí JPEG y no WebP porque el
  codificador WebP de `image` 0.25 solo hace WebP sin pérdida, que sale mucho más pesado.
- Ícono: `window_icon(xid, 64)` en PNG, una sola vez por ventana. Se borra cuando la ventana
  desaparece.
- `use_asset_handler("thumb")` sirve `/thumb/{xid}?v={n}` desde la caché, sin bloquear.
  `use_asset_handler("icon")` sirve `/icon/{xid}` desde la caché. Si el ícono todavía no está,
  manda el `RequestAsyncResponder` al hilo, que lo lee y responde. Si no hay ícono, responde 404.
- **`Thumbs` sumó un campo y un método:** `icons: Signal<HashSet<u32>>` y
  `Thumbs::icon(xid) -> Option<String>`, que devuelve la URL solo si la ventana tiene ícono.
  Así, a `WindowTile` le llega `None` y no un `<img>` roto. `url()` e `icon_url()` no cambiaron.
- Pausa: un `AtomicBool`. Lo actualizan `use_wry_event_handler` (con `Focused`, `Resized` y
  `Moved`) y una revisión cada 1 s de `is_visible() && !is_minimized()`, porque minimizar no
  siempre genera un evento. La misma revisión manda `Action::WindowVisible(shown)` cuando el
  store dice otra cosa, y eso cubre el arranque, donde `Store::default()` tiene
  `visible: false`. Al volver a verse, captura enseguida.

### Inicio (`screens/home/`)

- `mod.rs`: la página (`max-w-[1040px]`, `px-10 pt-8 pb-12`, `gap-7`, como el lienzo), más
  `start()`, que manda `StartScaling` y lo guarda en `LAST_START` (un `GlobalSignal` del módulo)
  para Reintentar. También `copy_to_clipboard()`, que usa `navigator.clipboard` y, si no está, un
  `textarea` con `execCommand`. Acerca de también la usa.
- `session.rs` (`SessionCard`):
  - `Idle`: no muestra nada.
  - `Starting`: tarjeta ámbar con `LoadingIndicator` y Detener.
  - `Running`: «Escalando …», la línea mono (modo con su nombre de `modes/`, `src → dst` y
    `capture → present fps (×N)`), el aviso de presupuesto de GPU si llega `gpu_warning`,
    Reiniciar y Detener.
  - `Failed`: `error-container` con la etapa traducida, el mensaje y Reintentar. Reintentar
    aparece si `retry` es verdadero y hay un arranque anterior en esta ejecución de la GUI.
- `activation.rs` (`ActivationGroup`):
  - los dos atajos, leídos de `config.yaml` → `hotkeys.start_fullscreen` y `start_windowed`, o
    de `HotkeyAction::default_binding()`. Con `null` o vacío se muestra «Sin atajo». El lápiz
    lleva a Ajustes ▸ Atajos;
  - escalado diferido: dos «Empezar» (pantalla completa y en ventana) que mandan
    `Start { target: Focused, delay_s }`, y un menú con 3, 5, 10 y 15 s;
  - interruptor del daemon: refleja `store.daemon.on` y manda `SetDaemon { on }`. Encendido, el
    subtítulo dice «Vigilando N perfiles».
- `grid.rs` (`WindowsSection`):
  - «Ventanas abiertas · N», con N contando todas las ventanas;
  - `Connected<QuickFilter>` (Todas, Con perfil, Sin perfil) y búsqueda por título o
    `WM_CLASS`, sin distinguir mayúsculas;
  - la cuadrícula: con `auto-fill` y `minmax(240px, 1fr)`, o con `gui.tile_columns` columnas
    fijas. El aspecto sale de `gui.tile_aspect`, y en Auto es el de la ventana;
  - perfil del mosaico: `matching_profile`. Solo un `Named` cuenta como «con perfil».
  - estado del mosaico: `Running` si es la ventana de la sesión (también mientras arranca, con
    `Starting`) y con sus fps, `Focus` si tiene el foco del teclado, si no `Idle`;
  - clic o Intro: `Start { Window(xid), Fullscreen, profile: el que coincide }`;
  - menú (`on_menu`): Escalar en ventana, Crear perfil desde esta ventana
    (`OpenDialog(NewProfile { from: Some(window) })`) y Copiar título;
  - teclado: las flechas, Inicio y Fin mueven el foco real del DOM con `document::eval`. Para
    arriba y abajo, las columnas se calculan con el ancho del `onresize`. Hacia abajo, desde la
    penúltima fila incompleta, el foco va al último mosaico. Intro y la tecla Menú son de
    `WindowTile`;
  - Ctrl+F: el campo se vuelve a montar con `key` igual a `FOCUS_FILTER` y `autofocus`. Un
    `GlobalSignal` (`FOCUS_DONE`) recuerda qué Ctrl+F ya se atendió, así que el foco llega
    también cuando Ctrl+F trae a la persona desde otra página;
  - estados vacíos: no hay ventanas, o ninguna coincide con el filtro.

### Acerca de (`screens/about.rs`)

- Ícono, nombre, la versión de la GUI (`CARGO_PKG_VERSION`) y la descripción de 1.x.
- Enlaces a Código fuente, Proyecto original y Documentación. Se abren con `xdg-open`, con las
  mismas URL que `identity.py`.
- Licencias de terceros, plegables: LirtUp Forge (GPL-3.0-or-later), CuNNy (LGPL-3.0), Magpie
  (GPL-3.0), lsfg-vk, pe-parse y volk (MIT), Dioxus (MIT/Apache-2.0), Google Sans Flex y
  JetBrains Mono (OFL-1.1) y Material Symbols (Apache-2.0). Revisé las licencias de lsfg-vk,
  pe-parse, volk y Magpie en los `LICENSE` del repositorio. La de CuNNy la puse de memoria:
  **conviene confirmarla** antes de publicar.
- Diagnóstico: con el link conectado, manda `RequestDiagnostics` una sola vez por montaje. Tiene
  Actualizar y Copiar. El texto (`diagnostics_text`) va **en inglés a propósito**, porque es para
  pegar en un informe de error: versiones de la GUI y del escalador, pantalla, Vulkan, cada GPU,
  la generación de cuadros con su código de motivo, `Lossless.dll`, dxc, glslang y la carpeta de
  configuración.

### `home.ftl` y `about.ftl`

Las claves son las mismas en español y en inglés. El test
`screens::home::tests::home_and_about_texts_in_both_languages` compara los dos conjuntos y
resuelve cada clave con los argumentos que usan las pantallas. Los plurales («Vigilando 1 perfil»
y «N perfiles») están hechos con selectores de Fluent.

## Dependencias que pido

**Ninguna.** Uso `image` (JPEG y PNG, ya con esas features), `tokio`, `serde_json` y, en los
tests, `serde_yaml_ng`, que ya están en el `Cargo.toml` de la GUI.

## Lo que necesito de A (componentes)

Programé contra las firmas, así que esto es lo que espero de cada componente:

- **`WindowTile`**: que Intro y Espacio llamen a `on_activate`, y que la tecla Menú y
  Shift+F10 llamen a `on_menu` con coordenadas de página (por ejemplo, la esquina del mosaico).
  También que tenga un elemento enfocable (un `button`). `focus_tile` busca
  `button, a[href], [tabindex]:not([tabindex="-1"])` dentro de la celda `data-tile-index`. Si no
  encuentra ninguno, enfoca la celda, que tiene `tabindex="-1"`, y entonces Intro no haría nada.
- **`Menu`**: con `at: Some((x, y))`, que se posicione en coordenadas de página, y que Esc y el
  clic afuera pongan `open` en `false`. Con `at: None`, que quede debajo del hermano anterior. Así
  lo uso en el menú de demora: el `IconButton` y el `Menu` van dentro de un `div.relative`.
- **`TextField`**: que `autofocus` enfoque al montar. Lo vuelvo a montar con `key` para cada
  Ctrl+F.
- **`SettingRow`**: con `Origin::Inherited`, que muestre `subtitle` en lugar de «Hereda de
  Predeterminado». Las filas de Inicio y de Acerca de no son ajustes heredables, y todas pasan
  `Inherited` con subtítulo. Si A prefiere una variante sin origen (`Origin` opcional o una
  prop `plain`), lo cambio después de integrar.
- **`Connected` y `Switch`**: que escriban la `Signal` que reciben. Al interruptor del daemon le
  paso `on_change` y lo vuelvo a sincronizar con el estado del núcleo.
- **`Button` con `ButtonVariant::Tertiary`**: el lienzo pone Detener **lleno en `--tertiary`**
  sobre la tarjeta `tertiary-container`. Si `Tertiary` es el contenedor terciario, Detener se
  pierde en la tarjeta. Una de dos: que `Tertiary` sea lleno, o una variante más.
- `LoadingIndicator { size: 48 }`, `Icon { size: 32 }` (lo uso para el ícono de 30 a 40 px de la
  tarjeta y de Acerca de), `Badge`, `Keycap` e `IconButton { selected }`.

## Lo que necesito de C (estado)

`update` tiene que tratar estas acciones, que manda B:

| Acción | Qué espero |
| --- | --- |
| `Windows(list)` | `store.windows = list` |
| `WindowVisible(b)` | `store.visible = b`, y la regla de `WatchGpuTimes` |
| `StartScaling { .. }` | `Send(Start { target, kind, delay_s, profile })` |
| `StopScaling` / `RestartScaling` | `Send(Stop)` / `Send(Restart)`. Con `Failed`, conviene que `StopScaling` deje la tarjeta en `Idle`: hoy la tarjeta de error no tiene cómo cerrarse |
| `SetDaemon { on }` | `Send(SetDaemon { on, keep_after_exit: gui.keep_running_after_exit })` |
| `RequestDiagnostics` | `Send(Diagnostics)`; `Core(Diagnostics(r))` → `store.diagnostics = Some(r)` |
| `OpenDialog(NewProfile { from })` | `store.dialog = Some(..)` |
| `Core(SessionStarting / SessionStarted / SessionError / SessionStopped / Stats / DaemonState)` | `store.session`, `store.stats` y `store.daemon` |
| `Core(Snapshot)` | `session` y `daemon`. Si llega `last_error`, sugiero `SessionState::Failed { retry: false, .. }` |

Además, `Store::matching_profile` se llama una vez por mosaico en cada render de la cuadrícula.
Si compila regex en cada llamada, conviene una caché.

## Pendientes

- **Ver la pantalla y correr los flujos** después de integrar A y C (guion abajo).
- **La demora del escalado diferido no se guarda.** Empieza en 3 s en cada arranque porque no hay
  una clave en `gui-config.yaml`. Propongo `gui.start_delay_s` en `lirtup-config` (revisor), y
  que la GUI la escriba con `SetGui`.
- **El daemon encendido no colorea el ícono de la fila** (`primary-container` en el lienzo):
  `SettingRow` no lo permite. Si A agrega `lead_class` o algo así, lo uso.
- La tarjeta de error no tiene botón para cerrarse, salvo que C haga lo de `StopScaling` de la
  tabla.
- Copiar título y Copiar diagnóstico no avisan con un snackbar, porque no hay una `Action` para
  un `Toast` suelto. En Acerca de, el botón cambia a «Copiado». Si el revisor agrega
  `Action::Toast(Toast)`, aviso también en el menú del mosaico.
- **Hallazgo en `i18n.rs` (revisor): `has_key` da `false` con cualquier mensaje que tenga
  variables**, porque `lookup_single_language` sin argumentos falla. Por ejemplo,
  `home-session-starting` o el mismo `savebar-changes`. El test de la ronda 4 lo va a necesitar.
  Una salida es buscar el mensaje en el bundle sin formatearlo, o pasar argumentos de relleno. Mi
  test esquiva el problema con `tr_in` y argumentos.

## Guion a mano (criterios de 5.8 de B, después de integrar)

1. `lirtup serve` corriendo, o que lo lance el link, y una ventana X11 de prueba (`glxgears`,
   `vkcube` o un juego):
   - Inicio muestra el mosaico con su vista previa en vivo y su ícono;
   - la GUI y la superposición no aparecen en la lista;
   - al minimizar la GUI, `top -H` muestra `lirtup-thumbs` en reposo.
2. **Flujo 1**:
   - clic en el mosaico: la tarjeta pasa por «Iniciando…» y queda en «Escalando …», con la
     línea mono y los fps; el mosaico se ve ámbar con los fps;
   - Detener: la tarjeta desaparece;
   - Alt+Shift+A sobre la ventana: lo mismo;
   - «Empezar» con 3 s y volver a la ventana: lo mismo;
   - cerrar la ventana de prueba mientras escala: la tarjeta pasa a «Idle» o a error, según
     `StopReason`.
3. Error: escalar una ventana que se cierra durante el arranque. Tiene que aparecer la tarjeta
   roja con la etapa, el mensaje y Reintentar.
4. **Flujo 6 (parte de Inicio)**: encender el daemon. El interruptor queda encendido cuando
   llega `DaemonState`, y el subtítulo dice «Vigilando N perfiles». Apagarlo.
5. **Teclado**:
   - Tab hasta la cuadrícula, y las flechas, Inicio y Fin se mueven por los mosaicos;
   - Intro escala;
   - la tecla Menú abre el menú del mosaico;
   - desde Ajustes, Ctrl+F vuelve a Inicio con el foco en la búsqueda.
6. Acerca de:
   - el diagnóstico llega solo;
   - Copiar y pegar en un editor da el texto en inglés;
   - los enlaces se abren en el navegador.
7. Idioma: cambiar a inglés en Ajustes; Inicio y Acerca de cambian sin reiniciar.
