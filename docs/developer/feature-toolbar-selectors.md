# Selector de modo (y de perfil) en la barra sobre el juego — estudio para la 2.1.0

Rama **`claude/toolbar-mode-selector-49f364`**, sobre `main` en `ae0cd0c0` (v2.0.1). Fecha:
2026-11-02. Solo investigación: no hay código cambiado, ni push.

**Objetivo.** Probar varios modos de escalado en una novela visual sin salir del juego: un
selector en la barra flotante (la que dibuja Vulkan) para el **modo** y, quizá, para el
**perfil**.

**Resumen.**

- El modo **ya se puede cambiar en vivo**: existe `PipelineCommand::UpdateConfig`. Un cambio de modo
  no recrea la superposición, ni el swapchain, ni la generación de cuadros. Solo rehace el
  upscaler SRCNN y la cadena de efectos. Lo único caro es MagpieFX, y solo cuando falla la caché.
- El perfil también se puede cambiar en vivo, porque `follow_focus` ya lo hace. Pero arrastra más
  cosas (la superposición, el recorte, el estado del motor) y la bandeja ya ofrece «Perfil ▸»
  reiniciando la sesión.
- **Recomiendo el flujo propuesto, con ajustes.** Solo el modo, como botón que recorre la lista,
  aplicado al momento como cambio temporal de la sesión, más «Guardar en perfil». El perfil se
  queda fuera de la barra.

---

## 1. Cambiar el modo en caliente

**Sí, sin reiniciar la sesión.** Ya existe el camino:

- `Command::ReloadConfig` → `Inner::reload` (`crates/lirtup-core/src/engine.rs:1227`) vuelve a
  resolver la config de la ventana y manda `PipelineCommand::UpdateConfig { config, profile,
  mode }` (`engine.rs:1251-1262`, que se define en `crates/lirtup-pipeline/src/pipeline.rs:136-144`).
- El hilo de la sesión lo atiende en `Runner::handle_command` (`pipeline.rs:797-805`) →
  `Runner::update_config` (`pipeline.rs:1163-1233`), que compara la config vieja con la nueva y
  rehace solo lo que cambió.

Un modo es exactamente `ModeSettings` / `MODE_KEYS` (`crates/lirtup-config/src/model.rs:422`,
`:545`): modelo, `double_upscale`, escaladores, deband, MagpieFX y efectos nativos. Ninguna de
esas claves toca la superposición ni la generación de cuadros. Lo que cuesta cada parte:

| Parte | ¿Cambia? | Qué pasa | Costo |
| --- | --- | --- | --- |
| Superposición y swapchain | No | Se recrean solo si cambian `overlay_mode`, `monitor` o `vulkan_present_mode` (`pipeline.rs:1176-1191`), que no son claves de modo | — |
| Captura | No | Solo si cambian el recorte o los mosaicos (`pipeline.rs:1167-1169`) | — |
| Upscaler SRCNN | Si cambian `model` o `double_upscale` | `recreate_upscaler` (`pipeline.rs:1150-1161`): `finish()` del anterior y `Upscaler::new` (`upscale.rs:71-97`). Los shaders nativos van embebidos en SPIR-V (`lirtup-effects/src/assets.rs:10`): **se crean pipelines, no se compila nada** | Unos ms, una vez |
| Efectos antes del escalado (deband, nativos, MagpieFX) | Sí | `Presenter::update_config` (`presenter.rs:683-732`). Si es la misma cadena con otros valores, actualiza en el lugar (`update_native`, `update_magpie_params` en `:736-760`, con `set_params` y sin recompilar). Si no, marca `pre_stale` y la cadena se rehace en el próximo cuadro (`prepare` → `build_pre`, `presenter.rs:882-951`) | Nativos: barato. MagpieFX: ver abajo |
| MagpieFX | Si el modo nuevo tiene otros efectos | `build_magpie` → `MagpieContext::compile` (`presenter.rs:207-254`, `:953`): memoria de la sesión → `SpirvCache` en disco → **subproceso dxc/glslang, síncrono en el hilo de la sesión** | Si el efecto está en la caché, unos ms. Si no, **segundos con el juego congelado** |
| Escalador final y efectos lineales | Si cambian el escalador o sus parámetros | `self.scale = None` y se rehace en `prepare` (`presenter.rs:712-721`) | Barato |
| LUT y grano | Sí | `build_screen_passes` (`presenter.rs:724-726`) | Barato |
| Generación de cuadros | **No** | `FrameGen::update_config` (`framegen.rs:532-591`) compara sus `Settings` y vuelve sin hacer nada si son iguales (`:563`). Las claves `frame_gen_*` no son de modo, y la GUI las trata como globales (`lirtup-ui/src/state/store.rs:162`) | — |
| Capa lsfg-vk | No | Solo `Restart` con `exec` (`engine.rs:1172`) | — |

