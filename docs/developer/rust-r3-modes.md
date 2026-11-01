# Ronda 3, frente D: modos, medición y Ajustes

Rama **`claude/rust-r3-modes-e64822`**, que sale de `rust/r3-skeleton` (`01cd43d5`). Fecha:
2026-11-01. Sin push y sin merge.

- `cargo check -p lirtup-ui --all-targets`: **pasa**.
- `cargo clippy -p lirtup-ui --all-targets`: **sin avisos en los archivos de D**. Quedan solo los
  `todo!()` de A y C.
- `cargo test -p lirtup-ui`: **25 tests en verde**, 21 de ellos de D.
- Compilé con `CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r3-d`. No hice `cargo build
  --release` y no toqué `assets/tailwind.css`, los `Cargo.toml`, `package.json` ni los archivos de
  bloqueo.
- **No vi la pantalla.** Los componentes (A) y `store.rs` (C) siguen en `todo!()`, así que la app
  entra en pánico. Todo lo de D usa los componentes y el estado **solo por sus firmas**. Las clases
  Tailwind que escribí (diseño de página, no controles) salen del lienzo y de `tailwind.css`. Hay
  que verlas después de integrar A y C.

Commits:

| Commit | Qué |
| --- | --- |
| `8f2cf12d` | `screens/modes/**` y `modes.ftl` (es, en) |
| `b6de5886` | `screens/bench.rs` |
| `d1a59094` | `screens/settings/**` y `settings.ftl` (es, en) |

## Qué hice

### `screens/modes/`

| Archivo | Qué |
| --- | --- |
| `mod.rs` | `ModesPage`: la lista a la izquierda (máx. 340 px) y el editor a la derecha, o «Elige un modo». `ModeList` muestra cada modo con modelo, efectos y uso («fast · Anime4K + LumaSharpen · 3 perfiles»), su costo y un punto si tiene cambios sin guardar. Tiene «Nuevo modo» y `DeleteModeDialog`, que avisa cuántos perfiles pasan a Predeterminado |
| `editor.rs` | `ModeEditor`, con `key` por id para que sus hooks empiecen de cero al cambiar de modo. Incluye: la cabecera (nombre en línea con `SetField name`, «Usado por A, B y C», Duplicar, Eliminar); `SharedModeBanner` desde 2 perfiles; `ModelRow` (un `Slider` con los 10 `stops` de `Model::ALL`); el escalado doble; el muestreo (sobremuestreador, submuestreador, desenfoque y un grupo plegable «Opciones de Lanczos» con anti-ringing, anti-ringing estricto y radio forzado); el motor con `Connected<EffectsEngine>` y su explicación; según el motor, la cadena MagpieFX o las 3 pasadas nativas (Anime4K, LumaSharpen y CRT, este último con sus 17 parámetros plegados); el total con la GPU y «Medir en mi GPU»; y el retoque final (CAS, supresión de bandas, resplandor, viñetado, grano y LUT, con su tabla). Cada ajuste aparece solo mientras su interruptor está encendido |
| `chain.rs` | `EffectChain` y `EffectCard`. El arrastre es HTML5 y solo lo empieza la manija (`draggable` se activa con `mousedown` en la manija), para que los sliders de adentro sigan andando. Con el teclado: flechas sobre la manija, o los botones subir y bajar. Cada tarjeta muestra el costo, «Sin probar» (`!verified`), «Tuyo», compilando, y los errores: desactivado por la sesión (`EffectCompile { session: true }`, en error-container), no compila, no se pudo leer, o no está en el catálogo. `EffectParam` arma un slider por cada `EffectParamInfo` (rango, paso, entero) y muestra el valor predeterminado cuando cambió. Hay «Restablecer los parámetros». `ChainItem` lee las 3 formas (`"Name"`, `"Name:p=1"` y la larga) y **escribe sin cambios las que no se editaron** |
| `picker.rs` | `EffectPickerDialog`: búsqueda, chips Todos (N) / Verificados / Míos, «Sin probar» y «Tuyo», «En la cadena», el costo, y los efectos con error de lectura deshabilitados. Si el catálogo está vacío, pide `ListEffects`. «Agregar» suma `Short(name)` a la cadena, manda `CompileEffect(name)` para compilar por adelantado y cierra |
| `costs.rs` | El costo de cada pasada: lee `bench.json` una vez por pantalla (la GPU con `selected` de `ListDevices`, por la clave `vendor:device:name`) y encima pone el último `BenchResult`. Las pasadas nativas van por su etiqueta en el bench (`cas`, `anime4k`, `crt_easymode`…) y los efectos MagpieFX por nombre. El total de un modo es la suma de sus pasadas, o «Sin medir» si falta alguna. El modelo SRCNN no se mide por separado |
| `widgets.rs` | Lo que comparten modos y Ajustes: `use_value` y `use_origin` (memos sobre el store, así cada fila se vuelve a dibujar solo cuando cambia su ajuste) y `use_control` (la señal del control, que sigue al store). También `NumberRow`, `SwitchRow`, `CostBadge`, `fmt_num`/`fmt_ms` con la coma decimal del español, `number()` (enteros como enteros, flotantes redondeados a 4 decimales) e `inherit()`. Las filas usan `SetField` e `Inherit` |

