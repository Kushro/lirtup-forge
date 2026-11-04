# Live tuning + MagpieFX seed — the contract of the round

Round decided with the user on 2026-11-04. Base: `main` at `dd662f95` (v2.1.0). This file is the
only shared spec: every worker codes against the names fixed here, without seeing the others.

- Integration branch: `live-tuning/integration`, worktree
  `/media/octavio/DATOS/Gitbutler Repos/lirtup-forge/.claude/worktrees/lt-integration`.
- The contract commit holds this file plus **compiling stubs** of every shared interface
  (`cargo check --workspace --all-targets` passes on it). Stubs are marked `TODO(<worker>)`;
  `#[allow(dead_code)]` on a stub goes away when its worker uses it.
- Worker branches (all from `live-tuning/integration`), worktrees under
  `/media/octavio/DATOS/Gitbutler Repos/lirtup-forge/.claude/worktrees/`:

| Worker | Branch | Worktree |
| --- | --- | --- |
| proto-engine | `claude/lt-proto-engine` | `lt-proto-engine` |
| session | `claude/lt-session` | `lt-session` |
| hotkeys | `claude/lt-hotkeys` | `lt-hotkeys` |
| presenter | `claude/lt-presenter` | `lt-presenter` |
| gui | `claude/lt-gui` | `lt-gui` |
| magpiefx | `claude/lt-magpiefx` | `lt-magpiefx` |

Background reading: `docs/developer/feature-toolbar-selectors.md` (the 2.1.0 mode chip,
`resolve_with_mode`, `SessionSetup.resolve/modes`, `SessionView.mode_temporary`),
`docs/developer/rust-r4-toolbar.md` (toolbar internals), and the analysis behind this round,
which is **not committed** (untracked in the main checkout, written against 2.0.1):
`/media/octavio/DATOS/Gitbutler Repos/lirtup-forge/docs/developer/live-tuning-analysis.md`.

---

## 1. The design (as decided with the user)

Four blocks are in scope. Everything else (retiring the native engine, chain position before/after
the model, licenses, lsfg priority) is **out of scope**.

### Block 1 — Live tuning basics (session + hotkeys + gui)

- **1a. Multiplier chip with the lsfg-vk layer engine.** Today `Runner::cycle_multiplier`
  (`pipeline.rs`) shows the OSD «La capa lee el multiplicador…» and returns. New behaviour with the
  layer: cycle Off → ×2 → ×3 → ×4 → Off (**no Auto**: Auto needs the built-in engine), the label
  updates immediately, OSD «Generación: ×3 (reinicia la sesión…)» / «Frame generation: ×3 (restarts
  the session…)», and after `MULTIPLIER_APPLY_AFTER` = 1.2 s without further clicks send
  `Event::SaveSetting { profile: None, key: "frame_gen_multiplier", value }` (a global key). The
  GUI's `save_setting` (`store.rs`) then sets `Saving.restart = true` when
  `key == "frame_gen_multiplier"` && the engine is the layer && a session is `Running`, so that
  `Command::Restart` goes out after `ReloadConfig` (same as the tray's `set_multiplier` +
  `RestartScaling`, same flag as «Guardar y reiniciar»). With the Native engine keep today's live
  path. Pure helper `next_multiplier_layer(cur: Option<u8>) -> Option<u8>` in `pipeline.rs` with a
  unit test.
- **1b. Mode chip with a single mode.** When `SessionSetup.modes()` returns <= 1 entry, a click on
  the chip shows the OSD «Solo hay un modo: creá otro en LirtUp Forge» / «Only one mode: create
  another in LirtUp Forge» and does nothing else.
- **1c. New `HotkeyAction` variants** (`lirtup-config/src/hotkeys.rs`, serde snake_case, `as_str`
  = YAML key): `ModeNext` (`mode_next`, default `Alt+Shift+N`), `ModePrev` (`mode_prev`,
  `Alt+Shift+B`), `SwapAb` (`swap_ab`, `Alt+Shift+X`), `ToggleCompare` (`toggle_compare`,
  `Alt+Shift+C`). `Runner::handle_hotkey`: `ModeNext`/`ModePrev` reuse the chip's
  `cycle_mode(forward)` (same 300 ms debounce, `MODE_APPLY_AFTER`); `SwapAb` and `ToggleCompare` →
  block 1d / block 3.
- **1d. A/B by blinking (`SwapAb`).** The `Runner` keeps `ab_previous` = the working config before
  the last `update_config` that changed anything the presenter/upscaler cares about (mode keys,
  scaler, linear/screen effects, model; ignore `toolbar_pinned` / windowed position keys). `SwapAb`
  swaps current and previous (apply the previous through the same update path, keep the other as
  the new previous), OSD «A: configuración anterior» / «B: configuración actual» (en: «A: previous
  settings» / «B: current settings»). With no previous: OSD «Nada que comparar todavía» / «Nothing
  to compare yet». Nothing is written to disk; it lasts the session.

### Block 2 — «Probar en vivo» (preview) in the mode editor (proto-engine + gui + session)

- **2a. Protocol** (`lirtup-proto`): `Command::Preview { profile, mode, raw }` and
  `Command::PreviewEnd`; `SessionView` gets `#[serde(default)] pub preview: bool`.
  `PROTOCOL_VERSION` stays 1. `roundtrip.rs` and `lirtup-cli` updated. (**Done in the contract.**)