**Un antecedente en el propio código.** `HotkeyAction::CycleModel` ya cambia el modelo en vivo con
`recreate_upscaler` y un OSD (`pipeline.rs:1246-1251`). Cambiar de modo es lo mismo con más
campos.

**Detalles que hay que cuidar:**

- **El archivo de un modo puede traer claves que no son de modo.** El resolvedor aplica el archivo
  crudo menos `name` y `description` (`resolve.rs:104-116`). Un `modes/x.yaml` editado a mano con
  `overlay_mode` dispararía la reconstrucción completa. Es raro y no rompe nada, pero se ve un
  parpadeo.
- **MagpieFX sin caché.** Recorrer modos con efectos que nunca se compilaron congela el juego
  mientras compilan. Hay dos mitigaciones:
  - avisar con el OSD («Compilando efectos…») antes de rehacer la cadena;
  - o, mejor, compilar en otro hilo y cambiar la cadena cuando termine.

  La GUI ya precompila con `CompileEffect`, y la caché de disco es la misma.
- **Clics seguidos.** Cada clic rehace la cadena. Conviene esperar unos 300 ms desde el último
  clic para aplicar: la etiqueta cambia enseguida y la cadena se rehace una vez.

## 2. Cambiar el perfil en caliente

**Técnicamente se puede: `follow_focus` ya lo hace.** `Runner::switch_window`
(`pipeline.rs:1047-1095`) resuelve el perfil de la ventana nueva y, si cambia, pone
`self.profile`, `self.mode` y `update_config` + `rebuild_capture` (`:1063-1078`). La bandeja ofrece
otro camino: «Perfil ▸» manda `StartScaling` sobre la misma ventana con el perfil elegido, es
decir, **reinicia la sesión** (`lirtup-ui/src/tray.rs:631-645`).

Qué arrastra un cambio de perfil a mitad de sesión con `UpdateConfig`:

| Cosa que el perfil puede cambiar | Qué pasa en vivo |
| --- | --- |
| `overlay_mode`, `monitor`, `vulkan_present_mode` | `teardown` y `build` completos: ventana de superposición nueva y swapchain nuevo (`pipeline.rs:1176-1191`). Parpadea, y en `top-transparent` cambia el ruteo del puntero |
| Recorte, `auto_crop_menu_bar` | Captura nueva. El recorte de barra de menú detectado (`menu_crop`) sigue el del perfil viejo, salvo que se reinicie como en `switch_window` (`:1052-1056`) |
| Generación de cuadros | Las claves son globales (`store.rs:162`): en la práctica no cambian. Con la capa, `layer_needs_exec` pide `Restart` |
| `gpu` | El dispositivo se abre una vez y queda hasta el cierre (`engine.rs`, `device()`): **se ignora sin aviso** |
| Atajos | Se toman del nivel de arriba (más `-p`), no del perfil de la sesión (`engine.rs:59-61`): el cambio no hace nada |
| `match` y ventana objetivo | No cambian la ventana actual. El daemon puede volver a elegir otro perfil después |
| Estado del motor | `Running.profile` (`engine.rs:183-188`) y `last_start` (`:198`) siguen con el perfil viejo. **El próximo `ReloadConfig` (`engine.rs:1251-1256`) vuelve a resolver con el perfil viejo y deshace el cambio**, y `Restart` también vuelve al viejo. Hay que actualizarlos |
| La tarjeta de la GUI | Ver la sección 3: hoy no se entera |

**Conclusión:** el cambio de perfil es posible, pero toca el motor, la superposición y la GUI.
Además, «perfil» mezcla la ventana con la configuración, y ya existe el camino seguro de la
bandeja, que reinicia la sesión. En la barra no lo pondría.

## 3. Protocolo

