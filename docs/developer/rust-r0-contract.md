# Ronda 0 del refactor a Rust: el contrato

Estado: **terminada**. Rama `claude/rust-r0-contract-6aa817` (sale de
`claude/rust-refactor-analysis-bb2c50`). Fecha: 2026-11-01. Plan completo en
[rust-refactor-analysis.md](rust-refactor-analysis.md) (secciones 3, 5 y 6).

`cargo check --workspace` (rustc 1.99.1): **pasa, sin avisos**. Es el único build de la ronda; no
se corrió `cargo build`, `cargo test` ni `clippy`.

## Qué quedó

- Workspace de Cargo en la raíz del repositorio (`Cargo.toml`, `Cargo.lock`, `crates/`), al lado
  de la app de Python, que no se toca. Resolver 3, edición 2024, `rust-version = "1.99"`,
  licencia GPL-3.0-or-later, versión `2.0.0-alpha.0`.
- 12 crates, con su API pública escrita como firmas documentadas (cuerpos en `todo!()`), unas
  6 000 líneas:

  | Crate | Frente | Qué es |
  | --- | --- | --- |
  | `lirtup-config` | C (r1) | `Config`, `ModeSettings`, perfiles, `Match`, modos, `gui-config.yaml`, atajos, CLI, capas |
  | `lirtup-gpu` | G (r1) | `ash`: `Instance`, `Device`, recursos, compute, grabación, externos, swapchain |
  | `lirtup-x11` | X (r1) | `x11rb`: ventanas, captura, tiles, atajos, superposición, reenvío, miniaturas |
  | `lirtup-effects` | E (r1) | trait `Pass`, SRCNN, escaladores, efectos nativos, conversores, OSD |
  | `lirtup-magpiefx` | E (r1) | parser, expresiones, codegen, compilador, `.dds`, catálogo, ejecución |
  | `lsfg-sys` | C (r1) | `include/lsfg_c.h`, declaraciones `extern "C"` y `build.rs` vacío |
  | `lirtup-fg` | C (r1) | capa lsfg-vk, búsqueda de DLL, motor integrado seguro, multiplicador automático |
  | `lirtup-proto` | revisor | el contrato de 6.5: `Command`, `Event` y sus tipos; codec ya implementado |
  | `lirtup-pipeline` | P, F, S (r2) | solo el `lib.rs` con los 11 módulos de la ronda 2 declarados |
  | `lirtup-core` | K (r2) | `Engine`, `server::serve`, `daemon` (vacío); reexporta `proto` |
  | `lirtup-cli` | K (r2) | binario `lirtup` con `run`, `serve`, `ctl`, `list` (clap) |
  | `lirtup-ui` | revisor (r3) | binario `lirtup-gui`: un `App` mínimo de Dioxus para fijar la versión |

- `crates/lsfg-sys/include/lsfg_c.h` con la API C de 3.5 (con los cambios de abajo).
- Dependencias fijadas en `[workspace.dependencies]` (lista de 5.2 y las que los frentes van a
  necesitar), `rust-toolchain.toml` (1.99.1), `rustfmt.toml` (`max_width = 110`), perfiles y
  lints.

## Decisiones (lo que cambia respecto del análisis)

1. **`lirtup-proto` es un crate aparte**, y `lirtup-core` lo reexporta como `lirtup_core::proto`
   (el nombre del análisis sigue valiendo). Motivo: `StatsSample` lo llena el pipeline, que está
   *debajo* de `core`; sin un crate común habría un ciclo. Además, **la GUI depende de
   `lirtup-proto`, `lirtup-config` y `lirtup-x11`, no de `lirtup-core`**: el binario
   `lirtup-gui` no enlaza Vulkan, lsfg ni el pipeline. El cliente del socket (`CoreLink`) va en
   la GUI (`link.rs`, 5.5), como ya decía el análisis.
2. **`lirtup-x11` no depende de `lirtup-gpu`** (el análisis decía que X usa `Format` de G). La
   captura escribe bytes BGRA8 en un `&mut [u8]` que da quien llama; el pipeline le pasa la
   memoria mapeada de un `Buffer` de subida con `Buffer::write_with`, así no hay copia. La
   superposición entrega el `xcb_connection_t*` y la ventana; `lirtup_gpu::XcbTarget` los
   envuelve. Así X y G no se esperan en la ronda 1 y el orden de integración puede ser cualquiera.
