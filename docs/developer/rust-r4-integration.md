# Ronda 4: integración final (2.0.0)

Rama **`rust/r4-integration`**, que sale de `rust/r4-skeleton`. Fecha: 2026-11-01. Sin push, sin
tags y sin merge a `main`.

| Commit | Qué |
| --- | --- |
| `3621f421`, `5e138afc`, `73e63ec4`, `b1e9fcfa` | Merges de M, T, Y y P |
| `99b1571b` | Python retirado y datos movidos a los crates |
| `c14a2a34`, `734ca441`, `58040b5f`, `a7f83bde` | Las filas de paridad del revisor (pipeline, CLI y GUI) |
| `f5414be6`, `eb70c398` | La pasada de correcciones |
| `e86d44ae` | Versión `2.0.0` y `CHANGELOG.md` |
| (este) | Esta nota |

Los cuatro merges entraron sin conflictos. Notas de los frentes:
[rust-r4-migrate.md](rust-r4-migrate.md), [rust-r4-toolbar.md](rust-r4-toolbar.md),
[rust-r4-tray.md](rust-r4-tray.md) y [rust-r4-packaging.md](rust-r4-packaging.md). El
esqueleto, con la lista de paridad original, está en [rust-r4-skeleton.md](rust-r4-skeleton.md).

Compilé con `CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r4`.

## Revisión de las ramas

- **M**: tocó solo sus archivos.
  - **La migración funciona con la configuración real.** En una copia, `lirtup migrate` llevó los
    dos campos sueltos al modo «Predeterminado», el nivel superior pasó a nombrarlo y quedó
    `config.yaml.1x.bak`.
  - **Pendiente:** su diálogo no lo vio nadie en vivo; va al guion.
- **T**: además de sus archivos, tocó `MaterialSymbolsRounded-Fill{0,1}.ttf`, `design/icons.json`
  y `design/tokens.rs`. Son salidas de `scripts/gen-tokens-icons.py`, que en esta ronda era
  suyo, así que no chocan con nada.
  - **Firmas:** cambió varias del esqueleto, todas en sus archivos y anotadas en su nota.
  - **Barra:** suma 0,002 ms de GPU.
  - **Hallazgos 2 y 4:** no se reproducen. Los ciclos de FG quedan entre −3,6 % y +4,9 % de la
    1.3.0, y no hubo que tocar `framegen.rs`.
- **Y**: tocó cinco archivos fuera de su reparto:
  - `lirtup-cli/src/main.rs`: la bandera `--exit-with-clients`, `run -d` y `lifecycle::launch`;
  - `lirtup-ui/src/main.rs`: una sola instancia, la ventana creada oculta y la geometría;
  - `link.rs`: `--exit-with-clients` y `process_group(0)`;
  - `components/hotkey.rs`: la prop `on_capture`;
  - `screens/settings/hotkeys.rs`: `PauseHotkeys`.

  Todos los pedían sus filas de paridad. Son cambios mínimos y comentados, y los acepto. Los dos
  hallazgos de su prueba en vivo quedan documentados en su nota:
  - `muda` tiene un solo manejador de eventos, y dioxus lo pisa: escucha los dos;
  - con la ventana oculta, la GUI se congela si no se crea oculta.
- **P**: borró `build_wheels.yml`, agregó `rust.yml` y `release.yml`, y un `tailwindcss`
  envoltorio en `packaging/tools/`. `makepkg` con el PKGBUILD apuntando a la rama construye el
  paquete. No pudo armar la AppImage: faltan `appimagetool` y `patchelf`, y no instalé nada.

## Python retirado y datos en los crates (`99b1571b`)

Es la decisión de la persona. Antes de borrar, busqué todas las lecturas de `src/upscaler/` desde
Rust y las moví.

| Qué | De | A |
| --- | --- | --- |
| Shaders (fuentes y `.spv`) | `src/upscaler/shaders/` | `crates/lirtup-effects/shaders/` (`lirtup-gpu` toma `clear.spv` de ahí) |
| Modelos CuNNy | `src/upscaler/srcnn/cunny/` | `crates/lirtup-effects/models/cunny/` |
| 117 efectos de Magpie y `SwapRB.hlsl` | `src/upscaler/magpiefx/{effects,internal}` | `crates/lirtup-magpiefx/{effects,internal}` |
| C++ de lsfg | `src/upscaler/lsfg/{lib,third_party}` | `crates/lsfg-sys/vendor/`, sin `lsfg_module.cpp` (el puente de CPython) |

