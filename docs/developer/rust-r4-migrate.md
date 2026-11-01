# Ronda 4, frente M: migración y traducciones

Rama **`claude/rust-r4-migrate-b8f680`**, que sale de `rust/r4-skeleton` (`27b7d08b`). Fecha:
2026-11-01. Sin push y sin merge.

Compilé con `CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r4-m`.
- `cargo test -p lirtup-config`: **60 pasan** (55 + 5 de `files_1x`), 11 nuevos de `migrate`.
- `cargo test -p lirtup-ui`: **98 pasan**, 1 ignorado (el de antes), 10 nuevos.
- `cargo clippy -p lirtup-config -p lirtup-ui -p lirtup-cli --all-targets`: sin avisos en lo mío
  (quedan los `todo!()` de T e Y).
- `scripts/check-ftl-keys.py`: 580 claves en cada idioma, ninguna falta.

## Commits

| Commit | Qué |
| --- | --- |
| `53e89f84` | `lirtup-config/src/migrate.rs`: `needs_migration`, `plan`, `apply`, `migrated`, `legacy_gui`, `apply_legacy_gui` y sus tests |
| `8e3c3e12` | `scripts/ts-to-ftl.py` y `scripts/ts-to-ftl-1x.tsv` (las 526 cadenas de la 1.x) |
| `37e03cd0` | `screens/migrate.rs` (`MigrateDialog`), los brazos de `update`, el ofrecimiento después de `Loaded` y `i18n/{es,en}/migrate.ftl` |
| `30f84933` | `i18n_check.rs`: el test de traducciones completas |

## La migración (`lirtup_config::migrate`)

Las reglas están en el doc del módulo. En corto:

- **Cada combinación distinta de campos de modo es un modo.** La combinación es lo que aplica el
  resolutor, así que depende de dónde se ubica el modo de cada capa:
  - el nivel superior: su modo (si nombraba uno) y encima sus campos sueltos → «Predeterminado»
    (`predeterminado`, o `predeterminado-2` si ya existe);
  - un perfil que no nombra modo: la combinación del nivel superior y encima sus campos
    sueltos. Si da lo mismo que el nivel superior, el perfil queda sin `mode:` y hereda;
  - un perfil que nombra un modo: los campos sueltos del nivel superior, el modo y sus campos
    sueltos. Por eso un perfil con `mode:` y sin campos sueltos también cambia si el nivel
    superior tenía campos sueltos que el modo no tapaba.
- Los perfiles con la misma combinación **comparten** un modo («Modo de <el primero>»). Una
  combinación igual a un modo que ya existe **nombra ese modo** en vez de copiarlo.
- Los campos sueltos salen de cada capa, y `mode:` queda donde estaba el primero. Un `null`
  («heredar») no cuenta como campo para mover. Las claves que no son de modo, el `match`, el
  ícono y los atajos no se tocan.
- **Respaldo**: `config.yaml` se copia a `config.yaml.1x.bak` antes de escribir nada. Si ya
  existe, va a `.1x.bak2`, `.1x.bak3`… (el primero es la 1.x de verdad y nunca se pisa). Después
  se escriben los modos y al final `config.yaml` con `ConfigFile::save`, que además rota sus
  `.bak1…5` como siempre. Si algo falla antes del final, `config.yaml` queda como estaba.
- **Claves que no son de modo dentro de un archivo de modo** (la 2.0 no las escribe, pero se
  conservan): se mueven con el modo cuando el modo nuevo ocupa su lugar (el del nivel superior o
  el del perfil). Si un perfil las heredaba del nivel superior y ahora nombra un modo propio,
  recibe las que el nivel superior no fija. Lo cubre un test.
- Un `mode:` que nombra un modo inexistente (antes daba error al resolver) se trata como modo
  vacío: después de migrar, el error desaparece.

**El criterio de 5.8 está en los tests**: `migrate_and_check` resuelve el nivel superior y cada
perfil por nombre antes y después, y compara los `Config` completos (salvo `mode`, que es lo que
cambia). También comprueba que el respaldo es el archivo original y que una segunda pasada no
encuentra nada. Corre sobre:
- la configuración real de la persona (copiada en el test);
- `tests/fixtures/config-1x-user.yaml` (perfiles de la 1.3);
- `config-example.yaml`, si existe;
- casos armados: perfiles que heredan, que comparten modo, con `null`, con `mode:` y sin campos,
  con `mode:` y con campos, modos que se reusan e ids ocupados.

`real_config_from_env` migra una copia de la carpeta que diga `LIRTUP_MIGRATE_REAL` (sin esa
variable no hace nada):

```bash
LIRTUP_MIGRATE_REAL=$HOME/.config/linux-rt-upscaler cargo test -p lirtup-config real_config_from_env -- --nocapture
```

