# fx-unify — the contract of the round

Round decided with the user on 2026-11-04. Base: `main` at `f8427714` (2.1.0 plus the merged
live-tuning round, MagpieFX SPIR-V seed included). This file is the only shared spec: every worker
codes against the names fixed here, without seeing the others.

- Integration branch: `fx-unify/integration`, worktree
  `/media/octavio/DATOS/Gitbutler Repos/lirtup-forge/.claude/worktrees/fu-integration`.
- The contract commit holds this file plus **compiling stubs** of every shared interface
  (`cargo check --workspace --all-targets` passes on it, `cargo test -p lirtup-config -p
  lirtup-pipeline -p lirtup-effects -p lirtup-ui` too). Stubs are marked `TODO(<worker>)`;
  `#[allow(dead_code)]` on a stub goes away when its worker uses it.
- Worker branches (all from `fx-unify/integration`), worktrees under
  `/media/octavio/DATOS/Gitbutler Repos/lirtup-forge/.claude/worktrees/`:

| Worker | Branch | Worktree |
| --- | --- | --- |
| config | `claude/fu-config` | `fu-config` |
| migrate | `claude/fu-migrate` | `fu-migrate` |
| effects | `claude/fu-effects` | `fu-effects` |
| presenter | `claude/fu-presenter` | `fu-presenter` |
| upscaler | `claude/fu-upscaler` | `fu-upscaler` |
| gui | `claude/fu-gui` | `fu-gui` |

Background reading: `docs/developer/live-tuning-contract.md` (the previous round, same rules),
`docs/developer/magpiefx-report.md` (the red/blue swap folded into the shaders, costs of the three
effects), `docs/developer/rust-r4-migrate.md` (the 1.x → modes migration this round extends).

**Important: the tree does not compile between merges.** The contract commit keeps every old name
(the native fields, `EffectsEngine`, `NativeEffect::{Anime4kS, LumaSharpen, CrtEasymode}`) and only
adds the new ones. Each worker removes the old names it owns and the uses of them in its own files
(§3). Removing a field in `model.rs` breaks `effects.rs`, `presenter.rs`, the GUI… in *your* branch:
that is expected; the other workers remove those uses in theirs. After the six merges (§6) nothing
refers to the old names and the integrator's build is the first one that must pass. So: **read the
section of every worker whose names you use**, and do not keep an old name alive "for the others".

---

## 1. The design (as decided with the user)

Two scope items. Everything else (licenses, lsfg priority, DDS lookup textures, new effects) is out
of scope. **Default behaviour does not change**: a mode that does not use the new key renders and
costs what it did (the native effects become the same effects through MagpieFX, in the same place).

### A. Retire the native effects engine

- `ModeSettings` loses `effects_engine` and the 24 native fields (`anime4k_enabled`,
  `luma_sharpen_{enabled,strength,clamp,pattern,offset_bias}`, `crt_enabled` + 17 `crt_*`); the
  enum `EffectsEngine` goes away. MODE_KEYS, defaults, validators (`range!`), CLI flags,
  `config-example.yaml`, the presenter's plan, `bench.rs`, the GUI (engine selector, native section,
  costs), i18n and docs follow. **The MagpieFX chain is the only effects chain and always runs**; no
  engine selector anywhere.
- The three hand ports (Anime4K_Upscale_S, LumaSharpen, CRT_Easymode: passes + shaders in
  `lirtup-effects`) stay **only as parity references**: module `lirtup_effects::ports` behind the
  non-default cargo feature `ports` (R: declared in the contract), turned on for tests by a self
  dev-dependency (`lirtup-effects = { path = ".", features = ["ports"] }`, checked to work with
  `cargo check --all-targets`) and by `lirtup-magpiefx`'s dev-dependency. `cargo test --workspace`
  builds the existing ignored `crates/lirtup-effects/tests/parity.rs` and the new ignored
  `crates/lirtup-magpiefx/tests/ports.rs` (each port against the same effect through MagpieFX). The
  ports' `.spv` are embedded only with the feature; the app binary carries none of them.
- **Old configs: translate on read + migrate** (user decision).
  1. Read-time translation, `lirtup_config::legacy::translate_native` (§2.2), applied by the
     resolver to the merged mapping of the **file layers** (top level, mode, profile, and a mode
     named by `--mode`) **before the command-line layer goes on top** and before
     `Config::from_mapping`. `lirtup serve`, the CLI and previews keep working with unmigrated files.
  2. `lirtup migrate` and the GUI's first-run dialog (`Dialog::Migrate`) also rewrite `config.yaml`
     (top level + profiles) and `modes/*.yaml` without legacy keys, with backups; criterion:
     resolving every profile × every mode gives the same `Config` before (read-time translation) and
     after (§2.3).
- **CLI: remove** `--effects-engine`, `--enable-anime4k`, `--enable-luma-sharpen`,
  `--luma-sharpen-*`, `--enable-crt`, `--crt-*`. `--magpie-effect` stays (after the model); new
  twin `--magpie-effect-before-model` (repeatable, same `Name` / `Name:param=value,...` form).

Known, documented edges (no code can tell them apart):

- A file with `magpie_effects` and **no legacy key at all** (no `effects_engine`, no native field)
  had an inactive chain (the default engine was native) and now runs it. The GUI always wrote
  `effects_engine: magpiefx` when the chain was chosen, so in practice only hand-written files.
- Legacy keys given on the command line (`--set crt_enabled=true`) are not translated: they are
  dropped with a warning («retired native engine setting, ignored; use --magpie-effect»).
- With legacy keys, every layer is read with the old rule, so a hand-edited mix (a migrated mode
  with a chain plus a loose `crt_enabled: true` in a profile) runs what it ran in 2.1: the CRT only.
- The image may differ by rounding: the native passes were hand ports; MagpieFX may run
  Anime4K_Upscale_S in FP16 (`CAPABILITY FP16`, `fp16_wanted`). `tests/ports.rs` measures it.

### B. Chain position relative to the model

Today: capture → upscaler (SRCNN/CuNNy, optional tiles) → presenter (deband → MagpieFX → scaler →
linear effects → screen effects). User decisions: **per-effect granularity, default after the
model**.

- **Representation: two lists.** New mode key `magpie_effects_before_model:
  Vec<MagpieEffectSpec>` (R, default empty, in MODE_KEYS right after `magpie_effects`; layered,
  inherited and shadowed exactly like `magpie_effects`). `magpie_effects` keeps its meaning (after
  the model). Short and long spec forms in both. An effect may be in both lists (it runs twice).
- **The order of a frame:**

  ```text
  capture ─upload─▶ [MagpieFX before the model] ─▶ model (SRCNN ×1/×2, or none)
          ─▶ presenter: [deband] ─▶ [MagpieFX after the model] ─▶ scaler ─▶ [CAS, bloom, vignette]
          ─▶ 8 bit ─▶ [LUT, grain] ─▶ overlays ─▶ swapchain | FrameGen
  ```

  (Deband stays in the presenter, after the model, as today.)