**Hoy no hay nada para «cambiar el modo» desde afuera.** `Command` (`crates/lirtup-proto/src/lib.rs:55-139`)
solo tiene `ReloadConfig` (relee el disco), `Start { profile }` (reinicia) y `Hotkey`.
`Event::SaveSetting { profile, key, value }` (`lib.rs:195-202`) ya sirve para guardar: la GUI
escribe `key` en la capa del perfil (`store.rs:1006-1030`). `mode` no es global, así que va al
perfil, y después la GUI manda `ReloadConfig`.

**Para el flujo recomendado no hace falta ningún comando nuevo.** El selector vive dentro del
proceso, en la barra y en el hilo de la sesión, y no cruza el socket. Lo que sí hace falta:

1. **Que la GUI vea el cambio temporal.** La GUI solo se entera de la vista de la sesión por
   `SessionStarted` y por `Snapshot` (`store.rs:906`). `publish_view` solo actualiza el `ArcSwap`
   (`pipeline.rs:1834-1838`). **Hoy la tarjeta ya queda vieja** después de un `ReloadConfig` o de
   un `follow_focus` que cambia el modo o la cadena. Propongo:
   - volver a mandar `SessionStarted(view)` después de cada `update_config`, como ya hace «A
     ventana» (`pipeline.rs:1441`; la GUI lo toma como una actualización);
   - sumar a `SessionView` (`proto/src/types.rs:56-73`) un campo
     `#[serde(default)] mode_temporary: bool`, o `profile_mode: Option<String>`, el modo que nombra
     el perfil, para que la GUI muestre «Modo: X (temporal)».
2. **Opcional:** `Command::SetMode { mode: Option<String> }` para que la bandeja, la GUI o
   `lirtup ctl` también puedan cambiar el modo temporal. `None` vuelve al del perfil.

**Compatibilidad con `PROTOCOL_VERSION` 1** (`lib.rs:50`): **no hace falta subirla.**

- Un campo nuevo con `#[serde(default)]`:
  - el cliente viejo lo ignora, porque ningún tipo usa `deny_unknown_fields`;
  - el cliente nuevo frente a un `serve` viejo lee `false`.
- Una variante nueva de `Command`: un `serve` viejo no la entiende y contesta con
  `Event::Log { Warn, "bad message: …" }` sin cortar la conexión (`lirtup-core/src/server.rs:312-328`,
  `:390-393`).
- Una variante nueva de `Event`: la GUI vieja la descarta con un `warn` (`lirtup-ui/src/link.rs:168-178`).

En todos los casos la función nueva no anda contra una versión vieja, pero nada se rompe. La GUI y
`serve` salen en el mismo paquete, y la diferencia de versión ya se avisa al conectar.

## 4. La barra

**Cómo está hecha** (`crates/lirtup-pipeline/src/toolbar.rs`, notas en `rust-r4-toolbar.md`):

- **Los ítems.** Una fila de ítems (`ItemKind`: `Button`, `Divider`, `Multiplier(String)` y
  `Readout`, en `toolbar.rs:186-195`) que `layout()` acomoda de izquierda a derecha
  (`:471-529`). El ancho tiene un tope de `MAX_W` = 900 px de diseño (`:177`).
- **El dibujo.** La escena se rasteriza en la CPU con `UiDraw` en una imagen del tamaño de la
  barra. El búfer de subida (`max_image`, `:722`) mide la barra más el margen de arriba.
- **La entrada.**
  - `ToolbarRouter` (`lirtup-x11/src/overlay_input.rs`) manda a la barra lo que pasa dentro de
    **un solo rectángulo**: `ToolbarZone { bar: Option<Rect>, reveal }` (`overlay_input.rs:22-27`).
  - En `top-transparent`, ese rectángulo también es la región de entrada de XFixes.
  - `Toolbar::pointer` (`toolbar.rs:337-397`) usa clic izquierdo, hover y suelta. El router **ya
    manda la rueda y los otros botones** cuando están sobre la barra (`overlay_input.rs:140-142`),
    pero `pointer` los ignora.
- **Las acciones.** `ToolbarAction` (`toolbar.rs:71-99`) → `Runner::toolbar_action`
  (`pipeline.rs:1350-1379`).
- **El precedente.** El chip del multiplicador (`Multiplier`) es justo el patrón de «botón que
  recorre valores»: se cambia en vivo, va al OSD y se guarda con `SaveSetting`
  (`pipeline.rs:1399-1426`).

**Botón que recorre la lista (elegido):** un chip `ItemKind::Mode(String)` como el del
multiplicador.

