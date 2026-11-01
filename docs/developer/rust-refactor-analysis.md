# Reescritura en Rust con Dioxus: análisis y plan

Estado: **fase 1 del análisis** (inventario, referencias, arquitectura, etapas y plan de
ejecución). La fase 2, que mapea el diseño nuevo de la GUI a componentes Dioxus, espera el
artefacto de diseño de la sesión «Diseño: sistema de diseño y flujos de LirtUp Forge».
Fecha: 2026-11-01. Base: `main` en la 1.3.0 (`454d7962`).

Convención, la misma de [frame-generation-analysis.md](frame-generation-analysis.md):
**verificado** = leído en el código; **estimado** = cuenta hecha aquí, sin medir ni probar.

## Resumen

- **Se puede, y conviene hacerlo por etapas.** La app tiene unas 23 000 líneas de Python fuera
  de la GUI, 15 000 de GUI en PySide6 y unas 8 000 de C/C++ propio (Vulkan, captura, puente con
  lsfg). Casi todo el C/C++ propio está atado a la API de Python (`PyObject`, el GIL), así que
  no se puede reusar tal cual: se reescribe en Rust. Lo vendorizado (lsfg-vk, pe-parse, volk)
  se queda en C++ detrás de una API C nueva. Ojo: Enigmata Temple **no** usa FFI; corre lsfg como
  un proceso aparte y le pasa los cuadros por pipes. Para lirtup recomiendo FFI en el mismo
  proceso, con imágenes compartidas por fd como en la 1.3, porque evita copiar cada cuadro a la
  CPU (sección 3.5).
- **Vulkan con `ash`.** Es la única opción de las tres que da lo mismo que hoy sin pelear con la
  biblioteca: memoria y semáforos externos por fd para lsfg, timestamps dentro de la cadena de
  dispatches, `robustImageAccess`, SPIR-V de dxc sin traducir y control de la `VkQueue`. `wgpu`
  pierde casi todo eso; `vulkano` lo tiene, pero su seguimiento automático de sincronización
  estorba al compartir imágenes con otro `VkDevice`.
- **La superposición no puede ser una ventana de Dioxus.** Dioxus Desktop dibuja con WebKitGTK;
  la superposición necesita una ventana X11 propia, transparente y encima de todo, con su
  swapchain Vulkan. Se crea con `x11rb` en su propio hilo, igual que hoy el swapchain usa su
  propia conexión XCB.
- **Etapa recomendada:** primero el núcleo como binario de línea de comandos (`lirtup`, el
  equivalente a `upscale`), comparado lado a lado con la versión de Python; después la GUI
  Dioxus sobre ese mismo núcleo, en proceso. **No** recomiendo poner el núcleo de Rust detrás
  de la GUI de Qt con PyO3: la superposición, los atajos y el hilo del pipeline hoy dependen de
  Qt, y ese puente se tiraría al terminar.
- **Esfuerzo (estimado):** unas 4 rondas de trabajo en paralelo (4 subagentes y un revisor por
  ronda), del orden de 18 000 a 24 000 líneas de Rust. La parte riesgosa no es la GUI sino la
  paridad del pipeline: tiles, la cadena de efectos, el ritmo del motor integrado de generación
  de cuadros y el reenvío de entrada.

## 1. Inventario de la app actual (verificado)

### 1.1 Módulos de Python

Cuentas de `wc -l` sobre `src/upscaler`, sin los `.hlsl` de Magpie ni `blue_noise.py` (una
tabla de 16 000 líneas).

| Módulo | Líneas | Qué hace | Destino en Rust |
| --- | ---: | --- | --- |
| `capture/` (Python) | 125 | `FrameGrabber` por `ctypes` sobre la biblioteca C | `lirtup-x11::capture` |
| `vulkan/__init__.py` | 1 134 | Envoltorio de la extensión: `Device`, `Heap`, `Resource`, `Buffer`, `Texture2D`, `Sampler`, `Compute`, `Swapchain` | `lirtup-gpu` |
| `pipeline/` | 5 958 | `Pipeline` (hilo propio), `Presenter` (cadena de efectos), `TileProcessor`, `UpscalerManager`, `SwapchainManager`, `StatsHUD`, `OSDManager`, `NativeFrameGen` (hilo `lsfg-present`), `DaemonMonitor`, detector de barra de menú, `PipelineController` | `lirtup-pipeline` |
| `overlay/` | 1 357 | `OverlayWindow` (QMainWindow translúcida), `X11EventForwarder` (eventos sintéticos con `SendEvent`), `CoordinateMapper`, opacidad | `lirtup-x11::overlay`, `::input` |
| `window/` | 1 728 | Lista de ventanas, info (título, clase, PID, ícono), foco, `HotkeyManager` (`GrabKey` por xcffib), búsqueda por PID o título | `lirtup-x11` |
| `shaders/` | 2 793 (sin `blue_noise`) | Clases de los shaders nativos: escaladores (copy, Catmull-Rom, Lanczos, FSR, NIS), efectos (CAS, deband, bloom, viñeta, grano, LUT, LumaSharpen, Anime4K S, CRT Easymode), conversores (linealizar, deslinealizar, dither), mezcla del OSD | `lirtup-effects` |
| `srcnn/` | 1 405 | Carga de los modelos CuNNy (JSON + SPIR-V por pasada, variantes de tile) | `lirtup-effects::srcnn` |
| `magpiefx/` | 2 511 | Cargador del formato MagpieFX: parser de `//!`, expresiones, codegen, compilador (dxc o glslang por subproceso), `.dds`, swizzle, ejecución | `lirtup-magpiefx` |
| `framegen.py` | 400 | Capa lsfg-vk: búsqueda de la capa y la DLL (Steam), `lsfg-vk.toml` propio, estados | `lirtup-fg` |
| `lsfg/` (Python) | 7 | Carga de la extensión | `lirtup-fg` |
| `config/` | 3 261 | `Config` (112 campos), YAML, perfiles, validadores, argumentos de CLI (1 551 líneas), listado | `lirtup-config` |
| `api/` | 671 | `UpscalerSession` y búsqueda de ventanas, para uso como biblioteca | `lirtup-core` (API pública) |
| `utils/` | 784 | Geometría del escalado, color, atajos, pantallas | repartido |
| `bench.py` | 764 | «Medir en mi GPU» | `lirtup-pipeline::bench` |
| `cli.py`, `env.py`, `identity.py` | 124 | Entrada de `upscale`, variables de entorno | `lirtup-cli` |
| `gui/` | 15 257 | GUI PySide6 (ver 1.5) | `lirtup-ui` (Dioxus) |

### 1.2 Extensiones nativas

Se compilan desde `setup.py` como tres extensiones de CPython.

