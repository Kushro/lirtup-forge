#!/usr/bin/env python3
"""
Reference PNGs for the Rust parity tests of round 2 (front S).

Runs the 1.x (``src/upscaler``) headless on a synthetic RGB test image and
writes one PNG per case, next to the case lists the Rust tests read:

* ``crates/lirtup-effects/tests/fixtures/parity/cases.json``: native
  effects, the final scalers and the SRCNN models;
* ``crates/lirtup-magpiefx/tests/fixtures/parity/cases.json``: every
  MagpieFX effect marked ``ok`` in ``docs/developer/magpiefx-report.md``.

The references go to ``refs/<name>.png`` beside each ``cases.json``
(MagpieFX names with ``/`` are written with ``__``). Every output is
stored as 8-bit RGBA: 8-bit textures as they are, RGBA16F textures (the
in-place effects on the 16-bit screen texture, the scalers) as
``round(clamp(v, 0, 1) * 255)`` of the stored value, which is what the Rust
tests compute too. The Rust side runs the same cases with
``cargo test -p lirtup-effects -p lirtup-magpiefx --test parity -- --ignored``.

The test image is computed, not read: :func:`make_input` and
``input_image`` in the Rust tests are the same integer formula.

Usage (from the repository root, with the 1.x environment):

    PYTHONPATH=src python scripts/rust-parity-refs.py [--only effects|magpiefx] [--case NAME ...]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import struct
import sys
import traceback
from typing import Dict, List, Optional, Tuple

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EFFECTS_DIR = os.path.join(ROOT, "crates/lirtup-effects/tests/fixtures/parity")
MAGPIEFX_DIR = os.path.join(ROOT, "crates/lirtup-magpiefx/tests/fixtures/parity")
REPORT = os.path.join(ROOT, "docs/developer/magpiefx-report.md")

BLACK = (0.0, 0.0, 0.0, 1.0)


# ----------------------------------------------------------------------
# Test image and pixel formats (mirrored in the Rust tests)
# ----------------------------------------------------------------------
def make_input(width: int, height: int) -> bytes:
    """
    The test image, RGBA8: red grows to the right, green downwards, blue is
    a xor pattern; white columns every 23 pixels, and a gray checkerboard in
    the bottom-right quarter (flat areas and hard edges).
    """
    out = bytearray(width * height * 4)
    for y in range(height):
        for x in range(width):
            r = x * 255 // max(width - 1, 1)
            g = y * 255 // max(height - 1, 1)
            b = ((x * 7) ^ (y * 13)) & 255
            if x % 23 == 0:
                r = g = b = 255
            if x >= width * 3 // 4 and y >= height // 2:
                r = g = b = 64 if (x // 4 + y // 4) % 2 else 192
            i = (y * width + x) * 4
            out[i : i + 4] = bytes((r, g, b, 255))
    return bytes(out)


def to_half(rgba8: bytes) -> bytes:
    """RGBA8 → RGBA16F (``v / 255``, round to nearest even)."""
    return struct.pack(f"<{len(rgba8)}e", *(v / 255.0 for v in rgba8))


def quantize(v: float) -> int:
    return int(min(max(v, 0.0), 1.0) * 255.0 + 0.5)


def download_rgba8(tex, half: bool) -> bytes:
    """The texture as tight 8-bit RGBA (16F values quantized)."""
    w, h = tex.width, tex.height
    bpp = 8 if half else 4
    data = tex.download()
    pitch = getattr(tex, "row_pitch", 0) or w * bpp
    rows = [data[y * pitch : y * pitch + w * bpp] for y in range(h)]
    raw = b"".join(rows)
    if not half:
        return raw
    return bytes(quantize(v) for v in struct.unpack(f"<{len(raw) // 2}e", raw))


def save_png(path: str, rgba8: bytes, width: int, height: int) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    Image.frombytes("RGBA", (width, height), rgba8).save(path)


# ----------------------------------------------------------------------
# lirtup-effects cases
# ----------------------------------------------------------------------
def upload(width: int, height: int, data: bytes, half: bool = False):
    from upscaler.vulkan import R16G16B16A16_FLOAT, R8G8B8A8_UNORM, Texture2D

    fmt = R16G16B16A16_FLOAT if half else R8G8B8A8_UNORM
    tex = Texture2D(width, height, format=fmt)
    tex.upload_subresources([(to_half(data) if half else data, 0, 0, width, height)])
    return tex


def run_effect(case: Dict, src_bytes: bytes, w: int, h: int) -> Tuple[object, bool]:
    """A native effect as the 1.x presenter runs it. Returns (output, is_16f)."""
    from upscaler.shaders import (
        CAS,
        LUT,
        Anime4KUpscaleS,
        Bloom,
        CRTEasymode,
        Deband,
        FilmGrain,
        LumaSharpen,
        Vignette,
    )
    from upscaler.vulkan import Texture2D

    effect = case["effect"]
    p = case.get("params", {})
    half = case.get("input") == "rgba16f"
    src = upload(w, h, src_bytes, half)

    if effect in ("cas", "bloom", "vignette", "grain", "lut"):
        # In place: CAS, bloom and vignette on the RGBA16F screen texture,
        # grain and LUT on the 8-bit one.
        if effect == "cas":
            shader = CAS()
            kwargs = {"strength": p["cas_strength"]}
        elif effect == "bloom":
            shader = Bloom()
            kwargs = {
                "strength": p["bloom_strength"],
                "threshold": p["bloom_threshold"],
                "radius": int(p["bloom_radius"]),
            }
        elif effect == "vignette":
            shader = Vignette()
            kwargs = {
                "strength": p["vignette_strength"],
                "radius": p["vignette_radius"],
                "falloff": p["vignette_falloff"],
            }
        elif effect == "grain":
            shader = FilmGrain()
            kwargs = {
                "strength": p["grain_strength"],
                "grain_size": p["grain_size"],
                "frame_index": 0,
            }
        else:
            shader = LUT(preset=case.get("preset", "identity"))
            kwargs = {"intensity": p["lut_intensity"]}
        shader.set_target_texture(src)
        shader.update_constants(**kwargs)
        shader.dispatch_auto()
        return src, half

    if effect == "anime4k":
        a4k = Anime4KUpscaleS()
        a4k.set_source_texture(src)
        a4k.dispatch()
        return a4k.output_texture, False

    if effect == "deband":
        shader = Deband()
        dst = Texture2D(w, h)
        kwargs = {"strength": p["deband_strength"], "frame_index": 0}
    elif effect == "luma_sharpen":
        shader = LumaSharpen()
        dst = Texture2D(w, h)
        kwargs = {
            "strength": p["luma_sharpen_strength"],
            "clamp": p["luma_sharpen_clamp"],
            "pattern": int(p["luma_sharpen_pattern"]),
            "offset_bias": p["luma_sharpen_offset_bias"],
        }
    elif effect == "crt_easymode":
        shader = CRTEasymode()
        dst = Texture2D(*case["dst"])
        kwargs = {}  # Magpie's defaults, as CrtParams::default()
    else:
        raise ValueError(f"unknown effect {effect}")
    shader.set_source_texture(src)
    shader.set_target_texture(dst)
    shader.update_constants(**kwargs)
    shader.dispatch_auto()
    return dst, False


def run_scaler(case: Dict, src_bytes: bytes, w: int, h: int) -> Tuple[object, bool]:
    """A final scaler as presenter._scale runs it, over the whole output."""
    from upscaler.shaders import (
        CatmullRomScaler,
        Clear,
        CopyScaler,
        FSRScaler,
        LanczosScaler,
        Linearize,
        NISScaler,
    )
    from upscaler.vulkan import R16G16B16A16_FLOAT, Texture2D

    kinds = {
        "copy": CopyScaler,
        "catmull_rom": CatmullRomScaler,
        "lanczos": LanczosScaler,
        "fsr": FSRScaler,
        "nis": NISScaler,
    }
    scaler = kinds[case["scaler"]]()
    # Scaler.configure with the default config (blur 1.0, antiring 0.8,
    # tight antiring, automatic radius), as ScalerParams in the Rust test.
    scaler.blur = 1.0
    scaler.antiring_strength = 0.8
    scaler.tight_antiring = True
    scaler.radius_override = None

    src = upload(w, h, src_bytes)
    dw, dh = case["dst"]
    screen = Texture2D(dw, dh, format=R16G16B16A16_FLOAT)
    scaler.set_target_texture(screen)
    rect = {
        "src_width": w,
        "src_height": h,
        "dst_width": dw,
        "dst_height": dh,
        "dst_x": 0,
        "dst_y": 0,
        "dst_w": dw,
        "dst_h": dh,
    }
    if not scaler.requires_linear_input:
        clear = Clear()
        clear.set_target_texture(screen)
        clear.update_constants(BLACK)
        clear.dispatch_auto()
        scaler.set_source_texture(src)
        scaler.update_constants(**rect)
        scaler.dispatch_auto()
        return screen, True

    linear = Texture2D(w, h, format=R16G16B16A16_FLOAT)
    linearize = Linearize()
    linearize.set_source_texture(src)
    linearize.set_target_texture(linear)
    linearize.dispatch_auto()
    scaler.set_source_texture(linear)
    scaler.update_constants(background_color=BLACK, **rect)
    scaler.dispatch_auto()
    return screen, True


def run_srcnn(case: Dict, src_bytes: bytes, w: int, h: int) -> Tuple[object, bool]:
    """One full-frame 2x stage, as UpscalerManager builds it."""
    from upscaler.pipeline.utils import collect_uav_names
    from upscaler.srcnn import SRCNN, PipelineFactory, dispatch_groups, load_model
    from upscaler.vulkan import Texture2D

    cfg = load_model(case["srcnn"], variant="")
    factory = PipelineFactory(cfg)
    src = upload(w, h, src_bytes)
    textures = {
        name: Texture2D(w, h, format=cfg.intermediate_format)
        for name in collect_uav_names(cfg)
    }
    output = Texture2D(w * 2, h * 2)
    textures["output"] = output
    stage = SRCNN(factory, w, h, src, textures)
    gx, gy = dispatch_groups(w, h, last_pass=False)
    sequence = [(pipe, gx, gy, 1, b"") for pipe in stage.pipelines]
    stage.pipelines[0].dispatch_sequence(sequence=sequence)
    return output, False


def effects_refs(only: Optional[List[str]]) -> List[str]:
    with open(os.path.join(EFFECTS_DIR, "cases.json"), encoding="utf-8") as f:
        manifest = json.load(f)
    w, h = manifest["input"]["width"], manifest["input"]["height"]
    src = make_input(w, h)
    failures = []
    for case in manifest["cases"]:
        name = case["name"]
        if only and name not in only:
            continue
        try:
            if "effect" in case:
                out, half = run_effect(case, src, w, h)
            elif "scaler" in case:
                out, half = run_scaler(case, src, w, h)
            else:
                out, half = run_srcnn(case, src, w, h)
            data = download_rgba8(out, half)
            save_png(os.path.join(EFFECTS_DIR, "refs", f"{name}.png"), data, out.width, out.height)
            print(f"ok   {name} ({out.width}x{out.height})")
        except Exception as e:  # keep going: one broken case is one missing PNG
            traceback.print_exc()
            print(f"FAIL {name}: {e}")
            failures.append(name)
    return failures


# ----------------------------------------------------------------------
# lirtup-magpiefx cases
# ----------------------------------------------------------------------
def verified_names() -> List[str]:
    """Effects whose ``run`` column is ``ok`` (``catalog::parse_report``)."""
    names = []
    with open(REPORT, encoding="utf-8") as f:
        for line in f:
            cells = [c.strip() for c in line.split("|")]
            if len(cells) != 6:
                continue
            m = re.fullmatch(r"`(.+)\.hlsl`", cells[1])
            if m and cells[4].startswith("ok"):
                names.append(m.group(1))
    return sorted(names)


def ref_name(effect: str) -> str:
    return effect.replace("/", "__")


def magpiefx_refs(only: Optional[List[str]]) -> List[str]:
    from upscaler.magpiefx import MagpieEffect

    with open(os.path.join(MAGPIEFX_DIR, "cases.json"), encoding="utf-8") as f:
        manifest = json.load(f)
    w, h = manifest["input"]["width"], manifest["input"]["height"]
    target = tuple(manifest["target"])
    src = upload(w, h, make_input(w, h))
    names = verified_names() if manifest["effects"] == "verified" else manifest["effects"]
    failures = []
    for name in names:
        if only and name not in only:
            continue
        try:
            fx = MagpieEffect.load(name)
            out = fx.process(src, target, frame_count=0)
            fmt = (fx.desc.textures[1].format or "R8G8B8A8_UNORM").upper()
            if fmt not in ("R8G8B8A8_UNORM", "R16G16B16A16_FLOAT"):
                raise ValueError(f"OUTPUT format {fmt} is not compared")
            data = download_rgba8(out, fmt == "R16G16B16A16_FLOAT")
            save_png(
                os.path.join(MAGPIEFX_DIR, "refs", f"{ref_name(name)}.png"),
                data,
                out.width,
                out.height,
            )
            print(f"ok   {name} ({out.width}x{out.height})")
        except Exception as e:
            traceback.print_exc()
            print(f"FAIL {name}: {e}")
            failures.append(name)
    return failures


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--only", choices=["effects", "magpiefx"], help="Only one crate's cases")
    ap.add_argument("--case", action="append", help="Only these cases or effects (repeatable)")
    args = ap.parse_args()

    from upscaler.vulkan import get_current_device

    print(f"GPU: {get_current_device().name}")
    failures = []
    if args.only in (None, "effects"):
        failures += effects_refs(args.case)
    if args.only in (None, "magpiefx"):
        failures += magpiefx_refs(args.case)
    if failures:
        print(f"{len(failures)} failed: {', '.join(failures)}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
