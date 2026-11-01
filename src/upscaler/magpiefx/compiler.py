"""
Runtime HLSL -> SPIR-V compilation for generated MagpieFX passes.

Two backends are supported:

* ``dxc`` (preferred): the DirectX Shader Compiler, called with the same
  ``-fvk-*-shift`` options as ``scripts/hlsl-to-spv.fish``.
* ``glslang``: ``glslangValidator``'s HLSL front-end, used as a fallback
  when dxc is not installed. Its HLSL support is closer to fxc's SM5
  dialect, but it is less complete than dxc.

The backend is chosen with the ``UPSCALER_MAGPIEFX_COMPILER`` environment
variable (``dxc``, ``glslang`` or ``auto``, the default: dxc first, glslang
if dxc is missing or rejects the source). The dxc binary can be overridden
with ``DXC``.

Passes of FP16 effects can be compiled with 16-bit types enabled
(``fp16=True``): dxc then targets shader model 6.2 with
``-enable-16bit-types`` and glslang gets ``--hlsl-enable-16bit-types``, so
``min16float`` becomes a real 16-bit float (SPIR-V ``Float16``, which needs
the device's ``shaderFloat16``).

Compiled SPIR-V is cached under ``$XDG_CACHE_HOME/linux-rt-upscaler/magpiefx``
keyed by a SHA-256 of the generated source, include directory contents that
the source may reference, the backend and its command line.
"""

from __future__ import annotations

import hashlib
import logging
import os
import shutil
import subprocess
import tempfile
from typing import Callable, List, Optional, Tuple

from .parser import MagpieFXError

logger = logging.getLogger(__name__)

_CACHE_VERSION = "1"


def cache_dir() -> str:
    base = os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache"))
    return os.path.join(base, "linux-rt-upscaler", "magpiefx")


def _find_dxc() -> Optional[str]:
    env = os.environ.get("DXC")
    if env and os.path.isfile(env):
        return env
    return shutil.which("dxc")


def _find_glslang() -> Optional[str]:
    return shutil.which("glslangValidator") or shutil.which("glslang")


def select_backends() -> List[Tuple[str, str]]:
    """
    Return the ``(backend_name, executable_path)`` pairs to try, in order.

    In ``auto`` mode dxc comes first and glslang is kept as a fallback for
    sources dxc rejects (e.g. fxc-only constant folding in array sizes).
    """
    wanted = os.environ.get("UPSCALER_MAGPIEFX_COMPILER", "auto").lower()
    found: List[Tuple[str, str]] = []
    if wanted in ("auto", "dxc"):
        dxc = _find_dxc()
        if dxc:
            found.append(("dxc", dxc))
        elif wanted == "dxc":
            raise MagpieFXError(
                "dxc requested but not found "
                "(install 'directx-shader-compiler' or set DXC)"
            )
    if wanted in ("auto", "glslang"):
        glslang = _find_glslang()
        if glslang:
            found.append(("glslang", glslang))
        elif wanted == "glslang":
            raise MagpieFXError("glslangValidator requested but not found")
    if not found:
        raise MagpieFXError(
            "No HLSL compiler found: install dxc ('directx-shader-compiler') "
            "or glslangValidator"
        )
    return found


def _command(
    backend: str,
    exe: str,
    src: str,
    out: str,
    include_dir: Optional[str],
    fp16: bool = False,
) -> List[str]:
    if backend == "dxc":
        cmd = [
            exe,
            "-spirv",
            src,
            "-Fo",
            out,
            "-T",
            "cs_6_2" if fp16 else "cs_6_0",
            "-E",
            "__M",
            # Magpie effects are written for fxc (HLSL 2018 semantics)
            "-HV",
            "2018",
            "-fvk-auto-shift-bindings",
            "-fvk-t-shift",
            "1024",
            "0",
            "-fvk-u-shift",
            "2048",
            "0",
            "-fvk-s-shift",
            "3072",
            "0",
            "-fvk-use-dx-layout",
            "-Wno-conversion",
        ]
        if fp16:
            cmd.append("-enable-16bit-types")
    else:
        cmd = [
            exe,
            "-V",
            "-D",
            "-S",
            "comp",
            "-e",
            "__M",
            "--target-env",
            "vulkan1.1",
            "--hlsl-offsets",
            "--shift-texture-binding",
            "1024",
            "--shift-uav-binding",
            "2048",
            "--shift-image-binding",
            "2048",
            "--shift-sampler-binding",
            "3072",
            "-o",
            out,
            src,
        ]
        if fp16:
            cmd.insert(-3, "--hlsl-enable-16bit-types")
    if include_dir:
        cmd.insert(1, f"-I{include_dir}")
    return cmd


