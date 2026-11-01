# Ronda 3, frente C: perfiles y estado

Rama **`claude/rust-r3-profiles-8f029e`**, que sale de `rust/r3-skeleton`. Fecha: 2026-11-01. Sin
push y sin merge.

Notas relacionadas: el esqueleto ([rust-r3-skeleton.md](rust-r3-skeleton.md)) y la sección 6 del
análisis ([rust-refactor-analysis.md](rust-refactor-analysis.md)).

## Resumen

| | Resultado |
| --- | --- |
| `cargo check -p lirtup-ui --tests` | pasa |
| `cargo clippy -p lirtup-ui --tests` | **0 avisos en mis archivos**. Los que quedan son los `todo!()` de A, B y D |
| `cargo test -p lirtup-ui` | **44 de 44**, más 1 ignorado (`real_config_round_trip`, que lee un archivo real) |
| `real_config_round_trip` sobre una copia de los 4 `config.yaml` reales de la persona (`config.yaml` y `.bak2` a `.bak4`) | **pasa en los 4**: leer y guardar sin ediciones da el mismo texto, byte a byte |
| Ver la página | **no la vi**: los componentes de A siguen en `todo!()`, así que la app entra en pánico apenas dibuja |

`CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r3-c`. No hay dependencias nuevas.

Commits (sin coautoría):

1. `0505e2a8`: `store.rs` entero (`Field`, capas, ediciones, `update()` y sus tests) y lo que
   agregué a `state/mod.rs`.
2. `812185f3`: la página de perfil, Nuevo perfil, los diálogos y `profile.ftl`.
3. `7f0006a2`: lo que pidieron B y D en sus notas (Detener cierra la tarjeta de error, el error del
   `Snapshot` sin «Reintentar», la clave `name` de un modo sin nombre).

## Qué hice

### `state/store.rs`

**Capas.** Un ajuste se busca por su clave YAML en una capa, con las ediciones sin guardar de esa
capa encima. Lo que cada capa hereda sigue el orden de `lirtup_config::resolve`:

- un archivo de modo hereda los valores del sistema (`Config::default()` como mapping);
- *Predeterminado* hereda el modo que nombra (solo para las claves de modo) y después los valores
  del sistema;
- un perfil hereda *Predeterminado*. Para las claves de modo, el modo que nombra el perfil queda
  en el medio y el de *Predeterminado* no se usa, igual que en el resolver. Lo cubre el test
  `modes_sit_where_the_resolver_puts_them`.

Un `null` en una capa significa «hereda», como lo lee `merge_layer`. Lo que hereda una capa
incluye las ediciones sin guardar de la capa de abajo: si se cambia *Predeterminado*, el perfil
muestra el valor nuevo como heredado.

**`Field<T>`**:
- `value()` devuelve la edición, si no el valor de la capa, y si no el heredado;
- `is_dirty()` es verdadero cuando hay una edición distinta de lo guardado;
- `origin()` da `Dirty { before }` con el valor guardado, `Overridden` si la capa lo pone e
  `Inherited` si no.

`Store::field::<T>` arma todo eso con serde. Un valor que no se puede leer como `T` cuenta como
no puesto.

**Ediciones** (`ConfigDoc::edits`):
- Una edición igual al valor guardado se descarta. Así, deshacer un cambio a mano apaga el punto
  en la fila, en la pestaña y en el cajón. Los números se comparan por valor (`20` y `20.0`).
- `hotkeys.<acción>` va un nivel más adentro.
- Las reglas de un perfil son una edición más, con la clave `match` (`MATCH_KEY`): `SetMatch`
  queda sin guardar hasta Guardar, como dice la firma.
- `apply_edits` fusiona sobre los `Mapping` tal como se leyeron. Una clave que ya estaba conserva
  su lugar, y las claves que la GUI no conoce no se tocan. Las quitas usan `shift_remove`, para
  que no se reordene nada.

**Sucio en los tres niveles**:
- fila: `Field::origin`;
- pestaña: `tab_dirty`, sobre `tab_keys`;
- cajón: `profile_dirty`.

La barra y la pregunta al salir usan `Store::page_edits(route)`, que es lo de la capa más las
claves globales editadas en *Predeterminado*. Esas se ven en cualquier página de perfil, así que
las cuenta la barra de esa página, pero el punto del cajón queda en *Predeterminado*, que es donde
se guardan.

