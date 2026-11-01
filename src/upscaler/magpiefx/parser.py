"""
Parser for the MagpieFX effect format (version 4).

MagpieFX files are HLSL sources annotated with ``//!`` directives that
describe parameters, textures, samplers and the passes of an effect. The
format is documented at https://github.com/Blinue/Magpie/wiki/MagpieFX-(EN);
this parser follows the behaviour of Magpie's ``EffectCompiler.cpp`` so that
the effects shipped with Magpie load unmodified.

The parser only reads the source. Code generation lives in
:mod:`.codegen` and execution in :mod:`.effect`.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

from .formats import FORMATS

MAGPIE_FX_VERSION = 4


class MagpieFXError(Exception):
    """Raised when an effect cannot be parsed, compiled or executed."""


@dataclass
class EffectParameter:
    name: str
    type: str  # "float" or "int"
    default: float
    min: float
    max: float
    step: float
    label: str = ""


@dataclass
class EffectTexture:
    name: str
    format: str = "R8G8B8A8_UNORM"  # "" for a //!SOURCE texture without //!FORMAT
    width_expr: str = ""
    height_expr: str = ""
    source: str = ""  # lookup texture file, relative to the effect


@dataclass
class EffectSampler:
    name: str
    filter: str = "POINT"  # POINT | LINEAR
    address: str = "CLAMP"  # CLAMP | WRAP


@dataclass
class EffectPass:
    index: int  # 1-based
    inputs: List[int] = field(default_factory=list)  # texture indices
    outputs: List[int] = field(default_factory=list)
    block_size: Tuple[int, int] = (0, 0)
    num_threads: Tuple[int, int, int] = (1, 1, 1)
    ps_style: bool = False
    desc: str = ""
    code: str = ""


@dataclass
class EffectDesc:
    name: str
    sort_name: str = ""
    use_muladd: bool = False
    use_dynamic: bool = False
    supports_fp16: bool = False
    header_code: str = ""  # e.g. an #include in the header block
    params: List[EffectParameter] = field(default_factory=list)
    textures: List[EffectTexture] = field(default_factory=list)  # [INPUT, OUTPUT, ...]
    samplers: List[EffectSampler] = field(default_factory=list)
    commons: List[str] = field(default_factory=list)
    passes: List[EffectPass] = field(default_factory=list)
    warnings: List[str] = field(default_factory=list)

    @property
    def output(self) -> EffectTexture:
        return self.textures[1]

    def param(self, name: str) -> Optional[EffectParameter]:
        for p in self.params:
            if p.name == name:
                return p
        return None


# ----------------------------------------------------------------------
# Comment removal
# ----------------------------------------------------------------------
def remove_comments(source: str) -> str:
    """
    Strip ``//`` and ``/* */`` comments but keep ``//!`` directives.

    Newlines of line comments are kept so that directive lines stay
    separated. Block comments are removed entirely, like in Magpie.
    String literals are not special-cased (HLSL has almost none).
    """
    out: List[str] = []
    i, n = 0, len(source)
    while i < n:
        c = source[i]
        if c == "/" and i + 1 < n:
            nxt = source[i + 1]
            if nxt == "/" and not source.startswith("//!", i):
                j = source.find("\n", i)
                if j < 0:
                    break
                i = j  # keep the newline
                continue
            if nxt == "*":
                j = source.find("*/", i + 2)
                if j < 0:
                    raise MagpieFXError("Unterminated block comment")
                i = j + 2
                continue
        out.append(c)
        i += 1
    return "".join(out)


# ----------------------------------------------------------------------
# Block splitting
# ----------------------------------------------------------------------
_BLOCK_KINDS = ("PARAMETER", "TEXTURE", "SAMPLER", "COMMON", "PASS")
_DIRECTIVE_RE = re.compile(r"^[ \t]*//![ \t]*([A-Za-z_][A-Za-z0-9_]*)[ \t]*(.*?)[ \t]*$")


def _split_blocks(source: str) -> List[Tuple[str, List[str]]]:
    """
    Split the comment-free source into ``(kind, lines)`` blocks.

    A new block starts at every line whose directive is one of
    :data:`_BLOCK_KINDS`; everything before the first one is the header.
    """
    blocks: List[Tuple[str, List[str]]] = [("HEADER", [])]
    for line in source.split("\n"):
        m = _DIRECTIVE_RE.match(line)
        if m and m.group(1).upper() in _BLOCK_KINDS:
            blocks.append((m.group(1).upper(), [line]))
        else:
            blocks[-1][1].append(line)
    return blocks


def _take_directives(
    lines: List[str],
) -> Tuple[List[Tuple[str, str]], str]:
    """
    Consume the leading run of ``//!`` lines (blank lines allowed between
    them) and return ``([(KEY, value), ...], remaining_code)``.
    """
    directives: List[Tuple[str, str]] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if not line.strip():
            i += 1
            continue
        m = _DIRECTIVE_RE.match(line)
        if not m:
            break
        directives.append((m.group(1).upper(), m.group(2)))
        i += 1
    return directives, "\n".join(lines[i:])


def _parse_number(text: str, kind: str, what: str) -> float:
    text = text.strip()
    try:
        if kind == "int":
            return int(text)
        return float(text.rstrip("fF"))
    except ValueError:
        raise MagpieFXError(f"{what}: invalid {kind} value '{text}'") from None


_DECL_RE = re.compile(r"^\s*(\w+)\s+(\w+)\s*;\s*$", re.S)


# ----------------------------------------------------------------------
# Individual block resolvers
# ----------------------------------------------------------------------
def _resolve_header(desc: EffectDesc, lines: List[str]) -> None:
    directives, code = _take_directives(lines)
    if not directives or directives[0] != ("MAGPIE", "EFFECT"):
        raise MagpieFXError("Missing '//!MAGPIE EFFECT' header")

    version_seen = False
    for key, value in directives[1:]:
        if key == "VERSION":
            if value.strip() != str(MAGPIE_FX_VERSION):
                raise MagpieFXError(
                    f"Unsupported MagpieFX version {value!r} "
                    f"(only {MAGPIE_FX_VERSION} is supported)"
                )
            version_seen = True
        elif key == "SORT_NAME":
            desc.sort_name = value
        elif key == "USE":
            for flag in (f.strip().upper() for f in value.split(",")):
                if flag == "MULADD":
                    desc.use_muladd = True
                elif flag == "_DYNAMIC":
                    desc.use_dynamic = True
                elif flag:
                    desc.warnings.append(f"Unknown USE flag '{flag}'")
        elif key == "CAPABILITY":
            for flag in (f.strip().upper() for f in value.split(",")):
                if flag == "FP16":
                    desc.supports_fp16 = True
                elif flag:
                    desc.warnings.append(f"Unknown CAPABILITY flag '{flag}'")
        else:
            desc.warnings.append(f"Unknown header directive '{key}'")

    if not version_seen:
        raise MagpieFXError("Missing '//!VERSION' directive")

    # The header may only contain an #include line
    code = code.strip()
    if code:
        if code.startswith("#include") and "\n" not in code:
            desc.header_code = code
        else:
            raise MagpieFXError(
                "Unexpected code in the header block (only #include is allowed)"
            )


def _resolve_parameter(desc: EffectDesc, lines: List[str], idx: int) -> None:
    directives, code = _take_directives(lines)
    opts: Dict[str, str] = {}
    for key, value in directives[1:]:
        if key in opts:
            raise MagpieFXError(f"Parameter #{idx}: duplicated '{key}'")
        if key in ("DEFAULT", "LABEL", "MIN", "MAX", "STEP"):
            opts[key] = value
        else:
            desc.warnings.append(f"Parameter #{idx}: unknown directive '{key}'")
    for req in ("DEFAULT", "MIN", "MAX", "STEP"):
        if req not in opts:
            raise MagpieFXError(f"Parameter #{idx}: missing '//!{req}'")

    m = _DECL_RE.match(code)
    if not m or m.group(1) not in ("float", "int"):
        raise MagpieFXError(
            f"Parameter #{idx}: expected 'float name;' or 'int name;', got {code.strip()!r}"
        )
    kind, name = m.group(1), m.group(2)
    what = f"Parameter '{name}'"
    p = EffectParameter(
        name=name,
        type=kind,
        default=_parse_number(opts["DEFAULT"], kind, what),
        min=_parse_number(opts["MIN"], kind, what),
        max=_parse_number(opts["MAX"], kind, what),
        step=_parse_number(opts["STEP"], kind, what),
        label=opts.get("LABEL", ""),
    )
    if not (p.min <= p.default <= p.max):
        raise MagpieFXError(f"{what}: default outside [min, max]")
    desc.params.append(p)


def _resolve_texture(desc: EffectDesc, lines: List[str], idx: int) -> None:
    directives, code = _take_directives(lines)
    tex = EffectTexture(name="")
    seen = set()
    for key, value in directives[1:]:
        if key in seen:
            raise MagpieFXError(f"Texture #{idx}: duplicated '{key}'")
        seen.add(key)
        if key == "SOURCE":
            tex.source = value
        elif key == "FORMAT":
            if value not in FORMATS:
                raise MagpieFXError(f"Texture #{idx}: unknown format '{value}'")
            tex.format = value
        elif key == "WIDTH":
            tex.width_expr = re.sub(r"\s+", "", value)
        elif key == "HEIGHT":
            tex.height_expr = re.sub(r"\s+", "", value)
        else:
            desc.warnings.append(f"Texture #{idx}: unknown directive '{key}'")

    if ("WIDTH" in seen) != ("HEIGHT" in seen):
        raise MagpieFXError(f"Texture #{idx}: WIDTH and HEIGHT must come in pairs")
    if "SOURCE" in seen and "WIDTH" in seen:
        raise MagpieFXError(f"Texture #{idx}: SOURCE excludes WIDTH/HEIGHT")

    m = _DECL_RE.match(code)
    if not m or m.group(1) != "Texture2D":
        raise MagpieFXError(
            f"Texture #{idx}: expected 'Texture2D name;', got {code.strip()!r}"
        )
    name = m.group(2)

    if name == "INPUT":
        if seen:
            raise MagpieFXError("INPUT texture cannot have options")
        return
    if name == "OUTPUT":
        if "SOURCE" in seen or "FORMAT" in seen:
            raise MagpieFXError("OUTPUT texture only accepts WIDTH/HEIGHT")
        desc.textures[1].width_expr = tex.width_expr
        desc.textures[1].height_expr = tex.height_expr
        return

    if "FORMAT" not in seen:
        if "SOURCE" not in seen:
            raise MagpieFXError(f"Texture '{name}': missing '//!FORMAT'")
        # Taken from the file when the texture is loaded (Magpie's UNKNOWN)
        tex.format = ""
    tex.name = name
    desc.textures.append(tex)


def _resolve_sampler(desc: EffectDesc, lines: List[str], idx: int) -> None:
    directives, code = _take_directives(lines)
    sam = EffectSampler(name="")
    seen = set()
    for key, value in directives[1:]:
        if key in seen:
            raise MagpieFXError(f"Sampler #{idx}: duplicated '{key}'")
        seen.add(key)
        value = value.upper()
        if key == "FILTER":
            if value not in ("POINT", "LINEAR"):
                raise MagpieFXError(f"Sampler #{idx}: invalid FILTER '{value}'")
            sam.filter = value
        elif key == "ADDRESS":
            if value not in ("CLAMP", "WRAP"):
                raise MagpieFXError(f"Sampler #{idx}: invalid ADDRESS '{value}'")
            sam.address = value
        else:
            desc.warnings.append(f"Sampler #{idx}: unknown directive '{key}'")
    if "FILTER" not in seen:
        raise MagpieFXError(f"Sampler #{idx}: missing '//!FILTER'")

    m = _DECL_RE.match(code)
    if not m or m.group(1) != "SamplerState":
        raise MagpieFXError(
            f"Sampler #{idx}: expected 'SamplerState name;', got {code.strip()!r}"
        )
    sam.name = m.group(2)
    desc.samplers.append(sam)


def _resolve_common(desc: EffectDesc, lines: List[str], idx: int) -> None:
    directives, code = _take_directives(lines)
    if len(directives) > 1:
        raise MagpieFXError(f"Common #{idx}: COMMON blocks take no options")
    desc.commons.append(code)


def _resolve_passes(desc: EffectDesc, blocks: List[List[str]]) -> None:
    parsed: List[Tuple[int, List[Tuple[str, str]], str]] = []
    for lines in blocks:
        directives, code = _take_directives(lines)
        try:
            number = int(directives[0][1])
        except ValueError:
            raise MagpieFXError(f"Invalid pass number {directives[0][1]!r}") from None
        parsed.append((number, directives[1:], code))
    parsed.sort(key=lambda t: t[0])
    for i, (number, _, _) in enumerate(parsed):
        if number != i + 1:
            raise MagpieFXError("PASS numbers must be consecutive starting at 1")

    tex_index = {t.name: i for i, t in enumerate(desc.textures)}
    n_passes = len(parsed)

    for i, (number, directives, code) in enumerate(parsed):
        p = EffectPass(index=number, code=code)
        seen = set()
        available = dict(tex_index)
        for key, value in directives:
            if key in seen:
                raise MagpieFXError(f"Pass {number}: duplicated '{key}'")
            seen.add(key)
            if key == "IN":
                for name in (s.strip() for s in value.split(",")):
                    j = available.pop(name, None)
                    if j is None or j == 1:
                        raise MagpieFXError(f"Pass {number}: invalid input '{name}'")
                    p.inputs.append(j)
            elif key == "OUT":
                names = [s.strip() for s in value.split(",")]
                if i == n_passes - 1:
                    if names != ["OUTPUT"]:
                        raise MagpieFXError(
                            f"Pass {number}: the last pass must write only OUTPUT"
                        )
                    p.outputs.append(1)
                    continue
                if len(names) > 8:
                    raise MagpieFXError(f"Pass {number}: at most 8 outputs")
                for name in names:
                    j = available.pop(name, None)
                    if j is None or j in (0, 1) or desc.textures[j].source:
                        raise MagpieFXError(f"Pass {number}: invalid output '{name}'")
                    p.outputs.append(j)
            elif key == "BLOCK_SIZE":
                nums = [int(s) for s in value.split(",")]
                if not 1 <= len(nums) <= 2 or min(nums) <= 0:
                    raise MagpieFXError(f"Pass {number}: invalid BLOCK_SIZE")
                p.block_size = (nums[0], nums[-1])
            elif key == "NUM_THREADS":
                nums = [int(s) for s in value.split(",")]
                if not 1 <= len(nums) <= 3:
                    raise MagpieFXError(f"Pass {number}: invalid NUM_THREADS")
                nums += [1] * (3 - len(nums))
                p.num_threads = (nums[0], nums[1], nums[2])
            elif key == "STYLE":
                if value == "PS":
                    p.ps_style = True
                    p.block_size = (16, 16)
                    p.num_threads = (64, 1, 1)
                elif value != "CS":
                    raise MagpieFXError(f"Pass {number}: invalid STYLE '{value}'")
            elif key == "DESC":
                p.desc = value
            else:
                desc.warnings.append(f"Pass {number}: unknown directive '{key}'")

        if "IN" not in seen or "OUT" not in seen:
            raise MagpieFXError(f"Pass {number}: IN and OUT are required")
        if p.ps_style and ("BLOCK_SIZE" in seen or "NUM_THREADS" in seen):
            raise MagpieFXError(
                f"Pass {number}: PS style passes cannot set BLOCK_SIZE/NUM_THREADS"
            )
        if not p.ps_style and ("BLOCK_SIZE" not in seen or "NUM_THREADS" not in seen):
            raise MagpieFXError(
                f"Pass {number}: CS style passes need BLOCK_SIZE and NUM_THREADS"
            )
        p.desc = p.desc or f"Pass {number}"
        desc.passes.append(p)


# ----------------------------------------------------------------------
# Entry point
# ----------------------------------------------------------------------
def parse_effect(source: str, name: str = "effect") -> EffectDesc:
    """
    Parse a MagpieFX source into an :class:`EffectDesc`.

    Raises:
        MagpieFXError: on any syntax or semantic error.
    """
    source = remove_comments(source.replace("\r\n", "\n"))
    desc = EffectDesc(name=name)
    desc.textures = [
        EffectTexture("INPUT", "R8G8B8A8_UNORM", "INPUT_WIDTH", "INPUT_HEIGHT"),
        EffectTexture("OUTPUT", "R8G8B8A8_UNORM"),
    ]

    blocks = _split_blocks(source)
    _resolve_header(desc, blocks[0][1])

    counters: Dict[str, int] = {}
    pass_blocks: List[List[str]] = []
    for kind, lines in blocks[1:]:
        counters[kind] = counters.get(kind, 0) + 1
        idx = counters[kind]
        if kind == "PARAMETER":
            _resolve_parameter(desc, lines, idx)
        elif kind == "TEXTURE":
            _resolve_texture(desc, lines, idx)
        elif kind == "SAMPLER":
            _resolve_sampler(desc, lines, idx)
        elif kind == "COMMON":
            _resolve_common(desc, lines, idx)
        else:
            pass_blocks.append(lines)

    if not pass_blocks:
        raise MagpieFXError("Effect has no PASS block")

    names = [p.name for p in desc.params]
    names += [t.name for t in desc.textures]
    names += [s.name for s in desc.samplers]
    dupes = {n for n in names if names.count(n) > 1}
    if dupes:
        raise MagpieFXError(f"Duplicated identifiers: {', '.join(sorted(dupes))}")

    _resolve_passes(desc, pass_blocks)
    return desc
