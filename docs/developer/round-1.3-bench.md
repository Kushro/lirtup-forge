# Ronda 1.3, frente C: «Medir en mi GPU»

Rama del frente: `claude/round13-bench-50f4f0` (desde `main` 1.2.0). Solo se escribió y compiló:
no se corrió la app, ni pytest, ni nada en la GPU. Las extensiones compilan
(`python3 setup.py build_ext --inplace`) y todos los módulos nuevos importan en
`~/.cache/lirtup-test-venv`.

## Lo hecho

### Módulo `src/upscaler/bench.py`

- **Casos:** los 3 efectos nativos (LumaSharpen, Anime4K_Upscale_S, CRT_Easymode), todos los
  efectos de MagpieFX (`list_effects()`, incluidos los del usuario y los que agregue el frente A)
  y 12 casos de lsfg: ×2, ×3, ×4 × flujo 1,0 / 0,5 × Calidad / Rendimiento.
- **Cómo mide:**
  - Efectos: timestamps de Vulkan (`dispatch_sequence(timestamps=True)`), como los scripts
    `bench-magpie-effects.py` y `magpiefx-bench.py`. Imagen de prueba sintética (degradados,
    líneas finas, zonas planas y ruido) subida en BGRA como un cuadro capturado; MagpieFX con el
    cambio de rojo/azul plegado en los shaders, como la app. Leen el tamaño de entrada y los que
    escalan dibujan al tamaño de salida (CRT a la salida, Anime4K ×2), como el presentador.
  - lsfg: corre en su propio VkDevice y no hay timestamps que leer, así que cada ciclo se mide en
    la CPU, desde la copia del cuadro real a la textura compartida hasta que todos los cuadros
    generados están señalizados. Es el trabajo que lsfg agrega por cuadro real, al tamaño de
    salida. Usa `upscaler.lsfg` y `Texture2D.external` / `create_external_semaphore`
    directamente; `native_framegen.py` no se toca. Alterna dos imágenes distintas (la segunda
    desplazada) para que el flujo óptico tenga movimiento.
  - Por caso: 5 cuadros de calentamiento y luego hasta 100 cuadros o 1,5 s (mínimo 10); se
    informa la mediana y el p95. Un caso que falla (no compila, falta un `.dds`, falta
    `Lossless.dll`) queda con su error y la medición sigue.
- **Cancelable:** `Benchmark.cancel()` (seguro desde otro hilo) corta entre cuadros, cierra lo
  que estaba abierto (incluido `lsfg.finalize()`) y devuelve lo ya medido.
- **Separado de la GPU** por un `Backend` (`VulkanBackend` es el real), para los tests.
- **Línea de comandos:** `python -m upscaler.bench [--section effects|framegen] [--effect N]
  [--input-size WxH] [--target-size WxH] [--gpu ID] [--dll RUTA] [--json] [--save]`. Con `--json`
  escribe un evento JSON por línea (`start`, `case`, `result`, `done` / `cancelled` / `error`).
  SIGTERM o Ctrl+C cancelan limpiamente. Sirve también para medir a mano en el guion.
- **Resultados por GPU:** `~/.config/linux-rt-upscaler/bench.json`, con clave
  `vendor:device:nombre`. Por GPU y por tipo (nativo, MagpieFX, generación de cuadros) guarda ms,
  p95, cuadros, error, tamaños y fecha. Medir de nuevo reemplaza el caso; medir a otro tamaño
  descarta los de ese tipo (no son comparables). Además recuerda qué GPU eligió cada valor de
  `gpu` de la config, así la GUI encuentra los resultados sin abrir la GPU
  (`results_for_gpu(config.gpu)`).

### Diálogo `src/upscaler/gui/dialogs/bench.py`

- `BenchDialog`: casillas *Magpie effects* / *Frame generation*, tamaños de entrada y salida,
  barra de progreso, estado («Midiendo X (n de N)…»), tabla de resultados (motor, efecto,
  ms/cuadro, p95; los fallidos dicen *failed* con el error en el tooltip) y botones Iniciar /
  Cancelar / Cerrar. Al abrir muestra lo guardado para esa GPU.
- La medición corre en **otro proceso** (`QProcess` con `python -m upscaler.bench --json
  --save`): la GUI nunca espera a la GPU, cancelar es SIGTERM (y SIGKILL si en 5 s no terminó),
  y el estado global de lsfg (`g_initialized`) nunca se mezcla con el de una sesión, que vive
  en el proceso de la GUI. El proceso hijo lleva `DISABLE_LSFGVK=1` y sin `LSFGVK_CONFIG`.
- **Sesión activa:** si hay una sesión manual o el modo daemon está activo, Iniciar queda
  deshabilitado con un aviso; se revisa cada segundo. Nunca toca la sesión.
- Si la generación integrada no está compilada o no se encuentra `Lossless.dll` (la de la
  config o la de Steam), la casilla de generación de cuadros queda deshabilitada con el motivo.
- Cancelar guarda los casos ya medidos.

### Pestaña de efectos