| Extensión | Fuentes | Líneas | Depende de | Destino |
| --- | --- | ---: | --- | --- |
| `upscaler.capture.capture` | `capture/lib/*.c`: XShm, XDamage, caché de tiles con xxHash64, sincronización | 1 174 | X11, X11-xcb, Xext, Xdamage, Xfixes, xcb | Portar a Rust (`x11rb` + `xxhash-rust`) |
| `upscaler.vulkan.vulkan` | `vulkan/lib/*.cpp`: instancia, dispositivo, heap, recursos, compute, sampler, swapchain, memoria externa | 6 385 | `libvulkan` | Portar a Rust (`ash`) |
| `upscaler.lsfg.lsfg` | `lsfg/lib/` (puente 270, `shader_loader` 381, `unicode_posix`, `volk_unit`) + `third_party/` (lsfg-vk 3.1 y 3.1P, pe-parse, volk) | ~650 propias, ~14 000 vendorizadas | `dl`, `pthread`; Vulkan por volk | **Se queda en C++**; se cambia el puente de Python por una API C |

Detalles del módulo `vulkan` que el port tiene que conservar (verificado en `vk_device.cpp`,
`vk_external.cpp`, `vk_instance.cpp`):

- Extensiones de instancia: `VK_KHR_surface`, `VK_KHR_xcb_surface` (y Wayland si está),
  `VK_EXT_debug_utils` opcional.
- Extensiones de dispositivo: `VK_KHR_swapchain`, `VK_KHR_external_memory_fd` y
  `VK_KHR_external_semaphore_fd` cuando están, `VK_EXT_image_robustness` o la
  característica de Vulkan 1.3 (`robustImageAccess`), `shaderFloat16`.
- Pool de 128 timestamps cuando `timestampComputeAndGraphics`, con `timestampPeriod`.
- `Texture2D.external`: asignación dedicada en el primer tipo `DEVICE_LOCAL`, exportable por fd
  opaco; semáforos binarios exportables; `copy_texture_signal` y `wait_semaphore`.
- Hoy el GIL serializa el uso de la `VkQueue` entre el hilo del pipeline y `lsfg-present`. En
  Rust eso pasa a ser un `Mutex` explícito alrededor de la cola.

### 1.3 Shaders y SPIR-V

| Conjunto | Fuente | Cuándo se compila | Archivos |
| --- | --- | --- | --- |
| Modelos CuNNy (9 variantes) | `.hlsl` + `.spv` por pasada y variante de tile, más un JSON por modelo | Antes, con `scripts/hlsl-to-spv.fish`; el `.spv` está en el repositorio | ~230 |
| Shaders nativos | `shaders/**.hlsl` + `.spv` (FSR y NIS con sus cabeceras) | Antes, igual | ~80 |
| Efectos de Magpie | 117 `.hlsl` sin modificar, `.hlsli`, `.dds` | En tiempo de ejecución con `dxc` (o `glslangValidator`), con caché | 153 |
| lsfg | Dentro de `Lossless.dll` (RT_RCDATA 304..400) | Los extrae `shader_loader.cpp` en memoria | — |

Los `.spv` precompilados se embeben en Rust con `include_bytes!` (o se instalan como datos); no
hace falta compilar HLSL en el build. MagpieFX sigue necesitando un compilador en tiempo de
ejecución (ver 3.4).

### 1.4 Dependencias del sistema

| Dependencia | Para qué | En Rust |
| --- | --- | --- |
| X11, XCB, Xext (XShm), Xdamage, Xfixes | Captura, ventanas, entrada, atajos | `x11rb` (puro Rust, habla el protocolo; no necesita Xlib). Solo hace falta `libxcb` para la superficie Vulkan |
| `libvulkan` | Todo el cómputo y la presentación | `ash` con carga dinámica (`ash::Entry::load`) |
| `dxc` / `glslangValidator` | MagpieFX | Subproceso, igual que hoy (ver 3.4) |
| Capa lsfg-vk 2.x y `lsfg-vk.dll` | Generación de cuadros por capa | Igual: variables de entorno y `lsfg-vk.toml` antes de crear la instancia |
| `Lossless.dll` | Motor integrado | Igual, por el `shader_loader` de C++ |
| Qt 6 / PySide6 | GUI, superposición, bandeja, traducciones | Desaparece: Dioxus Desktop (WebKitGTK 4.1, GTK 3), `x11rb` para la superposición, bandeja aparte |
| Pillow | Texto del OSD, capturas de pantalla, `.png` de MagpieFX | `ab_glyph` o `fontdue` para el texto, `image` para PNG/BMP/JPEG/TIFF |
| PyYAML | Configuración | `serde` + `serde_yaml_ng` (o `serde_norway`; `serde_yaml` está sin mantenimiento) |

### 1.5 GUI actual

`gui/` tiene 15 257 líneas. Piezas que el diseño nuevo tiene que cubrir (la fase 2 las mapea a
componentes):

- **Ventana principal** (`main.py`, 946): tres columnas (las capturas que acompañan esta tarea
  lo muestran): perfiles a la izquierda, cuadrícula de ventanas con miniaturas y filtro en el
  centro, ajustes a la derecha con barra de íconos y *Guardar* / *Restablecer*.
- **Perfiles** (`sidebars/profiles.py`, `helpers/profiles.py`, `dialogs/profile.py`): global más
  perfiles por título o tamaño, con ícono de la ventana; crear, duplicar, editar, borrar,
  reordenar.
- **Cuadrícula** (`grid/`): escena de Qt con miniaturas de cada ventana y filtro.
- **Pestañas de ajustes** (`sidebars/tabs/`): general, escalado, pantalla, presentación
  (generación de cuadros, recorte, barra de menú), efectos (motor Native o MagpieFX, controles
  generados de cada efecto, 820 líneas), estilo, avanzado, extras (stats, OSD), atajos.
- **Controles** (`sidebars/controls/`): casilla, combo, deslizador, color, fuente, atajo
  (captura de teclas), ruta, texto, sección.
- **Diálogos**: acerca de, medición en la GPU (559), confirmación, fuente, perfil, ventana.
- **Bandeja** (`helpers/tray.py`, 587): `QSystemTrayIcon` con reiniciar, cambiar de ventana o de
  perfil, multiplicador de generación de cuadros.
- **Daemon** (`helpers/daemon.py`): busca ventanas que coincidan con perfiles y arranca solo.
- **Reinicio** (`helpers/restart.py`): `os.execve` para que la capa lsfg-vk relea el
  multiplicador.
- **Paleta y estilos** (`config/palette.py`, `styles.py`, `utils/scheme.py`): tema claro/oscuro
  y colores propios, unas 2 000 líneas de QSS y paleta.

La GUI arranca el pipeline **en el mismo proceso** (`create_pipeline_session` en `main.py` y en
el daemon), no como subproceso.

### 1.6 Configuración y perfiles

- `~/.config/linux-rt-upscaler/config.yaml`; orden: valores por defecto, opciones de nivel
  superior, perfil (elegido con `-p` o por coincidencia de título), y la CLI encima.
- `Config` es una dataclass de 112 campos con validadores (`config/validators.py`) y enums
  (`OverlayMode`: `always-on-top`, `top-transparent`, `fullscreen`, `windowed`).
