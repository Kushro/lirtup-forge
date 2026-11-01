# Ronda 2, frente S: stats, OSD, latencia, barra de menú, medición y paridad de E

Rama **`claude/rust-r2-stats-46ef59`**, que sale de `rust/r2-skeleton`. Fecha: 2026-11-01. Sin
push y sin merge.

Notas anteriores: [rust-r2-skeleton.md](rust-r2-skeleton.md) (el reparto y las firmas) y
[rust-r0-contract.md](rust-r0-contract.md).

## `cargo check`

| Crate | `cargo check -p <crate> --all-targets` |
| --- | --- |
| `lirtup-magpiefx` | **pasa, sin avisos** |
| `lirtup-effects` | la biblioteca pasa; `tests/parity.rs` falla solo porque falta `image` en `[dev-dependencies]` (2 errores) |
| `lirtup-pipeline` | falla solo porque falta `serde_json` en `bench.rs` (4 errores, todos `serde_json`) |

Las dos dependencias ya están en `[workspace.dependencies]`; las pido abajo. Para comprobar el
resto, las agregué un momento a los `Cargo.toml` de los crates, sin commitearlas: con ellas, los
tres crates pasan `cargo check --all-targets`, y `cargo clippy` no da avisos en mis archivos.
Después restauré los `Cargo.toml` y el `Cargo.lock`.

No corrí `cargo build`, `cargo test` ni la 1.x. Los tests están escritos y sin correr. El script
de Python solo pasó `py_compile`.

## Commits

1. `lirtup-pipeline`: stats, panel de stats, OSD y CSV de latencia.
2. `lirtup-pipeline`: detección de la barra de menú de Wine.
3. `lirtup-pipeline`: medición «Medir en mi GPU» y `bench.json`.
4. La paridad de E: los casos, los tests `#[ignore]` y `scripts/rust-parity-refs.py`.

## Qué hice

### `latency`

`LatencyLog` escribe el mismo CSV que la 1.x:

- el encabezado `capture_s,present_s,latency_ms,kind,multiplier`;
- las filas con `{:.6},{:.6},{:.3},{kind},{multiplier}`;
- el disco se vacía más o menos una vez por segundo.

Los presents sin captura (`capture: None`) se saltan.

**Diferencia con 1.x:** los tiempos son segundos desde el inicio de la sesión, no de
`perf_counter`. Así lo pide la firma (`origin`). El contenido por fila es el mismo: la latencia y
el tipo, y la captura compartida entre los cuadros generados del mismo cuadro real.

### `stats`

- **`PresentSink`**: un canal acotado de 4096. Si se llena, el registro se descarta y se anota en
  `debug`. Nunca bloquea.
- **`Stats`**:
  - las tasas usan una ventana de 1 s, como `_Rate` de 1.x;
  - `capture_fps` cuenta los cuadros nuevos, `present_fps` todos los presents (los generados
    también) y `generated_fps` solo los generados;
  - la latencia es la media y el p95, con el mismo rango de `summarize`;
  - los tiempos de GPU se suavizan por etiqueta (`0.8·viejo + 0.2·nuevo`) y suman las dispatches
    repetidas de un pase. Las etiquetas que ya no están se descartan; equivale al
    `clear_gpu_ms` de 1.x cuando cambia la cadena;
  - `poll` vacía el canal, escribe el CSV y devuelve la muestra cada `sample_interval`;
  - `close` vacía lo que quede en el canal antes de cerrar.

  Un CSV que no se puede crear se anota como error y la sesión sigue, como en 1.x.
- **`gpu_budget_warning`**: la misma regla que 1.x (sin control por debajo de 5 fps, presupuesto
  `1000 / max(fps, 30)`). Los casos de `tests/test_stats.py` están portados como tests.
  - **Ojo:** el parámetro se llama `refresh_hz`, pero hace lo que hacía 1.x: es la tasa que hay que
    sostener. `Stats` le pasa la tasa del juego (`capture_fps`), no la del monitor. Si se usara la
    del monitor, a 144 Hz el aviso saltaría con efectos que sí llegan. Está en el doc de la
    función.
  - `heaviest` lleva hasta 3 etapas, de la más pesada a la más liviana; el panel muestra la
    primera, como 1.x.
  - El pase `osd` no cuenta para el presupuesto.