- **2b. Engine** (`lirtup-core/src/engine.rs`): the preview lives in the engine's running state.
  `resolve_for(window, profile, mode)` applies it: if the mode id that would be resolved (the
  override, else the profile's named mode) == `preview.mode`, resolve with a cloned `ModeStore`
  whose entry for that id is replaced by the preview's `raw` (both `modes` and `raw` maps), then the
  existing `resolve_with_mode`. `Command::Preview` stores it and sends
  `PipelineCommand::UpdateConfig { config, profile, mode, preview: true }` to the live session.
  `Command::PreviewEnd` clears it and does a normal reload (`preview: false`). `ReloadConfig` keeps
  the preview (the GUI sends `PreviewEnd` itself after saving). A preview is cleared when the
  session stops and when the client that sent it disconnects. `SessionView.preview` reflects it.
- **2c. Pipeline** (session): `PipelineCommand::UpdateConfig` gains `preview: bool`. The `Runner`
  stores it, `session_view().preview` = it, OSD on transitions: «Vista previa: cambios sin guardar»
  / «Preview: unsaved changes» when it turns on, «Vista previa terminada» / «Preview ended» when it
  turns off. `ToolbarState` gains `preview: bool` and the mode chip shows it (tertiary tone with a
  dot, like temporary).
- **2d. GUI** (`lirtup-ui`): a toggle «Probar en vivo» / «Try live» in the SaveBar
  (`app.rs::SaveBar`), visible only when `save_bar_scope()` is `Scope::Mode(id)` AND the session is
  `Running` AND `view.mode == Some(id)`. `Store.preview: Option<PreviewState>`,
  `Action::TogglePreview`. While on, every `SetField`/`Inherit`/`Revert` on `Scope::Mode(that id)`
  schedules a `Command::Preview` with the current edited raw mapping after a 150 ms debounce.
  Save → after the writes land (`written()`), send `Command::PreviewEnd` before
  `Command::ReloadConfig` and clear the preview. Revert-all/Undo → `PreviewEnd` + clear. Leaving the
  editor (route change) or the session stopping → `PreviewEnd` + clear. The session card
  (`home/session.rs`) shows a «vista previa» / «preview» badge when `view.preview`. Tests in
  `store.rs` following `save_setting_from_the_bar` / `save_and_restart_*`.

### Block 3 — Split comparison on a frozen frame (presenter + session)

Semantics: **A = `ab_previous`** (block 1d), **B = current**. Toggle with the toolbar button
(`ToolbarAction::Compare`, icon `"compare"`) or the `ToggleCompare` hotkey.

- **3a. Runner** (session): if `ab_previous` is `None` → OSD «Nada que comparar todavía» and
  no-op. Else: freeze = stop pulling new frames (keep presenting the last one, keep OSD/toolbar
  overlays and the pointer alive; this is NOT `set_paused`, which hides the overlay),
  `presenter.begin_compare(&previous_config)?`, `presenter.set_split(0.5)`, OSD «Comparación: ←
  anterior · actual →» / «Compare: ← previous · current →». Pointer: while comparing, a button-1
  press/drag anywhere on the overlay (outside the bar) moves the split to `x / overlay_width`. The
  router delivers pointer events for the whole overlay: `ToolbarZone.whole: bool` (when true every
  pointer event is routed to the session as a toolbar-style `PointerEvent` and, in
  `top-transparent`, the XFixes input region is the full overlay); `surface.rs` applies it. Toggling
  again → `presenter.end_compare()`, unfreeze, OSD «Comparación terminada» / «Compare ended».
  Compare also ends on any `update_config`, mode change, `SwapAb`, window switch,
  `toggle_windowed`, resize. The toolbar button is highlighted while comparing
  (`ToolbarState.comparing`).
- **3b. Presenter** (`presenter.rs` + a new compute shader in `lirtup-effects`): see §2.4 for the
  API. Composition: a small compute pass `split_compose` (HLSL + precompiled `.spv` committed under
  `crates/lirtup-effects/shaders/misc/split_compose/`, registered in `assets.rs` like
  `overlay_blend`; the worker may run dxc/glslang to produce the `.spv`, but no cargo) that writes
  `screen_out = x < split ? screen_a : screen_b`, with a 2 px vertical line (white, 70 % alpha) and a
  small handle. Both `present()` and `present_unchanged()` must show the composition while active,
  and the overlays (OSD/panel/toolbar) draw on top of the composed image. FrameGen (built-in engine)
  keeps receiving the composed screen. Memory: `screen_a` + A's intermediates only while active.
  Unit tests for the pure parts (split clamping, plan building for A); GPU tests `#[ignore]`.

### Block 4 — MagpieFX: portable cache key + precompiled seed (magpiefx)

- **4a.** `compiler.rs`: the cache key must not contain the absolute include path (`-I<dir>`):
  every AppImage launch misses today because `/tmp/.mount_*` changes. Replace the `-I` path in
  `cache_key()` with the stable marker `-I<effect-dir>`; `include_digest` already covers the include
  contents. Bump `CACHE_VERSION` to `"rs2"`. **Contract decision: do NOT put the compiler version in
  the key** (see §6, magpiefx notes: the seed must hit for any dxc/glslang version, and with none).
- **4b. Seed lookup:** `SpirvCache::new(dir)` also learns a read-only seed directory from
  `lirtup_config::paths::magpiefx_seed_dir()`: `$LIRTUP_MAGPIEFX_SEED_DIR`, else
  `<builtin effects dir>/../magpiefx-seed` (i.e. `/usr/share/lirtup/magpiefx-seed`; in the AppImage
  the launcher sets the env var). Lookup order: user cache → seed. Hits from the seed are used
  as-is (no copy). Nothing outside the magpiefx crate + `paths.rs` changes (`MagpieContext` in
  `presenter.rs`, `CompileEffect` in `engine.rs` and `bench.rs` keep calling `SpirvCache::new`).
- **4c. Seed generator:** `crates/lirtup-magpiefx/src/bin/magpiefx-seed.rs` (evolve
  `examples/compile_all.rs`; keep the example or delete it). CLI: `--out DIR`, `--jobs N`,
  `--fp16 both|on|off` (default both: FP32 always, FP16 for effects that declare the capability),
  `--effects a,b` (default all builtin). For every effect and every swap combination a session can
  request (`(in, out)` ∈ {ff, tf, ft, tt}; dedupe by the actual key) write the `.spv` files into DIR
  with the runtime's key scheme. dxc first, glslang fallback (the 3 Bilateral effects). Exit 1 on
  any failure, print a summary. No GPU needed.
- **4d. Packaging:** `scripts/build-release-linux.sh` runs the generator
  (`cargo run --release -p lirtup-magpiefx --bin magpiefx-seed -- --out build/magpiefx-seed`) before
  `install-tree.sh`; `packaging/linux/install-tree.sh` copies `$LIRTUP_SEED` (default
  `build/magpiefx-seed`, skip with a warning if missing) to `PREFIX/share/lirtup/magpiefx-seed`;
  `packaging/linux/launcher.sh` exports `LIRTUP_MAGPIEFX_SEED_DIR`; `packaging/aur/PKGBUILD`
  `build()` generates the seed (`makedepends += directx-shader-compiler glslang`) and `.SRCINFO`
  follows; `.github/workflows/release.yml` installs `glslang-tools` and dxc if a package exists,
  else the seed step must not fail the release (document). `docs/user/magpiefx.md`: a short
  «Semilla precompilada» section. Unit tests: key is path-independent (same key from two include
  dirs with the same contents), seed fallback hit, `CACHE_VERSION` bump.

### Cross-cutting rules

- Commits in English, clear subject lines. **No push, ever.** No version bump (stays 2.1.0).
  `CHANGELOG.md` is written by the integrator from the workers' notes under `## Unreleased`:
  workers do NOT touch `CHANGELOG.md`.
- OSD strings in the pipeline use the inline `tr("es", "en")` helper. GUI strings go in
  `i18n/{es,en}/*.ftl` (both).
- **Workers do not run cargo at all** (no build/check/test/fmt/clippy/run). Running
  dxc/glslang/git/python is fine.
- Shared files (`Cargo.toml`, `mod.rs`/`lib.rs`, `*.ftl`, `config-example.yaml`) only with
  additive edits.
- New code must be unit-testable: extract pure helpers; `Runner`/`Presenter` need X11/GPU and
  cannot be tested.

---

## 2. Shared interfaces (fixed; already in the code as stubs or real)

`R` = real in the contract commit, `S` = stub to be filled by the named worker.

### 2.1 `lirtup-proto` (R, owner proto-engine)

`crates/lirtup-proto/src/lib.rs`, in `enum Command` (after `PauseHotkeys`, before `Quit`):

```rust
Preview {
    profile: Option<String>,          // informational: the session's profile as the GUI saw it
    mode: String,                     // mode id (file stem of modes/<id>.yaml)
    raw: lirtup_config::Mapping,      // = serde_yaml_ng::Mapping, the mode file + unsaved edits
},
PreviewEnd,
```

Wire: `{"v":1,"type":"Preview","profile":"Ys VIII","mode":"calidad","raw":{"name":"Calidad","model":"4x12"}}`
and `{"v":1,"type":"PreviewEnd"}`. `raw` keys are strings (mode keys); values are any YAML value
that JSON can carry (the codec is JSON; `Event::SaveSetting.value` already does the same).

`crates/lirtup-proto/src/types.rs`, `SessionView` (after `mode_temporary`):

```rust
#[serde(default)]
pub preview: bool,
```

Already updated: `tests/roundtrip.rs` (`commands()` has two `Preview` and one `PreviewEnd`, the
coverage test counts 20 commands, `session()` sets `preview: false`, new test
`a_session_view_of_2_1_has_no_preview`); `lirtup-cli/src/main.rs` (`answer_timeout` = 1 s for both,
`answers` = none, `NAMES` has 18 entries incl. `Preview`, `PreviewEnd`). Every `SessionView { .. }`
literal in the workspace got `preview: false`.

### 2.2 `lirtup-config` hotkeys (R, owner hotkeys)

`crates/lirtup-config/src/hotkeys.rs`: `HotkeyAction::{ModeNext, ModePrev, SwapAb, ToggleCompare}`
appended after `ToggleToolbar`; `ALL: [HotkeyAction; 20]` ends with them in that order; `as_str`
= `mode_next`, `mode_prev`, `swap_ab`, `toggle_compare`; `default_binding` = `Alt+Shift+N`,
`Alt+Shift+B`, `Alt+Shift+X`, `Alt+Shift+C`. The engine forwards them to the session unchanged
(its `hotkey()` sends every non-activation action as `PipelineCommand::Hotkey`). The overlay also
forwards them (`OverlayNotice::Hotkey`). The model.rs test uses `ALL.len() - 1`: no count to fix.

GUI (`screens/settings/hotkeys.rs`, R): new group `("settings-hotkeys-tuning", &[ModeNext, ModePrev,
SwapAb, ToggleCompare])` (5th and last of `GROUPS`); icons `mode_next → "line_end"`,
`mode_prev → "line_start"`, `swap_ab → "swap_horiz"`, `toggle_compare → "difference"` (all already
in the GUI's woff2 subset; any other icon must be one of `design/tokens.rs` `ICONS`, or the woff2
must be regenerated). ftl keys (es+en, in `settings.ftl`): `settings-hotkeys-tuning`,
`settings-hotkey-mode_next`, `settings-hotkey-mode_prev`, `settings-hotkey-swap_ab`,
`settings-hotkey-toggle_compare`. The hotkeys worker may reword them.

### 2.3 `lirtup-pipeline` (owner session unless noted)

`crates/lirtup-pipeline/src/pipeline.rs`:

```rust
pub enum PipelineCommand {
    UpdateConfig { config: Arc<Config>, profile: Option<String>, mode: Option<String>, preview: bool }, // R
    ...
}
pub const MULTIPLIER_APPLY_AFTER: Duration = Duration::from_millis(1200);              // R
pub(crate) fn next_multiplier_layer(cur: Option<u8>) -> Option<u8>                      // S (returns cur)
// Runner fields (private): preview: bool (R, set from UpdateConfig), ab_previous: Option<Config> (S),
// frozen: bool (S), comparing: bool (S)
```

`next_multiplier_layer` semantics: `None | Some(0) | Some(1)` → `Some(2)`; `Some(2)` → `Some(3)`;
`Some(3)` → `Some(4)`; `Some(4..)` → `None` (off). The value saved for off is `1` (as today's
`Fixed(1)`), else `n`. `cur` is the pending value if a click is waiting, else the configured
`frame_gen_multiplier` (`Fixed(n)` → `Some(n)`, `Auto` → treat as `None`, so the next is ×2).

`handle_command` already stores `self.preview = preview` before `reload_config` (TODO: OSD on
transitions, end the comparison). `session_view()` already sets `preview: self.preview`;
`update_toolbar` already fills `preview`/`comparing` from the Runner. `handle_hotkey` has a no-op
arm for the four new actions and `toolbar_action` a no-op arm for `Compare` (TODO(session)).
The `Runner`'s private fields may be reshaped by the session worker (e.g. `ab_previous` as a
private struct that also carries `profile`/`mode`/`named_mode`/`mode_override`, so the mode chip
does not lie after a swap); only `PipelineCommand::UpdateConfig.preview`, `MULTIPLIER_APPLY_AFTER`
and `next_multiplier_layer` are shared names.