- **Cómo se usa.**
  - Clic izquierdo: el modo siguiente.
  - Clic derecho o rueda hacia arriba: el anterior.
  - Rueda hacia abajo: el siguiente.

  No hacen falta zonas nuevas: es un ítem más dentro de la barra, con su `item_at`.
- **«← nombre →» con dos flechas** suma dos sub-zonas y dos íconos. Es posible y barato, pero no
  aporta mucho frente al clic derecho y la rueda.
- **Íconos.** Las instancias `.ttf` de Material Symbols de la barra solo traen los íconos de
  `ICON_NAMES` (`toolbar.rs:56`). Un ícono nuevo (`chevron_left`, `save`, `style`/`palette`) obliga
  a correr `scripts/gen-tokens-icons.py`, que necesita `fontTools` y bajar la fuente de GitHub.
  `palette`, `tune` y `save` ya están en `design/icons.json` de la GUI. Se puede evitar con
  `auto_awesome`/`animation` u otro ícono que ya esté.
- **Nombres largos.** Hay que recortarlos con «…» según `measure` (máximo unos 160 px de diseño),
  para no pasar `MAX_W`.
- **La etiqueta del chip.** Cambia el ancho, así que provoca `layout()`, igual que
  `multiplier_label` (`toolbar.rs:426-431`).

**Desplegable (lista bajo la barra):** bastante más caro y frágil.

- La lista queda **fuera del rectángulo de la barra**. Hay que pasar `ToolbarZone` a varios
  rectángulos: el router, la región XFixes en `top-transparent` y el hilo de la superposición en
  `surface.rs`. Es el mismo pendiente de r4: «el panel no se arrastra».
- Agrandar la imagen y el búfer de `UiDraw`, o usar un segundo `UiDraw`.
- Cerrar la lista al hacer clic afuera. Hoy un clic afuera va al juego, y en `top-transparent` ni
  siquiera llega.
- Desplazarse si hay muchos modos, con la rueda y sin teclado.
- Que no se oculte a los 2 s (`HIDE_AFTER`) con la lista abierta.
- Calculo de 2 a 3 veces el trabajo del chip, con más casos de puntero real que hoy no se pueden
  automatizar (`xdotool` no llega a XWayland, según la nota r4).

**Conclusión: el chip que recorre la lista es más simple y más robusto.** Con 3 a 8 modos, que es
lo típico, alcanza de sobra.

## 5. Persistencia

**Hoy, cambiar el modo desde la sesión no escribe nada.**

- **El escalador nunca escribe YAML.** Es la regla del protocolo (`proto/src/lib.rs:23-26`), y la
  barra solo pide guardar con `SaveSetting`, que escribe la GUI.
- **`ModeStore::save` y `save_raw`** (`lirtup-config/src/mode.rs:93`, `:108`) escriben el
  **archivo del modo**, no el perfil, y solo los usa el editor de modos de la GUI. El selector no
  debe tocarlos: elegir un modo no es editarlo.
- **El perfil guarda el modo como `mode: <id>`** en `profile.options`, igual que hace
  `create_profile` (`store.rs:1045-1048`).
- **Hoy `save_setting` del hilo de la sesión siempre manda `profile: None`**
  (`pipeline.rs:1448-1454`). Para el modo hay que mandar `self.profile`. Sin perfil va a
  *Predeterminado*, es decir, al `mode` del nivel de arriba (`resolve.rs:102`), y eso afecta a
  todos los perfiles que no nombran un modo. La etiqueta tiene que decirlo: «Guardar en
  Predeterminado».
- **Sin GUI conectada, `SaveSetting` se pierde** («dura solo la sesión», `lib.rs:193-194`).

### Dónde se aplica el cambio temporal

El orden de las capas importa (`resolve.rs:83-141`):

- el modo queda **justo debajo** de la capa que lo nombra;
- por eso, los campos de modo sueltos del perfil le ganan (por ejemplo, un `cas_enabled` puesto en
  el perfil).

Si el cambio temporal se pasara como la capa de CLI (`cli["mode"]`), el modo quedaría **encima**
del perfil. Lo que se ve al probar no sería lo mismo que queda después de guardar. Por eso
propongo:

- `resolve` acepta un `mode_override: Option<&str>` que **reemplaza el id nombrado sin mover la
  capa**: queda en el perfil si el perfil nombraba uno, o en el nivel de arriba si no. Así lo que
  se prueba es exactamente lo que se guarda.
