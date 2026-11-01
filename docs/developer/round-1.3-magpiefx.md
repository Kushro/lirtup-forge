# Ronda 1.3, frente A: MagpieFX (`//!SOURCE` y FP16)

Rama `claude/round13-magpiefx-4749b9` (el plan la llama `round13/magpiefx`),
desde `main` en `8b67acd6`. Sin push.

## Lo hecho

### `//!SOURCE` (texturas de consulta)

- `src/upscaler/magpiefx/dds.py` (nuevo): lector de `.dds` sin GPU.
  - Cabecera clásica (máscaras RGB/luminancia/bump o FourCC D3DFMT 36,
    110-116) y cabecera `DX10`.
  - Formatos: los intermedios de `formats.py` salvo `R11G11B10_FLOAT`, más
    `B8G8R8A8_UNORM`, que se lee como RGBA. Se usa solo el primer mip.
  - Imágenes `.png`, `.bmp`, `.jpg` y `.tif` con Pillow, como
    `R8G8B8A8_UNORM` (Magpie también las acepta).
  - **BC no hace falta:** los 17 `.dds` de Magpie son sin comprimir. RAVU y
    NIS usan `R16G16B16A16_FLOAT` (FourCC 113), SMAA `AreaTex`
    `R8G8B8A8_UNORM` y `SearchTex` `R8_UNORM` (luminancia). Por eso BC1-BC7,
    los cube maps, los volúmenes y los arrays se rechazan con un error claro,
    en lugar de decodificarse.
- `src/upscaler/magpiefx/lookup.py` (nuevo): subida a la GPU.
  `Texture2D.upload_subresources` copia siempre 4 bytes por texel (y con
  datos más largos desborda su staging), así que no sirve para FP16 ni R8. En
  cambio los texels se pasan a RGBA float en la CPU, van a un `Buffer` de
  storage (`format=0`) y un pase de cómputo de un solo uso los escribe en una
  textura del formato correcto (`RWByteAddressBuffer` → `RWTexture2D` con
  `[[vk::image_format]]`). No toca `vulkan/**`.
- Parser: con `//!SOURCE`, `//!FORMAT` es opcional (como el `UNKNOWN` de
  Magpie). La textura se declara entonces `Texture2D<float4>` y toma el
  formato del archivo. Si hay `//!FORMAT`, tiene que coincidir con el
  archivo.
- `effect.py`: `compile_effect` lee los archivos al cargar (rutas relativas a
  la carpeta del efecto; no se puede salir de ella) y devuelve un
  `CompiledEffect` (NamedTuple) en vez de la 4-tupla. `MagpieEffect` sube cada
  textura una vez, en el primer `_rebuild`, y la enlaza como SRV en los pases
  que la nombran en `//!IN`.

### Los 31 efectos

- 26 RAVU, 4 SMAA y NIS, con sus 17 `.dds` y los `.hlsli` que incluyen
  (`RAVU/prescalers.hlsli`, `SMAA/SMAA.hlsli`), sin modificar. Vienen de
  `Blinue/Magpie` `src/Effects` en `241e0a447d1c45fc95781f9cd4b5a9198e738a3b`,
  el mismo commit que los 86. `diff -r` contra ese checkout solo muestra
  `LICENSE-Magpie` y `README.md`, que son nuestros.
- `*.dds` agregado al package-data (`pyproject.toml`) y al `MANIFEST.in`.
  Ocupan unos 5,3 MB.
- `effects/README.md` actualizado: 117 efectos.

### FP16

- `codegen.mf_type_macros(fp16)`: con FP16, `MP_FP16` y `MF*` =
  `min16float*`, como Magpie. Sin FP16, `float*`.
- `compiler`: con `fp16=True`, dxc usa `-T cs_6_2 -enable-16bit-types` y
  glslang `--hlsl-enable-16bit-types`. Con eso `min16float` es un float de 16
  bits de verdad (capacidad SPIR-V `Float16`). El flag entra en la clave de
  la caché.
