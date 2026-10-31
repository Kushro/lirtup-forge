"""
Runtime for MagpieFX effects on top of the Vulkan extension.

A :class:`MagpieEffect` parses an effect, compiles every pass to SPIR-V,
allocates the intermediate textures declared by the effect and records all
passes into a single ``dispatch_sequence`` submission.

Usage::

    fx = MagpieEffect.load("LumaSharpen", {"sharpStrength": 1.0})
    out = fx.process(src_texture, target_size=(1920, 1080))
"""

from __future__ import annotations

import logging
import os
import struct
from typing import Dict, List, Mapping, Optional, Sequence, Tuple

from ..vulkan import (
    Buffer,
    Compute,
    SAMPLER_ADDRESS_MODE_CLAMP,
    SAMPLER_ADDRESS_MODE_WRAP,
    SAMPLER_FILTER_LINEAR,
    SAMPLER_FILTER_POINT,
    Sampler,
    Texture2D,
)
from .codegen import constant_buffer_layout, generate_pass_source
from .compiler import compile_hlsl
from .expr import evaluate_size
from .formats import FORMATS
from .parser import EffectDesc, MagpieFXError, parse_effect

logger = logging.getLogger(__name__)

# Effects bundled with the package (copied from Magpie, GPLv3)
BUILTIN_EFFECTS_DIR = os.path.join(os.path.dirname(__file__), "effects")

# Internal helper effect that swaps red and blue (BGRA <-> RGBA)
SWAP_RB_EFFECT = os.path.join(os.path.dirname(__file__), "internal", "SwapRB.hlsl")


def find_effect(name_or_path: str) -> str:
    """
    Resolve an effect given as a path or as a bare name.

    Bare names (``LumaSharpen``, ``CRT/CRT_Easymode``) are looked up in
    ``$XDG_CONFIG_HOME/linux-rt-upscaler/effects`` first and then in the
    bundled effects directory, recursively, with or without ``.hlsl``.
    """
    path = os.path.expanduser(name_or_path)
    if os.path.isfile(path):
        return os.path.abspath(path)

    stem = name_or_path[:-5] if name_or_path.endswith(".hlsl") else name_or_path
    xdg = os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config"))
    roots = [os.path.join(xdg, "linux-rt-upscaler", "effects"), BUILTIN_EFFECTS_DIR]
    for root in roots:
        direct = os.path.join(root, stem + ".hlsl")
        if os.path.isfile(direct):
            return direct
        if not os.path.isdir(root):
            continue
        for dirpath, _, files in os.walk(root):
            for f in files:
                if f == os.path.basename(stem) + ".hlsl":
                    return os.path.join(dirpath, f)
    raise MagpieFXError(f"MagpieFX effect '{name_or_path}' not found")


def list_effects() -> List[str]:
    """Names of the bundled and user effects."""
    xdg = os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config"))
    names = set()
    for root in (BUILTIN_EFFECTS_DIR, os.path.join(xdg, "linux-rt-upscaler", "effects")):
        for dirpath, _, files in os.walk(root):
            names.update(f[:-5] for f in files if f.endswith(".hlsl"))
    return sorted(names)