- Hice el movimiento con `git mv`, así que la historia sigue.
- Quedan dos herramientas de Python sueltas, que no importan el paquete: `converter.py` (la usa
  `cunny-to-hlsl.fish`) y `blue_noise.py` (con la que se hizo `blue_noise.bin`).
- Borré:
  - el resto de `src/`;
  - `setup.py`, `pyproject.toml`, `uv.lock` y `MANIFEST.in`;
  - `tests/` de Python;
  - `data/applications` (el `.desktop` de la 1.x);
  - `docs/api`;
  - los scripts que solo servían a la 1.x: `compile-translations.fish`,
    `desktop_integration.sh`, `generate_dependencies.fish` y los tres de MagpieFX en Python.
- Ajusté:
  - los `include_bytes!` y el `build.rs` de `lsfg-sys`;
  - los scripts de shaders;
  - `rust-parity-refs.py`, que ahora corre con el Python de la 1.3.0 de pipx;
  - `install-tree.sh`, que toma los efectos del crate;
  - `docs/index.md`.
- **`builtin_dir` en release ya no mira la carpeta donde se compiló**, como pidió P. Así
  `makepkg` deja de avisar por las referencias a `$srcdir`.
- La 1.x queda en la rama local **`py_historical_backup`** (`454d7962`), sin push.

## Builds

| Paso | Build 1 | Build 2 |
| --- | --- | --- |
| `cargo fmt --all --check` | — | Pasa |
| `cargo build --workspace --release` | 0 avisos | **0 avisos** |
| `cargo clippy --workspace --all-targets -- -D warnings`, como en el CI de P | **Falla**: 3 errores en código mío de esta integración (dos `if` anidados y un `Some(..).filter`) | **Pasa** |
| `cargo test --workspace` | 444 pasan, 0 fallan, 5 ignorados | **444 pasan**, 0 fallan, 5 ignorados |
| Ignorados, con `LIRTUP_UI_CONFIG=config-example.yaml` | Los 5 pasan: paridad nativa y MagpieFX con Python (leen los shaders desde su lugar nuevo), lsfg con fds, la barra en 4K y el perfil real | **Los 5 pasan** |
| `dx build --release -p lirtup-ui` | Compila; sin avisos de Tailwind | **Compila; sin avisos** |
| `makepkg -d` en `~/.cache/lirtup-forge/r4-makepkg`, con el PKGBUILD apuntando a la rama | `linux-rt-upscaler-2.0.0-1` (19 MB), con un aviso de referencias a `$srcdir` | **`linux-rt-upscaler-2.0.0-1`** (19 MB), compilado desde cero (`-C`). Sin instalar: `lirtup --version` da `2.0.0` y `lirtup list effects`, 117. El aviso de `$srcdir` queda solo por los metadatos de `dx` en `lirtup-gui` |
| `check-ftl-keys.py` | — | 606 claves en cada idioma, ninguna falta |

**El aviso de `$srcdir` de `makepkg`** venía de dos lugares:
- en `lirtup`, el `CARGO_MANIFEST_DIR` de `builtin_dir`, que quedaba en el binario aunque en
  release no se usara. Ahora solo se compila en debug (`eb70c398`);
- en `lirtup-gui`, los metadatos de los assets que mete `dx` (`absolute_source_path`). Son de
  `dx`, inofensivos, y quedan.

`dx` sigue regenerando `assets/tailwind.css` con su Tailwind 4.1.5 en cada build local; lo
devuelvo a la versión de git. El PKGBUILD y el CI usan el envoltorio de P, con la 4.3.3.