- Los perfiles guardan solo lo que difiere; los atajos se guardan como diferencia contra los
  predeterminados.
- La caché de MagpieFX y `lsfg-vk.toml` viven en la misma carpeta.

El port tiene que **leer el mismo archivo sin migración**: `serde` con `#[serde(default)]` por
campo, `deny_unknown_fields` desactivado y los mismos nombres. Un test de ida y vuelta sobre
`config-example.yaml` lo asegura.

### 1.7 Traducciones

`gui/locale/upscale_gui_{en,es}.ts` (526 mensajes en español) compilados a `.qm` con
`scripts/compile-translations.fish`. Las cadenas están en el código con `self.tr(...)`. El
contexto de Qt (la clase) es parte de la clave.

### 1.8 Tests

14 archivos, 2 616 líneas, unos 195 tests con pytest: configuración, MagpieFX (fuentes, `.dds`,
FP16), medición, generación de cuadros (multiplicador automático, motor integrado), stats,
barra de menú, reinicio, pestaña de presentación, traducciones. Casi todos prueban lógica pura,
así que se pueden reescribir como tests unitarios de Rust con los mismos casos (tabla en 4.5).

### 1.9 Empaquetado

Hoy: `pipx install git+…` compila las extensiones; `cibuildwheel` en GitHub Actions para
CPython 3.10 a 3.14; `data/applications/*.desktop` y los íconos en `data/icons/hicolor`.

## 2. Cómo lo resuelven Miko Kagura y Enigmata Temple

### 2.1 Miko Kagura (verificado por un subagente que leyó el repositorio)

Un solo crate (`miko-kagura` 0.2.0, edición 2021), sin workspace, sin FFI y sin bandeja; usa la
GPU solo a través de programas ncnn-vulkan externos. Lo que sirve:

- **Dioxus 0.7.9 Desktop** (wry 0.53, tao 0.34, webkit2gtk 2.0), toolchain fijo
  (`rust-toolchain.toml`, 1.94.1) y el CLI `dx` fijado a la misma versión que el crate.
- **Puente núcleo ↔ UI**: un `Arc<AppState>` global en un `OnceLock` (mutexes y atómicos), un
  `tokio::sync::broadcast` de eventos del núcleo, y en la UI un `use_future` que los pasa a
  señales locales. La UI llama al núcleo directo y el trabajo bloqueante va por
  `spawn_blocking`. La cola de GPU es un `std::thread` propio que recibe trabajos por
  `std::sync::mpsc` y responde por `oneshot`.
- **Estilos**: Tailwind v4 compilado con Bun (`bunx @tailwindcss/cli`) a `assets/tailwind.css`,
  que se versiona; temas como clases que redefinen variables `--color-*`; recursos con
  `asset!()`. Ojo: `cargo run` no junta los `asset!()`, hay que usar `dx serve` o `dx build`.
- **Ventana sin decoraciones** con barra de título propia (`use_window()`, `drag()`).
- **Empaquetado Linux por scripts**, sin `dx bundle`: tarball portable, `.deb` a mano, AppImage
  (sin incluir WebKitGTK), PKGBUILD de AUR; prueba de humo en Docker con `xvfb-run`.
- **WebKitGTK**: `WEBKIT_DISABLE_DMABUF_RENDERER=1` por defecto en los lanzadores (ventana en
  blanco con NVIDIA/Wayland).
- **Sin i18n**: las cadenas están en inglés en el `rsx!`. No hay nada que copiar para idiomas.

### 2.2 Enigmata Temple (verificado por un subagente que leyó los dos repositorios)

`Enigmata-Temple` (0.4.0) solo compila para Windows; `Enigmata-Temple-linux` (0.7.0) agrega Linux
(`unicode_posix.cpp`, VAAPI, empaquetado). Fuera de eso tienen la misma forma. No es un
escalador de ventanas: es un servidor que procesa video (FFmpeg, libplacebo, Anime4K, LSFG) y lo
manda a un teléfono. No tiene superposición, captura, swapchain ni código Vulkan en Rust.

- **Workspace** (resolver 3, edición 2024, Rust 1.94.1): `temple-core` (biblioteca),
  `temple-cli`, `temple-ui` (Dioxus 0.7.10 Desktop), `temple-bench`. Release con `lto = "thin"`,
  `codegen-units = 1`, `strip`. Lints: `unsafe_code = "deny"`, clippy `todo` y `dbg_macro` en
  advertencia.
- **lsfg no va por FFI, sino como proceso aparte.** `native/temple-lsfg/` se compila con CMake
  (C++20, sin SDK de Vulkan) a un ejecutable `temple-lsfg`. Rust lo lanza con
  `tokio::process::Command` y le pasa cuadros RGBA8 por stdin y los recibe por stdout; `--probe`
  da un JSON de una línea, los códigos de salida y una línea de evento por stderr dicen qué pasó.
  Dentro, tres hilos (lectura, GPU, escritura), hasta 3 cuadros en vuelo y staging
  `HOST_CACHED`. Para eso agrega `createContextLocal` a lsfg-vk (imágenes propias) y deja sin usar
  el camino por fd (`createContext`). El ADR `docs/adr/0001-stack.md` lo justifica por
  aislamiento de fallas y cancelación, licencias y su propio Vulkan.
- **Lo que lirtup ya copió de Temple** en la 1.3: lsfg-vk vendorizado, `shader_loader.cpp`
  (RT_RCDATA 353..400 FP32, 304..351 FP16), `unicode_posix.cpp`, la búsqueda de la DLL en Steam
  (`libraryfolders.vdf`, también Steam de Flatpak) y `DISABLE_LSFGVK=1`. lirtup lo usa **en el
  mismo proceso** y comparte imágenes por fd, sin staging; eso no viene de Temple.
- **Puente núcleo ↔ UI** (`temple-ui/src/worker.rs`): `CoreThread::spawn` crea un `std::thread`
  con su runtime de tokio; la UI le habla por un `CoreLink { commands: mpsc::UnboundedSender,
  events: broadcast::Sender }` que entra como contexto
  (`LaunchBuilder::desktop().with_context(link)`). El núcleo hace `tokio::select!` sobre comandos
  y temporizadores. Al cerrar, `Event::LoopDestroyed` llama a `core.shutdown(timeout)`.
- **Estado de la UI estilo Elm**: `UiModel::update(Action) -> Option<Effect>` en `model.rs`,
  puro y con tests (2 770 líneas). La UI tiene una señal con el modelo y un `use_callback` que
  despacha; el `Effect` se manda al núcleo. Un `use_future` escucha los eventos (con `Lagged`
  pide un refresco completo) y otro marca el ritmo.
- **Estilos**: CSS a mano, **sin Tailwind**. `assets/tokens.css` se genera con
  `scripts/generar-tokens-css.js` desde los tokens de Material 3 Expressive; íconos en
  `src/icons.rs`; un test (`tests/no_external_urls.rs`) prohíbe URLs externas (sin fuentes de
  CDN).
