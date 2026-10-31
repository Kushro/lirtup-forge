# MagpieFX loader: compatibility and performance report

Generated with the generic MagpieFX loader (`src/upscaler/magpiefx`) on
2026-10-31.

- Magpie `src/Effects` at commit `241e0a447d1c45fc95781f9cd4b5a9198e738a3b`
  (2026-10-28), 117 effect files (`.hlsli` and `StubDefs.hlsli` excluded).
- dxc 1.10.2605.37 (`directx-shader-compiler`, CachyOS), glslangValidator
  16.6.0.
- GPU: AMD Radeon RX 9070 XT, RADV (Mesa), Linux 7.3.

## Measurements of the three target effects

GPU time is measured with Vulkan timestamps written before and after each
dispatch (`Compute.dispatch_sequence(timestamps=True)`) and summed over the
passes. FP32 (`CAPABILITY FP16` is ignored).

### Red/blue swap

Captured frames travel through the app as BGRA bytes in R8G8B8A8 textures,
while MagpieFX effects expect RGBA (luma weights and the Anime4K CNN depend
on it). The swap is folded into the generated shaders: the pass source is
preprocessed by the compiler (macros expanded) and rewritten so that every
read of `INPUT` (`Load`, `Sample*`, `[]`) is swizzled, `GatherRed` and
`GatherBlue` trade places, and every store `OUTPUT[...] = v;` stores `v.bgra`.
Only the first effect of the chain swaps on read and only the last one on
write; textures between effects and intermediate textures stay RGBA. Effects
that pass `INPUT`/`OUTPUT` to a function (the three FXAA) fall back to a
separate `SwapRB` pass for that side. Output is bit-identical to the
previous approach (separate `SwapRB` passes before and after the chain) for
the three effects and for the chain Anime4K_Upscale_S + LumaSharpen +
CRT_Easymode, on a red/green/blue stripes image and on the test image.

### Headless

`scripts/magpiefx-bench.py` with input
`docs/assets/comparisons/fatamoru/original.png` resized with Lanczos and
uploaded as BGRA (`--bgra fold,pass`); `--target` is the OUTPUT size of
effects that do not fix it (CRT). `fold` (swap in the shaders, current) and
`pass` (separate SwapRB passes, previous) alternate in batches of 20 frames;
median of 400 frames each after 20 warm-up frames.

| Effect | 1280x720 -> 2560x1440, fold | pass | 1920x1080 -> 3840x2160, fold | pass |
|---|---|---|---|---|
| LumaSharpen (output = input size) | 0.016 ms | 0.029 ms | 0.034 ms | 0.055 ms |
| Anime4K_Upscale_S (2x, 4 passes: 0.017 + 0.043 + 0.043 + 0.041 at 720p) | 0.144 ms | 0.171 ms | 0.296 ms | 0.345 ms |
| CRT_Easymode | 0.119 ms | 0.145 ms | 0.257 ms | 0.311 ms |

The folded swap costs nothing measurable: the `fold` times equal the times
with no swap at all (0.016 / 0.144 / 0.117 ms at 720p in the first report).
It also removes two submissions per frame: CPU submit + fence wait drops
from ~0.20-0.54 ms to ~0.07-0.37 ms. dxc and glslang SPIR-V give the same
GPU times.

### In the app

`upscale -m none --magpie-effect X --magpie-profile` over a 60 fps mpv
window (panning video made from the test image) captured at 2560x1440,
overlay on a 2560x1440 screen. The log prints the median every 5 s; the
table is the median of 8 windows (~300 frames each, first two windows of
each run skipped). Old (commit `b993d7c1`, SwapRB passes) and new runs were
interleaved: old, new for each effect, twice.

| Effect | Old: effect + SwapRB in/out | New: total | Saved |
|---|---|---|---|
| LumaSharpen | 0.070 + 0.046 = 0.116 ms | 0.067 ms | 0.049 ms |
| Anime4K_Upscale_S (2560x1440 -> 5120x2880) | 0.617 + 0.105 = 0.722 ms | 0.615 ms | 0.107 ms |
| CRT_Easymode (2560x1440 -> 2560x1440) | 0.141 + 0.048 = 0.189 ms | 0.139 ms | 0.050 ms |

Between sessions the absolute app numbers vary by about 10% (the first
report measured 0.064 / 0.564 / 0.130 ms for the effects alone); within one
interleaved run they are stable to ±0.002 ms. Pipelines are built once and
only rebuilt when the input texture or the target size changes.

First-load compile time (no cache): LumaSharpen ~40 ms, CRT_Easymode ~35 ms,
Anime4K_Upscale_S ~0.5 s with dxc (~0.2 s with glslang); the folded swap
adds one preprocessor run per affected pass. Later starts read
`~/.cache/linux-rt-upscaler/magpiefx` (1-2 ms).

