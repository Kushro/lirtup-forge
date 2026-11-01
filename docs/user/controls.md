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

`start_fullscreen`, `start_windowed` and `toggle_toolbar` are new in 2.0; the
rest keep the 1.x bindings.

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
| Multiplier | Next frame generation multiplier (off, ×2, ×3, ×4, *Auto*), saved in the configuration |
| Hide | Hide the toolbar until the cursor comes back to the edge |
| To window / fullscreen | Switch between the two |
| Stop | Stop scaling |

The toolbar also shows the frame rate on screen.

In window mode, drag the toolbar to move the window. Settings changed from the
toolbar are saved by the GUI when it is open; otherwise they last until the
session ends.

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