- **Sin i18n ni bandeja.**
- **Empaquetado**: `dx build -p temple-ui --desktop --release` con `dx` fijado; script de
  release que arma `/opt/enigmata-temple`, incluye `libxdo.so.3` con `patchelf`, genera `.deb`
  (`dpkg-shlibdeps`) y AppImage; CI con fmt, clippy `-D warnings` y tests.
- **Tests**: unos 900, con ~30 archivos de integración en `temple-core/tests/`; los del worker
  levantan `CoreThread` con un runtime de un solo hilo; `tempfile` para no tocar carpetas reales.
- **Detalles**: WebKitGTK en blanco con NVIDIA + Wayland (`WEBKIT_DISABLE_DMABUF_RENDERER=1`);
  `libxdo.so.3` vs `.so.4` en Arch; dioxus-desktop 0.7.10 escribe `window-state.json` en debug;
  ignorar `SIGPIPE` en el auxiliar.

### 2.3 Qué tomar de cada uno

| Tema | De dónde | Para lirtup |
| --- | --- | --- |
| Workspace con núcleo, CLI y UI separados, edición 2024, toolchain fijo, lints | Temple | Sí (3.1). `unsafe_code = "deny"` en todos los crates salvo `lirtup-gpu`, `lirtup-x11` y `lsfg-sys`, donde se permite con `// SAFETY:` obligatorio |
| `CoreThread` + `CoreLink` (comandos por mpsc, eventos por broadcast, entra como contexto) | Temple | Sí, es más limpio que el `OnceLock` global de Miko Kagura y se puede probar |
| Modelo de UI puro `update(Action) -> Option<Effect>` con tests | Temple | Sí, para el estado de la GUI (perfiles, borradores sin guardar, pestañas) |
| Tailwind v4 compilado con Bun | Miko Kagura | Sí, si el diseño viene en Tailwind; los tokens de Material 3 Expressive se generan a CSS como en Temple y Tailwind los usa en `@theme` |
| Sin URLs externas (test) | Temple | Sí: fuentes e íconos dentro del paquete |
| Ventana sin decoraciones con barra propia | Miko Kagura | Según el diseño |
| Empaquetado por scripts: AppImage, `.deb`, PKGBUILD, `libxdo.so.3`, `WEBKIT_DISABLE_DMABUF_RENDERER=1` | los dos | Sí; lirtup agrega el PKGBUILD de AUR (Miko Kagura lo tiene) |
| lsfg como proceso aparte | Temple | No como primera opción (ver 3.5): lirtup necesita cero copias y baja latencia |
| i18n, bandeja, Vulkan en Rust, X11 | ninguno | Es terreno nuevo; ver 3.2, 3.3, 3.8 y 3.9 |

## 3. Arquitectura del núcleo en Rust

### 3.1 Workspace

```
lirtup/                      (workspace, edición 2024, toolchain fijo)
  crates/
    lirtup-config/           Config, perfiles, YAML, validadores, CLI (clap)
    lirtup-gpu/              ash: instancia, dispositivo, recursos, compute, timestamps,
                             memoria externa, swapchain, mutex de cola
    lirtup-x11/              x11rb: captura (XShm + XDamage + tiles), ventanas, foco,
                             atajos, reenvío de entrada, ventana de superposición
    lirtup-effects/          SRCNN (CuNNy), escaladores y efectos nativos, OSD; .spv embebidos
    lirtup-magpiefx/         parser, expresiones, codegen, compilador, .dds, ejecución
    lsfg-sys/                C++ vendorizado + API C + build.rs (cc)
    lirtup-fg/               capa lsfg-vk (config, entorno) y motor integrado sobre lsfg-sys
    lirtup-pipeline/         hilo del pipeline, presentador, tiles, stats, latencia,
                             hilo de presentación de FG, daemon, medición
    lirtup-core/             API pública: Engine, comandos, eventos, estado (contrato con la UI)
    lirtup-cli/              binario `lirtup` (reemplaza `upscale`)
    lirtup-ui/               binario `lirtup-gui`, Dioxus Desktop
  assets/                    tailwind.css, íconos, fuentes
  i18n/                      es.ftl, en.ftl
  shaders/                   .hlsl y .spv de CuNNy y nativos; effects/ de Magpie
  packaging/                 .desktop, metainfo, PKGBUILD, AppImage
```

Las dependencias van en una sola dirección: `config` y `gpu` abajo; `x11`, `effects`,
`magpiefx` y `fg` encima de `gpu`; `pipeline` encima de todos; `core` es la fachada; `cli` y
`ui` solo dependen de `core` (y de `config` para los tipos). Eso es lo que permite repartir el
trabajo por crates sin que dos subagentes toquen lo mismo (sección 5).

### 3.2 Captura X11

Opciones:

| Opción | A favor | En contra |
| --- | --- | --- |
| **Portar a `x11rb`** (recomendada) | Puro Rust, con las extensiones `shm`, `damage`, `xfixes` y `composite`. Una sola conexión por hilo, sin Xlib ni su bloqueo global. El segmento compartido se crea con `memfd` y `shm::attach_fd` | Hay que reescribir unas 1 100 líneas de C. El rendimiento de `ShmGetImage` es el mismo (lo hace el servidor), pero hay que medirlo |
| Mantener el C con `cc` y FFI | Cero riesgo de rendimiento | Mezcla Xlib (`XInitThreads`) con x11rb en el mismo proceso; otro `build.rs` y otro `unsafe` |
| `xcb` (rust-xcb) | Envuelve `libxcb` | Menos idiomático que x11rb; no aporta nada aquí |

`xxhash-rust` (con `xxh64`) reemplaza a `xxhash64.c`. La caché de tiles y la expansión de
rectángulos (`pipeline/utils.py`) son lógica pura, fáciles de probar con tests unitarios.

Ventanas, foco, PID, ícono (`_NET_WM_ICON`) y atajos (`GrabKey`) son llamadas de protocolo que
ya se hacen con xcffib: pasan casi línea por línea a x11rb.

### 3.3 Vulkan

| | `ash` | `vulkano` | `wgpu` |
| --- | --- | --- | --- |
| Memoria y semáforos externos por fd (lsfg) | Sí, directo (`khr::external_memory_fd`) | Sí (`export_fd`), pero las imágenes compartidas con otro `VkDevice` quedan fuera de su seguimiento de sincronización y hay que marcarlas a mano | No en la API pública; solo con `as_hal` y `wgpu-hal`, que es inestable |
| Timestamps por dispatch | Sí | Sí | Solo con `TIMESTAMP_QUERY_INSIDE_PASSES`, nativo |
| `robustImageAccess` | Se activa como hoy | Se activa como hoy | wgpu aplica su propio chequeo de límites en el shader traducido; no se elige |
| SPIR-V de dxc tal cual | Sí | Sí | Pasa por naga, que no acepta todo lo que genera dxc (16 bits, `Rmp8x8`, ciertos `OpTypeImage`), salvo con `SPIRV_SHADER_PASSTHROUGH` |
| Superficie XCB y modos de presentación | Sí | Sí | Sí, desde un `raw-window-handle` |
| Prioridad de colas o cola de cómputo aparte (pendiente 5) | Sí | Sí | No |
| `VK_KHR_present_wait` (pendiente 6) | Sí | Parcial | No |
| Costo | Todo `unsafe`, como el C++ de hoy | Menos `unsafe`, pero otro modelo mental | El más cómodo, pero lo que se pierde es justo lo que necesita lirtup |

