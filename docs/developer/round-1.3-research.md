# Ronda 1.3, frente D: modelos de escalado y ports nativos

Investigación de solo lectura, sin cambios de código. Responde las dos preguntas del
[plan de la ronda](round-1.3-plan.md): si conviene sumar otra arquitectura de escalado además de
CuNNy, y qué efectos de Magpie valdría portar al motor Native.

Cada dato lleva una marca:

- **[medido]**: medido en este proyecto, en la RX 9070 XT con RADV. Viene de
  [magpiefx-report.md](magpiefx-report.md), del README (sección *Magpie effects*) o de
  [frame-generation-analysis.md](frame-generation-analysis.md).
- **[verificado]**: comprobado en la fuente externa enlazada, o en el código o la configuración de
  este repositorio, el 2026-11-01.
- **[estimado]**: cálculo o juicio propio, sin medir. Hay que confirmarlo con el diálogo «Medir en
  mi GPU» del frente C o con el guion de pruebas final.

## Resumen

1. **Modelos:** no recomiendo integrar otra arquitectura en `srcnn/`. En la única comparación
   publicada con las mismas condiciones para todos los modelos, CuNNy da la mejor calidad por
   milisegundo en arte 2D y novelas visuales. ArtCNN es el único rival en calidad y cuesta de 2,5
   a 3 veces más para la misma puntuación. Los demás modelos (Anime4K, FSRCNNX, ACNet,
   k7/ESRGAN, NNEDI3) ya corren hoy por MagpieFX, sin trabajo nuevo. Lo que sí vale la pena, y es
   barato, es sumar las **variantes DS y SOFT de CuNNy**. Son la misma arquitectura que ya usa el
   pipeline y vienen entrenadas para quitar ruido y afilar, o para suavizar y quitar el
   aliasing.
2. **Ports nativos:** no recomiendo portar más efectos. El motor Native y MagpieFX miden lo mismo
   (diferencias de ±0,004 ms) porque compilan el mismo HLSL con el mismo dxc. Las ganancias de
   robustez que justificaron los primeros ports las dan ahora mejoras generales del motor
   MagpieFX: `robustImageAccess` (frente B), FP16 (frente A) y, propuesto acá, SPIR-V
   precompilado para los efectos incluidos. Esas mejoras sirven para los 86 efectos (117 con los
   `.dds`), no solo para uno.
