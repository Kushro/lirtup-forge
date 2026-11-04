# Migrating from 1.x

Version 2.0 is a rewrite in Rust that replaces the Python 1.x. It keeps the
package name (`linux-rt-upscaler`), the configuration folder
(`~/.config/linux-rt-upscaler`) and the file format, so your settings and
profiles carry over.

## 1. Replace the program

Uninstall the 1.x and install the 2.0 ([Installation](installation.md)):

```sh
pipx uninstall linux-rt-upscaler
```

If you made a menu entry with the 1.x `desktop_integration.sh`, remove it; the
2.0 package installs its own:

```sh
rm -f ~/.local/share/applications/io.github.baronsmv.linux-rt-upscaler.desktop
```

The commands change name: `upscale` is `lirtup run` and `upscale-gui` is
`lirtup-gui`. The options are the same (see [Command line](cli.md#changes-from-the-1x-upscale)).

## 2. Your configuration

The 2.0 reads `config.yaml` as it is: nothing needs to change for it to work.

What is new is the **scaling mode** ([Scaling modes](modes.md)): the model,
the sampling, the effects and the finishing touches move out of the profiles
into mode files that profiles share. The first time the GUI starts with a 1.x
file, it offers to do this for you:

- every distinct combination of those settings becomes a mode: *Predeterminado*
  for the top-level ones and *Modo de <profile>* for each profile that differs;
  profiles with the same combination share one;
- the profiles then name their mode, and lose the loose fields;
- `config.yaml` is copied first to `config.yaml.1x.bak`;
- every window is scaled exactly as before.

*Not now* keeps the file as it is and does not ask again (not for the native
effects engine either, below: your files keep working as they are). The command
line does the same at any time; `--dry-run` only shows the plan:

```sh
lirtup migrate --dry-run
lirtup migrate
```

To go back, copy the backup over the file:

```sh
cp ~/.config/linux-rt-upscaler/config.yaml.1x.bak ~/.config/linux-rt-upscaler/config.yaml
```

Also carried over:

- the tray settings of the 1.x GUI (in `~/.config/linux-rt-upscaler.conf`:
  the icon, close and minimize to the tray, start hidden, keep running) move to
  `gui-config.yaml` when you migrate. The size of the 1.x window and the
  position of its overlay are not carried over;
- the named color palette of the 1.x GUI becomes the seed color of the new theme.
  Loose custom colors are not used: the 2.0 theme is built from the seed;
- your MagpieFX effects in `~/.config/linux-rt-upscaler/effects/` and the
  compiled shader cache stay where they are.

## The native effects engine

Up to 2.1 a mode could choose between two effects engines: *native* (hand ports
of Anime4K_Upscale_S, LumaSharpen and CRT_Easymode, set with `effects_engine`,
`anime4k_enabled`, `luma_sharpen_*` and `crt_*`) or *MagpieFX* (the
`magpie_effects` chain). The native engine is gone: the same three effects run
through MagpieFX, in the same place of the frame, with the same parameters.

Files that still use those settings keep working without doing anything: when
it reads them, LirtUp turns them into the MagpieFX effects they stand for
(`Anime4K/Anime4K_Upscale_S`, `Sharpen/LumaSharpen`, `CRT/CRT_Easymode`, in
that order, with the parameters you changed). Settings of the native engine
given on the command line (`--set crt_enabled=true`) are ignored with a warning;
use `--magpie-effect` instead.

Migrating writes that translation into the files, so the mode editor can show
and edit the effects:

- each mode file with native-engine settings is rewritten: the enabled native
  effects become its `magpie_effects` (with the native engine chosen, a
  `magpie_effects` list that was not running goes away; with MagpieFX chosen,
  the list stays as it was), and the native settings are removed. Its name,
  description and anything else in it stay;
- the old file is copied first to `modes/<id>.yaml.bak` (`.bak2`, … if a copy
  is already there; those copies are not read as modes);
- loose native settings in `config.yaml` or a profile (1.x files) move into
  modes with the other mode fields, already translated;
- every window is scaled as before.

The GUI asks once (*Update the effects* when that is all there is to do, or
inside the 1.x dialog). If you answered *Not now* to the 1.x migration it does
not ask again; run it from the command line when you want:

```sh
lirtup migrate --dry-run   # one line per mode: mode crt: native effects → magpie_effects (1 effect)
lirtup migrate
```

To go back, copy the backup over the mode file:

```sh
cp ~/.config/linux-rt-upscaler/modes/crt.yaml.bak ~/.config/linux-rt-upscaler/modes/crt.yaml
```

Two edges: a hand-written file with `magpie_effects` and **no** native setting
at all (no `effects_engine` either) used to keep that chain inactive, since the
native engine was the default; now the chain runs. And the image can differ by
rounding, as the native passes were ports of the same shaders.

## 3. What changed on purpose

| 1.x | 2.0 |
| --- | --- |
| Qt interface with a settings sidebar | New interface: Home, Scaling modes, a page per profile, Settings ([Graphical interface](gui.md)) |
| Model and effects in each profile | Scaling modes shared by profiles |
| Native effects engine (Anime4K_Upscale_S, LumaSharpen, CRT_Easymode) or MagpieFX | MagpieFX only; the three effects run through it ([above](#the-native-effects-engine)) |
| Scaling with unsaved settings | The scaler reads the saved files; the bar at the bottom counts what is not saved |
| — | A toolbar over the game ([Controls](controls.md#toolbar-over-the-game)) |
| Python API (`upscaler.api`) | No Python API; `lirtup serve` has a socket protocol |
| A value out of range stops with code 2 | A warning, and the default value is used |
| Mouse forwarding with XTest or `SendEvent` | `SendEvent` only |
| `--list-*`, `--help-all`, `-v` for the version, `python -m upscaler.bench` | `lirtup list …`, `--help`, `-V`, `lirtup bench` |
| `upscale-gui` takes every CLI option | `lirtup-gui` takes only `-c` |
| `UPSCALER_EFFECT_TIMING=1` with `--magpie-profile` | `--magpie-profile` alone |
| The launched program gets `__GLX_VENDOR_LIBRARY_NAME=nvidia` | Not set: it breaks OpenGL on AMD and Intel. Set it yourself on NVIDIA if a game needs it |

## Keeping the 1.x

The last 1.x is 1.3.0. Both versions read the same `config.yaml` until you
migrate it; after migrating, the 1.x ignores the modes, so restore
`config.yaml.1x.bak` if you go back to it.
