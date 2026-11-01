# Ronda 1.3: plan de trabajo en paralelo

Objetivo: cerrar los pendientes de la ronda 1.2 (generación de cuadros, MagpieFX, medición) en
frentes que corran a la vez, cada uno en su propia rama, y dejar **todas** las pruebas para el
final, en una sola pasada ordenada ([guion de pruebas](round-1.3-test-script.md)).

## Reglas para todos los frentes

- Cada frente trabaja en su propia sesión, worktree y rama desde `main` (`round13/<frente>`).
- **Solo escribe y compila.** Puede correr `python3 setup.py build_ext --inplace` y comprobar
  que los módulos importan. No corre la app, ni pytest, ni pruebas en la GPU: todo eso va al guion
  final. Sí escribe los tests unitarios de lo que agrega, sin correrlos.
- **No toca** archivos compartidos que se integran al final: `src/upscaler/gui/locale/*`
  (`.ts` y `.qm`), `CHANGELOG.md`, la versión (`pyproject.toml`, `__init__.py`) ni `uv.lock`.
  Las cadenas nuevas de la GUI van con `self.tr(...)` como siempre; la traducción se hace en la
  integración.
- Del `README.md` solo edita la sección de su tema.
- Termina con un commit por cambio coherente y una nota en su rama
  (`docs/developer/round-1.3-<frente>.md`) con lo hecho, lo que no se hizo y qué probar.

## Frentes

Los cuatro son independientes: ninguno necesita el resultado de otro ni toca los archivos de
otro (ver la tabla de propiedad). Pueden empezar a la vez.

### A. MagpieFX: texturas `.dds` y FP16

1. **`//!SOURCE` (texturas de consulta).** Leer `.dds` (sin compresión y BC, según lo que usen
   RAVU, SMAA y NIS), subirlas como `Texture2D` y enlazarlas en el pipeline del efecto.
2. **Incluir los 31 efectos** que faltan (26 RAVU, 4 SMAA, NIS) con sus `.dds`, sin modificar,
   bajo la licencia de Magpie; agregarlos como package-data y al `MANIFEST.in`.
3. **FP16** (`//!CAPABILITY FP16`): tipos `MF` a 16 bits cuando el efecto lo pide y la GPU lo
   soporta (`Device.supports_fp16`), con vuelta a 32 bits si no.
4. Documentación: sección *Magpie effects* del README y `docs/user/magpiefx.md`.
5. Tests unitarios: lector de `.dds` (archivos chicos sintéticos), parser de `//!SOURCE`,
   generación de tipos con y sin FP16.

### B. Generación de cuadros y Vulkan

1. **Multiplicador automático** (solo motor integrado; la capa fija el multiplicador al
   arrancar): opción *Auto* que elige ×2 a ×5 según los fps del juego y el refresco del monitor,
   con histéresis para no saltar entre valores. lsfg necesita reiniciar su dispositivo para
   cambiar la cantidad de cuadros generados: hacerlo entre ciclos, sin cortar la imagen. En
   config (`frame_gen_multiplier: auto` o equivalente), CLI, pestaña Presentación y bandeja.
2. **`fifo` sin frenar al pipeline:** hoy `Swapchain.present` puede bloquear hasta el vblank con
   el GIL tomado. Soltar el GIL en las esperas de `vkAcquireNextImageKHR` y de las fences,
   protegiendo el uso de la `VkQueue` con un mutex propio del dispositivo (todos los
   `vkQueueSubmit`, `vkQueuePresentKHR` y `vkQueueWaitIdle`), ya que el GIL deja de alcanzar.
3. **Latencia medible:** marca de tiempo de captura de cada cuadro y momento de su
   presentación; fila *Latency* en el panel de stats y registro opcional a CSV
   (`--latency-log`), para la capa y el motor integrado (con la capa solo se mide hasta que
   lirtup presenta).
4. **`robustImageAccess`:** activarlo en el dispositivo cuando esté disponible.
5. **Aviso de GPU:** no avisar de más con contenido de menos de 30 fps. Por ejemplo, usar como
   presupuesto el intervalo real del juego cuando no se pierden cuadros (comparar cuadros nuevos
   con lo que la ventana dibuja) y el mínimo de 30 fps solo cuando sí.