### `screens/bench.rs`

`BenchDialog` tiene 3 pasos en un solo diálogo:

1. **Elegir**: tamaños con `Connected` (540p → 1080p, 720p → 1440p, que es el que trae marcado,
   o 1080p → 2160p) y un interruptor para incluir la generación de cuadros. Si el modo tiene cambios
   sin guardar, avisa que se mide la versión guardada.
2. **Progreso**: `BenchProgress` con barra (`role="progressbar"`) y Cancelar (`CancelBench`).
3. **Resultado**: una tabla por pasada (mediana, p95 y error) y el total del modo, sin contar la
   generación de cuadros, con un aviso si se canceló. Ofrece «Medir otra vez» y «Listo».

Si al abrir ya hay una medición corriendo, muestra el progreso. Cerrar mientras mide manda
`CancelBench`. `frames: 0` deja que el bench use su presupuesto por caso.

### `screens/settings/`

| Archivo | Qué |
| --- | --- |
| `mod.rs` | `SettingsPage` con `Tabs<SettingsTab>`. Cambiar de pestaña es `Navigate`, y `same_page` hace que no pregunte. **General** tiene: idioma (Sistema / Español / English, cada idioma en su propio nombre), tema, semilla (`ColorField` con 21 muestras: la predeterminada y el acento de cada paleta de 1.x), cerrar a la bandeja, empezar oculto, modo daemon (`SetDaemon`, con «vigilando N perfiles») y seguir ejecutándose tras salir. Todo se escribe al instante con `SetGui` (`write_gui` solo despacha si algo cambió) |
| `hotkeys.rs` | **Atajos**: los 15 `HotkeyAction` en los grupos del diseño (Activación, Sesión, Vista, Zoom y desplazamiento), con `HotkeyCapture { require_modifier: true }`. El subtítulo muestra el predeterminado o **«También lo usa: …»** si hay conflicto (las teclas se comparan sin distinguir mayúsculas). «Restablecer» de cada fila es `Inherit`, y hay «Restablecer todos». Ver «Lo que necesito de C» |
| `style.rs` | **Estilo**: zoom, escala de fuente, fuente (`FontField` con `fc-list : family`, leído en un hilo aparte una sola vez), forma de los mosaicos, columnas (0 = Auto) y un grupo plegable con los intervalos de la lista y de la vista previa |

### Traducciones

`modes.ftl` (incluye las claves `bench-*`) y `settings.ftl`, en español y en inglés. Un test
recorre los archivos de D y verifica que cada clave literal de `t!` esté definida en los dos
idiomas. Otro test hace lo mismo con las claves dinámicas (títulos de las filas, tablas LUT, acciones
de los atajos y grupos).

## Decisiones

- **El diálogo y la tarjeta no estilan controles**: usan solo los componentes de A. Lo que lleva
  clases propias es diseño de página: la lista de modos (un `button` por fila, como el
  `.group > .row` del lienzo), las tarjetas de la cadena, la barra de progreso y la tabla de
  resultados.
- **`Disclosure` no existe en las firmas de A.** Usé `SettingGroup { collapsible, collapsed }`
  para las opciones de Lanczos, las de CRT (más de 4 parámetros) y los intervalos.
- **Cadena**: cada cambio escribe la lista entera de `magpie_effects` como una sola edición. Un
  efecto editado se guarda en la forma larga (`{name, params}`). Si vuelve a sus valores
  predeterminados, pasa a la forma corta.
- **Radio del núcleo**: apagar «Forzar el radio» es `Inherit` (quita la clave), no `null`.
- **Costos**: los leo de `bench.json` en la GUI, sin pedirle nada al núcleo, porque es un archivo
  que la GUI ya puede leer y es el mismo que escribe `lirtup bench`.
- **Bandeja**: el lienzo tiene «Mostrar ícono en la bandeja» y «Minimizar a la bandeja», pero
  `GuiConfig` no tiene esos campos. No los puse. Los agrega quien haga la bandeja (ronda 4,
  frente Y), en `GuiConfig` y en `General`.
- **«Barra de herramientas» (Alt+Shift+D)**: está en el lienzo pero no en `HotkeyAction`. No la
  puse: llega con la barra (ronda 4, frente T).

## Lo que necesito de A (componentes)

1. **`SettingRow` fuera de un perfil.** Con `Origin::Inherited` y sin `subtitle`, la fila dice
   «Hereda de Predeterminado», que está mal en un modo (la capa de abajo es el valor del sistema).
   En Ajustes no hay capas: ahí siempre paso `subtitle` y `Origin::Inherited` (`PlainRow`). Pido
   una de estas dos:
   - un prop `inherited_note: Option<String>` (para un modo, «Valor del sistema»);
   - un prop `plain: bool` sin franja ni nota.
