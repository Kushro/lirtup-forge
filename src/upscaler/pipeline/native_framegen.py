"""
Native frame generation: lsfg runs inside the upscaler, and a presentation
thread paces the frames it generates.

For each new frame of the game (the pipeline only presents new frames while
frame generation is on), on the pipeline thread:

1. Wait until the presentation thread is done with the previous cycle, so
   no texture it still shows is written again.
2. Copy the finished screen texture into one of two shared input textures
   (they alternate, lsfg reads the previous and the current one), and signal
   a shared semaphore once the copy is done.
3. lsfg generates ``multiplier - 1`` frames between the previous frame and
   this one, into shared output textures, signalling one semaphore each.
4. Hand the cycle over to the presentation thread and go back to capturing.

The presentation thread waits for each generated frame, presents it, and
then presents the real one (the input texture, which holds a copy of the
frame), spread evenly over the game's frame interval. The pipeline thread
never sleeps for the pacing, so it can capture and process the next frame
meanwhile. Both threads use the same Vulkan queue; the GIL keeps them apart,
since the vulkan module holds it whenever it uses the queue.

The textures and semaphores live on the upscaler's device and are shared with
lsfg's device by fd (``Texture2D.external``, ``Device.create_external_semaphore``).
Any error turns frame generation off for the session; presentation goes on.
"""

from __future__ import annotations

import logging
import statistics
import threading
import time
from collections import deque
from dataclasses import dataclass
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
# so a median would be biased; the long average is not. An interval of at
# least _LOST_FRAME_RATIO medians lost frames, and counts as as many frames
# as medians fit in it. (At 60 fps and 144 Hz the intervals alternate 13.8
# and 20.7 ms: a lower ratio would count the long ones as two frames.)
_INTERVAL_HISTORY = 30
_LOST_FRAME_RATIO = 1.75
# A chain that restarts (its frame came late) starts this share of a step
# later than needed, so the jitter of later frames (polling rounds them to
# the display refresh) does not restart it again: a little latency for
# steady pacing
_CHAIN_MARGIN = 0.5
_DRAIN_TIMEOUT = 1.0  # seconds; a presentation thread slower than this is stuck