class MagpieEffect:
    """
    One MagpieFX effect instance with its own textures and pipelines.

    Attributes:
        desc: Parsed effect description.
        output: The OUTPUT texture of the last :meth:`process` call.
        last_gpu_ms: Per-pass GPU times (ms) of the last profiled run.
    """

    def __init__(
        self,
        desc: EffectDesc,
        spirv: Sequence[bytes],
        params: Optional[Mapping[str, float]] = None,
    ) -> None:
        self.desc = desc
        self._spirv = list(spirv)
        self._params: Dict[str, float] = {}
        self.set_params(params or {})

        self._samplers = [
            Sampler(
                address_mode_u=self._address(s.address),
                address_mode_v=self._address(s.address),
                address_mode_w=self._address(s.address),
                filter_min=self._filter(s.filter),
                filter_mag=self._filter(s.filter),
            )
            for s in desc.samplers
        ]
        n_words = len(constant_buffer_layout(desc)) * 2  # upper bound
        self._cb = Buffer(max(16, (n_words * 4 + 15) // 16 * 16))

        self._input: Optional[Texture2D] = None
        self._target_size: Tuple[int, int] = (0, 0)
        self._textures: List[Optional[Texture2D]] = []
        self._computes: List[Compute] = []
        self._dispatches: List[Tuple[int, int]] = []
        self.output: Optional[Texture2D] = None
        self.last_gpu_ms: List[float] = []

    # ------------------------------------------------------------------
    #  Construction
    # ------------------------------------------------------------------
    @classmethod
    def load(
        cls,
        name_or_path: str,
        params: Optional[Mapping[str, float]] = None,
        use_cache: bool = True,
    ) -> "MagpieEffect":
        path = find_effect(name_or_path)
        desc, spirv = compile_effect(path, use_cache=use_cache)
        for w in desc.warnings:
            logger.warning("MagpieFX %s: %s", desc.name, w)
        return cls(desc, spirv, params)

    @staticmethod
    def _address(mode: str) -> int:
        return SAMPLER_ADDRESS_MODE_WRAP if mode == "WRAP" else SAMPLER_ADDRESS_MODE_CLAMP

    @staticmethod
    def _filter(mode: str) -> int:
        return SAMPLER_FILTER_LINEAR if mode == "LINEAR" else SAMPLER_FILTER_POINT

    # ------------------------------------------------------------------
    #  Parameters
    # ------------------------------------------------------------------
    def set_params(self, params: Mapping[str, float]) -> None:
        """Validate and store parameter overrides (unknown names raise)."""
        for name, value in params.items():
            p = self.desc.param(name)
            if p is None:
                known = ", ".join(q.name for q in self.desc.params) or "none"
                raise MagpieFXError(
                    f"{self.desc.name}: unknown parameter '{name}' (known: {known})"
                )
            value = float(value)
            if not p.min <= value <= p.max:
                raise MagpieFXError(
                    f"{self.desc.name}: {name}={value} outside [{p.min}, {p.max}]"
                )
            self._params[name] = value

    def param_value(self, name: str) -> float:
        p = self.desc.param(name)
        return self._params.get(name, p.default if p else 0.0)

    # ------------------------------------------------------------------
    #  Resources
    # ------------------------------------------------------------------
    def output_size(self, in_w: int, in_h: int, target: Tuple[int, int]) -> Tuple[int, int]:
        """
        OUTPUT size: the effect's WIDTH/HEIGHT expression if present,
        otherwise *target* (the size the content will be shown at, like
        Magpie's scaling-mode based default).
        """
        out = self.desc.output
        if not out.width_expr:
            return target
        consts = {"INPUT_WIDTH": in_w, "INPUT_HEIGHT": in_h}
        return evaluate_size(out.width_expr, consts), evaluate_size(out.height_expr, consts)

    def _rebuild(self, src: Texture2D, target: Tuple[int, int]) -> None:
        desc = self.desc
        in_w, in_h = src.width, src.height
        out_w, out_h = self.output_size(in_w, in_h, target)
        consts = {
            "INPUT_WIDTH": in_w,
            "INPUT_HEIGHT": in_h,
            "OUTPUT_WIDTH": out_w,
            "OUTPUT_HEIGHT": out_h,
        }

        textures: List[Optional[Texture2D]] = [src, None]
        old_out = self.output
        if old_out is not None and (old_out.width, old_out.height) == (out_w, out_h):
            textures[1] = old_out
        else:
            textures[1] = Texture2D(out_w, out_h, format=FORMATS["R8G8B8A8_UNORM"].vk_format)

        for i, tex in enumerate(desc.textures[2:], start=2):
            if tex.source:
                raise MagpieFXError(
                    f"{desc.name}: texture '{tex.name}' uses //!SOURCE "
                    f"('{tex.source}'), which is not supported yet"
                )
            if tex.width_expr:
                w = evaluate_size(tex.width_expr, consts)
                h = evaluate_size(tex.height_expr, consts)
            else:
                w, h = out_w, out_h
            prev = self._textures[i] if i < len(self._textures) else None
            if prev is not None and (prev.width, prev.height) == (w, h):
                textures.append(prev)
            else:
                textures.append(Texture2D(w, h, format=FORMATS[tex.format].vk_format))

        computes: List[Compute] = []
        dispatches: List[Tuple[int, int]] = []
        for p, spirv in zip(desc.passes, self._spirv):
            computes.append(
                Compute(
                    spirv,
                    cbv=[self._cb],
                    srv=[textures[i] for i in p.inputs] or None,
                    uav=[textures[i] for i in p.outputs],
                    samplers=self._samplers or None,
                )
            )
            first_out = textures[p.outputs[0]]
            bw, bh = p.block_size
            dispatches.append(
                ((first_out.width + bw - 1) // bw, (first_out.height + bh - 1) // bh)
            )

        self._textures = textures
        self._computes = computes
        self._dispatches = dispatches
        self._input = src
        self._target_size = target
        self.output = textures[1]
        logger.debug(
            "MagpieFX %s: %dx%d -> %dx%d, %d passes",
            desc.name, in_w, in_h, out_w, out_h, len(computes),
        )

    def _pack_constants(self, frame_count: int) -> bytes:
        in_w, in_h = self._input.width, self._input.height
        out_w, out_h = self.output.width, self.output.height
        words: List[bytes] = [
            struct.pack("<IIII", in_w, in_h, out_w, out_h),
            struct.pack("<ffffff", 1 / in_w, 1 / in_h, 1 / out_w, 1 / out_h,
                        out_w / in_w, out_h / in_h),
        ]
        for p in self.desc.passes[:-1]:
            if p.ps_style:
                t = self._textures[p.outputs[0]]
                words.append(struct.pack("<IIff", t.width, t.height, 1 / t.width, 1 / t.height))
        for param in self.desc.params:
            value = self.param_value(param.name)
            if param.type == "int":
                words.append(struct.pack("<i", int(round(value))))
            else:
                words.append(struct.pack("<f", value))
        if self.desc.use_dynamic:
            words.append(struct.pack("<I", frame_count & 0xFFFFFFFF))
        data = b"".join(words)
        return data + b"\0" * (-len(data) % 16)

    # ------------------------------------------------------------------
    #  Execution
    # ------------------------------------------------------------------
    def process(
        self,
        src: Texture2D,
        target_size: Tuple[int, int],
        frame_count: int = 0,
        profile: bool = False,
    ) -> Texture2D:
        """
        Run all passes on *src* and return the OUTPUT texture.

        Args:
            src: Input texture (any float-readable format).
            target_size: Size used for OUTPUT when the effect does not fix it.
            frame_count: Value of ``__frameCount`` for ``USE _DYNAMIC`` effects.
            profile: Collect GPU timestamps into :attr:`last_gpu_ms`.
        """
        if (
            src is not self._input
            or (not self.desc.output.width_expr and tuple(target_size) != self._target_size)
            or not self._computes
        ):
            self._rebuild(src, tuple(target_size))

        self._cb.upload(self._pack_constants(frame_count))
        sequence = [
            (c, x, y, 1, b"") for c, (x, y) in zip(self._computes, self._dispatches)
        ]
        result = self._computes[0].dispatch_sequence(sequence, timestamps=profile)
        if profile and result:
            ts = result[1]
            n = len(sequence)
            # Layout: [start, copy_before, copy_after, (before_i, after_i)...]
            self.last_gpu_ms = [
                (ts[4 + 2 * i] - ts[3 + 2 * i]) / 1e6 for i in range(n)
            ]
        return self.output


def compile_effect(path: str, use_cache: bool = True) -> Tuple[EffectDesc, List[bytes]]:
    """Parse *path* and compile all of its passes, returning ``(desc, spirv)``."""
    with open(path, "r", encoding="utf-8-sig") as f:
        source = f.read()
    name = os.path.splitext(os.path.basename(path))[0]
    desc = parse_effect(source, name)
    if any(t.source for t in desc.textures):
        names = ", ".join(t.source for t in desc.textures if t.source)
        raise MagpieFXError(f"{name}: //!SOURCE textures are not supported ({names})")
    include_dir = os.path.dirname(path)
    spirv = [
        compile_hlsl(
            generate_pass_source(desc, p.index),
            f"{name}_Pass{p.index}",
            include_dir=include_dir,
            use_cache=use_cache,
        )
        for p in desc.passes
    ]
    return desc, spirv
