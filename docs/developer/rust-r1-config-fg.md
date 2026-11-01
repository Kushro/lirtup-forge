# Ronda 1, frente C: Config y FG

Estado: **terminado, sin correr tests**. Rama `claude/rust-r1-config-fg-178a75` (sale de
`claude/rust-r0-contract-6aa817`). Fecha: 2026-11-01. Contrato en
[rust-r0-contract.md](rust-r0-contract.md).

`cargo check -p lirtup-config -p lsfg-sys -p lirtup-fg --tests` (rustc 1.99.1): **pasa, sin
avisos**. El `build.rs` de `lsfg-sys` compila todo el C++ vendorizado y `lsfg_c.cpp` (unos 40 s).
También pasan `cargo check` de `lirtup-proto`, `lirtup-x11`, `lirtup-cli`, `lirtup-core`,
`lirtup-effects` y `lirtup-magpiefx`, que dependen de estos crates. No se corrió `cargo build`,
`cargo test` ni `clippy`.

## Commits

1. `lirtup-config`: `Config` tolerante, perfiles, modos, `gui-config`, CLI y capas, con los tests
   de ida y vuelta de la 1.x.
2. `lsfg-sys`: `lsfg_c.cpp` sobre `LSFG_3_1`/`LSFG_3_1P` y `build.rs` con `cc`.
3. `lirtup-fg`: capa lsfg-vk, búsqueda de DLL, motor integrado seguro y multiplicador automático.

## Qué quedó

### `lirtup-config`

- **`Config::from_mapping`** es tolerante. Primero prueba el mapping entero; si falla, prueba clave
  por clave. Una clave con un valor malo se descarta con un `ConfigWarning` y queda el valor por
  defecto. Un `null` que no encaja se descarta sin aviso: en 1.x significa «sin valor». Las claves
  desconocidas van a `extra`. Los `hotkeys` se leen aparte y se aplican encima de los
  predeterminados; un `null` desasigna el atajo, igual que `""`.
- **El doble `flatten`** (`scaling` + `extra`) funciona con serde: `ModeSettings` se queda con sus
  claves y `extra` recibe el resto. Lo cubre el test `mode_keys_are_flattened_not_extra` (escrito,
  sin correr). No hizo falta llenar `extra` a mano.
- **`validate`**: los rangos de `validators.py`, más las opacidades de la superposición, la
  geometría, el color y el multiplicador. Un valor fuera de rango vuelve a su valor por defecto con
  un aviso (1.x se negaba a arrancar). No valida que exista la GPU o el monitor (hace falta el
  hardware) ni la forma de `magpie_effects`, que es de `lirtup-magpiefx::spec`.
- **`to_diff_mapping`** se porta de `to_dict(diff_only=True)`: deja afuera `program`, `log_level` y
  `log_file`; el color se compara en RGBA y se guarda en hex; los atajos se guardan como diferencia.
  Las claves de `extra` se conservan. Los `f32` se escriben con su forma más corta (`0.03`, no
  `0.029999999329447746`), porque `serde_yaml_ng::to_value` los pasa a `f64`.
- **`OutputGeometry::parse`**: la gramática de `validate_geometry`. **`BackgroundColor`**: hex de 3,
  4, 6 y 8 dígitos (alfa al final, como supone 1.x), `rgb()`, `rgba()`, `hsl()`, `hsla()`,
  `transparent` y los 148 nombres SVG que acepta `QColor`.
- **`Hotkey`**: `FromStr` y `Display` en el orden `Ctrl+Alt+Shift+Super+Tecla`; `diff_hotkeys`.
- **Perfiles**: `Match::matches` hace AND de las reglas y compara los títulos sin distinguir
  mayúsculas; la regex busca sin anclar. Una regla desconocida nunca coincide, como en 1.x.
  `Interval` acepta las formas de `parse_interval`. `find_matching_profile` toma el primero en el
  orden del archivo.
