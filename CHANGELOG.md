# Changelog

## 1.2.0 (unreleased)

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