**El CI del PR falló** en «Committed tailwind.css is current». El `assets/tailwind.css` de git
no se había regenerado después de los merges de esta ronda, y le faltaba
`.\!hidden{display:none!important}`. Es un falso positivo de Tailwind, que lo toma del código Rust
`if !hidden` de `tray.rs` (Y). Es inofensivo. **Regla:** después de cada merge que toque
`crates/lirtup-ui/src`, regenerar con `npm run tailwind:build` (4.3.3), o con `dx build` y
`NO_DOWNLOADS=1 PATH=packaging/tools:$PATH`, que da el mismo archivo byte por byte. Nunca subir
el que deja `dx` sin el envoltorio (4.1.5). El paso del CI lo detecta.

## Lista de paridad con la 1.x: estado de cada fila

**Ninguna fila queda abierta.** Cada una está implementada o es una diferencia decidida y
documentada en `docs/user/`. Lo que necesita manos para confirmarse va al guion de abajo.

| # | Qué | Estado | Quién |
| --- | --- | --- | --- |
| 1 | Buscar la ventana del programa lanzado por PID y por clase | **Hecha** | Y |
| 2 | Terminar el programa lanzado | **Hecha** | Y |
| 3 | Forzar X11 en el programa lanzado | **Hecha**; `__GLX_VENDOR_LIBRARY_NAME` queda fuera a propósito | Y |
| 4 | Enfocar la ventana objetivo | **Hecha** | Y |
| 5 | Pausar y ocultar con el objetivo minimizado | **Hecha** | T |
| 6 | `pause_on_focus_loss` solo en always-on-top y top-transparent | **Hecha** | Revisor |
| 7 | `follow_focus` aplica el perfil de la ventana nueva | **Hecha**: la sesión recibe el `resolve_for` del motor | Revisor |
| 8 | `follow_focus` espera otra ventana si se cierra la suya | **Hecha** (superposición oculta hasta la próxima ventana con foco); con daemon, el daemon vuelve a buscar | Revisor, Y |
| 9 | Opacidad en top-transparent | **Hecha** | T |
| 10 | Mantener el hover | **Hecha** | T |
| 11 | Posición de la superposición en modo ventana | **Hecha** | T |
| 12 | `scale_factor` | **Distinto a propósito**: en la 1.x pasaba coordenadas lógicas de Qt a físicas. La 2.0 trabaja en píxeles físicos de X11, sin capa de Qt, y la clave no hace falta | Revisor (documentar en `configuration.md`, P) |
| 13 | `swapchain_debounce` | **Hecha** | Revisor |
| 14 | `magpie_profile` | **Hecha**: tiempos de cada pasada en el log cada 5 s, como la 1.x. `UPSCALER_EFFECT_TIMING` no se porta | Revisor |
| 15 | Capturas: `strftime` en hora local, título del momento, aviso después de guardar | **Hecha** | T |
| 16 | API de Python | **Distinto a propósito** (retirada con Python) | — |
| 17 | `swapchain_buffers` | **Cerrada**: la 1.x tampoco la expone. Queda fija en 2 | — |
| 18 | XTest | **Distinto a propósito**, salvo que un juego lo pida (guion) | — |
| 19 | Programa como posicional | **Hecha** (`lirtup run prog args`, y también con `--`) | Revisor |
| 20 | Orden de elección del objetivo | **Hecha** (título o regex, después `--select`, después el programa) | Revisor |
| 21 | `--select` como la 1.x | **Hecha** | Revisor |
| 22 | `run -d` | **Hecha** | Y |
| 23 | `--log-file` y `log_level` | **Hecha** (archivo en DEBUG con hora) | Revisor |
| 24 | Códigos de salida | **Hecha**: 2 para configuración, 1 para el resto, 130 con Ctrl+C | Revisor |
| 25 | `-p` desconocido | **Hecha** (avisa y sigue) | Revisor |
| 26 | `--help-all`, `-v`, `--list-*`, opciones de `bench` | **Distinto a propósito** | — |
| 27 | `lirtup-gui -c` | **Hecha** (y lo pasa a `serve`) | Revisor |
| 28 | Valores fuera de rango | **Distinto a propósito** | — |
| 29 | QSettings de la 1.x | **Hecha** en lo que la 2.0 guarda (bandeja). La geometría es un blob de Qt y no se migra | M |
| 30 | Paletas | **Hecha** (`palette_preset` → semilla); los colores sueltos, distinto a propósito | M |
| 31 | Migración a modos | **Hecha** | M |
| 32 | Una sola instancia | **Hecha** | Y |
| 33 | Ocultarse al escalar | **Hecha** | Y |
| 34 | `ExitApp` cierra la GUI | **Distinto a propósito**: con la bandeja encendida, que es lo predeterminado, la 1.x tampoco cerraba la GUI | Y |
| 35 | Preguntar por los cambios sin guardar al salir | **Hecha** | Y |
| 36 | Escalar con ajustes sin guardar | **Distinto a propósito** (la GUI es la única que escribe) | — |
| 37 | Reordenar perfiles | **Hecha** (Ctrl+Shift+↑/↓) | Revisor |
| 38 | Ícono después de crear el perfil; mover o borrar el PNG | **Hecha** (Editar perfil: desde la ventana, un archivo o quitarlo) | Revisor |
| 39 | Ctrl+Q, Supr y F2 | **Hecha** | Revisor |
| 40 | Geometría de la ventana | **Hecha** | Y |
| 41 | `PauseHotkeys` desde la GUI | **Hecha** | Y |
| 42 | Pendientes de la ronda 3 | `start_delay_s` y el test de traducciones, **hechos**; el ícono en el cajón, **hecho**; el teclado, al guion. Deshacer de modos, `Route::Modes { from }` y `Action::Toast` no son de paridad: pasan a después de la 2.0 | — |
| 43 | Bandeja | **Hecha**. «Cambiar de ventana» no está en el menú del diseño: distinto a propósito | Y |
| 44 | Daemon | **Hecha**; con `follow_focus` la sesión sigue el foco (filas 7 y 8) y el daemon espera | Y |
| 45 | Reanudación, «seguir ejecutándose», avisos | **Hecha** | Y |
| 46 | `.desktop`, íconos, paquete | **Hecha** (mismo paquete de AUR) | P |
| 47 | Efectos instalados | **Hecha** | P |
| 48 | README y `docs/user` | **Hecha** | P |
| — | `hide_cursor: 0` | **Hecha** (siempre oculto) | T |
| — | `program:` en el YAML | **Hecha**: `lirtup run` lo lanza si la línea de comandos no nombra uno | Revisor |

