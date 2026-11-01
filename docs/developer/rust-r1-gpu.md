# Ronda 1, frente G: `lirtup-gpu`

Estado: **terminado**, sin probar en GPU (las reglas de la ronda no dejan correr ni tests ni
ejemplos). Rama `claude/rust-r1-gpu-c5568c`, sale de `claude/rust-r0-contract-6aa817`. Fecha:
2026-11-01.

`cargo check -p lirtup-gpu` y `cargo check -p lirtup-gpu --all-targets` (incluye los tests y el
ejemplo): **pasan, sin avisos** (rustc 1.99.1). `cargo fmt -p lirtup-gpu` aplicado. No se corrió
`cargo build`, `cargo test`, `clippy` ni el ejemplo.

## Qué se hizo

Port de `src/upscaler/vulkan/lib/*.cpp` sobre `ash` 0.38 y `gpu-allocator` 0.28. No queda ningún
`todo!()` y se borró el `#![allow(unused_variables)]` de la ronda 0.

| Módulo | Qué hace | Origen en 1.x |
| --- | --- | --- |
| `instance` | `Entry::load`, `VK_KHR_surface` y `VK_KHR_xcb_surface` si están, `VK_EXT_debug_utils` y `VK_LAYER_KHRONOS_validation` con `debug` (solo si están instalados). Mensajes al log (`tracing`) y a un buzón que vacía `Device::debug_messages`. `AdapterInfo` completo. `best_adapter` (respeta `VULKAN_DEVICE`; después hardware, discreta, VRAM, FP16), `select_adapter` (índice o subcadena; sin coincidencia avisa y usa la mejor) | `vk_instance.cpp`, `VulkanContext` |
| `device` | Cola gráfica+cómputo, `Mutex<vk::Queue>` (todo submit, present y wait idle pasa por él), extensiones (swapchain, fd externos, `VK_EXT_image_robustness` si el dispositivo es < 1.3), features (las de 1.1 y 1.2 que soporte, como 1.x; `shaderStorageImage{Read,Write}WithoutFormat`, `shaderInt16`, `robustImageAccess`), `gpu-allocator`, caché de pipelines, pools de timestamps de 128 | `vk_device.cpp` |
| `resource` | `Buffer` (sub-asignado; `Upload` = `CpuToGpu`, `Readback` = `GpuToCpu`, mapeado coherente, vista tipada si hay `format`) y `Texture` (2D o arreglo, vista 2D/arreglo, transición inicial a `GENERAL`) | `vk_resource.cpp`, `Buffer`/`Texture2D` |
| `external` | Textura con asignación dedicada exportable (`OPAQUE_FD`, primer tipo `DEVICE_LOCAL`, fuera del asignador) y semáforo binario exportable; `export_fd` → `OwnedFd` | `vk_external.cpp` |
| `sampler` | Filtros y modos como `vk_Device_create_sampler` | `vk_sampler.cpp` |
| `spirv` (privado) | Punto de entrada GLCompute, tamaño de grupo (`LocalSize`, `LocalSizeId` y el built-in `WorkgroupSize`), parche `NonReadable` | `vk_spirv_*` de `vk_utils.cpp` |
| `compute` | Layout y escrituras de descriptores con los bindings de dxc (b=0, t=1024, u=2048, s=3072), bindless con update-after-bind, push constants, el parche `NonReadable` para UAV BGRA cuando falta `shaderStorageImageReadWithoutFormat` | `vk_compute.cpp`, `vk_utils.cpp` |
| `recorder` | `CommandRecorder` (barreras automáticas, timestamps por dispatch con nombre) → `Device::submit` → `Submission::{is_done, wait}` → `Timestamps` en ms | `dispatch_sequence`, `execute_tile_batch` |
| `swapchain` | Superficie XCB, formato (el pedido en sRGB no lineal, si no `B8G8R8A8_SRGB`, si no el primero conocido), modo (FIFO de respaldo), cantidad y tamaño dentro de los límites, `present` con recorte por desplazamiento, color de fondo y semáforo de espera, `recreate` con `oldSwapchain` | `vk_swapchain.cpp` |

### Cómo funciona por dentro