**Escrituras.** El store muestra el resultado enseguida y guarda en `Store::saving` lo que había
antes, por si la escritura falla:
- `Save` manda `WriteConfig(apply_edits(…))` y los `WriteMode` de los modos que cambiaron;
- `ReloadConfig` sale recién cuando llegó el último `Written(Ok)`, junto con el aviso;
- si un `Written` llega con `Err`, vuelven el archivo y las ediciones de antes, con
  `toast-write-failed`;
- solo el aviso de un Guardar trae «Deshacer»: `Store::undo` es el `ConfigFile` de antes, y
  `Undo` lo escribe de nuevo.

Crear, renombrar, duplicar y borrar un perfil, igual que el recorte de la barra de menú, escriben
el archivo guardado en ese momento y dejan las ediciones pendientes como estaban.

**`update()`** trata todas las `Action`:
- **Navegación.** Si la página que se deja tiene ediciones, abre `Dialog::Unsaved`; con otra
  pestaña de la misma página no pregunta. «Guardar y cambiar» guarda y navega, «Descartar y
  cambiar» tira las ediciones de esa página y navega, y «Cancelar» se queda.
- **Restablecer.**
  - «Deshacer los cambios de esta pestaña» borra las ediciones de las claves de la pestaña,
    también las globales que se ven en ella.
  - «Quitar los ajustes del perfil en esta pestaña» marca para heredar las claves de la pestaña
    que pone el perfil. No toca las globales de *Predeterminado*.
  - «Volver todo a los valores del sistema» marca para heredar todas las claves conocidas de la
    capa, salvo `hotkeys` (se editan en Ajustes) y, en un perfil, las globales. Las claves
    desconocidas quedan.
- **Perfiles.**
  - `CreateProfile` copia las `options` del perfil de origen (copiar «Predeterminado» es
    heredarlo). También pone el `mode`, las reglas y la ruta del ícono, y abre la página nueva.
  - `DuplicateProfile` crea «<nombre> (2)» justo después del original. Uso un número porque
    `update` no sabe el idioma.
  - `RenameProfile` conserva la posición en el archivo y lleva las ediciones al nombre nuevo.
  - `DeleteProfile` tira las ediciones del perfil y vuelve a *Predeterminado*.
- **Modos.**
  - `CreateMode` usa `ModeStore::new_id`, escribe y navega al editor.
  - `DuplicateMode` hace lo mismo con «<nombre> (2)». Pasa a la copia las ediciones sin guardar
    del original (eran para la copia) y, con `for_profile`, cambia el `mode` del perfil.
  - `DeleteMode` cierra el diálogo y vuelve a la lista.
- **Núcleo.** Cada evento actualiza el `Store`:
  - `Snapshot`: sesión, daemon, FG y versión. Un `last_error` sin sesión queda como `Failed` sin
    «Reintentar», porque la GUI no sabe qué se escalaba;
  - las cinco de la sesión;
  - `Stats`, `FrameGen` y `DaemonState`;
  - `WindowPicked`, que va a `picked`;
  - `Effects` y `EffectCompile`, este último con `session`;
  - `Devices`, los dos `Bench*` y `Diagnostics`;
  - `Log`, con un máximo de `MAX_LOGS` líneas.
- **`MenuBarDetected`** guarda el recorte en el perfil de la sesión y pone
  `auto_crop_menu_bar: false` en ese perfil, como `remember_menu_bar_crop` de la 1.x. Sin perfil
  no guarda nada.
- **Comandos de B y D.**
  - `RunBench` reinicia `bench`, así que `report` queda en `None`.
  - `SetDaemon` toma `keep_after_exit` de `gui`.
  - `SetGui` escribe, y además manda `SetDaemon` si cambió «seguir tras salir» con el daemon
    encendido.
  - `StopScaling` también cierra una tarjeta de error.
- **`WatchGpuTimes`.** Al final de cada `update`, se calcula «conectado y ventana visible y
  sesión corriendo y ruta Inicio». Si cambió respecto de `Store::gpu_times`, sale
  `Send(WatchGpuTimes { on })`. Cuando el socket se cae, `gpu_times` vuelve a `false` sin mandar
  nada, porque el servidor olvida esa conexión. Al reconectar, se vuelve a pedir.

Al final de `update`, si la ruta apunta a un perfil que ya no existe, pasa a *Predeterminado*, y
`ModesDoc::users` se recalcula con los modos efectivos, ediciones incluidas.

### `screens/profile/**` y `new_profile.rs`