**Recomendación: `ash`**, con una capa propia delgada y segura (`Texture`, `Buffer`,
`ComputePipeline`, `CommandRecorder`, `Swapchain`) que copie la forma del módulo `vulkan` de hoy.
El port es casi mecánico desde `vk_*.cpp`. `gpu-allocator` puede reemplazar al `Heap` propio,
pero las texturas exportables siguen yendo con asignación dedicada, fuera del asignador.

Reglas que hoy pone el GIL y en Rust van explícitas:

- Un `Mutex<vk::Queue>` en el dispositivo: todo `vkQueueSubmit`, `vkQueuePresentKHR` y
  `vkQueueWaitIdle` pasa por él (lo mismo que se hizo en la 1.3 para soltar el GIL en `fifo`).
- `Send`/`Sync` solo donde corresponde: los command buffers y los descriptor pools son de un
  hilo.

### 3.4 SRCNN y MagpieFX

**SRCNN y shaders nativos:** el `.spv` y el JSON de cada modelo se embeben con `include_bytes!`
(unos 230 archivos) o se instalan en `/usr/share/lirtup/`; el loader (`srcnn/loader.py`,
`factory.py`) es poca lógica. Las clases de shaders nativos (`shaders/**/__init__.py`) son
descripciones de descriptores, constantes y dispatch: pasan a un trait `Pass` con una
implementación por shader. `blue_noise.py` pasa a un `.bin` embebido.

**MagpieFX:** parser, expresiones, codegen, swizzle, `.dds` y la ejecución son lógica pura sobre
texto y bytes (unas 2 500 líneas de Python); se portan sin problema y los tests de hoy dan los casos.
Para compilar HLSL hay tres caminos:

| Opción | A favor | En contra |
| --- | --- | --- |
| **Subproceso `dxc` / `glslangValidator`** (recomendada para la paridad) | Es lo que hace hoy, con los mismos argumentos y la misma caché; nada que enlazar | Necesita los binarios instalados |
| `hassle-rs` (carga `libdxcompiler.so` en tiempo de ejecución) | Sin subproceso; más rápido en la primera carga | La biblioteca tiene que estar igual; API menos estable |
| `shaderc` (glslang) | Se puede enlazar estático | El frente HLSL de glslang es el respaldo, no el principal: 3 efectos solo compilan ahí, pero la mayoría prefiere dxc |

Recomiendo el subproceso detrás de un trait `HlslCompiler`, con `hassle-rs` como mejora
opcional más adelante.

### 3.5 Generación de cuadros (lsfg)

- **Capa lsfg-vk:** no cambia. Antes de `ash::Entry::load` el proceso escribe `lsfg-vk.toml` y
  pone las variables de entorno, igual que `framegen.prepare()`. El reinicio para cambiar el
  multiplicador pasa de `os.execve(sys.executable)` a `exec` de `/proc/self/exe`.
- **Motor integrado:** el C++ vendorizado se queda. Hay tres formas de llamarlo desde Rust:

  | Opción | A favor | En contra |
  | --- | --- | --- |
  | **FFI en el mismo proceso, imágenes por fd** (recomendada) | Es lo que la 1.3 ya probó con video; cero copias; la latencia más baja | Un fallo de lsfg (o del driver dentro de lsfg) tira la app; hace falta `unsafe` |
  | Proceso aparte con cuadros por pipe (lo que hace Temple) | Aislado: si se cae, la app sigue; sin `unsafe`; se reusa `temple-lsfg` casi tal cual | Cada cuadro baja de la GPU y vuelve a subir (a 1440p, unos 15 MB por cuadro en cada sentido; a ×4, unos 60 MB por cuadro del juego, estimado): más latencia y más carga, justo lo que la pendiente 5 quiere bajar |
  | Proceso aparte con fds por socket Unix (`SCM_RIGHTS`) | Aislado y sin copias: la memoria opaca exportada sirve en otro proceso del mismo driver | No probado en ninguno de los proyectos; protocolo propio de ciclos y semáforos |

  Recomiendo la primera, con la tercera anotada como mejora si los cierres por lsfg resultan ser
  un problema. Para la FFI se cambia `lsfg_module.cpp` (puente de CPython) por una API C chica
  (`lsfg_c.h`):

  ```c
  int  lsfg_probe(const char *dll, char *err, size_t errlen);
  int  lsfg_initialize(const char *dll, uint32_t vendor, uint32_t device, int perf,
                       int hdr, float flow, int generated);
  int  lsfg_create_context(int in0, int in1, const int *outs, size_t n,
                           uint32_t w, uint32_t h, uint32_t vk_format);
  int  lsfg_present(int ctx, int in_sem, const int *out_sems, size_t n);
  void lsfg_delete_context(int ctx);
  void lsfg_finalize(void);
  const char *lsfg_last_error(void);
  ```

  Las excepciones de C++ se atrapan en el borde y vuelven como código y mensaje. `lsfg-sys`
  compila todo con el crate `cc` (los mismos archivos y banderas que `setup.py`: C++20,
  `-fvisibility=hidden`, `VK_NO_PROTOTYPES`, `-Wl,-Bsymbolic`), sin CMake ni bindgen: la API es
  tan chica que las declaraciones `extern "C"` se escriben a mano. volk sigue cargando su propio
  `libvulkan` con visibilidad oculta, así no choca con `ash`.
- **El resto de `native_framegen.py`** (multiplicador automático, ciclos, hilo `lsfg-present`,
  ritmo, reinicio en segundo plano) es Rust en `lirtup-fg` y `lirtup-pipeline`.

### 3.6 Superposición y presentación

Hoy la superposición es una `QMainWindow` de Qt (translúcida, siempre encima, modos
`always-on-top`, `top-transparent`, `fullscreen`, `windowed`) y el swapchain usa su `winId` con
una conexión XCB aparte.

En Rust:

- **Ventana propia con `x11rb`**: visual ARGB de 32 bits para la transparencia,
  `_NET_WM_STATE_ABOVE` o `_NET_WM_STATE_FULLSCREEN`, `_NET_WM_BYPASS_COMPOSITOR` cuando
  corresponda, sin decoraciones (`_MOTIF_WM_HINTS`), y la región de entrada con XFixes
  (`SetWindowShapeRegion`) cuando tiene que dejar pasar el ratón.