- **`StatsPanel`**: lo dibuja como `_render` de 1.x:
  - dos columnas (etiquetas en celeste, valores en blanco, avisos en rojo) sobre una caja negra
    con alfa 170 y esquinas redondeadas;
  - el margen, el relleno, la separación y el alto de línea de 1.x;
  - la fuente es monoespaciada, a 1,6 % del alto de pantalla.

  Se redibuja solo si cambió la muestra o el contexto (las muestras llegan cada 500 ms, el mismo
  ritmo de 1.x), o si se acaba de mostrar. Ocultarlo pide un present. `rebind` lo redibuja con la
  fuente del tamaño nuevo.
- Las filas siguen el orden de `_stats_rows`: Window, Game, Shown, Frame gen, Latency, Source,
  Upscale, Effects (con ms por etapa), GPU (el aviso), Output, GPU ms/frame, Mode y Profile.
  **No son iguales en contenido a 1.x.** Faltan datos que `StatsSample` y `PanelContext` no traen
  (ver «Cambios de firma que pido»).

### `osd`

`Osd` muestra un mensaje a la vez, centrado y con el estilo de 1.x:

- texto blanco con sombra negra (alfa 180, desplazamiento `max(2, 5 %)`);
- una caja con alfa 180 y radio igual al relleno (el 50 % del tamaño de fuente);
- la fuente a 4 % del alto de pantalla y como mínimo 24 px;
- DejaVuSans, como 1.x.

El texto se rasteriza al llamar a `show`, con una caché de hasta 64 textos; en 1.x se rasterizaba
antes, en otro hilo.

- `update` devuelve `true` cuando el mensaje aparece y cuando vence. En 1.x, al vencer no pedía
  redibujar, y el último cuadro presentado podía quedar con el mensaje.
- No hay fundido, como en 1.x.
- `update_config` toma `show_osd` y `osd_duration`, y si se apaga `show_osd`, borra el mensaje
  que esté a la vista.

Hay dos ayudas `pub(crate)` en `osd.rs` que también usa el panel:

- `Canvas`: compone imágenes premultiplicadas; sirve para la sombra y para las dos columnas,
  porque `TextRenderer` dibuja con un solo color;
- `load_font`: busca la fuente por nombre de archivo en `XDG_DATA_HOME`, `~/.fonts` y
  `XDG_DATA_DIRS/fonts`, como hacía PIL con fontconfig.

### `menubar`

Es un port línea por línea de `menubar.py`, con el `stride` de la firma: mismas constantes,
mediana por canal, separador de 1 o 2 filas y marco del borde del cliente. `MenuBarDetector`
hace lo de `_check_menu_bar`: una muestra cada 250 ms; 3 muestras iguales seguidas con barra dan
`Found`; 12 muestras sin acuerdo dan `NotFound`. Después de decidir, devuelve siempre la misma
respuesta. Están portados los 11 casos de `tests/test_menubar.py`, más uno con filas con relleno
(`stride`) y dos del detector.

### `bench`

Corre en el proceso hijo, sin presentador, como pide la decisión 5. Mide igual que 1.x:

- cuadros de calentamiento;
- después, cuadros medidos hasta `max_frames`, o hasta `min_frames` cuando se acaba el
  presupuesto de tiempo;
- devuelve la mediana y el p95 de `summarize`;
- un caso que falla se informa con la primera línea del error (300 caracteres) y la corrida
  sigue;
- la cancelación se revisa entre cuadros y la corrida termina con `cancelled: true`.

