import logging
import os
import struct
from typing import Optional

from ...shader import Shader
from ....vulkan import Sampler, Texture2D

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Constant buffer layout - must match `cbuffer Constants` in crt_easymode.hlsl
#   uint2  inputSize, outputSize;  float2 inputPt, outputPt;
#   float  sharpnessH, sharpnessV, maskStrength;
#   int    maskDotWidth, maskDotHeight, maskStagger, maskSize;
#   float  scanlineStrength, scanlineBeamWidthMin, scanlineBeamWidthMax,
#          scanlineBrightMin, scanlineBrightMax;
#   int    scanlineCutoff;
#   float  gammaInput, gammaOutput, brightBoost;
#   int    dilation;
#   uint   _pad0, _pad1, _pad2;
# ---------------------------------------------------------------------------
CB_FORMAT = "IIIIffff" + "fff" + "iiii" + "fffff" + "i" + "fff" + "i" + "III"
CB_SIZE = struct.calcsize(CB_FORMAT)

_SHADER_DIR = os.path.dirname(__file__)
DEFAULT_SHADER_PATH = os.path.join(_SHADER_DIR, "crt_easymode.spv")

# Parameter ranges, as declared by Magpie's //!PARAMETER blocks, in
# constant buffer order. name: (default, min, max, is_int)
PARAMS = {
    "sharpness_h": (0.5, 0.0, 1.0, False),
    "sharpness_v": (1.0, 0.0, 1.0, False),
    "mask_strength": (0.3, 0.0, 1.0, False),
    "mask_dot_width": (1, 1, 100, True),
    "mask_dot_height": (1, 1, 100, True),
    "mask_stagger": (0, 0, 100, True),
    "mask_size": (1, 1, 100, True),
    "scanline_strength": (1.0, 0.0, 1.0, False),
    "scanline_beam_width_min": (1.5, 0.5, 5.0, False),
    "scanline_beam_width_max": (1.5, 0.5, 5.0, False),
    "scanline_bright_min": (0.35, 0.0, 1.0, False),
    "scanline_bright_max": (0.65, 0.0, 1.0, False),
    "scanline_cutoff": (400, 1, 1000, True),
    "gamma_input": (2.0, 0.1, 5.0, False),
    "gamma_output": (1.8, 0.1, 5.0, False),
    "bright_boost": (1.2, 1.0, 2.0, False),
    "dilation": (1, 0, 1, True),
}


class CRTEasymode(Shader):
    """
    CRT-Easymode (EasyMode), ported by hand from Magpie.

    Flat CRT look: sharpened Lanczos-like horizontal filter, scanlines and
    an RGB aperture-grille / shadow mask. Like in Magpie it is also the
    scaler: the target texture must have the final on-screen size of the
    content (the mask is evaluated per output pixel).

    Scanlines are skipped when the source height is >= `scanline_cutoff`
    (400 by default), so high resolution sources only get the mask.

    Requires separate source and target textures.
    """

    def __init__(self, shader_path: str = DEFAULT_SHADER_PATH) -> None:
        self.source_texture: Optional[Texture2D] = None
        super().__init__(shader_path)

    @staticmethod
    def _cb_size() -> int:
        return CB_SIZE

    def _create_persistent_resources(self) -> None:
        super()._create_persistent_resources()
        self._sampler = Sampler()  # POINT, CLAMP

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

    def update_constants(self, **params) -> None:
        """
        Pack and upload CRT-Easymode parameters.

        Keyword arguments are the keys of :data:`PARAMS`; missing ones take
        Magpie's defaults and every value is clamped to Magpie's range.
        """
        values = []
        for name, (default, lo, hi, is_int) in PARAMS.items():
            value = max(lo, min(params.get(name, default), hi))
            values.append(int(value) if is_int else float(value))

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
            *values,
            0,
            0,
            0,
        )
        self._cb.upload(data)