- `mod.rs` tiene:
  - `ProfilePage`: cabecera, `Tabs` con el punto de cada pestaña y el contenido, con `key` por
    perfil y pestaña para que las señales empiecen de cero. Pide `ListDevices` una vez;
  - `ProfileHeader`: el ícono (el PNG de `icon:` como `data:` URL, o las iniciales), el nombre,
    las reglas como chips, «Editar perfil» y ⋮ con Duplicar y Eliminar;
  - `DeleteProfileDialog` y `save_bar_scope`.

  «Editar perfil» abre en la misma cabecera el nombre, el editor de reglas y la vista previa. No
  puede ser un diálogo, porque `Dialog` no tiene una variante para eso y `app.rs` hace `match`
  exhaustivo sobre él. Renombrar se escribe al tocar «Listo», como en la 1.x; las reglas quedan
  sin guardar.
- `rows.rs`:
  - `RowSpec` con su `Control`: interruptor, desplegable, botones conectados, número, número
    opcional, texto, ruta o color;
  - `Row`, que arma `SettingRow` con su `origin` y su control, con «volver a heredar» solo en
    perfiles con nombre y claves no globales, y `global` en las claves globales;
  - las filas especiales: modo (más «Editar modo» y el aviso de campos de modo sueltos de la
    1.x), monitor, GPU y geometría (Ajustar, Estirar, Cubrir o una personalizada que se valida
    con `OutputGeometry::parse`);
  - `use_synced`, la señal que sigue al store: los componentes reciben `Signal`, pero la verdad
    está en el store.
- `tabs.rs`: las 5 pestañas como tablas de filas, con los grupos del lienzo. Avanzado tiene sus
  grupos plegables. Un test comprueba que cada clave de `tab_keys` tenga su fila y que cada
  etiqueta exista en los dos idiomas.
- `framegen.rs` (`FrameGenSection`):
  - el título «Generación de cuadros» con «Global · los perfiles no lo cambian»;
  - la tarjeta de estado: activa con ×N o con Auto, apagada o configurada, más el motivo;
  - la insignia «Motor listo» o «Motor no disponible»;
  - motor, multiplicador (con Auto en primario y el aviso de que Auto necesita el motor
    integrado), escala de flujo, modelo y la carpeta de la DLL;
  - «Medir en mi GPU», que abre `Dialog::Bench`;
  - **«Guardar y reiniciar»** con una sesión corriendo y cambios en la sección: la acción nueva
    `SaveAndRestart` manda `Restart` después del `ReloadConfig`.

  Con `env_locked`, todo queda deshabilitado y se muestra «Definida por variables LSFGVK_*».
- **La traducción del código de motivo.** `reason_key(FrameGenReason)` tiene una clave por cada
  uno de los 6 códigos (`fg-reason-*`). Debajo va `reason`, el detalle en inglés, en mono. Si el
  núcleo no manda código, sale del `FrameGenState`.
- `unsaved.rs`: «Cambios sin guardar» con el nombre de la página que se deja, y Cancelar,
  Descartar y cambiar y Guardar y cambiar.
- **`new_profile.rs`**:
  - el ícono: capturado de la ventana elegida (`_NET_WM_ICON` con su propia conexión de X11, en
    un hilo aparte), de un archivo (un `input type=file` que lee cualquier imagen con `image`) o
    ninguno;
  - el nombre, con validación en línea: vacío, o ya usado sin distinguir mayúsculas;
  - el modo y «Copiar ajustes de»;
  - `RuleEditor`, que también usa la cabecera: tipo y valor por fila, con los errores de
    `Match::check` debajo;
  - `MatchPreview`, en verde con coincidencias y en ámbar sin ellas, sobre `Store::windows`;
  - el panel *Capturar ventana*, con la lista de ventanas y «Señalar ventana» (`PickWindow`,
    cuya respuesta toma de `Store::picked`).

  Elegir una ventana completa el nombre, las reglas (título contiene, ancho y alto) y el ícono.
  El diálogo escribe el PNG en `icons/<nombre>.png`, junto al `config.yaml`, como la 1.x; es lo
  único que escribe. Después despacha `CreateProfile`, y `update` solo anota la ruta.

### `i18n/{es,en}/profile.ftl`

