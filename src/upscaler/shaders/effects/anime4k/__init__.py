import logging
import os
import struct
from typing import List, Optional, Tuple

from ...shader import Shader
from ....vulkan import (
    Buffer,
    Compute,
    Sampler,
    Texture2D,
    R16G16B16A16_FLOAT,
    SAMPLER_FILTER_LINEAR,
)

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Constant buffer layout - must match `cbuffer Constants` in
# anime4k_common.hlsli (shared by the four passes)
#   uint2  inputSize;   // effect input
#   uint2  outputSize;  // 2x output
#   float2 inputPt;
#   float2 outputPt;
# ---------------------------------------------------------------------------
CB_FORMAT = "IIIIffff"
CB_SIZE = struct.calcsize(CB_FORMAT)

_SHADER_DIR = os.path.dirname(__file__)
PASS_SHADER_PATHS = [
    os.path.join(_SHADER_DIR, f"anime4k_upscale_s_pass{i}.spv") for i in range(1, 5)
]

# Every pass covers a 16x16 block per 64-thread workgroup
_BLOCK_SIZE = 16


class _Anime4KPass(Shader):
    """One compute pass of Anime4K; resources are owned by the effect."""

    def __init__(self, shader_path: str, cb: Buffer, samplers: List[Sampler]):
        self._shared_cb = cb
        self._samplers = samplers
        self._srvs: List[Texture2D] = []
        super().__init__(shader_path)

    @staticmethod
    def _cb_size() -> int:
        return CB_SIZE

    def _create_persistent_resources(self) -> None:
        self._cb = self._shared_cb

    def _get_bindings(self):
        return self._srvs, [self.target_texture], self._samplers

    def bind(self, srvs: List[Texture2D], uav: Texture2D) -> None:
        self._srvs = srvs
        self.target_texture = uav
        self._rebuild_compute()

    def update_constants(self, **kwargs) -> None:
        raise NotImplementedError("constants are uploaded by Anime4KUpscaleS")


class Anime4KUpscaleS:
    """
    Anime4K_Upscale_S (CNN x2, small), ported by hand from Magpie.

    Four compute passes:
        1. Conv-4x3x3x3   INPUT -> tex1
        2. Conv-4x3x3x8   tex1  -> tex2
        3. Conv-4x3x3x8   tex2  -> tex1
        4. Conv-4x3x3x8 + depth-to-space, added to a bilinear upscale of
           INPUT -> OUTPUT (2x)

    `tex1`/`tex2` are RGBA16F feature maps at input resolution; the output
    is RGBA8 at twice the input resolution. Operates on sRGB-encoded values
    and has no parameters.

    Usage:
        a4k.set_source_texture(src)   # (re)allocates on size change
        a4k.dispatch()
        a4k.output_texture            # 2x result
    """

    scale = 2

    def __init__(self) -> None:
        self._cb = Buffer(CB_SIZE)
        self._point = Sampler()  # POINT, CLAMP
        self._linear = Sampler(
            filter_min=SAMPLER_FILTER_LINEAR,
            filter_mag=SAMPLER_FILTER_LINEAR,
        )
        self._passes = [
            _Anime4KPass(PASS_SHADER_PATHS[0], self._cb, [self._point]),
            _Anime4KPass(PASS_SHADER_PATHS[1], self._cb, [self._point]),
            _Anime4KPass(PASS_SHADER_PATHS[2], self._cb, [self._point]),
            _Anime4KPass(PASS_SHADER_PATHS[3], self._cb, [self._point, self._linear]),
        ]
        self.source_texture: Optional[Texture2D] = None
        self.output_texture: Optional[Texture2D] = None
        self._tex1: Optional[Texture2D] = None
        self._tex2: Optional[Texture2D] = None

    def set_source_texture(self, tex: Texture2D) -> None:
        """Bind the input; reallocates intermediates if its size changed."""
        if tex is self.source_texture:
            return
        w, h = tex.width, tex.height
        if self._tex1 is None or (self._tex1.width, self._tex1.height) != (w, h):
            self._tex1 = Texture2D(w, h, format=R16G16B16A16_FLOAT)
            self._tex2 = Texture2D(w, h, format=R16G16B16A16_FLOAT)
            self.output_texture = Texture2D(w * self.scale, h * self.scale)
            logger.debug("Anime4K_Upscale_S textures allocated for %dx%d", w, h)
        self.source_texture = tex

        p1, p2, p3, p4 = self._passes
        p1.bind([tex], self._tex1)
        p2.bind([self._tex1], self._tex2)
        p3.bind([self._tex2], self._tex1)
        p4.bind([tex, self._tex1], self.output_texture)

        ow, oh = self.output_texture.width, self.output_texture.height
        self._cb.upload(
            struct.pack(CB_FORMAT, w, h, ow, oh, 1.0 / w, 1.0 / h, 1.0 / ow, 1.0 / oh)
        )

    def sequence(self) -> List[Tuple[Compute, int, int, int, bytes]]:
        """Dispatch list for `Compute.dispatch_sequence` (one per pass)."""
        if self.source_texture is None:
            raise RuntimeError("Anime4KUpscaleS: call set_source_texture() first")
        w, h = self.source_texture.width, self.source_texture.height
        ow, oh = self.output_texture.width, self.output_texture.height
        gx, gy = -(-w // _BLOCK_SIZE), -(-h // _BLOCK_SIZE)
        ogx, ogy = -(-ow // _BLOCK_SIZE), -(-oh // _BLOCK_SIZE)
        p1, p2, p3, p4 = self._passes
        return [
            (p1.compute, gx, gy, 1, b""),
            (p2.compute, gx, gy, 1, b""),
            (p3.compute, gx, gy, 1, b""),
            (p4.compute, ogx, ogy, 1, b""),
        ]

    def dispatch(self) -> None:
        """Run the four passes in a single submission."""
        seq = self.sequence()
        seq[0][0].dispatch_sequence(seq)
