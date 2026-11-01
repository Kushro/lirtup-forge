# Ronda 4, frente P: empaquetado y documentación

Rama **`claude/rust-r4-packaging-ad5ac3`**, que sale de `rust/r4-skeleton` (`27b7d08b`). Fecha:
2026-11-01. Sin push y sin merge.

Compilé con `CARGO_TARGET_DIR=$HOME/.cache/lirtup-forge/target-r4-p`. Los artefactos de prueba
quedaron en `~/.cache/lirtup-forge/r4-p/` y en `target-r4-p/release-linux*`.

## Qué hice

| Commit | Qué |
| --- | --- |
| `19b40c3c` | `packaging/linux/`: el lanzador, el `.desktop`, el metainfo e `install-tree.sh`; `packaging/tools/tailwindcss` |
| `91ed1c98` | `packaging/aur/PKGBUILD` y `.SRCINFO` |
| `5b374c60` | `packaging/appimage/AppRun` y `scripts/build-release-linux.sh` |
| `9d19700d` | Íconos de 32, 48, 64, 128 y 512 px en `data/icons/hicolor`, del mismo SVG |
| `7222c8c0` | `.github/workflows/rust.yml` (CI) y `release.yml`; borré `build_wheels.yml`; `docs.yml` sin Python |
| `98f07d14`, `286fe18b` | `docs/user/**` y `mkdocs.yml` |
| `8fa7bfc1` | `README.md` |

### El árbol de instalación

`packaging/linux/install-tree.sh DESTDIR PREFIX` lo comparten el PKGBUILD y la AppImage:

| Ruta | Qué |
| --- | --- |
| `PREFIX/bin/lirtup`, `PREFIX/bin/lirtup-gui` | El mismo lanzador dos veces: el nombre elige el binario |
| `PREFIX/lib/lirtup/lirtup`, `lirtup-gui` | Los binarios, juntos: la GUI lanza `serve` desde su carpeta (`link.rs`) |
| `PREFIX/lib/lirtup/assets/` | Los assets del bundle de `dx`. El resolvedor de dx 0.7.10 en Linux prueba `<exe>/../../lib/<producto>` y después la carpeta del ejecutable; con el binario en `lib/lirtup/` cae siempre en la segunda, sin recorrer `/usr/lib` |
| `PREFIX/share/lirtup/effects/` | Los 117 efectos de MagpieFX (`SYSTEM_EFFECTS_DIR` de `catalog.rs`) |
| `PREFIX/share/applications`, `metainfo`, `icons/hicolor` | El `.desktop`, el metainfo y los íconos, con el id de la 1.x (`io.github.baronsmv.linux-rt-upscaler`) |

**Datos de `src/upscaler/`.** Los `.spv` y los modelos CuNNy van dentro de los binarios
(`include_bytes!` en `lirtup-effects` y `lirtup-gpu`); el C++ de lsfg se compila. Lo único que
se lee en tiempo de ejecución son los efectos de MagpieFX. `install-tree.sh` los toma de
`crates/lirtup-magpiefx/effects` si existe y, si no, de `src/upscaler/magpiefx/effects`; con
`LIRTUP_EFFECTS` se fija otra carpeta. Así funciona antes y después de que el revisor mueva los
datos, si los pone en ese lugar.

**El lanzador** calcula el prefijo desde su propia ruta, así que es el mismo en `/usr`, en
`/usr/local` (el tarball) y en la AppImage. Exporta:
- `LIRTUP_EFFECTS_DIR=<prefijo>/share/lirtup/effects`, si no está puesta. Hace falta porque
  `builtin_dir` prueba antes la carpeta del repositorio donde se compiló (`CARGO_MANIFEST_DIR`),
  y `makepkg` deja ese `src/` en disco;
- `WEBKIT_DISABLE_DMABUF_RENDERER=1` para la GUI, salvo que ya esté puesta.

