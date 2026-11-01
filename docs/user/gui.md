# Graphical interface

Start the app from the menu (*LirtUp Forge*) or with `lirtup-gui`. The window
has a drawer on the left and the current page on the right; below 1100 px the
drawer shrinks to a rail of icons. The interface is in English and Spanish
(*Settings* ▸ *General* ▸ *Language*).

The GUI does not scale anything itself: it talks to `lirtup serve`, which it
starts the first time it needs it. The banner at the top says so when the
scaler cannot be reached.

## Home

**Activation** starts scaling the focused window without picking it:

| Row | What it does |
| --- | --- |
| *Scale fullscreen* | `Alt`+`Shift`+`A`: the focused window, with the mode of its profile |
| *Scale in a window* | `Alt`+`Shift`+`Q`: the same, in an always-on-top window you can move from the toolbar |
| *Deferred scaling* | *Start* waits a few seconds (3 by default, the arrow picks another delay) so you can go back to the game, then scales the focused window |
| *Daemon mode* | Scales the windows that match a profile as they appear, even with this window closed (see [Tray and daemon](tray-daemon.md)) |

The pencil next to each hotkey changes it.

**Open windows** shows a live preview of every window that can be scaled.
Filter them by title or class (`Ctrl`+`F`), or with *All*, *With profile* and
*Without profile*. Then:

- click a tile (or select it with the arrows and press `Enter`) to scale it
  with the mode of the profile that matches it;
- right-click it, or use its ⋮ button, to scale it in a window, create a
  profile from it or copy its title.

While a window is scaled, a card at the top shows it: the mode, the source
and output sizes, the captured and shown frame rates and the frame generation
multiplier, with *Restart* and *Stop*. If the GPU cannot keep up, the card
says how many milliseconds a frame takes against the time available. If
scaling fails, the card says at which step and offers *Retry*.

## Scaling modes

A **mode** groups how a window is processed: the SRCNN model, the sampling
filters, the Magpie effects and the finishing touches. Profiles name a mode,
so several profiles can share one. *Scaling modes* lists them with their total
GPU cost and how many profiles use each one. The editor is described in
[Scaling modes](modes.md).

## Profiles

A profile holds the settings for the windows that match its rules. The drawer
lists *Default* (the settings every window starts from) and your profiles; a
dot marks unsaved changes and an amber ▶ the profile being scaled.

A profile page has a header with its icon, name and rules (*Edit profile*
changes them) and five tabs:

| Tab | Settings |
| --- | --- |
| *Scaling* | The mode (and *Edit mode*), follow focus, pause when the window loses focus, exclude from the daemon |
| *Presentation* | Overlay mode, output geometry, menu bar detection, cursor hiding, crop and offset, background color, frame generation |
| *Display* | Monitor, GPU, present mode, frame rate limit, scale factor |
| *Extras* | Screenshots, on-screen messages, stats panel |
| *Advanced* | Tiles, damage tracking, timing and error recovery |

Every row shows where its value comes from: inherited from *Default*,
changed in this profile (a stripe, with a button to inherit again), or
changed and not saved yet (an amber stripe with the previous value).

### New profile

*New profile* in the drawer (`Ctrl`+`N`), or *Create a profile from this
window* in a tile's menu, opens a dialog with:

- an icon, captured from a window or taken from a file;
- the name;
- the mode, and which profile to copy the settings from;
- the rules: the title (exact, contains or regular expression) and the width
  and height, with `>` and `<`;
- a preview that counts the open windows that match.

*Capture window* lists the open windows; *Point at window* lets you click one.

## Saving

Changes are not applied until you save them. While there are unsaved changes,
a bar at the bottom counts them and offers *Reset* (discard them, or restore
the defaults) and *Save* (`Ctrl`+`S`). Leaving a page with unsaved changes
asks whether to save or discard them. After saving, *Undo* in the message at
the bottom goes back to the previous files.

The scaler reads the saved files: a session that uses the profile you saved
applies the change at once.

## Settings

| Tab | Settings |
| --- | --- |
| *General* | Language, theme (system, light or dark) and its seed color, the tray icon (show it, minimize and close to the tray, start hidden), daemon mode and *Keep running after exit* |
| *Hotkeys* | Every hotkey, grouped as Activation, Session, View, and Zoom and offset. Click one and press the new combination |
| *Style* | Zoom, font size, font, tile shape (4:3, 16:9 or automatic) and how often the window list and the previews refresh |

## About

The version, links, third-party licenses and *Diagnostics*: the GPU and
driver, whether lsfg and `dxc` were found, with a button to copy it all when
reporting a problem.

## Keyboard

The whole window works with the keyboard: `Tab` moves between the title bar,
the drawer and the page, and the arrows move inside lists and the window
grid.

| Shortcut | Action |
| --- | --- |
| `Ctrl`+`S` | Save |
| `Ctrl`+`N` | New profile |
| `Ctrl`+`D` | Duplicate the current profile |
| `Ctrl`+`F` | Filter the windows |

## Files

The GUI writes the same files as the command line reads:

- `~/.config/linux-rt-upscaler/config.yaml`: the default settings and the profiles;
- `~/.config/linux-rt-upscaler/modes/*.yaml`: the scaling modes;
- `~/.config/linux-rt-upscaler/gui-config.yaml`: the settings of the app itself.

See [Configuration](configuration.md).