- **Superficie Vulkan** con `VK_KHR_xcb_surface`. `x11rb` puede abrir su conexión sobre
  `libxcb` (`XCBConnection`), así el puntero `xcb_connection_t*` sirve para la superficie.
- **Eventos**: el hilo de la superposición lee sus eventos (ratón, teclado, `ConfigureNotify`)
  y los traduce con el `CoordinateMapper` a eventos sintéticos para la ventana original
  (`SendEvent`), como `X11EventForwarder`.
- **`winit` no**: sirve para la ventana y la superficie, pero esconde el visual ARGB, los
  atributos de override y los eventos crudos, y quiere el hilo principal, que en Dioxus Desktop
  ya es de tao/GTK.
- **OSD y panel de stats**: hoy Pillow dibuja el texto en una imagen que se sube y se mezcla.
  En Rust, `ab_glyph` (o `fontdue`) dibuja en un buffer RGBA con la misma fuente; la mezcla
  (`overlay_blend.hlsl`) no cambia.
- **Capturas de pantalla**: lectura del `screen_tex_8bit` y `image` para el PNG.

### 3.7 Hilos

| Hilo | Hoy | En Rust |
| --- | --- | --- |
| Principal | Bucle de Qt (GUI, superposición, temporizadores) | Bucle de Dioxus/tao (GTK). Solo la GUI |
| Pipeline | `threading.Thread` (`PipelineThread`) | `std::thread` con su conexión X11; captura, cómputo y presentación como hoy |
| Presentación de FG | `lsfg-present` | `std::thread`; recibe ciclos por canal acotado; comparte la cola por el mutex |
| Reinicio de lsfg | Hilo temporal | `std::thread` temporal o tarea en el hilo de FG |
| Superposición y entrada | Hilo de Qt | `std::thread` propio (bucle de eventos X11 de la superposición) |
| Atajos globales | `HotkeyManager` en Qt | `std::thread` con `GrabKey` y su propia conexión |
| Foco y daemon | `threading.Thread` con sondeo | `std::thread` o tareas de `tokio` con intervalo |
| Compilación de MagpieFX y miniaturas | En el hilo que las pide | `spawn_blocking` |

**Comunicación**: el patrón `CoreThread` + `CoreLink` de Temple. La UI manda comandos por un
`mpsc` al hilo del núcleo (con su runtime de tokio), que reenvía al pipeline lo que toca por un
canal que el pipeline lee entre cuadros (`crossbeam-channel`); los eventos del núcleo a la UI
van por `tokio::sync::broadcast`; los datos de alta frecuencia (stats, fps) por un
`watch` o un `ArcSwap` que la UI lee a su ritmo (dos veces por segundo, como el panel), nunca un
evento por cuadro.

La configuración viva se publica como `Arc<Config>` inmutable (`arc-swap`): la UI crea una nueva
y el pipeline la toma al principio del cuadro siguiente, igual que hoy `update_config`.

### 3.8 Bandeja

| Opción | A favor | En contra |
| --- | --- | --- |
| **`ksni`** (StatusNotifierItem por D-Bus, puro Rust) | Funciona bien en KDE (la máquina de la persona usa Plasma), sin GTK, en su propio hilo; menús con submenús y casillas | En GNOME necesita la extensión AppIndicator, igual que hoy Qt |
| `tray-icon` (lo que usa Dioxus/tauri) | Integrado con el bucle de tao | En Linux va por `libayatana-appindicator` y GTK; menús más limitados |

Recomiendo `ksni`. El menú actual (reiniciar, cambiar de ventana, de perfil, multiplicador)
pasa como está; las acciones son los mismos comandos que manda la UI.

### 3.9 GUI con Dioxus

- **Dioxus Desktop 0.7** (WebKitGTK), como Miko Kagura. Dioxus Native (Blitz) todavía no está
  listo para una GUI de este tamaño.
- **Tailwind v4** compilado con Bun a `assets/tailwind.css`, como Miko Kagura; los tokens de
  Material 3 Expressive del diseño se generan a CSS con un script, como en Temple
  (`generar-tokens-css.js`), y Tailwind los toma en su `@theme`.
- **Estado**: el modelo puro de Temple (`update(Action) -> Option<Effect>`) para la GUI, y el
  `CoreLink` como contexto.
- **Miniaturas de ventanas**: el núcleo captura cada ventana una vez (XShm o `GetImage`), la
  achica y la entrega como PNG o WebP en base64, o por un protocolo propio de Dioxus
  (`use_asset_handler`), que evita base64.
- **i18n**: `fluent` (con `fluent-templates` o `i18n-embed`), archivos `i18n/{es,en}.ftl`, una
  señal global de idioma y una función `t!("clave")`. Las 526 cadenas de los `.ts` se pasan a
  `.ftl` con un script de una vez; el contexto de Qt pasa a ser el prefijo de la clave.
- **El contrato con el núcleo** (estado, comandos y eventos) se detalla en la fase 2. La
  semilla: `Engine::start(target, profile)`, `stop()`, `apply_config(Arc<Config>)`,
  `list_windows()`, `thumbnail(window)`, `bench(...)`, y los eventos `Started`, `Stopped`,
  `Error`, `WindowChanged`, `DaemonMatch`, `FrameGenStatus` y `Stats`.

### 3.10 Qué se conserva en C/C++ y qué se porta

| Pieza | Decisión | Por qué |
| --- | --- | --- |
| lsfg-vk 3.1 y 3.1P, pe-parse, volk | **Se conserva** en C++ (vendorizado) | Es código de terceros que ya funciona; portarlo no aporta y alejaría el código de su origen |
| `shader_loader.cpp`, `unicode_posix.cpp` | **Se conserva** | Parte del mismo bloque, copiado de Temple |
| `lsfg_module.cpp` | **Se reemplaza** por `lsfg_c.cpp` (API C) | Era el puente con CPython |
| `vulkan/lib/*.cpp` | **Se porta** a `ash` | Atado a la API de Python; la lógica Vulkan pasa casi igual |
| `capture/lib/*.c` | **Se porta** a `x11rb` (con el C como respaldo si el rendimiento no da) | Chico, y elimina Xlib |
| Shaders `.hlsl`/`.spv` | **Se conservan** | Son datos |
| Python | **Se porta** todo | — |

## 4. Estrategia por etapas

### 4.1 Opciones

| Opción | Cómo | A favor | En contra |
| --- | --- | --- | --- |
| A. Núcleo de Rust detrás de la GUI de Qt (PyO3) | El pipeline pasa a Rust y se llama desde Python | Siempre hay una app que funciona | El puente PyO3 se tira al final; la superposición y los atajos dependen de Qt y habría que separarlos dos veces; el hilo del pipeline y el GIL vuelven a mezclarse |
| B. Todo de una | Workspace nuevo, se cambia cuando está todo | Sin trabajo descartable | Mucho tiempo sin nada que probar; las fallas aparecen todas juntas |
| **C. Núcleo como CLI primero, después la GUI** (recomendada) | Ronda 1 y 2: núcleo y `lirtup` (CLI); se compara con `upscale` en las mismas ventanas. Ronda 3: GUI Dioxus sobre `lirtup-core`. Ronda 4: bandeja, daemon, empaquetado, paridad | Cada etapa se prueba sola; la GUI puede empezar contra un núcleo falso; nada se tira | La app de Python sigue siendo la que se publica hasta la ronda 4 (hay que congelar funciones nuevas o hacerlas dos veces) |