- **Todo en `GENERAL`.** Como en 1.x, las texturas pasan a `GENERAL` al crearse y no salen de ahí:
  compute, copias y `clear` usan ese layout. El grabador solo pone una barrera de memoria entre
  pasos (lo que escribió el paso anterior → lo que lee y escribe el siguiente); el primer paso
  espera todo lo enviado antes en la cola. Es lo mismo que hacía 1.x entre dispatches. Solo las
  imágenes del swapchain cambian de layout.
- **Un pool de comandos por grabación.** `VkCommandPool` es de un hilo, así que cada
  `CommandRecorder` toma un «frame» propio (pool + command buffer + fence) de una lista libre del
  dispositivo. El hilo del pipeline y el de FG pueden grabar a la vez sin compartir pool.
- **Vida de los recursos.** El grabador guarda un clon de cada recurso que usa; la `Submission`
  los suelta cuando el fence señala. Si una `Submission` se suelta sin esperar, espera hasta 1 s y,
  si la GPU sigue, la deja «estacionada» en el dispositivo (se recicla en la próxima grabación).
- **Errores sin `Result` en el grabador.** Los métodos del contrato devuelven `()`. Un recurso de
  otro dispositivo, una región fuera de rango, una textura sin `COPY_DST` o push constants de más
  saltan el paso, se loguean y `Device::submit` devuelve el primer error.
- **Timestamps.** Un pool de 128 consultas por grabación (64 dispatches con nombre como mucho), en
  la etapa de compute antes y después de cada dispatch (igual que 1.x). Se enmascaran con
  `timestampValidBits`. `total_ms` va del primer al último timestamp.
- **Swapchain.** Un «slot» por imagen (semáforo de adquisición, fence y command buffer propios) y un
  semáforo de fin por imagen; 1.x usaba un solo par de semáforos para todo, que con 3 imágenes en
  vuelo no es válido. La textura presentada se guarda hasta que su copia termina.
- **`robustImageAccess`.** La instancia pide Vulkan 1.3 si el loader lo tiene (1.x pedía 1.2). En
  dispositivos 1.3 la feature es core; en 1.2 se activa `VK_EXT_image_robustness`. En los dos
  casos se encadena `VkPhysicalDeviceImageRobustnessFeatures`.

### Diferencias con 1.x (a propósito)

- `ComputeDesc.entry` vacío toma el primer punto de entrada GLCompute (1.x siempre buscaba el
  primero). Con nombre, busca ese.
- Buffers SRV con `stride > 0` se declaran `STORAGE_BUFFER` (es lo que emite dxc para
  `StructuredBuffer`); sin `stride` siguen como `UNIFORM_BUFFER`, como 1.x. Con vista tipada,
  `UNIFORM_TEXEL_BUFFER`.
- Bindless: se escribe `dstBinding` = base y `dstArrayElement` = i. 1.x escribía `dstBinding` =
  1024 + i, que no existe en el layout.
- El parche `NonReadable` se aplica a todos los UAV BGRA, no solo al primero.
- La copia buffer → textura deja la imagen en `GENERAL`; 1.x volvía de `TRANSFER_DST` con
  `oldLayout = UNDEFINED`, que en teoría descarta el contenido.
- Si el formato no admite `STORAGE_IMAGE` (los sRGB), se quita `STORAGE` del uso con un aviso en
  `debug`; 1.x lo pedía igual.
- Se habilitan las features de Vulkan 1.1 que el dispositivo soporte y `shaderInt16` (los tipos
  `min16` de dxc), además de lo de 1.x.
- `present` acepta `clear` (fondo) y `wait` (semáforo de lsfg). `Suboptimal` se informa aparte de
  `OutOfDate` (1.x devolvía `False` en los dos).
- Sin heaps explícitos ni recursos sparse (no estaban en el contrato y la app no los usa).

## Cambios de API

Ninguna firma del contrato cambió. Solo se agregó:

- `Format::all()`: los 45 formatos en orden DXGI (lo usan los tests; sirve a la GUI y a MagpieFX).
- Los campos privados de los tipos opacos (`_private` se reemplazó por los campos reales;
  `CommandRecorder` y `Swapchain` siguen con `PhantomData<Cell<()>>`, así que no son `Sync`).
- Un módulo privado `spirv`.

Notas de uso para los demás frentes (sin cambio de firma):

