# Ronda 1: integración y builds

Rama **`rust/r1-integration`**, que sale de `claude/rust-r0-contract-6aa817`. Fecha: 2026-11-01.
Sin push y sin merge a `main`. Contrato en [rust-r0-contract.md](rust-r0-contract.md); notas de
los frentes en [rust-r1-config-fg.md](rust-r1-config-fg.md), [rust-r1-gpu.md](rust-r1-gpu.md),
[rust-r1-x11.md](rust-r1-x11.md) y [rust-r1-effects.md](rust-r1-effects.md).

Máquina de prueba: Arch (CachyOS), rustc 1.99.1, AMD Radeon RX 9070 XT (RADV, Mesa 26.2.5,
Vulkan 1.4) más la iGPU del Ryzen 5 7600X, sesión Wayland con XWayland (`DISPLAY=:0`), dxc y
glslang instalados, capas de validación de Khronos instaladas.

## Resumen

| | Build 1 | Build 2 |
| --- | --- | --- |
| `cargo build --workspace --release` | pasa, 0 avisos | pasa, 0 avisos |
| `cargo clippy --workspace --all-targets` | pasa con 69 avisos de estilo | pasa; solo quedan los 10 `todo` de `lirtup-cli` y `lirtup-core` (ronda 2) |
| `cargo test --workspace` | 206 de 207: falla `profile::tests::interval_forms` | **209 de 209** |
| `cargo fmt --all --check` | no (C y E no lo habían pasado) | pasa |

Criterios de 5.8 para la ronda 1: se cumplen **todos menos la mitad del de E** (que la salida sea
igual a la de Python), que pasa a la ronda 2. Detalle más abajo.

## Merges

En el orden C → G → X → E, con `--no-ff`. **No hubo conflictos**: cada frente tocó solo sus
crates y su nota.

| Frente | Rama | Commits |
| --- | --- | --- |
| C | `claude/rust-r1-config-fg-178a75` | 4 |
| G | `claude/rust-r1-gpu-c5568c` | 3 |
| X | `claude/rust-r1-x11-aaac12` | 5 |
| E | `claude/rust-r1-effects-905ea3` | 4 |

Commits del revisor después de los merges:

1. `Cargo.toml`: `cc` con la feature `parallel` (pedido de C, para los 39 `.cpp` de lsfg); se
   quita `libc` de `lirtup-gpu` (sin uso, lo dice G); `Cargo.lock` con `tempfile` para
   `lirtup-fg` y `lirtup-magpiefx` (pedido de C y E).
2. `lirtup-gpu`: arreglo del lector de SPIR-V (abajo).
3. La pasada de correcciones del Build 1.
4. `lirtup-magpiefx/examples/compile_all.rs`, para medir el criterio de E.
5. Esta nota.

## Revisión contra el contrato

Las firmas del contrato de la ronda 0 se mantuvieron: lo que hay son agregados, salvo los casos
de abajo. Los acepto todos.

- **C, CLI**: `-g` pasa a ser `--gpu` y la geometría pasa a `-o/--output-geometry`, con `--geometry`
  como alias, igual que en 1.x. `-q`/`-v` quedan como globales de `lirtup-cli` y en
  `OptionArgs` están `--debug`, `--log-level` y `--log-file`. Lo correcto es seguir a 1.x.
- **C, `unsafe` en `lirtup-fg`**: el contrato lo permitía solo en `lirtup-gpu`, `lirtup-x11` y
  `lsfg-sys`. El envoltorio seguro de la FFI y `set_var`/`remove_var` (que en la edición 2024
  son `unsafe`) lo necesitan. Queda permitido **por módulo**, en `native` y en las dos funciones
  de `layer`, cada bloque con su `// SAFETY:`. El comentario de `[workspace.lints]` ya lo dice.
- **C, `Interval`**: el `Serialize` ahora está escrito a mano, para que `720` se guarde como
  `720`. No cambia la firma.
- **E, `LutPreset`**: tiene 12 variantes en lugar de una. Sin ellas no hay paridad con 1.x.
- **E, `Srcnn::new` con `tiles: Some(..)`** devuelve `Unsupported`; el modo tiles va por
  `Srcnn::new_tiled`. El frente P de la ronda 2 tiene que usar esa función.
- **E, `CompiledEffect`**: tiene campos privados y ya no se construye con un literal desde fuera
  del crate. Nadie lo hacía.
- **G y X**: ningún cambio de firma.

Supuestos cruzados que revisé al integrar:

- E usa `CopyRegion { src_offset, src_row_length, dst_slice }` tal como G lo implementa: sin
  regiones se copia la textura entera, y `src_offset` es la x de origen en las copias de textura
  a textura.
- `lirtup-x11` y `lirtup-fg` siguen sin depender de `lirtup-gpu`.
- El doble `flatten` de `Config` funciona: lo cubre `mode_keys_are_flattened_not_extra`, que
  pasa.
- Ya no queda ningún `todo!()` en los crates de la ronda 1. Solo quedan en `lirtup-cli` y
  `lirtup-core`, que son de la ronda 2.

## Lo que falló en el Build 1 y cómo se arregló

- **Test `profile::tests::interval_forms`**: esperaba un error para `"480 < ="`, porque así lo
  dice el docstring de `parse_interval` en 1.x. Pero 1.x quita los espacios antes de mirar, lo lee
  como `480<=` y lo acepta. El código de C ya hacía lo mismo que 1.x; el que estaba mal era el
  test. Ahora pide esa paridad (`Ge(480)`) y prueba con `"<=>480"`, que sí está mal formado.
- **Clippy (69 avisos, todos de estilo)**:
  - ifs que se pueden colapsar;
  - `is_multiple_of`;
  - `as_chunks::<N>()` en lugar de `chunks_exact(N)`;
  - structs armados con `..Default::default()`;
  - `!(x >= y)` con NaN, ahora explícito: `x.is_nan() || x < y`, con el mismo comportamiento;
  - `strip_prefix`;
  - alias de tipo;
  - `Box<ConfigArgs>` en el subcomando `Run` (`large_enum_variant`).

  Una parte salió de `clippy --fix` y el resto se hizo a mano.
- **`cargo fmt --all`**, que C y E no habían pasado.

## Error encontrado al probar con la GPU

**`lirtup-gpu::spirv` entraba en pánico con cualquier `.spv` real.** `workgroup_size` y
`constant` usaban `cond.then_some(ops[0])`, y `then_some` evalúa su argumento aunque `cond` sea
falso. Con una instrucción sin operandos (`OpReturn`, `OpFunctionEnd`) indexaba fuera de rango.
Los tests de G usaban SPIR-V armado a mano, sin esas instrucciones, por eso no lo detectaron. Lo
corregí con `then(|| …)` y agregué dos tests: uno con instrucciones cortas y otro con el
`clear.spv` de 1.x.

Esto rompía **cualquier** `create_compute`, así que también afectaba a los efectos nativos,
SRCNN y MagpieFX en cuanto tocaran la GPU.

## Criterios de 5.8, ronda 1

| Criterio | Estado | Cómo se verificó |
| --- | --- | --- |
| `cargo test --workspace` en verde | **Cumplido** | 209 de 209 |
| El ejemplo de G corre sin errores de validation layers | **Cumplido**, con una salvedad | `cargo run --release -p lirtup-gpu --example copy` en la RX 9070 XT: subida, copia de región, `clear`, el dispatch de `clear.spv` (grupo 16×16×1 leído del SPIR-V, 0,002 ms), exportación de fds de memoria y semáforo, y textura compartida. Todo `ok`. Ver la salvedad abajo |
| X captura una ventana a PNG | **Cumplido** | `capture_png --list` ve XWayland y los dos monitores RandR con sus Hz. Captura de `glxgears` 300×300, 20 cuadros: el primero `full`, los que no cambian salen `dirty false` (XDamage anda), entre 0,13 y 0,84 ms por cuadro. El PNG se ve bien, con los canales en orden |
| E compila los 117 efectos con dxc (los 3 Denoise con glslang) | **Cumplido** | `cargo run --release -p lirtup-magpiefx --example compile_all`, con caché vacía: 117 de 117 en 14 s. Con `--fp16`, 117 de 117 (42 en FP16). Los Denoise compilan con el respaldo automático |
| E da la misma salida que Python en la imagen RGB de prueba | **Pendiente → ronda 2** | Hace falta ligar los efectos a la GPU y comparar contra PNG de referencia. Pasa al frente P |
| C lee `config-example.yaml` y los perfiles de la persona sin cambiarlos | **Cumplido** | `tests/files_1x.rs` (5 tests): lee el ejemplo, la configuración de 1.3 y `~/.config/linux-rt-upscaler/config.yaml` de la persona, hace la ida y vuelta en una copia, y un segundo guardado da los mismos bytes. El sha256 del `config.yaml` de la persona es el mismo antes y después, y `config-example.yaml` sigue igual en git |
| `lsfg_probe` encuentra `Lossless.dll` | **Cumplido** | `probe_finds_lossless_dll` encuentra `~/.local/share/Steam/steamapps/common/Lossless Scaling/Lossless.dll` y la reconoce como `Fp16`. El `build.rs` compiló el C++ de lsfg con `cc` |

