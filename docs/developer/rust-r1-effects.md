# Ronda 1, frente E (Efectos): `lirtup-effects` y `lirtup-magpiefx`

Estado: **terminada**. Rama `claude/rust-r1-effects-905ea3` (sale de `claude/rust-r0-contract-6aa817`).
Fecha: 2026-11-01. Contrato: [rust-r0-contract.md](rust-r0-contract.md).

`cargo check -p lirtup-effects -p lirtup-magpiefx` (y con `--tests`), rustc 1.99.1: **pasa, sin
avisos**. Como piden las reglas, no se corrió `cargo build`, `cargo test`, `clippy` ni la app: los
tests están escritos, pero nadie los corrió. `lirtup-gpu` se usa solo por su API pública. Como
sigue en `todo!()`, nada de lo que toca la GPU se ejecutó todavía.

## Qué quedó

Se borró el `#![allow(unused_variables)]` de la ronda 0 en los dos crates. Ya no queda ningún
`todo!()` en ellos.

### `lirtup-effects`

- `assets`: tabla de `include_bytes!` con los 23 `.spv` nativos y los 9 modelos CuNNy (`model.json`,
  `PassN.spv` y `PassN_tile.spv`), leídos desde `src/upscaler/…` (decisión 5). Los genera un
  script a partir del árbol de 1.x. `blue_noise.py` pasó una sola vez a
  `crates/lirtup-effects/data/blue_noise.bin` (64×64 RGBA8, 16 384 bytes); es el único dato nuevo.
- `srcnn::Srcnn`: carga el `model.json` y crea las texturas intermedias con sus nombres, con el
  formato de `depth`. Todas las pasadas usan grupos `ceil(w/8)×ceil(h/8)` y el mismo buffer de
  constantes `IIIIffff` que la 1.x. El modo tiles va en `Srcnn::new_tiled` (ver «Cambios de API»):
  arrays de capas, la última pasada escribe en la salida completa y lee el residual, y cada tile
  lleva un bloque `TileParams` de 32 bytes por push constant.
- `scalers::Scaler`: Copy, Catmull-Rom, Lanczos (fijo de radio 2 o adaptativo, según la escala o
  `kernel_radius`, y cambia de variante en `set_dst_rect`), FSR (EASU, como la 1.x) y NIS (con las
  dos tablas de coeficientes). Cada escalador hace solo lo que en la 1.x hacía `presenter._scale`:
  linealiza su entrada a RGBA16F (salvo NIS) y escribe la salida RGBA16F en `output_size` (el
  `screen_tex`). Para NIS, primero limpia la salida con `recorder.clear`.
- `effects::NativeEffect`: `from_mode` respeta el orden de la 1.x. CAS, bloom, viñeta, grano y LUT
  trabajan sobre la misma textura (la entrada también es la salida). Deband, LumaSharpen y CRT
  escriben una textura nueva, y Anime4K S son 4 pasadas a 2×. Los parámetros se recortan a los
  rangos de la 1.x. `set_params` acepta los nombres de campo de la config (`cas_strength`,
  `crt_gamma_input`, …) y `set_frame` anima deband y grano.
- `lut` (privado): los 12 presets de `presets.py` se generan en la CPU (32³ RGBA8), con el mismo
  truncado `int(v*255)`.
- `convert::Conversion`: Linearize, Delinearize y Dither (con el ruido azul y `1/255`).
- `osd`: `TextRenderer` con `ab_glyph` (varias líneas, kerning, caja redondeada con antialias, RGBA
  premultiplicado). `OverlayBlend` sube la imagen desde un staging en `Upload` al grabar, pero solo
  cuando cambió, y mezcla con `overlay_blend.spv`.
- `util` (público, nuevo): `CbWriter` (empaqueta como `struct.pack` y rellena a 16 bytes), atajos
  de creación (`constant_buffer*`, `texture*`, `sampler*`, `compute`, `groups`), `upload_texture`
  (staging y copia de todas las capas, con espera) y `spirv_entry_point`. MagpieFX usa este módulo.

### `lirtup-magpiefx`

