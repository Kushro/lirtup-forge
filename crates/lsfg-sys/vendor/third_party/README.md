# Third-party code of the native frame generation engine

Copied from Enigmata Temple (`native/temple-lsfg/third_party/` and
`native/temple-lsfg/src/`), which vendors it from Enigmata. Nothing was
downloaded for this copy. `README.temple.md` is Temple's provenance record,
with the upstream commits and every modification.

| Folder / file | License | Use here |
| --- | --- | --- |
| `lsfg-vk/` | MIT (`lsfg-vk/LICENSE.md`) | Frame generation core, v3.1 and v3.1P. Built without `TEMPLE_LSFG_LOCAL`: images and semaphores are shared by fd (`createContext`). `src/common/temple_local.cpp` is not compiled. One LirtUp Forge change: `src/common/exception.cpp` builds its messages without `std::format` (GCC 13). |
| `pe-parse/` | MIT (`pe-parse/LICENSE`) | Reads the resources of `Lossless.dll`. |
| `volk/` | MIT (`volk/LICENSE.md`) | Vulkan loader of the core, compiled through `../lib/volk_unit.cpp` with hidden visibility. |
| `../lib/shader_loader.*` | GPL-3.0-only | Extracts the SPIR-V shader sets from `Lossless.dll` in memory. |
| `../lib/unicode_posix.cpp` | MIT | The UTF-16 conversion pe-parse needs. |

`Lossless.dll` is never included: the engine reads the copy of the person who
owns Lossless Scaling. The combined module is distributed under GPL-3.0.