### Los ajustes de la GUI de la 1.x (filas 29 y 30)

`apply` también llama a `apply_legacy_gui(carpeta de config.yaml)`, que lleva a
`gui-config.yaml` lo que la 2.0 ya tiene dónde guardar. Solo escribe si algo cambia, y un error
ahí solo deja una línea en el log:

- **QSettings** (`<XDG_CONFIG_HOME>/linux-rt-upscaler.conf`), sección `[tray]`, solo las claves
  que están escritas: `enabled` → `show_tray_icon`; `close_to_tray`, `minimize_to_tray` y
  `start_hidden` con el mismo nombre; `keep_running_on_exit` → `keep_running_after_exit`.
- **Paleta**: `palette_preset` con nombre → `seed_color`, solo si la semilla sigue siendo la
  predeterminada (una elegida en la 2.0 no se pisa). `Auto`, `Light` y `Dark` dejan la semilla
  predeterminada. La tabla `PALETTE_SEEDS` usa los 22 nombres de la 1.x (Gruvbox y Solarized,
  claro y oscuro, comparten semilla). Un test de la GUI comprueba que cada semilla está en
  `theme::PALETTES`. Los colores sueltos de `palette:` quedan en `extra`: distinto a propósito.
- `palette`, `palette_preset` y las demás claves de la 1.x se conservan en `gui-config.yaml`.

**No se migran** la geometría de la ventana (`mainwindow/geometry`, un blob de
`QWidget::saveGeometry`) ni la posición de la superposición (`overlay/windowed_position`). La
2.0 todavía no guarda esas cosas (filas 40 y 11, de Y y T). Si las agregan, el lugar es
`legacy_gui`.

## La GUI

- **Ofrecimiento**: en el primer `Loaded` (la ruta de `config.yaml` del store estaba vacía), si
  `needs_migration`, `!gui.migration_declined` y no hay otro diálogo abierto, se abre
  `Dialog::Migrate(plan)`. Una recarga posterior no vuelve a preguntar. Esc o el fondo cierran
  el diálogo (`CloseDialog`) y la próxima vez que se abra la GUI vuelve a ofrecerse.
- `Migrate` → `Effect::Migrate(plan)` (lo corre `app.rs`, ya escrito) y cierra el diálogo. Sin el
  diálogo abierto no hace nada.
- `SkipMigration` («Ahora no») → `gui.migration_declined = true` y `Effect::WriteGui`.
- `Migrated(Ok)` → aviso `migrate-done` con la ruta del respaldo, y `undo = None` (un «Deshacer»
  de antes traería de vuelta los campos sueltos). `Migrated(Err)` → aviso `migrate-failed`.
- `MigrateDialog`: cuántos ajustes se mueven y a cuántos modos nuevos; un renglón por modo, con
  «nuevo» si lo crea y quién lo nombra (Predeterminado y los perfiles); los perfiles que quedan
  heredando; la copia en `config.yaml.1x.bak`, y que la bandeja y la paleta pasan a Ajustes.
  Usa clases y un ícono (`tune`) que ya están en `tailwind.css` y en el subconjunto de íconos:
  no hace falta regenerar nada.
- En los tests de `store.rs`, `store_with` carga con `migration_declined: true`. `FILE_1X` tiene
  campos sueltos y, si no, el diálogo se abriría en todos los tests de C. Los del ofrecimiento
  están al final.

## Traducciones

- **Inglés completo**: las 570 claves de la ronda 3 ya estaban en los dos idiomas. Revisé las 35
  que tienen el mismo texto en los dos: son nombres propios o palabras iguales (`MagpieFX`,
  `CAS`, `Global`, `Auto`, `Zoom`…), y donde la 1.x tenía la cadena, la 1.x también las dejaba
  iguales. Agregué las 10 de `migrate.ftl` en los dos idiomas: 580 en total.
- **`scripts/ts-to-ftl.py`**:
  - `extract` lee `upscale_gui_es.ts` y escribe `scripts/ts-to-ftl-1x.tsv` con contexto, inglés,
    español y comentario (`{0}` → `{ $arg0 }`): las 526 cadenas. **El TSV queda en el
    repositorio** para cuando se retiren los `.ts`;
  - `check [--write]` compara los `.ftl` con el TSV. A una clave que falte en inglés le propone
    el inglés de la cadena de la 1.x con el mismo español (`--write` la agrega), lista las que
    tienen el mismo texto en los dos idiomas con lo que decía la 1.x y cuenta cuántas cadenas
    de la 2.0 vienen de la 1.x (hoy, 137).

  Las claves de la 2.0 no son las de la 1.x (otro diseño, otras pantallas), así que no se copian
  clave por clave: se emparejan por el texto.