- **El cambio temporal vive en el motor** (`Running`, `engine.rs:183`). Así `reload()` lo
  conserva al volver a resolver, y `Restart` también.
- **El cambio temporal se borra solo** cuando el modo resuelto del perfil coincide con él. Eso
  pasa después de que la GUI guarda y manda `ReloadConfig`. Sirve de confirmación: el chip deja
  de verse como «temporal».

### Flujo propuesto

```text
clic en el chip ─► Runner: siguiente id de la lista de modos ─► espera de 300 ms ─► resolver(ventana, perfil, Some(id))
                 └─► OSD «Modo: Anime nítido (temporal)»          └─► update_config (en vivo) ─► SessionStarted(view, mode_temporary)

«Guardar» (en la barra o en la GUI) ─► SaveSetting { profile: Some(perfil), key: "mode", value: id }
     └─► la GUI escribe profile.options.mode ─► ReloadConfig ─► el motor resuelve: el modo del perfil == el temporal ─► borra el temporal
```

**Fin de sesión:** el cambio temporal se pierde. Es intencional, porque es «para probar».

**`follow_focus` hacia otra ventana con otro perfil:** el cambio temporal se descarta, porque es
del perfil anterior.

---

## Opciones de flujo

### A. Solo el modo, chip que recorre la lista, temporal + «Guardar» (recomendada)

- **Barra:** un chip «▣ Anime nítido» entre la lectura de fps y el multiplicador.
  - Clic: el siguiente. Clic derecho o rueda: el anterior o el siguiente.
  - Con un cambio temporal, el chip se pinta con otro tono (`tertiary` o un punto) y aparece al lado
    un botón chico «Guardar», que manda `SaveSetting`.
- **GUI:** en la tarjeta de sesión (`screens/home/session.rs:109-117`), «Modo: X · temporal» y el
  botón «Guardar en perfil» (o «Guardar en Predeterminado»), con el mismo `save_setting`.
- **Pros:**
  - Usa el `UpdateConfig` que ya existe y no recrea la superposición ni la generación de cuadros.
  - No toca el disco hasta que se pide.
  - No sube el protocolo.
  - Hereda el patrón del multiplicador.
- **Contras:**
  - «Guardar» desde la barra no hace nada sin la GUI abierta. Igual que con el multiplicador, el
    chip sigue marcado como temporal, y eso al menos es honesto.
  - MagpieFX sin caché congela el juego (ver los riesgos).

### B. Igual que A, pero se guarda en cada clic (como el multiplicador)

- **Pros:** el menor código, porque no hay cambio temporal en el motor ni campo nuevo en
  `SessionView`.
- **Contras:**
  - Cada prueba escribe el perfil y fuerza un `ReloadConfig` completo.
  - Ensucia el YAML y los avisos de la GUI (un aviso por `SaveSetting`, que la nota r4 ya marca
    como molesto).
  - Sin GUI no se guarda, y la sesión siguiente vuelve al modo viejo sin avisar.
  - Va en contra de «probar varios».

### C. Modo y perfil en la barra

- **Perfil, por reinicio** (como la bandeja): robusto, pero tarda (la superposición y la captura
  se rehacen) y parpadea.
- **Perfil, en vivo** (como `follow_focus`):
  - hay que mantener `Running.profile` y `last_start` (si no, `ReloadConfig` lo deshace);
  - se ignoran sin aviso `gpu` y los atajos;
  - el recorte de la barra de menú queda viejo;
  - la superposición se recrea si cambia `overlay_mode`.
- **Pros:** un solo lugar para todo.
- **Contras:**
  - Dos chips más largos (los nombres de perfil son títulos de juegos) contra el tope de 900 px.
  - Más casos raros.
  - Duplica lo que la bandeja ya hace.
  - El perfil casi siempre está atado a la ventana por `match`, así que cambiarlo dentro del mismo
    juego tiene poco sentido.

## Recomendación

**A, sin selector de perfil en la barra.** Confirma la propuesta, con estos ajustes que salen del
código:

1. El cambio temporal **se resuelve en la capa del perfil** (`mode_override` en `resolve`), no
   como CLI, para que lo probado sea igual a lo guardado.
2. El cambio temporal **vive en el motor**, para que `ReloadConfig` y `Restart` no lo pierdan, y
   **se borra solo** cuando el perfil guardado ya lo tiene.