- Las texturas (SRV y UAV) se declaran siempre con tipos de 32 bits. dxc
  rechaza `Texture2D<min16float4>` en Vulkan ("sampled type ... smaller than
  32-bits"), así que la conversión ocurre al guardar en variables `MF`. En
  FP32 el resultado es idéntico (`MF` = `float`).
- Cuándo se usa: el efecto declara `//!CAPABILITY FP16` y `fp16_wanted()`
  devuelve verdadero, o sea que `get_current_device().supports_fp16` es
  verdadero y `UPSCALER_MAGPIEFX_FP16` no es `0`/`off`.
  - Si un pase no compila en FP16, se avisa en el log y se recompila todo el
    efecto en FP32.
  - `MagpieEffect.fp16` dice cuál quedó, y el log muestra
    `MagpieFX X: using FP16`.
  - `MagpieEffect.load(..., fp16=None|True|False)` permite forzarlo.

### Comprobado (solo compilación, sin GPU)

- `python3 setup.py build_ext --inplace` correcto. Los módulos importan con el
  venv y `list_effects()` da 117.
- Con dxc, los 31 efectos nuevos leen sus `.dds` y compilan todos sus pases
  (BGRA plegado). SMAA pliega el cambio de canales solo a la salida: pasa
  `INPUT` a una función, así que usa un pase SwapRB a la entrada, como FXAA.
- Con dxc y caché vacía, los 42 efectos con `//!CAPABILITY FP16` compilan en
  FP16, sin volver a FP32.
- El pase de subida compila con dxc y con glslang para los 19 formatos.
- Con glslang, FP16 vuelve a FP32 (rechaza las conversiones implícitas de
  float a half). NIS, NVSharpen y SMAA fallan con glslang también en FP32;
  ya figuraba en el informe de 1.2.

## Lo que no se hizo

- **Ejecutar nada en la GPU ni pytest** (regla de la ronda): ni la subida de
  las texturas ni los efectos nuevos corrieron todavía.
- Texturas BC (no las usa ningún efecto de Magpie), `R11G11B10_FLOAT` como
  textura de consulta y DDS sin canal alfa del tipo `X8`.
- Opción de config/CLI/GUI para FP16. Por ahora solo existe la variable
  `UPSCALER_MAGPIEFX_FP16`, porque `config/**` y la GUI son de otros frentes.

## Para la integración (archivos de otros frentes)

- `src/upscaler/gui/sidebars/tabs/effects.py` (frente C), en la contra del
  motor MagpieFX: `"Effects with .dds lookup textures are not supported yet"`.
  Hay que borrarla (y su traducción). Se podría poner en su lugar
  «Some effects need dxc (glslang cannot compile them)». `_bundled_effect_count()`
  ya cuenta 117 sola.
- `_REFERENCE_MS` / tiempos de referencia de la GUI: no tienen RAVU, SMAA ni
  NIS. Con «Medir en mi GPU» (frente C) quizá no haga falta.
- `scripts/magpiefx-compile-all.py` y `scripts/magpiefx-bench.py` no son de
  este frente y no los toqué.
  - `compile-all` con `--run` va a cargar los 31 con `MagpieEffect.load` y FP16
    automático. En el modo por backend (sin `--run`) compila siempre en FP32.
    Si se quiere comparar FP16 en el banco, falta agregarle un
    `--fp16 on|off` (puede hacerse con `UPSCALER_MAGPIEFX_FP16`).
- `CHANGELOG.md`: «Added: los 31 efectos de Magpie con texturas `.dds`
  (RAVU, SMAA, NIS); `//!SOURCE`; FP16 en los efectos que lo declaran
  (`UPSCALER_MAGPIEFX_FP16=0` lo desactiva)».

## Qué probar (guion, sección 2)

- **T2.1:** cargar los 31 en 1280×720 (`scripts/magpiefx-compile-all.py
  src/upscaler/magpiefx/effects --run`) y anotar el tiempo. Primero hay que
  verificar que la subida por cómputo funciona:
  - `R16G16B16A16_FLOAT`: RAVU y NIS.
  - `R8G8B8A8_UNORM` y `R8_UNORM`: SMAA.
  - Una forma de comprobarla: `MagpieEffect.load("SMAA_High")`, procesar un
    cuadro, descargar `_source_textures[5]` (`SearchTex`, 64×16
    `R8_UNORM`) con `download()` y compararla con los datos del `.dds`.
- **T2.2:** RAVU (p. ej. `RAVU_R3`, `RAVU_Zoom_R2_RGB`), NIS y `SMAA_High`
  sobre el video: colores y bordes. SMAA usa el pase SwapRB de entrada.
- **T2.3:** la cadena UL con FP16 (por defecto) y con `UPSCALER_MAGPIEFX_FP16=0`.
  Comparar los ms por efecto y la imagen, y confirmar `using FP16` en el log.
  - Hay que mirar sobre todo los Anime4K/CuNNy en FP16: algunos efectos
    pueden perder precisión. Magpie tiene la misma opción y la deja activada.
- **T2.4:** `UPSCALER_MAGPIEFX_FP16=0`, o lavapipe/la iGPU si no tienen
  `shaderFloat16`: compila en FP32 sin error.
- **T2.5:** borrar (o renombrar) `RAVU/ravu_lut2_f16.dds` y cargar `RAVU_R2`.
  Error esperado: `RAVU_R2: lookup texture 'ravu_lut2_f16.dds' of 'ravu_lut2'
  not found`. La cadena sigue sin el efecto.
- **T2.6:** los 86 de antes con el script de banco.
  - Ojo: ahora las texturas se declaran `Texture2D<float4>` en lugar de
    `Texture2D<MF4>`. En FP32 es lo mismo, pero el código generado cambia, así
    que la caché de SPIR-V se recompila una vez.
  - Además los efectos FP16 corren en FP16 por defecto. Para comparar con la
    1.2, usar `UPSCALER_MAGPIEFX_FP16=0`.
- **Tests nuevos:** `tests/test_magpiefx_dds.py`,
  `tests/test_magpiefx_source.py` y `tests/test_magpiefx_fp16.py`. Necesitan
  la extensión compilada (importan `upscaler.magpiefx`), pero no la GPU.