- `TextureUsage::COPY_DST` hace falta para `CommandRecorder::clear` (usa `vkCmdClearColorImage`).
  El `TextureUsage::default()` ya lo incluye.
- `copy_texture`: en cada `CopyRegion`, `src_offset` es la x de origen y `src_y` la y; sin
  regiones copia el tamaño común desde (0, 0). `copy_buffer_to_texture` sin regiones copia la
  textura entera desde el byte 0.
- `Submission::wait` con `timestamps: true` en un dispositivo sin timestamps devuelve
  `Some(Timestamps::default())`; con `timestamps: false`, `None`.
- `GpuError::vk` traduce `ERROR_DEVICE_LOST` a `DeviceLost`, `ERROR_OUT_OF_DATE_KHR` a `OutOfDate` y
  `TIMEOUT` a `Timeout` (el pipeline de la ronda 2 puede reaccionar a cada uno).

## Dependencias

**Ninguna nueva.** Se usan las que ya tenía el `Cargo.toml` del crate: `ash`, `gpu-allocator`,
`thiserror`, `tracing`, `libc` (sin uso todavía; se puede quitar) y `bitflags`. `OwnedFd` sale de
`std`.

## Cómo probar el ejemplo

Necesita un driver Vulkan 1.2 o superior. Para que no salgan avisos de validación, instalar las
capas (`vulkan-validation-layers` en Arch). Sin ellas el ejemplo corre igual y lo avisa en el log.

```bash
cargo run -p lirtup-gpu --example copy
```

```bash
cargo run -p lirtup-gpu --example copy -- --gpu 0 --no-debug
```

Qué hace, en orden, y qué tiene que dar:

1. Lista los adaptadores y elige uno con `select_adapter` (`--gpu` acepta índice o nombre).
2. Sube un patrón BGRA de 300×200 a una textura, borra otra, copia el cuarto inferior derecho de la
   primera al origen de la segunda y la baja a un buffer de lectura: `ok src downloads as uploaded`
   y `ok region copy and clear`.
3. Compila el `clear.spv` real de 1.x (`src/upscaler/shaders/misc/clear/`), lee el tamaño de grupo
   del SPIR-V (16×16×1), hace un dispatch con timestamp y comprueba los half floats:
   `ok dispatch wrote the clear color` y el tiempo del dispatch en ms.
4. Si hay fd externos: crea una textura y un semáforo exportables, exporta los dos fds, señala y
   espera el semáforo desde dos envíos y comprueba la textura.
5. `ok no validation messages`. El código de salida es distinto de 0 si algo falló.

Los tests unitarios (`cargo test -p lirtup-gpu`) no necesitan GPU: tabla de formatos, mapeo de
errores, selección de adaptador, totales de memoria, familia de colas, parser y parche de SPIR-V,
pool de descriptores, timestamps (con desborde de bits), valores de `clear`, recorte de `present`,
elección de formato, modo, cantidad y tamaño del swapchain, y `Send`/`Sync` de cada tipo.

## Pendiente y riesgos

- **Nada se corrió en una GPU.** Lo primero de la integración: `cargo test -p lirtup-gpu` y el
  ejemplo con las capas de validación (criterio 5.8 de la ronda 1).
- `Swapchain` no se puede probar con el ejemplo (no tiene ventana). Se prueba en la ronda 2, cuando
  el pipeline lo conecte con la `OverlayWindow` de `lirtup-x11`.
- Si `present` falla entre la adquisición y el envío (un error de Vulkan, no un `OutOfDate`), el
  semáforo de adquisición de ese slot queda señalado sin esperar. Solo pasa con errores que de
  todos modos obligan a recrear todo.
- `ExternalSemaphore` hace `vkDeviceWaitIdle` al soltarse (como `destroy_semaphore` de 1.x). Está
  bien al cerrar lsfg, pero no hay que crearlos y soltarlos por cuadro.
- `Texture::create` espera en la CPU la transición inicial (como 1.x). Para MagpieFX, que crea
  muchas texturas, convendría agruparlas en un solo envío más adelante.
- No hay `execute_tile_batch` como función aparte: el pipeline de la ronda 2 lo arma con
  `copy_buffer_to_texture` (varias regiones con `dst_slice`) y dispatches.
- `libc` está en las dependencias del crate y no se usa; el revisor puede quitarlo.
