# Changelog

## Unreleased

### Added

- **«Probar en vivo» / «Try live» in the mode editor.** While a session runs a
  mode, its editor's save bar shows a toggle that puts the unsaved edits on
  screen without writing any file (about 150 ms after the last change). It
  applies whether the session uses the profile's mode or a temporary mode
  picked on the bar. *Save*, *Reset*, leaving the editor, stopping the
  session, the session switching to another mode, or closing the GUI end it.
  The OSD says *Preview: unsaved changes* / *Preview ended*, the bar's mode
  chip turns tertiary with a dot, and Inicio's session card shows a
  «vista previa» / «preview» badge.
- New protocol commands `Preview { profile, mode, raw }` and `PreviewEnd`;
  `SessionView` gains `preview` (the protocol stays at version 1).
  `ReloadConfig` keeps a preview; it ends on `PreviewEnd`, when the session
  stops or when the client that sent it disconnects. `lirtup ctl preview
  mode=<id> raw={...}` and `lirtup ctl preview_end` send them (use
  `ctl --follow` to keep a preview alive).
- **A/B swap** (`Alt+Shift+X`): blink between the current settings and the
  ones before the last visible change («A: configuración anterior» /
  «B: configuración actual»). Nothing is saved; it lasts the session.
- **Split comparison** (the bar's new *Compare* button, or `Alt+Shift+C`):
  the frame freezes and shows the previous settings left of a draggable white
  line and the current ones right of it; drag anywhere outside the bar to move
  the line. The OSD, the stats panel and the bar stay alive, and the built-in
  frame generation receives the composed image. Any change of settings, mode,
  window, size or view ends it. New compute shader `misc/split_compose`
  (`lirtup_effects::compose::SplitCompose`).
- New session hotkeys: next mode (`Alt+Shift+N`), previous mode
  (`Alt+Shift+B`), A/B swap (`Alt+Shift+X`) and compare (`Alt+Shift+C`). In
  Ajustes ▸ Atajos the two mode hotkeys are under *Vista* and the other two in
  a new *Comparar* group; `config-example.yaml` lists them.
- **Precompiled MagpieFX seed.** The bundled effects ship precompiled
  (`/usr/share/lirtup/magpiefx-seed`, inside the AppImage too), so they load
  on first use without dxc or glslang, including the three
  Anime4K_Denoise_Bilateral effects. Your own and edited effects still compile
  locally. `LIRTUP_MAGPIEFX_SEED_DIR` points to another seed.
- New `magpiefx-seed` binary (`cargo run --release -p lirtup-magpiefx --bin
  magpiefx-seed -- --out DIR`) builds the seed. The release script, the AUR
  package (new makedepends `directx-shader-compiler` and `glslang`) and the
  release workflow build and install it.
- Docs: *Controls* covers the new hotkeys, the multiplier with the lsfg-vk
  layer, the single-mode message, the *Compare* button, A/B swap and the split
  comparison; *Modes* has a «Try live» section; *MagpieFX* explains the
  precompiled seed.
- **MagpieFX effects before the model.** New mode key
  `magpie_effects_before_model` (and the repeatable
  `--magpie-effect-before-model EFFECT` flag, same `Name` /
  `Name:param=value,...` form as `--magpie-effect`): its effects run on the
  captured frame, at the game's resolution, and the model gets the result;
  with `model: none` they feed the presenter directly. `magpie_effects` keeps
  its meaning (after the model); an effect may be in both lists. While the
  list has effects, tile processing is off and the upscaler runs full frame
  (logged at debug level). Another list (effects or order) rebuilds the
  upscaler; the same effects with other values apply live; effects that fail
  to compile or bind are dropped and reported once, like the ones after the
  model. The stats panel lists them before the SRCNN passes.
- **The mode editor's chain has two segments**, «Antes del modelo» /
  «Before the model» and «Después del modelo» / «After the model», with the
  model between them («Modelo fast · 2×», or «Sin modelo…»). Each has its
  own help, empty state and «Agregar efecto…»; a card's ⇅ button moves the
  effect to the end of the other segment, and cards can be dragged between
  segments. Costs: an effect before the model counts as measured, one after
  it × s² (s = 1 without a model, 2, or 4 with `double_upscale`). The mode
  list's summary and cost cover both lists.
- The split comparison's OSD adds «el modelo y los efectos antes del modelo
  no se comparan» / «the model and the effects before it are not compared»
  when the two sides differ in the model, `double_upscale` or the chain
  before the model (both sides use the same upscaler output).

### Changed

- **The bar's frame generation button works with the lsfg-vk layer.** It
  steps Off → ×2 → ×3 → ×4 (no Auto), saves the value 1.2 s after the last
  click and the session restarts once after the reload (LirtUp Forge must be
  open). It also shows while frame generation is off, so it can turn it on. The tray's ×N does the same with the layer. The built-in engine still
  changes it live.
- The mode chip (and the new mode hotkeys) say «Solo hay un modo: creá otro en
  LirtUp Forge» / «Only one mode: create another in LirtUp Forge» when there
  is only one mode.
- Loose mode settings in `config.yaml` (1.x leftovers such as a top-level
  `magpie_effects`) win over the mode they sit next to, so a mode picked on
  the bar could change nothing. The resolver now reports which keys shadow
  the mode (`Resolved::shadowed`, a config warning in the log) and the chip's
  message adds «ajustes sueltos de config.yaml lo tapan (lirtup migrate)» /
  «loose settings in config.yaml shadow it (lirtup migrate)».
- MagpieFX: the shader cache key no longer contains the effects folder's path,
  so the AppImage no longer recompiles every effect on each launch. The cache
  format moves to `rs2`: effects compile once more after updating, unless they
  come from the seed.
- «Medir en mi GPU» (bench) no longer refuses MagpieFX effects when no HLSL
  compiler is installed: the seed serves the bundled ones.
- **One effects engine: MagpieFX.** The native effects engine is retired:
  `effects_engine` and its 24 settings (`anime4k_enabled`,
  `luma_sharpen_*`, `crt_enabled` and the `crt_*` settings) are gone, and the
  `magpie_effects` chain always runs. Anime4K_Upscale_S, LumaSharpen and
  CRT_Easymode run through MagpieFX, in the same place; the image may differ
  by rounding (MagpieFX may run Anime4K_Upscale_S in FP16).
- **Old files keep working.** When the configuration is read (`lirtup
  serve`, the CLI, previews), the native settings become MagpieFX effects:
  the effects that were on, in the old order (Anime4K, LumaSharpen, CRT),
  under their catalog names (`Anime4K/Anime4K_Upscale_S`,
  `Sharpen/LumaSharpen`, `CRT/CRT_Easymode`) and with only the parameters
  that differ from the default. With the native engine the inactive
  `magpie_effects` list is dropped; with `effects_engine: magpiefx` it is
  kept. Bad values warn and fall back to the default, as before. A file with
  `magpie_effects` and no native setting at all (hand-written; the GUI always
  wrote `effects_engine: magpiefx`) used to keep that chain inactive and now
  runs it.
- **`lirtup migrate` and the GUI's migration dialog rewrite mode files that
  still use the native engine** (name, description and unknown keys kept),
  copying each one first to `modes/<id>.yaml.bak` (`.bak2`… if taken). Loose
  1.x native settings in `config.yaml` move into the new modes already
  translated. With only that to do, the dialog is titled «Actualizar los
  efectos» / «Update the effects»; `lirtup migrate --dry-run` prints one line
  per rewritten mode (`mode crt: native effects → magpie_effects (1
  effect)`). Declining the 1.x migration also declines this one; the
  read-time translation keeps everything working.
- A 2.1 mode with native-engine settings shows a banner in the mode editor
  with «Migrar…» / «Migrate…» and its chain read-only, as it runs.
- «Medir en mi GPU» / `lirtup bench` without a mode measures LumaSharpen,
  Anime4K_Upscale_S and CRT_Easymode as MagpieFX effects under their catalog
  names; with a mode, its native passes, then both chains (each effect once).
  Old `native/anime4k`, `native/luma_sharpen` and `native/crt_easymode`
  entries in `bench.json` are no longer written or read.
- Warming up an effect (`CompileEffect`, the GUI's picker) compiles it for
  every position in a chain (the four red/blue swap combinations), so a
  session finds it in the cache in either list; cold and without the seed
  that is up to four compiles.
- Changing the deband strength live no longer rebuilds the MagpieFX chain.
- The stats panel no longer shows the «(Native)» / «(MagpieFX)» engine row.
- The MagpieFX help text in the mode editor says that the bundled effects
  come precompiled; only the ones you add or edit compile the first time.

### Removed

- The CLI flags `--effects-engine`, `--enable-anime4k`,
  `--enable-luma-sharpen`, `--luma-sharpen-*`, `--enable-crt` and every
  `--crt-*` flag (use `--magpie-effect`, e.g. `--magpie-effect
  CRT/CRT_Easymode:maskSize=2`). The old keys given with `--set` are ignored
  with a warning («retired native engine setting, ignored; use
  --magpie-effect»).
- The engine selector and the native effect switches of the mode editor.
- The app no longer embeds the SPIR-V of the three hand ports: they stay only
  as test references (`lirtup_effects::ports`, behind the non-default cargo
  feature `ports`).

### Fixed

- On a mode page, the save bar's «Restablecer» now discards that mode's
  unsaved edits; it did nothing there before.
- The save bar no longer overflows a narrow window: «N cambios sin guardar»
  shortens with «…», and below 860 px «Probar en vivo» keeps only its icon.
- The chain cards find effects written with a bare stem (`LumaSharpen`, as
  1.x files have) in the catalog instead of showing «No está en el catálogo».

## 2.1.0 (2026-11-02)

### Added

- **Mode chip in the toolbar over the game.** It goes through the scaling
  modes without leaving the game: a click or the wheel down for the next one,
  a right click or the wheel up for the previous one. The change applies
  live, without restarting the session or the frame generation, 300 ms after
  the last click. It is temporary: the next session uses the profile's mode
  again. *Save* (next to the chip, or on Inicio's session card) writes it in
  the profile; the GUI must be open.

### Changed

- Inicio's session card follows a configuration change during the session
  (saving in the GUI, the mode chip, `follow_focus`): `lirtup serve` sends
  `SessionStarted` again with the new mode and chain. `SessionView` gains
  `mode_temporary`; the protocol stays at version 1.

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
