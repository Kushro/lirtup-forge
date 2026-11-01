# Ronda 2, frente F: generación de cuadros en el pipeline

Rama **`claude/rust-r2-framegen-f4e51e`**, que sale de `rust/r2-skeleton`. Fecha: 2026-11-01.
Sin push y sin merge.

`cargo check -p lirtup-fg --all-targets` y `cargo check -p lirtup-pipeline --all-targets`
(rustc 1.99.1): **pasan, sin avisos**. `cargo clippy` sobre los dos crates tampoco da avisos.
Los tests están escritos y **no se corrieron**.

## Commits

1. `lirtup-fg`: `EngineStart`, `EngineOptions::from_config` y `AutoMultiplier::chosen`.
2. `lirtup-pipeline::pacing`: `FrameInterval` y `FramePacer`.
3. `lirtup-pipeline::framegen`: `FrameGen` y `status_from_setup`.

## Qué hice

### `lirtup-fg`

- **`native::EngineStart`**: arranca o reinicia lsfg en un hilo propio (`lsfg-restart`), como
  `_begin_restart` y `_restart_lsfg` de 1.x:
  - `spawn(previous, options)`: finaliza el motor anterior y luego inicializa el nuevo. Antes
    comprueba que el `Arc` anterior sea el último, o sea, que no quede ningún contexto vivo.
  - `poll()`: no bloquea.
  - `wait()`: bloquea hasta que el hilo termina. `Drop` también hace `join`, porque lsfg no se
    puede finalizar mientras se inicializa.
- **`EngineOptions::from_config`**: arma las opciones con el flujo y el modo de rendimiento de la
  configuración, más la GPU.
- **`AutoMultiplier::chosen()`**: devuelve `None` hasta la primera elección. El motor espera esa
  elección antes de arrancar lsfg.
- `lirtup-fg` sigue sin depender de `lirtup-gpu`. La parte de la GPU (imágenes, semáforos y
  presents) vive en `lirtup-pipeline`.

### `lirtup-pipeline::pacing`

Lo cambié de forma, porque es interno de F: ahora son **dos mitades**, una por hilo, y así no
comparten estado.

- **`FrameInterval`** (hilo de la sesión): estima el intervalo del juego igual que
  `_update_interval`.
  - Usa los últimos 30 intervalos, recortados entre 1/240 y 1/15 s.
  - El resultado es el tiempo total dividido por los cuadros del juego. Un intervalo de 1,75
    medianas o más cuenta como varios cuadros.
  - Arranca en 1/60 s, como 1.x. No usa el refresco: es una estimación del juego, no del monitor.
- **`FramePacer`** (hilo `lsfg-present`): encadena cada ciclo al real anterior, un paso después.
  - Si el cuadro llega tarde, la cadena se reinicia medio paso más tarde, como margen.
  - Métodos: `schedule(generated, interval, ready)` devuelve los `generated + 1` instantes;
    `reset()` y `real_presented()` completan la API.
- **`sleep_until`**: duerme con `thread::sleep` y gira los últimos 200 µs.
- **Constantes**: `MIN_INTERVAL`, `MAX_INTERVAL` e `INITIAL_INTERVAL`.

### `lirtup-pipeline::framegen`

Es `native_framegen.py` llevado a Rust. El doc del módulo tiene el detalle. Lo principal:

- **`FrameGen::new`**:
  - Devuelve `None` con la capa, con la FG apagada o con un estado de `LayerSetup` distinto de
    `Active`.
  - Sin `supports_external_fd`, o sin `Lossless.dll`, manda `Event::FrameGen` con `NoNative` y
    su `reason`, y devuelve `None`.
  - Si arranca, manda un `Event::FrameGen` inicial.
- **`present`**, en el hilo de la sesión:
  1. Mide el intervalo.
  2. Espera el ciclo anterior (`drain`, 1 s como máximo; si se pasa, el hilo está trabado).
  3. Sigue a *Auto*: espera la elección después de 15 cuadros y, cuando la elección cambia,
     reinicia lsfg en segundo plano.
  4. Arranca o recoge lsfg.
  5. Copia la textura a la entrada `k % 2`. La copia señala el semáforo de entrada.
  6. Llama a `LsfgContext::present` con fds nuevos.
  7. Le pasa el ciclo al hilo `lsfg-present`.

  Mientras lsfg arranca o reinicia, presenta el cuadro real directamente y lo informa con
  multiplicador 1.
- **Hilo `lsfg-present`**:
  - Espera cada semáforo de salida en la CPU con un envío vacío que espera el semáforo y su
    fence, con 500 ms como máximo (el `wait_semaphore` de 1.x).
  - Presenta los generados y después el real, en los instantes de `FramePacer`.
  - El primer ciclo de un contexto espera todos los semáforos, pero muestra solo el real.
  - Cada present toma el lock de la `SharedSurface` solo durante ese present.
  - Un `OutOfDate` pierde el cuadro, y no se informa.
- **Objetos compartidos** (`Shared`):
  - dos entradas, `generated` salidas y sus semáforos, todos en `R8G8B8A8_UNORM` (como 1.x), con
    el uso por defecto de `lirtup-gpu`;
  - el `LsfgContext` va primero en los campos, así se borra en lsfg antes de soltar las imágenes;
  - un cuadro de otro tamaño crea objetos nuevos y un contexto nuevo.
- **Errores**: cualquier error (de lsfg, de la GPU o del hilo) pasa a `fail`. `fail` cierra todo,
  deja el estado en `NoNative` con `reason` y manda el evento. La presentación sigue, solo con
  cuadros reales.