- **The chain before the model runs in the upscaler** on the captured frame: upload (BGRA bytes in
  an R8G8B8A8 texture, damage tracking of the upload unchanged) → MagpieFX chain (first effect
  swaps on read, last on write, exactly as the presenter's: its output is BGRA bytes again; an
  effect that does not fix its OUTPUT size outputs **its own input size**, `OutSize::Input`) → the
  model's input. With `model: none` the chain's output is the upscaler's output (both lists then run
  back to back: before-list in the upscaler, deband + after-list in the presenter).
- **Tiles:** with a non-empty chain before the model, tile processing falls back to full frame for
  the whole upscaler (neighbourhood effects would make tile seams); log it once at debug level and
  document it.
- **Sizes:** upscaler output = pre-chain output × model scale. The presenter already takes any
  source size (`set_source`; the scaler and `Layout::dst_rect` work from the texture's size, the
  geometry from `capture`), so it needs no change for it.
- **Changes:** another list (names, order, an effect added/removed) rebuilds the upscaler (like a
  model change: `recreate_upscaler`); same names with other parameters go live through
  `Upscaler::update_pre_params` (`set_params`, cheap). Effects that fail to compile/bind are dropped
  and reported like the presenter's (`Upscaler::take_disabled` → `Event::EffectCompile { session:
  true }`).
- **Shared plumbing:** the presenter's `MagpieContext`, `build_magpie_chain`, `bind_magpie` moved
  to `crates/lirtup-pipeline/src/magpie.rs` **in the contract commit** (R, real code, the presenter
  uses it; §2.5).
- **Warm-up:** `lirtup-core`'s `CompileEffect` compiles the effect for **every swap combination**
  (`(swap_input, swap_output)` ∈ ff, tf, ft, tt), so whatever segment and position the effect lands
  in, the session finds it in the cache. `Presenter::new` and `Upscaler::new` keep compiling their
  own chain blocking before the first frame.
- **Chain labels / stats:** `Upscaler::labels()` = the effects bound before the model (catalog
  names as written, like `Presenter::chain`) then the SRCNN passes; `Runner::chain` already puts the
  upscaler's labels first. The stats panel loses the `(Native)`/`(MagpieFX)` engine row
  (`PanelContext.engine` goes away).
- **A/B:** `ab_relevant_changed` already compares `old.scaling != new.scaling`, which now includes
  the before-list: keep it (SwapAb goes through `update_config`, which rebuilds the upscaler, so the
  blink shows it). **Split comparison limitation:** `begin_compare` renders A over the same
  upscaler output, so differences in the model and in the before-list are not shown; when A and B
  differ there (`model`, `double_upscale` or `magpie_effects_before_model`), the OSD says so when
  the comparison starts (§4).
- `lirtup_proto::Stage` names no native effect: unchanged. `BenchKind::Native` stays (deband, CAS,
  bloom, vignette, grain, LUT are still native passes). No protocol change in this round.
- **GUI:** the chain editor shows two segments «Antes del modelo» / «Después del modelo» (en:
  «Before the model» / «After the model»), effects move between them (a button on each card, and
  drag between segments if cheap); the engine selector and the native section disappear; costs:
  before-list at the capture resolution, after-list at the model output resolution (§7 gui). The
  stale help text that says MagpieFX compiles the first time with dxc/glslangValidator is fixed: with
  the seed that only applies to effects the person adds or edits.

### Cross-cutting rules

- Commits in English, clear subject lines. **No push, ever.** No version bump (stays 2.1.0).
  `CHANGELOG.md` and `docs/developer/fx-unify-integration.md` are written by the integrator from the
  workers' notes: workers do not touch them.
- OSD strings in the pipeline use the inline `tr("es", "en")` helper. GUI strings go in
  `i18n/{es,en}/*.ftl` (both); the `i18n_check` test (`both_languages_are_complete`) must keep
  passing: when you remove a key from the code, remove it from both `.ftl`; when you add one, add it
  to both. Spanish copy in the user's voice: voseo / neutral Rioplatense for instructions («creá»,
  «migrá», «agregá»).
- **Workers do not run cargo at all** (no build/check/test/fmt/clippy/run). Running
  dxc/glslang/git/python is fine.
- Shared files (`Cargo.toml` of the workspace and the crates, `lib.rs`/`mod.rs` declarations) are
  already edited by the contract; if you need more, ask in your notes. `*.ftl`: each file has one
  owner (§3).
- New logic must be unit-testable: pure helpers next to the code; GPU/X11 tests `#[ignore]`.

---

## 2. Shared interfaces (fixed; already in the code as stubs or real)

`R` = real in the contract commit, `S` = stub to be filled by the named worker.

### 2.1 `lirtup-config` model (R, owner config)

`crates/lirtup-config/src/model.rs`:

```rust
pub struct ModeSettings {
    ...
    pub magpie_effects: Vec<MagpieEffectSpec>,               // after the model (unchanged meaning)
    pub magpie_effects_before_model: Vec<MagpieEffectSpec>,  // NEW, default empty
    pub magpie_profile: bool,
    ...
}
pub const MODE_KEYS: &[&str] = &[..., "magpie_effects", "magpie_effects_before_model", "magpie_profile", ...];
```

Still there, marked `TODO(config): remove`: `EffectsEngine`, `ModeSettings::effects_engine` and the
24 native fields, their MODE_KEYS entries, defaults and `range!` lines. After the config worker's
branch, `MODE_KEYS` holds no legacy key (test in `legacy.rs` flips to `!contains`).

### 2.2 `lirtup_config::legacy` (R names, S translation, owner config)

`crates/lirtup-config/src/legacy.rs` (new, `pub mod legacy;` in `lib.rs`):

```rust
pub const LEGACY_KEYS: [&str; 25];          // R: "effects_engine" + the 24 native fields
pub const ANIME4K_EFFECT: &str = "Anime4K/Anime4K_Upscale_S";   // R
pub const LUMA_SHARPEN_EFFECT: &str = "Sharpen/LumaSharpen";    // R
pub const CRT_EFFECT: &str = "CRT/CRT_Easymode";                 // R
pub fn has_legacy_keys(map: &Mapping) -> bool;                    // R
pub fn translate_native(map: &mut Mapping, warnings: &mut Vec<ConfigWarning>) -> bool; // S: returns false
```

The names are the **catalog names** (what `ListEffects` returns and the GUI picker writes, so the
chain cards find their entry; `Catalog::find` also resolves them).

**The rule of `translate_native(map, warnings)`** (`map` = the merged file layers, or one mode file
for the migration):

1. If `!has_legacy_keys(map)`: return `false`, touch nothing.
2. Engine = `map["effects_engine"]`: absent or `null` → native; `"native"` → native; `"magpiefx"` →
   magpiefx; anything else → warning (`key: "effects_engine"`, «invalid value X, using native») and
   native.
3. Native: build the entries in the native order — `anime4k_enabled` → `ANIME4K_EFFECT`,
   `luma_sharpen_enabled` → `LUMA_SHARPEN_EFFECT`, `crt_enabled` → `CRT_EFFECT` — only those enabled
   (`true`), each with the parameters of the table below **whose value differs from the default**.
   An entry without parameters is the short form (`MagpieEffectSpec::Short(name)`); with parameters
   the long form (`MagpieEffectSpec::Full { name, params }`), exactly what the GUI's
   `ChainItem::from_parts` writes. Then: entries not empty → `map["magpie_effects"] = entries`
   (replacing the inactive list; position of the old key if any, else appended); entries empty →
   remove `magpie_effects` (the inactive list is dropped; empty is the default).
   Magpiefx: `magpie_effects` stays as it is.
4. Remove every `LEGACY_KEYS` key from `map`. `magpie_effects_before_model` is never touched.
5. Return `true`.

Values are read like the old tolerant deserialization + `validate()`: `null` = absent (default).
Booleans: only YAML booleans count (anything else → warning, `false`). Floats: any YAML number.
Integers (the old `u32` fields): a non-negative integer (`Value::as_u64`); a float or negative →
warning, default. Then the old range (inclusive): outside → warning «must be LO to HI, got V; using
the default D», default. Warning keys are the field names (`crt_mask_size`), as the old validators
gave. Parameters go out as `f64` (`maskDotWidth: 2.0` is fine: `params` is `BTreeMap<String, f64>`).

**The parameter table** (checked against `crates/lirtup-magpiefx/effects/{Sharpen/LumaSharpen,
CRT/CRT_Easymode,Anime4K/Anime4K_Upscale_S}.hlsl` and the native fields; every default and range
is identical on both sides, so no conversion is needed):

| Native field | Type | Default | Range | MagpieFX effect | `//!PARAMETER` | Kind | Default | MIN–MAX (STEP) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `anime4k_enabled` | bool | false | | `Anime4K/Anime4K_Upscale_S` | (none: no parameters; OUTPUT is 2× INPUT) | | | |
| `luma_sharpen_enabled` | bool | false | | `Sharpen/LumaSharpen` | (output = input size) | | | |
| `luma_sharpen_strength` | f32 | 0.65 | 0–3 | | `sharpStrength` | float | 0.65 | 0–3 (0.01) |
| `luma_sharpen_clamp` | f32 | 0.035 | 0–1 | | `sharpClamp` | float | 0.035 | 0–1 (0.005) |
| `luma_sharpen_pattern` | u32 | 1 | 0–3 | | `pattern` | int | 1 | 0–3 (1) |
| `luma_sharpen_offset_bias` | f32 | 1.0 | 0–6 | | `offsetBias` | float | 1 | 0–6 (0.01) |
| `crt_enabled` | bool | false | | `CRT/CRT_Easymode` | (no OUTPUT size: renders at the size it is given — the content size after the model, as the native port did) | | | |
| `crt_sharpness_h` | f32 | 0.5 | 0–1 | | `sharpnessH` | float | 0.5 | 0–1 (0.01) |
| `crt_sharpness_v` | f32 | 1.0 | 0–1 | | `sharpnessV` | float | 1 | 0–1 (0.01) |
| `crt_mask_strength` | f32 | 0.3 | 0–1 | | `maskStrength` | float | 0.3 | 0–1 (0.01) |
| `crt_mask_dot_width` | u32 | 1 | 1–100 | | `maskDotWidth` | int | 1 | 1–100 (1) |
| `crt_mask_dot_height` | u32 | 1 | 1–100 | | `maskDotHeight` | int | 1 | 1–100 (1) |
| `crt_mask_stagger` | u32 | 0 | 0–100 | | `maskStagger` | int | 0 | 0–100 (1) |
| `crt_mask_size` | u32 | 1 | 1–100 | | `maskSize` | int | 1 | 1–100 (1) |
| `crt_scanline_strength` | f32 | 1.0 | 0–1 | | `scanlineStrength` | float | 1 | 0–1 (**1**) |
| `crt_scanline_beam_width_min` | f32 | 1.5 | 0.5–5 | | `scanlineBeamWidthMin` | float | 1.5 | 0.5–5 (0.1) |
| `crt_scanline_beam_width_max` | f32 | 1.5 | 0.5–5 | | `scanlineBeamWidthMax` | float | 1.5 | 0.5–5 (0.1) |
| `crt_scanline_bright_min` | f32 | 0.35 | 0–1 | | `scanlineBrightMin` | float | 0.35 | 0–1 (0.01) |
| `crt_scanline_bright_max` | f32 | 0.65 | 0–1 | | `scanlineBrightMax` | float | 0.65 | 0–1 (0.01) |
| `crt_scanline_cutoff` | u32 | 400 | 1–1000 | | `scanlineCutoff` | int | 400 | 1–1000 (1) |
| `crt_gamma_input` | f32 | 2.0 | 0.1–5 | | `gammaInput` | float | 2 | 0.1–5 (0.01) |
| `crt_gamma_output` | f32 | 1.8 | 0.1–5 | | `gammaOutput` | float | 1.8 | 0.1–5 (0.01) |
| `crt_bright_boost` | f32 | 1.2 | 1–2 | | `brightBoost` | float | 1.2 | 1–2 (0.01) |
| `crt_dilation` | u32 | 1 | 0–1 | | `dilation` | int | 1 | 0–1 (1) |

Notes: Magpie declares `scanlineStrength` with STEP 1 although it is a float and the native field
took any 0–1 value: the translation keeps the value as written (e.g. `0.5`); MagpieFX passes it to
the shader unchanged (the runtime clamps to MIN/MAX only, it does not snap to STEP). The GUI slider
uses the catalog step, so the person can only move it to 0 or 1 afterwards: acceptable, documented
in `docs/user/configuration.md`. "Differs from the default" compares the read value as `f64` with
the default literal above (`0.65`, not `0.65f32 as f64`).

**Where the resolver applies it** (`resolve_with_mode`, config worker): right after the mode layer
at the `Named::Cli` position is merged and **before** `merge_layer(&mut merged, cli)`. Then the CLI
layer: drop its legacy keys (a clone of `cli` without them) with one warning per key («retired native
engine setting, ignored; use --magpie-effect»). Translation warnings join `Resolved.warnings`. In the
`shadowed` computation, treat `LEGACY_KEYS` as mode keys too (`MODE_KEYS.contains(k) ||
LEGACY_KEYS.contains(k)`), so an unmigrated profile with loose native fields over a mode is still
reported («lirtup migrate»). Unknown keys never warned and still do not; legacy keys are removed
before `Config::from_mapping`, so they do not reach `Config::extra` either.

### 2.3 `lirtup_config::migrate` (R signatures, S behaviour, owner migrate)

`crates/lirtup-config/src/migrate.rs`:

```rust
pub const MODE_BACKUP_SUFFIX: &str = ".bak";                              // R
pub struct MigrationPlan { ..., pub rewritten: Vec<(String, Mapping)> }   // R field (no derive change)
impl MigrationPlan { pub fn is_empty(&self) -> bool }                     // R: also `rewritten.is_empty()`
pub fn needs_migration(file: &ConfigFile, modes: &ModeStore) -> bool      // R signature; S: ignores `modes`
pub fn plan(file: &ConfigFile, modes: &ModeStore) -> MigrationPlan        // unchanged signature
pub fn apply(file: &mut ConfigFile, modes: &mut ModeStore, plan: &MigrationPlan) -> Result<PathBuf, ConfigError> // unchanged
```

The contract already updated the callers of `needs_migration` (`store.rs::offer_migration`, the
migrate tests) and the one `MigrationPlan { .. }` literal (`screens/migrate.rs` test). Semantics:

- `needs_migration`: loose mode fields **or legacy keys** (non-null) in the top level / a profile,
  **or** a mode file (`modes.raw[id]`) with a legacy key (`legacy::has_legacy_keys`, any value).
- `plan`: step A (layers → modes) as today but with `MODE_KEYS ∪ LEGACY_KEYS` as the mode fields
  (`mode_fields`, `loose_count`, `non_mode`, `ordered`, `rename_layer`); every combination is run
  through `translate_native` before it becomes a new mode's content, and the dedupe (`pick`)
  compares translated forms on both sides (`translate(strip_names(existing raw))`). Step B: every
  **existing** mode file with legacy keys goes to `rewritten` as `translate_native` of its raw
  mapping (name, description and unknown keys kept, key order kept: `magpie_effects` where it was,
  else where the first legacy key was). New modes of step A never carry legacy keys.
  `moved_fields` keeps counting the loose fields moved (legacy ones included).
- Why per-file translation is right in step B: after step A no layer of `config.yaml` has a mode
  or legacy field, so every resolution is one mode file (or none) + the command line, and translating
  the mode file alone equals translating the merged layers.
- `apply`: backup of `config.yaml` as today (always, `.1x.bak`, first free), then for each
  rewritten mode copy `modes/<id>.yaml` to `modes/<id>.yaml.bak` (first free: `.bak`, `.bak2`, …;
  never overwrite), then write the new modes, then the rewritten ones (`ModeStore::save_raw`), then
  `config.yaml`. Returns the `config.yaml` backup's path (the GUI's toast).
- Criterion test (extend `migrate_and_check`): resolve every `ProfileChoice` (none + each profile by
  name) × every mode (`None` + each mode id through `resolve_with_mode`) before (read-time
  translation) and after; same `Config` (with `mode` cleared); `needs_migration` false and `plan`
  empty afterwards; backups equal the old files.

### 2.4 `lirtup_effects::ports` (R skeleton, S content, owner effects)

`crates/lirtup-effects/Cargo.toml` (R): `[features] ports = []`; `[dev-dependencies] lirtup-effects
= { path = ".", features = ["ports"] }`. `crates/lirtup-effects/src/lib.rs` (R):
`#[cfg(feature = "ports")] pub mod ports;`. `crates/lirtup-magpiefx/Cargo.toml` (R):
`[dev-dependencies] lirtup-effects = { workspace = true, features = ["ports"] }`.
`crates/lirtup-effects/src/ports.rs` (S: docs only) and `crates/lirtup-magpiefx/tests/ports.rs` (S:
an empty ignored test).

Final API (suggested, the effects worker may reshape; only tests use it):

```rust
// lirtup_effects::ports (feature "ports")
pub struct CrtParams { ... }                 // moved from effects.rs, same fields/Default/pack
pub enum PortEffect { Anime4kS, LumaSharpen { strength: f32, clamp: f32, pattern: u32, offset_bias: f32 }, CrtEasymode(CrtParams) }
impl PortEffect {
    pub fn label(&self) -> &'static str;                 // "anime4k", "luma_sharpen", "crt_easymode"
    pub fn magpie_name(&self) -> &'static str;           // the catalog names of §2.2
    pub fn magpie_params(&self) -> ParamValues;          // every parameter, HLSL names of §2.2
    pub fn output_size(&self, input: (u32, u32), final_size: (u32, u32)) -> (u32, u32);
    pub fn create(&self, device: &Arc<Device>, input: &Texture, output_size: (u32, u32)) -> Result<Box<dyn Pass>>;
}
```

`NativeEffect` keeps `Cas`, `Deband`, `Bloom`, `Vignette`, `Grain`, `Lut` only; `from_mode` loses the
engine check and the three ports.

### 2.5 `lirtup-pipeline::magpie` (R, owner upscaler for follow-ups)

`crates/lirtup-pipeline/src/magpie.rs` (new, `mod magpie;` in `lib.rs`), moved from `presenter.rs`
with two additions; the presenter already uses it:

```rust
pub(crate) const SWAP_RB_LABEL: &str = "SwapRB";
pub(crate) struct MagpieContext { .. }
impl MagpieContext {
    pub(crate) fn new(device: &Device) -> MagpieContext;
    pub(crate) fn compile(&mut self, name: &str, swap_input: bool, swap_output: bool) -> Result<CompiledEffect>;
    pub(crate) fn swap_rb(&mut self) -> Result<CompiledEffect>;
    pub(crate) fn precompile(&mut self, specs: &[EffectSpec], disabled: &mut Vec<(String, String)>); // Presenter::new's loop
}
pub(crate) fn parse_chain(items: &[MagpieEffectSpec]) -> Vec<EffectSpec>;   // invalid entry → empty + error log
pub(crate) enum OutSize { Target((u32, u32)), Input }                     // Input: #[allow(dead_code)] TODO(upscaler)
pub(crate) fn build_magpie_chain(device, magpie: &mut Option<MagpieContext>, input: &Texture,
    specs: &[EffectSpec], out_size: OutSize, disabled: &mut Vec<(String, String)>) -> (Vec<Box<dyn Pass>>, Vec<EffectSpec>);
pub(crate) fn bind_magpie(device, ctx, input, specs, out_size: OutSize) -> Result<Vec<Box<dyn Pass>>, (usize, PipelineError)>;
pub(crate) fn set_chain_params(passes: &mut [Box<dyn Pass>], specs: &[EffectSpec]) -> Result<bool>; // skips SwapRB passes
```

The presenter calls `build_magpie_chain(.., OutSize::Target(content size), ..)` and
`set_chain_params(&mut self.pre[deband_count..], ..)`: behaviour identical to before. The upscaler
uses `OutSize::Input`. The presenter worker does **not** edit `magpie.rs`; if the presenter needs
something there, say so in the notes.

### 2.6 `Upscaler` (S, owner upscaler)

`crates/lirtup-pipeline/src/upscale.rs`, `impl Upscaler`:

```rust
pub fn new(device: &Arc<Device>, config: &Config, src_size: (u32, u32)) -> Result<Upscaler> // unchanged signature; builds the chain before the model
pub fn pre_chain(&self) -> &[EffectSpec]                                  // S: &[]
pub fn take_disabled(&mut self) -> Vec<(String, String)>                  // S: empty
pub fn update_pre_params(&mut self, config: &Config) -> Result<bool>      // S: Ok(false)
pub fn labels(&self) -> Vec<String>                                       // S: SRCNN only today
pub fn output(&self) -> &Texture  /  scale(&self) -> u32  /  model(&self) // unchanged meaning (scale = model scale)
```

### 2.7 `lirtup-ui` (R, owner gui; state files are contract-only)

`crates/lirtup-ui/src/state/mod.rs`: `Dialog::EffectPicker { mode: ModeId, before_model: bool }`.
`crates/lirtup-ui/src/app.rs`: passes `before_model` to `EffectPickerDialog`.
`crates/lirtup-ui/src/screens/modes/chain.rs`: `pub(crate) const PRE_CHAIN_KEY: &str =
"magpie_effects_before_model"`, `pub(crate) fn chain_key(before_model: bool) -> &'static str`; the
add button opens the picker with `before_model: false` (TODO(gui)).
`crates/lirtup-ui/src/screens/modes/picker.rs`: `EffectPickerDialog(mode: ModeId, before_model:
bool)` reads and writes `chain_key(before_model)`.
`crates/lirtup-ui/src/state/store.rs`: `offer_migration` calls `needs_migration(&file,
&store.modes.store)`. Nobody edits `state/**` or `app.rs` in this round: if the gui or the migrate
worker needs a change there, say so in the notes.

---

## 3. Ownership (no overlap; anything not listed belongs to nobody — ask first)

| Worker | Files |
| --- | --- |
| config | `crates/lirtup-config/src/model.rs`, `crates/lirtup-config/src/legacy.rs`, `crates/lirtup-config/src/resolve.rs`, `crates/lirtup-config/src/cli.rs`, `crates/lirtup-config/src/mode.rs` (only if needed), `crates/lirtup-config/src/util.rs` (only if needed), `crates/lirtup-config/tests/**` (`files_1x.rs`, `fixtures/`), `config-example.yaml`, `docs/user/configuration.md`, `docs/user/cli.md` |
| migrate | `crates/lirtup-config/src/migrate.rs`, `crates/lirtup-cli/src/main.rs`, `crates/lirtup-ui/src/screens/migrate.rs`, `crates/lirtup-ui/i18n/{es,en}/migrate.ftl`, `crates/lirtup-ui/src/i18n_check.rs` (only `migrate_messages_format`), `docs/user/migration.md` |
| effects | `crates/lirtup-effects/src/**` (`effects.rs`, `ports.rs`, `assets.rs`; `lib.rs` only if needed), `crates/lirtup-effects/shaders/effects/{anime4k,luma_sharpen,crt_easymode}/**` (only if needed), `crates/lirtup-effects/tests/**` (`parity.rs`, `fixtures/parity/cases.json`), `crates/lirtup-magpiefx/tests/ports.rs`, `scripts/rust-parity-refs.py` (only if needed) |
| presenter | `crates/lirtup-pipeline/src/presenter.rs`, `crates/lirtup-pipeline/src/bench.rs`, `crates/lirtup-core/src/engine.rs` |
| upscaler | `crates/lirtup-pipeline/src/upscale.rs`, `crates/lirtup-pipeline/src/pipeline.rs`, `crates/lirtup-pipeline/src/tiles.rs`, `crates/lirtup-pipeline/src/magpie.rs`, `crates/lirtup-pipeline/src/stats.rs` |
| gui | `crates/lirtup-ui/src/screens/modes/**` (`chain.rs`, `editor.rs`, `costs.rs`, `mod.rs`, `picker.rs`, `widgets.rs`), `crates/lirtup-ui/i18n/{es,en}/modes.ftl`, `docs/user/modes.md`, `docs/user/magpiefx.md`, `README.md` |
| contract only (nobody edits) | every `Cargo.toml`, `crates/lirtup-*/src/lib.rs` declarations, `crates/lirtup-ui/src/state/**`, `crates/lirtup-ui/src/app.rs`, `crates/lirtup-proto/**`, `packaging/**` |
| integrator only | `CHANGELOG.md`, `docs/developer/fx-unify-integration.md` |

Historical developer docs (`docs/developer/rust-r*.md`, `round-1.3-*.md`, …) and
`scripts/ts-to-ftl-1x.tsv` mention the native engine as history: leave them.

**Who removes what** (each worker in its own files; the tree compiles again only after all merges):

| Old name | Removed by | Its uses removed by |
| --- | --- | --- |
| `EffectsEngine`, `ModeSettings::effects_engine`, the 24 native fields, their MODE_KEYS/defaults/`range!` | config | effects (`effects.rs`), presenter (`presenter.rs`, `bench.rs`), upscaler (`pipeline.rs`), gui (`modes/**`), config (`resolve.rs` tests, `tests/files_1x.rs`) |
| CLI flags `--effects-engine`, `--enable-anime4k`, `--enable-luma-sharpen`, `--luma-sharpen-*`, `--enable-crt`, `--crt-*` | config | config (`cli.rs` tests), docs (config: `cli.md`; gui: `README.md`, `magpiefx.md`) |
| `NativeEffect::{Anime4kS, LumaSharpen, CrtEasymode}`, `effects::CrtParams` (moved to `ports`) | effects | presenter (`presenter.rs`, `bench.rs`), effects (`tests/parity.rs`) |
| `PanelContext.engine` | upscaler (`stats.rs`) | upscaler (`pipeline.rs`) |
| `modes-engine*`, `modes-anime4k*`, `modes-luma-sharpen*`, `modes-crt*`, `modes-pass-{anime4k,luma_sharpen,crt_easymode}` ftl keys | gui | gui |

## 4. Strings

### OSD (pipeline, `tr(es, en)`, upscaler worker)

| When | es | en |
| --- | --- | --- |
| Compare starts and A/B differ in the model or the chain before it | `Comparación: el modelo y los efectos antes del modelo no se comparan` | `Compare: the model and the effects before it are not compared` |

(Shown together with or right after the existing «Comparación: ← anterior · actual →».) The existing
«compilando efectos» note of the mode chip (`rebuilds_effects`) now covers both lists.

### GUI (Fluent, both languages)

`modes.ftl` (gui; placeholder-quality, reword freely, keep both languages and the variables):

| Key | es | en |
| --- | --- | --- |
| `modes-chain-before` | Antes del modelo | Before the model |
| `modes-chain-after` | Después del modelo | After the model |
| `modes-chain-before-help` | Sobre la captura, a la resolución del juego; el modelo recibe el resultado. | On the capture, at the game's resolution; the model gets the result. |
| `modes-chain-after-help` | Sobre la salida del modelo, antes del escalado final. | On the model's output, before the final scaling. |
| `modes-chain-before-empty` | Nada antes del modelo. | Nothing before the model. |
| `modes-effect-to-before` | Mover antes del modelo | Move before the model |
| `modes-effect-to-after` | Mover después del modelo | Move after the model |
| `modes-effects-help` | ejecuta los .hlsl de Magpie sin modificarlos · { $count } efectos en el catálogo ({ $verified } verificados; los demás llevan «Sin probar») y los tuyos en ~/.config/linux-rt-upscaler/effects · los incluidos vienen precompilados; los que agregues o edites se compilan la primera vez (dxc o glslangValidator). | runs Magpie's .hlsl unchanged · { $count } effects in the catalog ({ $verified } verified; the rest are marked «Untested») and yours in ~/.config/linux-rt-upscaler/effects · the bundled ones come precompiled; the ones you add or edit compile the first time (dxc or glslangValidator). |
| `modes-effects-help-unloaded` | (the same without the counts) | (the same without the counts) |
| `modes-legacy-banner` | Este modo tiene ajustes del motor de efectos nativo, que ya no existe. Al escalar se aplican como efectos MagpieFX; migrá la configuración para editarlos acá. | This mode has settings of the retired native effects engine. Scaling applies them as MagpieFX effects; migrate the configuration to edit them here. |
| `modes-legacy-migrate` | Migrar… | Migrate… |

Removed (gui, both languages): `modes-engine`, `modes-engine-native`, `modes-engine-magpiefx`,
`modes-engine-magpiefx-help`, `modes-engine-magpiefx-help-unloaded`, `modes-engine-native-help`,
`modes-anime4k*`, `modes-luma-sharpen*`, `modes-crt*`, `modes-pass-anime4k`,
`modes-pass-luma_sharpen`, `modes-pass-crt_easymode` — check with `rg` that no code names them.

`migrate.ftl` (migrate):

| Key | es | en |
| --- | --- | --- |
| `migrate-native-body` | `{ $count -> [one] Un modo usa *[other] { $count } modos usan }` el motor de efectos nativo, que ya no existe: sus efectos pasan a la cadena MagpieFX, con el mismo resultado. | `{ $count -> [one] One mode uses *[other] { $count } modes use }` the retired native effects engine: its effects move to the MagpieFX chain, with the same result. |
| `migrate-native-backup` | Antes se copia cada archivo de modo que cambia a { $file }. | Each mode file that changes is copied to { $file } first. |
| `migrate-native-title` | Actualizar los efectos | Update the effects |

(`migrate-native-title` when the plan only rewrites modes; with 1.x loose fields too keep
`migrate-title` and add the native paragraph. `$file` = `<id>.yaml.bak`.)

CLI output (`lirtup migrate`, English, migrate): one line per rewritten mode, e.g. `mode crt: native
effects → magpie_effects (1 effect)`; the «nothing to migrate» line says «no loose mode fields and no
native-engine settings».

## 5. Constants

| Name | Value | Where | Use |
| --- | --- | --- | --- |
| `LEGACY_KEYS` | 25 keys | `lirtup_config::legacy` (R) | translation, migration, `shadowed` |
| `ANIME4K_EFFECT` / `LUMA_SHARPEN_EFFECT` / `CRT_EFFECT` | `Anime4K/Anime4K_Upscale_S` / `Sharpen/LumaSharpen` / `CRT/CRT_Easymode` | `lirtup_config::legacy` (R) | translation, bench defaults, ports test |
| `MODE_BACKUP_SUFFIX` | `".bak"` | `lirtup_config::migrate` (R) | rewritten mode files |
| `PRE_CHAIN_KEY` | `"magpie_effects_before_model"` | `screens/modes/chain.rs` (R) | GUI |
| `SWAP_RB_LABEL` | `"SwapRB"` | `lirtup_pipeline::magpie` (R) | chain labels, `set_chain_params` |
| feature `ports` | off by default | `lirtup-effects` (R) | tests only |
| CLI flag | `--magpie-effect-before-model EFFECT` (repeatable) | `cli.rs` (config) | → `magpie_effects_before_model` |

## 6. Merge order (integrator)

1. `claude/fu-config`
2. `claude/fu-migrate`
3. `claude/fu-effects`
4. `claude/fu-presenter`
5. `claude/fu-upscaler`
6. `claude/fu-gui`

No file is shared, so the order only matters for reading the diffs; the build step runs after the
sixth merge (`cargo check --workspace --all-targets`, `cargo test --workspace`, clippy with
`-D warnings`, fmt) and catches what reading could not. Then the integrator writes `CHANGELOG.md`
`## Unreleased` from the workers' notes and `docs/developer/fx-unify-integration.md`. GPU tests to
run by hand on the integration build: `cargo test -p lirtup-magpiefx --test ports -- --ignored`,
`cargo test -p lirtup-effects --test parity -- --ignored`, and a session with an unmigrated 2.1 mode
(`crt_enabled: true`) before and after `lirtup migrate`.

## 7. Notes for workers

General:

- Work only in your worktree and your branch; commit there (English subject lines; several commits
  are fine). No push. No cargo of any kind: write code that compiles by reading carefully (imports,
  exhaustive matches, borrow scopes, `#[must_use]`, unused imports after removals). Remove the
  `TODO(<you>)` markers and the `#[allow(dead_code)]` of your stubs when done.
- Do not rename or remove any shared name of §2. If you need a shared change, say so in your final
  notes (the integrator handles it) instead of editing another worker's file.
- Grep before you finish: `rg -n 'effects_engine|EffectsEngine|anime4k_enabled|luma_sharpen|crt_'
  <your files>` must only show what is meant to stay (legacy tables, test inputs of the translation,
  parity cases).
- End with notes for the integrator: what changed for users (for the CHANGELOG), deviations from
  this contract, what is untested (GPU/X11), and manual test steps.

config:

- `model.rs`: remove the old names (§3 table); `mode_keys_match_mode_settings` keeps passing; flip
  `legacy::tests::legacy_keys_are_not_mode_keys` to `!contains`. Update the `ModeSettings` docs
  ("model, samplers, both MagpieFX chains, final touch-up").
- `legacy.rs`: implement `translate_native` exactly as §2.2. Keep the table as data (e.g.
  `const LUMA_PARAMS: [(&str /*field*/, &str /*param*/, Kind, f64 /*default*/, f64, f64); 4]` and
  `CRT_PARAMS: [...; 17]`) so the tests can iterate it. Unit tests: no legacy key → untouched and
  `false`; `anime4k_enabled: true` → `["Anime4K/Anime4K_Upscale_S"]`; LumaSharpen with one non-default
  param → long form with only that param; all three → native order; defaults omitted; bad
  types/out of range → warnings + default (and the param omitted); `effects_engine: magpiefx` keeps
  the list and drops the native keys; native with nothing enabled drops the inactive list; `null`
  values; `magpie_effects_before_model` untouched; the result deserializes into the same
  `Vec<MagpieEffectSpec>` the GUI writes.
- `resolve.rs`: apply it where §2.2 says; CLI legacy keys dropped with a warning; `shadowed` counts
  legacy keys. Tests: a mode with `crt_enabled: true` + a profile with loose `crt_sharpness_h: 0.7`
  resolve to one `CRT/CRT_Easymode` entry with `sharpnessH: 0.7`; `--magpie-effect` (cli mapping
  `magpie_effects`) wins over a translated list; `--set crt_enabled=true` warns and is ignored;
  `effects_engine: magpiefx` in the profile over a mode with `luma_sharpen_enabled: true` keeps the
  mode's `magpie_effects`; the existing tests that use `crt_enabled` move to the new key or check
  the translation.
- `cli.rs`: remove the flags (§1 A), add `--magpie-effect-before-model` (repeatable, `value_name =
  "EFFECT"`, heading "Pre-processing", doc: "Add a MagpieFX effect to the chain before the model
  (repeatable)") → `magpie_effects_before_model`; update `--magpie-effect`'s doc ("…after the
  model"). Tests: `flags_become_yaml_keys` covers both lists; the removed flags are rejected.
- `config-example.yaml`: drop the native section and `effects_engine`; document both lists
  (`magpie_effects_before_model: []` right after `magpie_effects`) with a commented example and a
  note on tiles (full frame with a chain before the model). `tests/files_1x.rs`
  (`reads_config_example`, `reads_1x_user_profiles`) and `migrate.rs`'s `config_example` test read
  it: keep them consistent (the 1.x fixture keeps its legacy keys: it is a 1.x file; assert through
  `resolve` that it translates).
- Pre-existing failure, not from this round: `tests/files_1x.rs::reads_the_installed_config_if_any`
  resolves the person's real `~/.config/linux-rt-upscaler/config.yaml` with an empty `ModeStore`,
  so it fails with `UnknownMode("modo-nuevo")` on this machine. Fix it while you are there: load
  `modes/` next to that config (`ModeStore::load`) instead of `ModeStore::default()`.
- `docs/user/configuration.md`: the two lists, the order of a frame (§1 B), the retired native
  engine and what an old file becomes (with the table, short), the `scanlineStrength` note.
  `docs/user/cli.md`: removed flags, new flag.

migrate:

- See §2.3. Keep `lirtup migrate --dry-run` printing the plan (new modes, top level, profiles, then
  `mode <id>: native effects → magpie_effects (N effects)` per rewritten mode).
- `screens/migrate.rs`: the dialog shows the native paragraph (count = `plan.rewritten.len()`) and
  its backup line when `rewritten` is not empty; title `migrate-native-title` when the plan has only
  `rewritten`. The `rows` helper ignores `rewritten` (or lists them as «actualizado»: your call). Unit
  test for whatever pure helper decides the title/paragraphs.
- `i18n_check.rs::migrate_messages_format`: extend it for `migrate-native-body` (`$count` 1 and 3)
  if you touch the variables of `migrate-body`.
- `migration_declined` stays one flag: a person who declined the 1.x migration is not asked again
  for the native one (read-time translation keeps everything working). Say so in
  `docs/user/migration.md` (plus a section «El motor de efectos nativo», with `lirtup migrate`).
- Tests in `migrate.rs`: a 2.1-style folder (config.yaml already migrated, a mode `crt.yaml` with
  `crt_enabled: true` + `crt_mask_size: 2`, a mode with `effects_engine: magpiefx` + a chain, a mode
  with `effects_engine: native` + an inactive chain) → only `rewritten`, criterion holds, `.bak`
  files equal the old ones; the 1.x fixtures (`REAL`, `config-1x-user.yaml`, `PROFILES`, the
  example) keep passing with legacy keys moving into the new modes translated; a second backup does
  not overwrite the first (`.bak2`).

effects:

- See §2.4. Move `CrtParams`, the three variants (as `PortEffect`) and their `NativePass` arms to
  `ports.rs` (a `PortPass`, or keep one private pass type shared through `pub(crate)` helpers in
  `util.rs`/`effects.rs`: your call; no behaviour change for the ports). `NativeEffect::from_mode`:
  deband, CAS, bloom, vignette, grain, LUT only, no engine; `set_param`, `label`, `output_size`,
  `in_place` lose the port arms. Module docs of `effects.rs` and `lib.rs` (the latter only its doc
  comment if it names the ports).
- `assets.rs`: the seven `.spv` of the ports (`effects/anime4k/anime4k_upscale_s_pass{1..4}.spv`,
  `effects/luma_sharpen/luma_sharpen.spv`, `effects/crt_easymode/crt_easymode.spv`) are embedded
  only with `#[cfg(feature = "ports")]` (e.g. a second static table searched by `native_spirv`, or
  `#[cfg]` on the array elements); `native_paths()` follows. Check no test of `assets.rs` counts the
  paths without the feature in mind (the crate's own tests run with the feature on).
- `tests/parity.rs`: the three cases use `ports::PortEffect`; `cases.json` and
  `scripts/rust-parity-refs.py` stay (they generate the 1.x references).
- `crates/lirtup-magpiefx/tests/ports.rs`: (1) a plain (non-GPU) test: for each port with
  non-default values, write the legacy YAML (field names), run `lirtup_config::legacy::
  translate_native`, and check name and params equal `magpie_name()` / the non-default subset of
  `magpie_params()`; (2) the `#[ignore]` GPU test: the same BGRA test image (reuse the helpers of
  `tests/parity.rs` there by copying them; there is no shared test crate) through the port and
  through MagpieFX (`compile_effect_with(.., CompileOptions { fp16: false, swap_input: true,
  swap_output: true })`, `MagpieEffect::new` with the same output size: CRT at e.g. 320×192 from
  160×96), compare bytes; print the max difference per effect; assert ≤ 2 for LumaSharpen and CRT,
  ≤ 4 for Anime4K (FP32 both). Use `SpirvCache::new(<temp dir>)` (it also reads the seed, so the
  bundled effects need no compiler when the seed is installed) and the builtin catalog
  (`Catalog::scan(None)`); a compile error fails the test with its message (the crate's
  `tests/parity.rs` asserts a compiler is present the same way).
- Tests in `effects.rs`: `from_mode_keeps_the_1x_order` without ports; ports tests (output sizes,
  `crt_constants_are_28_words`, `magpie_params` covers the 17 CRT names) in `ports.rs`.

presenter:

- `presenter.rs`: `EffectPlan.pre` = deband only; `magpie` = `parse_chain(&config.scaling.
  magpie_effects)` always (no engine); `set_view`'s CRT check goes (the MagpieFX check stays);
  `native_params` loses the LumaSharpen/Anime4K/CRT arms; `update_config`'s branches simplify
  (native `pre` is only deband). Module docs: the chain diagram of §1 B (the before-list lives in the
  upscaler). Tests: `plan_orders_the_effects_like_1x` (deband, the MagpieFX list, CAS, LUT, grain),
  `native_params_cover_live_changes_only`, `plan_a_differs_only_where_the_other_config_does`
  without `EffectsEngine`. `begin_compare` stays as is (A over the same upscaler output).
- `bench.rs`: `NATIVE_EFFECTS` goes; without a mode the default cases are the three effects as
  **MagpieFX** cases by catalog name, in the 1.x order (`Sharpen/LumaSharpen`,
  `Anime4K/Anime4K_Upscale_S`, `CRT/CRT_Easymode`; use the `legacy` constants); with a mode: its
  native passes (deband, CAS, bloom, vignette, grain, LUT), then MagpieFX cases for
  `magpie_effects_before_model` then `magpie_effects` (deduplicated, then `request.effects`).
  `all_native` without the ports. The bench measures every MagpieFX case at the request's `src`
  input as today (the GUI scales the costs, §7 gui). Tests updated (`cases_follow_the_1x_order`,
  `native_names_map_to_passes`).
- `crates/lirtup-core/src/engine.rs`: `compile_effect` compiles the four swap combinations
  (`compile_effect_with` with `CompileOptions { fp16, swap_input, swap_output }` for ff, tf, ft, tt;
  stop at the first error and report it in the one `EffectCompile { done: true, error }` event).
  Update the module docs' command table. A pure helper for the four combinations is welcome.

upscaler:

- `upscale.rs` (see §1 B, §2.6): in `new`, `parse_chain(&config.scaling.magpie_effects_before_model)`;
  if not empty: `MagpieContext::new` + `precompile` (blocking) + `build_magpie_chain(device, &mut
  ctx, &input, &specs, OutSize::Input, &mut disabled)`; the model's input (and the tile residual) is
  the chain's output; tiles only when the chain is empty; `output()` with `model: none` is the
  chain's output. `process` records the chain after the upload and before the SRCNN, every frame.
  Fill `pre_chain`, `take_disabled`, `labels` (bound effect names, then SRCNN), `update_pre_params`
  (same names as the bound ones → `set_chain_params` on the chain's passes with the new specs → true;
  else false). Remove the `#[allow(dead_code)]` of `OutSize::Input`. Module docs: the chain before
  the model, the tile fallback.
- `pipeline.rs`: `update_config`: `needs_upscaler` also when the before-list's names/order change
  (or `update_pre_params` returns false); a param-only change goes live through
  `update_pre_params` (`stale = true`). `rebuilds_effects` (pure, tested): another non-empty chain in
  either list, no engine. Session-disabled effects: report the upscaler's `take_disabled` with the
  presenter's. `begin_compare`: the OSD of §4 when the pure helper `compare_misses(a, b)` (model,
  `double_upscale` or `magpie_effects_before_model` differ) says so; test it. `PanelContext.engine`
  goes (and the `(Native)/(MagpieFX)` row in `stats.rs`). `ab_relevant_changed` stays (it already
  sees the new key; add a test case for it). Tests that use `EffectsEngine` move to the new rules.
