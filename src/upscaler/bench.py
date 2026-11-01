"""
Measure the Magpie effects and the native frame generation on this GPU.

Each case runs headless on a test image for a few warm-up frames and then
for a number of measured frames (bounded by a time budget); the median time
per frame is reported.

* **Native effects** (LumaSharpen, Anime4K_Upscale_S, CRT_Easymode) and
  **MagpieFX effects** are timed with Vulkan timestamps (GPU time from the
  first dispatch to the end of the last one, as
  ``scripts/bench-magpie-effects.py`` and ``scripts/magpiefx-bench.py`` do).
  Effects read the input size and render to the target size when they
  scale, like the presenter.
* **Frame generation** (:mod:`upscaler.lsfg`, built-in engine) runs on its
  own VkDevice, so there are no timestamps to read: a cycle is timed on the
  CPU, from the copy of a new real frame into the shared input until every
  generated frame is signalled. That is the work lsfg adds per real frame,
  at the target size, for x2 to x4, flow scale 1.0 / 0.5 and the quality /
  performance model.

The GUI runs this module in a separate process (``python -m upscaler.bench
--json``) and reads one JSON object per line from its output: a measurement
never blocks the GUI, cancelling is stopping the process, and lsfg's global
state is never shared with an upscaling session. :class:`Benchmark` is the
reusable part; GPU work goes through a backend so tests can replace it.

Results are kept per GPU in ``bench.json`` next to the configuration
(:func:`save_results`, :func:`results_for_gpu`).
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import signal
import statistics
import sys
import threading
import time
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from typing import Any, Callable, Dict, Iterable, List, Optional, Protocol, Tuple

from .config.yaml import default_config_path

logger = logging.getLogger(__name__)

# Case kinds
NATIVE = "native"
MAGPIEFX = "magpiefx"
FRAMEGEN = "framegen"

# Sections a run can include (the dialog's checkboxes)
SECTION_EFFECTS = "effects"  # native + MagpieFX
SECTION_FRAMEGEN = "framegen"

# Effects of the native engine, under their MagpieFX names
NATIVE_EFFECTS = ("LumaSharpen", "Anime4K_Upscale_S", "CRT_Easymode")

# Frame generation settings measured: multiplier, flow scale, performance
FRAMEGEN_MULTIPLIERS = (2, 3, 4)
FRAMEGEN_FLOW_SCALES = (1.0, 0.5)
FRAMEGEN_MODELS = (False, True)  # quality, performance

DEFAULT_INPUT = (1280, 720)
DEFAULT_TARGET = (2560, 1440)

RESULTS_FILE = "bench.json"
_RESULTS_VERSION = 1

_LSFG_TIMEOUT_NS = 2_000_000_000  # a generated frame later than this fails the case


class BenchCancelled(Exception):
    """Raised inside a run when :meth:`Benchmark.cancel` was called."""


# ----------------------------------------------------------------------
# Cases and results
# ----------------------------------------------------------------------
@dataclass(frozen=True)
class BenchCase:
    """One thing to measure."""

    kind: str  # NATIVE, MAGPIEFX or FRAMEGEN
    name: str  # effect name, or framegen_key() for FRAMEGEN
    multiplier: int = 0
    flow_scale: float = 1.0
    performance: bool = False

    @property
    def key(self) -> str:
        return f"{self.kind}:{self.name}"


@dataclass
class BenchResult:
    """Median ms per frame of one case, or the error that stopped it."""

    kind: str
    name: str
    ms: Optional[float] = None
    p95: Optional[float] = None
    frames: int = 0
    error: Optional[str] = None
    extra: Dict[str, Any] = field(default_factory=dict)

    @property
    def ok(self) -> bool:
        return self.error is None and self.ms is not None


def framegen_key(multiplier: int, flow_scale: float, performance: bool) -> str:
    """Name of a frame generation case, e.g. ``x2 flow 1.0 quality``."""
    model = "performance" if performance else "quality"
    return f"x{multiplier} flow {flow_scale:.1f} {model}"


def framegen_cases() -> List[BenchCase]:
    return [
        BenchCase(FRAMEGEN, framegen_key(m, f, p), m, f, p)
        for p in FRAMEGEN_MODELS
        for f in FRAMEGEN_FLOW_SCALES
        for m in FRAMEGEN_MULTIPLIERS
    ]


def build_cases(
    sections: Iterable[str], magpiefx_effects: Optional[Iterable[str]] = None
) -> List[BenchCase]:
    """
    The cases of a run, in the order they are measured.

    Args:
        sections: SECTION_EFFECTS and/or SECTION_FRAMEGEN.
        magpiefx_effects: MagpieFX effects to measure; all the bundled and
            user effects when None.
    """
    sections = set(sections)
    cases: List[BenchCase] = []
    if SECTION_EFFECTS in sections:
        cases += [BenchCase(NATIVE, n) for n in NATIVE_EFFECTS]
        if magpiefx_effects is None:
            from .magpiefx import list_effects

            magpiefx_effects = list_effects()
        cases += [BenchCase(MAGPIEFX, n) for n in magpiefx_effects]
    if SECTION_FRAMEGEN in sections:
        cases += framegen_cases()
    return cases


# ----------------------------------------------------------------------
# Measuring
# ----------------------------------------------------------------------
class Runner(Protocol):
    """One prepared case: each step() runs a frame and returns its ms."""

    def step(self) -> float: ...

    def close(self) -> None: ...


class Backend(Protocol):
    """Creates runners on a GPU (the real one: :class:`VulkanBackend`)."""

    gpu_name: str
    gpu_key: str

    def runner(self, case: BenchCase) -> Runner: ...

    def close(self) -> None: ...


@dataclass
class BenchSettings:
    input_size: Tuple[int, int] = DEFAULT_INPUT
    target_size: Tuple[int, int] = DEFAULT_TARGET
    warmup: int = 5
    min_frames: int = 10
    max_frames: int = 100
    budget_s: float = 1.5  # stop measuring a case after this (min_frames first)


def summarize(times: List[float]) -> Tuple[float, float]:
    """(median, p95) of a list of frame times."""
    ordered = sorted(times)
    p95 = ordered[max(0, int(len(ordered) * 0.95 + 0.5) - 1)]
    return statistics.median(ordered), p95


class Benchmark:
    """
    Runs cases one after the other. Thread-safe :meth:`cancel`; a case that
    fails is reported with its error and the run goes on.
    """

    def __init__(
        self,
        backend: Backend,
        settings: Optional[BenchSettings] = None,
        clock: Callable[[], float] = time.perf_counter,
    ) -> None:
        self.backend = backend
        self.settings = settings or BenchSettings()
        self._clock = clock
        self._cancel = threading.Event()

    def cancel(self) -> None:
        self._cancel.set()

    @property
    def cancelled(self) -> bool:
        return self._cancel.is_set()

    def _check(self) -> None:
        if self._cancel.is_set():
            raise BenchCancelled()

    def measure(self, case: BenchCase) -> BenchResult:
        """Measure one case (raises BenchCancelled when cancelled)."""
        s = self.settings
        self._check()
        try:
            runner = self.backend.runner(case)
        except BenchCancelled:
            raise
        except Exception as e:
            return BenchResult(case.kind, case.name, error=_error_text(e))
        try:
            for _ in range(s.warmup):
                self._check()
                runner.step()
            times: List[float] = []
            start = self._clock()
            while len(times) < s.max_frames:
                self._check()
                times.append(float(runner.step()))
                if len(times) >= s.min_frames and self._clock() - start >= s.budget_s:
                    break
        except BenchCancelled:
            raise
        except Exception as e:
            return BenchResult(case.kind, case.name, error=_error_text(e))
        finally:
            try:
                runner.close()
            except Exception as e:  # pragma: no cover - logged only
                logger.warning("Closing %s: %s", case.key, e)
        median, p95 = summarize(times)
        extra = dict(getattr(runner, "extra", None) or {})
        return BenchResult(case.kind, case.name, median, p95, len(times), extra=extra)

    def run(
        self,
        cases: List[BenchCase],
        on_result: Optional[Callable[[int, int, BenchResult], None]] = None,
        on_start: Optional[Callable[[int, int, BenchCase], None]] = None,
    ) -> List[BenchResult]:
        """
        Measure *cases* in order. Returns the results so far when cancelled
        (check :attr:`cancelled`).
        """
        results: List[BenchResult] = []
        total = len(cases)
        for i, case in enumerate(cases):
            if on_start is not None:
                on_start(i, total, case)
            try:
                result = self.measure(case)
            except BenchCancelled:
                break
            results.append(result)
            if on_result is not None:
                on_result(i + 1, total, result)
        return results


def _error_text(e: BaseException) -> str:
    text = str(e).strip() or type(e).__name__
    return text.splitlines()[0][:300]


# ----------------------------------------------------------------------
# Vulkan backend
# ----------------------------------------------------------------------
def gpu_key(name: str, vendor_id: int, device_id: int) -> str:
    """Key of a GPU in the results file."""
    return f"{vendor_id:04x}:{device_id:04x}:{name}"


def make_test_image(width: int, height: int, shift: int = 0):
    """
    A synthetic test frame (PIL RGBA): gradients, fine lines, flat areas and
    noise, so effects that branch on content do some of every kind of work.
    *shift* moves the pattern right, to make a second frame that differs.
    """
    from PIL import Image, ImageDraw

    base = Image.linear_gradient("L").resize((width, height))
    other = Image.radial_gradient("L").resize((width, height))
    noise = Image.effect_noise((width, height), 40)
    img = Image.merge("RGB", (base, other, noise)).convert("RGBA")
    draw = ImageDraw.Draw(img)
    step = max(8, width // 64)
    for x in range(shift % step, width, step):
        draw.line([(x, 0), (x, height // 3)], fill=(255, 255, 255, 255), width=1)
    for y in range(height // 3, 2 * height // 3, max(4, step // 2)):
        draw.line([(0, y), (width, y)], fill=(0, 0, 0, 255), width=1)
    box = (width // 2 + shift, 2 * height // 3, width // 2 + width // 4 + shift, height - 1)
    draw.rectangle(box, fill=(200, 60, 40, 255))
    return img


class _DispatchRunner:
    """A native effect: one dispatch sequence, timed with timestamps."""

    def __init__(self, sequence, keep_alive) -> None:
        self._sequence = sequence
        self._keep = keep_alive

    def step(self) -> float:
        seq = self._sequence
        _, ts = seq[0][0].dispatch_sequence(seq, timestamps=True)
        return (ts[4 + 2 * (len(seq) - 1)] - ts[3]) / 1e6

    def close(self) -> None:
        self._keep = None
        self._sequence = None


class _MagpieRunner:
    """A MagpieFX effect, as the presenter runs it (BGRA input, swap folded)."""

    def __init__(self, effect, src, target) -> None:
        self._fx = effect
        self._src = src
        self._target = target
        self._frame = 0

    def step(self) -> float:
        self._frame += 1
        self._fx.process(self._src, self._target, frame_count=self._frame, profile=True)
        return float(sum(self._fx.last_gpu_ms))

    def close(self) -> None:
        self._fx = None


class _LsfgRunner:
    """One lsfg configuration: initialize, a context at the target size, cycles."""

    def __init__(self, lsfg, device, case: BenchCase, frames, dll: str) -> None:
        from .vulkan import R8G8B8A8_UNORM, Texture2D

        self._lsfg = lsfg
        self._dev = device
        self._frames = frames  # two source textures that alternate
        self._generated = case.multiplier - 1
        self._context = None
        self._in_sem = 0
        self._out_sems: List[int] = []
        self._frame = 0
        if not device.supports_external_fd:
            raise RuntimeError(
                "the GPU driver cannot share memory by fd (VK_KHR_external_memory_fd)"
            )
        self.fp16 = lsfg.initialize(
            dll,
            device.vendor_id,
            device.device_id,
            self._generated,
            flow_scale=case.flow_scale,
            performance=case.performance,
            fp16=device.supports_fp16,
        )
        self.extra = {"fp16": bool(self.fp16)}
        try:
            w, h = frames[0].width, frames[0].height
            self._inputs = [Texture2D.external(w, h, R8G8B8A8_UNORM) for _ in range(2)]
            self._outputs = [
                Texture2D.external(w, h, R8G8B8A8_UNORM) for _ in range(self._generated)
            ]
            self._in_sem = device.create_external_semaphore()
            self._out_sems = [
                device.create_external_semaphore() for _ in range(self._generated)
            ]
            self._context = lsfg.create_context(
                self._inputs[0].export_memory_fd(),
                self._inputs[1].export_memory_fd(),
                [t.export_memory_fd() for t in self._outputs],
                w,
                h,
                37,  # VK_FORMAT_R8G8B8A8_UNORM
            )
        except Exception:
            self.close()
            raise

    def step(self) -> float:
        dev = self._dev
        t0 = time.perf_counter()
        dev.copy_texture_signal(
            self._frames[self._frame % 2], self._inputs[self._frame % 2], self._in_sem
        )
        self._lsfg.present(
            self._context,
            dev.export_semaphore_fd(self._in_sem),
            [dev.export_semaphore_fd(s) for s in self._out_sems],
        )
        for semaphore in self._out_sems:
            if not dev.wait_semaphore(semaphore, _LSFG_TIMEOUT_NS):
                raise RuntimeError("lsfg did not finish a generated frame in time")
        self._frame += 1
        return (time.perf_counter() - t0) * 1000

    def close(self) -> None:
        lsfg, self._lsfg = self._lsfg, None
        if lsfg is not None:
            try:
                if self._context is not None:
                    lsfg.delete_context(self._context)
            finally:
                self._context = None
                lsfg.finalize()
        for semaphore in [self._in_sem, *self._out_sems]:
            if semaphore:
                try:
                    self._dev.destroy_semaphore(semaphore)
                except Exception:
                    pass
        self._in_sem = 0
        self._out_sems = []
        self._inputs = []
        self._outputs = []


class VulkanBackend:
    """Runs the cases on the current Vulkan device of this process."""

    def __init__(
        self,
        input_size: Tuple[int, int] = DEFAULT_INPUT,
        target_size: Tuple[int, int] = DEFAULT_TARGET,
        gpu: Optional[str] = None,
        dll: Optional[str] = None,
    ) -> None:
        from . import vulkan

        vulkan.select_device(gpu)
        self._vk = vulkan
        self.device = vulkan.get_current_device()
        self.gpu_name = self.device.name
        self.gpu_key = gpu_key(self.gpu_name, self.device.vendor_id, self.device.device_id)
        self.input_size = tuple(input_size)
        self.target_size = tuple(target_size)
        self.dll = dll
        self._src = None  # input-size frame, BGRA bytes like a capture
        self._lsfg_frames = None  # two target-size frames

    def _source(self):
        if self._src is None:
            w, h = self.input_size
            tex = self._vk.Texture2D(w, h, format=self._vk.R8G8B8A8_UNORM)
            tex.upload_subresources([(make_test_image(w, h).tobytes("raw", "BGRA"), 0, 0, w, h)])
            self._src = tex
        return self._src

    def runner(self, case: BenchCase) -> Runner:
        if case.kind == NATIVE:
            return self._native(case.name)
        if case.kind == MAGPIEFX:
            from .magpiefx import MagpieEffect

            fx = MagpieEffect.load(case.name, swap_input=True, swap_output=True)
            return _MagpieRunner(fx, self._source(), self.target_size)
        if case.kind == FRAMEGEN:
            return self._framegen(case)
        raise ValueError(f"unknown case kind {case.kind!r}")

    def _native(self, name: str) -> Runner:
        from .shaders import Anime4KUpscaleS, CRTEasymode, LumaSharpen

        Texture2D = self._vk.Texture2D
        src = self._source()
        if name == "Anime4K_Upscale_S":
            a4k = Anime4KUpscaleS()
            a4k.set_source_texture(src)
            return _DispatchRunner(a4k.sequence(), a4k)
        if name == "LumaSharpen":
            shader, dst = LumaSharpen(), Texture2D(src.width, src.height)
        elif name == "CRT_Easymode":
            shader, dst = CRTEasymode(), Texture2D(*self.target_size)
        else:
            raise ValueError(f"{name} is not a native effect")
        shader.set_source_texture(src)
        shader.set_target_texture(dst)
        shader.update_constants()
        seq = [(shader.compute, (dst.width + 15) // 16, (dst.height + 15) // 16, 1, b"")]
        return _DispatchRunner(seq, (shader, dst))

    def _framegen(self, case: BenchCase) -> Runner:
        if not self.dll:
            raise RuntimeError("Lossless.dll not found")
        try:
            from .lsfg import lsfg
        except ImportError as e:
            raise RuntimeError(f"the built-in frame generation is not built ({e})")
        if self._lsfg_frames is None:
            w, h = self.target_size
            frames = []
            for shift in (0, max(4, w // 160)):
                tex = self._vk.Texture2D(w, h, format=self._vk.R8G8B8A8_UNORM)
                tex.upload_subresources([(make_test_image(w, h, shift).tobytes(), 0, 0, w, h)])
                frames.append(tex)
            self._lsfg_frames = frames
        return _LsfgRunner(lsfg, self.device, case, self._lsfg_frames, self.dll)

    def close(self) -> None:
        self._src = None
        self._lsfg_frames = None
        try:
            self.device.wait_idle()
        except Exception:
            pass


# ----------------------------------------------------------------------
# Stored results
# ----------------------------------------------------------------------
def results_path() -> str:
    return default_config_path(RESULTS_FILE)


def load_results(path: Optional[str] = None) -> Dict[str, Any]:
    """The whole results file (``{"version", "gpus": {key: entry}, "selectors"}``)."""
    path = path or results_path()
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError):
        return {"version": _RESULTS_VERSION, "gpus": {}, "selectors": {}}
    if not isinstance(data, dict) or not isinstance(data.get("gpus"), dict):
        return {"version": _RESULTS_VERSION, "gpus": {}, "selectors": {}}
    data.setdefault("selectors", {})
    return data


def save_results(
    gpu: Dict[str, str],
    results: Iterable[BenchResult],
    input_size: Tuple[int, int],
    target_size: Tuple[int, int],
    selector: Optional[str] = None,
    path: Optional[str] = None,
    now: Optional[datetime] = None,
) -> Dict[str, Any]:
    """
    Merge *results* into the entry of *gpu* (``{"key", "name"}``) and write
    the file. A case measured again replaces the old value; failed cases are
    kept with their error so the GUI can tell them from unmeasured ones.
    *selector* is the ``gpu`` setting the run used ("" for automatic), so
    the GUI can find the entry without opening the GPU.
    """
    path = path or results_path()
    data = load_results(path)
    stamp = (now or datetime.now(timezone.utc)).isoformat(timespec="seconds")
    entry = data["gpus"].setdefault(gpu["key"], {})
    entry["name"] = gpu["name"]
    for r in results:
        sizes = {"target": list(target_size)}
        if r.kind != FRAMEGEN:
            sizes["input"] = list(input_size)
        if entry.setdefault("sizes", {}).get(r.kind) != sizes:
            # Times at other sizes are not comparable: start this kind over
            entry[r.kind] = {}
            entry["sizes"][r.kind] = sizes
        entry.setdefault(r.kind, {})[r.name] = {
            "ms": None if r.ms is None else round(r.ms, 4),
            "p95": None if r.p95 is None else round(r.p95, 4),
            "frames": r.frames,
            "error": r.error,
            **({"extra": r.extra} if r.extra else {}),
        }
        entry.setdefault("updated", {})[r.kind] = stamp
    data["selectors"][selector or ""] = gpu["key"]
    data["version"] = _RESULTS_VERSION
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=1, sort_keys=True)
    os.replace(tmp, path)
    return data


def results_for_gpu(
    selector: Optional[str] = None, data: Optional[Dict[str, Any]] = None
) -> Optional[Dict[str, Any]]:
    """
    The stored entry for the GPU the ``gpu`` setting *selector* picks, or
    None. Uses the GPU last measured with that setting; otherwise, with a
    single measured GPU, that one; otherwise a name that contains the
    selector.
    """
    if data is None:
        data = load_results()
    gpus = data.get("gpus") or {}
    if not gpus:
        return None
    key = (data.get("selectors") or {}).get(selector or "")
    if key in gpus:
        return gpus[key]
    if selector:
        lower = selector.lower()
        for entry in gpus.values():
            if lower in str(entry.get("name", "")).lower():
                return entry
    if len(gpus) == 1:
        return next(iter(gpus.values()))
    return None


def stored_ms(entry: Optional[Dict[str, Any]], kind: str, name: str) -> Optional[float]:
    """Median ms of one case in a stored entry, or None."""
    if not entry:
        return None
    value = (entry.get(kind) or {}).get(name) or {}
    ms = value.get("ms")
    return float(ms) if isinstance(ms, (int, float)) else None


def stored_framegen_ms(
    entry: Optional[Dict[str, Any]], multiplier: int, flow_scale: float, performance: bool
) -> Optional[float]:
    """Median ms per real frame of one frame generation setting, or None."""
    return stored_ms(entry, FRAMEGEN, framegen_key(multiplier, flow_scale, performance))


# ----------------------------------------------------------------------
# Command line (what the GUI runs)
# ----------------------------------------------------------------------
def _parse_size(text: str) -> Tuple[int, int]:
    w, _, h = text.lower().partition("x")
    size = int(w), int(h)
    if size[0] < 16 or size[1] < 16 or size[0] > 8192 or size[1] > 8192:
        raise argparse.ArgumentTypeError(f"size out of range: {text}")
    return size


def _emit(event: Dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(event) + "\n")
    sys.stdout.flush()


def main(argv: Optional[List[str]] = None, backend_factory=None) -> int:
    ap = argparse.ArgumentParser(
        prog="python -m upscaler.bench",
        description="Measure the Magpie effects and frame generation on this GPU.",
    )
    ap.add_argument(
        "--section",
        action="append",
        choices=[SECTION_EFFECTS, SECTION_FRAMEGEN],
        help="What to measure (repeatable; default: both)",
    )
    ap.add_argument("--effect", action="append", help="Only these MagpieFX effects")
    ap.add_argument("--input-size", type=_parse_size, default=DEFAULT_INPUT)
    ap.add_argument("--target-size", type=_parse_size, default=DEFAULT_TARGET)
    ap.add_argument("--gpu", default=None, help="GPU index or name (like --gpu)")
    ap.add_argument("--dll", default=None, help="Lossless.dll for frame generation")
    ap.add_argument("--max-frames", type=int, default=100)
    ap.add_argument("--budget", type=float, default=1.5, help="Seconds per case")
    ap.add_argument("--json", action="store_true", help="One JSON event per line")
    ap.add_argument("--save", action="store_true", help="Store the results for the GUI")
    args = ap.parse_args(argv)

    logging.basicConfig(level=logging.WARNING, stream=sys.stderr)
    sections = args.section or [SECTION_EFFECTS, SECTION_FRAMEGEN]

    dll = args.dll
    if SECTION_FRAMEGEN in sections:
        from .framegen import find_dll

        found = find_dll(dll, native=True)
        dll = str(found) if found else None

    factory = backend_factory or VulkanBackend
    try:
        backend = factory(args.input_size, args.target_size, args.gpu, dll)
    except Exception as e:
        if args.json:
            _emit({"event": "error", "error": _error_text(e)})
        else:
            print(f"error: {e}", file=sys.stderr)
        return 1

    settings = BenchSettings(
        input_size=args.input_size,
        target_size=args.target_size,
        max_frames=max(args.max_frames, 1),
        min_frames=min(10, max(args.max_frames, 1)),
        budget_s=args.budget,
    )
    bench = Benchmark(backend, settings)

    # The GUI stops a run with SIGTERM: finish the current frame and clean up
    def on_term(signum, frame):
        bench.cancel()

    old_handlers = {
        sig: signal.signal(sig, on_term) for sig in (signal.SIGTERM, signal.SIGINT)
    }

    try:
        cases = build_cases(sections, args.effect)
    except Exception as e:
        cases = []
        logger.error("Cannot list the effects: %s", e)
    gpu = {"key": backend.gpu_key, "name": backend.gpu_name}

    if args.json:
        _emit(
            {
                "event": "start",
                "gpu": gpu,
                "total": len(cases),
                "input_size": list(args.input_size),
                "target_size": list(args.target_size),
            }
        )

    def on_start(index: int, total: int, case: BenchCase) -> None:
        if args.json:
            _emit({"event": "case", "index": index, "total": total, "kind": case.kind, "name": case.name})

    def on_result(done: int, total: int, r: BenchResult) -> None:
        if args.json:
            _emit({"event": "result", "done": done, "total": total, **asdict(r)})
        elif r.ok:
            print(f"{r.kind:9} {r.name:40} {r.ms:8.3f} ms  (p95 {r.p95:.3f}, {r.frames} frames)")
        else:
            print(f"{r.kind:9} {r.name:40}   failed: {r.error}")

    try:
        results = bench.run(cases, on_result=on_result, on_start=on_start)
    finally:
        backend.close()
        for sig, handler in old_handlers.items():
            signal.signal(sig, handler)

    if args.save and results:
        try:
            save_results(gpu, results, args.input_size, args.target_size, args.gpu)
        except OSError as e:
            logger.error("Cannot save the results: %s", e)
    if args.json:
        _emit({"event": "cancelled" if bench.cancelled else "done", "count": len(results)})
    return 0


if __name__ == "__main__":
    sys.exit(main())
