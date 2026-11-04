# fx-unify — integration

Integration of the round described in `fx-unify-contract.md`, on branch
`fx-unify/integration` (base: the contract commit `5ea1a812`, on top of
`main` at `f8427714`, v2.1.0). **Nothing was compiled or run**: neither the
workers nor the integrator ran cargo. The first step after this is
`cargo fmt --all`, `cargo check --workspace --all-targets`,
`cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test --workspace`, then the `#[ignore]` GPU tests below on a machine
with Vulkan and X11.

## What merged

Merged with `git merge --no-ff`, in the contract's order (§6):

| # | Branch | Commits | Scope |
| --- | --- | --- | --- |
| 1 | `claude/fu-config` | `48b8a169`, `606436d6` | `EffectsEngine`, `effects_engine` and the 24 native fields out of `ModeSettings`/`MODE_KEYS`/defaults/validators; `legacy::translate_native` (table as data); resolver applies it to the file layers before the CLI layer, drops CLI legacy keys with a warning, counts legacy keys in `shadowed`; `Mode::from_mapping` translates too; `--magpie-effect-before-model`, removed flags; `config-example.yaml`, `tests/files_1x.rs`, `docs/user/{configuration,cli}.md` |
| 2 | `claude/fu-migrate` | `9f5222c5`, `7c538415`, `0680b820`, `b67aa914` | `needs_migration`/`plan`/`apply` with `MODE_KEYS ∪ LEGACY_KEYS`, step B `rewritten`, `.bak`/`.bak2` mode backups; `lirtup migrate --dry-run` lines; the dialog's native paragraph and `migrate-native-*` keys; `docs/user/migration.md` |
| 3 | `claude/fu-effects` | `54ee7755`, `5819e23f` | `NativeEffect` = deband, CAS, bloom, vignette, grain, LUT; `lirtup_effects::ports` (`CrtParams`, `PortEffect`, private `PortPass`) behind feature `ports`; port `.spv` in a second table only with the feature; `tests/parity.rs` on `PortEffect`; `lirtup-magpiefx/tests/ports.rs` (plain translation test + ignored GPU comparison) |
| 4 | `claude/fu-presenter` | `52c2674e`, `7ea82fcb`, `60fd9fb8` | `EffectPlan.pre` = deband, MagpieFX chain always; `update_pre_params` (deband + chain values live); bench `DEFAULT_EFFECTS` as MagpieFX cases, `mode_cases` with both lists; `CompileEffect` compiles the four swap combinations (`warm_up_options`) |
| 5 | `claude/fu-upscaler` | `3390ede1`, `a162ef1e` | the chain before the model in `Upscaler` (`OutSize::Input`, tile fallback, `pre_chain`/`take_disabled`/`update_pre_params`/`labels`); `pipeline.rs` (`pre_chain_changed`, live params, `rebuilds_effects` on both lists, `compare_misses`/`compare_osd`, both chains' disabled effects); `PanelContext.engine` gone; `magpie::{same_names, chain_names}` |
| 6 | `claude/fu-gui` | `5ba3731d`, `21702b02` | two-segment chain editor, model row, ⇅ button and drag between segments, picker target, legacy banner + «Migrar…» + read-only translated chain, `Costs::mode(&ModeSettings, pre, post)`, summary of both lists, stem fallback in the catalog lookup, `modes.ftl`, `docs/user/{modes,magpiefx}.md`, `README.md` |

## Conflicts

None. No file was touched by two branches; all six merges were clean.

## Deviations accepted

From the workers' notes, all kept:

- **config:** `Mode::from_mapping` (in `mode.rs`, "only if needed") keeps the
  legacy keys in `Mode.extra` and runs `translate_native` so `Mode.settings`
  holds the translated chain (warnings prefixed `modes.<id>.`); the raw
  mapping is unchanged. Native field warnings are given for every field
  whatever the engine. A new `magpie_effects` goes in the old key's slot,
  else the first legacy key's slot (the §2.3 rule, also in the resolver).
  The parameter tables are private data (`#[rustfmt::skip]`).
- **migrate:** the profile × mode criterion in `migrate_and_check` runs only
  when `moved_fields == 0` (documented there); step A runs only when there
  are loose fields; translation warnings are dropped in `plan` (the resolver
  reports them); `config.yaml` is saved (and backed up) even on a native-only
  plan; the `migration.md` section title is in English, like the rest of the
  file.
- **effects:** six port `.spv` (four Anime4K passes, LumaSharpen, CRT), not
  seven as the contract counted. `NativeEffect::output_size` and `create`
  keep their signatures but every native effect keeps its input size.
  `PortEffect` also has `set_param` (MagpieFX parameter names),
  `with_defaults`, `LUMA_SHARPEN_DEFAULT`, `CRT_PARAM_NAMES`,
  `LUMA_SHARPEN_PARAM_NAMES`. `effects::spirv` is `pub(crate)` for `ports.rs`.
- **presenter:** `native_params` returns `ParamValues` (every native effect
  takes its values live; only another LUT preset rebuilds);
  `update_magpie_params` became `update_pre_params` (deband strength and
  chain values together).
- **upscaler:** the compare limitation is appended to the one compare OSD
  message (`compare_osd(misses)`), since the OSD shows one message at a time.
  `Upscaler::labels()` uses the bound passes' labels (file stem, like
  `Presenter::chain` once built), without `SwapRB`. Effects that fail to
  compile are not bound again (`without_disabled`), so each failure is
  reported once. `rebuilds_effects` still compares whole lists (values
  included): a value-only change shows «preparando efectos…» although it
  goes live.