| Tipo | Cómo se mide |
| --- | --- |
| Nativos | `NativeEffect::create` sobre la imagen de prueba en BGRA (como una captura); tiempo de GPU con timestamps (`total_ms`) |
| MagpieFX | `compile_effect_with` con el BGRA plegado en los shaders (`swap_input` y `swap_output`), FP16 según `fp16_wanted`; tiempo de GPU con timestamps |
| FG | `LsfgEngine` y `LsfgContext` con texturas y semáforos externos al tamaño de destino; reloj de CPU desde la copia del cuadro real hasta que se señalan todos los cuadros generados. La espera es un envío vacío que espera los semáforos de salida; el motor se crea y se suelta en cada caso (un motor por proceso) |

`build_cases`:

- **sin modo:** los 3 nativos de 1.x (LumaSharpen, Anime4K_Upscale_S, CRT_Easymode);
- **con modo:** los pases nativos del modo (por etiqueta) o su cadena de MagpieFX;
- después, `effects` sin repetir;
- con `framegen`, los 12 casos de FG en el orden de 1.x.

Los parámetros de los nativos salen del modo, o los de `ModeSettings::default()` si no hay modo.
`request.frames`, si no es 0, limita los cuadros por caso, como `--max-frames`.

`test_image` es la imagen de `make_test_image`: degradados, líneas, una caja y ruido. El ruido es
una secuencia fija, no el de PIL.

`bench.json` tiene la forma de 1.x (`version`, `gpus`, `selectors`). Cada GPU guarda `name`, los
casos por tipo (`ms` y `p95` redondeados a 4 decimales, `frames`, `error`) y `updated` por tipo
en ISO UTC. La escritura es atómica (`.tmp` y `rename`). La clave de la GPU es la de 1.x:
`{vendor:04x}:{device:04x}:{name}`. `results_for_gpu` acepta una clave o un selector, como 1.x.

### Paridad de E

- **`crates/lirtup-effects/tests/fixtures/parity/cases.json`**: 25 casos.
  - 10 nativos: deband, CAS, bloom y viñeta sobre RGBA16F; grano y 2 LUT sobre RGBA8;
    LumaSharpen, Anime4K y CRT.
  - 6 escaladores: copia, Lanczos arriba y abajo, Catmull-Rom abajo, FSR y NIS.
  - Los 9 modelos SRCNN.
- **`crates/lirtup-magpiefx/tests/fixtures/parity/cases.json`**: los efectos `ok` del informe
  (`verified_names()`), con entrada RGBA, parámetros por defecto, cuadro 0 y destino 320×192.
  Hay un mapa `max_diff_by_effect` para dar tolerancia por efecto.
- **`scripts/rust-parity-refs.py`** corre la 1.x como lo hacen el presentador
  (`_scale`, Clear, Linearize) y `UpscalerManager`, y escribe `refs/<caso>.png` al lado de cada
  `cases.json`. Las salidas RGBA16F se guardan como `round(clamp(v)·255)`, y el test hace lo
  mismo.
- **La imagen de prueba no se lee de un archivo.** Es la misma fórmula entera en Python
  (`make_input`) y en Rust (`input_image`), de 160×96. Así los PNG quedan chicos y no hace falta
  un fixture de entrada.
- Los tests (`native_passes_match_python`, `verified_effects_match_python`) son `#[ignore]` y
  comparan canal por canal con tolerancia 1. Si fallan, informan cuántos canales difieren, el
  máximo y el primer píxel. `LIRTUP_GPU` elige la GPU y `LIRTUP_PARITY_CASE` corre un solo caso.
  También hay tests comunes de la conversión a half y de la imagen.

Para correrlos, en la integración:

```text
PYTHONPATH=src python scripts/rust-parity-refs.py
cargo test -p lirtup-effects -p lirtup-magpiefx --test parity -- --ignored
```

Los PNG de referencia **no están en la rama**: los genera el script con la 1.x en la GPU de la
integración.

## Pendiente

- **Dependencias** (abajo). Sin ellas, `lirtup-pipeline` y el test de paridad de `lirtup-effects`
  no compilan.
- **El contenido del panel no es igual al de 1.x** hasta que se resuelva lo de «Cambios de firma
  que pido».
