# Bundled MagpieFX effects

These files are copied unmodified from [Magpie](https://github.com/Blinue/Magpie)
(`src/Effects`, commit `241e0a447d1c45fc95781f9cd4b5a9198e738a3b`,
2026-10-28), keeping Magpie's folder layout. Magpie is licensed under the
GNU GPL v3 (see `LICENSE-Magpie`), the same license as this project. Each
effect keeps the attribution and license comments of its original authors.

Included: all 117 effects of that commit, with the `.hlsli` files they
include (`FXAA/FXAA.hlsli`, `NIS/NIS_Scaler.hlsli`,
`NNEDI3/prescalers.hlsli`, `RAVU/prescalers.hlsli`, `SMAA/SMAA.hlsli`) and
the `.dds` lookup textures that the 26 `RAVU` effects, the 4 `SMAA_*`
effects and `NIS` read with `//!SOURCE`. `StubDefs.hlsli` is an IDE-only
helper and is not needed.

The first 86 (those without lookup textures) were compiled and run once on
a 1280x720 frame with `scripts/magpiefx-compile-all.py`: 83 with dxc, and
the three `Anime4K/Anime4K_Denoise_Bilateral_*` with the glslangValidator
fallback (dxc rejects their array sizes). The 31 that use `.dds` files were
added in round 1.3 and still have to go through that check.

More effects can be dropped into `~/.config/linux-rt-upscaler/effects/`
(keep any `.hlsli` they include and any `//!SOURCE` file they read next to
them).
