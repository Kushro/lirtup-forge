#!/usr/bin/env python3
"""design/tokens.json -> assets/tokens.css and design/tokens.rs (lirtup-gui).

tokens.json is the single source of the design's tokens. This writes:

- crates/lirtup-ui/assets/tokens.css: the color roles per theme
  (`[data-theme]`), shape, fonts and motion as CSS variables, which
  tailwind.css maps to classes. theme.rs overrides the colors from the seed
  on the root element; for the default seed they are the same values.
- crates/lirtup-ui/design/tokens.rs: the same tokens as Rust constants for
  the overlay's bar drawn in Vulkan (round 4, front T), with the icon code
  points of design/icons.json.

With `--colors FILE` (what `scripts/gen-tokens` passes), the colors computed
by theme.rs from the seed replace the ones in tokens.json first.

    python3 scripts/gen-tokens.py [--colors FILE]
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "crates/lirtup-ui"
TOKENS = UI / "design/tokens.json"
ICONS = UI / "design/icons.json"
CSS = UI / "assets/tokens.css"
RS = UI / "design/tokens.rs"


def px(value):
    return float(str(value).removesuffix("px"))


def rgba(value):
    value = value.strip()
    if value.startswith("#"):
        return [int(value[i : i + 2], 16) for i in (1, 3, 5)] + [255]
    nums = [float(x) for x in re.findall(r"[\d.]+", value)]
    return [int(nums[0]), int(nums[1]), int(nums[2]), round(nums[3] * 255)]


def bezier(easing):
    return [float(x) for x in re.findall(r"-?\d*\.?\d+", easing)]


def write_css(t):
    out = [
        "/* Generated from design/tokens.json by scripts/gen-tokens.py; do not edit",
        "   by hand. The theme from the seed color (theme.rs) overrides the colors",
        "   in the root element's style. */",
        "",
    ]
    for selector, scheme, name in (
        (':root, [data-theme="dark"]', "dark", "dark"),
        ('[data-theme="light"]', "light", "light"),
    ):
        out.append(f"{selector} {{")
        out.append(f"  color-scheme: {scheme};")
        out += [f"  --{role}: {value};" for role, value in t["color"][name].items()]
        out += ["}", ""]
    out.append(":root {")
    out += [f"  --shape-{k}: {v};" for k, v in t["shape"].items()]
    out += [f"  --font-{k}: {v};" for k, v in t["font"].items()]
    for k, m in t["motion"].items():
        if "easing" in m:
            out.append(f"  --ease-{k}: {m['easing']};")
        out.append(f"  --dur-{k}: {m['duration']};")
    out += ["}", ""]
    CSS.write_text("\n".join(out))


def const(name):
    return re.sub(r"[^A-Z0-9]", "_", name.upper())


def write_rs(t):
    icons = json.loads(ICONS.read_text()) if ICONS.exists() else {}
    out = [
        "//! Generated from `design/tokens.json` by `scripts/gen-tokens.py`; do not",
        "//! edit by hand. The design tokens for the overlay's bar drawn in Vulkan",
        "//! (round 4, front T): the same colors, shapes, type and motion as the GUI.",
        "",
        "/// A color role: name and RGBA (sRGB, straight alpha).",
        "pub type Role = (&'static str, [u8; 4]);",
        "",
    ]
    for name in ("dark", "light"):
        roles = t["color"][name]
        out.append(f"/// The {name} scheme for the seed `{t['seed']}`.")
        out.append(f"pub const {name.upper()}: [Role; {len(roles)}] = [")
        out += [f'    ("{r}", {rgba(v)}),' for r, v in roles.items()]
        out += ["];", ""]
    out.append("/// Corner radii, px.")
    out += [f"pub const SHAPE_{const(k)}: f32 = {px(v):.1f};" for k, v in t["shape"].items() if k != "full"]
    out += ["", "/// A step of the M3 type scale (px, weight).", "#[derive(Debug, Clone, Copy, PartialEq)]"]
    out += [
        "pub struct TypeStyle {",
        "    pub size: f32,",
        "    pub line: f32,",
        "    pub weight: u16,",
        "    pub tracking: f32,",
        "}",
        "",
    ]
    for k, s in t["type"].items():
        out.append(
            f"pub const TYPE_{const(k)}: TypeStyle = TypeStyle {{ size: {px(s['size']):.1f}, "
            f"line: {px(s['line']):.1f}, weight: {s['weight']}, tracking: {px(s.get('tracking', 0)):.2f} }};"
        )
    out += [
        "",
        "/// A motion token: cubic Bézier control points and duration.",
        "#[derive(Debug, Clone, Copy, PartialEq)]",
        "pub struct Motion {",
        "    pub bezier: [f32; 4],",
        "    pub ms: u32,",
        "}",
        "",
    ]
    for k, m in t["motion"].items():
        b = bezier(m.get("easing", "cubic-bezier(0.31, 0.94, 0.34, 1.00)"))
        ms = int(m["duration"].removesuffix("ms"))
        out.append(f"pub const MOTION_{const(k)}: Motion = Motion {{ bezier: {b}, ms: {ms} }};")
    out += [
        "",
        "/// Material Symbols Rounded code points of the icons the GUI uses",
        "/// (`design/icons.json`): `ab_glyph` applies no ligatures.",
        f"pub const ICONS: [(&str, u32); {len(icons)}] = [",
    ]
    out += [f'    ("{n}", 0x{c.upper()}),' for n, c in icons.items()]
    out += ["];", ""]
    RS.write_text("\n".join(out))


def main():
    t = json.loads(TOKENS.read_text())
    if "--colors" in sys.argv:
        colors = json.loads(Path(sys.argv[sys.argv.index("--colors") + 1]).read_text())
        for name in ("dark", "light"):
            t["color"][name] = {role: colors[name][role] for role in t["color"][name]}
        TOKENS.write_text(json.dumps(t, indent=2, ensure_ascii=False) + "\n")
    write_css(t)
    write_rs(t)
    print(f"{CSS.relative_to(ROOT)} y {RS.relative_to(ROOT)} escritos")


if __name__ == "__main__":
    main()