- `parser`: puerto línea por línea de `parser.py`, con los mismos mensajes. `remove_comments`
  conserva las directivas y los saltos de línea de los comentarios `//`. Un `/*` sin cerrar es error
  en `parse_effect`.
- `expr`: tokenizador y parser recursivo con la gramática de muparser de `expr.py` (funciones
  `min`, `max`, `abs`, `floor`, `ceil`, `round`, `rint`, `sqrt`, `log2`, `ln`, `log`, `exp`, `sign`),
  más `evaluate_size` (redondea como `lround` y exige un resultado positivo).
- `formats`: la tabla de 20 formatos (`FormatDesc` público y, en privado, los tipos UAV y el formato
  SPIR-V).
- `codegen`: genera el mismo texto que `codegen.py`, byte por byte (macros, cbuffer, declaraciones,
  helpers, `MulAdd`, entradas PS y CS). `constant_buffer_layout` devuelve también los offsets.
- `swizzle`: `swap_rb_source()` (es el `SwapRB.hlsl` de 1.x) y `rewrite` (el swap plegado en el
  fuente preprocesado), con `SwapUnsupported`.
- `spec`: forma corta (`"Nombre:a=1,b=2"`, separa en el último `:` seguido de pares) y forma larga.
- `compiler`: `Dxc` y `Glslang` por subproceso, con los mismos argumentos que `_command`, en una
  carpeta temporal. Detectan si el compilador murió por una señal. También tienen `preprocess`
  (`-P -Fi` en dxc, `-E` en glslang). `detect_compilers` lee `UPSCALER_MAGPIEFX_COMPILER`, `DXC` y
  `GLSLANG`. `SpirvCache` prueba los compiladores en orden y escribe la caché de forma atómica.
  Cuando falta glslang, el error lo dice antes que el log de dxc, como en la 1.x.
- `dds`: lector DDS (cabecera vieja con máscaras o FourCC D3DFMT, y cabecera DX10), rechaza BC,
  cubos, volúmenes y arrays. Además: `load_image` (PNG, BMP, JPG y TIFF con `image`),
  `source_format`, `resolve_source_path` (no deja salir de la carpeta del efecto), `to_rgba_floats`
  y `f16_to_f32`.
- `catalog`: `builtin_dir()` (`$LIRTUP_EFFECTS_DIR`, si no `src/upscaler/magpiefx/effects` del repo
  y si no `/usr/share/lirtup/effects`). `Catalog::scan` busca recursivamente los `.hlsl`, con
  nombres relativos separados por `/`; los efectos del usuario tapan a los incluidos y no se marcan
  como verificados. `find` busca por nombre, ruta o nombre de archivo, con prioridad para el
  usuario, como `find_effect`. `verified_names()` sale de la tabla de `magpiefx-report.md`
  (`include_str!`): son 86, los que tienen `ok` en la columna `run`.
- `runtime`: `compile_effect` lee el archivo y los `//!SOURCE`, y compila cada pasada con `__M`.
  Con FP16 pedido y declarado, prueba 16 bits y, si falla, recompila en FP32 con un aviso. El swap
  R/B se pliega donde el efecto lo permite y se prueban los mismos intentos que la 1.x.
  `fp16_wanted` lee `UPSCALER_MAGPIEFX_FP16` y `shaderFloat16`. `MagpieEffect` crea las texturas
  (expresiones con `INPUT_*`/`OUTPUT_*`), los samplers y un pipeline por pasada, con los grupos
  según `BLOCK_SIZE` sobre la primera salida, y empaqueta el cbuffer igual que `_pack_constants`.

### Tests escritos (sin correr)

- **Parser**: un efecto completo (parámetros float e int, OUTPUT con tamaño, SAMPLER, COMMON,
  pasadas desordenadas, PS y CS), comentarios, CRLF e `#include`, avisos, y los errores de cabecera,
  parámetros, texturas, samplers, identificadores duplicados y pasadas. También verifica que los 117
  efectos incluidos se parsean. Además hay tests de `expr`, `spec`, `swizzle`, `formats` y del
  catálogo (117 efectos, 86 verificados, efectos de usuario que tapan a los incluidos).