`crates/lirtup-pipeline/src/toolbar.rs` (R):

```rust
pub const ICON_NAMES: [&str; 12] = [..., "animation", "compare", "visibility_off", ...];
pub enum ToolbarAction { ..., Multiplier, Compare, Hide, ... }
pub struct ToolbarState { ..., pub mode_temporary: bool, pub preview: bool, pub comparing: bool, ... } // Default = false
```

The `"compare"` glyph (U+E3B9) is **already in the bar's font instances**
(`crates/lirtup-ui/assets/fonts/MaterialSymbolsRounded-Fill{0,1}.ttf`, regenerated by the contract
with `scripts/gen-tokens-icons.py`) and in `crates/lirtup-ui/design/{icons.json,tokens.rs}`
(`ICONS: [(&str, u32); 173]`, `("compare", 0xE3B9)`). The GUI woff2 was **not** regenerated
(it had unrelated pending differences; the GUI does not use `"compare"`). Nobody needs to touch
fonts or tokens in this round. The session worker places the button (suggested: right after the
mode chip / save button, before the multiplier) and highlights it like `Stats` when
`state.comparing`. `Toolbar::zone` sets `whole: false` today (TODO: `self.state.comparing`).

### 2.4 `Presenter` (S, owner presenter)

`crates/lirtup-pipeline/src/presenter.rs`, `impl Presenter`:

```rust
pub fn begin_compare(&mut self, other: &Config) -> Result<()>   // stub: Ok(())
pub fn set_split(&mut self, x: f32)                              // stub: no-op
pub fn compare_active(&self) -> bool                             // stub: false
pub fn end_compare(&mut self)                                    // stub: no-op
```

Contract semantics (the session relies on them):

- `begin_compare(other)`: A = `EffectPlan::from_config(other)` (pre/magpie, scaler from `other`'s
  sampler settings and geometry, linear, convert, screen passes) over the **same source texture**
  (`self.source`, the shared upscaler output: models are not compared); B = this presenter's own
  chain. Renders A once into `screen_a` and B once into `screen_b` (both screen-sized, 8-bit like
  `screen`). Calling it while active rebuilds A. Errors (e.g. a MagpieFX compile failure in A) are
  returned and leave the presenter in the normal (non-compare) state. Split starts at 0.5.
- `set_split(x)`: clamps to `0..=1` (NaN → 0.5) and remembers it; the next `present()` composes
  with it. The session sets `stale = true` after calling it.
- While active, `present(overlays, timed)` does **not** re-run either chain (the session froze
  the frame and does not call `set_source` with new content): it records `split_compose(screen_a,
  screen_b, split) → screen`, then the overlays, then presents (or hands `screen` to `FrameGen`).
  `present_unchanged()` presents `screen` as it is (already composed, with overlays).
- `resize()` and `update_config()` end the comparison internally (the session ends it first
  anyway). `end_compare()` drops `screen_a`, `screen_b`, A's passes and the compose pass.
