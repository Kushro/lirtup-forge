# Live tuning + MagpieFX seed — integration

Integration of the round described in `live-tuning-contract.md`, on branch
`live-tuning/integration` (base: the contract commit `89828ac0`, on top of
`main` at `dd662f95`, v2.1.0). **Nothing was compiled or run**: neither the
workers nor the integrator ran cargo. The first step after this is
`cargo fmt --all`, `cargo check --workspace --all-targets`,
`cargo clippy --workspace --all-targets` and `cargo test --workspace`
(then the `#[ignore]` GPU tests on a machine with Vulkan).

## What merged

Merged with `git merge --no-ff`, in the contract's order:

| # | Branch | Commits | Scope |
| --- | --- | --- | --- |
| 1 | `claude/lt-magpiefx` | `fdb12a99`, `a0a81017` | rs2 cache key without the include path, read-only seed lookup (user cache → seed, for every compiler kind, also with no compiler), `magpiefx-seed` binary, `paths::magpiefx_seed_dir`, packaging (release script, install tree, launcher, PKGBUILD/.SRCINFO, release workflow), `docs/user/magpiefx.md` |
| 2 | `claude/lt-hotkeys` | `4b4cc3fd` | `mode_next`/`mode_prev`/`swap_ab`/`toggle_compare` tests and docs, settings groups and labels, `config-example.yaml`, `docs/user/controls.md` |
| 3 | `claude/lt-proto-engine` | `5aa04586`, `748d7fb7` | `Command::Preview`/`PreviewEnd` in the engine (`Running.preview`, `resolve_previewed`, `update_session`), the server's `PreviewOwner` disconnect guard, wire tests, `lirtup ctl preview` |
| 4 | `claude/lt-presenter` | `e91f4858`, `9e4c7488` | `split_compose` shader (+ `.spv`) and `compose::SplitCompose`, presenter refactor into shared lane builders, `begin_compare`/`set_split`/`compare_active`/`end_compare` |
| 5 | `claude/lt-session` | `dd8f9204`, `3827305f`, `1afbf3cd` | layer multiplier, single-mode OSD, A/B swap, preview flag and OSD, frozen-frame comparison loop and pointer routing, compare button and preview dot on the bar, `ToolbarZone.whole` in the router and the XFixes input region |
| 6 | `claude/lt-gui` | `8d2d846e`, `9d778313` | «Probar en vivo» in the save bar (debounced `Preview`, `PreviewEnd` on save/reset/undo/leave/stop), `Action::ResetMode`, session card badge, layer multiplier restart in `save_setting` and the tray, `docs/user/modes.md` |

## Conflicts

None. The ownership table of the contract held: no file was touched by two
branches, so every merge was clean and there was no file to read end to end
for a textual conflict.

## Reconciliations (cross-branch, by reading)

Checked across branches without compiling:

- **Presenter compare API vs the Runner.** `begin_compare(&Config) ->
  Result<()>`, `set_split(f32)`, `end_compare()` match the calls in
  `pipeline.rs` (`toggle_compare`, `compare_pointer`, `end_compare`). The
  Runner does not call `compare_active()` (it keeps its own `comparing`
  flag); the method stays as public API. The session worker assumed the
  presenter also ends a comparison in `set_view`/`set_source`; it only does in
  `resize`/`update_config`, but the Runner ends the comparison itself before
  every `set_view` (`move_view`, `apply_config_view`) and before
  `recreate_upscaler`, so the two stay in sync.
- **`PipelineCommand::UpdateConfig.preview`.** The engine's only
  constructor (`Inner::update_session`, used by `Preview`, `PreviewEnd`'s
  reload and `ReloadConfig`) fills `preview: running.preview.is_some()`; the
  Runner stores it before `reload_config` and `reload_config` ends with
  `announce_view()`, so the GUI's `SessionView.preview` follows (the
  proto-engine worker's request).
