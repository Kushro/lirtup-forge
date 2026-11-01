# Ronda 3, frente A: componentes y tema

Rama **`claude/rust-r3-components-61549a`**, que sale de `rust/r3-skeleton`. Fecha: 2026-11-01.
Sin push y sin merge.

- `cargo check -p lirtup-ui`: pasa, sin avisos.
- `cargo clippy -p lirtup-ui --all-targets`: sin avisos en `components/` ni en `theme.rs`. Quedan solo
  los `todo!()` de B, C y D.
- `cargo test -p lirtup-ui`: 10 tests en verde. Los míos son 6:
  - `theme`: `tokens.json` coincide con la semilla, las 21 semillas pasan el contraste en los dos
    temas, y el estilo tiene todos los roles;
  - `components`: cada `t!("cmp-…")` existe en `es` y en `en`;
  - `inputs`: números (coma decimal, ajuste al paso);
  - `hotkey`: códigos del DOM → nombres de 1.x que `lirtup-x11` reconoce.
- Tailwind 4.3.3 (`npx tailwindcss -i tailwind.css`): compila sin avisos.
- `python3 scripts/contrast.py`: **0 fallos de 54 en cada tema** (detalle más abajo).

## Qué hice

### Tema (`src/theme.rs`)

- Los 39 roles salen de `material-colors` 0.5 con el esquema *tonal spot* a partir de la semilla.
  El lienzo pide dos familias fijas encima: el terciario siempre es ámbar (tono 49,5 y croma 40,
  el `#8A4A1F` del lienzo) y `success` es un verde (tono 153). M3 no tiene un rol de éxito.
- El cristal de la superposición siempre se calcula con el esquema oscuro de la semilla
  (`surface-container` al 86 %). Las capas de estado son `on-surface` al 8 % y al 10 %. El velo
  (`scrim`) es negro al 60 % en oscuro y al 40 % en claro.
- `PALETTES`: las paletas de 1.x convertidas en semillas, tomando su acento (`control`). Light y
  Dark eran las predeterminadas y usan la semilla del lienzo. Gruvbox y Solarized tienen el mismo
  acento en claro y en oscuro, así que quedan en una semilla cada una. Total: 20.
- `root_style` guarda en caché el último par (semilla, esquema), porque `app.rs` lo llama en cada
  render. Una semilla inválida vuelve a `#2F5F9E`.
- `resolve(Theme::System)` lee, una sola vez, `GTK_THEME` y después `color-scheme` y `gtk-theme`
  de `gsettings` (Plasma también los escribe). Si nada responde, usa oscuro.
- Exporto `contrast()` y `contrast_pairs()`: son los mismos pares que mide `contrast.py`.

**Comparación con el lienzo** (lo que el README del diseño dejó pendiente). Con la semilla
`#2F5F9E`, `material-colors` da 19 roles distintos de 35 en oscuro y 18 en claro. Casi todos
difieren en 1 o 2 unidades de un canal. Los cambios que se ven:

| Rol | Lienzo | Generado |
| --- | --- | --- |
| claro `primary` | `#2F5F9E` (la semilla tal cual) | `#3E5F90` |
| claro `on-primary-container` | `#0F3E73` | `#244777` |
| oscuro `tertiary-container` | `#7A3D10` | `#723609` |
| oscuro `success-container` | `#1E5130` | `#0A5227` |
| oscuro `inverse-primary` | `#3A5F94` | `#3E5F90` |

`tokens.json` ahora tiene los valores generados, para que `tokens.css` (el respaldo) y el estilo
de la raíz digan lo mismo.

### Tokens y scripts (`scripts/`)

- **`scripts/gen-tokens`** (bash) hace cuatro pasos:
  1. corre el test de `theme.rs` con `LIRTUP_THEME_DUMP`, que escribe los colores de la semilla;
  2. `gen-tokens-icons.py`;
  3. `gen-tokens.py --colors`, que los mete en `tokens.json` y escribe `assets/tokens.css` y
     `design/tokens.rs`;
  4. `contrast.py`.

  Corre en unos segundos una vez compilado. Usa `CARGO_TARGET_DIR` si está definido.
- **`gen-tokens.py`**: genera `tokens.css`, con los roles por `[data-theme]` y la forma, las
  fuentes y el movimiento en `:root`. También genera `design/tokens.rs` para la barra en Vulkan de
  la ronda 4: `DARK` y `LIGHT` como `[(&str, [u8; 4])]`, `SHAPE_*`, `TYPE_*`, `MOTION_*` (Bézier y
  ms) e `ICONS` (nombre → punto de código). El frente T lo copia o lo incluye con `include!`.
