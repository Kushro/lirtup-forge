//! Builds the vendored lsfg-vk core and `lsfg_c.cpp` with `cc`.
//!
//! Same sources and flags as `setup.py` (`lsfg_extension`): C++20, `-O2`,
//! `-mtune=generic`, `-fvisibility=hidden`, `VK_NO_PROTOTYPES`, `-w`, linked
//! with `dl` and `pthread`. The vendored tree stays in `src/upscaler/lsfg/`
//! (`third_party/` and `lib/shader_loader.cpp`, `unicode_posix.cpp`,
//! `volk_unit.cpp`) until the Python app is retired in round 4;
//! `lsfg_module.cpp` (the CPython bridge) is replaced by `src/lsfg_c.cpp`.
//!
//! `-Wl,-Bsymbolic` of `setup.py` is for a shared library; here the code is a
//! static archive inside an executable that exports nothing, and volk's
//! `vk*` pointers have hidden visibility, so they never meet `ash`'s
//! `libvulkan`.
//!
//! Needs the Vulkan headers (`vulkan/vulkan_core.h`). `LSFG_SYS_NO_BUILD=1`
//! skips the C++ (for `cargo check` or docs on a machine without them); the
//! final link then fails.

use std::path::{Path, PathBuf};

/// `.cpp` files of `dir`, sorted (as `sorted(glob("*.cpp"))`).
fn sources(dir: &Path, skip: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "cpp"))
        .filter(|p| !skip.iter().any(|s| p.file_name().is_some_and(|n| n == *s)))
        .collect();
    files.sort();
    files
}

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let lsfg = manifest.join("../../src/upscaler/lsfg");
    let lsfg_vk = lsfg.join("third_party/lsfg-vk");

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=include/lsfg_c.h");
    println!("cargo::rerun-if-changed=src/lsfg_c.cpp");
    println!("cargo::rerun-if-changed={}", lsfg.display());
    println!("cargo::rerun-if-env-changed=LSFG_SYS_NO_BUILD");

    if std::env::var_os("LSFG_SYS_NO_BUILD").is_some_and(|v| v == "1") {
        println!("cargo::warning=LSFG_SYS_NO_BUILD=1: the lsfg core is not built and will not link");
        return;
    }

    let mut files = vec![manifest.join("src/lsfg_c.cpp")];
    // The CPython bridge of 1.x needs Python.h; lsfg_c.cpp replaces it.
    files.extend(sources(&lsfg.join("lib"), &["lsfg_module.cpp"]));
    files.extend(sources(&lsfg.join("third_party/pe-parse/src"), &[]));
    for folder in [
        "src/common",
        "src/core",
        "src/pool",
        "v3.1_src",
        "v3.1_src/shaders",
        "v3.1p_src",
        "v3.1p_src/shaders",
    ] {
        // Enigmata Temple's staging-buffer API, not used here (images by fd)
        files.extend(sources(&lsfg_vk.join(folder), &["temple_local.cpp"]));
    }

    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .files(&files)
        .include(manifest.join("include"))
        .include(lsfg.join("lib"))
        .include(lsfg.join("third_party/volk"))
        .include(lsfg.join("third_party/pe-parse/include"))
        .include(lsfg_vk.join("include"))
        .include(lsfg_vk.join("public"))
        .include(lsfg_vk.join("v3.1_include"))
        .include(lsfg_vk.join("v3.1p_include"))
        .define("VK_NO_PROTOTYPES", None)
        // The shaders setup runs on the CPU at startup; keep it optimized even
        // in dev builds, as setup.py did.
        .opt_level(2)
        .flag("-mtune=generic")
        .flag("-fvisibility=hidden")
        // Vendored sources, not warning-clean.
        .warnings(false)
        .flag("-w")
        .compile("lsfg");

    println!("cargo::rustc-link-lib=dylib=dl");
    println!("cargo::rustc-link-lib=dylib=pthread");
}