Tiene todas las claves de la página, los diálogos, Nuevo perfil, la generación de cuadros (los 6
motivos) y los avisos del estado (`profile-toast-*`). En los avisos uso `toast-saved` y
`toast-undone` de `app.ftl`, y para lo que no es un perfil con nombre, `profile-toast-saved`
(«Cambios guardados.»). Así, los atajos de Ajustes no dicen «Perfil «Predeterminado» guardado».

## Lo que agregué a `state/mod.rs`

Solo agregué; no cambié ni borré nada.

| Qué | Para qué |
| --- | --- |
| `Store::saving: Option<Saving>` | Las escrituras en curso |
| `struct Saving { config, modes, undo, pending, error, done, undo_to, restart }` | Lo que se repone si una escritura falla, el aviso del final y si hay que reiniciar |
| `Action::SaveAndRestart` | «Guardar y reiniciar» de la generación de cuadros (flujo 5) |

También son públicos en `store.rs`, además de las firmas que ya estaban:
- `is_global`, `effective_scope` (la capa adonde va una edición), `icon_path`, `format_value` y
  `MATCH_KEY`;
- `Store::page_edits(route)` y `Store::windows_matching(matcher)`.

## La lista de 6.7: ajustes por perfil que no tienen sentido

`global_keys()` devuelve estas claves. Una edición desde un perfil va a *Predeterminado*, y la
fila lleva la insignia «Global» y no tiene «volver a heredar».

| Claves | Por qué son globales |
| --- | --- |
| `frame_gen_engine`, `frame_gen_multiplier`, `frame_gen_flow_scale`, `frame_gen_performance`, `frame_gen_dll` | La capa lsfg-vk se configura antes de crear la instancia de Vulkan, una vez por sesión. La 1.x ya las trataba como globales (`set_global_option`) |
| `daemon`, `daemon_poll_interval` | El daemon busca ventanas antes de que se aplique ningún perfil. El encendido es `SetDaemon`, no una clave de perfil |
| `target_delay`, `pid_timeout`, `class_timeout`, `total_timeout`, `starting_phase` | La detección de la ventana corre antes de elegir el perfil |
| `log_level`, `log_file` | El proceso tiene un solo registro para todas las sesiones |

No entraron en la lista:
- `hotkeys`: se editan en Ajustes sobre *Predeterminado*, y no aparecen en la página de perfil,
  así que no hace falta redirigirlas;
- `program`, `select`, `target_title` y `target_title_regex`: son de la línea de comandos y la
  GUI no los muestra.

**Pendiente para el revisor (núcleo):** el resolver todavía aplica estas claves si un perfil las
pone, y un perfil de la 1.x puede traerlas. La GUI no las borra, porque no se pierde ningún campo,
pero tampoco las muestra. Si el núcleo las tiene que ignorar en los perfiles, eso va en
`lirtup-config`.

## Las reglas de `update` y sus tests

| Regla | Test |
| --- | --- |
| Navegar con cambios abre `Unsaved`; cambiar de pestaña no | `leaving_a_page_with_edits_asks`, `editing_a_mode_page_asks_too` |
| Las tres respuestas del diálogo | `leaving_a_page_with_edits_asks`, `save_and_go_saves_then_navigates` |
| `Save` escribe con `lirtup-config`, guarda el archivo de antes en `undo`, y `ReloadConfig` sale recién con `Written(Ok)` | `save_writes_then_reloads_and_offers_undo`, `save_writes_modes_too` |
| Deshacer | `save_writes_then_reloads_and_offers_undo` |
| Una escritura fallida devuelve las ediciones | `a_failed_write_gives_the_edits_back` |
| `MenuBarDetected` guarda el recorte en el perfil | `menu_bar_crop_is_saved_in_the_profile` |
| `WatchGpuTimes` | `gpu_times_only_with_someone_looking` |
| Los eventos actualizan el `Store` | `events_update_the_store`, `stop_closes_the_error_card` |
| Herencia | `inheritance_by_layer`, `modes_sit_where_the_resolver_puts_them`, `null_means_inherit`, `edits_of_the_layer_below_show_through` |
| Sucio en fila, pestaña y cajón | `dirty_in_row_tab_and_drawer`, `global_keys_go_to_predeterminado` |
| Un perfil de la 1.x se guarda sin perder campos, y el diff sale vacío salvo lo cambiado | `a_1x_file_saves_back_unchanged_but_for_the_edit` (el texto guardado es el original con las 2 ediciones en su lugar), `the_1x_text_itself_reads_and_writes_back`, y `real_config_round_trip` sobre los archivos reales |
| Restablecer, con sus 3 opciones | `reset_tab_undoes_only_that_tab`, `remove_overrides_inherits_the_tab`, `reset_all_keeps_unknown_keys_and_hotkeys` |
| Perfiles y modos | `create_profile_writes_and_opens_it`, `rename_duplicate_delete`, `set_match_is_an_edit_until_saved`, `create_duplicate_delete_modes`, `a_mode_is_renamed_with_its_name_key` |
| Comandos de B y D, y Guardar y reiniciar | `commands_of_b_and_d_are_sent`, `save_and_restart_restarts_after_the_reload` |