- **`gen-tokens-icons.py`**: arma el subconjunto de Material Symbols Rounded.
  - No usa una lista a mano: toma **cualquier literal de cadena de `src/**/*.rs` que sea el nombre
    de un ícono**. Así cuentan los de B, C y D sin coordinar nada. Hay que correrlo **después de
    los merges** (hoy encuentra 70 íconos, 14 KiB).
  - Conserva FILL 0..1 y wght 400..500, y fija GRAD en 0 y opsz en 24.
  - Las ligaduras quedan solo para los íconos incluidos (`layout_closure = False`).
  - Descarga la fuente de origen (Apache 2.0) una vez en `~/.cache/lirtup-forge/fonts-src`, fuera
    del repositorio.
  - Escribe `design/icons.json`.
- **`contrast.py`**: lee `tokens.json` por defecto (también `lf.css`). Mide los 54 pares de cada
  tema y el texto sobre el cristal, sobre negro y sobre blanco. Si algo falla, sale con 1.

### Fuentes (`assets/fonts/`), sin red

- `GoogleSansFlex.ttf`: el archivo variable original de google/fonts, sin modificar, 3,9 MB, por
  la marca. Lo acompañan `GoogleSansFlex-OFL.txt` y `GoogleSansFlex-TRADEMARKS.md`.
- `JetBrainsMono.ttf`: el variable de wght, sin modificar, con `JetBrainsMono-OFL.txt`.
- `MaterialSymbolsRounded-subset.woff2`: el subconjunto, con
  `MaterialSymbolsRounded-LICENSE.txt` (Apache 2.0).
- Roboto Flex no va incluida: sigue como respaldo en la pila de `--font-brand` si está en el
  sistema.
- Los `@font-face` de `main.css` usan URL absolutas `/assets/fonts/…`, porque Tailwind 4 reescribe
  las relativas de un `@import`.
  - Con `dx`: `theme.rs` declara `asset!("/assets/fonts", folder sin hash)` y lo toca en
    `root_style`, así el empaquetado copia la carpeta tal cual.
  - Sin `dx`: el renderizador de escritorio sirve `/assets/` desde el crate.

### CSS (`assets/main.css`)

- Las clases de componentes de `lf.css` están en `@layer components` con el prefijo **`lf-`**,
  porque Tailwind ya define `group`. Por ejemplo: `lf-btn filled|tonal|…`, `lf-ibtn`, `lf-seg`,
  `lf-row ovr|dirty`, `lf-nav rail`, `lf-tile hov|foc|run`, `lf-menu`, `lf-dlg` y `lf-snack`.
- Los colores siempre salen de variables de rol. El hover y el pulsado son una capa de estado
  (`lf-state`, un `background-image`) que no toca el color del contenedor. Deshabilitado:
  contenido al 38 % y contenedor al 10 %.
- **Movimiento reducido**: `tailwind.css` ya acorta las transiciones. Aparte de eso:
  - los botones y el interruptor no cambian de forma al pulsar;
  - el indicador de carga pasa a ser un anillo que gira (el único movimiento que queda).

  La galería tiene un interruptor que fuerza lo mismo con la clase `lf-reduce`.
- La barra flotante y el snackbar se centran en la página y no en la ventana: usan `:has(.lf-nav)`
  para descontar el cajón o el riel. El contenedor de la barra **no usa transform**, porque un
  menú fijo adentro (el de Restablecer) se descolocaría.
- No toqué `tailwind.css`.

### Componentes

Los 30 de la nota, con estas decisiones:

| Componente | Cómo |
| --- | --- |
| `Button` | Con `title`, se envuelve en `Tooltip` |
| `IconButton` | Su `label` es `aria-label` y tooltip; `selected` lleva `aria-pressed`, ícono relleno y forma cuadrada |
| `SplitButton` | La mitad del menú abre un `Menu` debajo (o encima si no cabe) |
| `Connected` | `aria-pressed`. El segmento elegido muestra su ícono, o ✓ si no tiene |
| `Switch`, `Checkbox` | `button` con `role` `switch` o `checkbox` |
| `Slider` | Pista de diseño más un `input type=range` transparente encima, que da puntero, teclado y a11y. La caja del número se edita: con foco muestra el número plano, Intro o salir del campo confirman (con ajuste al paso y coma o punto), Esc descarta. Con `stops`, muestra el nombre y puntos en la pista |
| `Select` | Botón `.dd` y lista propia (`listbox`). La lista se abre con flechas y se maneja con flechas, Inicio, Fin, Intro y Esc |
| `TextField` | Con `placeholder` es compacto (48 px; el `label` queda como `aria-label`, como la búsqueda del diseño). Sin `placeholder`, la etiqueta flota (56 px). El error usa `aria-invalid` y `role=alert` |
| `PathField` | Campo más «Elegir…», un `input type=file` invisible encima del botón. **Dioxus desktop ya abre el diálogo nativo con `rfd` y devuelve la ruta completa** (`FileData::path`), así que no pido `rfd`. `directory` usa `webkitdirectory` y los filtros pasan a `accept` |
| `ColorField` | Muestras, `input type=color` (el selector de GTK) y un campo hex |
| `FontField` | Un `Select` con «Google Sans Flex (incluida)» como valor vacío, más una muestra en esa fuente |
| `SettingRow` | Los 3 orígenes. Con `global`: insignia «Global» y sin franja (`Overridden` se trata como heredado). Con cambios, el subtítulo de ayuda sigue a la vista |
| `SettingGroup` | `collapsible` usa `aria-expanded` |
| `NavDrawer`, `NavItem` | `nav` con arriba desplazable y abajo fijo. En riel, la etiqueta pasa a tooltip lateral y el punto o el ▶ a una marca en la esquina. `aria-current` e `aria-label` con «· Escalando · Sin guardar» |
| `Tabs` | `tablist` con *roving tabindex*: las flechas, Inicio y Fin seleccionan y mueven el foco |
| `WindowTile` | Ver la sección siguiente |
| `HotkeyCapture` | Ver la sección siguiente |
| `Menu` | Ver la sección siguiente |
| `Dialog` | Velo con clic que cierra solo si el clic empezó afuera. Foco atrapado con dos centinelas (no depende de `prevent_default`). Esc cierra. El foco va al diálogo, salvo que un campo con `autofocus` ya lo tenga, y al cerrar vuelve al que lo tenía. Cerrar pone `open` en `false` **y** llama a `on_close` |
| `Snackbar` | `role=status`; la acción también cierra; botón × |
| `FloatingToolbar` | Sin `open`, sale hacia abajo y queda `inert` |
| `Tooltip` | Solo CSS: hover o foco visible, 500 ms |
| `LoadingIndicator` | Forma que se transforma y gira; `role=progressbar` |

**`WindowTile`**:

- todo el mosaico es un solo elemento enfocable (`role=button`, con `data-xid` para las flechas
  de B);
- Intro y Espacio lo activan;
- la tecla Menú o Mayús+F10 abren el menú en su centro, y el clic derecho o ⋮ en el puntero;
- la vista previa usa `img` con `object-fit: contain`, y `aspect` acepta `4/3`, `16/9`, `video` y
  `auto` (el de la ventana).

**`HotkeyCapture`**:

- toma el `code` físico y lo traduce a los nombres de 1.x;
- al capturar, la fila de `SettingRow` se pone en `primary-container` con `:has`;
- muestra los modificadores que se mantienen apretados;
- Esc cancela y Retroceso borra;
- con `require_modifier`, avisa «Requiere un modificador»;
- el valor pasa por `Hotkey::from_str` antes de guardarse.

**`Menu`**:

- se ubica y se enfoca con un script chico;
- con `at`, se ajusta para no salirse de la ventana;
- maneja flechas, Inicio, Fin, Esc y Tab;
- al cerrar, el foco vuelve a quien lo abrió;
- un velo transparente lo cierra con un clic afuera.

### i18n

`i18n/{es,en}/components.ftl`: 26 claves con el prefijo **`cmp-`**, para que no choquen con las de
las pantallas. Reuso `nav-running` y `nav-unsaved` de `app.ftl`. Hay un test que verifica que
cada clave usada exista en los dos archivos.

### Galería

Con `LIRTUP_GUI_GALLERY=1` (o Ctrl+Shift+G) se ven:

- una tarjeta por componente con variantes y estados, cada uno en los dos temas;
- en la cabecera, un selector **Oscuro / Claro / Ambos** (lado a lado);
- las 21 semillas (`ColorField` con las paletas);
- «Reducir movimiento».

Los diálogos, el snackbar, la barra flotante y el menú del mosaico se abren con botones. Los
textos de la galería son datos de muestra del lienzo y no están en Fluent.