- **`ConfigFile`**: `load` separa `profiles` y lee cada perfil de forma tolerante. Un `match` malo
  se guarda tal cual en `extra`, nunca coincide y da un aviso. Un archivo que no existe es una
  configuración vacía. `save` escribe un temporal en la misma carpeta y hace `rename`; rota
  `config.yaml.bak1..5`; conserva el orden y las claves desconocidas; escribe los atajos como
  diferencia y un color en tupla como hex (`parse_profile_colors`).
- **Modos** (`ModeStore`): lee `modes/*.yaml` ordenados por id. Un archivo malo se saltea con un
  aviso. A un modo sin `name` le pone el id como nombre. `save` escribe temporal y `rename`;
  `delete` borra el archivo; `new_id` genera `modo-de-firefox`, `-2`, … y pliega los acentos. Un id
  con `/` o que empieza con `.` es un error.
- **`gui-config.yaml`**: lectura tolerante, con los números acotados a `_OVERRIDE_BOUNDS`
  (`tile_columns` admite 0 = automático). Las claves de 1.x sin uso (`palette`, …) quedan en
  `extra`. Un YAML roto da los valores por defecto.
- **Capas** (`resolve`): `merge_layer` mezcla clave por clave y `hotkeys` un nivel más adentro. Un
  `null` no pisa: significa «hereda», como `apply_overrides`, que salta `None`. **El modo va justo
  debajo de la capa que lo nombra**, así ganan los campos sueltos de esa capa y de las de arriba
  (6.6):
  - nombrado en el nivel superior: modo ← general ← perfil ← CLI;
  - nombrado por el perfil: general ← modo ← perfil ← CLI;
  - nombrado por `--mode`: general ← perfil ← modo ← resto de la CLI.

  Un perfil o un modo que no existe es `UnknownProfile` o `UnknownMode`. `-p` busca primero el
  nombre exacto y después sin distinguir mayúsculas (`find_profile` de 1.x). Al final corre
  `validate`, y `Resolved.warnings` junta todos los avisos.
- **CLI**: una bandera por campo con los nombres de `args.py`, incluidos `-s`, `-d`, `-t`, `-f`,
  `-2`, `-U`, `-D`, `-g`, `-o`, `--no-*`, `--enable-*`, `--crt-*` y `--magpie-effect` (que se
  puede repetir). Los valores con lista fija se validan en clap. `overrides()` solo trae lo que se
  pasó. `--set` lee un valor YAML y admite claves con punto (`hotkeys.zoom_in=Ctrl+Plus`).

### `lsfg-sys`

- `src/lsfg_c.cpp`: la API de `lsfg_c.h` llamada por llamada, igual que `lsfg_module.cpp`. Las
  excepciones vuelven como códigos: `LSFG::vulkan_error` da `LSFG_ERR_VULKAN` y cualquier otra da
  `LSFG_ERR_INTERNAL`; el error del cargador de shaders da `LSFG_ERR_DLL` o `LSFG_ERR_SHADERS`.
  `lsfg_last_error` es por hilo y un mutex guarda el estado global.
- `build.rs` usa `cc` con las mismas fuentes y banderas que `setup.py`: C++20, `-O2`,
  `-mtune=generic`, `-fvisibility=hidden`, `VK_NO_PROTOTYPES`, `-w`, y enlaza `dl` y `pthread`.
  Lee de `src/upscaler/lsfg/` y saca `lsfg_module.cpp` y `temple_local.cpp`. `-Wl,-Bsymbolic` no
  se pasa: era para un `.so`; acá es un archivo estático dentro de un ejecutable que no exporta
  nada, y los punteros `vk*` de volk tienen visibilidad oculta. `LSFG_SYS_NO_BUILD=1` saltea el
  C++ en una máquina sin los headers de Vulkan (después el enlace falla).
- En el header quedó escrito qué pasa con los fds tras un fallo: lsfg pudo haber importado
  algunos, así que quien llama **no los cierra** (perder unos fds en un error no hace daño; cerrar
  uno que ya es del driver, sí). `LSFG_ERR_ARGUMENT` y `LSFG_ERR_STATE` se devuelven antes de
  importar nada.

### `lirtup-fg`

