"""Pacing of the native frame generation engine, without a GPU."""

import pytest

pytest.importorskip("upscaler.vulkan.vulkan")

from upscaler.framegen import NativeSettings  # noqa: E402
from upscaler.pipeline import native_framegen  # noqa: E402
from upscaler.pipeline.native_framegen import NativeFrameGen  # noqa: E402


class FakeSwapchain:
    def __init__(self, clock):
        self.clock = clock
        self.presented = []

    def present(self, texture, wait_for_fence=True):
        self.presented.append((texture, self.clock.now))


class FakeClock:
    def __init__(self):
        self.now = 100.0

    def perf_counter(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


@pytest.fixture
def engine(monkeypatch):
    clock = FakeClock()
    monkeypatch.setattr(native_framegen.time, "perf_counter", clock.perf_counter)
    monkeypatch.setattr(native_framegen.time, "sleep", clock.sleep)
    monkeypatch.setattr(native_framegen, "get_current_device", lambda: None)
    fg = NativeFrameGen(NativeSettings("Lossless.dll", 3, 1.0, False))
    return fg, clock


def test_generated_frames_are_spread_over_the_interval(engine, monkeypatch):
    fg, clock = engine
    fg._interval = 0.03
    monkeypatch.setattr(fg, "_generate", lambda tex: ["gen1", "gen2"])
    swapchain = FakeSwapchain(clock)
    fg.present("real", swapchain)
    names = [t for t, _ in swapchain.presented]
    times = [round(t - 100.0, 4) for _, t in swapchain.presented]
    assert names == ["gen1", "gen2", "real"]
    assert times == [0.0, 0.01, 0.02]


def test_cycles_are_chained_to_the_previous_real_frame(engine, monkeypatch):
    """A new frame noticed early still keeps the generated one a step after the real one."""
    fg, clock = engine
    fg._interval = 0.03
    monkeypatch.setattr(fg, "_generate", lambda tex: ["gen"])
    monkeypatch.setattr(fg, "_update_interval", lambda now: None)
    swapchain = FakeSwapchain(clock)
    fg.present("real1", swapchain)  # gen at 0, real at 15 ms
    clock.now = 100.0 + 0.025  # next frame noticed 5 ms before its slot
    fg.present("real2", swapchain)
    times = [round(t - 100.0, 4) for _, t in swapchain.presented]
    assert times == [0.0, 0.015, 0.030, 0.045]


def test_chain_is_dropped_when_it_would_wait_too_long(engine, monkeypatch):
    fg, clock = engine
    fg._interval = 0.03
    monkeypatch.setattr(fg, "_generate", lambda tex: ["gen"])
    monkeypatch.setattr(fg, "_update_interval", lambda now: None)
    swapchain = FakeSwapchain(clock)
    fg.present("real1", swapchain)  # real at 15 ms
    clock.now = 100.0 + 0.016  # the chained slot (30 ms) is 14 ms away
    fg.present("real2", swapchain)
    times = [round(t - 100.0, 4) for _, t in swapchain.presented]
    assert times == [0.0, 0.015, 0.016, 0.031]


def test_a_late_frame_starts_a_new_chain(engine, monkeypatch):
    fg, clock = engine
    fg._interval = 0.03
    monkeypatch.setattr(fg, "_generate", lambda tex: ["gen"])
    monkeypatch.setattr(fg, "_update_interval", lambda now: None)
    swapchain = FakeSwapchain(clock)
    fg.present("real1", swapchain)  # real at 15 ms
    clock.now = 100.0 + 0.050  # late: the chained slot (30 ms) has passed
    fg.present("real2", swapchain)
    times = [round(t - 100.0, 4) for _, t in swapchain.presented]
    assert times == [0.0, 0.015, 0.050, 0.065]


def test_failure_turns_generation_off_but_presents(engine, monkeypatch):
    fg, clock = engine

    def broken(tex):
        raise RuntimeError("device lost")

    monkeypatch.setattr(fg, "_generate", broken)
    monkeypatch.setattr(fg, "close", lambda: None)
    swapchain = FakeSwapchain(clock)
    fg.present("real", swapchain)
    assert not fg.enabled
    assert [t for t, _ in swapchain.presented] == ["real"]


def test_interval_follows_the_game(engine, monkeypatch):
    fg, clock = engine
    monkeypatch.setattr(fg, "_generate", lambda tex: [])
    swapchain = FakeSwapchain(clock)
    for _ in range(60):
        fg.present("real", swapchain)
        clock.now += 1 / 30
    assert fg.interval == pytest.approx(1 / 30, rel=0.02)


def test_a_lost_frame_does_not_stretch_the_interval(engine, monkeypatch):
    fg, clock = engine
    monkeypatch.setattr(fg, "_generate", lambda tex: [])
    swapchain = FakeSwapchain(clock)
    for i in range(30):
        fg.present("real", swapchain)
        clock.now += 2 / 30 if i % 5 == 4 else 1 / 30
    assert fg.interval == pytest.approx(1 / 30)


def test_interval_is_bounded(engine):
    fg, _ = engine
    fg._update_interval(0.0)
    fg._update_interval(10.0)  # a long pause in the game
    assert fg.interval <= native_framegen._MAX_INTERVAL
