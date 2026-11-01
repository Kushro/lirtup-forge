"""
Stats HUD: an mpv-style panel with frame rates, GPU times and the active
processing chain, drawn in a corner of the overlay.

The pipeline feeds it events (captures, presents, GPU timings) and asks it
once per frame whether it needs a redraw. The panel text is rebuilt at most
every ``UPDATE_INTERVAL`` seconds, rendered with PIL on the pipeline thread
and blended onto the 8-bit screen texture before the OSD.
"""

import logging
import struct
import threading
import time
from collections import deque
from typing import Callable, Deque, Dict, List, Optional, Tuple, Union

from PIL import Image, ImageDraw, ImageFont

from ..shaders import OverlayBlender
from ..vulkan import Buffer, Compute, Texture2D

logger = logging.getLogger(__name__)

# A panel row: (label, value), or (label, value, warning) to draw it in the
# warning color. An empty label continues the previous row.
StatsRow = Union[Tuple[str, str], Tuple[str, str, bool]]

UPDATE_INTERVAL = 0.5  # seconds between panel refreshes
_WINDOW = 1.0  # seconds of history behind each rate
_MARGIN = 12  # pixels from the top-left corner of the screen

_FONTS = ("DejaVuSansMono.ttf", "LiberationMono-Regular.ttf", "NotoSansMono-Regular.ttf")
_LABEL_COLOR = (150, 200, 255, 255)
_VALUE_COLOR = (255, 255, 255, 255)
_WARNING_COLOR = (255, 110, 90, 255)
_BG_COLOR = (0, 0, 0, 170)
# Below this game frame rate (a still image) the GPU budget is not checked
_MIN_BUDGET_FPS = 5.0


def gpu_budget_warning(
    effects: List[Tuple[str, Optional[float]]], game_fps: float
) -> Optional[str]:
    """
    A warning when the effects take longer on the GPU than the game takes
    to draw a frame (they cannot keep up), naming the heaviest one.
    *effects* holds (name, GPU ms or None) pairs.
    """
    timed = [(name, ms) for name, ms in effects if ms is not None]
    if game_fps < _MIN_BUDGET_FPS or not timed:
        return None
    budget = 1000.0 / game_fps
    total = sum(ms for _, ms in timed)
    if total <= budget:
        return None
    heaviest = max(timed, key=lambda e: e[1])[0]
    return (
        f"{total:.1f} ms > {budget:.1f} ms per game frame: "
        f"try a lighter effect than {heaviest}"
    )


class _Rate:
    """Events per second over the last ``_WINDOW`` seconds."""

    def __init__(self) -> None:
        self._times: Deque[float] = deque()

    def add(self, now: float) -> None:
        self._times.append(now)

    def per_second(self, now: float) -> float:
        while self._times and now - self._times[0] > _WINDOW:
            self._times.popleft()
        return len(self._times) / _WINDOW