- `dll`: `steam_libraries` lee las raíces de Steam (nativa, `~/.steam/*` y Flatpak) y el
  `libraryfolders.vdf` de cada una. Devuelve solo carpetas que existen, sin repetidas tras
  `canonicalize`. `find_dll` hace lo mismo que 1.x con la pista, pero **si la pista no tiene la DLL,
  busca en Steam**, como dice el contrato (1.x devolvía `None`).
- `layer`: `find_layer` (XDG y `/etc`), `external_env` y `prepare`, port de `framegen.prepare`.
  Usa la marca `LIRTUP_FORGE_FRAMEGEN` para reconocer las variables propias y escribe el mismo
  `lsfg-vk.toml`; `auto` corre a ×2 con la capa. `restart_self` hace `exec` de `/proc/self/exe` con
  el mismo `argv`.
- `native`: `probe`, y `LsfgEngine` como único por proceso (`AtomicBool`) con un mutex para todas
  las llamadas. `LsfgContext` borra su contexto al soltarse y `LsfgEngine` llama a `lsfg_finalize`
  con el último `Arc`. Se comprueba que haya tantos fds de salida como cuadros generados. Si la
  llamada sale bien, los fds pasan a lsfg (`into_raw_fd`); si falla después de las comprobaciones,
  quedan sin cerrar.
- `multiplier`: `ideal_multiplier` y `AutoMultiplier` como en `native_framegen.py`: margen de 2 %,
  subida solo hasta el 97 %, 2 s de espera, 5 s entre cambios y 5 fps como mínimo.

### Tests (escritos, sin correr)

- `lirtup-config/tests/files_1x.rs`, para el criterio de 5.8:
  - `config-example.yaml` se lee sin avisos; el único campo desconocido es
    `pipeline_idle_interval`;
  - los cinco perfiles coinciden con las ventanas esperadas;
  - ida y vuelta: cargar, guardar en una copia y volver a cargar da las mismas opciones y perfiles
    y la misma configuración resuelta por perfil;
  - un segundo guardado da los mismos bytes;
  - el original no se toca.

  Lo mismo para `tests/fixtures/config-1x-user.yaml`: la forma real de la 1.3, con cadena MagpieFX
  larga, atajos como diferencia y color en hex. Y **la configuración instalada de la persona** si
  existe (`~/.config/linux-rt-upscaler/config.yaml`, o `LIRTUP_CONFIG_FILE`): solo lectura, la
  copia va a una carpeta temporal.
- Tests unitarios de color, atajos, intervalos y `Match`, geometría, `from_mapping`, diff,
  `validate`, capas y modos, modos en disco, `gui-config` y CLI (con `Command::debug_assert`).
- `lirtup-fg`: el parser del VDF y la búsqueda en bibliotecas; la pista de la DLL; el toml (igual
  byte por byte al de 1.x); `prepare` en sus estados (un solo test toca el entorno, así no compite
  con los otros); el multiplicador y su histéresis con tiempos sintéticos; `probe` de una DLL que no
  existe; y **`probe_finds_lossless_dll`**, el criterio de 5.8, que se saltea si Lossless Scaling no
  está instalado (o usa `LIRTUP_LOSSLESS_DLL`).

## Cambios de API respecto del contrato

Solo agregados. Ninguna firma cambió, salvo lo que se ve desde la línea de comandos.

- **CLI**: `-g` es `--gpu`, como en 1.x. La geometría es `-o/--output-geometry`, con
  `--geometry` como alias visible. La ronda 0 tenía `-g/--geometry`, que chocaba con 1.x.
  `-q`/`-v` no están en `OptionArgs` porque `lirtup-cli` ya las declara como globales y clap
  entraría en pánico por el choque; en su lugar quedan `--debug` (como en 1.x), `--log-level` y
  `--log-file`. `--set` sigue.
- `lirtup-config`, funciones públicas nuevas:
  - `Modifiers::from_name`, `BackgroundColor::try_rgba`, `Match::check`;
  - `Profile::from_value`, `Mode::from_mapping`, `ModeStore::get`;
  - `ConfigFile::to_yaml`, `file::MAX_BACKUPS`, `resolve::find_profile`, `GuiConfig::clamp`.

  `Interval` ahora tiene un `Serialize` escrito a mano (antes era derivado), para que `720` se
  guarde como `720` y no como `720.0`.