3. **Hay que volver a mandar `SessionStarted`** después de cada `update_config`. Hace falta para
   mostrar el cambio temporal en la GUI, y de paso corrige que la tarjeta quede vieja tras
   `ReloadConfig` o `follow_focus`.
4. **Esperar unos 300 ms** antes de aplicar, y avisar en el OSD si hay que compilar MagpieFX.
5. **Ícono:** uno que ya esté en las `.ttf` de la barra, o regenerar con
   `gen-tokens-icons.py` (red y `fontTools`).

Si se quiere algo para el perfil, que sea `Command::SetMode` para que la bandeja tenga también
«Modo ▸» junto a «Perfil ▸». Es barato, y no hace falta para la 2.1.0.

## Estimación (opción A)

| Archivo | Cambio | Tamaño aprox. |
| --- | --- | --- |
| `lirtup-config/src/resolve.rs` | `mode_override` (o `resolve_with_mode`) que reemplaza el id en su capa, con tests de capas | +40 código, +60 tests |
| `lirtup-core/src/engine.rs` | `Running.mode_override`; `resolve_for` con override; `reload` lo conserva y lo borra cuando coincide; el `Resolver` de `SessionSetup` acepta el modo y da la lista `(id, nombre)` de `ModeStore` | +70 |
| `lirtup-pipeline/src/pipeline.rs` | `SessionSetup` (la lista de modos y el resolvedor con modo); `ToolbarAction::ModeNext/ModePrev/SaveMode`; la espera; el OSD; `SessionStarted` después de `update_config`; `save_setting` con perfil | +120 |
| `lirtup-pipeline/src/toolbar.rs` | `ItemKind::Mode`, el estado (`mode_label`, `mode_temporary`), el botón «Guardar», la rueda y el clic derecho en `pointer`, `layout`, `shapes`, tests | +150 |
| `lirtup-proto/src/types.rs` | `SessionView.mode_temporary` con `serde(default)` | +5 |
| `lirtup-ui/src/screens/home/session.rs`, `state/store.rs`, `i18n/{es,en}/home.ftl` | «temporal» y «Guardar en perfil» en la tarjeta; test de store | +60 |
| Íconos (opcional) | Regenerar las `.ttf` de la barra | binarios |
| `docs/user/`, `CHANGELOG.md`, metainfo | La barra: el chip de modo | +30 |

**Total: unas 500 a 600 líneas con tests, en 8 a 10 archivos.** Son 1 o 2 días con la prueba a
mano. El riesgo principal está en la prueba con el puntero real.

## Riesgos

- **MagpieFX sin caché:** compila en el hilo de la sesión (`presenter.rs:953`, `:207-254`) y
  congela segundos. Hay que avisar por el OSD o compilar fuera del hilo.
- **El modo del perfil vs. los campos sueltos:** si el cambio temporal se resolviera en otra capa,
  lo probado no coincidiría con lo guardado (`resolve.rs:83-141`).
- **«Guardar» sin GUI:** `SaveSetting` se pierde (`lib.rs:193-194`). El chip tiene que seguir
  marcado como temporal y no mentir.
- **Sin perfil, «Guardar» escribe el `mode` del nivel de arriba,** que afecta a todos los perfiles
  que no nombran modo. Hay que decirlo en la etiqueta.
- **Modo nombrado por la CLI** (`lirtup run` con `mode`): la CLI gana (`resolve.rs:97-99`).
  Guardar en el perfil no cambia lo que se ve. Hay que avisar o deshabilitar.
- **Modos que cambian con la sesión en marcha:** si la GUI crea o borra modos, hace falta que la
  lista del chip se lea del motor en cada clic, no una vez al empezar. Si se borra el modo
  temporal, se vuelve al del perfil.
- **Archivos de modo con claves que no son de modo:** provocan una reconstrucción completa y un
  parpadeo (`resolve.rs:104-116`, `pipeline.rs:1176`).
- **Ancho de la barra:** un chip más se acerca al tope de 900 px con el multiplicador y la lectura
  «888 → 888 fps». Hay que recortar el nombre.
- **El puntero real:** el clic derecho y la rueda sobre la barra en `top-transparent` (región
  XFixes) y en modo ventana no se pueden probar con `xdotool`. Hay que sumarlos al guion de
  `rust-r4-toolbar.md`.