### 4.2 Rondas

| Ronda | Contenido | Resultado que se prueba |
| --- | --- | --- |
| 0 (revisor, corta) | Workspace, `Cargo.toml`, crates vacíos con su API pública escrita como firmas (`todo!()`), tipos compartidos, `lsfg_c.h`, reglas | Que el workspace pase `cargo check` vacío |
| 1 | `lirtup-gpu`, `lirtup-x11`, `lirtup-effects` + `lirtup-magpiefx`, `lirtup-config` + `lsfg-sys` + `lirtup-fg` | Tests unitarios; ejemplos sin ventana (escalar una imagen, compilar los 117 efectos, sondear lsfg) |
| 2 | `lirtup-pipeline`, `lirtup-core`, `lirtup-cli`, superposición y entrada | `lirtup` escala una ventana como `upscale`: mismos modos, efectos, tiles, FG por capa y por motor |
| 3 | `lirtup-ui` (Dioxus), según el diseño de la fase 2 | La GUI completa contra el núcleo real |
| 4 | Bandeja, daemon, reinicio, medición, i18n completo, empaquetado (AUR, AppImage), documentación, paridad | Reemplaza a la 1.x |

### 4.3 Esfuerzo (estimado)

Cuenta hecha a partir de las líneas de hoy, sin medir. En Rust se escribe algo menos que en
Python para la lógica y algo más para Vulkan y X11.

| Pieza | Líneas de Rust (estimado) | Dificultad |
| --- | ---: | --- |
| `lirtup-gpu` | 3 500 – 4 500 | Media: port mecánico de C++ |
| `lirtup-x11` (captura, ventanas, entrada, atajos, superposición) | 2 500 – 3 500 | Media-alta: la superposición nueva |
| `lirtup-effects` | 2 000 – 3 000 | Media |
| `lirtup-magpiefx` | 2 000 – 2 500 | Media |
| `lirtup-config` | 1 500 – 2 500 | Baja-media: muchos campos |
| `lsfg-sys` + `lirtup-fg` | 800 – 1 200 | Media |
| `lirtup-pipeline` + `lirtup-core` + `lirtup-cli` | 4 000 – 5 500 | Alta: es donde se juega la paridad |
| `lirtup-ui` | 4 000 – 6 000 (+ CSS) | Media, depende del diseño |
| Total | ~20 000 – 28 000 | |

En rondas como la 1.3 (4 subagentes y un revisor), unas **4 rondas más una de correcciones**
(estimado). La ronda 2 es la más larga y la que más pasadas de corrección va a necesitar.

### 4.4 Riesgos

| Riesgo | Mitigación |
| --- | --- |
| La superposición nueva se comporta distinto según el gestor de ventanas (KWin, Mutter, XWayland) | Probar los cuatro modos en KWin X11 y en Plasma Wayland (XWayland) en la ronda 2 antes de seguir |
| Rendimiento de captura con x11rb | Medir contra la versión C en la ronda 1 con un ejemplo; si pierde, usar el C con `cc` |
| Sincronización: lo que hoy separa el GIL queda a cargo de mutexes explícitos | Mutex de cola en `lirtup-gpu` desde el primer día; validation layers en todos los ejemplos |
| Paridad de imagen | Comparar bit a bit la salida de Rust y de Python en las mismas imágenes de prueba (ya existe la prueba de rojo/verde/azul de MagpieFX) |
| WebKitGTK: ventana en blanco con DMA-BUF, consumo de memoria | `WEBKIT_DISABLE_DMABUF_RENDERER=1` en el lanzador, como Miko Kagura |
| La GUI de Dioxus y la superposición en el mismo proceso: GTK y X11 compiten por el hilo principal | La superposición nunca toca GTK; tiene su propia conexión X11 en su hilo |
| Dos versiones vivas durante meses | Congelar funciones nuevas en Python durante las rondas 1 a 4, o anotarlas para portar |
| Cambios de API de Dioxus (0.7 → 0.8) | Fijar el crate y `dx` a la misma versión, como Miko Kagura |
| Licencias: Magpie (GPLv3), shader loader de Temple (GPL-3.0), lsfg-vk (MIT) | El workspace queda en GPL-3.0-or-later como hoy |

### 4.5 Tests que se portan

| Hoy (pytest) | En Rust |
| --- | --- |
| `test_config_manager.py`, `test_restart.py` | `lirtup-config`: ida y vuelta de `config-example.yaml`, perfiles, diferencias de atajos |
| `test_magpiefx_source.py`, `_dds.py`, `_fp16.py` | `lirtup-magpiefx`: los mismos casos |
| `test_framegen.py`, `test_native_framegen.py` | `lirtup-fg`: multiplicador, histéresis, búsqueda de la DLL, `lsfg-vk.toml` |
| `test_stats.py`, `test_menubar.py` | `lirtup-pipeline`: aviso de GPU, latencia, barra de menú |
| `test_bench.py`, `test_bench_dialog.py` | `lirtup-pipeline::bench` con la GPU simulada; el diálogo pasa a la UI |
| `test_translations.py`, `test_presentation_tab.py` | `lirtup-ui`: que todas las claves existan en `es.ftl` y `en.ftl`; los tests de pestañas se rehacen sobre los componentes |

## 5. Plan de ejecución en paralelo

### 5.1 Equipo y reglas

- **4 subagentes Opus 5.5 con esfuerzo medio** por ronda, cada uno con crates propios.
- **1 revisor general Opus 5.5 con esfuerzo alto**: escribe la ronda 0 (el contrato), revisa a
  cada subagente al terminar, integra y corre los builds.
- Cada subagente trabaja en su worktree y su rama (`rust/r<N>-<frente>`) desde la rama de
  integración de la ronda anterior.
- **Sin builds durante los frentes.** Ningún subagente corre `cargo build`, `dx build`,
  `cargo test` ni la app. Puede correr `cargo check -p <su crate>` para ver errores de tipos
  (es solo análisis, no genera binarios); si la persona prefiere que tampoco, se quita. Los tests
  unitarios se escriben, no se corren.
- **No toca** lo compartido: `Cargo.toml` del workspace (las dependencias nuevas se piden en la
  nota de rama y las agrega el revisor), `Cargo.lock`, `i18n/*.ftl`, `CHANGELOG.md`, la
  versión.
- **No cambia la API pública de otro crate.** Si la necesita distinta, lo anota en su nota de
  rama (`docs/developer/rust-r<N>-<frente>.md`) para el revisor.
