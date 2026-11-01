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
meanwhile. Both threads use the same Vulkan queue; the vulkan module keeps
them apart with a mutex of its own, and presents without the GIL.

With the automatic multiplier (:class:`AutoMultiplier`), the number of
generated frames follows the game's frame rate and the display refresh. lsfg
fixes that number when it initializes, so a change restarts it on a thread
of its own ("lsfg-restart") while the real frames go on being presented
alone; the next frame starts a new context.

Every present can be reported to :attr:`NativeFrameGen.on_present` with the
capture time of the game frame it was made from, to measure latency.

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
from typing import TYPE_CHECKING, Callable, Deque, List, Optional

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

# Automatic multiplier
AUTO_MIN_MULTIPLIER = 2
AUTO_MAX_MULTIPLIER = 5
# Output may exceed the display refresh by this share (measurement noise)
_AUTO_HEADROOM = 1.02
# Going up, the higher output must fit within this share of the refresh: with
# _AUTO_HEADROOM this leaves a band where neither change happens (hysteresis)
_AUTO_UP_FIT = 0.97
_AUTO_HOLD = 2.0  # seconds a new choice must last before switching to it
_AUTO_MIN_GAP = 5.0  # seconds between two switches (each restarts lsfg)
_AUTO_MIN_FPS = 5.0  # below this (a still image) the choice is kept
# Game frames measured before lsfg starts, so it starts with a sound choice
_AUTO_WARMUP_FRAMES = 15


def ideal_multiplier(game_fps: float, refresh_hz: float) -> int:
    """
    The largest multiplier (2 to 5) whose output fits the display refresh,
    e.g. at 144 Hz: x5 for 24 fps, x4 for 30 fps, x2 for 60 fps.
    """
    if game_fps <= 0 or refresh_hz <= 0:
        return AUTO_MIN_MULTIPLIER
    m = int(refresh_hz * _AUTO_HEADROOM / game_fps)
    return min(max(m, AUTO_MIN_MULTIPLIER), AUTO_MAX_MULTIPLIER)


class AutoMultiplier:
    """
    Chooses the frame generation multiplier from the game's frame rate and
    the display refresh, with hysteresis so it does not flip between two
    values:

    * It goes down as soon as the output exceeds the refresh (by more than
      _AUTO_HEADROOM), to :func:`ideal_multiplier`.
    * It goes up only while the higher output fits within _AUTO_UP_FIT of
      the refresh.
    * A new choice must hold for _AUTO_HOLD seconds, and switches are at
      least _AUTO_MIN_GAP seconds apart.
    """

    def __init__(self, refresh_hz: float, current: Optional[int] = None) -> None:
        self.refresh_hz = refresh_hz
        self.current = current
        self._candidate: Optional[int] = None
        self._since = 0.0
        self._last_switch: Optional[float] = None

    def target(self, game_fps: float) -> int:
        """The multiplier the hysteresis rules point to now (no timing)."""
        c = self.current
        if c is None:
            return ideal_multiplier(game_fps, self.refresh_hz)
        if game_fps * c > self.refresh_hz * _AUTO_HEADROOM:
            return min(c, ideal_multiplier(game_fps, self.refresh_hz))
        up = c
        while (
            up < AUTO_MAX_MULTIPLIER
            and game_fps * (up + 1) <= self.refresh_hz * _AUTO_UP_FIT
        ):
            up += 1
        return up

    def update(self, game_fps: float, now: float) -> int:
        """The multiplier to use at *now* (seconds) for *game_fps*."""
        if self.current is None:
            # First choice: nothing to restart, take it right away
            self.current = ideal_multiplier(game_fps, self.refresh_hz)
            self._last_switch = now
            return self.current
        if game_fps < _AUTO_MIN_FPS:
            self._candidate = None
            return self.current
        target = self.target(game_fps)
        if target == self.current:
            self._candidate = None
        elif target != self._candidate:
            self._candidate = target
            self._since = now
        elif now - self._since >= _AUTO_HOLD and (
            self._last_switch is None or now - self._last_switch >= _AUTO_MIN_GAP
        ):
            logger.info(
                "Automatic frame generation: x%d -> x%d (game %.1f fps, %.0f Hz)",
                self.current,
                target,
                game_fps,
                self.refresh_hz,
            )
            self.current = target
            self._candidate = None
            self._last_switch = now
        return self.current


