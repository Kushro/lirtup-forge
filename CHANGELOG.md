# Changelog

## 1.3.0 (unreleased)

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
- The GPU warning of the stats panel uses the game's real frame interval
  when no frames are lost, so content under 30 fps that keeps up is not
  flagged.
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
