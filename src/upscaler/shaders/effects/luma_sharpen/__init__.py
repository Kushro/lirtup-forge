import logging
import os
import struct
from typing import Optional

from ...shader import Shader
from ....vulkan import Sampler, Texture2D, SAMPLER_FILTER_LINEAR

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Constant buffer layout - must match `cbuffer Constants` in luma_sharpen.hlsl
#   uint2  inputSize;
#   uint2  outputSize;
#   float2 inputPt;
#   float2 outputPt;
#   float  sharpStrength;
#   float  sharpClamp;
#   int    pattern;
#   float  offsetBias;
# ---------------------------------------------------------------------------
CB_FORMAT = "IIIIffffffif"
CB_SIZE = struct.calcsize(CB_FORMAT)

_SHADER_DIR = os.path.dirname(__file__)
DEFAULT_SHADER_PATH = os.path.join(_SHADER_DIR, "luma_sharpen.spv")

# Parameter ranges, as declared by Magpie's //!PARAMETER blocks
# name: (default, min, max)
PARAMS = {
    "strength": (0.65, 0.0, 3.0),
    "clamp": (0.035, 0.0, 1.0),
    "pattern": (1, 0, 3),
    "offset_bias": (1.0, 0.0, 6.0),
}


def _clamp(name: str, value):
    _, lo, hi = PARAMS[name]
    return max(lo, min(value, hi))


class LumaSharpen(Shader):
    """
    LumaSharpen (CeeJay.dk), ported by hand from Magpie.

    Unsharp mask computed on luma only, with a clamp on the maximum
    sharpening to limit halos. Operates on sRGB-encoded values, at the
    resolution of its input (output size = input size).

    Requires separate source and target textures.

    Patterns:
        0 = fast (7-tap, 2 fetches)
        1 = normal (9-tap gaussian, default)
        2 = wider (17-tap)
        3 = pyramid
    """

    def __init__(self, shader_path: str = DEFAULT_SHADER_PATH) -> None:
        self.source_texture: Optional[Texture2D] = None
        super().__init__(shader_path)

    @staticmethod
    def _cb_size() -> int:
        return CB_SIZE

    def _create_persistent_resources(self) -> None:
        super()._create_persistent_resources()
        self._sampler = Sampler(
            filter_min=SAMPLER_FILTER_LINEAR,
            filter_mag=SAMPLER_FILTER_LINEAR,
        )

    def _get_bindings(self):
        return [self.source_texture], [self.target_texture], [self._sampler]

    def _rebuild_compute(self) -> None:
        """Only create the compute pipeline when both textures are set."""
        if self.target_texture is None or self.source_texture is None:
            return
        super()._rebuild_compute()

    def set_source_texture(self, tex: Texture2D) -> None:
        if tex is self.source_texture:
            return
        self.source_texture = tex
        self._rebuild_compute()

    def update_constants(
        self,
        strength: float = PARAMS["strength"][0],
        clamp: float = PARAMS["clamp"][0],
        pattern: int = PARAMS["pattern"][0],
        offset_bias: float = PARAMS["offset_bias"][0],
    ) -> None:
        """
        Pack and upload LumaSharpen parameters.

        Args:
            strength: Sharpening strength (0 - 3). Default 0.65.
            clamp: Maximum sharpening per pixel (0 - 1). Default 0.035.
            pattern: Sample pattern (0 - 3). Default 1.
            offset_bias: Radius of the sample pattern (0 - 6). Default 1.
        """
        src, dst = self.source_texture, self.target_texture
        data = struct.pack(
            CB_FORMAT,
            src.width,
            src.height,
            dst.width,
            dst.height,
            1.0 / src.width,
            1.0 / src.height,
            1.0 / dst.width,
            1.0 / dst.height,
            _clamp("strength", strength),
            _clamp("clamp", clamp),
            int(_clamp("pattern", pattern)),
            _clamp("offset_bias", offset_bias),
        )
        self._cb.upload(data)
