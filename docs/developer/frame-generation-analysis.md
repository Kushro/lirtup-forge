# Generación de cuadros (FG): análisis de posibilidad

Estado: **fase 0 integrada** (ajustes `frame_gen_*`, `--frame-gen` y sección en la GUI; ver [integración](#integración-de-la-fase-0-2026-10-31)) (ver [resultados](#fase-0-resultados-2026-10-31)). **Fase 1 integrada y probada con video** (ver [fase 1](#fase-1-implementación-2026-11-01)). Fecha: 2026-11-01. Rama de la fase 1: `feature/next-round`.

Convención: **verificado** = leído en el código o comprobado en esta máquina; **medido en otro
proyecto** = cifra de Enigmata Temple, en otro pipeline; **estimado** = cuenta hecha aquí, sin
medir.

## Resumen

- **Es posible.** La forma más directa es la que ya usa Enigmata Temple: el núcleo de
  [lsfg-vk](https://github.com/FrankBarretta/lsfg-vk-android) (MIT) corre sobre Vulkan los shaders
  de Lossless Scaling, sacados en memoria de la `Lossless.dll` que pone la persona. lirtup-forge es
  GPL-3.0, así que el cargador de shaders de Temple (GPL-3.0) se puede reusar tal cual.
- **Primero conviene un experimento sin código (fase 0).** lirtup presenta con su propio swapchain
  Vulkan, así que la capa implícita de lsfg-vk (instalada aquí: `lsfg-vk 2.0.0`) se le puede
  enganchar como a un juego. Para que sirva hay que cambiar algo chico: lirtup hoy re-presenta el
  mismo cuadro en cada vuelta (`present_unchanged`), y la capa interpolaría entre cuadros
  idénticos.
- **La integración propia (fase 1) es un trabajo mediano-grande**, de unas 2 a 3 semanas
  (estimado). Lo caro no es LSFG, que ya está portado en Temple. Lo caro es lo que lirtup no tiene:
  saber cuándo llega un cuadro nuevo del juego, un hilo que presente a ritmo del monitor
  independiente de la captura, y compartir imágenes entre el dispositivo Vulkan de lirtup y el de
  LSFG.
- **Costo inevitable: latencia.** Interpolar entre los cuadros k-1 y k obliga a esperar a k: se
  suma un intervalo del juego (16,7 ms a 60 fps, 33 ms a 30 fps) a la latencia que lirtup ya
  tiene por capturar con XShm.
- **Descartado para tiempo real:** RIFE (en Temple dio 15 a 33 cuadros/s en una RX 9070 XT, por
  lotes) y FSR 3 (necesita los vectores de movimiento del juego, que una captura de ventana no
  tiene).

## Qué hay en los proyectos de referencia

### Enigmata (Android)

Hace LSFG ×2 a ×5 en el teléfono al reproducir video, con lsfg-vk-android y
`AHardwareBuffer`. Es el origen del código que vendoriza Temple
(`app/src/main/cpp/third_party/`): lsfg-vk, volk y el cargador de shaders. Tiene una política
`FgPolicy.autoMultiplier` que elige el multiplicador según la cadencia de la fuente y el panel.

### Enigmata Temple (`Enigmata-Temple-linux`, v0.7.0)

`native/temple-lsfg/` es un auxiliar de C++/Vulkan (verificado, leído):

| Pieza | Qué hace | Licencia |
| --- | --- | --- |
| `third_party/lsfg-vk/` | Núcleo de framegen v3.1 y v3.1P ("performance"), con la modificación «Temple»: `createContextLocal`, que crea las imágenes de entrada y salida en el dispositivo de LSFG y devuelve sus handles (`TempleLsfg::LocalContext`) | MIT |
| `src/shader_loader.*` | Lee `Lossless.dll` con pe-parse y saca los SPIR-V (RT_RCDATA 353..400 FP32, 304..351 FP16) en memoria; reescribe los bindings | GPL-3.0-only |
| `src/main.cpp` | Lee cuadros RGBA por stdin y escribe por stdout `N-1` intermedios más el real. Tres hilos (lectura, GPU, escritura), hasta 3 cuadros en vuelo, buffers de staging `HOST_CACHED` | MIT |
| `third_party/volk`, `pe-parse`, `vulkan-headers` | Carga de Vulkan, parser PE, cabeceras | MIT / Apache-2.0 |

API pública de lsfg-vk (`public/lsfg_3_1.hpp`):

```cpp
LSFG_3_1::initialize(deviceUUID, isHdr, flowScale, generationCount, loader);
LSFG_3_1::createContext(int in0Fd, int in1Fd, std::vector<int> outFds, VkExtent2D, VkFormat);
LSFG_3_1::createContextLocal(VkExtent2D, VkFormat);       // modificación de Temple
LSFG_3_1::presentContext(id, inSemFd, std::vector<int> outSemFds);
LSFG_3_1::deleteContext(id);  LSFG_3_1::finalize();
```

LSFG crea **su propio `VkDevice`** con `initialize`. Hay dos maneras de pasarle imágenes: por fd
de memoria externa (`createContext`, lo que usa la capa lsfg-vk de escritorio) o con imágenes
propias y copias por staging (`createContextLocal`, lo que usa Temple).

Cifras de Temple (medidas en otro proyecto; Temple las marca como humo, no medición, en DT-55):

- El auxiliar solo supera 500 cuadros/s de salida en 1080p ×5.
- `--performance` y `--flow-scale 0.5` no cambiaron el tiempo total porque el límite era el
  codificador de video.
- RIFE (`rife-ncnn-vulkan`, rife-v4.6) dio 15,5 a 33,4 cuadros/s en la RX 9070 XT, por lotes de
  PNG. No es apto para tiempo real.

Temple también resuelve detalles que lirtup puede copiar:

- `crates/temple-core/src/media/lsfg.rs` busca la DLL: ajuste de la persona, después las carpetas
  de Steam y sus bibliotecas (`libraryfolders.vdf`).
- `temple-lsfg --probe` hace un sondeo de un solo JSON para decidir si LSFG está disponible.
- Temple pone `DISABLE_LSFGVK=1` a sus procesos para que la capa implícita no se cargue dos veces.
  lirtup va a necesitar lo mismo si integra LSFG por dentro.

### Esta máquina (verificado)

- `lsfg-vk 2.0.0` instalado como capa implícita
  (`/usr/share/vulkan/implicit_layer.d/VkLayer_LSFGVK_frame_generation.json`), configurado en
  `~/.config/lsfg-vk/conf.toml` con `dll = "/home/octavio/Lossless.dll"` y perfiles por
  `active_in`.
- También está `Lossless Scaling/Lossless.dll` en la biblioteca de Steam.
- Hay otra capa implícita, `afmf-linux.json`, sin evaluar.

## Cómo es hoy el pipeline de lirtup (verificado)

```
PipelineThread._process_one_frame()               (src/upscaler/pipeline/pipeline.py)
  wait_for_last_present (fence)
  grab()  -> XShmGetImage + XDamage + hash de tiles  (src/upscaler/capture/lib)
  idle? -> presenter.present_unchanged()            (mismo screen_tex_8bit otra vez)
  subir a GPU -> upscale (modelo elegido)
  presenter.present():                              (src/upscaler/pipeline/presenter.py)
    deband -> efectos Magpie -> escalado -> CAS/bloom/viñeta
    -> delinearize/dither -> LUT -> grano -> OSD -> swapchain.present(screen_tex_8bit)
```

Hechos que importan para FG:

1. **Un solo hilo** captura, procesa y presenta. El ritmo lo dan la captura y `max_fps`, no el
   monitor.
2. **Ya se sabe si el cuadro cambió**: `grab()` devuelve `is_dirty` (XDamage más hash). Pero un
   cuadro "sucio" no es lo mismo que un cuadro nuevo del juego. Si lirtup captura más rápido que el
   juego, ve el mismo cuadro varias veces. Si captura más lento, se saltea cuadros.
3. Swapchain propio (`SwapchainManager`, superficie XCB), modos `fifo`, `mailbox` e `immediate`.
4. Los cuadros viajan como BGRA en texturas R8G8B8A8, con las mismas reglas que los efectos
   Magpie. LSFG trabaja con RGBA8: como el flujo óptico no depende del orden de canales, basta con
   que entrada y salida usen el mismo orden.

## Caminos posibles

### Fase 0: capa lsfg-vk sobre lirtup (sin código propio de FG)

lirtup es una app Vulkan que presenta en su propia ventana, así que la capa de lsfg-vk le genera
cuadros igual que a un juego. Es el camino equivalente a Lossless Scaling en Windows: capturar
una ventana y generar cuadros sobre lo capturado. Además sirve para juegos que la capa no puede
tocar directamente, como OpenGL, X11 nativo, emuladores o video.

Qué hace falta:

- Un perfil en `conf.toml` con `active_in` igual al nombre del proceso de lirtup. Con pipx
  probablemente sea el intérprete de Python; hay que comprobarlo.
- **Presentar sólo cuadros nuevos.** Hoy `present_unchanged()` presenta el mismo cuadro en cada
  vuelta, y la capa interpolaría entre cuadros iguales: con ×2 se ve el mismo movimiento con
  duplicados. Hay dos salidas:
  - Mínima: `max_fps` igual a los fps del juego. Frágil si el juego varía.
  - Mejor: un ajuste nuevo, "presentar sólo al cambiar", que saltea `present_unchanged`. Es un
    cambio chico en `_process_one_frame`.
- Ponerle a la superposición `vulkan_present_mode` compatible con el `pacing_mode` de la capa.

Riesgos sin probar:

- Que la capa funcione con la ventana de superposición transparente de lirtup.
- Que el OSD de lirtup se interpole y deje estelas (la capa ve el cuadro final, con OSD).
- Que capturas desparejas (XShm) den saltos que la interpolación no esconde.

Costo: horas. Si funciona bien, la fase 1 puede esperar y la integración en la GUI se limita a un
interruptor que escriba el perfil de lsfg-vk y ajuste el modo de presentación.

### Fase 1: LSFG integrado en lirtup (recomendado si la fase 0 se queda corta)

Integrar lsfg-vk dentro del pipeline, como hace Temple, pero sin pipes:

1. **Módulo nativo** `src/upscaler/framegen/` con lo de `temple-lsfg`: lsfg-vk (v3.1 y 3.1P),
   `shader_loader`, pe-parse y volk, compilado como extensión Python igual que
   `vulkan/` y `capture/`. Expone algo como
   `FrameGen(width, height, multiplier, flow_scale, performance, dll)` con `push(tex)` y
   `generated(i)`.
2. **Compartir imágenes por fd** (`createContext` original, no el local de Temple). Las texturas
   de entrada y salida se crean exportables en el dispositivo de lirtup
   (`VK_KHR_external_memory_fd`, `VK_KHR_external_semaphore_fd`) y se le pasan a LSFG. Así no hay
   copias por la CPU. Toca `src/upscaler/vulkan/lib/vk_compute.cpp`, que ya es código nuestro, y
   exige que ambos dispositivos sean la misma GPU: se compara `deviceUUID`.
   El respaldo es el staging de Temple: copiar cuadro por cuadro a través de memoria del host.
   Funciona en cualquier GPU, pero son unos 15 MB por cuadro en 1440p (estimado).
3. **Detector de cadencia.** Marcar como "cuadro real" sólo un cuadro de juego nuevo, con
   `is_dirty` más hash completo, y guardar su marca de tiempo. Con eso se estiman los fps del
   juego y el modo automático elige el multiplicador para llegar al refresco del monitor, como
   el modo automático de Temple v0.7.0 y `FgPolicy.autoMultiplier` de Enigmata.
4. **Hilo de presentación** separado del de captura: una cola de cuadros con marca de tiempo
   (reales y generados), presentados a intervalos parejos con `mailbox` o `fifo`. Es el cambio
   más profundo, porque hoy `present()` corre en el mismo hilo que `grab()`.
5. **Dónde va FG en la cadena:** después de los efectos y del escalado, **antes del OSD**, sobre
   `screen_tex_8bit` a resolución de salida, con `flow_scale` menor que 1 en 1440p o 4K. Hacerlo
   antes del upscale sería más barato para LSFG, pero obligaría a escalar N veces por cuadro real.
6. **Config, GUI y CLI**, como con `effects_engine`:
   - Campos `frame_generation` (off, ×2..×5, auto), `fg_flow_scale`, `fg_performance` y
     `fg_dll_path`.
   - Una pestaña con pros y contras (latencia) y el estado del sondeo.
   - La DLL se busca como en Temple, más `dll` de `~/.config/lsfg-vk/conf.toml`.
7. Poner `DISABLE_LSFGVK=1` en el propio proceso para que la capa no se sume a nuestro FG.

| Parte | Esfuerzo (estimado) | Se reusa de |
| --- | --- | --- |
| Módulo nativo y build | 2–3 días | `temple-lsfg` (CMake, vendorizado) |
| Interop por fd | 2–4 días | lsfg-vk `createContext`; staging de Temple como respaldo |
| Detector de cadencia y auto | 2–3 días | lógica de Temple y Enigmata |
| Hilo de presentación y pacing | 3–5 días | nuevo |
| Config, GUI, CLI, traducciones, docs | 2 días | patrón de `effects_engine` |
| Pruebas en GPU real | 2 días | nuevo |

### Alternativa: `temple-lsfg` como proceso aparte

Leer el cuadro final a la CPU, escribirlo en el stdin de `temple-lsfg`, leer los intermedios y
volver a subirlos. Sirve como prototipo de un día para ver la calidad. Para uso real no: en
1440p ×2 a 60 fps son unos 0,9 GB/s de entrada y 1,8 GB/s de salida por pipes (estimado), con
dos copias GPU↔CPU más por cuadro y la latencia de las colas.

### Descartados

- **RIFE / ncnn**: 15 a 33 cuadros/s en la RX 9070 XT medidos en Temple, por lotes. Lejos de tiempo
  real.
- **FSR 3 Frame Generation**: está pensado para integrarse en el motor del juego, con vectores de
  movimiento y profundidad que una captura no tiene.
- **`minterpolate` / `frc_amf`**: CPU o sólo AMF en Windows. No aplican.
- **Flujo óptico propio**: es investigación, no integración. LSFG ya resuelve calidad y velocidad.

## Riesgos y preguntas abiertas

- **Latencia**: más un intervalo del juego, más captura y procesamiento. Hay que avisarlo en la
  UI y desaconsejarlo debajo de unos 40 fps reales.
- **Captura despareja**: XShm por sondeo no está sincronizada con el juego. Si se captura el
  mismo cuadro dos veces, o se pierde uno, el pacing de los generados sale mal. El detector de
  cadencia es lo que más pesa en la calidad percibida.
- **Dependencia de la DLL**: una versión nueva de Lossless Scaling puede cambiar los recursos
  (`missing_spirv` en Temple). Temple y lsfg-vk tienen el mismo riesgo.
- **Licencias**:
  - lirtup-forge es GPL-3.0, así que el `shader_loader` GPL-3.0 entra sin cambiar de licencia.
  - lsfg-vk, volk y pe-parse son MIT; las cabeceras de Vulkan, Apache-2.0.
  - `Lossless.dll` no se distribuye: la pone la persona.
- **GPU sin probar**: Temple sólo se probó en AMD (RADV). Quedan NVIDIA, Intel y la ausencia de
  FP16.
- **Wayland**: lirtup presenta en XCB/XWayland. Eso no cambia con FG.

## Fase 0: resultados (2026-10-31)

Prueba: mpv con un video de 30 fps (960×540, X11/XWayland) como ventana de origen; lirtup en
`--overlay-mode windowed --vulkan-present-mode mailbox` con configuración vacía; MangoHud registra
los fps de presentación de lirtup durante 8 s. Una corrida por caso, RX 9070 XT, RADV.

| Caso | Fps presentados (media) | Resultado |
| --- | --- | --- |
| lirtup sin cambios | 892 | Presenta el mismo cuadro en cada vuelta: la capa interpolaría entre cuadros iguales |
| `--present-on-change-only` (nuevo) | 33,3 | Presenta sólo los cuadros nuevos del juego (≈30), lo que necesita la capa |
| `--present-on-change-only` + lsfg-vk ×2 (DLL vieja) | — | La capa no arranca y lirtup se cae (ver abajo) |
| `--present-on-change-only` + lsfg-vk ×2 (rama `lsfg-vk`) | 33,3 | La capa arranca y arma su pipeline (2560×1440, flujo 1,0); sin errores en 14 s |
| `--present-on-change-only` + lsfg-vk ×4 (rama `lsfg-vk`) | 33,3 | Igual, sin errores |
| lirtup sin cambios + lsfg-vk ×2 (rama `lsfg-vk`) | 13,9 | Con cuadros repetidos, lo que presenta lirtup cae de 892 a 13,9 de media |

Hallazgos (verificado):

1. **El ajuste nuevo funciona.** `present_on_change_only` (CLI `--present-on-change-only`) se
   saltea `present_unchanged()` y espera 1 ms entre capturas cuando no hay cuadro nuevo.
   **Corregido después:** esos 1 ms daban unas 530 capturas por segundo y casi 40 % de un
   núcleo. Ahora espera un periodo de refresco del monitor (7 ms a 144 Hz): con el video de
   30 fps baja de 41 % a 14 % de CPU y sigue presentando 33,3 fps.
2. **lsfg-vk 2.0.0 no acepta `Lossless.dll`.**
   - El nombre de archivo del ajuste `dll` (o `LSFGVK_DLL_PATH`) se ignora: la capa siempre carga
     `lsfg-vk.dll` de esa carpeta. Con `dll = "/home/octavio/Lossless.dll"` busca
     `/home/octavio/lsfg-vk.dll` y falla con «The specified shader DLL does not exist».
   - Si se le pasa la `Lossless.dll` con ese nombre, falla con «Unable to find base shader
     'mipmaps' in DLL». Pasó con las tres copias, que tienen el mismo SHA-256 (`626b196d…`):
     `~/Lossless.dll`, `/media/octavio/Warehouse/Lossless.dll` y la de Steam.
   - La guía de instalación de lsfg-vk 2.0 pide pasar Lossless Scaling, en Steam, a la rama
     **`lsfg-vk`**. Esa rama trae la DLL que la capa espera.
   - Con esta instalación la capa tampoco funciona con vkcube, así que tampoco anda en los juegos
     que usan el `conf.toml` de la persona.
3. **Error de lirtup encontrado:** si `vkCreateSwapchainKHR` falla (acá porque la capa no arrancó),
   el proceso se cae con un abort o un segfault en `vk_Swapchain_dealloc`, llamado desde
   `vk_Device_create_swapchain_impl` (`src/upscaler/vulkan/lib`). Debería lanzar `SwapchainError`.
   **Arreglado** (`038cb5df`): la superficie se destruía dos veces, en
   `vk_Device_create_swapchain_impl` y en `vk_Swapchain_dealloc`, y el handle se escribía en
   `self->swapchain` aunque la creación fallara. Ahora se crea en una variable local y sólo
   `dealloc` libera. Con la DLL vieja, lirtup termina con «Failed to create swapchain (error -3)»
   y código 1, en vez de un segfault.
4. **El proceso de lirtup se llama `python3.14`** (venv) o como sea el intérprete de pipx. Para la
   capa, un perfil con `active_in = "python3.14"` alcanza, pero toma cualquier script de Python que
   use Vulkan. Por eso es mejor activar la capa con `LSFGVK_ENV=1` y `LSFGVK_MULTIPLIER` sólo en el
   proceso de lirtup.

5. **Con la rama `lsfg-vk` de Lossless Scaling en Steam** aparece
   `steamapps/common/Lossless Scaling/lsfg-vk.dll` (2,1 MB). Con ese archivo la capa carga los
   shaders y genera sobre el swapchain de lirtup sin errores, activada sólo para lirtup con
   `LSFGVK_ENV=1 LSFGVK_MULTIPLIER=N LSFGVK_DLL_PATH=<…/lsfg-vk.dll>`.
   - La `Lossless.dll` clásica sigue sirviendo para el motor de Temple: `temple-lsfg --probe` dio
     `ok` con ella. La fase 1 no depende de esa rama.
   - **Sin medir:** los fps que llegan a la pantalla. MangoHud queda por encima de la capa y sólo
     ve lo que presenta lirtup (33,3 en todos los casos con `--present-on-change-only`). Hace
     falta mirarlo a ojo o con un contador que quede por debajo de lsfg-vk.
   - **Visto a ojo por la persona** (2026-10-31, monitor de 144 Hz): con el video de 30 fps,
     ×2 y ×4 se ven «mucho más fluidos» que mpv detrás; ×4 más, porque 30 × 4 = 120 queda cerca
     del refresco del monitor.
   - Sin probar: estelas del OSD, latencia, un juego real y escenas difíciles (bordes de
     interfaz, cortes de escena).

## Integración de la fase 0 (2026-10-31)

- `src/upscaler/framegen.py` prepara lsfg-vk **antes de la primera llamada a Vulkan**: la capa elige
  su perfil una sola vez, al cargarse. Comprobado: un perfil escrito después de crear la instancia
  no se activa; una vez activa, la capa recarga el archivo cuando cambia, pero no reconstruye su
  pipeline durante la sesión. Por eso activar o cambiar la generación pide reiniciar.
- Escribe un perfil propio en `~/.config/linux-rt-upscaler/lsfg-vk.toml` (`active_in` = nombre
  de `/proc/self/exe`) y pone `LSFGVK_CONFIG`. El `conf.toml` de la persona no se toca. Un archivo
  sin perfiles es inválido para lsfg-vk 2.0 («Invalid profiles section»), así que en apagado no se
  escribe nada y se pone `DISABLE_LSFGVK=1`.
- Si la persona ya puso `LSFGVK_CONFIG`, `LSFGVK_ENV` o `DISABLE_LSFGVK`, no se toca nada.
  Estado `EXTERNAL`.
- `lsfg-vk.dll` se busca en las bibliotecas de Steam, siguiendo `libraryfolders.vdf`, o en la
  carpeta de `frame_gen_dll`.
- Con la generación activa, el pipeline presenta sólo cuadros nuevos aunque
  `present_on_change_only` esté apagado.
- Se configura con `frame_gen_multiplier` (1 a 5), `frame_gen_flow_scale`, `frame_gen_performance`
  y `frame_gen_dll`. Son globales: los perfiles no los cambian, porque se leen antes de elegir
  ventana. En la CLI: `--frame-gen N`, `--frame-gen-flow-scale`, `--frame-gen-performance` y
  `--frame-gen-dll`. En la GUI: pestaña Presentación, sección «Generación de cuadros», con su
  estado (activo, falta reiniciar, falta lsfg-vk o la DLL), pros y contras, y traducción al
  español.
- Probado:
  - CLI con ×2: la capa toma el perfil «LirtUp Forge» y presenta 33,3 fps en mailbox con el video
    de 30 fps. Sin la opción eran 892.
  - CLI con ×1: la capa no se carga.
  - Con `LSFGVK_ENV` puesto por la persona: se respeta su ×3.
  - Pestaña de la GUI renderizada sin ventana en inglés y español, en tres estados.
- **Probado por la persona en la GUI real** (2026-10-31, instalación con pipx): ×4 con la novela
  visual G-senjou no Maou funcionó bien en principio. Es una impresión, no una medición.
- Sin probar: latencia medida, juegos con mucho movimiento, estelas del OSD, otras GPU.

## Fase 1: implementación (2026-11-01)

Escrita, compilada y probada con video (ver [resultados](#fase-1-resultados-2026-11-01)). Es el motor «integrado»
(`frame_gen_engine: native`), al lado de la capa, que sigue siendo el predeterminado.

Qué se hizo, comparado con el plan de arriba:

| Pieza del plan | Qué quedó |
| --- | --- |
| 1. Módulo nativo | `src/upscaler/lsfg/`: lsfg-vk (3.1 y 3.1P), pe-parse, volk y `shader_loader` copiados de Enigmata Temple, compilados como la extensión `upscaler.lsfg.lsfg` desde `setup.py` (sin CMake). volk se compila como C++ (`lib/volk_unit.cpp`) con visibilidad oculta, así sus punteros no chocan con `libvulkan` que usa `upscaler.vulkan`. Única modificación a lsfg-vk: `exception.cpp` sin `std::format` (GCC 13). |
| 2. Compartir por fd | Hecho, sin el respaldo por staging. `vk_external.cpp` en `upscaler.vulkan`: el dispositivo activa `VK_KHR_external_memory_fd` y `VK_KHR_external_semaphore_fd` si están; `Texture2D.external` crea texturas exportables (asignación dedicada, primer tipo `DEVICE_LOCAL`, igual que importa lsfg); semáforos binarios exportables; `copy_texture_signal` y `wait_semaphore`. lsfg usa `createContext` original (por fd), no el `createContextLocal` de Temple. La GPU se elige por `vendorID << 32 \| deviceID` del dispositivo de lirtup. |
| 3. Detector de cadencia | No hace falta uno aparte: con FG activa el pipeline ya presenta sólo cuadros nuevos (`present_on_change_only`), y el intervalo entre presentaciones es el del juego. Se estima con los últimos 30 intervalos: tiempo total sobre cuadros del juego, contando como varios los intervalos que perdieron cuadros (la mediana y la media sesgaban, ver resultados). Sin modo automático de multiplicador todavía. |
| 4. Hilo de presentación | No: el mismo hilo presenta los generados y después el real, repartidos en el intervalo con `sleep`, y cada ciclo se encadena al real anterior (si eso no obliga a esperar más de medio paso). Alcanza con fuentes de 30 fps; con 60 fps y ×2 se pierden cuadros (ver resultados): ahí hace falta el hilo aparte. |
| 5. Dónde va en la cadena | Sobre `screen_tex_8bit`, **después** del OSD y del panel de stats (no antes, como proponía el plan): se interpolan también. |
| 6. Config, GUI, CLI | `frame_gen_engine` (`--frame-gen-engine`), selector «Motor» en la GUI, estados propios (falta el motor, falta `Lossless.dll`), traducción. La bandeja y el panel de stats lo conocen. |
| 7. `DISABLE_LSFGVK=1` | Sí, el motor integrado deja la capa apagada. |

Cada cuadro nuevo del juego (`pipeline/native_framegen.py`):

1. `copy_texture_signal`: copia `screen_tex_8bit` a la entrada `k % 2` y señala el semáforo de
   entrada.
2. `lsfg.present(contexto, fd del semáforo de entrada, fds de los de salida)`. Los fds se exportan
   de nuevo en cada cuadro (lsfg se queda con ellos); con fds opacos el semáforo es el mismo.
3. Espera los semáforos de salida (todos, aunque el primer cuadro del contexto no se muestre,
   para que vuelvan a quedar sin señalar), presenta los generados y después el real.

Cualquier excepción apaga la FG para la sesión y sigue presentando el cuadro real.

Riesgos que hay que mirar al probar:

- Que lsfg acepte la memoria importada: los usos de la imagen difieren entre los dos lados
  (lirtup agrega `TRANSFER_SRC/DST`), como en la capa de lsfg-vk.
- La transición de `UNDEFINED` de lsfg en el primer uso descarta el contenido: sólo afecta al
  primer cuadro, que no se muestra.
- Sin traspaso explícito de familia de colas (`VK_QUEUE_FAMILY_EXTERNAL`), igual que la capa de
  escritorio.
- El ritmo con `fifo`: cada `present` puede bloquear hasta el vblank y correr los siguientes.

## Fase 1: resultados (2026-11-01)

mpv con un video `testsrc2` de 1280×720 (X11/XWayland) como origen; lirtup desde el árbol de
trabajo en `--overlay-mode windowed --vulkan-present-mode mailbox`, salida 2560×1440; MangoHud
registra 8 s de presentaciones de lirtup, que con el motor integrado incluyen los cuadros
generados. RX 9070 XT, RADV. Una corrida por caso.

- **Arranca y genera**: lsfg crea su dispositivo en la misma GPU, shaders FP16 desde
  `Lossless.dll` de Steam, importa texturas y semáforos por fd. Sin errores de Vulkan.
- **Los cuadros son interpolados de verdad**: volcado a PNG del cuadro anterior, el generado y el
  real; en el generado los objetos que se mueven quedan a mitad de camino (con el halo típico de
  LSFG) y el reloj del video sale mezclado.
- **Ritmo** (video de 30 fps), después de tres correcciones:

| Multiplicador | Fps presentados | Mediana entre cuadros | Desvío |
| --- | --- | --- | --- |
| ×2 | 60,0 | 16,7 ms | 0,89 ms |
| ×3 | 89,9 | 11,1 ms | 0,19 ms |
| ×4 | 118,3 y 118,9 (dos corridas) | 8,4 ms | 1,14 y 0,27 ms |

  Correcciones que hicieron falta, en orden:
  1. Repartir desde el momento en que se nota el cuadro nuevo seguía el temblor del sondeo:
     ×4 alternaba 4,4 y 16,5 ms. Ahora cada ciclo se encadena al real anterior.
  2. Encadenar sin límite bloqueaba el hilo hasta el cuadro siguiente, que se perdía; eso
     estiraba el intervalo medido y se realimentaba (×4 cayó a 68,5 fps). El encadenado ahora
     espera como mucho medio paso.
  3. La media exponencial, la mediana y la media recortada sesgaban el intervalo, porque el
     sondeo lo redondea al refresco (144 Hz). Ahora es tiempo total sobre cuadros del juego.
- **Límite encontrado**: video de 60 fps con ×2 da 100,7 fps en vez de 120. Sin FG lirtup capta
  los 60 (`--present-on-change-only`, 60,0 fps). Con un solo hilo, el ciclo suma el render, la
  generación y un paso de espera (8,3 ms) y pasa de 16,7 ms. Arreglo: el hilo de presentación
  aparte (punto 4 del plan), que necesita sincronizar el acceso a la `VkQueue` entre hilos.
- **Con el juego** (G-senjou no Maou, 1284×743 bajo Wine, la cadena de la persona
  Anime4K_Upscale_Denoise_UL + Anime4K_Restore_Soft_UL + Lanczos, ×2, 85 s jugando):
  - Funciona de punta a punta, sin errores; la barra de menú se detecta y recorta.
  - Ritmo bajo: en los tramos con movimiento se presentaron unos 25 fps (mediana 33,5 ms entre
    presentaciones). Los efectos cuestan 34,5 ms de GPU y el render completo pasa de 70 ms, así
    que lirtup procesa unos 13 a 15 cuadros reales por segundo y ×2 los lleva a ~28. Con la capa
    no pasa: genera después de que lirtup presenta y no frena el bucle. El motor integrado comparte
    el hilo (y la GPU) con los efectos; el hilo de presentación aparte y un `flow_scale` menor son
    lo siguiente a probar.
  - La detección de la barra dio 21 filas en esta corrida y 23 en otra. Revisado después con
    cuadros reales: 21 era lo correcto. Debajo de la barra hay un marco hundido de 2 px (Win32
    client edge) alrededor de la imagen de 1280×720, y con una escena lisa arriba el detector
    tomaba 2 filas de la imagen como separador. Arreglado: separador gris de 1 o 2 filas, y el
    marco se recorta en los cuatro lados.
- **Sin probar todavía**: latencia medida, otras GPU.

## Próximos pasos propuestos

1. **Fase 0**: perfil de lsfg-vk para el proceso de lirtup más el ajuste "presentar sólo al
   cambiar". Probar con un juego a 60 fps en un monitor de 144 Hz, con ×2 y con auto. Anotar el
   resultado en este documento.
2. Según lo que salga:
   - Si alcanza: un interruptor en la GUI que gestione el perfil de lsfg-vk.
   - Si no: la fase 1, empezando por el detector de cadencia y el hilo de presentación. Esas dos
     piezas también mejoran lirtup sin FG.
3. Agregar un botón para medir en la GPU propia, igual que para los efectos Magpie.
