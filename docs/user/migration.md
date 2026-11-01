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

*Not now* keeps the file as it is and does not ask again. The command line does
the same at any time; `--dry-run` only shows the plan:

```sh
lirtup migrate --dry-run
lirtup migrate
```

To go back, copy the backup over the file:

```sh
cp ~/.config/linux-rt-upscaler/config.yaml.1x.bak ~/.config/linux-rt-upscaler/config.yaml
```

Also carried over:

- the tray settings of the 1.x GUI (in `~/.config/linux-rt-upscaler.conf`) move
  to `gui-config.yaml`;
- the named color palette of the 1.x GUI becomes the seed color of the new theme.
  Loose custom colors are not used: the 2.0 theme is built from the seed;
- your MagpieFX effects in `~/.config/linux-rt-upscaler/effects/` and the
  compiled shader cache stay where they are.

## 3. What changed on purpose

| 1.x | 2.0 |
| --- | --- |
| Qt interface with a settings sidebar | New interface: Home, Scaling modes, a page per profile, Settings ([Graphical interface](gui.md)) |
| Model and effects in each profile | Scaling modes shared by profiles |
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