- `lirtup-fg`, públicos nuevos:
  - `dll::STEAM_APP_DIR`, `layer::LAYER_MANIFEST`, `layer::toml_contents`;
  - `multiplier::{AUTO_MIN_MULTIPLIER, AUTO_MAX_MULTIPLIER, AUTO_START_MULTIPLIER}`;
  - `LsfgContext::id`, y `Debug` para el motor y el contexto.

  El nombre del manifiesto de la capa se corrigió en el doc de `find_layer` a
  `VkLayer_LSFGVK_frame_generation.json` (el de 1.x).
- **`unsafe` en `lirtup-fg`**: el contrato lo permitía solo en `lirtup-gpu`, `lirtup-x11` y
  `lsfg-sys`, pero el envoltorio seguro de la FFI no se puede escribir sin `unsafe`. Quedó
  `#[allow(unsafe_code)]` en el módulo `native` y en las dos funciones de `layer` que tocan el
  entorno (`set_var` y `remove_var` son `unsafe` en la edición 2024). Cada bloque lleva su
  `// SAFETY:`.

## Diferencias de comportamiento con 1.x (a propósito)

- Un valor fuera de rango vuelve a su valor por defecto con un aviso, en lugar de impedir el
  arranque.
- `"720..1080"`: en 1.x la regex lo leía como 0,108..720 (un error de 1.x). Acá es 720..1080, como
  dice su documentación.
- Copias de respaldo: `.bak1` a `.bak5`. La rotación de 1.x tenía un error por uno y escribía
  `.bak2` a `.bak5` (así está la carpeta de la persona hoy).
- Un `config.yaml` con YAML roto es `ConfigError::Yaml`. 1.x lo trataba como vacío, y así la GUI
  podía pisarlo al guardar.
- Los modificadores de los atajos no distinguen mayúsculas (`alt+S` vale).
- `to_hex` redondea en vez de truncar (`int(r*255)` en 1.x).
- `serde_yaml_ng` es YAML 1.2: `yes`, `no`, `on` y `off` no son booleanos. En un campo booleano
  dan un aviso y el valor por defecto. La 1.x los escribía como `true`/`false`, así que no debería
  pasar.
- Al guardar se pierden los comentarios del YAML, igual que con `yaml.dump` en 1.x. Las claves de
  cada perfil quedan en el orden `match`, `options` y el resto.

## Lo que pido al revisor

- **`Cargo.lock`**: agregar `tempfile` a las dependencias de `lirtup-fg` (es dev-dependency y ya
  está en el workspace). `cargo check` lo agrega solo; lo dejé sin tocar porque es del revisor.
- Opcional: `cc = { version = "1.10.0", features = ["parallel"] }` en el workspace, para compilar
  en paralelo los 39 `.cpp` de lsfg. Hoy tarda unos 40 s en serie.
- No hacen falta dependencias nuevas.

## Pendiente

- **Correr los tests** (`cargo test -p lirtup-config -p lirtup-fg`). Lo que más conviene mirar:
  - `files_1x.rs`, sobre todo «un segundo guardado da los mismos bytes»;
  - `mode_keys_are_flattened_not_extra`;
  - `cli::command_is_valid`, por si clap rechaza `-2` como corto;
  - `probe_finds_lossless_dll`, con la DLL de la persona.
- El frente K tiene que llamar a `layer::prepare` antes de crear el runtime de tokio y los hilos,
  porque cambia el entorno. Con `lirtup serve`, `restart_self` corta el socket: la GUI tiene que
  reconectarse.
- El estado `NoNative` (sin soporte de fd externo, o si `LsfgEngine::initialize` falla) lo decide
  el pipeline (frente F, ronda 2). `prepare` solo devuelve `NoDll` o `Active` para el motor
  integrado.
- Migración de la configuración a modos: ronda 4, frente M.