- **`.dds`**: el puerto de `test_magpiefx_dds.py` (constructor de DDS sintéticos, FP16 por FourCC
  113, máscaras RGBA/BGRA, luminancia, bump, DX10 R16, R32, RG32, BGRA y R10G10B10A2, solo el mip
  superior, todos los rechazos, `source_format`, `load_image` con DDS y PNG, rutas de `//!SOURCE` y
  los 17 `.dds` incluidos).
- **FP16**: el puerto de `test_magpiefx_fp16.py` (macros `MF`, el fuente FP16 es igual al FP32 salvo
  `min16float`, banderas de dxc y glslang, y elección y respaldo en `compile_source` con un
  compilador falso que implementa `HlslCompiler`), la política de `UPSCALER_MAGPIEFX_FP16`, y
  `f16_to_f32` (subnormales, infinito, NaN y −0).
- **Otros**: la caché (aciertos, claves distintas por fuente o banderas, respaldo al siguiente
  compilador, transformaciones), el swap plegado o descartado y los `//!SOURCE` leídos al compilar.
  En effects: `CbWriter`, los tamaños de cbuffer frente a los `struct` de la 1.x, la variante de
  Lanczos, los `model.json`, `TilePlacement`, los LUT, el orden de `from_mode`, el OSD y el entry
  point de cada `.spv`.

## Cambios de API respecto del contrato

Lo que cambia (son pocos casos y están justificados):

1. **`LutPreset` gana 11 variantes** (`Warm`, `Cool`, `Split`, `Vivid`, `Pastel`, `Lofi`, `Bleach`,
   `Film`, `Noir`, `Sepia`, `Cyano`), además de `ALL`, `name()` y `from_name()`. El contrato solo
   tenía `Identity`, pero la 1.x aplica `lut_preset` con estos 12 presets. Sin ellos no hay paridad.
2. **`CompiledEffect` gana campos privados** (los `//!SOURCE` y los dos flags del swap), que se leen
   con `sources()`, `input_swapped()` y `output_swapped()`. Ya no se puede construir con un literal
   fuera del crate; se construye con `compile_effect*`.
3. **`Srcnn::new` con `tiles: Some(..)` devuelve `EffectError::Unsupported`.** El modo tiles necesita
   el residual y el tamaño completo de salida, y `SrcnnOptions` no los trae. Para eso está
   `Srcnn::new_tiled(device, input_array, residual, output_size, &TileLayout, &SrcnnOptions)`, junto
   con `TilePlacement` (`from_grid`) y `set_tiles`. `set_active_tiles` recorta cuántas capas corren
   en un cuadro.
4. **`MagpieEffect::set_params` ignora los nombres desconocidos y recorta los valores** a `[min,
   max]`, con aviso. Sigue el doc de `Pass`; la 1.x lanzaba una excepción.
5. `generate_pass_source` toma `pass_index` desde 0, como dice el doc del contrato. La 1.x contaba
   desde 1.

Lo que se agrega sin cambiar las firmas: `HlslCompiler::preprocess` y `cache_key` (métodos con
implementación por defecto), `util` completo, `Scaler::{linear_output, set_nis_sharpness,
set_params, kind}`, `ScalerKind::linear`, `NativeEffect::{in_place, label, set_param}`,
`CrtParams: Default`, `assets::native_paths`, `CatalogEntry::{from_path, stem}`,
`Catalog::{scan_dirs, resolve}`, `catalog::EFFECTS_DIR_ENV`, `Dxc::args`, `Glslang::args`,
`compiler::Transform`, `SpirvCache::compile_with`, `runtime::{CompileOptions,
compile_effect_with, compile_swap_rb, compile_source}`, `MagpieEffect::{desc, param_value}`,
`dds::{to_rgba_floats, f16_to_f32, resolve_source_path, DDPF_*, IMAGE_EXTENSIONS}`,
`expr::evaluate_size`, `swizzle::{rewrite, SwapUnsupported, SWAP_FUNC}`,
`parser::MAGPIE_FX_VERSION`, `ParamKind::hlsl`, `EffectDesc::output` y `codegen::mf_type_macros`.

## Dependencias