6. Tests unitarios: elección del multiplicador automático, histéresis, nuevo presupuesto del
   aviso, registro de latencia.

### C. «Medir en mi GPU»

1. Módulo de medición reutilizable: corre cada efecto de MagpieFX y nativo sobre una imagen de
   prueba a la resolución elegida y devuelve ms por cuadro (con timestamps de GPU, como el
   script `scripts/bench-magpie-effects.py`), y mide el costo de lsfg (×2 a ×4, flujo, modelo)
   con el módulo `upscaler.lsfg` directamente (sin tocar `pipeline/native_framegen.py`).
2. Diálogo con progreso y resultados, que reemplaza los tiempos de referencia fijos de la GUI y
   guarda los resultados por GPU.
3. Botón en la pestaña de efectos. Para la sección de generación de cuadros, el frente deja
   lista una función para abrir el diálogo en esa sección; el botón se agrega en la integración
   (la pestaña Presentación es del frente B).
4. Tests unitarios del módulo con la GPU simulada.

### D. Investigación: modelos SRCNN y ports nativos (solo lectura)

1. Comparar arquitecturas de escalado en tiempo real posibles además de CuNNy (calidad,
   costo, licencia, formato de los pesos, esfuerzo de integración) y recomendar una o ninguna.
2. Con los tiempos de referencia ya medidos de MagpieFX, decidir qué efectos valdría la pena
   portar a nativo y por qué (o recomendar no portar más).
3. Resultado: `docs/developer/round-1.3-research.md`, sin cambios de código.

## Propiedad de archivos

| Frente | Toca | No toca |
| --- | --- | --- |
| A | `src/upscaler/magpiefx/**`, `MANIFEST.in`, package-data de `pyproject.toml`, `docs/user/magpiefx.md`, sección Magpie del README, `tests/test_magpiefx_*` | GUI, Vulkan, generación de cuadros |
| B | `src/upscaler/framegen.py`, `pipeline/{native_framegen,stats,pipeline,swapchain}.py`, `config/{models,validators,args}.py`, `vulkan/**`, `gui/sidebars/tabs/presentation.py`, `gui/helpers/tray.py`, `config-example.yaml`, sección Frame generation y Stats del README, `tests/test_framegen*`, `tests/test_native_framegen.py`, `tests/test_stats.py` | MagpieFX, pestaña de efectos |
| C | módulo nuevo de medición, diálogo nuevo en `gui/dialogs/`, `gui/sidebars/tabs/effects.py`, datos de referencia, `tests/test_bench*` | `presentation.py`, `native_framegen.py`, `vulkan/**`, `magpiefx/**` (solo lo usa) |
| D | `docs/developer/round-1.3-research.md` | todo lo demás |

Si un frente descubre que necesita tocar un archivo de otro, no lo hace: lo anota en su nota de
rama para la integración.

## Integración (después de los cuatro frentes)

1. Rama `round13/integration` desde `main`; merge de A, B, C y D, resolviendo conflictos.
2. Botón *Medir en mi GPU* en la sección de generación de cuadros (`presentation.py`).
3. Traducción: todas las cadenas nuevas en `upscale_gui_es.ts` y `upscale_gui_en.ts`, borrar
   las que ya no se usan, compilar los `.qm` con `scripts/compile-translations.fish`.
4. `CHANGELOG.md`, versión 1.3.0, `uv lock` con red (incluye el extra `test`).
5. Compilar las extensiones e instalar con pipx desde la rama de integración.
6. Correr el [guion de pruebas](round-1.3-test-script.md) completo, anotar cada falla en su
   tabla de resultados, corregir todo en una sola pasada y repetir solo lo que falló o lo que
   la corrección pudo afectar.
7. PR de `round13/integration` a `main`.

## Fuera de esta ronda

- Probar en NVIDIA e Intel: no hay ese hardware. Se prueba en la RX 9070 XT, en la GPU integrada
  del Ryzen (RADV Raphael) y en lavapipe (Vulkan por CPU), y queda anotado.
- Portar efectos a nativo o integrar otro modelo SRCNN: se decide con el informe del frente D.