- **`ToolbarState.preview`/`.comparing`, `ToolbarAction::Compare`,
  `ToolbarZone.whole`.** Filled by `update_toolbar`, drawn by the bar,
  `Toolbar::zone` sets `whole = comparing`; the router (`route_whole`) and
  `surface.rs` (full input region in `top-transparent`) honour it. Every
  `ToolbarZone { .. }` literal has `whole`; the one `ItemKind::Mode`
  destructuring in the tests got `..`.
- **`HotkeyAction` arms.** `Runner::handle_hotkey` handles the four new
  actions (`cycle_mode(true/false)`, `swap_ab`, `toggle_compare`); the GUI's
  `GROUPS` lists all of them (mode hotkeys in *Vista*, A/B and compare in
  `settings-hotkeys-tuning`, now labelled «Comparar» / «Compare»); the icons
  (`line_end`, `line_start`, `swap_horiz`, `difference`) are in
  `design/tokens.rs` and in the GUI woff2 subset.
- **`Command::Preview` field types.** `profile: Option<String>`, `mode:
  String`, `raw: lirtup_config::Mapping` are built the same way by the store
  (`Store::preview_command`, from `merged_mode_raw`), destructured by the
  engine (`Inner::handle`) and by the server's guard (`Command::Preview { .. }`).
- **The layer multiplier restart.** The session sends
  `Event::SaveSetting { profile: None, key: "frame_gen_multiplier" }`; the
  store's `save_setting` sets `saving.restart` when the configured engine is
  `FrameGenEngine::Layer` and a session runs, and the tray no longer runs its
  own restart loop in that case (no double `Restart`).
- **i18n.** Every key referenced in the code exists in both languages
  (`scripts/check-ftl-keys.py`: 614 keys in es and en). New GUI icons
  (`visibility`, `visibility_off`, `undo`) are in the woff2 subset.
- **Cargo.** No new dependencies in any branch. The `[[bin]] magpiefx-seed`
  target was already in the contract's `lirtup-magpiefx/Cargo.toml`; the
  binary only uses public items of the crate (`catalog`, `compiler`,
  `runtime`) and `std`.

Cross-file changes the workers asked for, applied in the final commit:

- `crates/lirtup-pipeline/src/bench.rs` (magpiefx): the early «no HLSL
  compiler found» error in `fn magpiefx` is gone, so «Medir en mi GPU» can use
  the seed without a compiler; `compile_effect_with` returns `NoCompiler`
  when nothing hits.
- `docs/user/controls.md` (hotkeys asked for a pass once the behaviour was
  known): the comparison also ends with the zoom, offset, model and geometry
  hotkeys and the built-in multiplier; the fps readout falls while frozen;
  the «Cannot compare» message; with the layer, stepping back to the running
  multiplier saves nothing and the button only shows when frame generation
  was on at launch.
- `docs/user/modes.md`: the bar's chip shows a dot (it does not say
  «preview»); the OSD messages of a preview.
- `CHANGELOG.md` `## Unreleased` written from the workers' notes.

## Deviations from the contract (accepted as they are)

- **proto-engine:** `PreviewEnd` does a full `reload()` (re-reads the files);
  on save the files are read twice (`PreviewEnd`, then `ReloadConfig`).
  The preview is applied by resolving on a patched `ModeStore` clone whenever
  a preview exists (no `preview_applies` helper), which also lets a mode not
  saved yet be previewed. A new session resolves without the preview.
  The disconnect guard does not track the sender: if client A previews, B
  previews, and A disconnects, B's preview ends too.
- **session:** extra OSD «No se pudo comparar» / «Cannot compare» when
  `begin_compare` fails. `ab_relevant_changed` = `scaling`, `output_geometry`,
  `background_color`. The compare button is always on the bar. A layer
  multiplier equal to the configured `Fixed(n)` saves nothing.
