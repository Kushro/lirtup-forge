<p align="center">
  <img src="https://raw.githubusercontent.com/baronsmv/linux-rt-upscaler/main/data/icons/hicolor/scalable/apps/io.github.baronsmv.linux-rt-upscaler.svg" width="128" alt="icon">
</p>

<h1 align="center">LirtUp Forge</h1>

<p align="center">
  <a href="https://www.gnu.org/licenses/gpl-3.0"><img src="https://img.shields.io/badge/License-GPLv3-blue.svg" alt="License: GPLv3"></a>
</p>

<p align="center">
  A fork of <a href="https://github.com/baronsmv/linux-rt-upscaler">Linux Real-Time Upscaler</a> by <a href="https://github.com/baronsmv">baronsmv</a>.
</p>

**LirtUp** = **Li**nux **R**eal-**T**ime **Up**scaler. This fork adds the
[Magpie effects](#magpie-effects): you can run them on a few hand-ported
shaders or on 86 of Magpie's effects loaded unmodified.

A real-time SRCNN upscaler for any X-Window (X11 or XWayland) on GNU/Linux. It uses [CuNNy](https://github.com/funnyplanter/CuNNy) neural networks to perform 2x (or 4x) upscaling to full screen while preserving aspect ratio. Mouse clicks and motion are automatically forwarded to the original window.

![](https://raw.githubusercontent.com/baronsmv/linux-rt-upscaler/main/docs/assets/screenshots/dark_02.png)

## Results at 400% magnification

![](https://raw.githubusercontent.com/baronsmv/linux-rt-upscaler/main/docs/assets/comparisons/gurikaji/w40-60_h20-50_4x_comparison.png)

![](https://raw.githubusercontent.com/baronsmv/linux-rt-upscaler/main/docs/assets/comparisons/fatamoru/w30-50_h10-50_4x_comparison.png)

## Features

- **Neural-Network upscaling** using SRCNNs trained for high-quality upscaling of visual novels and illustrations.
- **Model selection** from 9 variants with variable quality/performance trade-offs.
- **Hardware accelerated** using Vulkan compute.
- **Tile-Based processing** that upscales only the frame regions that change, reducing GPU load for mostly static content.
- **Customizable output geometry**: scaling mode (fit, stretch, cover), offset, crop and zoom.
- **Input forwarding** as if interacting directly with the original window.
- **System tray** controls: restart, switch the upscaled window or change its profile without opening the main window ([GUI guide](docs/user/gui.md#system-tray)).
- **Menu bar detection** (experimental): crops the menu bar that Windows programs draw below the title bar under Wine (`--auto-crop-menu-bar`, or *Presentation* > *Crop* > *Detect menu bar*).
- **Stats panel** (experimental), like mpv's stats page: frame rates, GPU times and the active processing chain, toggled with Alt+Shift+I.
- **Frame generation** through [lsfg-vk](https://lsfg-vk.dev) (×2 to ×5), see [below](#frame-generation).
- **Magpie effects** with a choice of engine: a few effects ported by hand, or 86 effects from [Magpie](https://github.com/Blinue/Magpie) run unmodified by a MagpieFX loader (see below).

## Requirements

- **OS:** GNU/Linux
- **Display server:** X11, or Wayland with XWayland
- **GPU:** Vulkan-capable (NVIDIA, AMD, Intel)
- **Python:** 3.10 or newer
- **Qt dependencies:** see [Dependencies](https://baronsmv.github.io/linux-rt-upscaler/user/installation/#dependencies)

## Installation

LirtUp Forge is not published on PyPI (`pipx install linux-rt-upscaler`
installs the original project). Install it from this repository with `pipx`:

```sh
pipx install git+https://github.com/Kushro/lirtup-forge
```

This builds the native Vulkan and capture extensions, so you need a C++
compiler and the Vulkan and XCB development headers. The MagpieFX engine also
needs `dxc` (`directx-shader-compiler`) or `glslangValidator` at runtime.
For the other dependencies and how to install `pipx`, see the original
[Installation Guide](https://baronsmv.github.io/linux-rt-upscaler/user/installation/).

After installation, the `upscale` and `upscale-gui` commands will be available globally. You can also create a [desktop entry](https://baronsmv.github.io/linux-rt-upscaler/user/installation/#desktop-integration-optional) of the GUI for easy access. 

## Guides and Docs

The guides of the original project apply to LirtUp Forge, except for the
Magpie effects, which are documented [here](#magpie-effects).


- [Home](https://baronsmv.github.io/linux-rt-upscaler/)
- [Installation](https://baronsmv.github.io/linux-rt-upscaler/user/installation/)
- [GUI Usage](https://baronsmv.github.io/linux-rt-upscaler/user/gui/)
- [CLI Options](https://baronsmv.github.io/linux-rt-upscaler/user/cli/)
- [Controls](https://baronsmv.github.io/linux-rt-upscaler/user/controls/)
- [Configuration](https://baronsmv.github.io/linux-rt-upscaler/user/configuration/)
- [API Reference](https://baronsmv.github.io/linux-rt-upscaler/api/overview/)

## Magpie effects

The effects of [Magpie](https://github.com/Blinue/Magpie) (Windows) run here
through two engines. You pick one in the GUI (*Effects* tab > *Engine*), with
`effects_engine` in the config or with `--effects-engine` on the CLI. Only the
selected engine runs.

| | Native | MagpieFX |
|---|---|---|
| How | Each effect ported by hand to a fixed shader class | Magpie's `.hlsl` files run unmodified by a generic loader |
| Effects | 3 | 86 bundled, plus any you add to `~/.config/linux-rt-upscaler/effects` |
| Loading | Precompiled, instant | Compiled on first load (dxc, or glslangValidator as fallback), then cached |
| Out-of-range writes | Bounds-checked | Rely on the driver discarding them |
| Settings | Tuned controls | Controls generated from each effect's parameters |

Reference GPU time (ms/frame, 1280x720 to 2560x1440, AMD RX 9070 XT, RADV,
dxc), measured without a window:

| Effect | Native | MagpieFX |
|---|---|---|
| LumaSharpen | 0.016 | 0.016 |
| Anime4K_Upscale_S | 0.145 | 0.142 |
| CRT_Easymode | 0.122 | 0.118 |

Details: [MagpieFX guide](docs/user/magpiefx.md) and
[compatibility report](docs/developer/magpiefx-report.md).

### Status

**Ported by hand (Native engine)**

- `LumaSharpen`, `Anime4K_Upscale_S` (4 passes) and `CRT_Easymode`, each with
  its config, CLI options and GUI controls.
- Adaptations: frames arrive as BGRA, so the shaders swap red and blue when
  they read and write (the Magpie code itself is unchanged). Anime4K's 2x2
  writes are bounds-checked.

**Adapted (MagpieFX engine)**

- A loader for the MagpieFX format (version 4). It reads the `//!`
  directives, generates one compute shader per pass (CS and PS styles,
  multiple outputs, `MulAdd`, `Rmp8x8`, the `MP_*` macros) and compiles them
  to SPIR-V at runtime with a cache. It also runs the passes with their
  intermediate textures.
- The red/blue swap is built into the generated shaders, at no measurable
  cost. Only the 3 FXAA effects still need a separate swap pass.
- 86 effects from Magpie's `src/Effects` (commit `241e0a44`) are bundled
  unmodified under GPLv3.
- Changing a parameter in the GUI updates the running effect without
  recompiling it.

**Shared changes**

- An `effects_engine` setting, plus a GUI section with the engine selector,
  its pros and cons and the reference timings above.
- Fix for GPU timestamps in `Compute.dispatch_sequence`. The readback could
  block forever, and the conversion to milliseconds was only accurate to
  about 1 ms.
- Spanish translations for the new GUI strings.

**Verified**

- All 86 bundled effects compile and run once on a 1280x720 frame: 83 with
  dxc, and the three `Anime4K_Denoise_Bilateral_*` with glslang (dxc rejects
  their array sizes).
- Both engines give correct colors, checked in the running app with a
  red/green/blue test image. Their output matches the reference
  bit for bit.
- Both engines were measured in the running app on their own branches, before
  they were merged. Merging them changed none of the numbers above (measured
  without a window).
- Picking an engine turns the other one off. A parameter change applies in
  about 0.02 ms without recompiling.
- The built wheel contains the 86 effects, their license and the Native
  shaders.

**Not included**

- The 31 Magpie effects that need `.dds` lookup textures: the 26 `RAVU`
  effects, the 4 `SMAA` effects and `NIS`.

**Not tested yet**

- The GUI and the engine selector in the running app on the merged branch
  (only checked without a window).
- GPUs other than AMD RADV (NVIDIA, Intel), and sources at odd sizes on
  drivers that do not discard out-of-range writes.
- How the MagpieFX engine behaves on a system without `glslangValidator` (the
  three Denoise effects should fail with a clear error).

**Pending**

- Reading `.dds` textures, to support RAVU, SMAA and NIS.
- Enabling `robustImageAccess` so out-of-range writes are well defined on
  every driver.
- A "Measure on my GPU" button in the GUI, to replace the fixed reference
  numbers.
- FP16 (`//!CAPABILITY FP16`), which is currently ignored.
- More native ports, where the effort pays off.

## Stats panel

An experimental panel in the top-left corner of the overlay, in the spirit of
mpv's stats page. Show or hide it with **Alt+Shift+I** (the `toggle_stats`
hotkey), start with it visible with `--show-stats` or `show_stats: true`, or
tick *Extras* > *Stats panel* in the GUI. It refreshes twice per second and
shows:

- **Game**: new frames per second from the target window, and how often the
  window is polled.
- **Shown**: frames presented per second, and the CPU time of a full render.
- **Frame gen**: the lsfg-vk multiplier and an estimate of the output rate.
  The frames lsfg-vk adds are generated after the app presents, so they are
  not counted here; MangoHud (`MANGOHUD=1`) measures them.
- **Source**, **Upscale**, **Effects** (each with its GPU time), **Output**,
  **Post**, **Present** and **Profile**: the processing chain in order.

While the panel is visible, the Magpie effects are timed with GPU timestamp
queries, which cost a little. With frame generation on, each panel refresh
presents one extra frame.

## Frame generation

LirtUp Forge presents through its own Vulkan swapchain, so the
[lsfg-vk](https://lsfg-vk.dev) layer (Lossless Scaling Frame Generation on
Linux) can generate frames on top of it, for any captured window: Vulkan,
OpenGL, emulators or video. Turn it on in the GUI (*Presentation* tab >
*Frame generation*), with `frame_gen_multiplier` in the config or with
`--frame-gen N` on the CLI. It is read at startup, so changes need a restart.

What it needs:

- lsfg-vk 2.x installed (its implicit Vulkan layer).
- `lsfg-vk.dll`, which comes with the `lsfg-vk` branch of Lossless Scaling on
  Steam (*Properties* > *Betas*). It is found in the Steam libraries, or in the
  folder given by `frame_gen_dll`. The regular `Lossless.dll` does not work
  with lsfg-vk 2.x.

How it works: the app writes its own lsfg-vk profile to
`~/.config/linux-rt-upscaler/lsfg-vk.toml` and points the layer to it before
the first Vulkan call. Your `~/.config/lsfg-vk/conf.toml` is not touched. With
frame generation on, the app presents only new frames of the target window
(`present_on_change_only`), so lsfg-vk never interpolates between identical
frames. If you set `LSFGVK_*` or `DISABLE_LSFGVK` yourself, the app leaves the
layer alone.

Trade-offs: smoother motion, at the cost of about one captured frame of
latency (33 ms at 30 fps, 17 ms at 60 fps), and generated frames can smear HUD
edges and fast motion.

Status: checked by eye on an RX 9070 XT (RADV) with a 30 fps video, where ×2
and ×4 looked much smoother. Not measured yet: latency, the frame rate that
reaches the screen, real games and other GPUs. Analysis and test notes:
[frame generation analysis](docs/developer/frame-generation-analysis.md).

## Future plans

- [ ] Broader model support. The upscaler currently uses [CuNNy](https://github.com/funnyplanter/CuNNy) models. Alternative SRCNN architectures are welcome if one offers a clear quality/performance advantage.
- [ ] ~~Native Wayland upscaling~~ (not planned: see [Architecture Limitations](https://baronsmv.github.io/linux-rt-upscaler/developer/architecture/limitations/#native-wayland-is-not-supported))

## Acknowledgments

This project stands on the shoulders of several open-source works, mantained by amazing people:

- **[baronsmv](https://github.com/baronsmv)**, author of [Linux Real-Time Upscaler](https://github.com/baronsmv/linux-rt-upscaler), the project LirtUp Forge is forked from.
- **[Blinue](https://github.com/Blinue)** and the contributors of [Magpie](https://github.com/Blinue/Magpie), whose MagpieFX format and effects are bundled here (GPLv3), and the authors of each effect, credited in its source.

- **[L65536](https://github.com/L65536)**, for the original [RealTimeSuperResolutionScreenUpscalerforLinux](https://github.com/L65536/RealTimeSuperResolutionScreenUpscalerforLinux), which demonstrated the feasibility of real-time SRCNN upscaling on Linux and served as a proof-of-concept for this project.
- **[funnyplanter](https://github.com/funnyplanter)**, for the incredible [CuNNy](https://github.com/funnyplanter/CuNNy) neural network upscaling models.
- **[Compushady](https://github.com/rdeioris/compushady)**, which served as an invaluable Vulkan foundation during early development.
- **[PySide6](https://pypi.org/project/PySide6/)**, the Qt binding that powers the entire graphical interface and overlay window.
- **[xcffib](https://github.com/tych0/xcffib)**, the XCB binding used for window management, monitor querying and event forwarding.
- **[Pillow](https://python-pillow.github.io/)**, the Python Imaging Library, used for saving screenshots and rendering OSD messages.
- **[PyYAML](https://github.com/yaml/pyyaml)**, the YAML parser used for configuration file operations.