3. **La cadena de 35 ms:** según mi estimación, casi todo el costo sale de **dónde** corre
   `Anime4K_Restore_Soft_UL` (a unos 5136×2972), no de cómo está implementado. Un port no lo
   cambia. Las palancas reales son FP16 (frente A) y la resolución a la que corre cada efecto
   (decisión de la persona). Detalle en la [parte 2](#la-cadena-anime4k-ul-de-la-persona).

---

## Parte 1: modelos de escalado

### Cómo entra un modelo hoy

Revisado en `src/upscaler/srcnn/` **[verificado]**:

- `loader.py` lee `model.json` (`passes`, `num_textures`, `srv_uav`, `samplers`, `scale`,
  `depth` ∈ {`rgba8`, `rgba16`}) y un `PassN.spv` por pasada (más `PassN_tile.spv` para el modo
  por mosaicos). Los pesos no se cargan aparte: van como constantes dentro del HLSL de cada
  pasada, compilado con dxc a SPIR-V y versionado en el repo.
- `factory.py` crea un `Compute` por pasada con un cbuffer fijo (`in/out` tamaño y recíprocos).
  Supone que todas las pasadas intermedias tienen el tamaño de la entrada y que **la última
  produce 2×** (`out = in * 2`). `srcnn.py` despacha grupos de 8×8 en las intermedias y de 16×16
  píxeles de salida (8×8 hilos × 2×2) en la última.
- `cunny/converter.py` convierte un `.hlsl` de Magpie de CuNNy al formato de arriba (normal y
  `_tile`). Los 9 modelos incluidos son todos la variante **NVL** (`model.json` → `name`).

Consecuencia: un modelo entra «gratis» si es una CNN con convoluciones a resolución de entrada y
un depth-to-space 2× al final, y si existe como `.hlsl` de Magpie (o en un formato que el
conversor entienda). Si trabaja a otra resolución (ArtCNN, por ejemplo, guarda sus capas
intermedias a 2×, ver abajo), usa texturas `//!SOURCE`, o solo existe como shader de mpv o
libplacebo (`//!HOOK`), hace falta un conversor nuevo o tocar `factory.py`.

### Candidatos

Calidad: tabla de [CuNNy `results/`](https://github.com/funnyplanter/CuNNy/tree/master/results)
**[verificado]**. Es el único banco que encontré que mide CuNNy, ArtCNN, Anime4K, FSRCNNX y RAVU
con el mismo método: imagen de novela visual (`aoko.png`) en luma, reducida a la mitad con box en
luz gamma y escalada 2× en mpv; la «Mean» promedia MAE, PSNR, SSIM, MS-SSIM y LPIPS
normalizados. Costo: tiempos del mismo README (RTX 4090, 1080p → 4K, «a ojo», según el propio
autor). Ojo con dos sesgos: lo hizo el autor de CuNNy, con un dato del dominio en el que CuNNy-NVL
está entrenado, y algunas filas son versiones viejas de CuNNy (1x8, 2x8, 3x8) que ya no se
publican. La [comparación de Artoriuz](https://artoriuz.github.io/blog/mpv_upscaling.html) (autor
de ArtCNN) pone a ArtCNN por encima de RAVU, pero no incluye CuNNy, Anime4K ni FSRCNNX
**[verificado]**.

| Modelo | Calidad («Mean», aoko) | Costo 1080p→4K en 4090 | Costo en este proyecto (1280×720 → 2×, una corrida) | Licencia | Formato disponible | Ya corre en lirtup |
|---|---|---|---|---|---|---|
| CuNNy 8x32 (fp16) | 1,000 | 5,6 ms | 7,48 ms [medido] | LGPL-3.0 | Magpie HLSL, mpv GLSL, pickle | Sí, `srcnn` y MagpieFX |
| CuNNy 4x32 | 0,974 | 2,35 ms | 3,99 ms [medido] | LGPL-3.0 | ídem | Sí |
| ArtCNN C4F32 | 0,973 | 7,2 ms | 28,8 ms (`Ani4Kv2_ArtCNN_C4F32_i2`, otros pesos) [medido] | MIT (arquitectura y pesos oficiales) | mpv GLSL, ONNX | Solo la variante Ani4Kv2, por MagpieFX |
| CuNNy 4x16 | 0,931 | 0,77 ms | 1,08 ms [medido] | LGPL-3.0 | ídem | Sí |
| ArtCNN C4F16 | 0,920 | 1,97 ms | — | MIT | mpv GLSL, ONNX | No |
| CuNNy 4x12 | 0,908 | 0,56 ms | 0,67 ms [medido] | LGPL-3.0 | ídem | Sí |
| Anime4K Upscale UL | 0,780 | 2,65 ms | 1,64 ms [medido] | MIT | mpv GLSL; Magpie HLSL | Sí, MagpieFX |
| Anime4K Upscale VL | 0,764 | 1,70 ms | 0,86 ms [medido] | MIT | ídem | Sí, MagpieFX |
| FSRCNNX 16-0-4-1 / 8-0-4-1 | 0,750 / 0,685 | 2,36 / 0,60 ms | 0,355 ms (8-0-4-1) [medido] | GPL-3.0 | mpv GLSL; Magpie HLSL (8-0-4-1) | 8-0-4-1, por MagpieFX |
| RAVU lite-ar r4 | 0,659 | 0,18 ms | — (necesita `.dds`, frente A) | LGPL-3.0 | mpv GLSL; Magpie HLSL + `.dds` | Tras el frente A |
| ACNet | — | — | 0,515 ms [medido] | MIT | Magpie HLSL | Sí, MagpieFX |
| k7_modernAnime_FHD_x2 (ESRNET) | — | — | 1,62 ms [medido] | sin licencia declarada en el repo de origen | Magpie HLSL | Sí, MagpieFX |
| NNEDI3 nns16–256 | — | — | 0,45–8,8 ms [medido] | LGPL | Magpie HLSL | Sí, MagpieFX |
| Real-ESRGAN animevideov3 (SRVGGNetCompact) | — | — | — | BSD-3-Clause | PyTorch, ncnn | No |

Fuentes de licencia (`gh api repos/<repo>`, campo `license`) **[verificado]**:
[CuNNy](https://github.com/funnyplanter/CuNNy) LGPL-3.0,
[ArtCNN](https://github.com/Artoriuz/ArtCNN) MIT,
[Anime4K](https://github.com/bloc97/Anime4K) MIT,
[FSRCNN-TensorFlow](https://github.com/igv/FSRCNN-TensorFlow) GPL-3.0,
[mpv-prescalers (RAVU)](https://github.com/bjin/mpv-prescalers) LGPL-3.0,
[ACNetGLSL](https://github.com/TianZerL/ACNetGLSL) MIT,
[mpv_PlayKit (k7)](https://github.com/hooke007/mpv_PlayKit) sin licencia detectada,
[Real-ESRGAN](https://github.com/xinntao/Real-ESRGAN) BSD-3-Clause. Todas son compatibles con la
GPL-3.0 de este proyecto, salvo los casos de abajo.

La columna «en este proyecto» sale de la tabla «Every Magpie effect» de
[magpiefx-report.md](magpiefx-report.md): una sola corrida, FP32, entrada de 1280×720
**[medido]**. Sirve para ordenar por costo, no como cifra precisa.

#### Notas por candidato

- **ArtCNN** ([repo](https://github.com/Artoriuz/ArtCNN)) **[verificado]**: arquitecturas C4F16 y
  C4F32 para tiempo real; R8F64 y R16F96, según el propio autor, no son para tiempo real.
  Variantes `DS` (quita ruido y afila), `DN` (quita ruido y suaviza), de croma y de JPEG.
  Publica GLSL para mpv (`vo=gpu-next`) y ONNX; no hay HLSL de Magpie oficial. El repo se movió el
  2026-10-22, así que sigue activo.
  - Formato: en `ArtCNN_C4F16.glsl` cada capa es un `//!HOOK LUMA` con `//!COMPUTE 24 32 12 16` y
    `//!WIDTH LUMA.w 2.0 *`, o sea que las capas intermedias se guardan al doble de resolución
    **[verificado]**. No calza con `factory.py` (intermedias a tamaño de entrada) ni con
    `cunny/converter.py`.
  - Integrarlo exige un conversor libplacebo → HLSL o un cargador de shaders de mpv (ver abajo).
    Esfuerzo **[estimado]**: medio (2 a 4 días).
  - En la tabla de CuNNy, C4F16 da 0,920 por 1,97 ms y CuNNy 4x16 da 0,931 por 0,77 ms; C4F32
    da 0,973 por 7,2 ms y CuNNy 4x32 da 0,974 por 2,35 ms. A igual calidad, ArtCNN cuesta 2,5 a 3
    veces más. Lo confirma la variante de Magpie medida acá: Ani4Kv2 ArtCNN C4F32 cuesta 28,8 ms
    contra 3,99 ms de CuNNy 4x32 **[medido]**.
  - Ojo: los pesos de `Ani4Kv2_ArtCNN_C4F32_i2.hlsl`, que ya incluimos, son de Sirosky bajo
    **CC BY-NC 4.0**, según la cabecera del archivo **[verificado]**. La cláusula no comercial
    no es compatible con la GPL-3.0 del resto del paquete. Lo anoto como pendiente para la
    integración (sacarlo del paquete o dejarlo como efecto que la persona agrega a mano), fuera
    del alcance de este frente.
- **Anime4K CNN** ([repo](https://github.com/bloc97/Anime4K)): MIT; sin cambios desde 2024-08
  **[verificado]**. Ya corren todas las variantes por MagpieFX **[medido]**. Como «modelo
  SRCNN» puntúa por debajo de CuNNy 4x16 y cuesta más (tabla), aunque esas métricas castigan
  su estilo de bordes marcados, que es justo lo que algunas personas buscan. No hay motivo para
  meterlo en `srcnn/`: ya está disponible.
- **FSRCNNX** ([repo](https://github.com/igv/FSRCNN-TensorFlow)): GPL-3.0; sin cambios desde 2021.
  Puntúa por debajo de CuNNy 4x12 y la versión 8-0-4-1 ya corre por MagpieFX. Descartado.
- **RAVU** ([mpv-prescalers](https://github.com/bjin/mpv-prescalers)): muy barato (0,14 a 0,18 ms
  en la 4090), con calidad intermedia. Llega con el lector de `.dds` del frente A, sin trabajo
  extra. Puede servir en GPU integradas.
- **Modelos de mpv y libplacebo:** libplacebo
  ([repo](https://github.com/haasn/libplacebo), LGPL-2.1) no trae modelos propios; ejecuta
  *user shaders* (`//!HOOK`/`//!BIND`/`//!SAVE`/`//!COMPUTE`). ArtCNN, CuNNy (incluidas DS, SOFT y
  `dp4a`), RAVU, NNEDI3, FSRCNNX y Anime4K se publican en ese formato. Un **cargador de shaders
  de mpv**, hermano de MagpieFX, abriría todos de una vez: GLSL compilado con glslang, un
  encabezado que defina `HOOKED_tex`, `*_pos`, `*_pt`, `*_size`, `imageStore(out_image, …)` y
  el plano `LUMA` sobre el RGB capturado. Esfuerzo **[estimado]**: alto (1 a 2 semanas, parecido
  al cargador MagpieFX, más el problema de luma y croma: casi todos esos modelos escalan solo
  luma). No lo recomiendo ahora: lo único que aportaría que no esté ya en MagpieFX es ArtCNN, más
  caro, y las variantes de CuNNy, que tienen un camino más corto (abajo).
- **Real-ESRGAN animevideov3 / SRVGGNetCompact:** son 16 convoluciones de 64 canales a resolución
  de entrada **[estimado a partir de la arquitectura publicada en el
  [repo](https://github.com/xinntao/Real-ESRGAN)]**. Del orden de 50 veces CuNNy 4x16 en
  operaciones, muy lejos de tiempo real a 1440p. Descartado.
- **CuNNy, variantes que no tenemos** ([README](https://github.com/funnyplanter/CuNNy))
  **[verificado]**: además de NVL, CuNNy publica **DS** (quita ruido y afila) y **SOFT** (suaviza
  y quita aliasing; según el autor, la variante con menos artefactos), en tamaños de veryfast a
  8x32, y versiones `dp4a` cuantizadas a 8 bits, más rápidas en hardware con producto punto
  entero. Pero en `magpie/normal/` solo hay NVL, y `pretrained/` solo trae los `.pickle` de NVL;
  DS y SOFT existen solo como GLSL de mpv (`mpv/ds/`, `mpv/soft/`). El repo no se mueve desde
  2025-08 **[verificado]**.

### Recomendación de la parte 1

**No integrar ninguna arquitectura nueva en `srcnn/`.** CuNNy sigue siendo la mejor relación
calidad/costo para novelas visuales e ilustración. ArtCNN, el único que lo iguala, cuesta 2,5 a 3
veces más y pide un conversor nuevo. El resto ya corre por MagpieFX o es demasiado caro.

**Sí: sumar CuNNy DS y SOFT** (prioridad media, frente propio en otra ronda). Es la misma
arquitectura, así que `loader.py`, `factory.py` y el formato `model.json` no cambian, y las
variantes responden a lo que la persona busca hoy con Anime4K (quitar ruido y suavizar). Camino
**[estimado]**:

1. Extraer los pesos de los `.glsl` de `mpv/ds/` y `mpv/soft/` (constantes literales por capa) a
   la estructura que espera `magpie.py` de CuNNy. Otra opción: pedirle al autor los `.pickle`.
2. Generar el HLSL de Magpie con el `magpie.py` del repo de CuNNy y pasarlo por
   `cunny/converter.py` como los NVL, compilar los `.spv` (normal y `_tile`) y agregar los
   nombres al selector de modelo.
3. Esfuerzo: 1 a 2 días, casi todo en el extractor y en verificar la salida contra mpv con la
   misma imagen.

Las versiones `dp4a` quedan para después: necesitan `VK_KHR_shader_integer_dot_product` y un
conversor distinto, y la ganancia publicada es moderada (CuNNy 4x32: 2,35 ms en dp4a contra
3,05 ms en fp16 en la 4090 **[verificado]**).

---

## Parte 2: ports nativos

### Lo que ya está medido

| Efecto (1280×720 → 2560×1440) | Native | MagpieFX |
|---|---|---|
| LumaSharpen | 0,016 ms | 0,016 ms |
| Anime4K_Upscale_S | 0,145 ms | 0,142 ms |
| CRT_Easymode | 0,122 ms | 0,118 ms |

**[medido]**: README, sección *Magpie effects*. En la app (captura a 2560×1440), MagpieFX con el
cambio de rojo y azul hecho dentro de los shaders mide 0,067, 0,615 y 0,139 ms, y Native 0,064,
0,566 y 0,132 ms ([magpiefx-report.md](magpiefx-report.md) y nota del camino A). La diferencia
entra en el ±10 % que varían las sesiones de la app.

Es lo esperable: el port nativo copia el mismo cuerpo HLSL de Magpie y lo compila con el mismo
dxc. Solo cambian el envoltorio (cbuffer, `main`) y las comprobaciones de límites **[verificado]**
en la nota del camino A. **Un port no da rendimiento por sí mismo.**

### Qué ganaba un port, y quién lo da ahora

| Ganancia de un port nativo | ¿Sigue haciendo falta? |
|---|---|
| Escrituras fuera de rango controladas (Anime4K escribe bloques 2×2 sin chequeo) | No: `robustImageAccess` (frente B, punto 4) lo define para todos los efectos en todos los drivers. |
| Arranque instantáneo, sin dxc ni glslang instalados | Parcial: la caché lo resuelve desde el segundo arranque (1-2 ms) **[medido]**. Para el primero y para sistemas sin dxc, propongo **SPIR-V precompilado para los efectos incluidos** (abajo): sirve para todos y no solo para los portados. |
| FP16 | No viene de un port: el frente A agrega `//!CAPABILITY FP16` al cargador y alcanza a todos los efectos que lo declaran. Los dos UL de la persona lo declaran **[verificado]**. |
| Controles afinados en la GUI | Es trabajo de interfaz, no de shader. Se puede hacer sobre los parámetros de MagpieFX sin portar. |
| Pasar de 1 a 0 pasadas SwapRB | Ya está: el cambio de rojo y azul va dentro de los shaders de MagpieFX, sin costo medible, salvo los 3 FXAA **[medido]**. |

El costo de mantener un port no es menor: el camino A agregó 1872 líneas para tres efectos, con
17 parámetros de CRT repetidos en cinco lugares (config, validadores, CLI, YAML, presentador)
**[medido]** en la nota del camino A. Cada efecto portado además se separa del original: si
Magpie corrige algo, hay que portarlo de nuevo.

### La cadena Anime4K UL de la persona

Configuración actual (`~/.config/linux-rt-upscaler/config.yaml`) **[verificado]**:
`effects_engine: magpiefx`, `Anime4K_Upscale_Denoise_UL` → `Anime4K_Restore_Soft_UL` →
`Lanczos`, sin `model:`. Toma entonces el modelo por defecto, `fast` (`config/models.py`,
`model: str = "fast"`), que es CuNNy 2× **[verificado]**. La cadena MagpieFX corre **después**
del modelo ([docs/user/magpiefx.md](../user/magpiefx.md)).

Medición en el juego: 34,5 ms de GPU para los efectos, con G-senjou no Maou a 1284×743
([frame-generation-analysis.md](frame-generation-analysis.md)) **[medido]**. Para explicar ese
número, escalo los tiempos de la tabla de 1280×720 por cantidad de píxeles **[estimado]**:

| Paso | Resolución de entrada | Píxeles / 720p | Tiempo a 720p [medido] | Estimado |
|---|---|---|---|---|
| CuNNy `fast` 2× | 1284×743 | 1,04 | 0,32 ms | ~0,3 ms (va aparte de los efectos) |
| Anime4K_Upscale_Denoise_UL 2× | 2568×1486 | 4,14 | 1,648 ms | ~6,8 ms |
| Anime4K_Restore_Soft_UL | 5136×2972 | 16,6 | 1,881 ms | ~31 ms |
| Lanczos → pantalla 2560×1440 | 5136×2972 | — | — | < 1 ms |
| **Total efectos** | | | | **~38 ms** (medido: 34,5) |

La estimación cae cerca de lo medido, así que la hipótesis es firme pero no está probada:
**cerca del 80 % del costo es `Restore_Soft_UL` a unos 15 megapíxeles**, para una imagen que
Lanczos después reduce a la mitad. Hay que confirmarlo con el «Medir en mi GPU» del frente C o
con `--magpie-profile` (da el tiempo por efecto).

Qué mueve ese número, de mayor a menor efecto **[estimado]**:

1. **Resolución de trabajo (lo decide la persona; no es un port).** Con `-m none` la cadena
   correría sobre 1284×743, quedaría en ~1,7 + ~7,8 ≈ 9,5 ms y saldría a 2568×1486, casi el
   tamaño de la pantalla. Poner `Restore_Soft_UL` antes del upscale, como en el modo B de las
   [instrucciones de Anime4K](https://github.com/bloc97/Anime4K/blob/master/md/GLSL_Instructions_Advanced.md)
   («Restore_Soft → Upscale»), lo bajaría otro tanto. Esas instrucciones también avisan que un
   shader después de un 2× cuesta unas 4 veces más **[verificado]**. Se sabe que la persona
   eligió la cadena a propósito, así que esto es solo información para decidir: cambia el
   resultado visual y no lo recomiendo por encima de su criterio.
2. **FP16 (frente A).** Los dos UL declaran `//!CAPABILITY FP16` y sus texturas intermedias ya son
   `R16G16B16A16_FLOAT` **[verificado]**. Con `MF` a 16 bits, RDNA4 puede usar operaciones
   empaquetadas en las convoluciones. No tengo una cifra confiable para esta GPU; Magpie lo
   declara justamente por rendimiento. Se mide en el guion final con y sin FP16.
3. **Port nativo con matrices cooperativas** (`VK_KHR_cooperative_matrix`, WMMA en RDNA3 y
   RDNA4). Es el único port que podría dar una ganancia real sobre el HLSL de Magpie, porque
   cambiaría el algoritmo (convoluciones como productos de matrices) y no solo el envoltorio.
   Costo **[estimado]**: alto (semanas), específico de GPU, con un camino de reserva obligatorio
   y verificación bit a bit difícil. No lo recomiendo antes de medir FP16; si FP16 deja la cadena
   por debajo del presupuesto, no hace falta.

### Recomendación de la parte 2

**No portar más efectos a Native.** En su lugar, en orden:

1. Terminar y medir lo que ya está en curso y sirve para todos: `robustImageAccess` (B), FP16 (A)
   y «Medir en mi GPU» (C). Medir en el guion final la cadena UL con y sin FP16, con el
   desglose por efecto.
2. **SPIR-V precompilado para los efectos MagpieFX incluidos** (propuesta para otra ronda,
   esfuerzo **[estimado]** bajo a medio: 1 a 2 días). Compilar en el build los 86 (o 117)
   efectos con sus valores por defecto y guardarlos en el paquete como semilla de la caché, para
   que el primer arranque y los sistemas sin dxc funcionen como el motor Native. Hay que resolver
   cómo entran los parámetros y la variante FP16: si los parámetros van en el cbuffer, como hoy,
   alcanza con un SPIR-V por variante.
3. Dejar el motor Native como está (3 efectos), como referencia verificada bit a bit y como
   respaldo sin compilador. Si en algún momento los dos motores se unifican, Native es el
   candidato a desaparecer, no a crecer.
4. Revisar el port con matrices cooperativas solo si, con FP16 medido, la cadena de la persona
   sigue sin entrar en el presupuesto de cuadro que ella quiere.

## Pendientes para la integración

- Licencia de `Ani4Kv2_ArtCNN_C4F32_i2.hlsl` (pesos CC BY-NC 4.0) dentro de un paquete GPL-3.0:
  decidir si se saca del paquete. No lo toca este frente.
- `k7_modernAnime_FHD_x2.hlsl`: su repo de origen
  ([mpv_PlayKit](https://github.com/hooke007/mpv_PlayKit)) no declara una licencia que GitHub
  detecte. Lo incluimos bajo la GPL-3.0 de Magpie; conviene confirmar el origen.
- Confirmar con el frente C el desglose de los 34,5 ms (la hipótesis de `Restore_Soft_UL` a 4×).