- Suggested pure helpers (unit-tested): `fn clamp_split(x: f32) -> f32`, and A's plan building via
  the existing `EffectPlan::from_config` (test that A's plan differs only where `other` differs).
- New Rust pass: `crates/lirtup-effects/src/compose.rs` (new file, presenter-owned; `pub mod
  compose;` additive in `lirtup-effects/src/lib.rs`), e.g. `pub struct SplitCompose` implementing
  `Pass`, `SplitCompose::new(device, a: &Texture, b: &Texture, out: &Texture) -> Result<_>`,
  `set_split(&mut self, x: f32)`. Shader at
  `crates/lirtup-effects/shaders/misc/split_compose/split_compose.{hlsl,spv}`, registered in
  `assets.rs` as `"misc/split_compose/split_compose.spv"` next to `overlay_blend`. Build the `.spv`
  with the same flags as `overlay_blend.spv` (see `scripts/hlsl-to-spv.fish`). Line: 2 px wide
  (times the UI scale is fine), white at 70 % alpha, plus a small handle (e.g. a 24×48 px rounded
  box or circle centred vertically on the line). The presenter worker owns everything inside
  `presenter.rs` and `lirtup-effects`; the session only uses the four methods above.

### 2.5 `lirtup-x11` overlay input (R field, S behaviour, owner session)

`crates/lirtup-x11/src/overlay_input.rs`:

```rust
pub struct ToolbarZone { pub bar: Option<Rect>, pub reveal: u32, pub whole: bool }
```

`whole == false` keeps today's routing exactly. `whole == true`: `ToolbarRouter::route` returns
`Routed::Toolbar(PointerEvent::…)` for **every** pointer event (motion, press, release of any
button, wheel) and `Routed::Both(PointerEvent::Leave)` on leave; nothing is forwarded to the game.
`surface.rs` (`set_toolbar_zone`) makes the XFixes input region the full overlay in
`top-transparent` while `whole`, and restores the bar rect after. Since the bar now receives events
outside itself, the session filters: events inside the bar rect (or the reveal band while hidden)
go to `Toolbar::pointer`; the others drive the split (button-1 press starts a drag and sets the
split, motion while dragging moves it, release ends it). A helper such as
`pub fn Toolbar::hit(&self, x, y) -> bool` is the session's to add.

### 2.6 `lirtup-core` engine (S, owner proto-engine)

`crates/lirtup-core/src/engine.rs`:

```rust
struct Running { session, window, profile, preview: Option<Preview> }            // R field, None at start
struct Preview { profile: Option<String>, mode: String, raw: lirtup_config::Mapping } // R
impl Engine { pub fn end_preview(&self) }                                          // S: no-op
// Inner::handle: Command::Preview { .. } / Command::PreviewEnd → warn "not implemented yet" (S)
// Inner::reload: UpdateConfig { .., preview: false } (S: must be running.preview.is_some())
```

Contract decisions for block 2b:

- **Where it lives:** `Running.preview` (dies with the session: stop, replace, restart). A
  `Command::Preview` with no live session is ignored (debug log).
- **Resolution:** `resolve_for(window, profile, mode)` reads the preview (lock `state`, clone
  `live().preview`, **release the lock**, then lock `files` — never hold both; `resolve_for` is also
  called from the session thread through `SessionSetup.resolve`). Target id = `mode` arg if `Some`,
  else the mode the profile layer names (resolve once normally and use `Resolved.named_mode`, or a
  small helper in `resolve.rs`). If it equals `preview.mode`: clone `files.modes`, set
  `modes.modes[id] = Mode::from_mapping(id, &raw).0` and `modes.raw[id] = raw.clone()` (note: the
  function is `lirtup_config::Mode::from_mapping`, there is no `ModeStore::from_mapping`), then
  `resolve_with_mode` on the clone.
- **`Command::Preview`:** store it in `Running.preview`, resolve with `mode = None` (the session's
  temporary mode override is NOT known to the engine) and send
  `UpdateConfig { config, profile, mode, preview: true }`. The Runner's `reload_config` keeps its own
  override as today and, if it has one, re-resolves through `SessionSetup.resolve` → `resolve_for`
  with `Some(override)`, which applies the preview too when the override is the previewed mode. So
  the preview shows whether the session's mode is the profile's or a temporary one.
- **`Command::PreviewEnd`:** clear it, then the same path as `reload()` without re-reading files
  (or simply `reload()`), with `preview: false`.
- **`ReloadConfig`:** keeps the preview; `UpdateConfig.preview = running.preview.is_some()`.
- **Disconnect:** `server.rs` keeps a per-connection flag like `Counted` (`WatchGpuTimes`): a
  `Preview` from this connection sets it (the command still goes to `engine.handle`), `PreviewEnd`
  clears it, and dropping the connection with the flag set calls `Engine::end_preview()`. If the
  per-connection hook turns out not to be cheap, document it as a deviation in your notes.
- Update the module docs' command table (`Preview`, `PreviewEnd`).

### 2.7 `lirtup-ui` (S, owner gui)

`crates/lirtup-ui/src/state/mod.rs` (R types):

```rust
pub struct Store { ..., pub preview: Option<PreviewState> }
pub const PREVIEW_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(150);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewState { pub mode: ModeId, pub profile: Option<String> }
pub enum Action { ..., SaveAndRestart, TogglePreview, ... }
pub enum Effect { ..., Toast(Toast), PreviewLater(Command), ... }
```

Stubs: `store::update` has `Action::TogglePreview => None`; `app.rs::run_effect` has
`Effect::PreviewLater(command) => core.send(command)` (no debounce yet). The gui worker may reshape
anything inside its files (e.g. replace `Effect::PreviewLater` by another mechanism), as long as the
store stays unit-testable: the store returns an effect carrying the `Command`; the debounce lives
in `run_effect` (generation counter; a pending `PreviewLater` must never be delivered after a
`PreviewEnd` that came later).

ftl keys added (es + en, placeholder-quality text, reword freely): `app.ftl`:
`savebar-try-live` («Probar en vivo» / «Try live»), `savebar-try-live-hint`; `home.ftl`:
`home-session-preview` («vista previa» / «preview»).

Contract decisions for 2d:

- **Visibility:** `save_bar_scope()` is `Some((Scope::Mode(id), _))`, `store.session` is
  `SessionState::Running(view)` and `view.mode.as_deref() == Some(id)`. (Shown even with 0 dirty
  edits? The save bar itself may hide at 0 edits; the gui worker decides where the toggle lives so
  it is reachable — e.g. keep the bar visible while the toggle is on.)
- **TogglePreview on:** `store.preview = Some(PreviewState { mode: id, profile: view.profile })`
  and `Effect::Send(Command::Preview { profile, mode: id, raw })` immediately, where `raw` = the
  mode's `store.modes.store.raw[id]` with that mode's edits applied (same merge as
  `apply_mode_edits`, which skips unchanged modes: write a helper `preview_raw(store, id) ->
  Option<Mapping>` that always returns the merged mapping). **Off:** clear, `Send(PreviewEnd)`.
- **Edits:** `SetField`/`Inherit`/`Revert` on `Scope::Mode(id)` with `store.preview.mode == id` →
  join `Effect::PreviewLater(Command::Preview { .. })`.
- **Save:** in `written()`, when the last write lands without error and `store.preview` is `Some`:
  effects in order `Send(PreviewEnd)`, `Send(ReloadConfig)`, (`Send(Restart)`), toast; clear the
  preview. On a failed write keep the preview.
- **Discard / Undo / reset of the mode page**, **route change away from `Route::Modes { id:
  Some(id) }`**, **`Event::SessionStopped`** (and a `SessionStarted` whose `mode` is no longer
  `id`): clear the preview; send `PreviewEnd` except after `SessionStopped` (harmless either way).
- **Multiplier restart (1a):** in `save_setting`, after `write_structure(..)`, if
  `key == "frame_gen_multiplier"` && `store.framegen.engine == FrameGenEngine::Layer` &&
  `matches!(store.session, SessionState::Running(_))` → set `store.saving.restart = true` (the
  saving that `write_structure` just began); `written()` then sends `Restart` after `ReloadConfig`.
- **Session card:** badge «vista previa» when `view.preview` (next to the mode, like temporary).
- Tests in `store.rs`: toggle on sends `Preview` with merged raw; an edit returns a
  `PreviewLater`; save sends `PreviewEnd` before `ReloadConfig`; navigate away / stop clears;
  `save_setting` of `frame_gen_multiplier` with layer + running restarts after the reload, and not
  with the native engine or without a session.

### 2.8 `lirtup-magpiefx` + paths (S, owner magpiefx)

```rust
// crates/lirtup-config/src/paths.rs (R const, S fn)
pub const MAGPIEFX_SEED_DIR_ENV: &str = "LIRTUP_MAGPIEFX_SEED_DIR";
pub fn magpiefx_seed_dir() -> Option<PathBuf>   // stub: the env var only (non-empty)
// crates/lirtup-magpiefx/src/compiler.rs
pub struct SpirvCache { pub dir: PathBuf, pub seed: Option<PathBuf> }   // stub: new() sets seed: None
// crates/lirtup-magpiefx/Cargo.toml: [[bin]] name = "magpiefx-seed", path = "src/bin/magpiefx-seed.rs"
// crates/lirtup-magpiefx/src/bin/magpiefx-seed.rs: fn main() stub (prints "not implemented", exit 1)
```

`magpiefx_seed_dir()` final semantics: `$LIRTUP_MAGPIEFX_SEED_DIR` (non-empty), else
`$LIRTUP_EFFECTS_DIR/../magpiefx-seed` when that env var is set, else
`/usr/share/lirtup/magpiefx-seed` (`lirtup-config` cannot call `lirtup_magpiefx::catalog::
builtin_dir`; mirror its env/system logic, without the debug-build repo fallback). It returns a path
even if it does not exist; a missing folder simply never hits.

---

## 3. Ownership (no overlap; anything not listed belongs to nobody — ask first)

| Worker | Files |
| --- | --- |
| proto-engine | `crates/lirtup-proto/src/lib.rs`, `crates/lirtup-proto/src/types.rs`, `crates/lirtup-proto/tests/roundtrip.rs`, `crates/lirtup-cli/src/main.rs`, `crates/lirtup-core/src/engine.rs`, `crates/lirtup-core/src/server.rs`, `crates/lirtup-core/src/lifecycle.rs` (only if needed), `crates/lirtup-config/src/resolve.rs` (only if a helper is needed) |
| session | `crates/lirtup-pipeline/src/pipeline.rs`, `crates/lirtup-pipeline/src/toolbar.rs`, `crates/lirtup-pipeline/src/osd.rs`, `crates/lirtup-pipeline/src/surface.rs`, `crates/lirtup-x11/src/overlay_input.rs`, `crates/lirtup-pipeline/src/tokens.rs`, `crates/lirtup-ui/design/tokens.rs` + the bar's `.ttf` instances (only if yet another icon is added; `compare` is already there) |
| hotkeys | `crates/lirtup-config/src/hotkeys.rs`, `crates/lirtup-config/src/model.rs` (ONLY the hotkeys test near line 1322), `crates/lirtup-ui/src/screens/settings/hotkeys.rs`, `crates/lirtup-ui/i18n/{es,en}/settings.ftl`, `config-example.yaml` (hotkeys section only), `docs/user/controls.md` (all of it: the new hotkeys AND the bar changes of this round — multiplier with the layer, single-mode OSD, compare button, A/B — written from this contract) |
| presenter | `crates/lirtup-pipeline/src/presenter.rs`, `crates/lirtup-effects/src/assets.rs`, `crates/lirtup-effects/src/osd.rs`, `crates/lirtup-effects/src/ui_draw.rs`, `crates/lirtup-effects/shaders/misc/**` (new `split_compose/`), `crates/lirtup-effects/src/compose.rs` (new), `crates/lirtup-effects/src/lib.rs` (additive), `crates/lirtup-effects/src/util.rs` (additive) |
| gui | `crates/lirtup-ui/src/state/store.rs`, `crates/lirtup-ui/src/state/mod.rs`, `crates/lirtup-ui/src/app.rs`, `crates/lirtup-ui/src/link.rs`, `crates/lirtup-ui/src/route.rs`, `crates/lirtup-ui/src/screens/modes/**`, `crates/lirtup-ui/src/screens/home/**`, `crates/lirtup-ui/src/screens/profile/**`, `crates/lirtup-ui/src/tray.rs`, `crates/lirtup-ui/src/notify.rs`, `crates/lirtup-ui/i18n/{es,en}/{app,home,modes,profile,components}.ftl`, `docs/user/modes.md` |
| magpiefx | `crates/lirtup-magpiefx/**` (src, src/bin, examples, Cargo.toml, tests), `crates/lirtup-config/src/paths.rs`, `packaging/**`, `scripts/build-release-linux.sh`, `.github/workflows/release.yml`, `docs/user/magpiefx.md` |
| integrator only | `CHANGELOG.md`, `docs/developer/live-tuning-integration.md` |

## 4. Strings

### OSD (pipeline, `tr(es, en)`, session worker)

| When | es | en |
| --- | --- | --- |
| Multiplier with the layer (click) | `Generación: ×3 (reinicia la sesión…)` | `Frame generation: ×3 (restarts the session…)` |
| … to off | `Generación: No (reinicia la sesión…)` | `Frame generation: Off (restarts the session…)` |
| Mode chip/hotkey with ≤ 1 mode | `Solo hay un modo: creá otro en LirtUp Forge` | `Only one mode: create another in LirtUp Forge` |
| SwapAb shows the previous | `A: configuración anterior` | `A: previous settings` |
| SwapAb shows the current | `B: configuración actual` | `B: current settings` |
| SwapAb / compare with no previous | `Nada que comparar todavía` | `Nothing to compare yet` |
| Preview turns on | `Vista previa: cambios sin guardar` | `Preview: unsaved changes` |
| Preview turns off | `Vista previa terminada` | `Preview ended` |
| Compare on | `Comparación: ← anterior · actual →` | `Compare: ← previous · current →` |
| Compare off | `Comparación terminada` | `Compare ended` |

(The «creá» voseo is the user's wording; keep it.)

### GUI (Fluent, both languages)

`settings.ftl`: `settings-hotkeys-tuning`, `settings-hotkey-{mode_next,mode_prev,swap_ab,toggle_compare}`
(hotkeys). `app.ftl`: `savebar-try-live`, `savebar-try-live-hint`; `home.ftl`:
`home-session-preview` (gui). Any further key is the owner's, in both languages; the
`i18n_check` test (`both_languages_are_complete`) must keep passing.

## 5. Constants

| Name | Value | Where | Use |
| --- | --- | --- | --- |
| `MODE_APPLY_AFTER` | 300 ms | `pipeline.rs` (exists) | mode chip and `ModeNext`/`ModePrev` hotkeys |
| `MULTIPLIER_APPLY_AFTER` | 1.2 s | `pipeline.rs` (new, R) | layer multiplier: `SaveSetting` after the last click |
| `PREVIEW_DEBOUNCE` | 150 ms | `lirtup-ui/src/state/mod.rs` (new, R) | GUI → `Command::Preview` after the last edit |
| `CACHE_VERSION` | `"rs1"` → `"rs2"` | `compiler.rs` | magpiefx |

## 6. Merge order (integrator)

1. `claude/lt-magpiefx`
2. `claude/lt-hotkeys`
3. `claude/lt-proto-engine`
4. `claude/lt-presenter`
5. `claude/lt-session`
6. `claude/lt-gui`

Then the integrator builds and tests everything (`cargo test --workspace`, clippy, fmt), writes
`CHANGELOG.md` `## Unreleased` from the workers' notes and `docs/developer/live-tuning-integration.md`.

## 7. Notes for workers

General:

- Work only in your worktree and your branch; commit there (English subject lines; several commits
  are fine). No push. No cargo of any kind: write code that compiles by reading carefully (check
  imports, exhaustive matches, borrow scopes). Remove the `TODO(<you>)` markers and the
  `#[allow(dead_code)]` of your stubs when done.
- Do not rename or remove any shared name of §2. If you need a shared change, say so in your final
  notes (the integrator handles it) instead of editing another worker's file.
- End with notes for the integrator: what changed for users (for the CHANGELOG), deviations from
  this contract, what is untested (GPU/X11), and manual test steps.

proto-engine:

- `server.rs` must keep forwarding `Preview`/`PreviewEnd` to `engine.handle` (blocking task), and
  track the per-connection flag before forwarding.
- Add unit tests where possible: the "which id gets the preview" decision is a good pure helper
  (e.g. `fn preview_applies(target: Option<&str>, preview: &str) -> bool` plus the store-swap
  helper taking `&ModeStore` → `ModeStore`), and a `resolve_with_mode` on a cloned store with a
  replaced entry (the `lirtup-core` tests that need no X11/GPU).
- `lirtup ctl preview mode=calidad raw={"model":"4x12"}` should work through `parse_command`
  (values are JSON when they parse); add a parse test in `lirtup-cli`.

session:

- 1a: keep `cycle_multiplier` live for `FrameGenEngine::Native`. With the layer, keep a pending
  value + deadline like `PendingMode`, show it on the chip label, apply in the run loop (next to
  `apply_pending_mode`) by sending the `SaveSetting`. Do not change `self.config` (the layer reads
  the value at process start; the GUI's save + `Restart` does the rest). If no GUI is connected
  the save is lost, as for the rest of the bar (documented).
- 1d: the natural place to record `ab_previous` is `update_config` (every live change goes
  through it: `ReloadConfig`, the mode chip, previews, `SwapAb` itself — which then leaves the other
  side as the new previous). `CycleModel`/geometry/zoom change `self.config` directly and do not
  count. Compare with a pure helper (e.g. `fn ab_relevant_changed(old, new) -> bool`, at least
  `old.scaling != new.scaling`) and unit-test it.
- 2c: the preview flag arrives with every `UpdateConfig`; OSD only on false→true / true→false.
  The chip: tertiary tone + a dot while `state.preview` (relayout when it changes: add it to the
  relayout condition in `Toolbar::update`).
- 3a: frozen frame loop: skip capture/upscale; when `redraw || stale` call
  `presenter.present(&overlays, timed)` (it composes), else idle-wait. With FrameGen active the
  idle path must still present on changes. End the comparison (and unfreeze) on `update_config`,
  mode apply, `SwapAb`, `switch_window`, `toggle_windowed`, `configured`/resize, `teardown`.
  `whole` follows `comparing` (via `Toolbar::zone` or by the Runner before
  `SetToolbarZone`). Pointer x → `x as f32 / overlay_width as f32`.
- Keep `cycle_mode` for both the chip and the hotkeys; 1b's OSD lives there (`self.modes.len() <=
  1`; note `mode_label()` hides the chip when the list is empty — the hotkey path still shows the
  OSD).

presenter:

- See §2.4. Keep memory only while active. Unit tests: `clamp_split`, A plan building. A GPU test
  (`#[ignore]`) that composes two solid colours and reads back left/right pixels is welcome.

gui:

- See §2.7. `Effect::PreviewLater` handling in `app.rs`: a `Signal<u64>`/atomic generation; each
  `PreviewLater` bumps it and spawns a 150 ms sleep that sends only if the generation is unchanged;
  `Send(PreviewEnd)` bumps it too.
- The tray (`tray.rs`) needs no change unless you want a «vista previa» hint; optional.
- `docs/user/modes.md`: a short «Probar en vivo» section.

hotkeys:

- `config-example.yaml`: add the four keys with comments in the hotkeys section only.
- `docs/user/controls.md`: the four hotkeys (table/list as the file does), and the bar: the
  multiplier chip with the layer (Off → ×2 → ×3 → ×4, saved after 1.2 s, restarts the session; needs
  the GUI open), the single-mode OSD, the compare button (frozen frame, drag the line, A = previous
  settings, B = current), A/B blinking.
- Check that the new default bindings do not collide with 1.x defaults (they do not today) and
  extend `every_action_is_in_one_group_with_its_text` only if needed (it iterates `ALL`).

magpiefx:

- **Seed hits must work with no compiler installed.** Today `SpirvCache::compile_with` returns
  `MagpieError::NoCompiler` before any cache lookup, and the key is computed per detected compiler.
  The seed matters most exactly when the user has no dxc/glslang (the AppImage). Make the key
  computable without a compiler instance (the args are already static: `Dxc::args` /
  `Glslang::args`), and look up, for each compiler kind in order (dxc, then glslang — or the
  detected order when there are compilers): user cache, then seed; only then compile with the
  detected compilers; `NoCompiler` only when nothing hit.
- No compiler version in the key (contract decision): a seed built with dxc X must hit for a user
  with dxc Y or none.
- The generator is a separate binary target: it cannot see `pub(crate)` items. Simplest: compile
  through the public runtime API into a cache whose `dir` is `--out` and whose seed is `None`
  (`runtime::compile_effect_with(entry, compilers, &cache, &CompileOptions { fp16, swap_input,
  swap_output })` for the four swap combinations and fp16 off/on, plus `runtime::compile_swap_rb`):
  the cache writes exactly the runtime's file names, and identical keys dedupe themselves. Count
  files before/after for the summary.
- In a release build `catalog::builtin_dir()` is `/usr/share/lirtup/effects` unless
  `LIRTUP_EFFECTS_DIR` is set: the generator needs `--effects-dir DIR` (default
  `$LIRTUP_EFFECTS_DIR`, else `concat!(env!("CARGO_MANIFEST_DIR"), "/effects")`) and
  `Catalog::scan_dirs(&dir, None)`. The include-dir marker (4a) is what makes keys from the build
  tree equal to keys from `/usr/share/lirtup/effects` or `/tmp/.mount_*`.
- `SpirvCache::new` reading the environment can make unit tests flaky: add
  `pub fn with_seed(dir: PathBuf, seed: Option<PathBuf>) -> SpirvCache` and use it in tests.
- `packaging/linux/launcher.sh` already exports `LIRTUP_EFFECTS_DIR` from `$prefix`; do the same
  for `LIRTUP_MAGPIEFX_SEED_DIR` (`$prefix/share/lirtup/magpiefx-seed`, only if the folder exists
  and the variable is unset).
