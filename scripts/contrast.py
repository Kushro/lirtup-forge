#!/usr/bin/env python3
"""Measure the WCAG 2.1 contrast of the GUI's color roles, in both themes.

Reads crates/lirtup-ui/design/tokens.json (the default) or the design's
lf.css. Text and indicators need 4.5:1, outlines 3:1; the text on the
overlay's glass is measured over black and white. The same pairs are in
theme.rs (`contrast_pairs`), whose test runs them for the 20 seeds too.

    python3 scripts/contrast.py [tokens.json | lf.css]

Exits with 1 if a pair fails.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT = ROOT / "crates/lirtup-ui/design/tokens.json"


def load(path):
    text = path.read_text()
    if path.suffix == ".json":
        color = json.loads(text)["color"]
        return {"oscuro": color["dark"], "claro": color["light"]}

    def block(sel):
        m = re.search(re.escape(sel) + r"\s*\{(.*?)\n\}", text, re.S)
        return dict(re.findall(r"--([\w-]+):\s*([^;]+);", m.group(1)))

    return {"oscuro": block(".lf-dark, .lf"), "claro": block(".lf-light")}


def rgba(value):
    """(r, g, b, a) of #RRGGBB or rgba(r,g,b,a)."""
    value = value.strip()
    if value.startswith("#"):
        return tuple(int(value[i : i + 2], 16) for i in (1, 3, 5)) + (1.0,)
    nums = [float(x) for x in re.findall(r"[\d.]+", value)]
    return (nums[0], nums[1], nums[2], nums[3])


def over(fg, bg):
    """fg composited over the opaque bg."""
    r, g, b, a = rgba(fg)
    br, bgg, bb, _ = rgba(bg)
    mix = [round(c * a + d * (1 - a)) for c, d in ((r, br), (g, bgg), (b, bb))]
    return "#%02X%02X%02X" % tuple(mix)


def lum(h):
    c = [rgba(h)[i] / 255 for i in range(3)]
    c = [x / 12.92 if x <= 0.03928 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def ratio(a, b):
    la, lb = sorted([lum(a), lum(b)], reverse=True)
    return (la + 0.05) / (lb + 0.05)


PAIRS = [
    (f"on-{r}", r, 4.5)
    for r in [
        "primary",
        "primary-container",
        "secondary",
        "secondary-container",
        "tertiary",
        "tertiary-container",
        "error",
        "error-container",
        "success-container",
    ]
]
for s in [
    "surface",
    "surface-container-lowest",
    "surface-container-low",
    "surface-container",
    "surface-container-high",
    "surface-container-highest",
]:
    PAIRS += [(fg, s, 4.5) for fg in ["on-surface", "on-surface-variant", "primary", "tertiary", "error", "success"]]
    PAIRS.append(("outline", s, 3))
PAIRS += [
    ("inverse-on-surface", "inverse-surface", 4.5),
    ("inverse-primary", "inverse-surface", 4.5),
    ("surface", "success", 4.5),
]


def main():
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT
    failed = 0
    for name, t in load(path).items():
        bad = 0
        for fg, bg, need in PAIRS:
            r = ratio(t[fg], t[bg])
            if r < need:
                bad += 1
                print(f"{name}: {fg} sobre {bg} = {r:.2f} (< {need})")
        # The overlay's glass, over the worst backgrounds a game can have.
        glass = []
        for under in ("#000000", "#FFFFFF"):
            g = over(t["glass"], under)
            for fg in ("glass-content", "primary"):
                if fg == "primary" and name == "claro":
                    # The bar uses the dark scheme's primary on its glass.
                    continue
                r = ratio(t[fg], g)
                glass.append(f"{fg} {r:.1f}")
                if r < 4.5:
                    bad += 1
                    print(f"{name}: {fg} sobre el cristal ({under}) = {r:.2f} (< 4.5)")
        print(f"{name} fallos: {bad} de {len(PAIRS)} · cristal sobre negro y blanco: {', '.join(glass)}")
        failed += bad
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
