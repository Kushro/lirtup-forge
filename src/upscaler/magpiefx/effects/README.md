# Bundled MagpieFX effects

These files are copied unmodified from [Magpie](https://github.com/Blinue/Magpie)
(`src/Effects`, commit `241e0a447d1c45fc95781f9cd4b5a9198e738a3b`,
2026-10-28), keeping Magpie's folder layout. Magpie is licensed under the
GNU GPL v3 (see `LICENSE-Magpie`), the same license as this project. Each
effect keeps the attribution and license comments of its original authors.

Included: the 86 effects that do not use `//!SOURCE` lookup textures, plus
the `.hlsli` files they include (`FXAA/FXAA.hlsli`, `NIS/NIS_Scaler.hlsli`,
`NNEDI3/prescalers.hlsli`).

Not included, because the loader cannot read `.dds` textures yet: the 26
`RAVU` effects, the 4 `SMAA_*` effects and `NIS`. `StubDefs.hlsli` is an
IDE-only helper and is not needed either.

All 86 were compiled and run once on a 1280x720 frame with
`scripts/magpiefx-compile-all.py`: 83 with dxc, and the three
`Anime4K/Anime4K_Denoise_Bilateral_*` with the glslangValidator fallback
(dxc rejects their array sizes).

More effects can be dropped into `~/.config/linux-rt-upscaler/effects/`
(keep any `.hlsli` they include next to them).
