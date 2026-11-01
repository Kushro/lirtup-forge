# Ronda 3: integración de la GUI

Rama **`rust/r3-integration`**, que sale de `rust/r3-skeleton`. Fecha: 2026-11-01. Sin push y sin
merge a `main`.

| Commit | Qué |
| --- | --- |
| `5d02e8b0` | Merge de A (`claude/rust-r3-components-61549a`) |
| `c5f67499` | Merge de C (`claude/rust-r3-profiles-8f029e`) |
| `d3f73629` | Merge de B (`claude/rust-r3-home-953724`) |
| `b873ce27` | Merge de D (`claude/rust-r3-modes-e64822`) |
| `76245ed4` | Íconos, tokens y `tailwind.css` regenerados; `scripts/check-ftl-keys.py` |
| `b5e45141` | Pendientes del núcleo |
| `0b006ad6`, `74949bcd` | La pasada de correcciones |

Los cuatro merges entraron sin conflictos. Notas de los frentes:
[rust-r3-components.md](rust-r3-components.md), [rust-r3-profiles.md](rust-r3-profiles.md),
[rust-r3-home.md](rust-r3-home.md), [rust-r3-modes.md](rust-r3-modes.md). El esqueleto está en
[rust-r3-skeleton.md](rust-r3-skeleton.md).

Todo compiló con `CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r3-integration`. Borré el
`target/` de esta worktree en DATOS, que era mi propia caché de compilación (14 GB).

## Revisión de las ramas

Cada frente tocó solo sus archivos y su nota, como dice la tabla de 5.5. Ninguno pidió
dependencias nuevas, de Rust ni de npm. C agregó a `state/mod.rs` `Store::saving`, `Saving` y
`Action::SaveAndRestart`, solo agregando, como permitía la tabla. A agregó props con valor por
defecto (`Tooltip.place`, `IconButton.tip`, `SettingRow.detail`) y algunos componentes y
funciones públicas nuevos (`ProfileIcon`, `format_number` y otros). Todo eso está en su nota.

## Integración

- **Íconos, tokens y CSS**, en el orden de la nota de A:
  1. `scripts/gen-tokens`: 171 íconos en el subconjunto de Material Symbols, `tokens.css` y
     `tokens.rs`; `contrast.py` da 0 fallos de 54 en cada tema y también pasa el cristal sobre
     negro y blanco;
  2. `assets/tailwind.css` con Tailwind 4.3.3 (`npm run tailwind:build`).

  `dx` trae su propio Tailwind 4.1.5 y regenera `assets/tailwind.css` sin minificar en cada
  build. Lo devuelvo a la versión de git después de cada `dx build`.
- **Traducciones.** No hay un script que pase las cadenas de los `.ts`: es `scripts/ts-to-ftl*`
  del frente M, en la ronda 4. En su lugar escribí **`scripts/check-ftl-keys.py`**, que comprueba
  tres cosas:
  - que cada clave que nombra el código exista en español y en inglés;
  - que los dos idiomas tengan las mismas claves;
  - que no haya claves repetidas.

  Resultado: 568 claves en cada idioma, 538 usadas en el código, ninguna falta. Los `.ftl` son uno
  por frente, así que juntarlos no tuvo conflictos.

## Pendientes del núcleo (`b5e45141`)

| Pendiente | Qué hice |
| --- | --- |
| `SetTarget` y `HideCursorAfter` en P | `switch_window`, que usan `SwitchWindow` y `follow_focus`, ya no rehace la superposición, el swapchain ni el presentador: manda `OverlayCommand::SetTarget` y rehace solo la captura y el escalador; en modo ventana también ajusta el tamaño de la superposición. `hide_cursor` va como `HideCursorAfter(ms)` al crear la superposición y cuando cambia la configuración. Antes escondía el cursor para siempre |
| `StartFullscreen` toma la GUI o una superposición vieja | `X11::is_lirtup_window` (`_LIRTUP_OVERLAY` o un `WM_CLASS` `lirtup*`; `WindowInfo::is_lirtup_class`). Si `Target::Focused` es LirtUp Forge, `SessionError` explica que hay que enfocar la ventana a escalar. `windows.rs` (B) usa la misma función |
| `bench` con modos, `sizes` y `selectors` | `build_cases` ya aceptaba un modo. `save_results` ahora recibe las medidas y el selector: guarda `sizes.<tipo>` como la 1.x (sin `input` para FG; un tipo medido con otras medidas empieza de cero) y `selectors[gpu o ""]`. Lo llama la CLI, también cuando lo lanza la GUI. Test nuevo |
| Hallazgo 3: «render» suma esperas | `PresentOutcome.waited` mide la espera del ciclo anterior de FG (`FrameGen::take_waited`) o del present bloqueante. «render» ya no la incluye, y va aparte en `StatsSample.wait_ms` (`serde(default)`). El panel muestra «render 1.2 ms (+12.5 wait)» |
| Hallazgo 1: captura de Vulkan en XWayland | **Probado contra la 1.x: las dos dan negro.** `vkcube --wsi xcb` (500×500): con el `FrameGrabber` de la 1.3.0, 0 píxeles distintos de negro de 250 000; con `capture_png` de la 2.0, también 0. Es una limitación de KWin con XWayland, no una regresión. Va al README en la ronda 4 (frente P) |

