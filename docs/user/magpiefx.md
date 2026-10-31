# MagpieFX effects

The upscaler can run effects written for [Magpie](https://github.com/Blinue/Magpie)
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
in the GUI's Effects tab):

| | Native | MagpieFX |
|---|---|---|
| Effects | LumaSharpen, Anime4K_Upscale_S, CRT_Easymode | 86 bundled from Magpie, plus any MagpieFX `.hlsl` you add |
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

## Requirements

One of these compilers must be installed:

- `dxc` (DirectX Shader Compiler), preferred. On Arch/CachyOS:
  `sudo pacman -S directx-shader-compiler`.
- `glslangValidator` (package `glslang`), used as a fallback when dxc is
  missing or rejects an effect.

Set `UPSCALER_MAGPIEFX_COMPILER=dxc|glslang|auto` to force a backend and
`DXC=/path/to/dxc` to use a specific binary. Compiled shaders are cached in
`~/.cache/linux-rt-upscaler/magpiefx`, so only the first start of an effect
pays the compile time.

## Choosing effects

The package bundles the 86 effects of Magpie's `src/Effects` that do not need
`.dds` lookup textures: Anime4K, ACNet, ArtCNN, CuNNy2, FSRCNNX, NNEDI3,
k7_modernAnime, the CRT and pixel-art families, FSR, CAS, FXAA, xBRZ, the
classic scalers and more (`upscale --list-magpie-effects` prints them). Any
other MagpieFX effect can be copied to `~/.config/linux-rt-upscaler/effects/`
(with the `.hlsli` files it includes) or given as a path.

```bash
upscale --list-magpie-effects
upscale --magpie-effect LumaSharpen:sharpStrength=1.0 -t "My game"
upscale --magpie-effect Anime4K_Upscale_S --magpie-effect CRT_Easymode -t "My game"
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

- In the app: `--magpie-profile` (or `magpie_profile: true`) logs the median
  GPU time per effect every 5 seconds, measured with Vulkan timestamps around
  each dispatch.
- Headless: `scripts/magpiefx-bench.py` runs effects on an image and prints
  the median GPU time per frame and per pass:

  ```bash
  python scripts/magpiefx-bench.py docs/assets/comparisons/fatamoru/original.png \
      LumaSharpen Anime4K_Upscale_S CRT_Easymode --resize 1280x720 --target 2560x1440
  ```

- `scripts/magpiefx-compile-all.py MAGPIE/src/Effects --run` tries every
  effect of a Magpie checkout and prints a compatibility table.

## Color channels

Captured frames travel through the app as BGRA bytes, while MagpieFX effects
expect RGBA. The loader swaps red and blue inside the generated shaders of
the first effect (on every read of `INPUT`) and of the last effect (on every
store to `OUTPUT`), at no measurable cost. Effects that pass `INPUT` or
`OUTPUT` to a function (FXAA) get a separate swap pass instead.

## Limitations

- `//!SOURCE` textures (lookup tables loaded from `.dds` files, used by
  RAVU, SMAA and NIS) are not supported yet, so those 31 effects are not
  bundled; copied by hand they fail with a clear error.
- `Anime4K_Denoise_Bilateral_*` do not compile with dxc (it rejects their
  array sizes); they need `glslangValidator` installed as the fallback.
- FP16 (`//!CAPABILITY FP16`) is ignored: `MF` types are always 32-bit floats.
- With the glslang backend, storage image formats are inferred from the
  HLSL texel type instead of being declared explicitly.