3. **`lirtup-x11` depende de `lirtup-config`** (que está abajo de todo) por `Crop`, `OverlayMode`,
   `Hotkey`, `HotkeyAction` y `MonitorSelector`. **`lirtup-fg` tampoco depende de `lirtup-gpu`**:
   recibe `OwnedFd` y el `VkFormat` como `u32`.
4. **El trait `Pass` vive en `lirtup-effects`**, y `lirtup-magpiefx` depende de él (mismo
   frente E). `lirtup-effects` y `lirtup-magpiefx` dependen de `lirtup-config` (`Model`,
   `ModeSettings`, `MagpieEffectSpec`).
5. **Los datos no se copian.** Los `.spv`, los modelos CuNNy, los 117 efectos de Magpie y el C++
   vendorizado de lsfg se quedan en `src/upscaler/…`; `include_bytes!` y `build.rs` los leen de
   ahí. La tabla de 5.3 decía que E toca `shaders/**`: no hace falta crear esa carpeta. Se mueven
   en la ronda 4, cuando se retire Python, así no hay dos copias que se desfasen.
6. **Capas de configuración sobre `Mapping` crudo.** `resolve::resolve` junta valores por defecto
   ← nivel superior ← modo ← opciones del perfil ← CLI como `serde_yaml_ng::Mapping` y recién
   al final arma el `Config` tipado con `Config::from_mapping`, que es tolerante (un valor malo
   deja el valor por defecto y da un `ConfigWarning`, como los validadores de 1.x). Es lo único
   que deja saber qué claves fija cada capa (lo que la GUI necesita para «Hereda / Cambiado») y
   que los campos sueltos de modo ganen sobre el modo (6.6). `hotkeys` se mezcla clave por clave.
7. **`ModeSettings` va aplanado dentro de `Config`** (`#[serde(flatten)] pub scaling`): las
   claves YAML siguen siendo las de 1.x (`model`, `cas_enabled`, …) y el mismo struct es el cuerpo
   de `modes/<id>.yaml`. `MODE_KEYS` lista esas claves. En código se lee `config.scaling.model`.
8. **El perfil nombra su modo en `options.mode`** (es un campo más de `Config`, `mode`), no en una
   clave nueva al lado de `match`. Así un perfil de 1.x y uno de 2.0 tienen la misma forma.
9. **Tipos de `Config`**: enums con serde para lo que 1.x valida contra una lista (`Model`,
   `Upsampler`, `OverlayMode`, `PresentMode`, `FrameGenEngine`, `LogLevel` con alias en
   minúscula, `FrameGenMultiplier` = `1..5` o `"auto"`); `MonitorSelector` y `GpuSelector`
   aceptan número o texto; `OutputGeometry` guarda el texto tal cual y `parse()` da la
   `Geometry`; `BackgroundColor` acepta texto CSS o 4 floats; `hotkeys` queda como
   `BTreeMap<String, String>` para no perder acciones desconocidas. Claves desconocidas en
   `extra` (en `Config`, `Profile`, `Match`, `Mode`, `GuiConfig`). `Config::default()` y
   `ModeSettings::default()` ya tienen los valores reales de 1.3.0 (no son `todo!()`).
10. **Dos acciones de atajo nuevas**, del diseño: `start_fullscreen` (`Alt+Shift+A`) y
    `start_windowed` (`Alt+Shift+Q`). Las agarra `lirtup serve` (6.0), así funcionan con la GUI
    cerrada. El escalado diferido usa `target_delay`.
11. **`lsfg_c.h` frente a 3.5**: orden de argumentos de `lsfg_initialize` como el puente de hoy
    (`dll, vendor, device, generated, flow_scale, performance, hdr, fp16`), con `fp16` agregado
    (el puente de 1.x lo tenía); `lsfg_initialize` devuelve un conjunto de bits
    (`LSFG_INIT_FP16`) o un error negativo; `lsfg_probe` distingue FP16 de solo FP32;
    `lsfg_delete_context` devuelve `int`; códigos `LSFG_ERR_*` definidos; **lsfg toma los fds si
    la llamada sale bien** (semántica de import de Vulkan), y si falla siguen siendo de quien
    llama.