## Every Magpie effect

`scripts/magpiefx-compile-all.py MAGPIE/src/Effects --run`: the `dxc` and
`glslang` columns compile every pass with that backend only; `run` loads the
effect with the default `auto` backend (dxc, glslang as fallback) the way
the app loads a single effect (BGRA in and out, swap folded in), creates the
Vulkan pipelines and runs it once on a 1280x720 texture (GPU time shown,
plus any SwapRB fallback pass).

Summary:

- **dxc compiles 114/117.** The 3 failures are
  `Anime4K_Denoise_Bilateral_{Mean,Median,Mode}`: they size local arrays
  with `max(uint(ceil(SPATIAL_SIGMA * 2.0)), 1) * 2 + 1`. fxc constant-folds
  intrinsics in array sizes, dxc does not ("variable length arrays are not
  supported in HLSL"), in both HLSL 2018 and 2021 modes. glslang accepts
  them, so the `auto` backend falls back and they run.
- **glslang compiles 105/117.** It crashes (SIGSEGV) on `NVSharpen`,
  `FineSharp` (pass 5) and the four `SMAA_*`, reports "Recursion detected"
  linking `NIS`, and rejects a float constructor in four `RAVU_*_RGB`
  variants. It also ignores `[[vk::image_format]]`, so with glslang the UAV
  format is inferred from the texel type.
- **86/117 run**, the same 86 as before folding the red/blue swap into the
  shaders. 83 fold the swap on both sides; the three FXAA pass `INPUT` to a
  function and use one SwapRB pass for the input side. All 31 that do not run use `//!SOURCE` lookup textures
  (`.dds` files): 26 RAVU, 4 SMAA, NIS. They compile with dxc; loading the
  DDS files is the missing piece.
- Undocumented bits the effects rely on and the loader provides: `MulAdd`
  (25k uses, only with `//!USE MulAdd`), `Rmp8x8`, the `MP_BLOCK_*` and
  `MP_NUM_THREADS_*` macros. `MP_FP16`, `MP_DEBUG` and `MP_INLINE_PARAMS` are
  only tested with `#ifdef` and are left undefined. `GetFrameCount()` /
  `__frameCount` (`//!USE _DYNAMIC`) is only declared in `StubDefs.hlsli`;
  no shipped effect uses it, but the loader supports it.
- Out-of-bounds UAV writes: CS passes such as Anime4K's write 2x2 blocks
  without bounds checks, relying on D3D11 discarding out-of-range writes. In
  Vulkan that is undefined without `robustImageAccess`; RADV discards them in
  practice (odd sizes looked clean), but enabling `robustImageAccess` at
  device creation would make it well-defined.
- dxc is run with `-HV 2018` because Magpie effects target fxc semantics
  (not compared against 2021 for the whole set).

| Effect | dxc | glslang | run |
|---|---|---|---|
| `ACNet.hlsl` | ok | ok | ok 0.515 ms |
| `Ani4Kv2_ArtCNN_C4F32_i2.hlsl` | ok | ok | ok 28.796 ms |
| `Anime4K/Anime4K_3D_AA_Upscale_US.hlsl` | ok | ok | ok 0.099 ms |
| `Anime4K/Anime4K_3D_Upscale_US.hlsl` | ok | ok | ok 0.099 ms |
| `Anime4K/Anime4K_Denoise_Bilateral_Mean.hlsl` | FAIL | ok | ok 0.035 ms |
| `Anime4K/Anime4K_Denoise_Bilateral_Median.hlsl` | FAIL | ok | ok 0.030 ms |
| `Anime4K/Anime4K_Denoise_Bilateral_Mode.hlsl` | FAIL | ok | ok 0.040 ms |
| `Anime4K/Anime4K_Restore_L.hlsl` | ok | ok | ok 0.438 ms |
| `Anime4K/Anime4K_Restore_M.hlsl` | ok | ok | ok 0.256 ms |
| `Anime4K/Anime4K_Restore_S.hlsl` | ok | ok | ok 0.125 ms |
| `Anime4K/Anime4K_Restore_Soft_L.hlsl` | ok | ok | ok 0.435 ms |
| `Anime4K/Anime4K_Restore_Soft_M.hlsl` | ok | ok | ok 0.253 ms |
| `Anime4K/Anime4K_Restore_Soft_S.hlsl` | ok | ok | ok 0.120 ms |
| `Anime4K/Anime4K_Restore_Soft_UL.hlsl` | ok | ok | ok 1.881 ms |
| `Anime4K/Anime4K_Restore_Soft_VL.hlsl` | ok | ok | ok 0.880 ms |
| `Anime4K/Anime4K_Restore_UL.hlsl` | ok | ok | ok 1.875 ms |
| `Anime4K/Anime4K_Restore_VL.hlsl` | ok | ok | ok 0.874 ms |
| `Anime4K/Anime4K_Thin_HQ.hlsl` | ok | ok | ok 0.105 ms |
| `Anime4K/Anime4K_Upscale_Denoise_L.hlsl` | ok | ok | ok 0.441 ms |
| `Anime4K/Anime4K_Upscale_Denoise_S.hlsl` | ok | ok | ok 0.154 ms |
| `Anime4K/Anime4K_Upscale_Denoise_UL.hlsl` | ok | ok | ok 1.648 ms |
| `Anime4K/Anime4K_Upscale_Denoise_VL.hlsl` | ok | ok | ok 0.864 ms |
| `Anime4K/Anime4K_Upscale_GAN_x2_M.hlsl` | ok | ok | ok 1.101 ms |
| `Anime4K/Anime4K_Upscale_GAN_x2_S.hlsl` | ok | ok | ok 0.611 ms |
| `Anime4K/Anime4K_Upscale_GAN_x3_L.hlsl` | ok | ok | ok 3.138 ms |
| `Anime4K/Anime4K_Upscale_L.hlsl` | ok | ok | ok 0.438 ms |
| `Anime4K/Anime4K_Upscale_S.hlsl` | ok | ok | ok 0.158 ms |
| `Anime4K/Anime4K_Upscale_UL.hlsl` | ok | ok | ok 1.636 ms |
| `Anime4K/Anime4K_Upscale_VL.hlsl` | ok | ok | ok 0.861 ms |
| `Bicubic.hlsl` | ok | ok | ok 0.081 ms |
| `Bilinear.hlsl` | ok | ok | ok 0.028 ms |
| `CAS/CAS.hlsl` | ok | ok | ok 0.016 ms |
| `CAS/CAS_Scaling.hlsl` | ok | ok | ok 0.075 ms |
| `CRT/CRT_Easymode.hlsl` | ok | ok | ok 0.126 ms |
| `CRT/CRT_Geom.hlsl` | ok | ok | ok 0.413 ms |
| `CRT/CRT_Hyllian.hlsl` | ok | ok | ok 0.194 ms |
| `CRT/CRT_Lottes.hlsl` | ok | ok | ok 0.613 ms |
| `CRT/GTU_v050.hlsl` | ok | ok | ok 0.213 ms |
| `CuNNy2/CuNNy-3x12-NVL.hlsl` | ok | ok | ok 0.538 ms |
| `CuNNy2/CuNNy-4x12-NVL.hlsl` | ok | ok | ok 0.665 ms |
| `CuNNy2/CuNNy-4x16-NVL.hlsl` | ok | ok | ok 1.077 ms |
| `CuNNy2/CuNNy-4x24-NVL.hlsl` | ok | ok | ok 2.278 ms |
| `CuNNy2/CuNNy-4x32-NVL.hlsl` | ok | ok | ok 3.989 ms |
| `CuNNy2/CuNNy-8x32-NVL.hlsl` | ok | ok | ok 7.481 ms |
| `CuNNy2/CuNNy-fast-NVL.hlsl` | ok | ok | ok 0.321 ms |
| `CuNNy2/CuNNy-faster-NVL.hlsl` | ok | ok | ok 0.209 ms |
| `CuNNy2/CuNNy-veryfast-NVL.hlsl` | ok | ok | ok 0.174 ms |
| `Deband.hlsl` | ok | ok | ok 0.123 ms |
| `FSR/FSR_EASU.hlsl` | ok | ok | ok 0.177 ms |
| `FSR/FSR_RCAS.hlsl` | ok | ok | ok 0.019 ms |
| `FSRCNNX/FSRCNNX.hlsl` | ok | ok | ok 0.355 ms |
| `FSRCNNX/FSRCNNX_LineArt.hlsl` | ok | ok | ok 0.355 ms |
| `FXAA/FXAA_High.hlsl` | ok | ok | ok 0.027 ms, 1 SwapRB pass(es) |
| `FXAA/FXAA_Medium.hlsl` | ok | ok | ok 0.031 ms, 1 SwapRB pass(es) |
| `FXAA/FXAA_Ultra.hlsl` | ok | ok | ok 0.035 ms, 1 SwapRB pass(es) |
| `ImageAdjustment.hlsl` | ok | ok | ok 0.019 ms |
| `Jinc.hlsl` | ok | ok | ok 0.225 ms |
| `k7_modernAnime_FHD_x2.hlsl` | ok | ok | ok 1.619 ms |
| `Lanczos.hlsl` | ok | ok | ok 0.276 ms |
| `Nearest.hlsl` | ok | ok | ok 0.027 ms |
| `NIS/NIS.hlsl` | ok | FAIL | FAIL |
| `NIS/NVSharpen.hlsl` | ok | FAIL | ok 0.027 ms |
| `NNEDI3/NNEDI3_nns128_win8x4.hlsl` | ok | ok | ok 3.299 ms |
| `NNEDI3/NNEDI3_nns128_win8x6.hlsl` | ok | ok | ok 4.721 ms |
| `NNEDI3/NNEDI3_nns16_win8x4.hlsl` | ok | ok | ok 0.448 ms |
| `NNEDI3/NNEDI3_nns16_win8x6.hlsl` | ok | ok | ok 0.601 ms |
| `NNEDI3/NNEDI3_nns256_win8x4.hlsl` | ok | ok | ok 6.749 ms |
| `NNEDI3/NNEDI3_nns256_win8x6.hlsl` | ok | ok | ok 8.830 ms |
| `NNEDI3/NNEDI3_nns32_win8x4.hlsl` | ok | ok | ok 0.838 ms |
| `NNEDI3/NNEDI3_nns32_win8x6.hlsl` | ok | ok | ok 1.138 ms |
| `NNEDI3/NNEDI3_nns64_win8x4.hlsl` | ok | ok | ok 1.659 ms |
| `NNEDI3/NNEDI3_nns64_win8x6.hlsl` | ok | ok | ok 2.281 ms |
| `Pixel Art/MMPX.hlsl` | ok | ok | ok 0.037 ms |
| `Pixel Art/Pixellate.hlsl` | ok | ok | ok 0.056 ms |
| `Pixel Art/SharpBilinear.hlsl` | ok | ok | ok 0.024 ms |
| `RAVU/RAVU_3x_R2.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_3x_R2_RGB.hlsl` | ok | FAIL | FAIL |
| `RAVU/RAVU_3x_R3.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_3x_R3_RGB.hlsl` | ok | FAIL | FAIL |
| `RAVU/RAVU_3x_R4.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_3x_R4_RGB.hlsl` | ok | FAIL | FAIL |
| `RAVU/RAVU_Lite_AR_R2.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Lite_AR_R3.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Lite_AR_R4.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Lite_R2.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Lite_R3.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Lite_R4.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_R2.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_R2_RGB.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_R3.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_R3_RGB.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_R4.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_R4_RGB.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Zoom_AR_R2.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Zoom_AR_R2_RGB.hlsl` | ok | FAIL | FAIL |
| `RAVU/RAVU_Zoom_AR_R3.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Zoom_AR_R3_RGB.hlsl` | ok | FAIL | FAIL |
| `RAVU/RAVU_Zoom_R2.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Zoom_R2_RGB.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Zoom_R3.hlsl` | ok | ok | FAIL |
| `RAVU/RAVU_Zoom_R3_RGB.hlsl` | ok | ok | FAIL |
| `SGSR.hlsl` | ok | ok | ok 0.033 ms |
| `Sharpen/AdaptiveSharpen.hlsl` | ok | ok | ok 0.059 ms |
| `Sharpen/FineSharp.hlsl` | ok | FAIL | ok 0.063 ms |
| `Sharpen/LCAS.hlsl` | ok | ok | ok 0.131 ms |
| `Sharpen/LumaSharpen.hlsl` | ok | ok | ok 0.019 ms |
| `SMAA/SMAA_High.hlsl` | ok | FAIL | FAIL |
| `SMAA/SMAA_Low.hlsl` | ok | FAIL | FAIL |
| `SMAA/SMAA_Medium.hlsl` | ok | FAIL | FAIL |
| `SMAA/SMAA_Ultra.hlsl` | ok | FAIL | FAIL |
| `SSimDownscaler.hlsl` | ok | ok | ok 0.304 ms |
| `xBRZ/xBRZ_2x.hlsl` | ok | ok | ok 0.066 ms |
| `xBRZ/xBRZ_3x.hlsl` | ok | ok | ok 0.074 ms |
| `xBRZ/xBRZ_4x.hlsl` | ok | ok | ok 0.131 ms |
| `xBRZ/xBRZ_5x.hlsl` | ok | ok | ok 0.218 ms |
| `xBRZ/xBRZ_6x.hlsl` | ok | ok | ok 0.618 ms |
| `xBRZ/xBRZ_Freescale.hlsl` | ok | ok | ok 0.120 ms |

Totals over 117 effects: dxc: 114 ok, glslang: 105 ok, run: 86 ok