- **`update_config`**:
  - Si la FG se apaga, o el motor pasa a la capa, cierra lsfg (estado `Off`). Si después vuelve a
    prenderse, arranca de nuevo.
  - Si cambian los cuadros generados, el flujo o el modo de rendimiento, reinicia lsfg en segundo
    plano. Esos tres valores los fija `lsfg_initialize`.
  - Si se pasa de fijo a *Auto*, sigue con el multiplicador actual.
  - Después de un `NoNative` queda apagada hasta la sesión siguiente, como 1.x.
- **`status`**: `active` es el multiplicador objetivo mientras la FG está prendida, también
  durante un reinicio. Así la GUI no parpadea a «apagado».
- **`active_multiplier`**: solo devuelve un valor cuando lsfg corre. Es lo que va a las stats.
- **`close`** y **`Drop`**: los dos cierran igual.
  1. Paran el hilo. Si no termina en 2 s, lo dejan atrás; solo tiene `Arc`s.
  2. Esperan la copia pendiente.
  3. Sueltan el contexto.
  4. Esperan un `EngineStart` en curso.
  5. Finalizan lsfg.
- **`status_from_setup`**:
  - toma el estado, el multiplicador de `LayerSetup`, la capa instalada y las DLL encontradas;
  - `env_locked` vale si el estado es `External` o si `external_env()` da verdadero;
  - `native_available` aquí solo dice si está `Lossless.dll`. El soporte de fd se conoce cuando
    una sesión abre el dispositivo, y entonces `FrameGen::new` manda `NoNative` si falta.

### Tests (escritos, sin correr)

- **`pacing`**: ritmo constante, el temblor del sondeo a 60 fps y 144 Hz, cuadros perdidos,
  recortes, historia acotada, mediana, encadenado, cadena reiniciada con margen, `reset` y
  `sleep_until`.
- **`framegen`**: multiplicadores configurados, estados del protocolo, motivos,
  `status_from_setup` y comparación de `Settings`.
  - Además, uno con `#[ignore]`: **`lsfg_generates_with_lirtup_gpu_fds`**. Inicializa lsfg,
    crea un contexto con fds reales de `lirtup-gpu` y genera un cuadro entre uno rojo y uno azul.
    Pide que el píxel central no sea negro y que la capa de validación no diga nada. Necesita una
    GPU y `Lossless.dll` (o `LIRTUP_LOSSLESS_DLL`). Es el pendiente de la ronda 1 «motor
    integrado con fds reales», sin ventana.
- **`lirtup-fg`**: `from_config` recorta el flujo, `EngineStart` informa el error de una
  inicialización fallida (`generated` 0) y `chosen()`.

## Diferencias con 1.x (a propósito)

- **La primera inicialización de lsfg también va en segundo plano**, no solo los reinicios. Así
  el primer cuadro no se traba mientras lsfg carga los shaders; los primeros cuadros salen solo
  reales. 1.x la hacía en el hilo del pipeline.
- **Los presents directos se informan con multiplicador 1**, porque es el que está en efecto.
  1.x informaba el configurado.
- **Los motivos (`reason`) están en inglés**, como el resto de los mensajes del núcleo. La GUI
  traduce por `state`; si quiere mostrar el motivo traducido, hace falta un código en vez de
  texto (ver los pedidos).

## Pendiente

- **Correr los tests**: `cargo test -p lirtup-fg -p lirtup-pipeline`, y el ignorado:

  ```bash
  cargo test -p lirtup-pipeline lsfg_generates_with_lirtup_gpu_fds -- --ignored --nocapture
  ```

- **Criterio de 5.8**: FG por motor integrado a ×2 y ×4, con video y sin errores de validación,
  sobre la ventana. Necesita P (el presentador) y K (la superposición y el swapchain). Lo que hay
  que mirar ahí:
  - el ritmo con `fifo` y `mailbox` (desvío y fps, contra la tabla de la ronda 1.3);
  - la competencia por el `Mutex` de la `SharedSurface` entre los dos hilos;
  - la latencia de los cuadros reales y de los generados en el CSV de S;
  - el cambio de *Auto* de 30 a 60 fps: tiene que reiniciar una sola vez y sin huecos.
- **FG por capa**: no pasa por este código. Es `layer::prepare`, que llama K, más el swapchain.
  Se prueba en la integración.
- **Visibilidad de memoria entre dispositivos**: el real se presenta en un envío posterior al que
  esperó el semáforo, y la CPU esperó el fence en el medio. Es lo mismo que hacía 1.x y anduvo.
  Si la capa de validación se queja, la alternativa es presentar con `PresentOptions::wait`, pero
  el ritmo necesita saber en la CPU cuándo está listo el primer generado.

## Lo que necesita P (sin cambio de firma)

- Después de `FrameGen::present`, mirar `PresentSurface::needs_recreation()`: `present` devuelve
  `Ok(())` aunque el swapchain esté `OutOfDate`, porque los presents del hilo no pueden avisar
  por el valor de retorno.
- Si `FrameGen::new` devolvió `None` porque la FG estaba apagada y un `ReloadConfig` la prende,
  hay que llamar a `FrameGen::new` otra vez. `FrameGen::update_config` cubre apagar y volver a
  prender, pero solo si el `FrameGen` existe.
- La textura que recibe `present` tiene que ser de 4 bytes por texel y del tamaño del swapchain.
  Se presenta en (0, 0), sin fondo.

## Dependencias que pido

Ninguna.

## Cambios de firma que pido

Ninguno. No toqué `lsfg-sys` ni su API.

Opcional, para la ronda 3: un código de motivo en `FrameGenStatus`, para que la GUI traduzca
`NoNative`. Hoy los motivos posibles son tres: sin fd externo, sin DLL y un error de lsfg o del
hilo. Hoy solo va `reason: Option<String>`.