## Contraste (`python3 scripts/contrast.py`)

```
oscuro fallos: 0 de 54 · cristal sobre negro y blanco: glass-content 13.2, primary 10.1, glass-content 8.2, primary 6.2
claro fallos: 0 de 54 · cristal sobre negro y blanco: glass-content 13.2, glass-content 8.2
```

Además, el test `every_seed_passes_contrast` mide esos mismos 54 pares para la semilla del lienzo y
las 20 paletas en los dos temas: **0 fallos**.

## Cambios de props (todos agregados con valor por defecto; no cambié ninguno existente)

- `Tooltip { place: &'static str }`: `""` (debajo y centrado), `"left"`, `"right"` o `"side"`.
- `IconButton { tip: &'static str }`: dónde abre su tooltip. **Revisor**: en `app.rs`, los botones
  de la barra de título conviene pasarlos con `tip: "right"`, porque el de cerrar se sale un poco
  de la ventana.
- `SettingRow { detail: Option<Element> }`: contenido a todo el ancho debajo de la fila (el cuerpo
  de un `Disclosure`, los parámetros de un efecto).
- `Menu` con `at: None`: se ubica respecto del ancestro posicionado más cercano. **B, C y D**:
  envuelvan el ancla y el menú en `span { class: "lf-menu-anchor", … }` (como `SplitButton`).
- Nuevos públicos: `components::ProfileIcon` (el ícono o las iniciales de un perfil; sirve para la
  cabecera del perfil de C); `inputs::{format_number, parse_number}` (coma decimal según
  `LANG`); `hotkey::{key_name, hotkey_keys}`; `theme::{DEFAULT_SEED, PALETTES, ROLES, roles,
  contrast, contrast_pairs, parse_hex}`.

## Dependencias que pido

**Ninguna** de Rust ni de npm. `material-colors` ya estaba, y `rfd` no hace falta (ver
`PathField`). Para correr los scripts hacen falta `fontTools` y `brotli` de Python (los del
sistema).

## Pendiente y avisos para el revisor

1. **Al integrar**, en este orden: `scripts/gen-tokens-icons.py` (después de los merges de B, C y
   D, para que el subconjunto tenga sus íconos), luego `scripts/gen-tokens.py` y luego el
   `assets/tailwind.css`. Un ícono que no esté en el subconjunto se ve como su nombre en texto.
2. **`i18n::has_key` falla con los mensajes que tienen variables** (por ejemplo `{ $label }`):
   `lookup_single_language` sin argumentos da error. Por eso mi test lee los `.ftl`. El test de
   traducciones completas de la ronda 4 debería pasar argumentos vacíos o mirar el AST.
3. **Atajos que agarra `lirtup serve`**: con el escalador corriendo, una combinación tomada con
   `GrabKey` (Alt+Shift+A…) nunca llega a la webview, así que `HotkeyCapture` no puede capturarla.
   Hace falta un comando tipo `PauseHotkeys { on }` mientras se captura (núcleo + D). Queda para
   el revisor o la ronda 4.
4. `dx serve` instala su propio Tailwind (v4.1.5) y regenera `assets/tailwind.css` con esa
   versión, no con la 4.3.3 de `package.json`. Compila igual, pero conviene fijar la versión en
   `Dioxus.toml` si se quiere la misma salida.
5. **La galería la vi en vivo** con `dx serve` (dx 0.7.10, ventana XWayland con
   `GDK_BACKEND=x11`), con capturas en oscuro y en claro:
   - las fuentes y los íconos cargan sin red;
   - se ven todos los componentes;
   - corregí dos cosas que encontré: el encabezado «Perfiles» se encogía a 0 dentro del cajón
     desplazable, y el título del mosaico desaparecía cuando la insignia del perfil era larga.

   **No probé con el teclado ni con el puntero** dentro de la ventana: las teclas y la rueda que
   manda `xdotool` no llegan a la ventana bajo Wayland. Quedan por probar a mano:
   - el foco y el atrapado del diálogo;
   - las flechas del menú y de las pestañas;
   - la captura de atajos;
   - el selector de archivos;
   - el anillo de foco en cada componente.
6. `Select` con muchas opciones (las familias de `FontField`) no tiene búsqueda por letra.
7. Las animaciones de entrada (menú, diálogo, snackbar) usan `@keyframes` simples. Los resortes de
   M3E se aproximan con las curvas Bézier de los tokens, no con física.
