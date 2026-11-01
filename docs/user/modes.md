# Scaling modes

A **mode** is how a window is processed, in order:

1. the SRCNN model, which doubles the size (or quadruples it with two passes);
2. the sampling filters that fit the result to the output size;
3. the Magpie effects;
4. the finishing touches: CAS sharpening, bloom, vignette, film grain and a
   color LUT, plus debanding, which runs on the source before the model.

Each mode is a file in `~/.config/linux-rt-upscaler/modes/<id>.yaml`, and each
profile names one with `mode: <id>`. Several profiles can share a mode: the
editor says how many use it and offers to duplicate it before you change it.

Open *Scaling modes* in the drawer to see the list, with the GPU cost of each
mode and how many profiles use it, or *Edit mode* on a profile's *Scaling* tab.

## Model

The slider goes from `none` (no neural network, only the filters) to `8x32`,
through `veryfast`, `faster`, `fast`, `3x12`, `4x12`, `4x16`, `4x24` and
`4x32`: faster models to the left, sharper ones to the right. They are
[CuNNy](https://github.com/funnyplanter/CuNNy) networks trained on visual
novels and illustrations.

*Double upscale* runs the model twice, for 4x. It costs about four times the
GPU time of one pass at the source size, so it suits small windows (800×600
to a 4K screen).

The scaler processes only the tiles of the frame that changed, so a mostly
still window costs less than a moving one. Compare models on a moving image;
`Alt`+`Shift`+`M` cycles through them while scaling.

## Sampling

The upsampler (Lanczos, FSR or NIS) and the downsampler (Catmull-Rom or
Lanczos) fit the image to the output size. Below them are the blur, the
anti-ringing strength and the Lanczos radius.

## Magpie effects

The effects of [Magpie](https://github.com/Blinue/Magpie) run through one of
two engines; only the selected one runs:

| | Native | MagpieFX |
| --- | --- | --- |
| How | Each effect ported by hand | Magpie's `.hlsl` files run unmodified by a generic loader |
| Effects | LumaSharpen, Anime4K_Upscale_S, CRT_Easymode | The 117 bundled with Magpie, plus any you add |
| Loading | Instant | Compiled on first use (`dxc`, or `glslang` as fallback), then cached |
| Settings | Tuned controls | Controls generated from each effect's parameters |

With MagpieFX, *Add effect* opens the catalog: the bundled effects (installed
in `/usr/share/lirtup/effects`) and yours, from
`~/.config/linux-rt-upscaler/effects/`. Drag the cards to reorder the chain,
or use their up and down buttons. Each card shows its GPU time and its
parameters; changing one applies at once, without recompiling.

Details, the compilers and the effects that need `dxc`:
[MagpieFX effects](magpiefx.md).

## Measure on my GPU

*Measure on my GPU* times every effect and the built-in frame generation on
your GPU and saves the results per GPU. The costs shown in the mode list and
the effect cards then come from your GPU instead of the reference numbers. The
measurement runs in its own process (`lirtup bench`), so the app keeps
responding, and *Cancel* stops it.

## Frame generation

LirtUp Forge presents through its own Vulkan swapchain, so the
[lsfg-vk](https://lsfg-vk.dev) frame generation can work on any captured
window: Vulkan, OpenGL, emulators or video. Turn it on in a profile's
*Presentation* tab, from the toolbar over the game, from the tray, with
`frame_gen_multiplier` in the configuration or with `--frame-gen N`. The
multiplier is the number of frames shown per captured frame: 2 to 5, or
*Auto*. It is read when scaling starts, so changing it restarts the session.

There are two engines:

| | lsfg-vk layer | Built in (experimental) |
| --- | --- | --- |
| Needs | lsfg-vk 2.x installed, and `lsfg-vk.dll` from the `lsfg-vk` branch of Lossless Scaling on Steam | `Lossless.dll` from Lossless Scaling (any branch); lsfg-vk is not needed |
| Where it runs | After the app presents | Inside the app, on its own Vulkan device on the same GPU |
| *Auto* | Runs at ×2 | Picks ×2 to ×5 from the game's frame rate and the display refresh |
| Stats panel | Does not see the generated frames (MangoHud does) | Shows them, with their latency |

The DLLs are found in your Steam libraries, or in the folder set in
`frame_gen_dll`. The app writes its own lsfg-vk profile to
`~/.config/linux-rt-upscaler/lsfg-vk.toml`; your `~/.config/lsfg-vk/conf.toml`
is not touched. If you set `LSFGVK_*` or `DISABLE_LSFGVK` yourself, the app
leaves the layer alone and says so in the *Presentation* tab.

Two settings trade quality for GPU time:

- *Flow scale* (default 1.0): the share of the output resolution at which
  lsfg estimates the motion. 0.5 to 0.75 is usually enough at 1440p and 4K.
- *Model*: *Quality* (LSFG 3.1) or *Performance* (LSFG 3.1P), lighter with
  more artifacts.

Frame generation smooths motion at the cost of about one captured frame of
latency (33 ms at 30 fps), and generated frames can smear HUD edges and fast
motion. With the built-in engine, the effects and frame generation share the
GPU time of each frame; with heavy effects the layer keeps more frames.

## Finishing touches

CAS sharpening, bloom, vignette, film grain and a color LUT (one of the
built-in presets) run last, on the output image. Debanding runs first, on the
captured image, before the model.