**Tailwind.** Con `NO_DOWNLOADS=1`, dx 0.7.10 no baja su Tailwind 4.1.5: usa el `tailwindcss` del
`PATH` (`CliSettings::prefer_no_downloads`). `packaging/tools/tailwindcss` corre el de
`crates/lirtup-ui/node_modules` (4.3.3, de `package.json`) con `--minify`, así que dx reescribe
`assets/tailwind.css` byte por byte igual y el árbol queda limpio. El CSS del bundle es el de
4.3.3; los bytes difieren del versionado solo por la minificación propia de dx al empaquetar.
`Dioxus.toml` no hace falta.

### PKGBUILD

- `pkgname=linux-rt-upscaler`, `pkgver=2.0.0`. Es el nombre del paquete de la 1.x
  (`pyproject.toml`). **En AUR no hay ningún paquete con ese nombre** (lo consulté por la API de
  AUR, solo lectura), ni de la 1.x ni del proyecto original.
- `source` es el tag `v2.0.0` del repositorio de GitHub, que todavía no existe.
- `prepare`: `cargo fetch --locked` y `npm ci --prefix crates/lirtup-ui`. `build`: comprueba que
  `dx --version` sea el `dioxus` de `Cargo.lock`, `cargo build --frozen --release -p lirtup-cli`
  y `dx build --release -p lirtup-ui --desktop` con el Tailwind de arriba. `package`:
  `install-tree.sh` y la licencia.
- Dependencias: `webkit2gtk-4.1`, `gtk3`, `xdotool` (libxdo), `libayatana-appindicator`,
  `libxcb` y `vulkan-icd-loader` (los dos con `dlopen`), `ttf-dejavu` (el OSD), `fontconfig`
  (`fc-list` en Estilo), `xdg-utils`. Para compilar: `cargo`, `dioxus-cli` (está en `extra`, en
  0.7.10), `npm`, `vulkan-headers`, `git`. Opcionales: `directx-shader-compiler`, `glslang`,
  `lsfg-vk`.
- `options=('!lto')`: el `-flto` de `makepkg.conf` terminaría en el archivo estático de lsfg.

### AppImage y `build-release-linux.sh`

`scripts/build-release-linux.sh` compila (salvo `--skip-build`), arma el árbol y deja en
`--out` (por defecto `$CARGO_TARGET_DIR/release-linux`):
- `lirtup-forge-<versión>-linux-x86_64.tar.gz`: el árbol `usr/`, que se descomprime en `/usr` o
  `/usr/local`;
- `LirtUp_Forge-<versión>-x86_64.AppImage`: el mismo árbol en un AppDir, con `AppRun`, el
  `.desktop` y el ícono en la raíz.

La AppImage no incluye WebKitGTK, GTK ni el cargador de Vulkan, como en Miko Kagura. Incluye la
`libxdo` que enlaza la GUI (`.so.3` en Debian, `.so.4` en Arch), con `patchelf --set-rpath
'$ORIGIN'`. `AppRun` abre la GUI; `cli` como primer argumento, o un enlace llamado `lirtup`
(`$ARGV0`), abre la CLI. `appimagetool` se toma del sistema, de `APPIMAGETOOL` o, con
`--fetch-tools`, de la release 1.9.1 de GitHub, comprobada con su sha256 (el que publica la API
de GitHub).

### CI

`rust.yml`, en un contenedor `archlinux:base-devel` (headers de Vulkan recientes para lsfg y
`dioxus-cli` en los repos): `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D
warnings`, `cargo test --workspace`, `check-ftl-keys.py`, que el `tailwind.css` versionado sea el
que da `package.json`, y `dx build` con `git diff --exit-code` al final. Si el `dx` de pacman no
es el de `Cargo.lock`, lo instala con `cargo install`.

`release.yml` (tags `v*` y a mano) corre `build-release-linux.sh --fetch-tools` y guarda el
tarball y la AppImage como artefactos del workflow. No publica una release ni nada en AUR.

`docs.yml` corría en `master` (la rama es `main`) e instalaba el paquete de Python para
mkdocstrings. Ahora corre en `main` y solo instala `mkdocs-material`.

### Documentación