- Un commit por cambio coherente, sin líneas de coautoría.

### 5.2 Ronda 0 (revisor)

1. Workspace con los crates de 3.1, cada uno con su `lib.rs` y la API pública como firmas con
   `todo!()` y documentación: tipos de `lirtup-gpu` (`Device`, `Texture`, `Buffer`, `Format`,
   `ComputePipeline`, `Dispatch`, `Timestamps`, `ExternalTexture`, `ExternalSemaphore`,
   `Swapchain`), de `lirtup-x11` (`WindowInfo`, `Capture`, `Frame`, `DamageRect`,
   `OverlayWindow`, `InputForwarder`, `Hotkeys`), de `lirtup-config` (`Config` completo con
   serde, `Profile`, `Match`) y de `lirtup-core` (comandos y eventos de 3.9).
2. `lsfg-sys/include/lsfg_c.h` con la API de 3.5.
3. Dependencias del workspace fijadas (`ash`, `x11rb`, `serde`, `serde_yaml_ng`, `clap`,
   `crossbeam-channel`, `arc-swap`, `tokio`, `dioxus 0.7.x`, `ksni`, `fluent`, `image`,
   `ab_glyph`, `xxhash-rust`, `cc`), `rust-toolchain.toml`, perfiles como Miko Kagura.
4. `cargo check` del workspace vacío. Es el único build de la ronda 0.

### 5.3 Ronda 1: frentes

| Frente | Toca | No toca | Entregable |
| --- | --- | --- | --- |
| **G. GPU** | `crates/lirtup-gpu/**` | el resto | Port de `vulkan/lib`: instancia, selección de dispositivo, recursos, compute con timestamps, memoria y semáforos externos, swapchain XCB, mutex de cola, `robustImageAccess`, FP16. Ejemplo `examples/copy.rs` sin ventana |
| **X. X11** | `crates/lirtup-x11/**` | el resto | Captura XShm + XDamage + tiles con xxh64; ventanas, foco, ícono, PID; atajos; reenvío de entrada; ventana de superposición (sin swapchain, que conecta la ronda 2). Ejemplo que captura una ventana a PNG |
| **E. Efectos** | `crates/lirtup-effects/**`, `crates/lirtup-magpiefx/**`, `shaders/**` | `lirtup-gpu` (solo lo usa por su API) | SRCNN, escaladores, efectos nativos, conversores, OSD; MagpieFX completo con compilador por subproceso y caché. Tests de parser, `.dds`, FP16 |
| **C. Config y FG** | `crates/lirtup-config/**`, `crates/lsfg-sys/**`, `crates/lirtup-fg/**` | el resto | `Config` con todos los campos y validadores, perfiles, CLI con clap; `lsfg_c.cpp` y `build.rs`; capa (toml, entorno, búsqueda de DLL y capa); multiplicador automático. Tests de config y FG |

Orden de integración de la ronda 1 (cada uno sobre el anterior, en la rama
`rust/r1-integration`): **C** (no depende de nadie) → **G** → **X** (usa `Format` de G) → **E**
(usa G). El revisor resuelve conflictos, agrega las dependencias pedidas y corre el build.

### 5.4 Ronda 2: frentes

| Frente | Toca | Entregable |
| --- | --- | --- |
| **P. Pipeline** | `crates/lirtup-pipeline/src/{pipeline,tiles,upscale,presenter}.rs` | Hilo del pipeline, tiles, escalado, cadena de efectos y presentación |
| **F. FG en el pipeline** | `crates/lirtup-pipeline/src/{framegen,pacing}.rs`, `crates/lirtup-fg/**` | Hilo de presentación de FG, ciclos, ritmo, reinicio en segundo plano |
| **S. Stats y OSD** | `crates/lirtup-pipeline/src/{stats,osd,latency,menubar,bench}.rs` | Panel de stats, OSD, CSV de latencia, barra de menú, medición |
| **K. Core y CLI** | `crates/lirtup-core/**`, `crates/lirtup-cli/**`, `crates/lirtup-x11/src/overlay*` | Fachada `Engine`, comandos y eventos, binario `lirtup`, conexión de la superposición con el swapchain |

Orden: **S** → **P** → **F** → **K**. El `mod.rs` de `lirtup-pipeline` lo escribe el revisor en
la ronda 0 de esta ronda, con los módulos ya declarados, para que P, F y S no compartan archivo.

### 5.5 Rondas 3 y 4

Se detallan en la fase 2, con el diseño: la ronda 3 reparte `lirtup-ui` por pantallas
(componentes propios por subagente, un `app.rs` y un `state.rs` del revisor); la ronda 4 es
bandeja, daemon, i18n, empaquetado y paridad.

### 5.6 Integración y builds

Al terminar los cuatro frentes de una ronda:

1. El revisor revisa cada rama contra su nota y contra el contrato de la ronda 0.
2. Merge en el orden de la ronda.
3. **Build 1**: `cargo build --workspace --release`, `cargo clippy --workspace`,
   `cargo test --workspace`, y en la ronda 3 en adelante `dx build --release` (antes,
   `bun run tailwind:build`).
4. Una sola pasada de correcciones con todo lo que falló, repartida por crate si hace falta.
5. **Build 2** igual al 1. Si algo sigue fallando, se anota y pasa a la ronda siguiente.
6. Pruebas a mano de la ronda (un guion como `round-1.3-test-script.md`).

### 5.7 Criterios de aceptación

| Ronda | Criterio |
| --- | --- |
| 0 | El workspace pasa `cargo check`; cada crate tiene su API pública documentada |
| 1 | `cargo test --workspace` en verde; el ejemplo de G corre sin errores de validation layers; X captura una ventana a PNG; E compila los 117 efectos con dxc (los 3 Denoise con glslang) y da la misma salida que Python en la imagen RGB de prueba; C lee `config-example.yaml` y los perfiles de la persona sin cambiarlos; `lsfg_probe` encuentra `Lossless.dll` |
| 2 | `lirtup` escala una ventana en los cuatro modos de superposición, con tiles, reenvío de ratón y atajos; los tiempos de GPU de la tabla del README dentro de un 5 % de la versión de Python; FG por capa y por motor integrado a ×2 y ×4 con video, sin errores de validación; panel de stats y CSV de latencia iguales en contenido |
| 3 | Todas las pantallas y flujos del diseño funcionan contra el núcleo real; las cadenas en español completas |
| 4 | Bandeja, daemon y reinicio; paquete AUR y AppImage que se instalan y arrancan en una máquina limpia; ningún pendiente de paridad abierto |

## 6. Fase 2 (pendiente)

Cuando esté el artefacto de diseño: mapa de cada pantalla, flujo y componente a componentes
Dioxus; el estado de la UI (señales, contextos, qué vive en el núcleo); el contrato completo
entre `lirtup-core` y `lirtup-ui`; y el reparto de la ronda 3 en frentes.
