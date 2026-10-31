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
dispatch (`Compute.dispatch_sequence(timestamps=True)`), median of 200
frames after 20 warm-up frames. FP32 (`CAPABILITY FP16` is ignored).

Headless (`scripts/magpiefx-bench.py`, input
`docs/assets/comparisons/fatamoru/original.png` resized with Lanczos;
`--target` is the OUTPUT size of effects that do not fix it, i.e. CRT):

| Effect | Passes | 1280x720 -> 2560x1440 | 1920x1080 -> 3840x2160 |
|---|---|---|---|
| LumaSharpen (output = input size) | 1 | 0.016 ms | 0.033 ms |
| Anime4K_Upscale_S (2x) | 4 | 0.144 ms (0.017 + 0.043 + 0.043 + 0.041) | 0.288 ms |
| CRT_Easymode | 1 | 0.117 ms | 0.253 ms |

dxc and glslang SPIR-V give the same GPU times for these three effects.
Submit + fence wait from Python adds ~0.06 ms per `dispatch_sequence`.

In the real app (`upscale -m none --magpie-effect X --magpie-profile` over a
60 fps mpv window captured at 2560x1440, overlay on a 2560x1440 screen; the
log prints the median every 5 s, ~300 frames):

| Effect | Effect | + SwapRB in/out | Total MagpieFX |
|---|---|---|---|
| LumaSharpen | 0.064 ms | 0.021 + 0.023 ms | 0.109 ms/frame |
| Anime4K_Upscale_S (2560x1440 -> 5120x2880) | 0.564 ms | 0.021 + 0.079 ms | 0.664 ms/frame |
| CRT_Easymode (2560x1440 -> 2560x1440) | 0.130 ms | 0.021 + 0.026 ms | 0.177 ms/frame |

`SwapRB` is an internal pass that swaps red/blue before and after the chain:
the capture pipeline carries BGRA bytes in R8G8B8A8 textures while MagpieFX
effects expect RGBA (luma weights and the Anime4K CNN depend on it).
Pipelines are built once; they are only rebuilt when the input texture or
the target size changes.

First-load compile time (no cache): LumaSharpen ~40 ms, CRT_Easymode ~35 ms,
Anime4K_Upscale_S ~0.5 s with dxc (~0.2 s with glslang). Later starts read
`~/.cache/linux-rt-upscaler/magpiefx` (1-2 ms).

## Every Magpie effect

`scripts/magpiefx-compile-all.py MAGPIE/src/Effects --run`: the `dxc` and
`glslang` columns compile every pass with that backend only; `run` loads the
effect with the default `auto` backend (dxc, glslang as fallback), creates
the Vulkan pipelines and runs it once on a 1280x720 texture (GPU time shown).

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
- **86/117 run.** All 31 that do not run use `//!SOURCE` lookup textures
  (`.dds` files): 26 RAVU, 4 SMAA, NIS. They compile with dxc; loading the
  DDS files is the missing piece.
- Undocumented bits the effects rely on and the loader provides: `MulAdd`
  (25k uses, only with `//!USE MulAdd`), `Rmp8x8`, the `MP_BLOCK_*` and
  `MP_NUM_THREADS_*` macros. `MP_FP16`, `MP_DEBUG` and `MP_INLINE_PARAMS` are
  only tested with `#ifdef` and are left undefined. `GetFrameCount()` /
  `__frameCount` (`//!USE _DYNAMIC`) is only declared in `StubDefs.hlsli`;
  no shipped effect uses it, but the loader supports it.
- dxc is run with `-HV 2018` because Magpie effects target fxc semantics
  (not compared against 2021 for the whole set).