- **`i18n_check.rs`** (`cargo test -p lirtup-ui i18n_check`) hace lo mismo que
  `check-ftl-keys.py`: cada clave que nombra el código existe en `es` y en `en`, los dos idiomas
  definen las mismas y ninguna está dos veces. Además comprueba:
  - que el cargador compilado tiene cada mensaje (un error de sintaxis de Fluent ya hace fallar
    todos los tests de traducción, porque el `LazyLock` del cargador entra en pánico);
  - que cada mensaje usa las mismas variables en los dos idiomas;
  - que los mensajes de la migración, con selectores anidados, se forman bien.

  Lee los archivos del disco, así que un `.ftl` o un `.rs` nuevos no piden tocar el test. Lo
  probé quitando una clave y una variable del inglés: falla y nombra las dos.

## La configuración real de la persona

Copia de `~/.config/linux-rt-upscaler/` y de `~/.config/linux-rt-upscaler.conf` en un
`XDG_CONFIG_HOME` de prueba (en el scratchpad), con el `lirtup` de esta rama. **La configuración
real no se tocó.**

```
$ XDG_CONFIG_HOME=<copia> lirtup migrate --dry-run
mode predeterminado: 2 fields
top level: mode predeterminado
$ XDG_CONFIG_HOME=<copia> lirtup migrate
2 fields moved; backup in <copia>/linux-rt-upscaler/config.yaml.1x.bak
$ XDG_CONFIG_HOME=<copia> lirtup migrate --dry-run
nothing to migrate: … has no loose mode fields
```

- `config.yaml` queda así: `mode: predeterminado`, `auto_crop_menu_bar: true`,
  `show_stats: true`, `frame_gen_multiplier: 4`.
- `modes/predeterminado.yaml` queda con `name: Predeterminado`, `effects_engine: magpiefx` y la
  cadena `Anime4K_Upscale_Denoise_UL`, `Anime4K_Restore_Soft_UL` y `Lanczos`.
- `config.yaml.1x.bak` tiene el mismo md5 que el `config.yaml` real.
- **Mismo resultado de escalado**: el `Config` resuelto es idéntico antes y después
  (`real_config` y `real_config_from_env`).
- `gui-config.yaml`: la persona tiene `[tray] enabled=false`, así que **`show_tray_icon` pasa a
  `false`**, como en la 1.x, donde no había ícono en la bandeja. `palette_preset: Auto` deja la
  semilla predeterminada. Ver la pregunta 1.

## Filas de la lista de paridad

| # | Estado |
| --- | --- |
| 29 | **Cerrada en lo que la 2.0 guarda**: bandeja → `gui-config.yaml`. Geometría y posición de la superposición: no se migran porque la 2.0 no las guarda (filas 40 y 11). Si Y o T las agregan, se migran en `legacy_gui` |
| 30 | **Cerrada**: `palette_preset` → `seed_color`. Los colores sueltos de `palette:`, distinto a propósito (P lo documenta) |
| 31 | **Cerrada**: la migración, en la GUI y en `lirtup migrate` |

## Lo que pido

1. **A la persona**: con la bandeja apagada en la 1.x, la migración deja `show_tray_icon: false`.
   Es paridad, pero la 2.0 se diseñó con la bandeja visible. Si se prefiere que la 2.0 arranque
   con su valor por defecto, basta con no migrar `tray/enabled` (una línea en `legacy_gui`).
2. **Revisor**:
   - `check-ftl-keys.py` queda duplicado por `i18n_check`: se puede borrar o dejar en el CI de P
     como comprobación rápida sin compilar;
   - `theme::PALETTES` (con los nombres de la 2.0) y `migrate::PALETTE_SEEDS` (con los de la 1.x)
     repiten las semillas, y un test las mantiene de acuerdo;
   - al retirar Python, `ts-to-ftl.py extract` pierde su `.ts` por defecto, pero el TSV queda y
     `check` sigue funcionando. Si se mueve `config-example.yaml`, el test `config_example` se
     saltea en vez de fallar;
   - `Cargo.toml`, archivos de bloqueo y `tailwind.css`: no necesito cambios.
3. **P**: documentar `lirtup migrate` y el diálogo, la copia `config.yaml.1x.bak` y qué pasa de
   la 1.x a la 2.0 (bandeja, paleta como semilla y colores sueltos que no pasan).

## Pendiente

- No vi el diálogo en la GUI corriendo (`dx serve`): está probado con los tests de `update` y
  compila, pero falta mirarlo en las pruebas a mano de la integración. Para verlo: un
  `XDG_CONFIG_HOME` con una copia de la configuración 1.x y abrir la GUI.
