"""
Native frame generation: lsfg runs inside the upscaler and the presenter
paces the frames it generates.

For each new frame of the game (the pipeline only presents new frames while
frame generation is on):

1. The finished screen texture is copied into one of two shared input
   textures (they alternate, lsfg reads the previous and the current one),
   and a shared semaphore is signalled once the copy is done.
2. lsfg generates ``multiplier - 1`` frames between the previous frame and
   this one, into shared output textures, signalling one semaphore each.
3. The generated frames are presented, then the real one, spread evenly over
   the game's frame interval.

The textures and semaphores live on the upscaler's device and are shared with
lsfg's device by fd (``Texture2D.external``, ``Device.create_external_semaphore``).
Any error turns frame generation off for the session; presentation goes on.
"""

from __future__ import annotations

import logging
import statistics
import time
from collections import deque
from typing import TYPE_CHECKING, Deque, List, Optional

from ..framegen import NativeSettings
from ..vulkan import R8G8B8A8_UNORM, Texture2D, get_current_device

if TYPE_CHECKING:
    from .swapchain import SwapchainManager

logger = logging.getLogger(__name__)

_VK_FORMAT_R8G8B8A8_UNORM = 37  # VkFormat of the shared textures
_SEMAPHORE_TIMEOUT_NS = 500_000_000  # a generated frame later than this is lost

# Bounds of the game frame interval used for pacing (seconds)
_MIN_INTERVAL = 1 / 240
_MAX_INTERVAL = 1 / 15
# The interval is the time spanned by the last measured intervals over the
# game frames they hold. Polling rounds each interval to the display refresh,
# so a median would be biased; the long average is not. An interval that
# lost frames counts as as many frames as medians fit in it.
_INTERVAL_HISTORY = 30
# A cycle waits at most this share of a step to stay chained to the previous
# one; more would block the pipeline into the next game frame and lose it
_MAX_CHAIN_WAIT = 0.5


