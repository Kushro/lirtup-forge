<p align="center">
  <img src="data/icons/hicolor/scalable/apps/io.github.baronsmv.linux-rt-upscaler.svg" width="128" alt="icon">
</p>

<h1 align="center">LirtUp Forge</h1>

<p align="center">
  <a href="https://www.gnu.org/licenses/gpl-3.0"><img src="https://img.shields.io/badge/License-GPLv3-blue.svg" alt="License: GPLv3"></a>
</p>

<p align="center">
  A fork of <a href="https://github.com/baronsmv/linux-rt-upscaler">Linux Real-Time Upscaler</a> by <a href="https://github.com/baronsmv">baronsmv</a>.
</p>

**LirtUp** = **Li**nux **R**eal-**T**ime **Up**scaler. It upscales any X11 or
XWayland window to full screen in real time, with
[CuNNy](https://github.com/funnyplanter/CuNNy) neural networks running on
Vulkan compute, while you keep playing: mouse clicks and motion go to the
original window. On top of that it runs the effects of
[Magpie](https://github.com/Blinue/Magpie) and frame generation with
[lsfg-vk](https://lsfg-vk.dev).

**Version 2.0 is a rewrite in Rust** with a new interface. It replaces the
Python 1.x under the same package name and reads its configuration; see
[Migrating from 1.x](docs/user/migration.md).

## Results at 400% magnification

![](docs/assets/comparisons/gurikaji/w40-60_h20-50_4x_comparison.png)

![](docs/assets/comparisons/fatamoru/w30-50_h10-50_4x_comparison.png)

## Features

- **Neural network upscaling** with 9 SRCNN models trained for visual novels
  and illustrations, from `veryfast` to `8x32`, and 4x with two passes.
- **Scaling modes**: the model, the sampling filters, the Magpie effects and
  the finishing touches (CAS, debanding, bloom, vignette, grain, LUT) in one
  place, shared by the profiles that use them.
- **Profiles** that match windows by title and size, each with its mode and
  presentation settings.
- **The 117 Magpie effects**, run unmodified by a MagpieFX loader, or three of
  them ported by hand. *Measure on my GPU* times them on your hardware.
- **Frame generation** ×2 to ×5 or automatic, through the lsfg-vk layer or
  built in.
- **A toolbar over the game** that appears at the top edge: stats, crop,
  screenshot, frame generation, window or fullscreen, stop.
- **Stats panel**, like mpv's: frame rates, latency, GPU time per effect, and
  a warning when the effects cannot keep up.
- **Tray icon and daemon**: scale the windows that match a profile as they
  appear, with the main window closed.
- **Tile-based processing**: only the parts of the frame that change are
  upscaled, so still scenes cost little.
- **Menu bar detection** for Windows programs under Wine, crop, offset, zoom
  and output geometry (fit, stretch, cover).
- English and Spanish.

## Requirements

- GNU/Linux with X11, or Wayland with XWayland.
- A Vulkan GPU (AMD, NVIDIA or Intel).
- WebKitGTK 4.1 and GTK 3, for the interface.
- Optional: `dxc` or `glslang` for the MagpieFX effects, lsfg-vk or Lossless
  Scaling for frame generation.

## Installation

- **Arch Linux and derivatives**: the PKGBUILD in [`packaging/aur`](packaging/aur)
  (`makepkg -si`), package `linux-rt-upscaler`.
- **Any distribution**: the AppImage.
- **From source**: `scripts/build-release-linux.sh` builds a tarball and the
  AppImage.

Details, dependencies per distribution and the build requirements:
[Installation](docs/user/installation.md).

## Quick start

1. Open *LirtUp Forge* from the menu, or run `lirtup-gui`.
2. Click a window in *Open windows*, or focus the game and press
   `Alt`+`Shift`+`A`.
3. `Alt`+`Shift`+`Escape` stops scaling.

From a terminal, `lirtup run` does the same as the 1.x `upscale`:

```sh
lirtup run -s                  # pick the window from a list
lirtup run -t "My game" -m 4x24
lirtup run -- wine game.exe    # launch a program and scale its window
```

## Guides

- [Installation](docs/user/installation.md)
- [Migrating from 1.x](docs/user/migration.md)
- [Graphical interface](docs/user/gui.md)
- [Scaling modes](docs/user/modes.md), with frame generation
- [Controls](docs/user/controls.md): hotkeys, the toolbar over the game, the stats panel
- [Tray and daemon](docs/user/tray-daemon.md)
- [Command line](docs/user/cli.md)
- [Configuration](docs/user/configuration.md)
- [MagpieFX effects](docs/user/magpiefx.md)
- [Troubleshooting](docs/user/troubleshooting.md)

## Known limitations

- **Vulkan games are captured black under KWin with XWayland.** On a Plasma
  Wayland session, windows that present with Vulkan through XWayland (DXVK
  and VKD3D-Proton games, native Vulkan games on X11) come out black. The
  1.x behaves the same way. OpenGL and plain X11 windows are not affected.
  A Plasma X11 session works; see [Troubleshooting](docs/user/troubleshooting.md#vulkan-games-are-captured-black-under-kwin-with-xwayland).
- **Native Wayland windows** cannot be captured, only X11 and XWayland ones.
- **The keyboard is not forwarded**: keep the focus on the game. The
  `always-on-top` overlay never takes it.

## Changes from 1.x

Most of the 1.x carries over: the configuration folder and format, the CLI
options, the hotkeys and the profiles. What changed on purpose:

| 1.x | 2.0 |
| --- | --- |
| Python, PySide6 and C++ extensions; `pipx install` | Rust, with a Dioxus interface; AUR package and AppImage |
| `upscale`, `upscale-gui` | `lirtup run`, `lirtup-gui` |
| Model and effects in each profile | Scaling modes shared by profiles; the GUI offers to migrate (`config.yaml.1x.bak` is kept) |
| Scaling with unsaved settings | The scaler reads the saved files |
| — | A toolbar over the game, *Measure on my GPU*, a daemon that keeps running after the GUI quits |
| Python API (`upscaler.api`) | No Python API; `lirtup serve` has a socket protocol |
| `--list-*`, `--help-all`, `-v` for the version | `lirtup list …`, `--help`, `-V` |
| A value out of range stops with code 2 | A warning, and the default value is used |
| Mouse forwarding with XTest or `SendEvent` | `SendEvent` only |
| The launched program gets `__GLX_VENDOR_LIBRARY_NAME=nvidia` | Not set (it breaks OpenGL on AMD and Intel) |

The full list: [Migrating from 1.x](docs/user/migration.md#3-what-changed-on-purpose).

## Magpie effects

The effects of [Magpie](https://github.com/Blinue/Magpie) (Windows) run through
one of two engines, picked in the mode editor, with `effects_engine` or with
`--effects-engine`:

| | Native | MagpieFX |
| --- | --- | --- |
| How | Each effect ported by hand | Magpie's `.hlsl` files run unmodified by a generic loader |
| Effects | LumaSharpen, Anime4K_Upscale_S, CRT_Easymode | All 117 of Magpie, plus any you add to `~/.config/linux-rt-upscaler/effects` |
| Loading | Instant | Compiled on first load (`dxc`, or `glslang`), then cached |
| Settings | Tuned controls | Generated from each effect's parameters |

Reference GPU time (ms/frame, 1280x720 to 2560x1440, AMD RX 9070 XT, RADV):

| Effect | Native | MagpieFX |
| --- | --- | --- |
| LumaSharpen | 0.016 | 0.016 |
| Anime4K_Upscale_S | 0.145 | 0.142 |
| CRT_Easymode | 0.122 | 0.118 |

The effects are bundled unmodified under GPLv3 (Magpie commit `241e0a44`).
Details: [MagpieFX guide](docs/user/magpiefx.md) and
[compatibility report](docs/developer/magpiefx-report.md).

## Frame generation

LirtUp Forge presents through its own Vulkan swapchain, so lsfg-vk can
generate frames for any captured window: Vulkan, OpenGL, emulators or video.
Two engines:

- **the lsfg-vk layer**: needs lsfg-vk 2.x and `lsfg-vk.dll` from the
  `lsfg-vk` branch of Lossless Scaling on Steam;
- **built in** (experimental): runs the lsfg core inside the app with
  `Lossless.dll` from Lossless Scaling, paces the frames itself, and picks the
  multiplier on its own with *Auto*.

See [Scaling modes](docs/user/modes.md#frame-generation).

## Acknowledgments

This project stands on the shoulders of several open-source works, maintained
by amazing people:

- **[baronsmv](https://github.com/baronsmv)**, author of [Linux Real-Time Upscaler](https://github.com/baronsmv/linux-rt-upscaler), the project LirtUp Forge is forked from.
- **[Blinue](https://github.com/Blinue)** and the contributors of [Magpie](https://github.com/Blinue/Magpie), whose MagpieFX format and effects are bundled here (GPLv3), and the authors of each effect, credited in its source.
- **[funnyplanter](https://github.com/funnyplanter)**, for the [CuNNy](https://github.com/funnyplanter/CuNNy) neural network upscaling models.
- **[PancakeTAS](https://github.com/PancakeTAS)** and the contributors of [lsfg-vk](https://github.com/PancakeTAS/lsfg-vk), and Enigmata Temple, whose lsfg core (MIT) and shader loader (GPL-3.0) the built-in frame generation uses.
- **[L65536](https://github.com/L65536)**, for the original [RealTimeSuperResolutionScreenUpscalerforLinux](https://github.com/L65536/RealTimeSuperResolutionScreenUpscalerforLinux), the proof of concept of real-time SRCNN upscaling on Linux.
- **[Compushady](https://github.com/rdeioris/compushady)**, the Vulkan foundation of the early versions.
- **[Dioxus](https://dioxuslabs.com)**, the interface; **[ash](https://github.com/ash-rs/ash)** and **[x11rb](https://github.com/psychon/x11rb)**, Vulkan and X11 from Rust; **[Tailwind CSS](https://tailwindcss.com)**; and the Google Sans Flex, JetBrains Mono and Material Symbols fonts.