- `README.md`: la 2.0 como la app; instalación, inicio rápido, guías, limitaciones conocidas
  (Vulkan bajo KWin con XWayland, Wayland nativo, teclado), «Changes from 1.x», efectos de
  Magpie y generación de cuadros resumidos, agradecimientos sin las dependencias de Python.
- `docs/user/`: `installation.md`, `migration.md` (nuevo), `gui.md`, `modes.md` (nuevo, con la
  generación de cuadros que estaba en el README), `controls.md` (atajos, barra sobre el juego,
  panel, capturas), `tray-daemon.md` (nuevo), `cli.md`, `configuration.md` (modos y
  `gui-config.yaml`), `magpiefx.md` (comandos de la 2.0), `troubleshooting.md` (nuevo).
- `mkdocs.yml`: las páginas nuevas, sin `mkdocstrings` ni la sección de la API de Python, y con
  `rust-refactor-analysis.md` en Developer.

## Lo que verifiqué

| Prueba | Resultado |
| --- | --- |
| `cargo build --release --locked -p lirtup-cli` | Pasa |
| `dx build --release -p lirtup-ui --desktop` con `NO_DOWNLOADS=1` y el envoltorio | Pasa; el CSS del bundle dice `tailwindcss v4.3.3`; `git status` limpio |
| `install-tree.sh` en un directorio aparte | Árbol de 49 MB con todo lo de la tabla |
| `lirtup list effects` desde el árbol | 117 efectos, desde `<árbol>/usr/share/lirtup/effects` (`sh -x` del lanzador) |
| La GUI desde el árbol, en un Xvfb con `XDG_CONFIG_HOME` y `XDG_RUNTIME_DIR` de prueba | Abre con sus fuentes y estilos; `WM_CLASS` es `lirtup-gui` (el del `.desktop`); un clic en «Modo daemon» lanza `<árbol>/usr/lib/lirtup/lirtup serve` |
| El AppDir, con `AppRun` directo | La GUI; `AppRun cli --version`; `ARGV0=…/lirtup` abre la CLI; el `serve` que lanza la GUI hereda `LIRTUP_EFFECTS_DIR` del AppDir y `WEBKIT_DISABLE_DMABUF_RENDERER=1` |
| `build-release-linux.sh --skip-build` | Tarball de 19 MB; el AppDir completo; se detiene en `appimagetool`, que no está instalado |
| `desktop-file-validate`, `appstreamcli validate --no-net` | Sin errores |
| `makepkg -d` en `~/.cache/lirtup-forge/r4-p/makepkg`, con el PKGBUILD apuntando a esta rama | **Pasa**: `linux-rt-upscaler-2.0.0-1-x86_64.pkg.tar.zst`, 17 MB, con el árbol completo y la licencia. Extraído (sin instalar): `lirtup list effects` da 117 desde su `usr/share`, y la GUI abre en el Xvfb y se conecta a su `serve` (daemon activo). Un aviso de `makepkg`: «referencias a `$srcdir`» en los dos binarios, por el `CARGO_MANIFEST_DIR` de `builtin_dir` (pedido 3 al revisor) |
| `cargo test --workspace` sin GPU (`VK_ICD_FILENAMES` inexistente) y sin `DISPLAY` | 390 pasan, 0 fallan: el CI puede correrlos así |
| `cargo fmt --all --check`, `check-ftl-keys.py` | Pasan (570 claves en cada idioma) |
| Enlaces internos del README y de `docs/user/` (con anclas) | Ninguno roto |

## Lo que no pude verificar

- **La AppImage como archivo.** No hay `appimagetool` ni `patchelf` en esta máquina, y no
  descargué ni instalé nada. El AppDir sí está probado.
- **El CI.** No hay `act`. Los YAML son válidos, pero no corrieron. En particular no sé si
  `clippy -D warnings` pasa después de los merges: en el esqueleto, los `todo!()` dan avisos de
  `clippy::todo`.
- **`mkdocs build`**: no está instalado. Dejé `docs.yml` sin `--strict` por los enlaces viejos
  de `developer/`.
