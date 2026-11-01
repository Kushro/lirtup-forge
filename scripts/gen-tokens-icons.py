#!/usr/bin/env python3
"""The Material Symbols Rounded subset of lirtup-gui (part of gen-tokens).

Finds every icon the GUI names (any string literal under
crates/lirtup-ui/src that is a Material Symbols name, so the screens of
every front count without a list to keep), and writes:

- crates/lirtup-ui/assets/fonts/MaterialSymbolsRounded-subset.woff2: the
  variable font cut to those icons, with their ligatures, FILL 0..1 and
  wght 400..500 (GRAD 0, opsz 24 pinned). The webview looks icons up by
  name (ligatures).
- crates/lirtup-ui/assets/fonts/MaterialSymbolsRounded-Fill0.ttf and
  -Fill1.ttf: two static instances (FILL 0 at wght 400, FILL 1 at wght 500,
  as `.ms` and `.ms.fill` of the design), cut to the icons that the bar over
  the game names (string literals of crates/lirtup-pipeline/src/toolbar.rs
  and osd.rs). The bar draws them with ab_glyph, which reads no woff2;
  fixed instances spare it setting the four axes at run time.
- crates/lirtup-ui/design/icons.json: name -> code point of both sets, for
  the table of tokens.rs (the bar finds glyphs by code point: ab_glyph
  applies no ligatures).

Needs fontTools and brotli. The source font (Apache 2.0) is downloaded once
to ~/.cache/lirtup-forge/fonts-src, outside the repository:

    python3 scripts/gen-tokens-icons.py
"""

import json
import re
import sys
import urllib.request
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "crates/lirtup-ui"
CACHE = Path.home() / ".cache/lirtup-forge/fonts-src"
BASE = "https://raw.githubusercontent.com/google/material-design-icons/master/variablefont/"
NAME = "MaterialSymbolsRounded%5BFILL%2CGRAD%2Copsz%2Cwght%5D"
FONT = CACHE / "MaterialSymbolsRounded.ttf"
CODEPOINTS = CACHE / "MaterialSymbolsRounded.codepoints"
OUT_FONT = UI / "assets/fonts/MaterialSymbolsRounded-subset.woff2"
OUT_JSON = UI / "design/icons.json"
BAR_SOURCES = [ROOT / "crates/lirtup-pipeline/src/toolbar.rs", ROOT / "crates/lirtup-pipeline/src/osd.rs"]
# (FILL, wght) of each static instance of the bar.
BAR_INSTANCES = {"Fill0": (0, 400), "Fill1": (1, 500)}


def fetch():
    CACHE.mkdir(parents=True, exist_ok=True)
    for path, url in ((FONT, BASE + NAME + ".ttf"), (CODEPOINTS, BASE + NAME + ".codepoints")):
        if not path.exists():
            print(f"descargando {url}", file=sys.stderr)
            urllib.request.urlretrieve(url, path)


def used_icons(codepoints, sources):
    names = set()
    for rs in sources:
        names.update(re.findall(r'"([a-z0-9_]+)"', rs.read_text()))
    return sorted(n for n in names if n in codepoints)


def bar_instances(codepoints, icons):
    """The two static .ttf of the bar: only the glyphs of `icons`, by code
    point (no ligatures, no letters)."""
    for suffix, (fill, wght) in BAR_INSTANCES.items():
        font = TTFont(FONT)
        options = subset.Options()
        options.layout_features = []
        options.name_IDs = ["*"]
        options.notdef_outline = True
        options.hinting = False
        sub = subset.Subsetter(options)
        sub.populate(unicodes=[int(codepoints[n], 16) for n in icons])
        sub.subset(font)
        font = instancer.instantiateVariableFont(font, {"GRAD": 0, "opsz": 24, "FILL": fill, "wght": wght})
        out = UI / f"assets/fonts/MaterialSymbolsRounded-{suffix}.ttf"
        font.save(out)
        print(f"{len(icons)} íconos de la barra -> {out.relative_to(ROOT)} ({out.stat().st_size // 1024} KiB)")


def main():
    fetch()
    codepoints = dict(line.split() for line in CODEPOINTS.read_text().splitlines() if line.strip())
    icons = used_icons(codepoints, (UI / "src").rglob("*.rs"))
    bar = used_icons(codepoints, BAR_SOURCES)

    font = TTFont(FONT)
    cmap = font.getBestCmap()
    glyphs = [cmap[int(codepoints[n], 16)] for n in icons]

    options = subset.Options()
    options.layout_features = ["liga", "rlig", "calt", "ccmp"]
    # Only the ligatures of the icons kept: without this, every ligature that
    # the letters can form (all of them) would stay.
    options.layout_closure = False
    options.name_IDs = ["*"]
    options.notdef_outline = True
    sub = subset.Subsetter(options)
    letters = "abcdefghijklmnopqrstuvwxyz0123456789_"
    sub.populate(glyphs=glyphs, unicodes=[ord(c) for c in letters] + [int(codepoints[n], 16) for n in icons])
    sub.subset(font)
    # Cut the axes after the glyphs (much less to instance).
    font = instancer.instantiateVariableFont(
        font, {"GRAD": 0, "opsz": 24, "FILL": (0, 1), "wght": (400, 500)}
    )
    OUT_FONT.parent.mkdir(parents=True, exist_ok=True)
    font.flavor = "woff2"
    font.save(OUT_FONT)

    bar_instances(codepoints, bar)
    table = sorted(set(icons) | set(bar))
    OUT_JSON.write_text(json.dumps({n: codepoints[n] for n in table}, indent=2) + "\n")
    print(f"{len(icons)} íconos -> {OUT_FONT.relative_to(ROOT)} ({OUT_FONT.stat().st_size // 1024} KiB)")


if __name__ == "__main__":
    main()