## Build 1

| Paso | Resultado |
| --- | --- |
| `cargo build --workspace --release` | 1 aviso: `key` sin usar en `screens/profile/mod.rs` |
| `cargo clippy --workspace --all-targets` | 0 avisos |
| `cargo test --workspace` | 389 pasan, 0 fallan, 4 ignorados |
| Ignorados (`-- --ignored`) | Paridad de E y lsfg con fds: pasan. `real_config_round_trip` falla sin `LIRTUP_UI_CONFIG` y, con `config-example.yaml`, falla porque compara el texto |
| `dx build --release -p lirtup-ui` | Compila y empaqueta; sin avisos de Tailwind. El mismo aviso de `key` |

## Prueba en vivo (antes de la pasada de correcciones)

La GUI de `dx build`, contra un `lirtup serve` de prueba:
- `XDG_CONFIG_HOME` en mi scratchpad, con `config-example.yaml`, un perfil BALDR SKY y un modo
  «Anime nítido»; `XDG_RUNTIME_DIR` corto para el socket;
- `GDK_BACKEND=x11` y dos ventanas X11 de prueba.

Las teclas y el ratón que inyecta `xdotool` no llegan a XWayland bajo KWin. Por eso agregué a
`app.rs` tres ayudas de prueba:
- `LIRTUP_GUI_ROUTE`: la página de entrada;
- `LIRTUP_GUI_THEME`: el tema;
- `LIRTUP_GUI_EVAL`: un `.js` que se ejecuta en la página. Sus clics sobre el DOM llegan a los
  manejadores de Dioxus.

Con eso recorrí los flujos y saqué las capturas con `import -window`.

## La pasada de correcciones (`0b006ad6`, `74949bcd`)

| Problema | Dónde | Corrección |
| --- | --- | --- |
| **La sesión se pasaba a la GUI.** Con `follow_focus`, al escalar desde la GUI la sesión pasaba a escalar la ventana de LirtUp Forge (encontrado en el flujo 1) | `pipeline.rs` | `check_focus` ignora las ventanas de LirtUp Forge: ni las toma ni pausa por ellas |
| **Guardar un modo lo escribía entero.** `ModeStore::save` serializa todo `Mode`: un archivo de 6 claves salía con 60. El modo pasaba a fijar valores que antes heredaba de Predeterminado (encontrado en el flujo 4) | `lirtup-config`, `state` | `ModeStore::save_raw` escribe la capa tal cual. `Effect::WriteMode` lleva `raw`: el archivo leído más las ediciones (crear, duplicar y guardar). Test nuevo |
| «Cambiado en este perfil» en Predeterminado, en los modos y en Atajos | `components/rows.rs`, `app.rs` | `RowLayer` (perfil, Predeterminado, modo), que `app.rs` saca de la ruta con un `Memo` y que `SettingRow` lee del contexto. Hay 3 claves nuevas: «Valor del sistema», «Cambiado en Predeterminado» y «Cambiado en este modo». Lo habían pedido B y D |
| Las pestañas no se volvían a montar en release | `screens/profile/mod.rs` | `rsx` solo respeta `key` dentro de una lista: va en una lista de uno |
| `has_key` daba `false` con variables (lo vieron A, B, C y D) | `i18n.rs` | Distingue el error de formato del de mensaje ausente |
| `real_config_round_trip` comparaba el texto | `state/store.rs` | Compara el YAML leído, sin los comentarios ni los atajos iguales a los por defecto, que la 1.x tampoco guarda. El texto se compara solo si el archivo no tiene comentarios |
| Pedidos sueltos de las notas | `app.rs`, `main.rs` | Tooltips de la barra de título a la derecha (A); «Quitar todos los ajustes del perfil» en un perfil con nombre (C); `with_disable_drag_drop_handler(true)` para el arrastre de la cadena (D) |
| clippy `large_enum_variant` en `Effect` | `state/mod.rs` | `allow`, con el motivo: los efectos duran un despacho |