- `magpie.rs`: yours for follow-ups (e.g. a shared helper for "same names as bound"); keep the
  presenter's call sites working (§2.5), or tell the integrator exactly what changed.
- `tiles.rs`: probably no change (the fallback is decided in `upscale.rs`).

gui:

- `editor.rs`: remove the engine selector, `NATIVE`, the native `ToggleGroup`s and their test; the
  effects section keeps the help paragraph (`modes-effects-help`, fixed text), the chain editor, the
  total and «Medir en mi GPU». When the mode's raw mapping (`store.modes.store.raw[id]`) has legacy
  keys (`lirtup_config::legacy::has_legacy_keys`): show `modes-legacy-banner` with a button
  `modes-legacy-migrate` that dispatches `Action::OpenDialog(Dialog::Migrate(lirtup_config::migrate::
  plan(&file, &modes)))`, and show the chain **read-only** from a translated copy
  (`translate_native` on a clone of the raw) — editing a mode that still has legacy keys would be
  overridden by the read-time translation, so it is offered only after migrating.
- `chain.rs`: two segments in one `EffectChain`: «Antes del modelo» (`PRE_CHAIN_KEY`) first, then a
  small row for the model (its name, e.g. «Modelo: fast»), then «Después del modelo» (`CHAIN_KEY`).
  Each segment: its list, its empty state, its «Agregar efecto» (`Dialog::EffectPicker { mode,
  before_model }`). Each card gets an `IconButton` `swap_vert` («Mover antes/después del modelo»)
  that writes both lists (two `SetField`, the removal first); drag and drop between segments if it
  falls out of the current drag state cheaply. A pure helper `move_between(pre, post, from_pre,
  index) -> (Vec<ChainItem>, Vec<ChainItem>)` (appends at the end of the other list) with a test.
  Session-disabled badges match by name, so an effect in both lists shows it on both (documented).