class StatsHUD:
    """
    Collects pipeline statistics and draws them as an overlay panel.

    Thread safety: :meth:`toggle` may be called from any thread; every other
    method runs on the pipeline thread.
    """

    def __init__(self, screen_height: int, visible: bool = False) -> None:
        self._visible = visible
        self._toggled = threading.Event()  # visibility changed, redraw once

        self._captures = _Rate()
        self._new_frames = _Rate()
        self._presents = _Rate()
        self._render_ms: Deque[Tuple[float, float]] = deque()
        self._gpu_ms: Dict[str, float] = {}

        self._font_size = 0
        self._font: Optional[ImageFont.ImageFont] = None
        self._set_font(screen_height)

        self._last_update = 0.0
        self._texture: Optional[Texture2D] = None
        self._blender = OverlayBlender()  # own constant buffer, apart from the OSD
        self._compute: Optional[Compute] = None
        # Textures the cached pipeline is bound to (kept alive by these refs)
        self._bound: Tuple[Optional[Texture2D], Optional[Texture2D]] = (None, None)

    # ------------------------------------------------------------------
    # Visibility
    # ------------------------------------------------------------------
    @property
    def visible(self) -> bool:
        return self._visible

    def toggle(self) -> None:
        """Show or hide the panel (thread-safe)."""
        self._visible = not self._visible
        self._toggled.set()
        logger.debug("Stats panel %s", "shown" if self._visible else "hidden")

    # ------------------------------------------------------------------
    # Events from the pipeline
    # ------------------------------------------------------------------
    def record_capture(self, new_frame: bool) -> None:
        now = time.perf_counter()
        self._captures.add(now)
        if new_frame:
            self._new_frames.add(now)

    def record_present(self, render_ms: Optional[float] = None) -> None:
        """Count a present; *render_ms* is the CPU time of a full render."""
        now = time.perf_counter()
        self._presents.add(now)
        if render_ms is not None:
            self._render_ms.append((now, render_ms))

    def set_gpu_ms(self, label: str, ms: float) -> None:
        """GPU time of a pass or effect this frame (smoothed)."""
        prev = self._gpu_ms.get(label)
        self._gpu_ms[label] = ms if prev is None else prev * 0.8 + ms * 0.2

    def clear_gpu_ms(self) -> None:
        self._gpu_ms.clear()

    def gpu_ms(self, label: str) -> Optional[float]:
        return self._gpu_ms.get(label)

    # ------------------------------------------------------------------
    # Rates for the panel
    # ------------------------------------------------------------------
    def rates(self) -> Tuple[float, float, float, Optional[float]]:
        """(captures/s, new frames/s, presents/s, avg full-render CPU ms)."""
        now = time.perf_counter()
        while self._render_ms and now - self._render_ms[0][0] > _WINDOW:
            self._render_ms.popleft()
        render = (
            sum(ms for _, ms in self._render_ms) / len(self._render_ms)
            if self._render_ms
            else None
        )
        return (
            self._captures.per_second(now),
            self._new_frames.per_second(now),
            self._presents.per_second(now),
            render,
        )

    # ------------------------------------------------------------------
    # Panel
    # ------------------------------------------------------------------
    def resize(self, screen_height: int) -> None:
        self._set_font(screen_height)
        self._last_update = 0.0

    def update(self, build_rows: Callable[[], List[StatsRow]]) -> bool:
        """
        Rebuild the panel when it is due. Returns True when the screen needs
        a full redraw (new panel content, or the panel was just hidden).
        """
        toggled = self._toggled.is_set()
        self._toggled.clear()
        if not self._visible:
            if toggled:
                self._texture = None
            return toggled

        now = time.perf_counter()
        if not toggled and now - self._last_update < UPDATE_INTERVAL:
            return False
        self._last_update = now
        try:
            self._texture = self._upload(self._render(build_rows()))
        except Exception as e:  # never let the HUD break presentation
            logger.error("Stats HUD failed, hiding it: %s", e)
            self._visible = False
            self._texture = None
        return True

    def blend(self, screen_tex: Texture2D) -> None:
        """Draw the panel onto *screen_tex* (no-op while hidden)."""
        tex = self._texture
        if not self._visible or tex is None:
            return
        # Clip to the screen: the blend shader does not check bounds
        w = min(tex.width, screen_tex.width - _MARGIN)
        h = min(tex.height, screen_tex.height - _MARGIN)
        if w <= 0 or h <= 0:
            return
        self._blender.cb.upload(struct.pack("iiii", _MARGIN, _MARGIN, w, h))
        if (
            self._compute is None
            or self._bound[0] is not screen_tex
            or self._bound[1] is not tex
        ):
            self._compute = Compute(
                self._blender.shader,
                srv=[screen_tex, tex],
                uav=[screen_tex],
                cbv=[self._blender.cb],
                samplers=[self._blender.sampler],
                push_size=0,
            )
            self._bound = (screen_tex, tex)
        self._compute.dispatch((w + 15) // 16, (h + 15) // 16, 1)

    def close(self) -> None:
        self._compute = None
        self._bound = (None, None)
        self._texture = None

    # ------------------------------------------------------------------
    # Rendering
    # ------------------------------------------------------------------
    def _set_font(self, screen_height: int) -> None:
        size = max(12, int(screen_height * 0.016))
        if size == self._font_size and self._font is not None:
            return
        self._font_size = size
        for name in _FONTS:
            try:
                self._font = ImageFont.truetype(name, size)
                return
            except OSError:
                continue
        logger.warning("No monospace TrueType font found; using the default font")
        self._font = ImageFont.load_default()

    def _render(self, rows: List[StatsRow]) -> Image.Image:
        font = self._font
        pad = max(6, self._font_size // 2)
        gap = self._font_size  # between the label and value columns
        line_h = int(self._font_size * 1.35)

        probe = ImageDraw.Draw(Image.new("RGBA", (1, 1)))
        label_w = max((probe.textlength(r[0], font=font) for r in rows), default=0)
        value_w = max((probe.textlength(r[1], font=font) for r in rows), default=0)
        width = int(pad * 2 + label_w + gap + value_w)
        height = int(pad * 2 + line_h * len(rows))

        img = Image.new("RGBA", (max(width, 1), max(height, 1)), (0, 0, 0, 0))
        draw = ImageDraw.Draw(img)
        draw.rounded_rectangle((0, 0, width - 1, height - 1), radius=pad, fill=_BG_COLOR)
        x_value = pad + label_w + gap
        for i, row in enumerate(rows):
            label, value = row[0], row[1]
            warning = len(row) > 2 and row[2]
            y = pad + i * line_h
            if label:
                fill = _WARNING_COLOR if warning else _LABEL_COLOR
                draw.text((pad, y), label, font=font, fill=fill)
            fill = _WARNING_COLOR if warning else _VALUE_COLOR
            draw.text((x_value, y), value, font=font, fill=fill)
        return img

    def _upload(self, img: Image.Image) -> Texture2D:
        # The screen texture holds BGRA bytes: swap red and blue
        r, g, b, a = img.split()
        data = Image.merge("RGBA", (b, g, r, a)).tobytes()
        tex = self._texture
        if tex is None or tex.width != img.width or tex.height != img.height:
            tex = Texture2D(img.width, img.height)
        buffer = Buffer(len(data))
        buffer.upload(data)
        buffer.copy_to(tex)
        return tex