@dataclass
class _Cycle:
    """What the presentation thread shows for one game frame."""

    generated: List[Texture2D]  # empty for the first frame of a context
    semaphores: List[int]  # one per output, waited even when not shown
    real: Texture2D
    swapchain: "SwapchainManager"


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

        # Presentation thread (started with the first cycle)
        self._thread: Optional[threading.Thread] = None
        self._wake = threading.Condition()
        self._next: Optional[_Cycle] = None
        self._stopping = False
        self._idle = threading.Event()
        self._idle.set()
        self._failed: Optional[str] = None  # set by the thread

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------
    @property
    def interval(self) -> float:
        """Smoothed game frame interval (seconds)."""
        return self._interval

    def present(self, texture: Texture2D, swapchain: SwapchainManager) -> None:
        """
        Generate the frames before *texture* and hand them, with *texture*,
        to the presentation thread. Returns without waiting for the pacing.
        """
        self._update_interval(time.perf_counter())
        self._drain()
        if self._failed is not None:
            self._disable(self._failed)
        if self.enabled:
            try:
                cycle = self._generate(texture, swapchain)
            except Exception as e:
                self._disable(str(e))
            else:
                self._hand_over(cycle)
                return
        self._last_real_present = None
        swapchain.present(texture, wait_for_fence=False)

    def close(self) -> None:
        """Stop the presentation thread and release lsfg and the shared objects."""
        thread = self._thread
        self._thread = None
        if thread is not None:
            with self._wake:
                self._stopping = True
                self._wake.notify()
            thread.join(_DRAIN_TIMEOUT * 2)
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

    def _disable(self, reason: str) -> None:
        logger.error("Native frame generation failed, turning it off: %s", reason)
        self.enabled = False
        self._failed = None
        self.close()

    # ------------------------------------------------------------------
    # Presentation thread
    # ------------------------------------------------------------------
    def _drain(self) -> None:
        """Wait until the presentation thread has shown the previous cycle."""
        if not self._idle.wait(_DRAIN_TIMEOUT):
            self._failed = "the presentation thread is stuck"

    def _hand_over(self, cycle: _Cycle) -> None:
        if self._thread is None:
            self._stopping = False
            self._thread = threading.Thread(
                target=self._run, name="lsfg-present", daemon=True
            )
            self._thread.start()
        with self._wake:
            self._idle.clear()
            self._next = cycle
            self._wake.notify()

    def _run(self) -> None:
        while True:
            with self._wake:
                while self._next is None and not self._stopping:
                    self._wake.wait()
                if self._stopping:
                    self._idle.set()
                    return
                cycle, self._next = self._next, None
            try:
                self._show(cycle)
            except Exception as e:
                self._failed = str(e)
            finally:
                self._idle.set()

    def _show(self, cycle: _Cycle) -> None:
        """Present the generated frames, then the real one, evenly spaced."""
        dev = self._device
        if not cycle.generated:
            # Nothing to show before the real frame, but lsfg signalled the
            # semaphores all the same: wait so they can be signalled again
            for semaphore in cycle.semaphores:
                self._wait(dev, semaphore)
            self._last_real_present = time.perf_counter()
            cycle.swapchain.present(cycle.real, wait_for_fence=False)
            return

        # Chain this cycle to the previous real frame, one step after it, so
        # the gaps do not follow the jitter of when a new frame is noticed
        # (polling) and generated. When the frame came too late for that,
        # start a new chain with some margin. (It never comes too early:
        # cycles are shown one at a time, so the previous real frame is
        # already on screen.)
        step = self._interval / (len(cycle.generated) + 1)
        self._wait(dev, cycle.semaphores[0])
        now = time.perf_counter()
        chained = (
            self._last_real_present + step
            if self._last_real_present is not None
            else None
        )
        if chained is not None and chained >= now:
            base = chained
        elif chained is not None:
            base = now + step * _CHAIN_MARGIN
        else:
            base = now
        for i, (frame, semaphore) in enumerate(zip(cycle.generated, cycle.semaphores)):
            if i > 0:
                self._wait(dev, semaphore)
            self._sleep_until(base + i * step)
            cycle.swapchain.present(frame, wait_for_fence=False)
        real_time = base + len(cycle.generated) * step
        self._sleep_until(real_time)
        cycle.swapchain.present(cycle.real, wait_for_fence=False)
        self._last_real_present = real_time

    @staticmethod
    def _wait(dev, semaphore: int) -> None:
        if not dev.wait_semaphore(semaphore, _SEMAPHORE_TIMEOUT_NS):
            raise RuntimeError("lsfg did not finish a generated frame in time")

    # ------------------------------------------------------------------
    # Generation
    # ------------------------------------------------------------------
    def _generate(self, texture: Texture2D, swapchain: SwapchainManager) -> _Cycle:
        """Start lsfg on *texture* (the presentation thread must be idle)."""
        if self._lsfg is None:
            self._initialize()
        size = (texture.width, texture.height)
        if size != self._size:
            self._create_context(*size)

        dev = self._device
        real = self._inputs[self._frame % 2]
        dev.copy_texture_signal(texture, real, self._in_semaphore)
        self._lsfg.present(
            self._context,
            dev.export_semaphore_fd(self._in_semaphore),
            [dev.export_semaphore_fd(s) for s in self._out_semaphores],
        )
        first = self._frame == 0
        self._frame += 1
        # The first frame has nothing before it to interpolate from
        return _Cycle(
            generated=[] if first else list(self._outputs),
            semaphores=list(self._out_semaphores),
            real=real,
            swapchain=swapchain,
        )

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
        frames = sum(
            max(2, round(v / median)) if v >= median * _LOST_FRAME_RATIO else 1
            for v in self._intervals
        )
        self._interval = sum(self._intervals) / frames

    @staticmethod
    def _sleep_until(target: float) -> None:
        delay = target - time.perf_counter()
        if delay > 0:
            time.sleep(delay)