- **La máquina limpia**: va al guion de abajo.
- La AppImage se construye en Arch, así que pide una glibc tan nueva como la de Arch. No sé si
  corre en Ubuntu 24.04. Si hace falta, el job de release puede pasar a un contenedor más viejo
  (Ubuntu 22.04 con el SDK de Vulkan de LunarG para los headers de lsfg).

## Filas de la lista de paridad

| # | Estado |
| --- | --- |
| 46 | **Cerrada**: `.desktop`, metainfo e íconos instalados por el PKGBUILD y la AppImage, con el mismo paquete de AUR. pipx y las wheels se retiran: el workflow de wheels está borrado |
| 47 | **Cerrada**: los efectos van a `/usr/share/lirtup/effects` (AUR) y a `$APPDIR/usr/share/lirtup/effects` (AppImage). El lanzador exporta `LIRTUP_EFFECTS_DIR`, así que **`LIRTUP_DATA_DIR` no hace falta** |
| 48 | **Cerrada** en lo que me toca: README y `docs/user` de `lirtup run/serve/ctl/list/bench/migrate`, las filas «Distinto a propósito» (16, 18, 26, 28, 30 y los colores sueltos, 36, `__GLX_VENDOR_LIBRARY_NAME` de la fila 3) y lo de Vulkan bajo XWayland |

**Las guías describen la 2.0 terminada**, con lo que T, Y, M y el revisor están implementando en
paralelo. Hay que comprobarlas después de los merges:
- T: los botones de la barra (los de `ToolbarAction`), cuándo aparece y se oculta, el arrastre
  en modo ventana y las capturas en hora local con todo `strftime` (`controls.md`);
- Y: el menú y los 4 estados de la bandeja, las opciones de Ajustes ▸ General, una sola
  instancia, «Seguir ejecutándose tras salir», los avisos con «Detener», `run -d` y la búsqueda
  de la ventana del programa lanzado (filas 1 a 4) (`tray-daemon.md`, `cli.md`);
- revisor: las filas de la CLI 19 a 21, 23 a 25 y 27 (`cli.md` documenta el programa como
  posicional, el orden de la 1.x, `--select` ordenado con `q`, los códigos 1, 2 y 130 y
  `lirtup-gui -c`).

## Lo que pido

**Al revisor:**
1. El tag **`v2.0.0`**: el `source` del PKGBUILD lo usa. Con la versión del workspace en `2.0.0`,
   el tarball y la AppImage se llaman `…-2.0.0-…`. La fecha de la release en el metainfo es la de
   hoy (`2026-11-01`): cámbiala si el tag sale otro día.
2. Al mover los datos: si los efectos no van a `crates/lirtup-magpiefx/effects`, agrega la ruta
   nueva al bucle de `install-tree.sh` (o avísame). El `build.rs` de `lsfg-sys` también lee de
   `src/upscaler/lsfg`.
3. Que `builtin_dir` pruebe la carpeta del repositorio solo en debug (`cfg!(debug_assertions)`):
   en un binario de release apunta a la carpeta de la compilación. El lanzador lo evita con
   `LIRTUP_EFFECTS_DIR`, pero quien corra `/usr/lib/lirtup/lirtup` directo lo vería.
4. Al retirar Python, lo que no es mío pero queda viejo: `docs/index.md` (enlaza la API de
   Python), `docs/api/`, `data/applications/*.desktop` (el de la 1.x, con
   `EXEC_PATH_PLACEHOLDER`) y `scripts/desktop_integration.sh`, reemplazados por
   `packaging/linux/`. El `.gitignore` de Python ignora `build/`, `dist/` y `lib64/`: ojo si
   alguna carpeta nueva se llama así.
5. `check-ftl-keys.py` lo dejé en el CI como comprobación rápida, aunque M agregó `i18n_check`.

**A la persona:**
1. Confirmar el nombre **`linux-rt-upscaler`** para AUR. Es el de la 1.x en `pyproject.toml`,
   pero también el del proyecto original en PyPI. En AUR está libre; `lirtup-forge` sería la
   alternativa.
