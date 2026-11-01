"""
Generic loader for Magpie's MagpieFX effect format.

See :mod:`.parser` for the directive reader, :mod:`.codegen` for the HLSL
generator, :mod:`.compiler` for the runtime SPIR-V compilation and
:mod:`.effect` for the multi-pass executor. :mod:`.dds` reads the
``//!SOURCE`` lookup textures and :mod:`.lookup` uploads them.
"""

from .effect import SWAP_RB_EFFECT, MagpieEffect, compile_effect, find_effect, list_effects
from .parser import EffectDesc, MagpieFXError, parse_effect

__all__ = [
    "SWAP_RB_EFFECT",
    "EffectDesc",
    "MagpieEffect",
    "MagpieFXError",
    "compile_effect",
    "find_effect",
    "list_effects",
    "parse_effect",
]
