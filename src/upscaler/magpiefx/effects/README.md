# Bundled MagpieFX effects

These `.hlsl` files are copied unmodified from
[Magpie](https://github.com/Blinue/Magpie) (`src/Effects`, commit
`241e0a447d1c45fc95781f9cd4b5a9198e738a3b`, 2026-10-28). Magpie is licensed
under the GNU GPL v3 (see `LICENSE-Magpie`), the same license as this
project. Each effect keeps the attribution comments of its original authors.

| File | Origin |
|---|---|
| `Sharpen/LumaSharpen.hlsl` | CeeJay.dk's LumaSharpen (SweetFX), ported by Magpie |
| `Anime4K/Anime4K_Upscale_S.hlsl` | bloc97's Anime4K `Upscale_CNN_x2_S`, ported by Magpie |
| `CRT/CRT_Easymode.hlsl` | libretro `crt-easymode`, ported by Magpie |

More effects can be dropped into `~/.config/linux-rt-upscaler/effects/`
(keep any `.hlsli` they include next to them).