@dataclass
class _Cycle:
    """What the presentation thread shows for one game frame."""

    generated: List[Texture2D]  # empty for the first frame of a context
    semaphores: List[int]  # one per output, waited even when not shown
    real: Texture2D
    swapchain: "SwapchainManager"
    capture_time: Optional[float] = None  # of the game frame (perf_counter)


class NativeFrameGen:
    """
    The lsfg engine for one presenter. Created lazily on the first frame;
    :meth:`close` releases everything.
    """

    def __init__(self, settings: NativeSettings, refresh_hz: float = 60.0) -> None:
        self.settings = settings
        self.enabled = True
        self.generated = settings.multiplier - 1
        self.fp16: Optional[bool] = None
        # Called after every present with (capture time, "real" or
        # "generated", multiplier), from either thread
        self.on_present: Optional[Callable[[float, str, int], None]] = None
        # Capture time of the frame the next present() shows (see mark_capture)
        self._capture_time: Optional[float] = None

        # Automatic multiplier: lsfg starts after a few frames, with its choice
        self._auto: Optional[AutoMultiplier] = (
            AutoMultiplier(refresh_hz) if settings.auto else None
        )
        self._frames_seen = 0

        self._lsfg = None  # the extension module, once initialized
        self._module = None  # the extension module, once imported (for close)
        # Restart of lsfg with another multiplier (see _begin_restart)
        self._restart: Optional[threading.Thread] = None
        self._restart_error: Optional[str] = None
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

    @property
    def multiplier(self) -> int:
        """Output frames per game frame lsfg runs with (or will start with)."""
        return self.generated + 1

    @property
    def auto(self) -> bool:
        """True when the multiplier follows the game's frame rate."""
        return self._auto is not None

    @property
    def restarting(self) -> bool:
        """True while lsfg restarts with another multiplier (real frames only)."""
        return self._restart is not None

    def set_refresh_rate(self, refresh_hz: float) -> None:
        """Display refresh the automatic multiplier aims at."""
        if self._auto is not None and refresh_hz > 0:
            self._auto.refresh_hz = refresh_hz

    def mark_capture(self, capture_time: float) -> None:
        """Capture time (``perf_counter``) of the frame the next present() shows."""
        self._capture_time = capture_time

    def present(self, texture: Texture2D, swapchain: SwapchainManager) -> None:
        """
        Generate the frames before *texture* and hand them, with *texture*,
        to the presentation thread. Returns without waiting for the pacing.
        """
        now = time.perf_counter()
        capture_time, self._capture_time = self._capture_time, None
        self._update_interval(now)
        self._frames_seen += 1
        self._drain()
        if self._failed is not None:
            self._disable(self._failed)
        if self.enabled and self._auto is not None:
            try:
                self._update_auto(now)
            except Exception as e:
                self._disable(str(e))
        if self.enabled and self._can_generate():
            try:
                cycle = self._generate(texture, swapchain, capture_time)
            except Exception as e:
                self._disable(str(e))
            else:
                self._hand_over(cycle)
                return
        self._last_real_present = None
        swapchain.present(texture, wait_for_fence=False)
        self._report(capture_time, "real")

    def close(self) -> None:
        """Stop the presentation thread and release lsfg and the shared objects."""
        thread = self._thread
        self._thread = None
        if thread is not None:
            with self._wake:
                self._stopping = True
                self._wake.notify()
            thread.join(_DRAIN_TIMEOUT * 2)
        restart = self._restart
        self._restart = None
        if restart is not None:
            restart.join()  # lsfg cannot be finalized while it initializes
        lsfg = self._lsfg
        self._lsfg = None
        module = self._module
        if module is not None:
            try:
                if lsfg is not None and self._context is not None:
                    lsfg.delete_context(self._context)
                module.finalize()  # no-op when it is not initialized
            except Exception as e:
                logger.warning("Closing native frame generation: %s", e)
        self._context = None
        self._release_shared()

    def _report(self, capture_time: Optional[float], kind: str) -> None:
        callback = self.on_present
        if callback is not None and capture_time is not None:
            try:
                callback(capture_time, kind, self.multiplier)
            except Exception as e:  # never let the stats break presentation
                logger.debug("Frame latency callback failed: %s", e)

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
            self._report(cycle.capture_time, "real")
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
            self._report(cycle.capture_time, "generated")
        real_time = base + len(cycle.generated) * step
        self._sleep_until(real_time)
        cycle.swapchain.present(cycle.real, wait_for_fence=False)
        self._report(cycle.capture_time, "real")
        self._last_real_present = real_time

    @staticmethod
    def _wait(dev, semaphore: int) -> None:
        if not dev.wait_semaphore(semaphore, _SEMAPHORE_TIMEOUT_NS):
            raise RuntimeError("lsfg did not finish a generated frame in time")

    # ------------------------------------------------------------------
    # Generation
    # ------------------------------------------------------------------
    def _can_generate(self) -> bool:
        """False while lsfg restarts, or waits for the first automatic choice."""
        if self._restart is not None:
            return False
        return self._auto is None or self._auto.current is not None

    def _update_auto(self, now: float) -> None:
        """
        Follow the automatic multiplier (the presentation thread is idle):
        finish a restart that is done, or start one when the choice changed.
        """
        restart = self._restart
        if restart is not None:
            if restart.is_alive():
                return
            self._restart = None
            error, self._restart_error = self._restart_error, None
            if error is not None:
                raise RuntimeError(f"restarting lsfg: {error}")
            self._lsfg = self._module
            return
        if self._auto.current is None and self._frames_seen <= _AUTO_WARMUP_FRAMES:
            return  # still measuring the game's frame rate
        chosen = self._auto.update(1.0 / self._interval, now)
        if self._lsfg is None:
            self.generated = chosen - 1  # lsfg starts with it (_initialize)
        elif chosen != self.multiplier:
            self._begin_restart(chosen)

    def _begin_restart(self, multiplier: int) -> None:
        """
        Restart lsfg with *multiplier* on a thread of its own. Until it is
        done, present() shows the real frames alone; then the next frame
        starts a new context (it shows no generated frames either).
        """
        lsfg = self._lsfg
        self._lsfg = None
        if self._context is not None:
            lsfg.delete_context(self._context)
            self._context = None
        self._release_shared()
        self._last_real_present = None
        self.generated = multiplier - 1
        self._restart_error = None
        self._restart = threading.Thread(
            target=self._restart_lsfg, name="lsfg-restart", daemon=True
        )
        self._restart.start()

    def _restart_lsfg(self) -> None:
        """Body of the restart thread: lsfg releases the GIL while it works."""
        try:
            self._module.finalize()
            self._initialize_module(self._module)
        except Exception as e:
            self._restart_error = str(e) or type(e).__name__

    def _generate(
        self,
        texture: Texture2D,
        swapchain: SwapchainManager,
        capture_time: Optional[float] = None,
    ) -> _Cycle:
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
            capture_time=capture_time,
        )

    def _initialize(self) -> None:
        from ..lsfg import lsfg

        self._module = lsfg
        self._initialize_module(lsfg)
        self._lsfg = lsfg

    def _initialize_module(self, lsfg) -> None:
        """Initialize lsfg for ``self.generated`` frames (any thread)."""
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
        logger.info(
            "Native frame generation x%d%s on %s (%s shaders, flow %.2f%s)",
            self.multiplier,
            " (auto)" if self._auto is not None else "",
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
