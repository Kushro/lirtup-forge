# Ronda 1, frente X: `lirtup-x11`

Rama `claude/rust-r1-x11-aaac12` (sale de `claude/rust-r0-contract-6aa817`). Fecha: 2026-11-01.

`cargo check -p lirtup-x11` (rustc 1.99.1): **pasa, sin avisos**. También
`cargo check -p lirtup-x11 --all-targets` (tests y ejemplo), sin avisos. No se corrió
`cargo build`, `cargo test`, `clippy` ni el ejemplo: **nada de esto se probó contra un servidor
X**. Se pasó `cargo fmt -p lirtup-x11`.

## Qué quedó

Todos los cuerpos del contrato, sin `todo!()`, y sin el `#![allow(unused_variables)]` de la
ronda 0. El crate tiene ahora unas 4 100 líneas (con las firmas y docs de la ronda 0), con tests
unitarios de la lógica pura.

| Módulo | Qué hace | Referencia de 1.x |
| --- | --- | --- |
| `conn` | `X11::connect` sobre `RustConnection`, átomos con `atom_manager!`, `is_xwayland` (extensión `XWAYLAND` o salidas RandR `XWAYLAND*`) | `window/connection.py` |
| `window` | `list_windows` (`_NET_CLIENT_LIST_STACKING` → `_NET_CLIENT_LIST` → árbol), `window_info` (un viaje de ida y vuelta: todas las peticiones primero), `active_window`, `window_icon`, `activate_window`, `is_viewable`, el filtro `is_application_window` | `window/info.py`, `acquisition.py` |
| `acquire` | `find_by_title` (regex sin distinguir mayúsculas, gana a `needle`), `wait_for_pid` (las dos fases), `active_after` | `window/acquisition.py` |
| `monitor` | `monitors` (RandR 1.5 `GetMonitors`, Hz de `dot_clock / (htotal·vtotal)`), `monitor_area` | `config/listing.py` (Qt) |
| `pick` | `pick_window`: cursor en cruz de la fuente `cursor`, clic izquierdo, ventana cliente con `WM_STATE` dentro del marco | `PickWindow` |
| `thumbs` | `enable_thumbnails` (`RedirectSubwindows` automático), `thumbnail` (pixmap del marco con `NameWindowPixmap`, si no `GetImage` de la ventana; caja promedio en CPU) | 6.4 |
| `capture` | `Capture` con `memfd` + `ShmAttachFd` + `ShmGetImage`, XDamage + XFixes, hashes xxh64 por tile | `capture/lib/*.c` |
| `tiles` | `TileCache`, `expand_rects` (con fusión de solapes), `dirty_fraction` (área de la unión, exacta) | `tile_cache.c`, `pipeline/utils.py` |
| `hotkeys` | `Hotkeys` con `GrabKey` en hilo y conexión propios, las 8 combinaciones de bloqueo, `rebind`, `keysym_for` (la tabla de `utils/hotkeys.py`) | `window/hotkeys.py` |
| `overlay` | `OverlayWindow` sobre `XCBConnection`: visual ARGB de 32 bits, modos, opacidad, región de entrada XFixes, cursor oculto, eventos | `overlay/overlay.py` |
| `input` | `CoordinateMapper::to_target`, `InputForwarder` con `SendEvent` (Enter/Leave, movimiento, botones, rueda, teclas) | `overlay/events.py`, `coordinates.py` |
| `cursor` (privado) | cursor de la fuente `cursor` y cursor vacío de 1×1, sin `libxcursor` | — |

Ejemplo: `crates/lirtup-x11/examples/capture_png.rs` (abajo).

### Cómo funciona la captura

1. `GetGeometry`: si la ventana no está → `NoWindow`; si cambió de tamaño → `Resized` (con el
   tamaño **de la ventana**, no el recortado).
2. Con seguimiento de daño: `DamageSubtract` a una región y `FetchRegion` de sus límites. Sin
   daño → cuadro limpio (`dirty = false`) y `out` no se toca.
3. Los límites se agrandan a la grilla de tiles, así cada tile tocado se lee entero, y se leen
   con `ShmGetImage` (o `GetImage` si no hay MIT-SHM 1.2).
4. Se copia lo leído a `out`, se calcula el xxh64 de cada tile y `TileCache::changed_tiles` deja
   solo los que cambiaron: esos son los `DamageRect` (con su hash). Si cambió más del 30 % de los
   tiles (`FULL_FRAME_TILE_FRACTION`, el `CAPTURE_TILE_THRESHOLD` de 1.x), el cuadro sale como
   `full` con un solo rectángulo de todo el cuadro.

Arregla de paso un detalle del C: el C solo comparaba los tiles que caían *enteros* dentro del
rectángulo dañado, así que un cambio que tocaba un tile solo en parte se perdía. Acá eso no pasa,
porque la región se alinea a la grilla.

## Cambios y aclaraciones del contrato

**Las firmas públicas no cambiaron.** Lo agregado:

