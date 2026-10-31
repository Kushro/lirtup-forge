#!/usr/bin/env python3
"""
Run MagpieFX effects headless on an image and report GPU time per frame.

Each effect is run on the input image (uploaded as R8G8B8A8_UNORM, like the
captured frame in the app) for a number of warm-up and measured iterations.
GPU time is the sum of the per-pass Vulkan timestamp deltas recorded by
``Compute.dispatch_sequence(timestamps=True)``; the median over the measured
iterations is reported (with --bgra fold,pass the variants alternate in
batches of 20 frames). The OUTPUT of the last run is saved as a PNG.

Usage:
    python scripts/magpiefx-bench.py IMAGE EFFECT [EFFECT...] \
        [--resize 1280x720] [--target 3840x2160] [--bgra fold,pass] [--iters 200] [--out DIR] [--param EFFECT:name=value]

Example:
    python scripts/magpiefx-bench.py docs/assets/comparisons/fatamoru/original.png \
        LumaSharpen Anime4K_Upscale_S CRT_Easymode --resize 1280x720 --target 2560x1440
"""

import argparse
import os
import statistics
import sys
import time

from PIL import Image

from upscaler.magpiefx import SWAP_RB_EFFECT, MagpieEffect
from upscaler.vulkan import R8G8B8A8_UNORM, Texture2D, get_current_device


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("image")
    ap.add_argument("effects", nargs="+")
    ap.add_argument("--resize", default=None, help="WxH to resize the input image to (Lanczos) before uploading")
    ap.add_argument("--target", default=None, help="WxH for effects without fixed OUTPUT size (default: input size)")
    ap.add_argument("--iters", type=int, default=200)
    ap.add_argument("--warmup", type=int, default=20)
    ap.add_argument("--out", default=".")
    ap.add_argument(
        "--bgra",
        default="none",
        help="none: RGBA input; fold: BGRA input, swap folded into the shaders "
        "(what the app does); pass: BGRA input, separate SwapRB passes. "
        "A comma list (fold,pass) runs the variants interleaved",
    )
    ap.add_argument("--param", action="append", default=[], help="EFFECT:name=value")
    args = ap.parse_args()

    img = Image.open(args.image).convert("RGBA")
    if args.resize:
        img = img.resize(tuple(int(v) for v in args.resize.split("x")), Image.LANCZOS)
    w, h = img.size
    target = tuple(int(v) for v in args.target.split("x")) if args.target else (w, h)

    src = Texture2D(w, h, format=R8G8B8A8_UNORM)
    src.upload_subresources([(img.tobytes(), 0, 0, w, h)])

    print(f"GPU: {get_current_device().name}")
    print(f"Input: {args.image} {w}x{h}, target {target[0]}x{target[1]}")
    os.makedirs(args.out, exist_ok=True)

    modes = args.bgra.split(",")
    if modes != ["none"]:
        # Upload BGRA bytes like a captured frame
        src.upload_subresources([(img.tobytes("raw", "BGRA"), 0, 0, w, h)])

    for name in args.effects:
        params = {}
        for spec in args.param:
            eff, _, kv = spec.partition(":")
            if eff == name:
                k, _, v = kv.partition("=")
                params[k] = float(v)

        variants = {}
        for mode in modes:
            t0 = time.perf_counter()
            if mode == "pass":
                # Separate SwapRB passes around the effect (the old approach)
                fx = MagpieEffect.load(name, params)
                fx._pre = MagpieEffect.load(SWAP_RB_EFFECT)
                fx._post = MagpieEffect.load(SWAP_RB_EFFECT)
                fx.swap_passes = 2
            else:
                fold = mode == "fold"
                fx = MagpieEffect.load(name, params, swap_input=fold, swap_output=fold)
            load_ms = (time.perf_counter() - t0) * 1000
            for _ in range(args.warmup):
                fx.process(src, target)
            variants[mode] = (fx, load_ms, [], [], [])

        # Interleave the variants in batches so drift affects them equally
        batch = 20
        for start in range(0, args.iters, batch):
            for mode in modes:
                fx, _, totals, per_pass, wall = variants[mode]
                for _ in range(min(batch, args.iters - start)):
                    t = time.perf_counter()
                    fx.process(src, target, profile=True)
                    wall.append((time.perf_counter() - t) * 1000)
                    totals.append(sum(fx.last_gpu_ms))
                    per_pass.append(fx.last_gpu_ms)

        for mode in modes:
            fx, load_ms, totals, per_pass, wall = variants[mode]
            out = fx._post.output if fx._post is not None else fx.output
            data = out.download()[: out.width * out.height * 4]
            raw_mode = "RGBA" if mode == "none" else "BGRA"
            suffix = "" if len(modes) == 1 else f"-{mode}"
            Image.frombytes("RGBA", (out.width, out.height), data, "raw", raw_mode).convert(
                "RGB"
            ).save(os.path.join(args.out, f"{name}{suffix}.png"))

            passes = ", ".join(
                f"{statistics.median(v):.3f}" for v in zip(*per_pass)
            )
            label = name if len(modes) == 1 else f"{name} [{mode}]"
            print(
                f"{label}: {out.width}x{out.height}, load {load_ms:.0f} ms, "
                f"GPU median {statistics.median(totals):.3f} ms/frame "
                f"(p95 {sorted(totals)[int(len(totals) * 0.95) - 1]:.3f}; passes: {passes}), "
                f"submit+wait median {statistics.median(wall):.3f} ms"
            )
    return 0


if __name__ == "__main__":
    sys.exit(main())
