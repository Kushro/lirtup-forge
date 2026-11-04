# MagpieFX effects

LirtUp Forge runs effects written for [Magpie](https://github.com/Blinue/Magpie)
in its MagpieFX format (`.hlsl` files with `//!` directives, version 4). The
effects are loaded unmodified: the loader reads the directives, generates
one compute shader per pass, compiles it to SPIR-V at runtime and runs all
passes on the GPU.

MagpieFX is the only effects engine: the hand-written «native» ports of
LumaSharpen, Anime4K_Upscale_S and CRT_Easymode that 2.1 and earlier offered
as an alternative are gone, and those three effects run through MagpieFX
like the other 114.

## Where the chain runs

A mode has two MagpieFX lists, and a frame goes through them in this order:

```text
capture ─▶ [magpie_effects_before_model] ─▶ model (SRCNN, or none)
        ─▶ [deband] ─▶ [magpie_effects] ─▶ final scaler ─▶ [CAS, bloom, vignette]
        ─▶ [LUT, grain] ─▶ screen
```

- `magpie_effects_before_model` (empty by default) runs on the captured frame,
  at the game's resolution; the model upscales its result. With a list here,
  the scaler processes the whole frame instead of only the tiles that
  changed.
- `magpie_effects` runs on the model's output (debanded if deband is on),
  before the final scaler, so an effect that changes the size (e.g.
  `Anime4K_Upscale_S`, 2x) hands a larger image to the scaler.

With `model: none` both lists run one after the other on the captured frame.
An effect can be in both lists; it then runs twice. In the GUI, the mode
editor shows the two lists as two segments, *Before the model* and *After the
model*, and a card moves between them with its `swap_vert` button or by
dragging it ([Scaling modes](modes.md#magpie-effects)).

Reference GPU time (ms/frame, 1280x720 to 2560x1440, AMD RX 9070 XT, dxc,
after the model):

| Effect | MagpieFX |
|---|---|
| Sharpen/LumaSharpen | 0.016 |
| Anime4K/Anime4K_Upscale_S | 0.142 |
| CRT/CRT_Easymode | 0.118 |

### Files from 2.1 with native effects

Files that still use the old keys (`effects_engine`, `anime4k_enabled`,
`luma_sharpen_*`, `crt_*`) keep working: when the configuration is read,
the native effects that were on become the same MagpieFX effects in
`magpie_effects`, in the old order (Anime4K, LumaSharpen, CRT) and with the
same parameters; with `effects_engine: magpiefx` the chain is kept as it
was. `lirtup migrate` (or *Migrate…* in the GUI) rewrites the files that
way, with a backup. On the command line the old flags (`--effects-engine`,
`--enable-anime4k`, `--enable-luma-sharpen`, `--luma-sharpen-*`,
`--enable-crt`, `--crt-*`) no longer exist: use `--magpie-effect`. See
[Configuration](configuration.md) and [Migrating](migration.md).

## Compilers

The package ships the bundled effects already compiled (see
[Precompiled seed](#precompiled-seed)), so they load without a compiler.
A compiler is needed for your own effects, for edited ones, and for any
variant the seed does not have:

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

## Precompiled seed

The AUR package, the tarball and the AppImage include the SPIR-V of all the
bundled effects, in `/usr/share/lirtup/magpiefx-seed` (inside the AppImage, in
its own tree). It covers FP32 and, for the effects that declare it, FP16, and
every red/blue swap a chain can ask for. When an effect loads, the app looks
in your cache (`~/.cache/linux-rt-upscaler/magpiefx`) first, then in the seed,
and only compiles when neither has it. The seed is read-only: its shaders are
used in place, not copied to your cache.

- It does not depend on which dxc or glslang you have, or whether you have
  one: the bundled effects, including the 3 `Anime4K_Denoise_Bilateral_*`
  that need glslang, load on a machine with neither.
- Your own effects, and bundled effects you edit, are not in the seed and
  still need a compiler.
- `LIRTUP_MAGPIEFX_SEED_DIR=/path` uses another seed folder. By default it is
  `magpiefx-seed` next to the effects folder (`LIRTUP_EFFECTS_DIR`).
- To build one yourself (needs dxc and glslang):
  `cargo run --release -p lirtup-magpiefx --bin magpiefx-seed -- --out DIR`
  (`--help` lists the options).

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
lirtup run --magpie-effect Sharpen/LumaSharpen:sharpStrength=1.0 -t "My game"
lirtup run --magpie-effect Anime4K/Anime4K_Upscale_S --magpie-effect CRT/CRT_Easymode -t "My game"
lirtup run --magpie-effect-before-model Sharpen/LumaSharpen -t "My game"
```

In the YAML configuration (or a mode file):

```yaml
magpie_effects_before_model:
  - Sharpen/LumaSharpen
magpie_effects:
  - name: CRT/CRT_Easymode
    params:
      maskStrength: 0.5
```

Both lists take the same forms: a name, `Name:param=value,...`, or
`{name, params}`. The names are the catalog's (`lirtup list effects`); the
bare file name (`LumaSharpen`) also works.

Parameter names, ranges and defaults are the ones in the effect's
`//!PARAMETER` blocks. Out-of-range or unknown parameters disable the effect
with an error in the log.

If an effect fails to load or to run, it is disabled and the rest of the
chain keeps working.

## Measuring performance

- In the GUI: *Measure on my GPU* in *Scaling modes* times every effect on
  your GPU; the costs on the effect cards then come from it (an effect after
  the model counts 4× its measurement with a 2× model, see
  [Scaling modes](modes.md#measure-on-my-gpu)).
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
store to `OUTPUT`) of each list, at no measurable cost. Effects that pass `INPUT` or
`OUTPUT` to a function (FXAA) get a separate swap pass instead.

## Limitations

- `//!SOURCE` reads uncompressed textures only (see
  [Lookup textures](#lookup-textures-source)).
- `NIS`, `NVSharpen`, `FineSharp` and the 4 `SMAA` effects need dxc:
  glslang crashes or rejects them.
- `Anime4K_Denoise_Bilateral_*` do not compile with dxc (it rejects their
  array sizes); they need `glslangValidator` installed as the fallback,
  unless they come from the [precompiled seed](#precompiled-seed).
- The 4 SMAA effects pass `INPUT` to a function, so, like FXAA, they use a
  separate red/blue swap pass for their input.
- With the glslang backend, storage image formats are inferred from the
  HLSL texel type instead of being declared explicitly.