**Salvedad del ejemplo de G.** En esta máquina está instalada la capa implícita de lsfg-vk
(`liblsfg-vk-layer.so`). El cargador de Vulkan avisa que no encuentra `vkGetInstanceProcAddr`
en esa capa, y el ejemplo cuenta ese mensaje como un aviso de validación y termina con 1. Con
`DISABLE_LSFGVK=1` pasa limpio («ok no validation messages»). El aviso no viene de lirtup, pero
lirtup tiene que decidir qué hacer con él (ver la ronda 2).

## Lo que pasa a la ronda 2

1. **Capa implícita de lsfg-vk en la instancia propia** (frentes K y F). El proceso de lirtup
   tiene que crear su instancia con la capa apagada, salvo en el modo FG por capa. Es lo que hace
   `framegen.prepare` en 1.x con `DISABLE_LSFGVK`. Lo más simple es que `layer::prepare` lo deje
   en el entorno antes de `Instance::new`, como ya pide C: hay que llamarlo antes de tokio y de
   los hilos. Además, `Instance` podría bajar a `debug` los mensajes del cargador que no son de
   validación.
2. **Paridad de la salida de E con Python** (frente P). Propuesta de E: un test `#[ignore]` que
   compare con PNG de referencia generados por `scripts/magpiefx-bench.py`, sobre la misma
   imagen RGB. Lo mismo para SRCNN, los escaladores y los efectos nativos.
3. **Carreras entre CPU y GPU en E** (`set_params`, `set_frame`, `set_dst_rect`,
   `OverlayBlend::set_image`): escriben buffers `Upload` mientras el cuadro anterior puede seguir
   en vuelo. Es lo mismo que pasa en 1.x. Conviene un anillo por cuadro o push constants
   (frente P).
4. **Swapchain** de G: no se pudo probar sin ventana. Se prueba cuando el presentador lo conecte
   con la `OverlayWindow` de X.
5. **XTest en `InputForwarder`**: falta decidir cuándo usarlo (X).
6. **El frente K empieza con el test de ida y vuelta del protocolo** (pendiente de la ronda 0).
7. **Los `todo!()` de `lirtup-cli` y `lirtup-core`**: son los únicos avisos de clippy que quedan.
8. `glxgears` aparece con `pid None` y `class None`. Probablemente no publica `_NET_WM_PID` ni
   `WM_CLASS`, pero no lo verifiqué. Conviene mirarlo con una ventana que sí los publique.

## Guion de pruebas a mano

Lo que no se pudo probar desde la sesión porque necesita interacción o el pipeline de la
ronda 2. Antes de abrir el ejemplo, compilar con `cargo build --release -p lirtup-x11 --examples`.

- **Señalar ventana**: `capture_png --pick -o /tmp/pick.png`. Aparece el cursor en cruz; un clic
  izquierdo sobre una ventana X la captura; Escape o el clic derecho cancelan.
- **Ventana activa**: `capture_png -o /tmp/active.png`, y enfocar una ventana X antes de 3 s.
  Tiene que guardar también el ícono de 64 px (`*.icon.png`).
- **Ventana tapada o minimizada**: capturar una ventana tapada (con XWayland compone el
  compositor) y una minimizada. La minimizada tiene que dar un cuadro limpio, no un error.
- **Atajos** (`Hotkeys`, con `GrabKey`), en la ronda 2 cuando `lirtup serve` los use: probar
  Alt+Shift+A y Alt+Shift+Q, con NumLock prendido y apagado, y que un atajo que ya tiene otro
  programa dé un error claro.
- **Superposición** en sus cuatro modos sobre KWin o GNOME (ronda 2, criterio de la ronda 2):
  pantalla completa, siempre arriba, ventana y top-transparent, más el cursor oculto, el
  reenvío de clics y de la rueda, y el doble clic.
- **Miniaturas** (`thumbnail` con `enable_thumbnails`) de una ventana tapada: lo usará la GUI en
  la ronda 3.
- **Motor integrado de FG** (`LsfgEngine::initialize`, `create_context` y `present`) con fds
  reales de `lirtup-gpu`: ronda 2, frente F.
