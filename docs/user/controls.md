# Controls

## Hotkeys

The hotkeys are global: they work while the game has the focus. Change them in
*Settings* ▸ *Hotkeys*, or under `hotkeys:` in the
[configuration](configuration.md).

| Shortcut | Action | Key in `hotkeys:` |
| --- | --- | --- |
| `Alt`+`Shift`+`A` | Scale the focused window, fullscreen | `start_fullscreen` |
| `Alt`+`Shift`+`Q` | Scale the focused window, in a window | `start_windowed` |
| `Alt`+`Shift`+`S` | Pause or resume scaling | `toggle_scaling` |
| `Alt`+`Shift`+`Escape` | Stop scaling | `exit_app` |
| `Alt`+`Shift`+`D` | Show or hide the toolbar over the game | `toggle_toolbar` |
| `Alt`+`Shift`+`I` | Show or hide the stats panel | `toggle_stats` |
| `Alt`+`Shift`+`P` | Screenshot of the scaled image | `screenshot` |
| `Alt`+`Shift`+`M` | Next SRCNN model | `cycle_model` |
| `Alt`+`Shift`+`G` | Next output geometry (fit, stretch, cover) | `cycle_geometry` |
| `Alt`+`Shift`+`+` / `-` | Zoom in or out | `zoom_in`, `zoom_out` |
| `Alt`+`Shift`+`↑` `↓` `←` `→` | Move the image | `offset_up`, `offset_down`, `offset_left`, `offset_right` |
| `Alt`+`Shift`+`R` | Restore the geometry, zoom and offset | `restore_view` |
| `Alt`+`Shift`+`N` | Next scaling mode, like the toolbar's *Mode* chip | `mode_next` |
| `Alt`+`Shift`+`B` | Previous scaling mode | `mode_prev` |
| `Alt`+`Shift`+`X` | Swap between the current settings and the previous ones ([A/B](#ab-swap)) | `swap_ab` |
| `Alt`+`Shift`+`C` | Start or end the [split comparison](#split-comparison) | `toggle_compare` |

`start_fullscreen`, `start_windowed` and `toggle_toolbar` are new in 2.0;
`mode_next`, `mode_prev`, `swap_ab` and `toggle_compare` are new after 2.1.0;
the rest keep the 1.x bindings. In *Settings* ▸ *Hotkeys* the two mode
hotkeys are listed under *View*, next to the model, and the A/B and split
comparison ones under *Compare*.

Hotkeys are written as modifiers and a key joined by `+`. The modifiers are
`Ctrl`, `Alt`, `Shift` and `Super`; keys follow the X11 key names (`A`,
`Space`, `Return`, `Escape`, `Left`, `Plus`, `Minus`, `F5`…), in any case:

```yaml
hotkeys:
  toggle_toolbar: "Ctrl+Alt+T"
  screenshot: "Super+Print"
```

While you record a new hotkey in *Settings*, the scaler releases its own, so
you can pick a combination it is using.

!!! note "Keyboard and mouse"

    Mouse clicks and motion on the scaled image are forwarded to the original
    window. **The keyboard is never forwarded**: keep the focus on the game.
    The `always-on-top` overlay mode never takes the focus, so it suits a
    single monitor.

## Toolbar over the game

Move the cursor to the top edge of the scaled image and a toolbar appears; it
hides again 2 seconds after the cursor leaves it (`Alt`+`Shift`+`D` shows or
hides it at any time). From left to right:

| Button | Action |
| --- | --- |
| Pin | Keep the toolbar visible |
| Stats | Show or hide the stats panel |
| Crop | Detect the menu bar again, or apply the profile's crop |
| Screenshot | Same as `Alt`+`Shift`+`P` |
| Mode | The scaling mode: a click or the wheel down picks the next one, a right click or the wheel up the previous one. See [trying modes](#trying-modes-in-the-game) |
| Save | Shown next to *Mode* while it is not the profile's: saves it in the profile |
| Multiplier | Next frame generation multiplier, saved in the configuration. See [the multiplier](#the-multiplier) |
| Compare | Start or end the [split comparison](#split-comparison); highlighted while it is on |
| Hide | Hide the toolbar until the cursor comes back to the edge |
| To window / fullscreen | Switch between the two |
| Stop | Stop scaling |

The toolbar also shows the frame rate on screen.

In window mode, drag the toolbar to move the window. Settings changed from the
toolbar are saved by the GUI when it is open; otherwise they last until the
session ends.

### Trying modes in the game

The *Mode* chip goes through your [scaling modes](modes.md) without leaving
the game, which helps to compare them on the same scene. The change applies a
moment after the last click, so you can skip several modes quickly.

- **It is temporary.** The mode you pick lasts until the session ends. It is
  not written anywhere: the next session uses the profile's mode again. The
  chip turns blue while the mode is not the profile's, and Inicio shows it as
  *(temporary)*.
- **Save** (next to the chip, or *Save in …* on Inicio) writes `mode: <id>` in
  the session's profile, or in *Default* when the session has no profile. The
  GUI does the writing, so it has to be open; the on-screen message confirms
  when the profile has it.
- The profile's own settings still apply on top of the mode you pick, as they
  do for the profile's mode, so what you see is what saving gives.
- A mode with Magpie effects that were never compiled can freeze the image for
  a few seconds the first time; the on-screen message says *preparing
  effects*. After that they are cached ([Magpie effects](magpiefx.md)).
- `Alt`+`Shift`+`N` and `Alt`+`Shift`+`B` do the same as a click and a right
  click on the chip, with the same short wait, also while the toolbar is
  hidden.
- **With a single mode** there is nothing to go through: the chip (or the
  hotkeys) only shows *Only one mode: create another in LirtUp Forge*. Create
  more in the GUI's [Modes](modes.md) page.
- While you edit a mode in the GUI with *Try live* on, the chip shows a dot
  and the session uses your unsaved changes; see [modes](modes.md).
- If the message adds *loose settings in config.yaml shadow it (lirtup
  migrate)*, `config.yaml` still has mode settings outside any mode (for
  example a top-level `magpie_effects` left from 1.x), and those win over the
  mode. Run `lirtup migrate` (or accept the GUI's offer) to move them into a
  mode, or delete them.

### The multiplier

What the *Multiplier* button does depends on the frame generation engine:

- **Built-in engine.** Each click picks the next multiplier (off, ×2, ×3, ×4,
  *Auto*) and the session uses it at once.
- **lsfg-vk layer.** The layer reads the multiplier only when the game starts,
  so a click goes Off → ×2 → ×3 → ×4 → Off (there is no *Auto*, which needs
  the built-in engine). The button shows the new value right away and the
  on-screen message says *Frame generation: ×3 (restarts the session…)*.
  1.2 seconds after the last click the value is saved and the session
  restarts with it, so you can click several times to skip values. The GUI
  does the saving and the restart: with the GUI closed nothing changes.
  Going back to the value the session already runs with saves nothing. With
  frame generation off the button still shows (*Off*), so a click turns it on
  with the next restart; it hides only when the layer is not installed or the
  `LSFGVK_*` variables set it from outside.

## Comparing settings

Two tools help you judge a change on the same scene. Both compare **A**, the
settings before the last change (a mode picked from the chip or with the
hotkeys, a saved edit, a *Try live* preview), with **B**, the current ones.
Changes made with the model, geometry, zoom or offset hotkeys do not count.
Nothing is written anywhere: A lasts until the session ends. Before the first
change the message says *Nothing to compare yet*.

### A/B swap

`Alt`+`Shift`+`X` swaps the current settings with the previous ones, so you
can blink between them on a live game. The message says which one you see:
*A: previous settings* or *B: current settings*. Press it again to go back.
The swap itself is a change, so the side you left becomes the new A.

### Split comparison

The *Compare* button of the toolbar (or `Alt`+`Shift`+`C`) freezes the image
on the current frame and shows it split in two: **A, the previous settings, on
the left**, and **B, the current ones, on the right**, divided by a vertical
line. The message says *Compare: ← previous · current →*.

- **Drag** with the left button anywhere on the image (outside the toolbar) to
  move the line, or click to put it there. While the comparison is on, the
  mouse is not forwarded to the game.
- The game keeps running behind, but the image does not change until the
  comparison ends. The toolbar, the stats panel and the messages keep
  working.
- Press the button or the hotkey again to end it (*Compare ended*). It also
  ends by itself when the settings change, when you pick another mode, swap
  A/B, switch window, switch between window and fullscreen, or the window
  changes size, and when you zoom, move the image, or change the model, the
  geometry or the built-in engine's multiplier.
- The stats panel's fps drop towards 0 while the image is frozen: no new
  frames are presented.
- If the previous settings cannot be prepared, the message says *Cannot
  compare* and the session goes on as before.

## Stats panel

The panel in the top-left corner of the overlay (`Alt`+`Shift`+`I`,
`show_stats: true` or *Extras* ▸ *Stats panel*) refreshes twice per second:

- **Game**: new frames per second from the window, and how often it is polled;
- **Shown**: frames presented per second and the time of a full render, with
  the time spent waiting for the display apart;
- **Frame gen**: the multiplier and the output rate; with the built-in engine
  also the precision, the pacing and, with *Auto*, the multiplier it picked;
- **Latency**: average and maximum over the last second, from the capture of a
  frame to the moment a frame made from it is presented, for real and
  generated frames. It does not include the display, nor what the lsfg-vk
  layer adds. `latency_log: PATH` (`--latency-log`) writes every present to a
  CSV file;
- **Source**, **Upscale**, **Effects** (with their GPU times), **Output**,
  **Post**, **Present** and **Profile**: the chain in order;
- **GPU**, in red, when the effects take longer than a game frame lasts. It
  names the heaviest effect.

While the panel is visible, the effects are timed with GPU timestamps, which
cost a little.

## Screenshots

`Alt`+`Shift`+`P` (or the toolbar) saves the scaled image, losslessly, to
`~/Pictures/Screenshots/` as `Screenshot_{timestamp:%Y%m%d_%H%M%S}.png`. Change
the folder and the name in *Extras*, with `screenshot_dir` and
`screenshot_filename`, or with `--screenshot-dir` and `--screenshot-filename`.
The name can use:

| Placeholder | Value |
| --- | --- |
| `{timestamp}` | The local time of the capture; `{timestamp:FORMAT}` takes any `strftime` format |
| `{title}` | The window title at the moment of the capture |
| `{profile}` | The active profile (the window title when none matched) |
| `{model}` | The SRCNN model |
| `{width}`, `{height}` | The size of the image |

For example, `{model}/{timestamp:%H-%M-%S}.png` saves to `fast/14-30-22.png`.
A message on the overlay confirms the capture once the file is written.