12. **`LsfgEngine::initialize` devuelve `Arc<LsfgEngine>`** y cada `LsfgContext` guarda un `Arc`,
    para que el hilo de FG de la ronda 2 pueda tener el motor y el contexto juntos sin préstamos
    autorreferentes. lsfg se finaliza al soltar el último `Arc`.
13. **Protocolo**: JSON por líneas con `{"v":1,"type":"…",…}` (`Envelope` con `flatten`, enums
    con `#[serde(tag = "type")]`). Como el etiquetado interno no admite un newtype de `Option` o
    de `Vec`, `WindowPicked` y `Effects` son variantes con campos (`window`, `effects`). Además:
    `Command::Hotkey { action }` (la bandeja, la barra y `lirtup ctl` disparan las mismas acciones
    que el teclado), `DaemonState.keep_after_exit`, `MenuBarDetected.profile: Option<String>`
    (sin perfil = nivel superior) y `EngineSnapshot.last_error`. `FrameGenState` se redeclara en
    proto para que la GUI no enlace `lirtup-fg`. **`encode_line`, `decode_line` y
    `socket_path` ya están implementados** (son diez líneas y fijan el formato).
14. **Bandeja**: `tray-icon = 0.21.3` y `muda = 0.17.2`, las mismas que trae `dioxus-desktop`
    0.7.10, no las últimas (0.26 y 0.21): dos versiones de `muda` en el mismo proceso GTK no
    conviven bien. `ksni` queda declarado como alternativa (3.8).
15. **Dioxus fijado a `=0.7.10`** (la última 0.7; hay una 0.8 alfa). `dx` tiene que ser 0.7.10.
16. **`x11rb` con `dl-libxcb`**: `XCBConnection` carga `libxcb` en tiempo de ejecución, sin
    enlazarla en el build.
17. **Lints**: `unsafe_code = "deny"` en el workspace; `lirtup-gpu`, `lirtup-x11` y `lsfg-sys` lo
    permiten con `#![allow(unsafe_code)]` en su `lib.rs`. `missing_docs = "warn"`,
    `unsafe_op_in_unsafe_fn = "deny"`, clippy `todo`, `dbg_macro` y
    `undocumented_unsafe_blocks` en advertencia. Cada crate con firmas en `todo!()` tiene un
    `#![allow(unused_variables)]` marcado «Round 0 stubs»: **el frente lo borra** al llenar los
    cuerpos.
18. **Perfiles**: release con `lto = "thin"`, `codegen-units = 1`, `strip`; en dev las
    dependencias van con `opt-level = 2` (Vulkan, `image` y `ab_glyph` sin optimizar son muy
    lentos) y los crates propios en 0.
19. **`rust-toolchain.toml` en 1.99.1**, la versión instalada. En esta máquina no hay `rustup`
    (rust de Arch), así que el archivo solo sirve a quien use rustup.
20. **`lirtup-pipeline`**: `lib.rs` con los 11 módulos de la ronda 2 declarados (tabla con el
    dueño de cada uno) y archivos vacíos con su comentario. Las firmas entre módulos (pipeline ↔
    stats ↔ osd ↔ framegen) las escribo antes de la ronda 2, sobre lo que entregue la ronda 1.
    El daemon va en `lirtup-core::daemon` (como 5.6), no en el pipeline (como decía 3.1).

## Grafo de dependencias

```
lirtup-config ──────────────┬─────────────┬───────────────┬──────────┐
lirtup-gpu ───────┐         │             │               │          │
                  ├── lirtup-effects ── lirtup-magpiefx   │          │
lsfg-sys ── lirtup-fg       │             │               │          │
lirtup-x11 (config)         │             │               │          │
lirtup-proto (config)       │             │               │          │
        └──────── lirtup-pipeline (todos los de arriba) ──┘          │
                          lirtup-core (pipeline, proto, x11, fg) ── lirtup-cli
lirtup-ui (proto, config, x11)
```

## Reglas para la ronda 1

Las de 5.1, más:

- Cada frente toca **solo** sus crates. `Cargo.toml` del workspace, `Cargo.lock`,
  `rust-toolchain.toml`, `rustfmt.toml` y `lirtup-proto` son del revisor. Las dependencias nuevas
  se piden en la nota de rama (`docs/developer/rust-r1-<frente>.md`); el `Cargo.toml` del propio
  crate sí lo puede tocar el frente (agregar `x.workspace = true` de lo que ya está en el
  workspace).
- **Las firmas públicas de este contrato no cambian.** Se pueden agregar funciones, campos
  privados, módulos privados e `impl` nuevos. Si una firma no sirve, se anota en la nota de rama
  y se sigue con la que hay.
- Los structs con `_private: ()` (o `Arc<()>`) son opacos: el frente pone los campos que quiera,
  pero respeta `Send`/`Sync`/`Clone` según el doc del tipo. Ojo con `OverlayWindow` (guarda un
  puntero: necesita `unsafe impl Send` con su `// SAFETY:`) y con `CommandRecorder` y
  `Swapchain`, que **no** son `Sync` a propósito (`PhantomData<Cell<()>>`).
- `cargo check -p <crate>` sí; `cargo build`, `cargo test`, `clippy` y correr ejemplos no (los
  tests se escriben, no se corren). `cargo fmt -p <crate>` está bien.
- Comentarios y docs en inglés, como el código de hoy; la nota de rama en español.

## Firmas por frente (ronda 1)

### G. `lirtup-gpu`

Port de `src/upscaler/vulkan/lib/*.cpp` con la forma del módulo de Python.

- `Instance::new(&InstanceOptions) -> Result<Arc<Instance>>` (carga dinámica, `VK_KHR_surface`,
  `VK_KHR_xcb_surface`, debug utils opcional), `adapters() -> &[AdapterInfo]`, `best_adapter()`,
  `select_adapter(Option<&str>)` (índice o subcadena del nombre), `create_device(&Arc<Self>, usize)
  -> Result<Arc<Device>>`, `raw()`.
- `AdapterInfo`: nombre, PCI vendor/device, driver, versión de API, `AdapterKind`, memorias,
  `supports_fp16`, `supports_external_fd`, `robust_image_access`, `timestamp_period_ns`.
- `Device` (`Send + Sync`, el **mutex de la cola** adentro): `info`, `instance`, `raw`,
  `with_queue(|q| …)`, `queue_family`, `wait_idle`, `debug_messages`, `create_buffer`,
  `create_texture`, `create_external_texture`, `create_external_semaphore`, `create_sampler`,
  `create_compute`, `recorder`, `submit(CommandRecorder, &SubmitOptions) -> Result<Submission>`,
  `create_swapchain`.
- `Buffer` y `Texture` (`Clone`, con `Arc`): `BufferDesc { size, stride, format, location:
  MemoryLocation, label }`, `write`, `write_with(|&mut [u8]| …)`, `read`, `raw`;
  `TextureDesc { width, height, format, slices, force_array_view, usage: TextureUsage, label }`,
  `width/height/slices/format/desc/row_pitch`, `download`, `raw`, `raw_view`. `Sampler` con
  `SamplerDesc { address: [AddressMode; 3], min, mag }`.
- `Format`: enum con los números DXGI de 1.x; `to_vk`, `from_vk`, `from_dxgi`,
  `bytes_per_texel`.
- `ComputeDesc { spirv, entry, cbv, srv, uav: Vec<Binding>, samplers, push_size, bindless_max,
  label }` → `ComputePipeline` (`Clone`): `workgroup_size`, `push_size`, `label`. Los bindings
  siguen el orden de registros de dxc de hoy (b, t, u, s).
- `CommandRecorder` (`Send`, no `Sync`; inserta las barreras): `copy_buffer_to_texture`,
  `copy_texture`, `copy_texture_to_buffer`, `clear`, `dispatch(&Dispatch { pipeline, groups,
  push, timestamp })`, `prepare_present`, `len`. `SubmitOptions { timestamps, wait, signal }`
  (semáforos externos). `Submission::is_done`, `wait(timeout_ns) -> Result<Option<Timestamps>>`;
  `Timestamps { dispatches: Vec<(String, f64)>, total_ms }`.
