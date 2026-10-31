#!/usr/bin/env python3
"""
Try every MagpieFX effect in a directory with the generic loader.

For each ``*.hlsl`` file (``.hlsli`` and ``StubDefs`` are skipped) this
parses the effect, compiles every pass with each requested backend and,
optionally, creates the Vulkan pipelines and runs the effect once on a
1280x720 texture. The result is printed as a Markdown table plus the first
lines of every error.

Usage:
    python scripts/magpiefx-compile-all.py MAGPIE/src/Effects \
        [--backends dxc,glslang] [--run] [--report report.md]
"""

import argparse
import os
import sys
import time
import traceback
from typing import Dict, List

from upscaler.magpiefx import MagpieFXError, parse_effect
from upscaler.magpiefx.codegen import generate_pass_source
from upscaler.magpiefx.compiler import compile_hlsl


def _first_error(msg: str, limit: int = 6) -> str:
    lines = [ln for ln in msg.splitlines() if ln.strip()]
    errs = [ln for ln in lines if "error" in ln.lower()] or lines
    return "\n".join(errs[:limit])


def try_backend(path: str, backend: str) -> str:
    os.environ["UPSCALER_MAGPIEFX_COMPILER"] = backend
    with open(path, encoding="utf-8-sig") as f:
        source = f.read()
    name = os.path.splitext(os.path.basename(path))[0]
    desc = parse_effect(source, name)
    for p in desc.passes:
        compile_hlsl(
            generate_pass_source(desc, p.index),
            f"{name}_Pass{p.index}",
            include_dir=os.path.dirname(path),
            use_cache=False,
        )
    return "ok"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("effects_dir")
    ap.add_argument("--backends", default="dxc,glslang")
    ap.add_argument("--run", action="store_true", help="also create pipelines and dispatch once ('auto' backend)")
    ap.add_argument("--report", default=None)
    args = ap.parse_args()
    backends = args.backends.split(",")

    files: List[str] = []
    for dirpath, _, names in os.walk(args.effects_dir):
        for n in names:
            if n.endswith(".hlsl") and n != "StubDefs.hlsli":
                files.append(os.path.join(dirpath, n))
    files.sort(key=lambda p: os.path.relpath(p, args.effects_dir).lower())

    src_tex = None
    if args.run:
        from upscaler.vulkan import Texture2D

        src_tex = Texture2D(1280, 720)

    rows = []
    details: Dict[str, List[str]] = {}
    for path in files:
        rel = os.path.relpath(path, args.effects_dir)
        row = {"effect": rel}
        for b in backends:
            t = time.perf_counter()
            try:
                row[b] = try_backend(path, b)
            except MagpieFXError as e:
                row[b] = "FAIL"
                details.setdefault(rel, []).append(f"[{b}] {_first_error(str(e))}")
            except Exception as e:  # noqa: BLE001 - report everything
                row[b] = "FAIL"
                details.setdefault(rel, []).append(f"[{b}] {type(e).__name__}: {e}")
            row[b + "_s"] = time.perf_counter() - t
        if args.run:
            if not any(row[b] == "ok" for b in backends):
                row["run"] = "-"
            else:
                os.environ["UPSCALER_MAGPIEFX_COMPILER"] = "auto"
                try:
                    from upscaler.magpiefx import MagpieEffect

                    # Like a single effect in the app: BGRA in and out
                    fx = MagpieEffect.load(path, swap_input=True, swap_output=True)
                    fx.process(src_tex, (2560, 1440), profile=True)
                    note = f", {fx.swap_passes} SwapRB pass(es)" if fx.swap_passes else ""
                    row["run"] = f"ok {sum(fx.last_gpu_ms):.3f} ms{note}"
                except Exception as e:  # noqa: BLE001
                    row["run"] = "FAIL"
                    details.setdefault(rel, []).append(
                        f"[run] {_first_error(str(e)) or traceback.format_exc(limit=2)}"
                    )
        rows.append(row)
        print(rel, {k: v for k, v in row.items() if k != "effect" and not k.endswith("_s")}, flush=True)

    cols = backends + (["run"] if args.run else [])
    out = ["| Effect | " + " | ".join(cols) + " |", "|---" * (len(cols) + 1) + "|"]
    for r in rows:
        out.append(f"| `{r['effect']}` | " + " | ".join(r.get(c, "") for c in cols) + " |")
    totals = {c: sum(1 for r in rows if str(r.get(c, "")).startswith("ok")) for c in cols}
    out.append("")
    out.append(
        f"Totals over {len(rows)} effects: "
        + ", ".join(f"{c}: {n} ok" for c, n in totals.items())
    )
    out.append("")
    out.append("## Errors")
    for rel, errs in details.items():
        out.append(f"\n### `{rel}`\n```\n" + "\n".join(errs) + "\n```")
    report = "\n".join(out)
    if args.report:
        with open(args.report, "w") as f:
            f.write(report + "\n")
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main())