class NativeFrameGen:
    """
    The lsfg engine for one presenter. Created lazily on the first frame;
    :meth:`close` releases everything.
    """

    def __init__(self, settings: NativeSettings) -> None:
        self.settings = settings
        self.enabled = True
        self.generated = settings.multiplier - 1
        self.fp16: Optional[bool] = None

        self._lsfg = None  # the extension module, once initialized
        self._device = get_current_device()
        self._context: Optional[int] = None
        self._size = (0, 0)
        self._inputs: List[Texture2D] = []
        self._outputs: List[Texture2D] = []
        self._in_semaphore = 0
        self._out_semaphores: List[int] = []
        self._frame = 0  # frames pushed into the current context

        self._last_frame_time: Optional[float] = None
        self._last_real_present: Optional[float] = None
        self._interval = 1 / 60
        self._intervals: Deque[float] = deque(maxlen=_INTERVAL_HISTORY)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------
    @property
    def interval(self) -> float:
        """Smoothed game frame interval (seconds)."""
        return self._interval

    def present(self, texture: Texture2D, swapchain: SwapchainManager) -> None:
        """Present the generated frames before *texture*, then *texture*."""
        start = time.perf_counter()
        self._update_interval(start)
        generated: List[Texture2D] = []
        if self.enabled:
            try:
                generated = self._generate(texture)
            except Exception as e:
                logger.error("Native frame generation failed, turning it off: %s", e)
                self.enabled = False
                self.close()
                generated = []

        if not generated:
            self._last_real_present = None
            swapchain.present(texture, wait_for_fence=False)
            return

        # Chain this cycle to the previous real frame, one step after it, so
        # the gaps do not follow the jitter of when a new frame is noticed
        # (polling) and generated. Start now instead when the frame came too
        # late for that, or when keeping the chain would wait too long.
        step = self._interval / (len(generated) + 1)
        now = time.perf_counter()
        base = now
        if self._last_real_present is not None:
            chained = self._last_real_present + step
            if 0 <= chained - now <= step * _MAX_CHAIN_WAIT:
                base = chained
        for i, frame in enumerate(generated):
            self._sleep_until(base + i * step)
            swapchain.present(frame, wait_for_fence=False)
        real_time = base + len(generated) * step
        self._sleep_until(real_time)
        swapchain.present(texture, wait_for_fence=False)
        self._last_real_present = real_time

    def close(self) -> None:
        """Release lsfg's device and the shared textures and semaphores."""
        lsfg = self._lsfg
        self._lsfg = None
        if lsfg is not None:
            try:
                if self._context is not None:
                    lsfg.delete_context(self._context)
                lsfg.finalize()
            except Exception as e:
                logger.warning("Closing native frame generation: %s", e)
        self._context = None
        self._release_shared()

    # ------------------------------------------------------------------
    # Generation
    # ------------------------------------------------------------------
    def _generate(self, texture: Texture2D) -> List[Texture2D]:
        if self._lsfg is None:
            self._initialize()
        size = (texture.width, texture.height)
        if size != self._size:
            self._create_context(*size)

        dev = self._device
        slot = self._frame % 2
        dev.copy_texture_signal(texture, self._inputs[slot], self._in_semaphore)
        self._lsfg.present(
            self._context,
            dev.export_semaphore_fd(self._in_semaphore),
            [dev.export_semaphore_fd(s) for s in self._out_semaphores],
        )
        first = self._frame == 0
        self._frame += 1

        # Wait for every output even when unused: the semaphores must be
        # unsignalled before lsfg signals them again
        for semaphore in self._out_semaphores:
            if not dev.wait_semaphore(semaphore, _SEMAPHORE_TIMEOUT_NS):
                raise RuntimeError("lsfg did not finish a generated frame in time")
        # The first frame has nothing before it to interpolate from
        return [] if first else list(self._outputs)

    def _initialize(self) -> None:
        from ..lsfg import lsfg

        dev = self._device
        if not dev.supports_external_fd:
            raise RuntimeError(
                "the GPU driver cannot share memory by fd "
                "(VK_KHR_external_memory_fd)"
            )
        s = self.settings
        self.fp16 = lsfg.initialize(
            str(s.dll),
            dev.vendor_id,
            dev.device_id,
            self.generated,
            flow_scale=s.flow_scale,
            performance=s.performance,
            fp16=dev.supports_fp16,
        )
        self._lsfg = lsfg
        logger.info(
            "Native frame generation x%d on %s (%s shaders, flow %.2f%s)",
            s.multiplier,
            dev.name,
            "FP16" if self.fp16 else "FP32",
            s.flow_scale,
            ", performance" if s.performance else "",
        )

    def _create_context(self, width: int, height: int) -> None:
        lsfg = self._lsfg
        if self._context is not None:
            lsfg.delete_context(self._context)
            self._context = None
        self._release_shared()

        dev = self._device
        self._inputs = [Texture2D.external(width, height, R8G8B8A8_UNORM) for _ in range(2)]
        self._outputs = [
            Texture2D.external(width, height, R8G8B8A8_UNORM)
            for _ in range(self.generated)
        ]
        self._in_semaphore = dev.create_external_semaphore()
        self._out_semaphores = [
            dev.create_external_semaphore() for _ in range(self.generated)
        ]
        self._context = lsfg.create_context(
            self._inputs[0].export_memory_fd(),
            self._inputs[1].export_memory_fd(),
            [t.export_memory_fd() for t in self._outputs],
            width,
            height,
            _VK_FORMAT_R8G8B8A8_UNORM,
        )
        self._size = (width, height)
        self._frame = 0
        logger.debug("Native frame generation context %dx%d", width, height)

    def _release_shared(self) -> None:
        dev = self._device
        for semaphore in [self._in_semaphore, *self._out_semaphores]:
            if semaphore:
                try:
                    dev.destroy_semaphore(semaphore)
                except Exception:
                    pass
        self._in_semaphore = 0
        self._out_semaphores = []
        self._inputs = []
        self._outputs = []
        self._size = (0, 0)

    # ------------------------------------------------------------------
    # Pacing
    # ------------------------------------------------------------------
    def _update_interval(self, now: float) -> None:
        last = self._last_frame_time
        self._last_frame_time = now
        if last is None:
            return
        self._intervals.append(min(max(now - last, _MIN_INTERVAL), _MAX_INTERVAL))
        median = statistics.median(self._intervals)
        frames = sum(max(1, round(v / median)) for v in self._intervals)
        self._interval = sum(self._intervals) / frames

    @staticmethod
    def _sleep_until(target: float) -> None:
        delay = target - time.perf_counter()
        if delay > 0:
            time.sleep(delay)