| Effect | dxc | glslang | run |
|---|---|---|---|
| `ACNet.hlsl` | ok | ok | ok 0.511 ms |
| `Ani4Kv2_ArtCNN_C4F32_i2.hlsl` | ok | ok | ok 28.967 ms |
| `Anime4K/Anime4K_3D_AA_Upscale_US.hlsl` | ok | ok | ok 0.102 ms |
| `Anime4K/Anime4K_3D_Upscale_US.hlsl` | ok | ok | ok 0.095 ms |
| `Anime4K/Anime4K_Denoise_Bilateral_Mean.hlsl` | FAIL | ok | ok 0.035 ms |
| `Anime4K/Anime4K_Denoise_Bilateral_Median.hlsl` | FAIL | ok | ok 0.028 ms |
| `Anime4K/Anime4K_Denoise_Bilateral_Mode.hlsl` | FAIL | ok | ok 0.044 ms |
| `Anime4K/Anime4K_Restore_L.hlsl` | ok | ok | ok 0.436 ms |
| `Anime4K/Anime4K_Restore_M.hlsl` | ok | ok | ok 0.254 ms |
| `Anime4K/Anime4K_Restore_S.hlsl` | ok | ok | ok 0.123 ms |
| `Anime4K/Anime4K_Restore_Soft_L.hlsl` | ok | ok | ok 0.433 ms |
| `Anime4K/Anime4K_Restore_Soft_M.hlsl` | ok | ok | ok 0.255 ms |
| `Anime4K/Anime4K_Restore_Soft_S.hlsl` | ok | ok | ok 0.125 ms |
| `Anime4K/Anime4K_Restore_Soft_UL.hlsl` | ok | ok | ok 1.873 ms |
| `Anime4K/Anime4K_Restore_Soft_VL.hlsl` | ok | ok | ok 0.874 ms |
| `Anime4K/Anime4K_Restore_UL.hlsl` | ok | ok | ok 1.875 ms |
| `Anime4K/Anime4K_Restore_VL.hlsl` | ok | ok | ok 0.876 ms |
| `Anime4K/Anime4K_Thin_HQ.hlsl` | ok | ok | ok 0.108 ms |
| `Anime4K/Anime4K_Upscale_Denoise_L.hlsl` | ok | ok | ok 0.445 ms |
| `Anime4K/Anime4K_Upscale_Denoise_S.hlsl` | ok | ok | ok 0.158 ms |
| `Anime4K/Anime4K_Upscale_Denoise_UL.hlsl` | ok | ok | ok 1.645 ms |
| `Anime4K/Anime4K_Upscale_Denoise_VL.hlsl` | ok | ok | ok 0.864 ms |
| `Anime4K/Anime4K_Upscale_GAN_x2_M.hlsl` | ok | ok | ok 1.105 ms |
| `Anime4K/Anime4K_Upscale_GAN_x2_S.hlsl` | ok | ok | ok 0.619 ms |
| `Anime4K/Anime4K_Upscale_GAN_x3_L.hlsl` | ok | ok | ok 3.215 ms |
| `Anime4K/Anime4K_Upscale_L.hlsl` | ok | ok | ok 0.445 ms |
| `Anime4K/Anime4K_Upscale_S.hlsl` | ok | ok | ok 0.156 ms |
| `Anime4K/Anime4K_Upscale_UL.hlsl` | ok | ok | ok 1.649 ms |
| `Anime4K/Anime4K_Upscale_VL.hlsl` | ok | ok | ok 0.859 ms |
| `Bicubic.hlsl` | ok | ok | ok 0.079 ms |
| `Bilinear.hlsl` | ok | ok | ok 0.022 ms |
| `CAS/CAS.hlsl` | ok | ok | ok 0.016 ms |
| `CAS/CAS_Scaling.hlsl` | ok | ok | ok 0.076 ms |
| `CRT/CRT_Easymode.hlsl` | ok | ok | ok 0.130 ms |
| `CRT/CRT_Geom.hlsl` | ok | ok | ok 0.415 ms |
| `CRT/CRT_Hyllian.hlsl` | ok | ok | ok 0.194 ms |
| `CRT/CRT_Lottes.hlsl` | ok | ok | ok 0.612 ms |
| `CRT/GTU_v050.hlsl` | ok | ok | ok 0.219 ms |
| `CuNNy2/CuNNy-3x12-NVL.hlsl` | ok | ok | ok 0.537 ms |
| `CuNNy2/CuNNy-4x12-NVL.hlsl` | ok | ok | ok 0.667 ms |
| `CuNNy2/CuNNy-4x16-NVL.hlsl` | ok | ok | ok 1.079 ms |
| `CuNNy2/CuNNy-4x24-NVL.hlsl` | ok | ok | ok 2.297 ms |
| `CuNNy2/CuNNy-4x32-NVL.hlsl` | ok | ok | ok 4.021 ms |
| `CuNNy2/CuNNy-8x32-NVL.hlsl` | ok | ok | ok 7.390 ms |
| `CuNNy2/CuNNy-fast-NVL.hlsl` | ok | ok | ok 0.324 ms |
| `CuNNy2/CuNNy-faster-NVL.hlsl` | ok | ok | ok 0.209 ms |
| `CuNNy2/CuNNy-veryfast-NVL.hlsl` | ok | ok | ok 0.172 ms |
| `Deband.hlsl` | ok | ok | ok 0.123 ms |
| `FSR/FSR_EASU.hlsl` | ok | ok | ok 0.180 ms |
| `FSR/FSR_RCAS.hlsl` | ok | ok | ok 0.020 ms |
| `FSRCNNX/FSRCNNX.hlsl` | ok | ok | ok 0.353 ms |
| `FSRCNNX/FSRCNNX_LineArt.hlsl` | ok | ok | ok 0.354 ms |
| `FXAA/FXAA_High.hlsl` | ok | ok | ok 0.015 ms |
| `FXAA/FXAA_Medium.hlsl` | ok | ok | ok 0.020 ms |
| `FXAA/FXAA_Ultra.hlsl` | ok | ok | ok 0.020 ms |
| `ImageAdjustment.hlsl` | ok | ok | ok 0.020 ms |
| `Jinc.hlsl` | ok | ok | ok 0.231 ms |
| `k7_modernAnime_FHD_x2.hlsl` | ok | ok | ok 1.593 ms |
| `Lanczos.hlsl` | ok | ok | ok 0.277 ms |
| `Nearest.hlsl` | ok | ok | ok 0.027 ms |
| `NIS/NIS.hlsl` | ok | FAIL | FAIL |
| `NIS/NVSharpen.hlsl` | ok | FAIL | ok 0.025 ms |
| `NNEDI3/NNEDI3_nns128_win8x4.hlsl` | ok | ok | ok 3.318 ms |
| `NNEDI3/NNEDI3_nns128_win8x6.hlsl` | ok | ok | ok 4.454 ms |
| `NNEDI3/NNEDI3_nns16_win8x4.hlsl` | ok | ok | ok 0.441 ms |
| `NNEDI3/NNEDI3_nns16_win8x6.hlsl` | ok | ok | ok 0.600 ms |
| `NNEDI3/NNEDI3_nns256_win8x4.hlsl` | ok | ok | ok 6.497 ms |
| `NNEDI3/NNEDI3_nns256_win8x6.hlsl` | ok | ok | ok 8.737 ms |
| `NNEDI3/NNEDI3_nns32_win8x4.hlsl` | ok | ok | ok 0.840 ms |
| `NNEDI3/NNEDI3_nns32_win8x6.hlsl` | ok | ok | ok 1.141 ms |
| `NNEDI3/NNEDI3_nns64_win8x4.hlsl` | ok | ok | ok 1.665 ms |
| `NNEDI3/NNEDI3_nns64_win8x6.hlsl` | ok | ok | ok 2.280 ms |
| `Pixel Art/MMPX.hlsl` | ok | ok | ok 0.038 ms |
| `Pixel Art/Pixellate.hlsl` | ok | ok | ok 0.056 ms |
| `Pixel Art/SharpBilinear.hlsl` | ok | ok | ok 0.021 ms |
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
| `SGSR.hlsl` | ok | ok | ok 0.036 ms |
| `Sharpen/AdaptiveSharpen.hlsl` | ok | ok | ok 0.063 ms |
| `Sharpen/FineSharp.hlsl` | ok | FAIL | ok 0.066 ms |
| `Sharpen/LCAS.hlsl` | ok | ok | ok 0.132 ms |
| `Sharpen/LumaSharpen.hlsl` | ok | ok | ok 0.025 ms |
| `SMAA/SMAA_High.hlsl` | ok | FAIL | FAIL |
| `SMAA/SMAA_Low.hlsl` | ok | FAIL | FAIL |
| `SMAA/SMAA_Medium.hlsl` | ok | FAIL | FAIL |
| `SMAA/SMAA_Ultra.hlsl` | ok | FAIL | FAIL |
| `SSimDownscaler.hlsl` | ok | ok | ok 0.305 ms |
| `xBRZ/xBRZ_2x.hlsl` | ok | ok | ok 0.065 ms |
| `xBRZ/xBRZ_3x.hlsl` | ok | ok | ok 0.069 ms |
| `xBRZ/xBRZ_4x.hlsl` | ok | ok | ok 0.130 ms |
| `xBRZ/xBRZ_5x.hlsl` | ok | ok | ok 0.223 ms |
| `xBRZ/xBRZ_6x.hlsl` | ok | ok | ok 0.624 ms |
| `xBRZ/xBRZ_Freescale.hlsl` | ok | ok | ok 0.123 ms |

Totals over 117 effects: dxc: 114 ok, glslang: 105 ok, run: 86 ok
