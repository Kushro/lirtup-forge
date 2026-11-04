# Configuration

## Files

The GUI and the command line share these files, the same folder as 1.x:

| File | What |
| --- | --- |
| `~/.config/linux-rt-upscaler/config.yaml` | The default settings (*Default* in the GUI) and the profiles |
| `~/.config/linux-rt-upscaler/modes/<id>.yaml` | The scaling modes (new in 2.0) |
| `~/.config/linux-rt-upscaler/gui-config.yaml` | The settings of the GUI itself |
| `~/.config/linux-rt-upscaler/effects/` | Your MagpieFX effects |
| `~/.cache/linux-rt-upscaler/magpiefx/` | Compiled MagpieFX shaders |

You only need to write the options that differ from the defaults. Another file
can be used with `-c <path>` (`lirtup run -c`, `lirtup serve -c`,
`lirtup-gui -c`). Keys that 2.0 does not know are kept when the GUI saves, so
the file stays readable by other versions.

The commented [`config-example.yaml`](https://github.com/Kushro/lirtup-forge/blob/main/config-example.yaml)
describes every option.

### Settings precedence

From lowest to highest priority:

1. built-in defaults;
2. top-level options of `config.yaml`;
3. the scaling mode named by `mode:` (the profile's, else the top-level one);
4. the options of the profile that matches the window (or the one picked with
   `-p`);
5. command-line flags (`-m 4x24`, `--set key=value`…).

A profile can override a top-level option, and a flag can override both.
Hotkeys are merged key by key. A mode field written loose in a profile (for
example `model:` next to `mode:`) wins over the mode, as in 1.x files.

### Example

```yaml
# Options that apply to all windows (top-level, priority 2)
model: 8x32
double_upscale: true

# Profiles that override options for specific windows or setups (priority 3)
profiles:
  Full HD (1920×1080):
    match:
      width: <=1920
      height: <=1080
    options:
      model: 4x24
      double_upscale: false
```

With this configuration:

- Most windows use `8x32` and 4x upscaling (top-level options).
- Windows up to 1920x1080 (matching the profile) use `4x24` and 2x upscaling (profile options override top-level ones).

In 2.0 the model and `double_upscale` usually live in a [scaling mode](#scaling-modes)
instead; files written this way keep working.

## Profiles

Profiles allow you to define settings for specific windows or setups, matching their properties. They are defined at the `profiles` top-level option of the configuration file.

### Match rules

A profile in the configuration file may contain a `match` section with one or more of these keys:

| Key                | Description                                       |
| ------------------ |---------------------------------------------------|
| `title`            | Exact window title match (case-insensitive).      |
| `title_contains`   | Substring match in the title (case-insensitive).  |
| `title_startswith` | Title begins with this string (case-insensitive). |
| `title_endswith`   | Title ends with this string (case-insensitive).   |
| `title_regex`      | Regular expression match (case-insensitive).      |
| `width`            | Window width interval.                            |
| `height`           | Window height interval.                           |

For a window to match a profile, all rules in its `match` block must be satisfied (AND logic). If a window matches, and the profile has an `options` block, then all of those options are applied.

Profiles are checked top-to-bottom: if multiple profiles would match a window, only the first one will match.

Width and height rules accept the following interval syntax:

- Exact values: `1920`
- Comparisons: `<1280`, `>1024`, `<=1366`, `>=1920`
- Ranges: `1280-1920`, `720..1080`, `1024,1366`

### Example

Here, profiles are used to get the best possible tradeoff between framerate and quality, depending on the resolution of the source window.

For instance, upscaling more pixels takes longer, so you might choose a faster model for higher resolutions.

```yaml
profiles:
  VGA (640×480):
    match:
      width: <=640
      height: <=480
    options:
      model: 4x24
      double_upscale: true
  SVGA (800×600):
    match:
      width: <=800
      height: <=600
    options:
      model: 4x24
      double_upscale: true
  XGA (1024×768):
    match:
      width: <=1024
      height: <=768
    options:
      model: 4x16
      double_upscale: true
  HD (1280×720):
    match:
      width: <=1280
      height: <=720
    options:
      model: 4x16
      double_upscale: true
  Full HD (1920×1080):
    match:
      width: <=1920
      height: <=1080
    options:
      model: 4x24
      double_upscale: false
  QHD (2560×1440):
    match:
      width: <=2560
      height: <=1440
    options:
      model: 4x16
      double_upscale: false
```

## Overlay

### Overlay mode

The overlay is the window that shows the scaled image over the target window. It supports these modes:

| Mode               | Description                                                                                                                                                                                                  |
|--------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `always-on-top`    | Floating, borderless overlay that stays above all other windows **without taking focus**. Mouse events are forwarded to the target window. This is the default mode.                                         |
| `top-transparent`  | Same as `always-on-top`, but click-through. Mouse events pass through the overlay to the window underneath, and the overlay gets semi-transparent when the mouse isn't inside the source window coordinates. |
| `fullscreen`       | Frameless fullscreen window covering the entire monitor (or most of it, depending on the Window Manager).                                                                                                    |
| `windowed`         | Normal window with decorations and a fixed size.                                                                                                                                                             |

!!! note "Keyboard events"

    **Keyboard events are never forwarded**, regardless of overlay mode.

    If you need to use the keyboard (or any other peripheral input besides the mouse) in the target application, keep its window always focused. On a single monitor, `always-on-top` works well for this since the overlay cannot be focused, keeping the focus on the target window instead.

## Scaling modes

A mode file holds the processing fields: `model`, `double_upscale`, the
sampling options, the two MagpieFX chains and the finishing touches, plus a
`name` (and an optional `description`) for the GUI. The file name is the
mode's id, which `mode:` names:

```yaml
# ~/.config/linux-rt-upscaler/modes/anime-sharp.yaml
name: Anime sharp
model: 4x24
double_upscale: false
magpie_effects:
  - name: Sharpen/LumaSharpen
    params:
      sharpStrength: 0.8
```

```yaml
# config.yaml
mode: predeterminado      # the mode of every window without a profile
profiles:
  Visual novels:
    match:
      title_contains: "Novel"
    options:
      mode: anime-sharp
```

The GUI creates and edits these files in *Scaling modes*. A 1.x file has no
modes; the GUI offers to move its fields into modes the first time
([Migrating from 1.x](migration.md)).

## MagpieFX effects

Effects in Magpie's MagpieFX format run in two chains of the mode:

| Key | Runs on | Command line |
| --- | --- | --- |
| `magpie_effects_before_model` | The captured frame, at the game's resolution; the model gets the result | `--magpie-effect-before-model` |
| `magpie_effects` | The model's output, after debanding and before the final scaler | `--magpie-effect` |

The order of a frame:

```text
capture ─▶ [MagpieFX before the model] ─▶ model (SRCNN ×1/×2, or none)
        ─▶ [deband] ─▶ [MagpieFX after the model] ─▶ scaler ─▶ [CAS, bloom, vignette]
        ─▶ [LUT, grain] ─▶ screen
```

Both lists take the same entries: an effect name as `lirtup list effects`
shows it (`Sharpen/LumaSharpen`), `Name:param=value,...`, or the long form
`{name: ..., params: {...}}`; a path to a `.hlsl` file works too. Both are
layered like any mode field (a loose list in a profile replaces the mode's).
An effect may be in both lists: it runs twice. Without a model
(`model: none`) the two lists run one after the other.

With a chain before the model, tile processing (`use_tile_processing`) is off
for the session: the whole frame is upscaled every time, since an effect that
reads neighbouring pixels would leave seams between tiles.

```yaml
magpie_effects_before_model:
  - "Sharpen/LumaSharpen:sharpStrength=0.3"
magpie_effects:
  - CRT/CRT_Easymode
```

### The retired native effects engine

Up to 2.1 a mode chose between two engines: `effects_engine: native` (the
default; hand ports of three effects driven by loose fields) or
`effects_engine: magpiefx` (the `magpie_effects` chain); only the chosen one
ran. Now the MagpieFX chain is the only one, and those three effects run
through it.

Files that still have the old keys keep working without changes: when a
session starts, the options of the files (top level, mode and profile,
merged) are read with the old rule and become `magpie_effects`. With the
native engine, the effects that were on, in their old order, with the
settings that differ from the default:

| Old keys | Becomes |
| --- | --- |
| `anime4k_enabled: true` | `Anime4K/Anime4K_Upscale_S` |
| `luma_sharpen_enabled: true` (+ `luma_sharpen_strength`, `_clamp`, `_pattern`, `_offset_bias`) | `Sharpen/LumaSharpen` (`sharpStrength`, `sharpClamp`, `pattern`, `offsetBias`) |
| `crt_enabled: true` (+ the 17 `crt_*` settings) | `CRT/CRT_Easymode` (`sharpnessH`, `sharpnessV`, `maskStrength`, `maskDotWidth`, `maskDotHeight`, `maskStagger`, `maskSize`, `scanlineStrength`, `scanlineBeamWidthMin`/`Max`, `scanlineBrightMin`/`Max`, `scanlineCutoff`, `gammaInput`, `gammaOutput`, `brightBoost`, `dilation`) |

The native engine's `magpie_effects` list did not run, so it is dropped; with
`effects_engine: magpiefx` the list is kept and the native keys are ignored.
Values and ranges are the same on both sides, so the image is the same (up
to rounding). `lirtup migrate` and the GUI's migration dialog rewrite the
files without the old keys ([Migrating from 1.x](migration.md)); the
`--effects-engine`, `--enable-*` and `--crt-*`/`--luma-sharpen-*` flags are
gone, and an old key given with `--set` is ignored with a warning.

Two edges: a hand-written file with `magpie_effects` and no old key at all
used to have an inactive chain (the default engine was native) and now runs
it. And Magpie declares `scanlineStrength` with a step of 1: a value such as
`0.5` from an old `crt_scanline_strength` is kept and works, but the GUI's
slider only moves it to 0 or 1.

## GUI settings

`gui-config.yaml` holds the language, the theme and its seed color, the tray
options (`show_tray_icon`, `close_to_tray`, `minimize_to_tray`,
`start_hidden`), `keep_running_after_exit`, the delay of deferred scaling
(`start_delay_s`), and the zoom, fonts and refresh intervals of *Settings* ▸
*Style*. Edit them from the GUI.