- Botón **«Measure on my GPU»** (bajo el texto del motor, visible con los dos motores).
- La tabla Nativo / MagpieFX muestra los tiempos medidos de la GPU de la config, con el nombre de
  la GPU y los tamaños; los tiempos fijos de referencia (`_REFERENCE_MS`) quedan solo hasta que
  se mida. Un efecto sin medir muestra «–».
- En la cadena de MagpieFX, cada efecto medido muestra su tiempo junto al nombre.
- Después de medir, la pestaña se refresca sola.

## Para la integración: sección de generación de cuadros

`presentation.py` es del frente B, así que no se tocó. Para agregar el botón:

```python
from ...dialogs import open_framegen_bench
from .... import bench

btn = QPushButton(self.tr("Measure on my GPU", "Benchmark button"))
btn.clicked.connect(
    lambda: open_framegen_bench(self._gui_config, self._config, self)
    and self._refresh_fg_times()  # opcional: devuelve True si se guardaron resultados
)
```

- `open_framegen_bench(gui_config, config, parent)` abre el diálogo con la sección de generación
  de cuadros marcada (usa `config.gpu` y `config.frame_gen_dll`) y devuelve `True` si se
  guardaron resultados. Es `open_bench_dialog(..., bench.SECTION_FRAMEGEN, ...)`.
- Para mostrar el costo de la configuración elegida:
  `bench.stored_framegen_ms(bench.results_for_gpu(config.gpu), multiplier, flow_scale,
  performance)` → ms por cuadro real o `None`. Con *Auto* (frente B) se puede mostrar el rango
  ×2 a ×4.
- `parent` debe estar dentro de la ventana principal: el diálogo busca `manual_session` y
  `daemon_ctrl.active` en `parent.window()` para saber si hay una sesión.

## Lo que quedó afuera

- **Traducciones:** las cadenas nuevas van con `self.tr(...)` en los contextos `BenchDialog` y
  `EffectsTab` (botón, casillas, estados, avisos, encabezados de la tabla, el nuevo pie de
  la tabla «GPU time in ms/frame measured on {0} ({1} to {2})»). Hay que agregarlas a los `.ts`
  en la integración. Los tests de `test_bench_dialog.py` buscan los textos en inglés (sin
  traductor cargado).
- **Medir sin sesión pero con el modo daemon activo** también se bloquea (el daemon puede estar
  escalando); si molesta, se puede limitar a cuando el daemon tiene ventana.
- **Costo de lsfg en ms de CPU:** incluye la sincronización entre los dos dispositivos (una
  espera de semáforo por cuadro generado), así que es una cota superior del trabajo de GPU. No
  hay forma de leer timestamps del dispositivo de lsfg sin tocar el módulo C++.
- lsfg se inicializa y finaliza 12 veces seguidas en el mismo proceso; en la app se hace una
  vez por sesión. Si el núcleo no tolera reinicializarse (el frente B lo necesita también para
  *Auto*), los casos siguientes fallarán con su error: ver T4.3.
- No hay botón en la bandeja ni en la CLI de `upscale`; la medición a mano es
  `python -m upscaler.bench`.
- Los efectos nativos que no son de Magpie (CAS, bloom, deband, viñeta, grano, LUT) no se
  miden: la GUI no muestra tiempos de esos.

## Archivos

- Nuevos: `src/upscaler/bench.py`, `src/upscaler/gui/dialogs/bench.py`,
  `tests/test_bench.py`, `tests/test_bench_dialog.py`, esta nota.
- Modificados: `src/upscaler/gui/dialogs/__init__.py` (exporta `BenchDialog`,
  `open_bench_dialog`, `open_framegen_bench`), `src/upscaler/gui/sidebars/tabs/effects.py`.
- No se tocaron archivos de otros frentes.

## Qué probar (además de T4.1 a T4.6 del guion)

1. `pytest tests/test_bench.py tests/test_bench_dialog.py` (T1.1/T1.2).
2. A mano, antes de la GUI: `python -m upscaler.bench --section effects --effect
   Anime4K_Upscale_S` y `python -m upscaler.bench --section framegen`; comparar los efectos con
   `scripts/magpiefx-bench.py` y `scripts/bench-magpie-effects.py` (T4.2: < 10 %).
3. T4.3: que los 12 casos de lsfg terminen (reinicializar lsfg en un proceso); ×3/×4 más caros
   que ×2, flujo 0,5 y Rendimiento más baratos.
4. T4.5: cancelar durante la compilación de un efecto (primera vez, sin caché) y durante lsfg;
   la GUI no se cuelga y el estado dice «Cancelled».
5. T4.6: con una sesión abierta, el botón Iniciar queda deshabilitado con el aviso; al cerrarla
   se habilita en ~1 s.
6. Medir con `--gpu` en la GPU integrada y en lavapipe: los resultados quedan separados por GPU
   y la pestaña muestra los de la GPU elegida en la config.
7. Cerrar el diálogo en mitad de una medición: tarda como mucho ~3 s y no deja procesos
   (`pgrep -f upscaler.bench`).
8. El texto de la pestaña de efectos en español después de traducir (T1.3).