2. Publicar el paquete en AUR, el tag y la release de GitHub: no publiqué nada.
3. Una captura de la GUI nueva para el README: la de la 1.x salió, y `docs/assets/` no es mío.
4. Si quiere que construya la AppImage aquí: hace falta instalar `patchelf` (está en los repos)
   y bajar `appimagetool` (o `--fetch-tools`, que lo baja de GitHub y lo comprueba).

## Guion de prueba en una máquina limpia

Una VM o un contenedor sin nada de LirtUp Forge ni de Rust. Con contenedor, la GUI necesita un
servidor X (`xvfb-run`) y la GPU pasada al contenedor (`--device /dev/dri`); una VM con
escritorio es más simple.

### A. Paquete de AUR (Arch o CachyOS)

1. En una instalación nueva con `base-devel` y `git`:

    ```bash
    git clone https://github.com/Kushro/lirtup-forge.git
    cd lirtup-forge/packaging/aur
    makepkg -si
    ```

    Antes del tag `v2.0.0`, cambiar en el PKGBUILD `#tag=v$pkgver` por `#branch=<rama>`. Con
    `devtools`, `pkgctl build` lo compila en un chroot limpio y prueba que las dependencias estén
    completas.
2. Comprobar: `pacman -Ql linux-rt-upscaler` lista `/usr/bin/lirtup`, `/usr/bin/lirtup-gui`,
   `/usr/lib/lirtup/{lirtup,lirtup-gui,assets/}`, `/usr/share/lirtup/effects/` (117 `.hlsl`), el
   `.desktop`, el metainfo y los íconos.
3. `lirtup --version` dice `2.0.0`; `lirtup list effects` lista 117; `lirtup list gpus` muestra la
   GPU.
4. Abrir *LirtUp Forge* desde el menú: tiene ícono, la ventana se abre con estilos y fuentes, y
   el ícono de la barra de tareas es el de la app (`StartupWMClass`).
5. Abrir `glxgears` y hacer clic en su mosaico: escala. `Alt`+`Shift`+`Escape` lo detiene.
6. Activar el modo daemon, cerrar la GUI, comprobar que `lirtup serve` sigue con «Seguir
   ejecutándose tras salir» y que termina sin esa opción.
7. Con `directx-shader-compiler` instalado, agregar un efecto de MagpieFX en un modo y escalar: el
   efecto compila y se ve.
8. `sudo pacman -Rns linux-rt-upscaler`: no queda nada en `/usr/lib/lirtup` ni en
   `/usr/share/lirtup`.

### B. AppImage (Ubuntu 24.04 y Fedora, además de Arch)

1. Instalar las dependencias de `docs/user/installation.md` (WebKitGTK 4.1, GTK 3,
   libayatana-appindicator, Vulkan, `libfuse2`).
2. `chmod +x LirtUp_Forge-2.0.0-x86_64.AppImage && ./LirtUp_Forge-2.0.0-x86_64.AppImage`: abre la
   GUI. Si falla por la glibc, anotar la versión del sistema (va a «Lo que no pude verificar»).
3. `./LirtUp_Forge-2.0.0-x86_64.AppImage cli list effects`: 117 efectos, desde
   `/tmp/.mount_*/usr/share/lirtup/effects`.
4. `ln -s "$PWD/LirtUp_Forge-2.0.0-x86_64.AppImage" lirtup && ./lirtup --version`.
5. Los pasos 5 a 7 de A.
6. En Ubuntu, `ldd` sobre `usr/lib/lirtup/lirtup-gui` del AppImage extraído
   (`--appimage-extract`) resuelve `libxdo` dentro de `usr/lib/lirtup/`.
7. Sin FUSE: `./LirtUp_Forge-2.0.0-x86_64.AppImage --appimage-extract-and-run` también abre.

### C. Migración desde la 1.x (en cualquiera de las dos)

1. Con la 1.3.0 instalada por pipx y un `config.yaml` con perfiles, `pipx uninstall
   linux-rt-upscaler` e instalar la 2.0.
2. Abrir la GUI: ofrece la migración; aceptarla deja `config.yaml.1x.bak` y los modos en
   `~/.config/linux-rt-upscaler/modes/`, y cada perfil escala igual que antes.