## Build 2

| Paso | Resultado |
| --- | --- |
| `cargo build --workspace --release` | **0 avisos** |
| `cargo clippy --workspace --all-targets` | **0 avisos** |
| `cargo test --workspace` | **389 pasan**, 0 fallan, 4 ignorados |
| Ignorados, con `LIRTUP_UI_CONFIG=config-example.yaml` | **Los 4 pasan**: paridad nativa y MagpieFX con Python, lsfg con fds, y el perfil real |
| `dx build --release -p lirtup-ui` | **Sin avisos**; sin advertencias de Tailwind |

## Criterios de 5.8 (ronda 3)

| Criterio | Estado |
| --- | --- |
| `dx build` sin advertencias de Tailwind | **Cumplido** |
| Los 6 flujos de 6.1 contra `lirtup serve` | **Cumplidos con clics por script; falta el teclado, que va al guion.** Uno por uno: |
| — 1. Escalar una ventana | Clic en el mosaico: la sesión arranca con el perfil que coincide (BALDR SKY) y su modo. La tarjeta ámbar muestra la ventana, el modo, `640×400 → 2560×1440` y los fps, con ▶ en el cajón. Detener la cierra |
| — 2. Crear un perfil | Nuevo perfil → Capturar ventana, que llena el nombre y las reglas de título, ancho y alto → Crear perfil: queda en el YAML, se abre su página y sale el snackbar |
| — 3. Editar y guardar | Geometría en Estirar: la fila se pone ámbar con «antes: Ajustar» y aparecen el punto de la pestaña, el punto del cajón y la barra con «1 cambio sin guardar». Guardar escribe solo `output_geometry: stretch` (diff del YAML) y el snackbar ofrece Deshacer |
| — 4. Modo y efectos | Agregar efecto (catálogo de 117, costos de `bench.json`, filtros) → buscar FSR_EASU → Agregar → Guardar: queda al final de la cadena, ahora sin tocar el resto del archivo |
| — 5. Generación de cuadros | En Presentación de Predeterminado: «Motor listo», ×2 queda marcado sin guardar y con su insignia Global. Guardar y reiniciar con una sesión activa está cubierto por un test de C, no lo probé en vivo |
| — 6. Daemon | El interruptor queda encendido cuando llega `DaemonState`, con «Vigilando 4 perfiles». La bandeja es de la ronda 4 |
| Un perfil de la 1.x se abre y se guarda sin perder campos | **Cumplido** con `config-example.yaml` (5 perfiles), con la configuración real de la persona y con sus copias de respaldo (una copia: no tienen perfiles). En vivo, Guardar cambió solo la clave editada. Los comentarios se pierden, igual que en la 1.x |
| Herencia y sucio en fila, pestaña y cajón | **Cumplido** (flujo 3 y capturas), con la nota de capa correcta después de la corrección |
| Tests de `update()` en verde | **Cumplido**: todos los de `state::store` |
| Ningún texto sin traducir en español | **Cumplido** en las claves (`check-ftl-keys.py`) y en las 28 capturas. No vi texto en inglés en las pantallas |
| `contrast.py` en verde | **Cumplido**: 0 de 54 en cada tema |
| Navegable con teclado | **No verificado aquí**: no puedo mandar teclas a XWayland. Va al guion |

## Capturas

Las 28 capturas finales, de la build de Build 2, están en:

```
/tmp/claude-1000/-media-octavio-DATOS-Gitbutler-Repos-lirtup-forge--claude-worktrees-rust-r0-contract-6aa817/d0b0160b-efc7-4353-b51e-dafc735acceb/scratchpad/final/
```

Son 14 pantallas en dos temas, con nombres como `home-dark.png`, `home-grid-light.png`,
`modes_anime-dark.png`, `profile_BALDR_SKY_presentation-light.png`, `settings_hotkeys-dark.png`
o `gallery-light.png`. Las de los flujos (antes de la corrección) están en `…/scratchpad/flows/`.
Están en `/tmp`: se pierden al reiniciar.

## Pasa a la ronda 4