- `tiles::TileCache::{tile_size, grid, tile_rect}` (la captura las usa y al pipeline le sirven
  para ubicar los tiles).
- `capture::MIN_TILE_SIZE` (16) y `capture::FULL_FRAME_TILE_FRACTION` (0,30).
- `impl From<…> for X11Error` para los errores de x11rb, y `Drop` en `Capture` (libera el daño, la
  región y el segmento), `Hotkeys` (suelta los atajos y espera al hilo) y `OverlayWindow`
  (destruye la ventana).
- `overlay::PointerKind` ya era público; no se reexporta en la raíz (igual que en la ronda 0).

Comportamientos que el contrato no decía y conviene que sepan K (ronda 2) y la GUI:

- **Con seguimiento de daño, `grab` escribe solo lo que cambió**: hay que pasar siempre el mismo
  buffer (tiene que tener el cuadro anterior). Es lo que pasa con un `Buffer` de subida mapeado.
  Si el pipeline alterna buffers, tiene que usar `damage_tracking: false`.
- **Ventana sin mapear o fuera de pantalla sin compositor** (`BadMatch` en `GetImage`):
  `grab` no falla. Devuelve un cuadro limpio y la próxima lectura es completa.
- La primera captura y las que siguen a un `BadMatch` son `full`. Sin seguimiento de daño, todas
  lo son (y no se calculan hashes).
- El umbral de 30 % es fijo (en 1.x venía de la variable de entorno `CAPTURE_TILE_THRESHOLD`).
  El pipeline igual decide con `dirty_fraction` contra `area_threshold`.
- `tile_size` menor que 16 se sube a 16, como en 1.x.
- `TileCache::changed_tiles`: un rectángulo que es exactamente un tile y trae hash se compara con
  el hash guardado; cualquier otro rectángulo marca como cambiados todos los tiles que toca y
  borra sus hashes. La salida va por filas y sin repetidos.
- `expand_rects` fusiona los rectángulos que se solapan (por su caja envolvente, hasta que no
  quede ningún solape). 1.x no fusionaba. El hash queda en 0.
- `wait_for_pid`: con `total_timeout <= 0` no hay límite (el `None` de 1.x). Los PID
  descendientes se leen de nuevo en cada vuelta desde `/proc/<pid>/task/*/children` (1.x leía
  `/proc/<pid>/children`, que no existe, así que en la práctica solo miraba el PID mismo).
  `starting_phase = 2` sin pista de clase arranca en la fase 1.
- `find_by_title` con una regex inválida da `X11Error::Request`.
- `monitors()`: el primario primero, después de izquierda a derecha; `index` es la posición en
  esa lista (como `QGuiApplication.screens()` de 1.x). Sin RandR 1.5, un solo monitor
  `default` del tamaño de la raíz.
- `monitor_area`: `all` da la caja envolvente y **el menor** de los Hz (la generación de cuadros
  no debe pasar de ninguna pantalla). Un nombre o índice que no existe cae en el primario con un
  `warn!`. `Name("1")` también vale como índice.
- `list_windows` sale en orden de apilado (de abajo hacia arriba). Además del filtro de 1.x
  descarta las ventanas override-redirect y las que tienen la propiedad `_LIRTUP_OVERLAY`, que la
  superposición se pone a sí misma.
- `activate_window` manda `_NET_ACTIVE_WINDOW` con origen «pager» (2), que los gestores respetan
  sin la prevención de robo de foco. Sin soporte EWMH, hace `ConfigureWindow(Above)` +
  `SetInputFocus`. 1.x llamaba a `xdotool`; acá no se lanza nada.
- `window_icon(xid, size)`: elige como 1.x (el más chico que llegue a `size`; si no hay, el más
  grande de los menores) y lo escala para que entre en `size × size` sin deformarlo.
- `thumbnail`: si antes se llamó a `enable_thumbnails`, lee del pixmap del marco del gestor de
  ventanas, así sale bien aunque la ventana esté tapada. Si falla, usa `GetImage` de la ventana.
  Solo ventanas de 24 o 32 bits; el alfa sale 255.
- `Hotkeys`: los modificadores tienen que coincidir exactos (sin contar Lock, NumLock ni
  ScrollLock); 1.x aceptaba modificadores de más. Si un mismo tecla+modificadores está en dos
  acciones, la segunda da error. Si la combinación ya la tiene otro programa (`BadAccess`), no
  queda nada agarrado a medias y vuelve `X11Error::Hotkey(…, "already grabbed …")`. Las letras de
  `keysym_for` no distinguen mayúsculas. Se agarran **todos** los keycodes que producen el keysym
  (1.x agarraba el primero). El texto del atajo en los errores se arma acá, sin `Display` de
  `Hotkey` (está en `todo!()` del frente C).
