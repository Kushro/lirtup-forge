# Scaling modes

A **mode** is how a window is processed, in order:

1. the Magpie effects **before the model**, on the captured image at the
   game's resolution (none by default);
2. the SRCNN model, which doubles the size (or quadruples it with two passes);
3. debanding, then the Magpie effects **after the model**, on the model's
   output;
4. the sampling filters that fit the result to the output size;
5. the finishing touches: CAS sharpening, bloom, vignette, film grain and a
   color LUT.

Each mode is a file in `~/.config/linux-rt-upscaler/modes/<id>.yaml`, and each
profile names one with `mode: <id>`. Several profiles can share a mode: the
editor says how many use it and offers to duplicate it before you change it.

Open *Scaling modes* in the drawer to see the list, with the GPU cost of each
mode and how many profiles use it, or *Edit mode* on a profile's *Scaling* tab.

To compare modes in the game, use the *Mode* chip of the toolbar over the game
([Controls](controls.md#trying-modes-in-the-game)): it changes the mode for the
session only, and *Save* keeps it in the profile.

## Try live

While a session is scaling with a mode, open that mode in the editor: the save
bar at the bottom shows *Try live*. Turn it on and every change you make in
the editor shows in the game at once, about 150 ms after you stop moving a
slider, without writing anything to disk. While it is on, the toolbar's
*Mode* chip over the game turns amber with a dot, the session card on *Home*
shows a *preview* badge, and the on-screen message says *Preview: unsaved
changes* (then *Preview ended*).

- *Save* writes the mode and ends the preview; the session keeps the saved
  settings.
- *Reset*, leaving the editor, closing the app or stopping the session end it,
  and the session goes back to the mode as saved.
- If the session switches to another mode (the *Mode* chip, a restart with
  another profile), the preview ends too.

Only the mode the session is using can be tried live. To try another one,
pick it first with the *Mode* chip over the game.

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

The effects of [Magpie](https://github.com/Blinue/Magpie) run through
MagpieFX, which runs Magpie's `.hlsl` files unmodified: the 117 bundled with
Magpie and any you add. Anime4K_Upscale_S, LumaSharpen and CRT_Easymode are
three of them; the separate «native» engine of 2.1 and earlier is gone.

The chain has two segments, with the model between them:

- **Before the model** (`magpie_effects_before_model`): on the captured image,
  at the game's resolution; the model gets the result. Cheap, since the image
  is small, and what the model sees changes (a sharpen or a denoise here is
  then upscaled).
- **After the model** (`magpie_effects`): on the model's output, before the
  final scaling. This is where effects went before, and where new ones go.

The row between the segments names the model and its scale. With the model
set to `none`, both segments run one after the other at the game's
resolution.

*Add effect* in each segment opens the catalog: the bundled effects (installed
in `/usr/share/lirtup/effects`) and yours, from
`~/.config/linux-rt-upscaler/effects/`. Drag the cards to reorder a segment,
or use their up and down buttons. The `swap_vert` button of a card moves it
to the end of the other segment; you can also drag a card onto a card of the
other segment (it goes before it) or onto the other segment's title (it goes
last). An effect can be in both segments; it then runs twice. Each card shows
its GPU time and its parameters; changing one applies at once, without
recompiling. If the session had to disable an effect, its card says so with
the error, in every segment where it appears.

The bundled effects come precompiled; the ones you add or edit compile the
first time (`dxc`, or `glslang` as fallback) and are then cached.

With a chain before the model, the scaler processes the whole frame instead
of only the tiles that changed (an effect that looks at neighbouring pixels
would leave seams between tiles).

### Modes from 2.1 with native effects

A mode file that still has settings of the retired native engine
(`effects_engine`, `anime4k_enabled`, `luma_sharpen_*`, `crt_*`) keeps
working: scaling applies them as the same MagpieFX effects, after the model.
The editor shows that chain read-only with a banner; *Migrate…* rewrites the
files (with a backup) so you can edit it here. See
[Migrating](migration.md).

Details, the compilers and the effects that need `dxc`:
[MagpieFX effects](magpiefx.md).

## Measure on my GPU

*Measure on my GPU* times every effect and the built-in frame generation on
your GPU and saves the results per GPU. The costs shown in the mode list and
the effect cards then come from your GPU instead of the reference numbers. The
measurement runs in its own process (`lirtup bench`), so the app keeps
responding, and *Cancel* stops it.

Every MagpieFX effect is measured at the size of the test image, which stands
for the captured window. An effect before the model costs what was measured;
one after the model runs on an image 2× larger per side (4× with *Double
upscale*, 1× without a model), so its card and the total count 4× (16×, 1×)
its measurement. This is an estimate: an effect that changes the size itself
(Anime4K_Upscale_S doubles it) is not followed through the rest of its
segment.

## Frame generation

LirtUp Forge presents through its own Vulkan swapchain, so the
[lsfg-vk](https://lsfg-vk.dev) frame generation can work on any captured
window: Vulkan, OpenGL, emulators or video. Turn it on in a profile's
*Presentation* tab, from the toolbar over the game, from the tray, with
`frame_gen_multiplier` in the configuration or with `--frame-gen N`. The
multiplier is the number of frames shown per captured frame: 2 to 5, or
*Auto*. It is read when scaling starts, so changing it restarts the session.
With the lsfg-vk layer, the multiplier chip of the toolbar cycles Off, ×2, ×3
and ×4; LirtUp Forge must be open to save the choice and restart the session
([Controls](controls.md)).

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
built-in presets) run last, on the output image. Debanding runs on the
model's output, before the effects after the model.
