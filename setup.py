#!/usr/bin/env python3

import sys
from pathlib import Path

from setuptools import setup, Extension


def _get_version_from_pyproject():
    pyproject_path = Path(__file__).parent / "pyproject.toml"
    if not pyproject_path.exists():
        return "0.0.0"
    try:
        if sys.version_info >= (3, 11):
            import tomllib
        else:
            import tomli as tomllib
        with open(pyproject_path, "rb") as f:
            data = tomllib.load(f)
        return data.get("project", {}).get("version") or "0.0.0"
    except Exception:
        return "0.0.0"


capture_extension = Extension(
    "upscaler.capture.capture",
    sources=[str(f) for f in (Path("src/upscaler/capture/lib")).glob("*.c")],
    libraries=["X11", "X11-xcb", "Xext", "Xdamage", "Xfixes", "xcb", "pthread"],
    extra_compile_args=["-O3", "-mtune=generic"],
    language="c",
)

vulkan_extension = Extension(
    "upscaler.vulkan.vulkan",
    sources=[str(f) for f in Path("src/upscaler/vulkan/lib").glob("*.cpp")],
    libraries=["vulkan"],
    extra_compile_args=["-std=c++17", "-O3", "-mtune=generic"],
    language="c++",
)

_lsfg = Path("src/upscaler/lsfg")
_lsfg_vk = _lsfg / "third_party/lsfg-vk"
_lsfg_sources = (
    sorted((_lsfg / "lib").glob("*.cpp"))
    + sorted((_lsfg / "third_party/pe-parse/src").glob("*.cpp"))
    + [
        f
        for folder in (
            "src/common",
            "src/core",
            "src/pool",
            "v3.1_src",
            "v3.1_src/shaders",
            "v3.1p_src",
            "v3.1p_src/shaders",
        )
        for f in sorted((_lsfg_vk / folder).glob("*.cpp"))
        # Enigmata Temple's staging-buffer API, not used here (images by fd)
        if f.name != "temple_local.cpp"
    ]
)

# Frame generation core (lsfg-vk, MIT) with the shader loader from Enigmata
# Temple (GPL-3.0). It loads Vulkan itself through volk.
lsfg_extension = Extension(
    "upscaler.lsfg.lsfg",
    sources=[str(f) for f in _lsfg_sources],
    include_dirs=[
        str(_lsfg / "lib"),
        str(_lsfg / "third_party/volk"),
        str(_lsfg / "third_party/pe-parse/include"),
        str(_lsfg_vk / "include"),
        str(_lsfg_vk / "public"),
        str(_lsfg_vk / "v3.1_include"),
        str(_lsfg_vk / "v3.1p_include"),
    ],
    define_macros=[("VK_NO_PROTOTYPES", None)],
    libraries=["dl", "pthread"],
    extra_compile_args=[
        "-std=c++20",
        "-O2",
        "-mtune=generic",
        "-fvisibility=hidden",
        "-w",  # vendored sources, not warning-clean
    ],
    extra_link_args=["-Wl,-Bsymbolic"],
    language="c++",
)

setup(
    name="linux-rt-upscaler",
    version=_get_version_from_pyproject(),
    ext_modules=[capture_extension, vulkan_extension, lsfg_extension],
)