- `costs.rs`: `native_passes` = deband, CAS, bloom, vignette, grain, LUT (no ports, no engine);
  `Costs::mode(&ModeSettings, pre: &[String], post: &[String]) -> Option<f32>`: native passes as
  today; `pre` effects at the bench's measured cost; `post` effects × `s²` where `s` = model scale (1
  for `none`, 2, 4 with `double_upscale`) — the bench measures at its input size, which stands for
  the capture; the after-list runs on `s×` that. Document the approximation (an effect that changes
  the size inside a list, like Anime4K ×2, is not followed). Tests: the old test without engine,
  a pre/post case.
- `mod.rs` (`summary`): the effects of both lists (pre first), no engine; test updated.
- `modes.ftl` (both): §4. Remove the dead keys.
- Note (pre-existing, optional): the chain cards match catalog entries by exact name, so a 1.x file
  with stems (`LumaSharpen`) shows «not in the catalog»; matching by stem as a fallback
  (`name.rsplit('/').next()`) would fix it. The translation writes catalog names, so it is not
  affected.
- Docs: `docs/user/modes.md` (the two segments, moving effects, costs), `docs/user/magpiefx.md`
  (no engines any more: drop the comparison table, the three effects run through MagpieFX; the
  before/after lists; the seed note), `README.md` (the engines paragraph and table become one
  MagpieFX paragraph; `--effects-engine` goes).