Para correr el de los archivos reales sobre una copia:

```bash
LIRTUP_UI_CONFIG=/ruta/a/una/copia/config.yaml cargo test -p lirtup-ui -- --ignored real_config
```

Los `config.yaml` reales de esta máquina no tienen perfiles, así que la parte de perfiles de la
1.x la cubre el texto de prueba `FILE_1X`, con `match`, `options`, `icon` y claves desconocidas.

## Pendientes

**Para el revisor (`app.rs`, `app.ftl` y los tipos):**
- **«Volver todo a los valores del sistema» en un perfil con nombre.** El texto de la 1.x para un
  perfil era «Quitar todos los ajustes del perfil». Dejé la clave `profile-reset-all-named` en
  `profile.ftl` por si la barra de `app.rs` la quiere usar cuando la ruta es un perfil con nombre.
- **El ícono propio en el cajón.** `app.rs::profile_icon` pasa `image: None`. El PNG está en la
  clave `icon:` del perfil. En la cabecera lo muestro como `data:` URL (`ProfileHeader`, con
  `new_profile::base64`), y el cajón puede hacer lo mismo o servirlo con un `use_asset_handler`.
- **«Duplicar para este perfil» desde el modo compartido** (D, punto 4): `Route::Modes` no sabe
  de qué perfil se vino. `DuplicateMode { for_profile }` ya lo soporta, así que solo falta el
  dato en la ruta.
- B propone una caché para `matching_profile`, porque `find_matching_profile` compila la regex en
  cada llamada. Con decenas de mosaicos, no lo vi necesario. Si se nota, la caché va en
  `lirtup-config` (`Match` con la regex compilada), no en el `Store`, para no romper
  `PartialEq`.
- `i18n::has_key` da `false` para los mensajes con argumentos (lo encontró B). Mis tests solo lo
  usan con claves sin argumentos.

**Mías, que quedan para después de integrar A:**
- **Ver la página y correr los flujos 2, 3 y 5** con `lirtup serve`. Programé contra las firmas
  de A y espero esto de los componentes:
  - que `Switch`, `Select`, `Connected`, `Slider` y `TextField` escriban su `Signal` y después
    llamen a `on_change`;
  - que `Dialog` llame a `on_close` con Esc y con el fondo;
  - que `Menu` con `at: None` se abra debajo de su hermano anterior.

  Si `TextField::on_change` dispara con cada tecla, la geometría personalizada valida y edita
  tecla por tecla. Funciona igual, pero sería mejor con `on_submit` o al perder el foco.
- **Los números opcionales en un perfil** (`hide_cursor`, `max_fps`, `scale_factor`): apagarlos
  es «heredar». Si *Predeterminado* tiene un valor, un perfil no lo puede apagar, porque un `null`
  en un perfil significa «hereda» para el resolver. Pasa lo mismo en la 1.x.
- **Los campos de modo sueltos de la 1.x en un perfil** (`model`, `cas_enabled`, …): se conservan
  y ganan sobre el modo, pero la página no los edita, porque viven en Modos. La fila del modo avisa
  cuántos hay. Los resuelve la migración de la ronda 4 (frente M).
- **Al borrar o renombrar un perfil, su ícono queda en `icons/`.** La ruta de `icon:` sigue
  apuntando al archivo viejo, que sigue existiendo. Renombrar el archivo es E/S, y lo dejé fuera
  de `update`.
- **Deshacer restaura solo `config.yaml`.** Los archivos de modo que guardó el mismo Guardar no
  vuelven atrás, porque `Store::undo` es un `ConfigFile`.

## Dependencias

Ninguna nueva. Uso `image` (íconos), `tokio` (`spawn_blocking` para la captura del ícono),
`lirtup-x11` (`X11::connect` y `window_icon`) y `serde_yaml_ng`, que ya estaban.
