# Command line

`lirtup` is the command line and the scaler. Its subcommands:

| Command | What it does |
| --- | --- |
| `lirtup run [OPTIONS] [PROGRAM ARGS…]` | Scale a window: the 1.x `upscale`, with the same options |
| `lirtup list windows\|models\|effects\|gpus\|monitors` | What can be picked |
| `lirtup migrate [--dry-run]` | Move the 1.x configuration to scaling modes ([Migrating from 1.x](migration.md)) |
| `lirtup serve [--daemon]` | The scaler for the GUI, on a Unix socket (the GUI starts it) |
| `lirtup ctl COMMAND [--follow]` | Send one command to a running `lirtup serve` and print its events |
| `lirtup bench --json REQUEST` | The measurement behind *Measure on my GPU* |

`lirtup --help` and `lirtup <command> --help` list every option. `-v`
(repeatable) and `-q` change how much is logged to the terminal; `RUST_LOG`
works too.

## Picking the window

- The window that has the focus after a 5-second wait:

    ```sh
    lirtup run
    ```

- From a list (sorted by title; `q` cancels):

    ```sh
    lirtup run -s
    ```

- By its title, as a substring or a regular expression:

    ```sh
    lirtup run -t firefox
    lirtup run --target-title-regex "(Yuzu|Ryujinx).*"
    ```

- Launching a program and scaling its window. LirtUp Forge looks for the
  window by the program's PID and by its class, which also finds the windows
  of launchers and Proton, and asks the program to use X11. The program ends
  when you stop scaling, or if its window does not appear.

    ```sh
    lirtup run -- wine game.exe
    lirtup run wine game.exe
    ```

- Daemon mode: scale every window that matches a profile as it appears (see
  [Tray and daemon](tray-daemon.md)):

    ```sh
    lirtup run -d
    ```

When more than one is given, the title wins over `--select`, and both over a
program.

## Common options

```sh
lirtup run -m 4x24              # SRCNN model
lirtup run -2                   # two 2x passes, for 4x
lirtup run --mode anime-sharp   # a scaling mode instead of the profile's
lirtup run -p "Visual novels"   # a profile instead of the matching one
lirtup run -f                   # follow the focused window
lirtup run -o stretch           # fill the screen, ignoring the aspect ratio
lirtup run --crop-top 29 --crop-left 3 --crop-right 3 --crop-bottom 3
lirtup run --magpie-effect LumaSharpen:sharpStrength=1.0
lirtup run --frame-gen 2        # frame generation, ×2
```

Every option of the configuration file has a flag; for the rest,
`--set key=value` takes a YAML value, with dotted keys for hotkeys:

```sh
lirtup run --set overlay_opacity_max=0.9 --set hotkeys.zoom_in=Ctrl+Plus
```

Options combine, and a flag wins over the profile and the configuration file
(see [Settings precedence](configuration.md#settings-precedence)).
`-c FILE` reads another configuration file, also in `lirtup-gui -c FILE`.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Scaling ended normally |
| 1 | The window was not found, or another error |
| 2 | The configuration could not be read |
| 130 | Interrupted with `Ctrl`+`C` |

## Changes from the 1.x `upscale`

| 1.x | 2.0 |
| --- | --- |
| `upscale` | `lirtup run` |
| `upscale-gui` | `lirtup-gui` |
| `upscale --list-models` (and the other `--list-*`) | `lirtup list models` |
| `upscale --help-all` | `lirtup run --help` (every option, grouped) |
| `upscale -v` (version) | `lirtup -V`; `-v` is more logging |
| `python -m upscaler.bench` | `lirtup bench` |
| `upscaler.api` (Python API) | No Python API; programs can drive `lirtup serve` over its socket (`lirtup ctl` shows the protocol) |
| A value out of range stops with code 2 | A warning, and the default value is used |
| `--magpie-profile` with `UPSCALER_EFFECT_TIMING=1` | `--magpie-profile` alone logs the GPU times |