- **Orden de bytes del OSD y del panel.** En 1.x, la textura de pantalla tenía bytes BGRA, y el
  panel intercambiaba rojo y azul antes de subirse. Acá subo RGBA premultiplicado, como el
  `TextRenderer` de E, y supongo que la textura de pantalla de P es RGBA. Si P usa BGRA, la
  etiqueta celeste se va a ver naranja, y hay que intercambiar rojo y azul en
  `Canvas::finish` o en `OverlayBlend`. Lo vemos en la integración.
- **Supuestos del script que no pude probar:**
  - `Linearize` sin `update_constants`, como el presentador;
  - `CRTEasymode.update_constants()` sin parámetros da los mismos valores que
    `CrtParams::default()`;
  - `Texture2D.download()` devuelve los bytes crudos (con 16F, 8 bytes por píxel);
  - `desc.textures[1].format` es el formato de OUTPUT.
- **FG en la medición**: probarlo de verdad. Sobre todo, que un envío vacío que espera los
  semáforos exportados funcione con el driver, y el orden copia → `lsfg_present` → espera.
- **`bench` sin modo** mide solo los 3 nativos de 1.x. El doc de `BenchRequest::mode` dice «every
  native pass and model». Si la GUI quiere todos los pases nativos o los modelos SRCNN sin modo,
  se agregan en `build_cases`.

## Dependencias que pido

| Crate | Sección | Línea | Para qué |
| --- | --- | --- | --- |
| `crates/lirtup-pipeline/Cargo.toml` | `[dependencies]` | `serde_json.workspace = true` | Leer y escribir `bench.json` con la forma de 1.x |
| `crates/lirtup-effects/Cargo.toml` | `[dev-dependencies]` | `image.workspace = true` | Leer los PNG de referencia en `tests/parity.rs` |

Las dos ya están en el workspace. `Cargo.lock` solo suma esos nombres a las listas de esos dos
crates. `lirtup-magpiefx` no necesita nada: ya tenía `image`, `regex` y `tempfile`, y el
manifiesto se lee con `regex`.

## Cambios de firma que pido

Ninguno es obligatorio para compilar. Todos sirven para que el panel y la medición tengan
paridad con 1.x.

1. **`lirtup_proto::StatsSample`** (contrato del revisor), para que el panel tenga el contenido
   de 1.x:
   - `poll_fps: f32`: las capturas por segundo (`Game … (N/s polled)`);
   - `render_ms: Option<f32>`: el tiempo de CPU del render (`Shown … render X ms`);
   - `latency_by_kind: Vec<(FrameKind o String, mean, max)>`: 1.x mostraba
     `real X ms (max Y)  generated …`.

   `Stats` ya junta los tres datos. Si se agregan, solo falta el `match` en `sample()` y en
   `panel_rows`.
2. **`PanelContext`**, que es mío pero lo arma P:
   - el recorte (`crop T B L R`);
   - el modo de escalado (`x2`/`x4`, tiles o cuadro completo);
   - `output_geometry`, el escalador de subida y el de bajada;
   - la lista «Post»;
   - el modo de presentación (`present_on_change_only`);
   - el motor de efectos (Native o MagpieFX).

   Propongo agregarlos como `extra: Vec<(String, String)>`, filas que P ya trae formateadas, para
   no atar a S con la config de P. Si a P le sirve, lo agrego en la integración.
3. **`bench::save_results(path, report)`**: el reporte no trae los tamaños ni el selector `gpu`,
   así que no escribo `sizes` (1.x reiniciaba los casos de un tipo cuando cambiaba el tamaño) ni
   `selectors`. Propongo que `BenchReport` traiga `src` y `dst`, o que `save_results` reciba el
   `BenchRequest` y el selector. K es quien la llama.
4. **`BenchCase::name`**: el doc dice `x2-flow1.0-quality`, pero uso la clave de 1.x
   (`x2 flow 1.0 quality`), para que `bench.json` lo lean las dos versiones. Solo cambia el doc.