def _preprocess(
    backend: str, exe: str, src: str, include_dir: Optional[str], name: str
) -> str:
    """Run the backend's preprocessor on *src* and return the expanded text."""
    if backend == "dxc":
        out = src + ".i"
        cmd = [exe, "-P", "-Fi", out, src]
    else:
        out = None
        cmd = [exe, "-E", "-D", "-S", "comp", src]
    if include_dir:
        cmd.insert(1, f"-I{include_dir}")
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        diag = (proc.stderr or proc.stdout or "").strip()
        raise MagpieFXError(f"{backend} failed to preprocess {name}:\n{diag}")
    if out is None:
        return proc.stdout
    with open(out, encoding="utf-8") as f:
        return f.read()


def _include_digest(include_dir: Optional[str]) -> str:
    """Hash the ``.hlsli`` files next to the effect (they may be #included)."""
    if not include_dir or not os.path.isdir(include_dir):
        return ""
    h = hashlib.sha256()
    for name in sorted(os.listdir(include_dir)):
        if name.endswith((".hlsli", ".h")):
            with open(os.path.join(include_dir, name), "rb") as f:
                h.update(name.encode() + b"\0" + f.read())
    return h.hexdigest()


def compile_hlsl(
    source: str,
    name: str,
    include_dir: Optional[str] = None,
    use_cache: bool = True,
    transform: Optional[Callable[[str], str]] = None,
    transform_key: str = "",
    fp16: bool = False,
) -> bytes:
    """
    Compile a generated pass to SPIR-V and return the bytecode.

    Args:
        source: Complete HLSL source (from :func:`codegen.generate_pass_source`).
        name: Human-readable name used in error messages and temp files.
        include_dir: Directory searched for ``#include`` files.
        use_cache: Read/write the on-disk SPIR-V cache.
        transform: Optional rewrite applied to the *preprocessed* source
            (macros and includes expanded by the same backend) before
            compiling. Exceptions it raises propagate to the caller.
        transform_key: Identifies *transform* in the cache key.
        fp16: Enable 16-bit types (the source must be generated with
            ``fp16=True`` too).

    Raises:
        MagpieFXError: with the compiler's diagnostics if compilation fails.
    """
    errors: List[str] = []
    backends = select_backends()
    for backend, exe in backends:
        try:
            spirv = _compile_with(
                backend, exe, source, name, include_dir, use_cache, transform,
                transform_key, fp16,
            )
        except MagpieFXError as e:
            errors.append(str(e))
            continue
        if errors:
            logger.warning("%s: dxc failed, compiled with %s instead", name, backend)
        return spirv
    if all(backend != "glslang" for backend, _ in backends):
        # Some effects (the Anime4K_Denoise_Bilateral ones) only compile
        # with glslang; say so before dxc's diagnostics
        errors.insert(
            0,
            f"{name} needs glslangValidator (dxc cannot compile it); "
            "install glslang to use this effect.",
        )
    raise MagpieFXError("\n".join(errors))


def _compile_with(
    backend: str,
    exe: str,
    source: str,
    name: str,
    include_dir: Optional[str],
    use_cache: bool,
    transform: Optional[Callable[[str], str]] = None,
    transform_key: str = "",
    fp16: bool = False,
) -> bytes:
    probe = _command(backend, exe, "SRC", "OUT", include_dir, fp16)

    key = hashlib.sha256()
    for part in (_CACHE_VERSION, backend, " ".join(probe[1:])):
        key.update(part.encode() + b"\0")
    key.update(_include_digest(include_dir).encode() + b"\0")
    key.update(transform_key.encode() + b"\0")
    key.update(source.encode())
    digest = key.hexdigest()
    cache_path = os.path.join(cache_dir(), f"{digest}.spv")

    if use_cache and os.path.isfile(cache_path):
        with open(cache_path, "rb") as f:
            return f.read()

    safe = "".join(c if c.isalnum() else "_" for c in name)
    with tempfile.TemporaryDirectory(prefix="magpiefx-") as tmp:
        src = os.path.join(tmp, f"{safe}.hlsl")
        out = os.path.join(tmp, f"{safe}.spv")
        with open(src, "w", encoding="utf-8") as f:
            f.write(source)
        if transform is not None:
            expanded = _preprocess(backend, exe, src, include_dir, name)
            with open(src, "w", encoding="utf-8") as f:
                f.write(transform(expanded))
        cmd = _command(backend, exe, src, out, include_dir, fp16)
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0 or not os.path.isfile(out):
            diag = (proc.stderr or proc.stdout or "").strip()
            if proc.returncode < 0 or proc.returncode >= 128:
                sig = -proc.returncode if proc.returncode < 0 else proc.returncode - 128
                diag = f"{backend} crashed (signal {sig})\n{diag}"
            raise MagpieFXError(f"{backend} failed to compile {name}:\n{diag}")
        with open(out, "rb") as f:
            spirv = f.read()

    if use_cache:
        try:
            os.makedirs(cache_dir(), exist_ok=True)
            tmp_path = f"{cache_path}.{os.getpid()}.tmp"
            with open(tmp_path, "wb") as f:
                f.write(spirv)
            os.replace(tmp_path, cache_path)
        except OSError as e:
            logger.debug("Could not write MagpieFX cache '%s': %s", cache_path, e)
    return spirv
