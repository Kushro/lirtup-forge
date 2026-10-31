#!/usr/bin/env python3
"""
Headless check and GPU benchmark for the hand-ported Magpie effects.

Uploads an image, runs one effect through `Compute.dispatch_sequence`
with Vulkan timestamp queries, prints the GPU time of the effect (first
dispatch start to last dispatch end, barriers included) and saves the
result as PNG for visual inspection.

Examples:
    python scripts/bench-magpie-effects.py img.png luma_sharpen -o out.png
    python scripts/bench-magpie-effects.py img.png anime4k --input-size 1920x1080
    python scripts/bench-magpie-effects.py img.png crt --output-size 2560x1440
"""

import argparse
import statistics
import sys

from PIL import Image

from upscaler.shaders import Anime4KUpscaleS, CRTEasymode, LumaSharpen
from upscaler.vulkan import Texture2D


def parse_size(text):
    w, h = text.lower().split("x")
    return int(w), int(h)


def upload(img: Image.Image) -> Texture2D:
    tex = Texture2D(img.width, img.height)
    tex.upload_subresources([(img.tobytes(), 0, 0, img.width, img.height)])
    return tex


def download(tex: Texture2D) -> Image.Image:
    data = tex.download()
    pitch = len(data) // tex.height
    rows = b"".join(
        data[y * pitch : y * pitch + tex.width * 4] for y in range(tex.height)
    )
    return Image.frombytes("RGBA", (tex.width, tex.height), rows)


def build(effect: str, src: Texture2D, output_size, params):
    """Return (dispatch sequence, output texture) for one effect."""
    if effect == "anime4k":
        a4k = Anime4KUpscaleS()
        a4k.set_source_texture(src)
        return a4k.sequence(), a4k.output_texture, a4k

    if effect == "luma_sharpen":
        shader, dst = LumaSharpen(), Texture2D(src.width, src.height)
    else:
        w, h = output_size or (src.width * 2, src.height * 2)
        shader, dst = CRTEasymode(), Texture2D(w, h)
    shader.set_source_texture(src)
    shader.set_target_texture(dst)
    shader.update_constants(**params)
    seq = [(shader.compute, (dst.width + 15) // 16, (dst.height + 15) // 16, 1, b"")]
    return seq, dst, shader


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("image")
    ap.add_argument("effect", choices=["luma_sharpen", "anime4k", "crt"])
    ap.add_argument("-o", "--output", help="Save the effect output as PNG")
    ap.add_argument("--input-size", type=parse_size, help="Resize input first (WxH)")
    ap.add_argument("--output-size", type=parse_size, help="CRT output size (WxH)")
    ap.add_argument("--iters", type=int, default=300)
    ap.add_argument("--warmup", type=int, default=30)
    ap.add_argument(
        "-p",
        "--param",
        action="append",
        default=[],
        help="Effect parameter as name=value (update_constants keyword)",
    )
    args = ap.parse_args()

    params = {}
    for item in args.param:
        k, v = item.split("=", 1)
        params[k] = float(v) if "." in v else int(v)

    img = Image.open(args.image).convert("RGBA")
    if args.input_size:
        img = img.resize(args.input_size, Image.LANCZOS)
    src = upload(img)

    seq, dst, keep_alive = build(args.effect, src, args.output_size, params)
    runner = seq[0][0]

    for _ in range(args.warmup):
        runner.dispatch_sequence(seq)

    times = []
    for _ in range(args.iters):
        _, ts = runner.dispatch_sequence(seq, timestamps=True)
        start = ts[3]
        end = ts[4 + 2 * (len(seq) - 1)]
        times.append((end - start) / 1e6)

    times.sort()
    print(
        f"{args.effect}: {src.width}x{src.height} -> {dst.width}x{dst.height}, "
        f"{len(seq)} dispatch(es), {args.iters} iters | GPU ms: "
        f"mean {statistics.fmean(times):.3f}  median {statistics.median(times):.3f}  "
        f"p95 {times[int(len(times) * 0.95) - 1]:.3f}  min {times[0]:.3f}"
    )

    if args.output:
        download(dst).convert("RGB").save(args.output)
        print(f"saved {args.output}")
    del keep_alive
    return 0


if __name__ == "__main__":
    sys.exit(main())