**Después de la 2.0** (no son de paridad):
- Deshacer que también restaure los modos;
- `Route::Modes { from }`;
- `Action::Toast` para «Copiado»;
- un `Action::SaveSetting` propio para la bandeja;
- guardar sin aviso «fijar» y la posición de la ventana;
- `libc` en `lirtup-pipeline` en lugar de las declaraciones a mano de `strftime`;
- el panel de stats arrastrable;
- el ícono en el OSD;
- optimizar el rasterizado de la barra en la CPU;
- `Conversion::create_into` (la medición de T no lo justifica: la cadena entera tarda 0,44 ms de
  GPU);
- el anillo por cuadro, que T cerró.

## Criterios de 5.8 (ronda 4)

| Criterio | Estado |
| --- | --- |
| La barra sobre el juego no cambia los tiempos de GPU en más de 0,1 ms | **Cumplido**: suma 0,002 ms (la medición de T) y el test con GPU `bar_on_a_4k_screen` pasa |
| Bandeja con los 4 estados | **Cumplido** en el test de `status` y en la prueba en vivo de Y; el guion lo repite a ojo |
| El daemon sigue escalando con la GUI cerrada | **Cumplido** (prueba en vivo de Y); va al guion |
| La migración de la configuración real de la persona da los mismos resultados de escalado | **Cumplido**: el test de M resuelve antes y después y compara, y la migración de una copia de la configuración real dio el modo esperado |
| Paquete de AUR y AppImage que se instalan y arrancan en una máquina limpia | **El de AUR, construido** (`makepkg`, `linux-rt-upscaler-2.0.0-1`, que da su versión y sus 117 efectos sin instalarse); arrancarlo en una máquina limpia va al guion. **La AppImage no se armó**: faltan `appimagetool` y `patchelf`, que no instalé |
| Ningún pendiente de paridad abierto | **Cumplido** (tabla de arriba); lo que necesita manos está en el guion |