No pido ninguna dependencia nueva al workspace. En `crates/lirtup-magpiefx/Cargo.toml` agregué
`tempfile.workspace = true` (ya está en el workspace), para la carpeta temporal del compilador y
para los tests. `cargo check` agrega por eso la línea `"tempfile"` a las dependencias de
`lirtup-magpiefx` en `Cargo.lock`. No la commiteé porque el lock es del revisor; se regenera sola en
el primer build. La clave de la caché usa dos `xxh64` con semillas distintas, así que no hace falta
la feature `xxh3`.

## Diferencias con la 1.x (a propósito)

- **Texturas de `//!SOURCE`**: los texels se copian tal como vienen a una textura de su formato
  (`util::upload_texture`). La 1.x los expandía a floats y los escribía con una pasada de cómputo
  que también se compilaba con dxc, porque su subida solo manejaba texels de 4 bytes. Los DDS en
  BGRA se reordenan a RGBA al leerlos, porque `DdsImage` no tiene `swap_rb`.
- **Caché**: la clave usa xxh64 y la versión `rs1`, así que los archivos no coinciden con los de la
  caché SHA-256 de Python. Pueden convivir en la misma carpeta (la elige quien llama, con
  `lirtup_config::paths::magpiefx_cache_dir`).
- **Escalador**: incluye la linealización (en la 1.x era una pasada aparte del presentador) y usa
  `recorder.clear` en lugar de `clear.spv`.

## Paridad que conservé aunque parece un error de la 1.x

- **Catmull-Rom** corre siempre con `blur = 1.0` y `antiring = 0.8`, porque `presenter._scale`
  nunca le pasa los de la config. Para cambiarlo basta tocar una línea de
  `scalers::scaler_constants`.
- **FSR** es solo EASU, como la 1.x, aunque el doc de `ScalerKind` y `Upsampler` dice «EASU + RCAS».
- **`header_code`** (el `#include` de la cabecera) se parsea pero no se emite, igual que en
  `codegen.py`.
- **`SrcnnOptions::fp16`** no tiene efecto: en el árbol no hay variantes FP16 de CuNNy, y los
  shaders usan `min16float` con precisión relajada en `cs_6_0`.

## Pendiente

- **Correr los tests y los criterios de 5.8** cuando haya build (revisor): que los 117 efectos
  compilen con dxc (los 3 Denoise con glslang) y que la salida sea igual a la de Python en la imagen
  RGB de prueba. Las dos cosas necesitan una GPU real y el frente G terminado. Propongo un ejemplo
  `lirtup-magpiefx/examples/compile_all.rs` y un test `#[ignore]` que compare con PNG de referencia
  generados por `scripts/magpiefx-bench.py`.
- **Lo que este frente supone de `lirtup-gpu`** (para revisar al integrar con G):
  - `copy_buffer_to_texture` con cualquier formato de la tabla (RGBA16F, R8, R10G10B10A2, …), con
    `src_offset`, `src_row_length` y `dst_slice`;
  - un `Buffer` en `Upload` que se pueda ligar como buffer de constantes;
  - la misma textura ligada como SRV y UAV en un pipeline (efectos sobre la misma textura y OSD:
    layout `GENERAL`);
  - vista `Texture2DArray` con `force_array_view` y `slices > 1` (LUT y tiles);
  - `recorder.clear` sobre una textura de almacenamiento RGBA16F;
  - un entry point distinto de `main` (`__M` en MagpieFX; se lee del SPIR-V con
    `util::spirv_entry_point`).
- **Carreras de CPU y GPU, iguales a las de la 1.x**: `set_params`, `set_frame`, `set_dst_rect` y
  `OverlayBlend::set_image` escriben buffers `Upload` mientras el cuadro anterior puede seguir en
  vuelo. En la ronda 2 (frente P) conviene un anillo por cuadro o push constants.
- El presentador (frente P) decide dónde corre cada `NativeEffect` y encadena `Scaler` →
  `Delinearize` o `Dither` según `linear_output()`. Para el swap R/B de la cadena MagpieFX tiene dos
  caminos: `compile_effect_with` con `swap_input`/`swap_output` más `compile_swap_rb` para el lado
  que no se pudo plegar, o una textura de captura `B8G8R8A8_UNORM`.