- `ExternalTexture::texture`, `export_fd -> OwnedFd`; `ExternalSemaphore::export_fd`, `raw`.
- `XcbTarget::new` (unsafe), `SwapchainDesc { target, format, buffers, width, height,
  present_mode: PresentMode }`, `Swapchain` (`Send`): `width/height/format/present_mode`,
  `present(&Texture, &PresentOptions { offset, clear, wait, wait_fence }) -> PresentStatus`,
  `recreate`, `needs_recreation`.
- `GpuError` (`Loading`, `NoDevice`, `Unsupported`, `Vk { call, result }`, `Allocation`,
  `Timeout`, `DeviceLost`, `OutOfDate`, `Invalid`). Se reexporta `ash::vk`.
- Entregable de 5.3: ejemplo `examples/copy.rs` sin ventana.

### X. `lirtup-x11`

Port de `capture/lib/*.c`, `window/` y `overlay/`.

- `X11::connect()`, `root`, `is_xwayland`; en `window`: `list_windows`, `window_info`,
  `active_window`, `window_icon(xid, size) -> Option<RgbaImage>`, `activate_window`,
  `is_viewable`. `WindowInfo { xid, title, rect: Rect, pid, class: Option<WindowClass> }`
  (serde).
- `acquire`: `find_by_title(needle, regex)`, `wait_for_pid(&PidSearch)` (las dos fases de 1.x),
  `active_after(Duration)`.
- `monitor`: `monitors() -> Vec<MonitorInfo>` (RandR, con `refresh_hz`) y
  `monitor_area(&MonitorSelector) -> (Rect, Option<f32>)`. `pick::pick_window()` (cursor en
  cruz). `thumbs`: `enable_thumbnails`, `thumbnail(xid, max_width)`.
- `Capture::new(&CaptureOptions { window, crop, tile_size, damage_tracking })`, `size`,
  `buffer_len`, `grab(&mut [u8], &mut Vec<DamageRect>) -> Result<Frame>`; errores `Resized` y
  `NoWindow`. `DamageRect` es `#[repr(C)]` como el de C. Formato de píxel: `PIXEL_FORMAT`
  (BGRA8).
- `tiles`: `TileCache::new/reset/changed_tiles`, `expand_rects`, `dirty_fraction`.
- `Hotkeys::start(&[(HotkeyAction, Hotkey)]) -> (Hotkeys, Receiver<HotkeyAction>, Vec<X11Error>)`,
  `rebind`, `stop`; `keysym_for(name)`.
- `OverlayWindow::create(&OverlayOptions { mode, rect, title, target, opacity, click_through,
  bypass_compositor })`, `window`, `xcb_connection`, `rect`, `set_mode`, `set_opacity`,
  `set_input_region`, `set_cursor_visible`, `set_visible`, `move_to`, `poll_events`,
  `wait_event`; `OverlayEvent` (puntero, tecla, entrar/salir, `Configure`, `Expose`, `Close`).
- `CoordinateMapper { content, source_size, crop_offset }::to_target`;
  `InputForwarder::new(target)`, `set_mapper`, `forward(&OverlayEvent)`.
- Entregable de 5.3: ejemplo que captura una ventana a PNG (`image` ya está como dev-dependency).

### E. `lirtup-effects` y `lirtup-magpiefx`

- `Pass: Send` con `label`, `output -> &Texture`, `record(&mut CommandRecorder)`, y por defecto
  `set_params(&ParamValues)` y `set_frame(frame, time_s)`.
- `assets`: `native_spirv(path)`, `cunny_model(Model) -> CunnyAssets { json, passes,
  tile_passes }`, `blue_noise()` (pasar `blue_noise.py` a un `.bin` una vez).
- `srcnn::Srcnn::new(device, input, &SrcnnOptions { model, tiles: Option<TileLayout>, fp16 })`,
  `pass_count`, `set_active_tiles`; implementa `Pass`.
- `scalers::Scaler::new(device, ScalerKind, input, output_size, dst_rect, &ScalerParams)`,
  `set_dst_rect`, `set_background`; implementa `Pass`.
- `effects::NativeEffect` (Cas, Deband, Bloom, Vignette, Grain, Lut, LumaSharpen, Anime4kS,
  CrtEasymode(CrtParams)): `from_mode(&ModeSettings)`, `output_size`, `create(...) -> Box<dyn
  Pass>`. `convert::Conversion::{Linearize, Delinearize, Dither}::create`.