## Guion único de pruebas a mano

Junta lo que sigue abierto de las rondas 2, 3 y 4. **Necesita el teclado y el ratón reales**: lo
que inyecta `xdotool` no llega a XWayland bajo KWin, así que nada de esto se pudo probar desde la
sesión.

Antes:
- instalar el paquete (sección A), o tener `target/release/lirtup` y el bundle de `dx`;
- una copia de la configuración en un `XDG_CONFIG_HOME` de prueba, con un perfil que coincida con
  una ventana de prueba y otro con `daemon_exclude: true`;
- ventanas de prueba: `glxgears -geometry 960x540`, mpv forzado a X11 y un juego con DXVK.

### A. Instalación (P)

1. Construir el paquete en una máquina limpia (Arch o CachyOS) con `makepkg -si` (antes del tag,
   `#branch=rust/r4-integration`) o `pkgctl build`.
2. Comprobar el paquete:
   - `pacman -Ql linux-rt-upscaler` incluye `/usr/bin/lirtup`, `/usr/bin/lirtup-gui`,
     `/usr/lib/lirtup/` y los 117 efectos;
   - `lirtup --version` dice `2.0.0`.
3. Abrir *LirtUp Forge* desde el menú: tiene ícono, estilos y fuentes.
4. Si se instala la herramienta: `scripts/build-release-linux.sh` arma la AppImage. Probarla en
   Ubuntu 24.04 y Fedora; si falla por la glibc, anotar la versión.
5. Migración desde la 1.x: con la 1.3.0 y perfiles, abrir la GUI.
   - Aparece el diálogo de M.
   - «Migrar» deja `config.yaml.1x.bak` y cada perfil escala igual.
   - «Ahora no» no vuelve a preguntar.

### B. Escalado, superposición y barra (rondas 2 y 4)

6. **Hallazgo 1**: un juego con DXVK se ve negro bajo KWin con XWayland, igual que con la 1.x.
   Confirmar que `troubleshooting.md` lo explica.
7. **Atajos globales** con una sesión, cada uno con NumLock prendido y apagado: Alt+Shift+I, M, G,
   P, Plus/Minus, las flechas, R, S, D (la barra) y Escape. Con `serve` sin clientes, Alt+Shift+A y
   Q arrancan sobre la ventana enfocada; con la GUI enfocada, avisan que hay que enfocar la ventana.
8. **Ratón** en `fullscreen` y `windowed`:
   - clics, doble clic, arrastre y rueda en el lugar correcto;
   - nada en las bandas;
   - en `top-transparent`, los clics pasan de largo;
   - el hover se mantiene con el puntero quieto.
9. **Barra**:
   - aparece en el borde y se va a los 2 s;
   - los botones: fijar, stats, captura (el aviso sale después de guardar), recorte, ocultar y
     detener;
   - el multiplicador recorre ×2 → Auto → No;
   - en modo ventana, el arrastre mueve la ventana y la posición queda guardada;
   - «a pantalla completa» y vuelta.
10. **OSD y panel**, en 1440p y 1080p:
    - se leen;
    - la fila «Shown» dice «render X ms (+Y wait)»;
    - el cursor, con `hide_cursor: 0`, no aparece sobre el juego, pero sí sobre la barra.
11. **Minimizar el objetivo**: pausa y oculta; al restaurarlo, vuelve.
12. **`follow_focus`**:
    - pasar el foco a otro juego: lo sigue sin parpadeo y con el perfil del juego nuevo;
    - pasar el foco a la GUI: se queda en el juego;
    - cerrar el juego: la superposición se oculta y toma la próxima ventana con foco.
13. **`pause_on_focus_loss`**: oculta la superposición al enfocar otra app solo en always-on-top y
    top-transparent.
14. **FG con video real** (mpv a 24 o 30 fps), por capa y por motor integrado, a ×2, ×3, ×4 y Auto:
    - el movimiento se ve parejo;
    - Auto reinicia una sola vez;
    - `Restart` con la capa retoma la sesión.