- `OverlayWindow`:
  - `always-on-top` y `top-transparent` son override-redirect (el
    `X11BypassWindowManagerHint` de 1.x); `fullscreen` pide `_NET_WM_STATE_FULLSCREEN` +
    `ABOVE`; `windowed` es una ventana normal de tamaño fijo, con decoraciones.
  - `WM_TRANSIENT_FOR` = destino en todos los modos menos `windowed`.
  - `top-transparent` implica `click_through`.
  - `set_mode` desmapea, cambia atributos y propiedades y vuelve a mapear: el swapchain se
    recrea con el `Configure`.
  - `Configure` se emite solo si el rectángulo cambió, en coordenadas de la raíz.
  - `Expose` solo con `count == 0`.
  - No guarda el puntero `xcb_connection_t*`: lo saca de `XCBConnection` en cada llamada, así que
    no hace falta un `unsafe impl Send` (hay un chequeo en tiempo de compilación de que
    `X11`, `Capture`, `Hotkeys`, `InputForwarder` y `OverlayWindow` son `Send`).
- `InputForwarder`: solo `SendEvent`, como 1.x. **XTest queda pendiente**: el doc del tipo lo
  menciona para los programas que ignoran eventos sintéticos, pero mueve el puntero real y hay
  que decidir cuándo usarlo (ronda 2). Fuera del contenido (las franjas negras) manda
  `LeaveNotify`. Las teclas van con la última posición del puntero. Usa el `time` del evento
  original (1.x mandaba `CurrentTime`), así funciona el doble clic. El origen del destino se lee
  en cada entrada y después de `set_mapper`.

## Dependencias

**No pido ninguna nueva.** Uso `x11rb` (con `composite`, `damage`, `randr`, `shape`, `shm`,
`xfixes`; `render` y `xtest` quedan sin usar), `xxhash-rust`, `libc` (`memfd_create`, `poll`),
`memmap2`, `crossbeam-channel`, `regex`, `serde`, `thiserror`, `tracing` y `lirtup-config`, todas
ya en el `Cargo.toml` del crate. El ejemplo usa `image` (ya está en `dev-dependencies`). No hizo
falta la feature `cursor` de x11rb.

## Cómo probar el ejemplo

En una sesión X11 o XWayland (el revisor, que es el único que puede compilar):

```bash
cargo run -p lirtup-x11 --example capture_png -- --list
```

```bash
cargo run -p lirtup-x11 --example capture_png -- "Firefox" -o /tmp/ff.png --frames 20
```

```bash
cargo run -p lirtup-x11 --example capture_png -- --pick -o /tmp/pick.png
```

- `--list`: monitores (con Hz) y ventanas (XID, geometría, PID, clase, título).
- Sin `WINDOW` toma la ventana activa a los 3 s. `WINDOW` puede ser parte del título o un XID
  (`0x3a00007`). `--pick` pide un clic.
- `--frames N` hace N capturas cada 100 ms e imprime tiempo, `dirty`, `full`, cantidad de
  rectángulos y el % del cuadro. Lo esperable es: el cuadro 1 `full`; si la ventana está quieta,
  `dirty false`; si algo se mueve, pocos rectángulos.
- Guarda el último cuadro en `-o` (por defecto `capture.png`) y el ícono de 64 px al lado
  (`*.icon.png`).

Tests unitarios (sin X): `cargo test -p lirtup-x11` cubre `tiles`, hashes y alineación de la
captura, recorte, `WM_CLASS`, elección y escala de íconos, reescalado RGBA, `keysym_for`,
combinaciones de bloqueo y coincidencia de atajos, selección de monitores y Hz,
`CoordinateMapper`, `WM_SIZE_HINTS` y PID descendientes. Ninguno necesita `$DISPLAY`.

## Pendiente y riesgos

- **Nada se probó contra un servidor.** Lo que más puede fallar:
  - `ShmAttachFd` con un `memfd` (lo usan mutter y otros; MIT-SHM 1.2) en XWayland.
  - `ShmGetImage` de la ventana cliente dentro de un marco reparentado con compositor.
  - El aviso `NON_EMPTY` de XDamage.
  - `NameWindowPixmap` sobre el marco + el desplazamiento del borde en `thumbnail`.
  - El modo `fullscreen` en KWin, GNOME y otros (estado antes de mapear).
  - El cursor vacío.
- Si `ShmGetImage` resulta lento en ventanas grandes, el siguiente paso es leer cada rectángulo
  dañado por separado en vez de la caja envolvente (hoy, como el C).
- `thumbnail` copia la ventana entera por el socket (unos 8 MB por ventana 1080p). Si en la GUI
  pesa, se puede achicar del lado del servidor con Render (`SetPictureTransform`) o usar MIT-SHM.
- XTest en `InputForwarder` (arriba).
- `_NET_WM_FULLSCREEN_MONITORS` para `monitor: all` en pantalla completa (hoy el gestor de
  ventanas elige un monitor; los override-redirect cubren el rectángulo pedido).
- La superposición no maneja `FocusIn`/`FocusOut` (no están en `OverlayEvent`);
  `pause_on_focus_loss` mira el foco con `X11::active_window`.
- `pick_window` agarra el teclado solo para Escape; si otro programa lo tiene agarrado, Escape no
  cancela (el clic derecho sí).