- `osd`: `TextRenderer::new(font)`, `render(text, &TextStyle) -> TextImage`; `OverlayBlend`
  (`Pass`) con `set_image`.
- MagpieFX: `parser::{parse_effect, remove_comments}` → `EffectDesc` (mismos campos que el
  dataclass), `expr::evaluate`, `codegen::{constant_buffer_layout, generate_pass_source}`,
  `compiler::{HlslCompiler, Dxc, Glslang, detect_compilers, SpirvCache, CompileRequest}`,
  `dds::{parse_dds, load_image, source_format}`, `formats::lookup`, `swizzle::swap_rb_source`,
  `spec::{parse_effect_spec, parse_effect_chain}`, `catalog::{Catalog::scan/find/describe,
  builtin_dir, verified_names}` y `runtime::{compile_effect, fp16_wanted, CompiledEffect,
  MagpieEffect}` (implementa `Pass`).
- Entregables de 5.8: los 117 efectos compilan con dxc (los 3 Denoise con glslang); misma salida
  que Python en la imagen RGB de prueba; tests de parser, `.dds` y FP16.

### C. `lirtup-config`, `lsfg-sys` y `lirtup-fg`

- `Config::from_mapping`, `to_diff_mapping`, `hotkey_bindings`, `validate`;
  `OutputGeometry::parse`; `BackgroundColor::{to_rgba, to_hex}`; `Hotkey: FromStr + Display`,
  `diff_hotkeys`.
- `Profile::{mode, daemon_excluded}`, `Match::{is_empty, matches(title, w, h)}`,
  `Interval::{parse, contains}`, `find_matching_profile`.
- `ConfigFile::{load, load_default, save}` (archivo temporal y `rename`, orden y claves
  desconocidas conservados), `ModeStore::{load, save, delete, new_id}`,
  `GuiConfig::{load, save}`, `resolve::{merge_layer, resolve(&ConfigFile, &ModeStore,
  &ProfileChoice, &cli) -> Resolved}`.
- `cli::ConfigArgs` (ya tiene `--config`, `--profile`, `--mode`, `--model`, `--geometry`,
  `--overlay-mode`, `--frame-gen`, `--set`): agregar una bandera por campo como `args.py`, en
  `OptionArgs`, y `overrides() -> Mapping` solo con lo que se pasó.
- `lsfg-sys`: `lsfg_c.cpp` (la API del header sobre `LSFG_3_1`/`LSFG_3_1P`, excepciones atrapadas,
  `lsfg_last_error` por hilo) y `build.rs` con `cc` con las fuentes y banderas de `setup.py`,
  leyendo `src/upscaler/lsfg/`.
- `lirtup-fg`: `dll::{steam_libraries, find_dll(DllKind, hint)}`, `layer::{find_layer,
  external_env, prepare(&Config) -> LayerSetup, restart_self}`, `native::{probe, LsfgEngine,
  LsfgContext, EngineOptions}`, `multiplier::{ideal_multiplier, AutoMultiplier}`,
  `FrameGenState`.
- Entregables de 5.8: lee `config-example.yaml` y los perfiles de la persona sin cambiarlos
  (test de ida y vuelta); `lsfg_probe` encuentra `Lossless.dll`.

## Pendientes y riesgos

- **El formato del socket no está probado** (no se corrieron tests): `Envelope` con `flatten`
  sobre un enum con etiqueta interna debería funcionar con serde, pero el frente K tiene que
  empezar la ronda 2 con un test de ida y vuelta de cada `Command` y `Event`.
- `Config` mezcla dos `flatten` (`scaling` y `extra`): el frente C confirma con el test de ida y
  vuelta que las claves de modo no terminan duplicadas en `extra`. Si serde no lo resuelve, la
  salida es que `extra` se llene a mano en `from_mapping` (sin `flatten`) y el resto del contrato
  no cambia.
- `config-example.yaml` usa `pipeline_idle_interval`, pero el campo de 1.x es
  `pipeline_poll_interval`: hoy esa clave se ignora. Queda igual (va a `extra`).
- Antes de la ronda 2: firmas entre los módulos de `lirtup-pipeline`. Antes de la ronda 3: el
  esqueleto de `lirtup-ui` (5.5).
