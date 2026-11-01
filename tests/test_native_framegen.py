"""Pacing of the native frame generation engine, without a GPU."""

import threading
import time

import pytest

pytest.importorskip("upscaler.vulkan.vulkan")

from upscaler.framegen import NativeSettings  # noqa: E402
from upscaler.pipeline import native_framegen  # noqa: E402
from upscaler.pipeline.native_framegen import NativeFrameGen, _Cycle  # noqa: E402


class FakeDevice:
    def __init__(self):
        self.waited = []

    def wait_semaphore(self, semaphore, timeout_ns):
        self.waited.append(semaphore)
        return True


class FakeSwapchain:
    def __init__(self, clock=time):
        self.clock = clock
        self.presented = []

    def present(self, texture, wait_for_fence=True):
        self.presented.append((texture, self.clock.perf_counter()))


class FakeClock:
    def __init__(self):
        self.now = 100.0

    def perf_counter(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


def _engine(monkeypatch, multiplier=3):
    monkeypatch.setattr(native_framegen, "get_current_device", FakeDevice)
    return NativeFrameGen(NativeSettings("Lossless.dll", multiplier, 1.0, False))


@pytest.fixture
def clocked(monkeypatch):
    """An engine on a fake clock, driven through _show (no thread)."""
    clock = FakeClock()
    monkeypatch.setattr(native_framegen.time, "perf_counter", clock.perf_counter)
    monkeypatch.setattr(native_framegen.time, "sleep", clock.sleep)
    fg = _engine(monkeypatch)
    fg._interval = 0.03
    return fg, clock, FakeSwapchain(clock)


def _times(swapchain, origin=100.0):
    return [round(t - origin, 4) for _, t in swapchain.presented]


def test_generated_frames_are_spread_over_the_interval(clocked):
    fg, clock, swapchain = clocked
    fg._show(_Cycle(["gen1", "gen2"], [1, 2], "real", swapchain))
    assert [t for t, _ in swapchain.presented] == ["gen1", "gen2", "real"]
    assert _times(swapchain) == [0.0, 0.01, 0.02]
    assert fg._device.waited == [1, 2]


def test_cycles_are_chained_to_the_previous_real_frame(clocked):
    """A new frame noticed early still keeps the generated one a step after the real one."""
    fg, clock, swapchain = clocked
    fg._show(_Cycle(["gen"], [1], "real1", swapchain))  # gen at 0, real at 15 ms
    clock.now = 100.0 + 0.020  # next frame noticed 10 ms before its slot
    fg._show(_Cycle(["gen"], [1], "real2", swapchain))
    assert _times(swapchain) == [0.0, 0.015, 0.030, 0.045]


def test_a_late_frame_starts_a_new_chain(clocked):
    fg, clock, swapchain = clocked
    fg._show(_Cycle(["gen"], [1], "real1", swapchain))  # real at 15 ms
    clock.now = 100.0 + 0.050  # late: the chained slot (30 ms) has passed
    fg._show(_Cycle(["gen"], [1], "real2", swapchain))
    # The new chain starts half a step later than now, as margin
    assert _times(swapchain) == [0.0, 0.015, 0.0575, 0.0725]


def test_first_frame_waits_for_every_output_but_shows_only_the_real_one(clocked):
    fg, clock, swapchain = clocked
    fg._show(_Cycle([], [1, 2], "real", swapchain))
    assert [t for t, _ in swapchain.presented] == ["real"]
    assert fg._device.waited == [1, 2]


def test_the_thread_presents_while_the_pipeline_goes_on(monkeypatch):
    fg = _engine(monkeypatch, multiplier=2)
    fg._interval = 0.04
    swapchain = FakeSwapchain()
    monkeypatch.setattr(
        fg, "_generate", lambda tex, sc: _Cycle([f"gen-{tex}"], [1], tex, sc)
    )
    monkeypatch.setattr(fg, "_update_interval", lambda now: None)
    start = time.perf_counter()
    fg.present("a", swapchain)
    returned = time.perf_counter() - start
    fg.present("b", swapchain)  # waits until "a" is on screen
    fg._drain()
    fg.close()
    assert returned < 0.01  # did not wait for the 20 ms step
    assert [t for t, _ in swapchain.presented] == ["gen-a", "a", "gen-b", "b"]
    assert threading.active_count() == 1


def test_a_failure_in_the_thread_turns_generation_off(monkeypatch):
    fg = _engine(monkeypatch, multiplier=2)
    swapchain = FakeSwapchain()
    monkeypatch.setattr(fg, "_generate", lambda tex, sc: _Cycle(["gen"], [1], tex, sc))
    monkeypatch.setattr(fg._device, "wait_semaphore", lambda s, t: False)  # lost frame
    fg.present("a", swapchain)
    fg.present("b", swapchain)
    assert not fg.enabled
    assert [t for t, _ in swapchain.presented] == ["b"]


def test_a_failure_while_generating_still_presents(monkeypatch):
    fg = _engine(monkeypatch)

    def broken(tex, sc):
        raise RuntimeError("device lost")

    monkeypatch.setattr(fg, "_generate", broken)
    swapchain = FakeSwapchain()
    fg.present("real", swapchain)
    assert not fg.enabled
    assert [t for t, _ in swapchain.presented] == ["real"]


def test_interval_follows_the_game(monkeypatch):
    fg = _engine(monkeypatch)
    t = 0.0
    for _ in range(60):
        fg._update_interval(t)
        t += 1 / 30
    assert fg.interval == pytest.approx(1 / 30)


def test_a_lost_frame_does_not_stretch_the_interval(monkeypatch):
    fg = _engine(monkeypatch)
    t = 0.0
    for i in range(30):
        fg._update_interval(t)
        t += 2 / 30 if i % 5 == 4 else 1 / 30
    assert fg.interval == pytest.approx(1 / 30)


def test_polling_jitter_is_not_taken_for_lost_frames(monkeypatch):
    """60 fps polled at 144 Hz: intervals alternate between 2 and 3 refreshes."""
    fg = _engine(monkeypatch)
    t = 0.0
    for i in range(40):
        fg._update_interval(t)
        t += (2 if i % 2 else 3) / 144
    assert fg.interval == pytest.approx(2.5 / 144)


def test_interval_is_bounded(monkeypatch):
    fg = _engine(monkeypatch)
    fg._update_interval(0.0)
    fg._update_interval(10.0)  # a long pause in the game
    assert fg.interval <= native_framegen._MAX_INTERVAL
