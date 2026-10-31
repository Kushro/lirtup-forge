"""
Red/blue swap folded into the generated shaders.

Frames travel through the app as BGRA bytes in R8G8B8A8 textures, while
MagpieFX effects expect RGBA. Instead of extra passes, the *preprocessed*
pass source (macros already expanded by the compiler) is rewritten so that:

* every read of ``INPUT`` returns swapped channels:
  ``INPUT.Load/Sample*(...)`` and ``INPUT[...]`` are wrapped in
  ``__MP_SwapRB(...)``, ``GatherRed`` and ``GatherBlue`` (and plain
  ``Gather``, which reads red) trade places;
* every store ``OUTPUT[...] = value;`` stores ``__MP_SwapRB(value)``.

Intermediate textures stay RGBA. The Magpie source files are not modified.
If ``INPUT`` or ``OUTPUT`` is used any other way (e.g. passed to a function
taking a ``Texture2D``, as FXAA and SMAA do), :class:`SwapUnsupported` is
raised and the caller falls back to a separate swap pass.
"""

from __future__ import annotations

import re
from typing import Tuple

SWAP_FUNC = (
    "float4 __MP_SwapRB(float4 v) { return v.bgra; }\n"
)

_SWAP_READS = ("Load", "Sample", "SampleLevel", "SampleGrad", "SampleBias")
_GATHER = {
    "Gather": "GatherBlue",
    "GatherRed": "GatherBlue",
    "GatherBlue": "GatherRed",
    "GatherGreen": "GatherGreen",
    "GatherAlpha": "GatherAlpha",
}
_PASSTHROUGH = ("GetDimensions",)


class SwapUnsupported(Exception):
    """The pass uses INPUT/OUTPUT in a way the rewriter cannot swap."""


def _match(s: str, i: int, open_c: str, close_c: str) -> int:
    """Index just past the bracket matching ``s[i] == open_c``."""
    depth = 0
    for j in range(i, len(s)):
        c = s[j]
        if c == open_c:
            depth += 1
        elif c == close_c:
            depth -= 1
            if depth == 0:
                return j + 1
    raise SwapUnsupported(f"unbalanced '{open_c}'")


def _statement_end(s: str, i: int) -> int:
    """Index of the ``;`` ending the expression that starts at *i*."""
    depth = 0
    for j in range(i, len(s)):
        c = s[j]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == ";" and depth == 0:
            return j
    raise SwapUnsupported("assignment without ';'")


def _skip_ws(s: str, i: int) -> int:
    while i < len(s) and s[i].isspace():
        i += 1
    return i


def rewrite(source: str, swap_input: bool, swap_output: bool) -> Tuple[str, bool, bool]:
    """
    Rewrite *source* and return ``(new_source, input_swapped, output_swapped)``.

    The flags tell which sides were actually rewritten (a side is only
    reported when the pass touches that texture).
    """
    names = []
    if swap_input:
        names.append("INPUT")
    if swap_output:
        names.append("OUTPUT")
    if not names:
        return source, False, False
    token = re.compile(r"(?<![\w.])(" + "|".join(names) + r")(?!\w)")
    used = set()
    out = _rewrite(source, token, used)
    if used:
        out = SWAP_FUNC + out
    return out, "INPUT" in used, "OUTPUT" in used


def _rewrite(s: str, token: re.Pattern, used: set) -> str:
    out = []
    pos = 0
    while True:
        m = token.search(s, pos)
        if not m:
            out.append(s[pos:])
            return "".join(out)
        name = m.group(1)
        out.append(s[pos : m.start()])
        i = _skip_ws(s, m.end())

        # Declaration: "... NAME : register(...)"
        if s.startswith(":", i):
            out.append(s[m.start() : i])
            pos = i
            continue

        if name == "INPUT":
            if s.startswith(".", i):
                j = _skip_ws(s, i + 1)
                mm = re.match(r"\w+", s[j:])
                method = mm.group(0) if mm else ""
                k = j + len(method)
                if method in _GATHER:
                    out.append(f"INPUT.{_GATHER[method]}")
                    pos = k
                elif method in _SWAP_READS:
                    k2 = _skip_ws(s, k)
                    if not s.startswith("(", k2):
                        raise SwapUnsupported(f"INPUT.{method} without call")
                    end = _match(s, k2, "(", ")")
                    inner = _rewrite(s[k2:end], token, used)
                    out.append(f"__MP_SwapRB(INPUT.{method}{inner})")
                    pos = end
                elif method in _PASSTHROUGH:
                    out.append(s[m.start() : k])
                    pos = k
                    continue
                else:
                    raise SwapUnsupported(f"INPUT.{method}")
                used.add("INPUT")
                continue
            if s.startswith("[", i):
                end = _match(s, i, "[", "]")
                inner = _rewrite(s[i:end], token, used)
                out.append(f"__MP_SwapRB(INPUT{inner})")
                used.add("INPUT")
                pos = end
                continue
            raise SwapUnsupported("INPUT used as a value (e.g. passed to a function)")

        # OUTPUT
        if s.startswith("[", i):
            end = _match(s, i, "[", "]")
            index = _rewrite(s[i:end], token, used)
            k = _skip_ws(s, end)
            if not s.startswith("=", k) or s.startswith("==", k):
                raise SwapUnsupported("OUTPUT read or compound assignment")
            semi = _statement_end(s, k + 1)
            value = _rewrite(s[k + 1 : semi], token, used)
            out.append(f"OUTPUT{index} = __MP_SwapRB({value.strip()})")
            used.add("OUTPUT")
            pos = semi
            continue
        if s.startswith(".", i):
            j = _skip_ws(s, i + 1)
            mm = re.match(r"\w+", s[j:])
            if mm and mm.group(0) in _PASSTHROUGH:
                out.append(s[m.start() : j + len(mm.group(0))])
                pos = j + len(mm.group(0))
                continue
        raise SwapUnsupported("OUTPUT used as a value (e.g. passed to a function)")
