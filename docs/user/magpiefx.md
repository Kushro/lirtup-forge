# MagpieFX effects

LirtUp Forge runs effects written for [Magpie](https://github.com/Blinue/Magpie)
in its MagpieFX format (`.hlsl` files with `//!` directives, version 4). The
effects are loaded unmodified: the loader reads the directives, generates
one compute shader per pass, compiles it to SPIR-V at runtime and runs all
passes on the GPU.

The MagpieFX chain runs on the captured (and optionally upscaled and
debanded) frame, **before** the final scaler, so an effect that changes the
size (e.g. `Anime4K_Upscale_S`, 2x) hands a larger image to the scaler.

## Native or MagpieFX

The Magpie effects come in two engines, and only the selected one runs
(`effects_engine` in the config, `--effects-engine` on the CLI, or *Engine*
in the GUI's mode editor):

| | Native | MagpieFX |
|---|---|---|
| Effects | LumaSharpen, Anime4K_Upscale_S, CRT_Easymode | All 117 of Magpie bundled, plus any MagpieFX `.hlsl` you add |
| Loading | Precompiled, instant | Compiled on first load, then cached |
| Out-of-range writes | Bounds-checked | Rely on the driver discarding them |
| Settings | `anime4k_enabled`, `luma_sharpen_*`, `crt_*` | `magpie_effects` chain |

Reference GPU time (ms/frame, 1280x720 to 2560x1440, AMD RX 9070 XT, dxc):

| Effect | Native | MagpieFX |
|---|---|---|
| LumaSharpen | 0.016 | 0.016 |
| Anime4K_Upscale_S | 0.145 | 0.142 |
| CRT_Easymode | 0.122 | 0.118 |

The rest of this page is about the MagpieFX engine.

## Compilers

One of these compilers must be installed:

- `dxc` (DirectX Shader Compiler), preferred. On Arch/CachyOS:
  `sudo pacman -S directx-shader-compiler`.
- `glslangValidator` (package `glslang`), used as a fallback when dxc is
  missing or rejects an effect.

Set `UPSCALER_MAGPIEFX_COMPILER=dxc|glslang|auto` to force a backend and
`DXC=/path/to/dxc` to use a specific binary. Compiled shaders are cached in
`~/.cache/linux-rt-upscaler/magpiefx`, so only the first start of an effect
pays the compile time.

`UPSCALER_MAGPIEFX_FP16=auto|0` controls FP16 (see [FP16](#fp16)); `auto`
is the default.

## Choosing effects

The package bundles all 117 effects of Magpie's `src/Effects`: Anime4K,
ACNet, ArtCNN, CuNNy2, FSRCNNX, NNEDI3, RAVU, k7_modernAnime, the CRT and
pixel-art families, FSR, NIS, CAS, FXAA, SMAA, xBRZ, the classic scalers and
more (`lirtup list effects` prints them). They are installed in
`/usr/share/lirtup/effects` (inside the AppImage, in its own tree;
`LIRTUP_EFFECTS_DIR` points elsewhere). Any other MagpieFX effect
can be copied to `~/.config/linux-rt-upscaler/effects/` (with the `.hlsli`
files it includes and the lookup textures it reads) or given as a path.

```bash
lirtup list effects
lirtup run --magpie-effect LumaSharpen:sharpStrength=1.0 -t "My game"
lirtup run --magpie-effect Anime4K_Upscale_S --magpie-effect CRT_Easymode -t "My game"
```

In the YAML configuration:

```yaml
magpie_effects:
  - LumaSharpen
  - name: CRT_Easymode
    params:
      maskStrength: 0.5
```

Parameter names, ranges and defaults are the ones in the effect's
`//!PARAMETER` blocks. Out-of-range or unknown parameters disable the effect
with an error in the log.

If an effect fails to load or to run, it is disabled and the rest of the
chain keeps working.

## Measuring performance

- In the GUI: *Measure on my GPU* in *Scaling modes* times every effect on
  your GPU; the costs on the effect cards then come from it.
- While scaling: the stats panel (`Alt`+`Shift`+`I`) shows the GPU time of
  each effect, and `--magpie-profile` (or `magpie_profile: true`) logs the
  median GPU time per effect every 5 seconds.
- Without a window: `lirtup bench` is the measurement the GUI runs.

## Lookup textures (`//!SOURCE`)

Some effects read precomputed tables from files next to them:

```hlsl
//!TEXTURE
//!SOURCE ravu_lut2_f16.dds
//!FORMAT R16G16B16A16_FLOAT
Texture2D ravu_lut2;
```

The 26 RAVU effects, the 4 SMAA effects and NIS use them. The loader reads
the file when the effect loads, uploads it once to a GPU texture and binds
it to the passes that list it in `//!IN`. As in Magpie:

- The path is relative to the effect's folder; it cannot point outside it.
- `//!FORMAT` is optional. When present it must match the file's format,
  otherwise the effect fails with an error that names both formats.
- Supported files: uncompressed `.dds` (legacy header or `DX10` header) in
  any of the intermediate formats (`R16G16B16A16_FLOAT`, `R8G8B8A8_UNORM`,
  `R8_UNORM`, ...), and `.png`, `.bmp`, `.jpg` and `.tif` images, read as
  `R8G8B8A8_UNORM`. Only the first mip level is used.
- Block-compressed `.dds` (BC1-BC7), cube maps, volumes and texture arrays
  are rejected with a clear error. None of Magpie's effects use them.

## FP16

Effects with `//!CAPABILITY FP16` declare that their `MF` types (`MF`,
`MF2`, `MF4x4`, ...) may be 16-bit floats. When the GPU supports 16-bit
floats in shaders (`shaderFloat16`), the loader compiles those effects with
`MF` = `min16float` and 16-bit types enabled (dxc: shader model 6.2 and
`-enable-16bit-types`; glslang: `--hlsl-enable-16bit-types`), and defines
`MP_FP16`, which some effects (FSR, CAS, NIS) use to pick packed half-float
code. Textures are still declared with 32-bit texel types, because Vulkan
cannot sample 16-bit texels; values are converted when the effect stores
them in `MF` variables.

The effect falls back to 32-bit floats when:

- the GPU has no FP16 support,
- `UPSCALER_MAGPIEFX_FP16=0` is set, or
- an FP16 pass does not compile (a warning with the compiler message is
  logged and the whole effect is compiled again in FP32).

The log says `MagpieFX <effect>: using FP16` when an effect runs in FP16.

With dxc, all 42 bundled FP16 effects compile in FP16. glslang rejects the
implicit float-to-half conversions the effects rely on, so with the glslang
backend they fall back to FP32.

## Color channels

Captured frames travel through the app as BGRA bytes, while MagpieFX effects
expect RGBA. The loader swaps red and blue inside the generated shaders of
the first effect (on every read of `INPUT`) and of the last effect (on every
store to `OUTPUT`), at no measurable cost. Effects that pass `INPUT` or
`OUTPUT` to a function (FXAA) get a separate swap pass instead.

## Limitations

- `//!SOURCE` reads uncompressed textures only (see
  [Lookup textures](#lookup-textures-source)).
- `NIS`, `NVSharpen`, `FineSharp` and the 4 `SMAA` effects need dxc:
  glslang crashes or rejects them.
- `Anime4K_Denoise_Bilateral_*` do not compile with dxc (it rejects their
  array sizes); they need `glslangValidator` installed as the fallback.
- The 4 SMAA effects pass `INPUT` to a function, so, like FXAA, they use a
  separate red/blue swap pass for their input.
- With the glslang backend, storage image formats are inferred from the
  HLSL texel type instead of being declared explicitly.