- **hotkeys:** `mode_next`/`mode_prev` moved to the *Vista* group; the last
  group keeps the key `settings-hotkeys-tuning` with only A/B and compare,
  labelled «Comparar» / «Compare».
- **presenter:** a failing MagpieFX effect in lane A is dropped with a
  warning (like the normal chain) instead of failing `begin_compare`; A's
  passes are dropped right after its single render.
- **gui:** `Saving.end_preview` (a save ends the preview only if it wrote
  the previewed mode); `Action::ResetMode`; the stale-preview rules live in
  one guard (`end_stale_preview`) at the end of `update()`; the save bar
  stays open with 0 edits on the running mode's editor to show the toggle.
- **magpiefx:** lookup covers the keys of every known compiler kind, not
  only the detected ones (with `UPSCALER_MAGPIEFX_COMPILER=glslang`, an
  existing dxc entry is still used). `compile_passes` falls through
  `NoCompiler` on swapped attempts. `magpiefx_seed_dir()` always returns
  `Some`. Docs section titled «Precompiled seed» (the file is in English).

## Known gaps / undone

- **Nothing compiled, nothing tested.** Every new unit test (engine 8,
  proto 2, cli 1, config hotkeys 2 + model, ui hotkeys 2, store 11, pipeline
  6, overlay_input 2, compose 3, presenter 3, magpiefx compiler 9 + runtime 1
  + paths 1 + the seed binary's own) has never run. The session worker flagged
  two borrow spots to watch: `toggle_compare` (let-else over
  `ab_previous`/`parts`, then `self.osd` in the error branch) and
  `compare_pointer` (disjoint field borrows while `parts` is live).
  `cargo fmt` may re-wrap a few lines.
- **Untested at runtime (X11/GPU):** the engine's live preview path and the
  server's disconnect guard; the Runner's frozen loop, pointer routing,
  `SwapAb` and OSD; the XFixes input region while comparing; the bar's preview
  dot and compare highlight; the split composition's look and FrameGen with
  the composed screen; the GUI's debounce timing in a live session.
- GPU tests marked `#[ignore]`: `the_compare_button_takes_the_whole_overlay`,
  the updated `the_mode_chip_steps_and_saves`, `composes_two_solid_images`.
  No presenter-level GPU test (begin_compare then present).
- With the layer engine the multiplier chip still only appears when the layer
  was active at launch: a session launched with frame generation off cannot
  turn it on from the bar (existing limitation, now documented).
- While frozen no `PresentRecord` is recorded: the fps readout falls towards
  0 during a comparison.
- The GUI only sees a session's own mode switch through `SessionStarted`;
  `SessionView.preview` stays true while the engine holds a preview even if
  the session runs another mode, until the GUI's guard sends `PreviewEnd`.
- No real seed was generated: its size, generation time and the «117 effects,
  0 failures» expectation are unverified; the release workflow change is not
  exercised. The AUR `build()` runs the generator directly, so one failing
  effect fails the package build (the release script only warns unless
  `--require-seed`). Old `rs1` cache files stay in users' caches.
- GUI/diagnostics texts still say MagpieFX compiles with dxc or glslang
  (`modes-engine-magpiefx-help*` in `modes.ftl`); with a seed that is only
  true for user and edited effects. Optional follow-up.
- `assets/tailwind.css` was not regenerated (no new utility classes were
  added). No tray «vista previa» hint (optional in the contract).
- Manual test steps are in the workers' notes, summarized: preview on the
  running mode (badge, chip dot, OSD, save ends it, killing the GUI ends it);
  layer multiplier clicks (one save, one restart after 1.2 s); single-mode
  OSD; A/B after a saved change; compare on/off, dragging, ending on
  mode/zoom/resize, in `top-transparent` too; MagpieFX with
  `LIRTUP_MAGPIEFX_SEED_DIR=build/magpiefx-seed UPSCALER_MAGPIEFX_COMPILER=none`
  and an empty cache.
