# Changelog

## 2.0.1 (2026-11-01)

### Fixed

- `lirtup-gui` starts `lirtup serve` as soon as it opens. In 2.0.0 it
  waited for the first command: until then Inicio showed «El escalador no
  responde», no open windows, and the shortcuts did nothing.
- If `lirtup serve` goes away while the GUI is open (a crash), the GUI
  starts it again after 10 s, without leaving zombie processes.
- The notifications with «Detener» close when the session stops, when
  another session replaces them and when `lirtup serve` exits. Plasma never
  expires a notification with an action, so each session left a
  `notify-send` waiting (orphaned once `serve` exited) or a thread of the
  GUI blocked, and a dead «Detener» in the history.

## 2.0.0 (2026-11-01)

LirtUp Forge is rewritten in Rust, with a new GUI in Dioxus. 2.0 replaces
the Python app (1.x) under the same package name (`linux-rt-upscaler`) and
reads the same files in `~/.config/linux-rt-upscaler/`. The 1.x code stays in
the `py_historical_backup` branch. See `docs/user/migration.md`.

### Changed

- **Two programs.** `lirtup` is the scaler and the command line (`run`,
  `serve`, `ctl`, `list`, `bench`, `migrate`); `lirtup-gui` is the GUI. The
  GUI talks to `lirtup serve` over a Unix socket, so the daemon keeps
  scaling with the GUI closed. `upscale` and `upscale-gui` are gone.
- **New GUI** (Material 3 Expressive): Inicio with live previews of the
  windows, profiles with visible inheritance and a floating save bar,
  scaling modes, Ajustes, light and dark themes from a seed color (the 1.x
  palettes become seeds), Spanish and English.
- **Scaling modes**: model, sampling, effects engine and chain, and final
  touches live in `modes/<id>.yaml`, and profiles name one. 1.x files work
  as they are; `lirtup migrate` (or the GUI on its first run) moves the loose
  fields into modes, with a backup in `config.yaml.1x.bak`.
- The command line: `lirtup run [options] [program args]`; lists are
  `lirtup list windows|gpus|monitors|effects|models`; `-V` is the version
  and `-v` more logging; out-of-range values warn and fall back to the
  default instead of exiting.
- `--select` numbers the windows from 0 sorted by title, and `q` cancels,
  as 1.x.

### Added

- **The bar over the game**, drawn in Vulkan (pin, stats, crop,
  screenshot, frame generation multiplier, fps, hide, to window, stop):
  it shows at the top edge or with Alt+Shift+D, and drags the window in
  windowed mode. The stats panel and the OSD use the same design.
- **Tray** with four states, its menu (stop, restart, profile, frame
  generation, daemon), close or minimize to the tray, start hidden; desktop
  notifications with «Detener»; one GUI instance.
- **Daemon** in `lirtup serve`: scales the first window that matches a
  profile, and «keep running after exit».
- `Restart` with the lsfg-vk layer resumes the session in the new process.
- `lirtup migrate`, `lirtup-gui -c`, profile reordering (Ctrl+Shift+↑/↓),
  changing a profile's icon after creating it, Ctrl+Q, F2 and Supr.
- `follow_focus` applies the profile of the newly focused window and, if
  the window closes, waits for the next one.
- Captures: `{timestamp:...}` takes any `strftime` code, in local time.

### Fixed (against 1.x behavior kept in 2.0)

- `pause_on_focus_loss` acts only in the always-on-top and top-transparent
  modes, as 1.x; `swapchain_debounce`, `magpie_profile`, `log_level` and
  `log_file` work again.
- Vulkan windows on KWin with XWayland capture as black, as in 1.x: a
  limitation of the X server, documented in `docs/user/troubleshooting.md`.

### Removed

- The Python package, its API (`upscaler.api`), the PyPI wheels and
  `scripts/desktop_integration.sh`. The shaders, CuNNy models, Magpie
  effects and the lsfg C++ core now live in the crates.

## 1.3.0 (not released; its changes ship in 2.0.0)

### Added

- **31 more Magpie effects** in MagpieFX: the 26 RAVU, the 4 SMAA and NIS,
  with their `.dds` lookup textures (`//!SOURCE`), unmodified. MagpieFX now
  bundles 117 effects.
- **FP16** in the MagpieFX effects that declare `//!CAPABILITY FP16`, when
  the GPU supports it (`UPSCALER_MAGPIEFX_FP16=0` turns it off). A pass that
  does not compile in FP16 falls back to FP32.
- **Measure on my GPU**: a dialog that times every effect and the built-in
  frame generation (×2 to ×4, flow scale, model) on this GPU, saves the
  results per GPU and shows them instead of the reference times. Also as
  `python -m upscaler.bench`.
- **Automatic frame generation multiplier** (`frame_gen_multiplier: auto`,
  built-in engine only): picks ×2 to ×5 from the game's frame rate and the
  display refresh, with hysteresis.
- **Latency** in the stats panel, and an optional per-present CSV
  (`--latency-log`).

### Changed

- `fifo` presentation no longer holds the GIL while it waits: the Vulkan
  queue is guarded by its own mutex, so the frame generation thread does
  not stall the pipeline.
- `robustImageAccess` is enabled when the GPU offers it.
- The flow scale and the lsfg model are explained in the GUI, and the
  frame generation section shows the measured cost of the chosen setting.
- Every GUI string has a Spanish translation, including the settings tab
  names and the Style and Scaling tabs.

## 1.2.0

First release of the LirtUp Forge fork with its own features.

### Added

- **Frame generation** with lsfg-vk, ×2 to ×5 (`frame_gen_*` options,
  `--frame-gen`, *Presentation* > *Frame generation*).
  - Through the lsfg-vk Vulkan layer over the app's own window (default).
  - Built in (experimental, `frame_gen_engine: native`): the lsfg core runs
    inside the app on the same GPU, sharing frames by fd, with the shaders
    read from `Lossless.dll`. Written and compiled, not yet tested on
    hardware.
  - `present_on_change_only`: present only new frames of the target window.
- **Stats panel** (experimental), toggled with Alt+Shift+I: frame rates, GPU
  times and the processing chain. It warns in red when the effects take
  longer than a game frame, naming the heaviest one.
- **System tray**: restart, switch to another window, choose the profile and
  the frame generation multiplier (the app restarts itself to apply it and
  upscales the same window again).
- **Menu bar detection** (experimental, `auto_crop_menu_bar`): crops the menu
  bar of Windows programs under Wine, and the sunken frame around their
  image when there is one. In the GUI the crop is saved in the window's
  profile.
- **Magpie effects** with a choice of engine: a few ported by hand, or 86
  effects run unmodified by the MagpieFX loader.
- Unit tests (`pytest`, `pip install .[test]`).

### Fixed

- The GUI showed English in pip and pipx installs: the compiled translations
  are now part of the package.
- A failed swapchain creation crashed the app instead of raising an error.
- Waiting for a new frame (only new frames, frame generation) polled the
  window hundreds of times per second; it now polls once per display refresh.