2. **`Slider` con `stops`**: el valor es el índice (lo dice la firma). `ModelRow` lo usa así.
3. **`IconButton` dentro de un `span` con `onkeydown`**: la manija de la cadena escucha las flechas
   del botón por burbujeo. El botón no tiene que llamar a `stop_propagation` en `keydown`.
4. **`Dialog`**: abro todos con `open = use_signal(|| true)` y cierro con `on_close` →
   `CloseDialog`. Espero que Esc y el fondo llamen a `on_close`.
5. **`HotkeyCapture`**: Backspace deja `None`, que guardo como `""` (sin asignar), igual que 1.x.

## Lo que necesito de C (estado)

1. **`update` de `RunBench`** tiene que poner `bench.report = None` (y `done`/`total` en 0).
   El diálogo decide «corriendo» o «listo» según si `report` es `None`.
2. **`CreateMode`**: después de despachar `CreateMode { name }`, navego a
   `ModeStore::new_id(name)`. Si `update` ya navega, mi `Navigate` no hace nada. Si `CreateMode`
   deja el modo como edición sin guardar, `Navigate` no tiene que abrir «Cambios sin guardar» por
   ese modo nuevo.
3. **`DeleteMode`**: cerrar el diálogo y, si la ruta era ese modo, volver a
   `Modes { id: None }`. Igual, si el modo desaparece, el editor muestra «Este modo ya no existe»
   con «Volver a la lista».
4. **`DuplicateMode { for_profile: None }`**: el editor no sabe de qué perfil vino (la ruta
   `Modes { id }` no lo lleva). Si quieren «duplicar para este perfil» desde el aviso de modo
   compartido, hace falta un `from: Option<ProfileRef>` en `Route::Modes` (lo decide el revisor).
   Por ahora el aviso solo duplica.
5. **`Store::field` con `Scope::Mode` y la clave `name`**: renombrar es
   `SetField { key: "name" }`, y `apply_mode_edits` tiene que tomarla como el `Mode::name`.
6. **Atajos**: son `hotkeys.<acción>` de `Scope::Profile(ProfileRef::Default)`. Ajustes no tiene
   barra de cambios, así que cada cambio es `SetField` (o `Inherit`) seguido de `Save`. Hay dos
   condiciones:
   - `Save` no tiene que tirar ediciones de otras páginas sin preguntar (en Ajustes no debería
     haber, porque salir de un perfil con cambios pregunta);
   - el aviso de guardado puede decir «Perfil «Predeterminado» guardado», que no queda bien en
     Atajos.

   Si C prefiere una acción propia (`SetHotkey { action, binding }`, que escriba y recargue sin
   tocar otras ediciones), cambio `hotkeys.rs` en la integración.
7. **`keep_running_after_exit`**: `SetDaemon { on }` no lleva `keep_after_exit`. Espero que
   `update` lo tome de `store.gui` al mandar el comando, y que lo mande también cuando cambia
   `SetGui` con el daemon encendido.

## Lo que necesito del núcleo y del revisor

1. **`bench` con modos**: `build_cases` ya acepta `mode` y mide sus pasadas nativas por etiqueta
   y su cadena MagpieFX. Falta lo que dice el esqueleto: `sizes` y `selectors` en `bench.json`.
   Además, **que `Bench` guarde el resultado en `bench.json`** (`save_results`) cuando lo pide la
   GUI, porque el editor saca de ahí el costo de cada pasada entre sesiones. Y que `BenchResult`
   llegue también después de `CancelBench` (el contrato ya lo dice).
2. **Arrastre en la webview**: en algunas plataformas, el manejador de arrastre de archivos de wry
   se come los eventos HTML5 `drag*`. Si la cadena no reordena al arrastrar, hay que ponerle a la
   ventana de `main.rs` `with_disable_drag_drop_handler(true)` (lo decide el revisor; los botones
   y las flechas andan igual).
3. **`i18n::has_key`** devuelve `false` para los mensajes con argumentos cuando se lo llama sin
   ellos (`lookup_single_language(…, None)`). El test de la ronda 4 que lo usa va a dar falsos
   negativos. Por eso mi test lee los `.ftl` directamente.

## Dependencias

Ninguna nueva. Uso las que ya están: `serde_yaml_ng` (`to_value`), `serde_json` (`bench.json`),
`indexmap`, `tokio` (`spawn_blocking` para `fc-list`).

## Pendiente

- Mirar las tres pantallas después de integrar A y C: clases, foco, teclado y los dos temas.
- Flujo 4 de punta a punta contra `lirtup serve`: agregar, reordenar, medir y ver el cambio en vivo
  con `ReloadConfig`.
- Debounce de los sliders de Estilo: hoy cada `on_change` escribe `gui-config.yaml`. Si `Slider`
  avisa durante el arrastre, conviene esperar 300 ms. Se arregla en `write_gui`.
- El aviso de modo compartido aparece desde 2 perfiles y se oculta con «Editar igual» (solo
  mientras dura la página).
