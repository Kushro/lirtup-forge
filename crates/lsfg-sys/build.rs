//! Builds the vendored lsfg-vk core and `lsfg_c.cpp` with `cc` (front C).
//!
//! Same sources and flags as `setup.py` (`lsfg_extension`): C++20, `-O2`,
//! `-fvisibility=hidden`, `VK_NO_PROTOTYPES`, `-w`, linked with `dl` and
//! `pthread` and `-Wl,-Bsymbolic`. The vendored tree stays in
//! `src/upscaler/lsfg/` (`third_party/` and `lib/shader_loader.cpp`,
//! `unicode_posix.cpp`, `volk_unit.cpp`) until the Python app is retired in
//! round 4; `lsfg_module.cpp` is replaced by `lsfg_c.cpp` here.
//!
//! Round 0: nothing is compiled yet, so `cargo check` works and nothing links
//! against the symbols.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=include/lsfg_c.h");
}