15. **`ctl pick_window`**: sale la cruz; un clic devuelve la ventana y Escape cancela.
16. **La GPU integrada** (`--gpu 1`), si existe: un modelo y un efecto MagpieFX.

### C. GUI (rondas 3 y 4)

17. **Teclado**:
    - Tab recorre todo, con anillo de foco;
    - Ctrl+S, N, D, F y Q, Ctrl+Shift+G, F2, Supr (no mientras se escribe) y Ctrl+Shift+↑/↓;
    - las flechas en la cuadrícula, Intro y la tecla Menú.
18. **Diálogos**: el foco queda atrapado, Esc y el fondo cierran, y el foco vuelve a quien abrió.
    Probar Nuevo perfil, Cambios sin guardar, Agregar efecto, Medir, Salir y Migrar.
19. **Cambios sin guardar** al navegar y al salir: las tres opciones.
20. **Restablecer**: las tres opciones, y «Volver a heredar» en una fila.
21. **Modos**:
    - reordenar la cadena arrastrando y con las flechas;
    - «Medir en mi GPU», con Cancelar;
    - con una sesión activa, Guardar aplica el cambio en vivo.
22. **Perfil**:
    - cambiar el ícono desde «Editar perfil» (desde la ventana, un archivo, quitarlo);
    - renombrarlo: el PNG se mueve;
    - borrarlo: el PNG se borra;
    - el ícono aparece en el cajón.
23. **Atajos (Ajustes)**: con `serve` corriendo, capturar Alt+Shift+A.
24. **Idioma y tema**: cambiarlos sin reiniciar.

### D. Bandeja, daemon y ciclo de vida (ronda 4)

25. **Ícono y menú**:
    - los cuatro estados del ícono (reposo, vigilando, escalando y error);
    - el menú completo, que cambia de idioma con la GUI.
26. **Daemon desde la bandeja**:
    - escala la ventana que coincide;
    - la GUI se oculta;
    - llega el aviso con «Detener», que la detiene sin volver a escalarla hasta que se cierre.
27. **Perfil ▸ y Generación de cuadros ▸** con sesión: reinician con lo elegido.
28. **Cerrar y minimizar a la bandeja**, empezar oculto, una sola instancia, geometría al volver
    a abrir.
29. **«Seguir ejecutándose tras salir»**:
    - apagado: `serve` termina en unos 5 s;
    - encendido: sigue, escala solo y avisa con `notify-send`.
30. **Escalado diferido**: la demora guardada.
31. **`lirtup run`**:
    - `lirtup run vkcube` escala la ventana del programa y la cierra al terminar;
    - `lirtup run -t título vkcube` no lanza nada;
    - `lirtup run -d` vigila;
    - `--log-file` escribe en DEBUG;
    - un `-p` desconocido avisa;
    - un error de configuración sale con código 2.

## Pasos que le quedan a la persona para publicar

1. **Confirmar el nombre `linux-rt-upscaler` para AUR.** Lo pidió P: está libre en AUR, pero es
   también el nombre del proyecto original en PyPI. La alternativa es `lirtup-forge`.
2. **Respaldo de la 1.x:**

   ```bash
   git push origin py_historical_backup
   ```

3. **Merge a `main`:** revisar `rust/r4-integration` (la ronda 4 entera más las rondas 1 a 3 que
   ya contiene) y hacer el merge, por PR o local.
4. **Tag y release:**

   ```bash
   git tag v2.0.0 && git push origin v2.0.0
   ```

   El workflow `release.yml` arma el tarball y la AppImage. La fecha del metainfo es
   `2026-11-01`: si el tag sale otro día, hay que cambiarla.
5. **AUR:** subir `packaging/aur/PKGBUILD` y `.SRCINFO` (ya con `#tag=v2.0.0`) al repositorio de
   AUR del paquete, después del tag.
6. **AppImage local** (opcional): instalar `patchelf` (está en los repos) y `appimagetool` (o
   `scripts/build-release-linux.sh --fetch-tools`).
7. **Una captura de la GUI nueva** para el README (`docs/assets/`).
8. Correr el guion de arriba, por lo menos A, B 6 a 9, C 17 a 18 y D 25 a 29.