| Pendiente | Para |
| --- | --- |
| Hallazgo 1: Vulkan da negro en XWayland con KWin, igual que en la 1.x. Documentarlo en el README | P |
| Hallazgos 2 y 4: ciclos de FG medidos con el reloj de la CPU y latencia media de FG integrado | T |
| `PauseHotkeys { on }`: con `lirtup serve` corriendo, `HotkeyCapture` no puede capturar una combinación que agarra `GrabKey` (lo vio A) | Y, núcleo |
| Ícono propio del perfil en el cajón. La cabecera ya lo muestra; el cajón se redibuja con cada `Stats`, así que necesita una caché o un `use_asset_handler` | Revisor o Y |
| Deshacer restaura solo `config.yaml`, no los modos del mismo guardado | C, o quien toque el estado |
| `Route::Modes { from: Option<ProfileRef> }` para «Duplicar para este perfil» (D y C) | Revisor |
| `gui.start_delay_s` para recordar la demora del escalado diferido (B) | M |
| `Action::Toast` para avisar «Copiado» desde el menú del mosaico (B) | Revisor |
| Campos de la bandeja en `GuiConfig` («Mostrar ícono», «Minimizar a la bandeja») (D) | Y |
| Fijar la versión de Tailwind de `dx` (4.1.5) o pasar el CSS solo por `package.json` (4.3.3) | P (CI) |
| El test de traducciones completas (`has_key` ya sirve con variables) y la migración `.ts` → `.ftl` | M |

## Guion de pruebas a mano

Preparación:

```bash
lirtup serve &
```

```bash
GDK_BACKEND=x11 ~/.cache/lirtup-forge/target-r3-integration/dx/lirtup-gui/release/linux/app/lirtup-gui
```

Abrir antes un juego o `glxgears`. La configuración real queda intacta si se exporta un
`XDG_CONFIG_HOME` de prueba.

1. **Teclado en el marco**:
   - Tab recorre la barra de título, el cajón y la página, siempre con el anillo de foco de 3 px;
   - Ctrl+S guarda y Ctrl+N abre Nuevo perfil;
   - Ctrl+D duplica el perfil abierto;
   - Ctrl+F vuelve a Inicio con el foco en el filtro;
   - Ctrl+Shift+G abre la galería.
2. **Inicio**:
   - las flechas, Inicio y Fin se mueven por los mosaicos e Intro escala;
   - la tecla Menú o Shift+F10 abre el menú del mosaico;
   - Alt+Shift+A sobre el juego escala y vuelve a pulsarlo detiene;
   - con la GUI enfocada no escala la GUI: aparece el error «enfoque la ventana».
3. **Diálogos**: el foco queda atrapado (Tab da la vuelta), Esc y el fondo cierran, y el foco vuelve
   a quien abrió el diálogo. Probar Nuevo perfil, Cambios sin guardar, Agregar efecto y Medir.
4. **Cambios sin guardar**: cambiar algo en un perfil y pulsar otro perfil en el cajón. Probar las
   tres opciones: Guardar y cambiar, Descartar y cambiar, Cancelar.
5. **Restablecer**: probar las tres opciones del menú, más «Volver a heredar» en una fila.
6. **Modos**:
   - reordenar la cadena arrastrando (ahora sin el manejador de wry) y con las flechas de la
     manija;
   - Medir en mi GPU: progreso, Cancelar y resultado; `bench.json` tiene `sizes` y `selectors`;
   - con una sesión activa con ese modo, Guardar aplica el cambio en vivo.
7. **Generación de cuadros**: con una sesión activa, cambiar ×2 → ×3 y «Guardar y reiniciar».
   La sesión vuelve con el multiplicador nuevo.
8. **Atajos (Ajustes)**: Cambiar atajo → Ctrl+Alt+K. Hoy no se puede capturar una combinación que
   ya agarra el escalador (pendiente `PauseHotkeys`).
9. **`follow_focus`**: con un perfil que lo tenga, escalar y pasar el foco a otro juego: el
   escalado lo sigue sin rehacer la ventana (no hay parpadeo). Pasar el foco a la GUI: sigue con
   el juego.
10. **`hide_cursor`** (por ejemplo, 2000): el cursor desaparece a los 2 s sin moverlo y vuelve al
    moverlo.
11. **Panel de stats** con FG integrado: la fila «Shown» dice «render X ms (+Y wait)».
12. **Idioma y tema**: en Ajustes, cambiar a English y a Claro sin reiniciar; las capturas no
    muestran texto mezclado.