- **gui:** extra keys `modes-chain-model`, `modes-chain-no-model`; reworded
  `modes-chain-empty`, `bench-what-native` and the legacy banner;
  `modes-pass-{deband,cas,bloom,vignette,grain,lut}` kept (the bench
  screen's `format!("modes-pass-{}")` lookup). `move_between` is generic,
  plus `move_between_at` for drops; pure `apply_edit`. The read-only chain is
  built from `modes.raw[id]` alone.

## Reconciliation (cross-branch, by reading)

Fix commit `e64b9843`:

- **`same_names` deduplicated.** `presenter.rs` had a private
  `same_names` with the same body as the upscaler's
  `crate::magpie::same_names`; the presenter now imports the shared one and
  its duplicate test (`same_names_ignores_the_parameters`, also in
  `magpie.rs`) goes.
- **`state/store.rs::offer_migration`** doc comment now says it also offers
  the rewrite of mode files with native-engine settings, one
  `migration_declined` flag for both.

Checked and left as is:

- **Leftover old names.** `rg` over the whole workspace for
  `EffectsEngine|effects_engine|anime4k_enabled|luma_sharpen_*|crt_*|Anime4kS|CrtEasymode|CrtParams|PanelContext.engine`
  and the removed flags: what remains is legacy by design — `legacy.rs`,
  `migrate.rs`, `mode.rs` tests, `resolve.rs` tests, `tests/files_1x.rs` and
  the 1.x fixture, `cli.rs`'s "removed flags are rejected" test,
  `lirtup-effects/src/{ports,assets}.rs`, `tests/parity.rs`, `cases.json`,
  `lirtup-magpiefx/tests/ports.rs`, `scripts/rust-parity-refs.py`, the
  GUI's `chain.rs` translation test, `store.rs`'s
  `!keys.contains(&"crt_enabled")`, and the user docs that describe the
  retired engine. Every other `engine` hit is the frame generation engine
  or `lirtup_core::Engine`. Nothing in `state/**`, `app.rs`,
  `lirtup-proto`, `lirtup-cli`, `packaging/` or `scripts/` needed a change.
- **Contract stubs.** No `TODO(<worker>)` left in code; no
  `#[allow(dead_code)]` of a contract stub left (`OutSize::Input` lost its
  one; the two remaining, `srcnn.rs:85` and `gpu/device.rs:24`, predate the
  round).
- **Bench keys vs GUI costs.** `bench.rs` files native passes under
  `NativeEffect::label()` (`deband`, `cas`, `bloom`, `vignette`, `grain`,
  `lut`) and MagpieFX cases under the spec's name as written;
  `costs.rs::native_passes` uses the same six labels and
  `chain::chain_names` gives the names as written, so the lookups agree.
  Old `bench.json` entries `native/anime4k`, `native/luma_sharpen`,
  `native/crt_easymode` are read into the map but never looked up: harmless.
  The bench screen's `modes-pass-*` lookup falls back to the raw name.
- **Signatures across branches.** `legacy::translate_native(&mut Mapping,
  &mut Vec<ConfigWarning>) -> bool` (not `#[must_use]`; callers in
  `resolve.rs`, `mode.rs`, `migrate.rs`, GUI `chain.rs`, magpiefx
  `tests/ports.rs`); `migrate::needs_migration/plan(&ConfigFile,
  &ModeStore)` (`store.rs`, `editor.rs`, `lirtup-cli`); `Costs::mode(
  &ModeSettings, &[String], &[String])` (`editor.rs`, `mod.rs`, tests);
  `Upscaler::{new, output, labels, take_disabled, update_pre_params}` in
  `pipeline.rs`; `NativeEffect::{from_mode, output_size, create}` in
  `presenter.rs` and `bench.rs`; `PortEffect`/`CrtParams` only in the two
  test files; `NativeEffect` matches in `presenter.rs` (`EffectPlan`,
  `native_params`) cover the six variants; `Dialog::EffectPicker { mode,
  before_model }` in `chain.rs` and `app.rs`.
- **Cargo features.** `lirtup-effects`: `[features] ports = []`, self
  dev-dependency with `ports`, `#[cfg(feature = "ports")] pub mod ports`;
  `lirtup-magpiefx` dev-dependency on `lirtup-effects` with `ports`
  (`lirtup-config` is a normal dependency there, used by `tests/ports.rs`).
- **i18n.** `es` and `en` have the same key set (all `.ftl` files); every
  `t!("…")` / `tr("…")` key in `lirtup-ui/src` exists in both; none of the
  removed `modes-engine*`, `modes-anime4k*`, `modes-luma-sharpen*`,
  `modes-crt*`, `modes-pass-{anime4k,luma_sharpen,crt_easymode}` keys is
  left in an `.ftl` or named by code.

## Where the build may complain

Read, not compiled; places to look first:

- `crates/lirtup-ui/src/screens/modes/editor.rs:19` imports `CostBadge`
  without using it. The import predates the round (same line at
  `f8427714`), so it passed before; if clippy now flags it, drop it.
- `crates/lirtup-pipeline/src/presenter.rs:88`: the `crate::magpie` import
  line is 107 columns (max 110); `cargo fmt` may still rewrap it.
- `cargo fmt` will likely touch several worker files (the config and gui
  workers said so).
- `crates/lirtup-effects/src/effects.rs`: `output_size`/`create` keep the
  now-unused parameters with `let _ = …;` — fine for rustc and clippy, but
  check `-D warnings`.
- `crates/lirtup-pipeline/src/upscale.rs:142`:
  `pre.last().map(|p| p.output()).unwrap_or(&input).clone()` borrows
  through a closure; should infer, but it is the one non-trivial borrow of
  the new code.
- The self dev-dependency: `cargo check --workspace --all-targets` must
  build `lirtup-effects` both with and without `ports` (the app without,
  the tests with); `assets.rs`'s `tables_are_complete` asserts on
  `cfg!(feature = "ports")`.

## What is untested

Everything that needs a GPU, X11 or a running session: the chain before the
model in the upscaler (colours, sizes, tile fallback, live parameters,
rebuild on another list), the presenter's chain after the model, the four
warm-up compiles, the bench cases, the ports against MagpieFX, the GUI (two
segments, drag and drop, costs, legacy banner) and the migration dialog.

## Manual tests (integration build, GPU + X11)

1. GPU tests:

   ```sh
   cargo test -p lirtup-magpiefx --test ports -- --ignored --nocapture
   # prints the max difference per effect; ≤ 4 for Anime4K, ≤ 2 for the others.
   # LIRTUP_PORTS_CASE=<name> runs one case.
   cargo test -p lirtup-effects --test parity -- --ignored
   # references: PYTHONPATH=src python scripts/rust-parity-refs.py --only effects
   ```

2. **Unmigrated 2.1 mode.** In a scratch config dir: `config.yaml` with
   `mode: crt`, `modes/crt.yaml` with `name: CRT`, `crt_enabled: true`,
   `crt_mask_size: 2`. `lirtup run --mode crt` shows the CRT effect (stats
   panel: CRT_Easymode, no engine row). `lirtup migrate --dry-run` prints
   `mode crt: native effects → magpie_effects (1 effect)`; `lirtup migrate`
   writes `magpie_effects: [{name: CRT/CRT_Easymode, params: {maskSize:
   2.0}}]`, `modes/crt.yaml.bak` equals the old file; the session looks the
   same; a second `lirtup migrate` says nothing to migrate. Migrating again
   after restoring the file keeps the first backup (`.bak2`).
3. **CLI.** `--set crt_enabled=true` warns «retired native engine setting,
   ignored; use --magpie-effect»; `--enable-crt` is a clap error;
   `--magpie-effect-before-model Sharpen/LumaSharpen` sharpens before the
   model.
4. **Chain before the model.** Mode with `magpie_effects_before_model:
   [Sharpen/LumaSharpen]` and `model: fast`: sharpened, colours right (no
   red/blue swap), stats list LumaSharpen before the SRCNN passes. With
   `use_tile_processing: true` and `RUST_LOG=debug`: «tile processing off,
   full frame». Moving `sharpStrength` in the GUI goes live (no
   «preparando efectos…» stall beyond the chip note). `Anime4K/
   Anime4K_Upscale_S` before the model with `model: none` doubles the
   source. A broken name is reported disabled once.
5. **Compare.** Change the model, then the bar's *Compare*: the OSD adds «el
   modelo y los efectos antes del modelo no se comparan».
6. **GUI.** Empty mode: both empty states and the model row; «Agregar
   efecto…» in «Antes del modelo» writes `magpie_effects_before_model`; ⇅
   and drag move effects between segments; costs ×1/×4/×16 after the model
   with `none`/2×/`double_upscale`; the 2.1 `crt` mode shows the banner, a
   read-only chain and «Migrar…» opens the migration dialog («Actualizar
   los efectos»); `magpie_effects: [LumaSharpen]` (a 1.x stem) is found in
   the catalog; «Medir en mi GPU» without a mode measures the three effects
   as MagpieFX; the English UI shows the English strings.
