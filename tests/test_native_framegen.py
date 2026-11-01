"""Pacing of the native frame generation engine, without a GPU."""

import threading
import time

import pytest

pytest.importorskip("upscaler.vulkan.vulkan")

from upscaler.framegen import NativeSettings  # noqa: E402
from upscaler.pipeline import native_framegen  # noqa: E402
from upscaler.pipeline.native_framegen import (  # noqa: E402
    AutoMultiplier,
    NativeFrameGen,
    _Cycle,
    ideal_multiplier,
)


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


def _engine(monkeypatch, multiplier=3, auto=False, refresh_hz=60.0):
    monkeypatch.setattr(native_framegen, "get_current_device", FakeDevice)
    settings = NativeSettings("Lossless.dll", multiplier, 1.0, False, auto)
    return NativeFrameGen(settings, refresh_hz)


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
        fg, "_generate", lambda tex, sc, *_: _Cycle([f"gen-{tex}"], [1], tex, sc)
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
    monkeypatch.setattr(
        fg, "_generate", lambda tex, sc, *_: _Cycle(["gen"], [1], tex, sc)
    )
    monkeypatch.setattr(fg._device, "wait_semaphore", lambda s, t: False)  # lost frame
    fg.present("a", swapchain)
    fg.present("b", swapchain)
    assert not fg.enabled
    assert [t for t, _ in swapchain.presented] == ["b"]


def test_a_failure_while_generating_still_presents(monkeypatch):
    fg = _engine(monkeypatch)

    def broken(tex, sc, *_):
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


# ----------------------------------------------------------------------
# Latency reports
# ----------------------------------------------------------------------
def test_every_present_reports_its_capture_time(clocked):
    fg, clock, swapchain = clocked
    reports = []
    fg.on_present = lambda capture, kind, m: reports.append((capture, kind, m))
    fg._show(_Cycle(["gen1", "gen2"], [1, 2], "real", swapchain, capture_time=99.5))
    assert reports == [
        (99.5, "generated", 3),
        (99.5, "generated", 3),
        (99.5, "real", 3),
    ]


def test_a_cycle_without_capture_time_reports_nothing(clocked):
    fg, clock, swapchain = clocked
    reports = []
    fg.on_present = lambda *args: reports.append(args)
    fg._show(_Cycle(["gen"], [1], "real", swapchain))
    assert reports == []


def test_the_capture_mark_reaches_the_cycle(monkeypatch):
    fg = _engine(monkeypatch, multiplier=2)
    seen = []

    def generate(tex, sc, capture_time=None):
        seen.append(capture_time)
        raise RuntimeError("stop here")

    monkeypatch.setattr(fg, "_generate", generate)
    fg.mark_capture(12.5)
    fg.present("real", FakeSwapchain())
    assert seen == [12.5]


def test_presents_without_generation_report_real_frames(monkeypatch):
    fg = _engine(monkeypatch)
    fg.enabled = False
    reports = []
    fg.on_present = lambda capture, kind, m: reports.append((capture, kind))
    fg.mark_capture(7.0)
    fg.present("real", FakeSwapchain())
    fg.present("real", FakeSwapchain())  # no new mark: nothing to report
    assert reports == [(7.0, "real")]


def test_a_failing_latency_callback_does_not_stop_presenting(clocked):
    fg, clock, swapchain = clocked

    def broken(*args):
        raise ValueError("stats gone")

    fg.on_present = broken
    fg._show(_Cycle(["gen"], [1], "real", swapchain, capture_time=1.0))
    assert [t for t, _ in swapchain.presented] == ["gen", "real"]


# ----------------------------------------------------------------------
# Automatic multiplier
# ----------------------------------------------------------------------
@pytest.mark.parametrize(
    "fps, refresh, expected",
    [
        (24, 144, 5),
        (30, 144, 4),
        (48, 144, 3),
        (60, 144, 2),
        (100, 144, 2),  # never below x2
        (10, 144, 5),  # never above x5
        (30, 60, 2),
        (24, 120, 5),
        (29.97, 144, 4),
        (30.5, 144, 4),
    ],
)
def test_ideal_multiplier_fits_the_refresh(fps, refresh, expected):
    assert ideal_multiplier(fps, refresh) == expected


def test_ideal_multiplier_without_a_measure():
    assert ideal_multiplier(0, 144) == 2


def test_first_choice_is_taken_at_once():
    auto = AutoMultiplier(144)
    assert auto.update(30, now=0.0) == 4


def test_a_new_choice_must_hold_before_switching():
    auto = AutoMultiplier(144, current=4)
    auto._last_switch = -100.0
    assert auto.update(24, now=0.0) == 4  # candidate x5
    assert auto.update(24, now=1.0) == 4
    assert auto.update(24, now=2.1) == 5


def test_a_short_change_is_ignored():
    auto = AutoMultiplier(144, current=4)
    auto._last_switch = -100.0
    auto.update(24, now=0.0)
    auto.update(30, now=1.0)  # back before the hold time: candidate dropped
    assert auto.update(24, now=2.5) == 4
    assert auto.update(24, now=4.6) == 5


def test_switches_are_spaced_out():
    auto = AutoMultiplier(144)
    assert auto.update(30, now=0.0) == 4
    auto.update(60, now=1.0)
    assert auto.update(60, now=3.5) == 4  # held 2.5 s, but 5 s since the last switch
    assert auto.update(60, now=5.0) == 2


def test_hysteresis_band_keeps_the_multiplier():
    """Between 27.9 and 29.3 fps at 144 Hz, x4 and x5 both stay put."""
    at_x4 = AutoMultiplier(144, current=4)
    at_x5 = AutoMultiplier(144, current=5)
    for fps in (28.0, 28.5, 29.0, 29.3):
        assert at_x4.target(fps) == 4
        assert at_x5.target(fps) == 5
    assert at_x4.target(27.5) == 5
    assert at_x5.target(30.0) == 4


def test_output_above_the_refresh_goes_down_to_the_ideal():
    auto = AutoMultiplier(144, current=5)
    assert auto.target(60) == 2


def test_a_still_image_keeps_the_choice():
    auto = AutoMultiplier(144, current=4)
    auto._last_switch = -100.0
    for t in range(10):
        assert auto.update(1.0, now=float(t)) == 4


def test_noise_around_30_fps_never_switches():
    auto = AutoMultiplier(144)
    choices = {
        auto.update(30 + 0.4 * ((i % 5) - 2), now=i * 0.1) for i in range(2, 302)
    }
    assert choices == {4}


class FakeLsfg:
    def __init__(self):
        self.calls = []

    def initialize(self, dll, vendor, device, generated, **kwargs):
        self.calls.append(("initialize", generated))
        return True

    def finalize(self):
        self.calls.append(("finalize",))

    def delete_context(self, context):
        self.calls.append(("delete_context", context))


def _auto_engine(monkeypatch):
    fg = _engine(monkeypatch, multiplier=2, auto=True, refresh_hz=144)
    fg._device.vendor_id = fg._device.device_id = 0
    fg._device.name = "fake"
    fg._device.supports_fp16 = True
    fg._device.supports_external_fd = True
    monkeypatch.setattr(fg, "_release_shared", lambda: None)
    return fg


def test_auto_waits_for_a_measure_before_starting_lsfg(monkeypatch):
    fg = _auto_engine(monkeypatch)
    generated = []

    def generate(tex, sc, *_):
        generated.append(tex)
        return _Cycle([], [], tex, sc)

    monkeypatch.setattr(fg, "_generate", generate)
    fg._interval = 1 / 30
    monkeypatch.setattr(fg, "_update_interval", lambda now: None)
    swapchain = FakeSwapchain()
    for i in range(native_framegen._AUTO_WARMUP_FRAMES):
        fg.present(i, swapchain)
    assert generated == []  # real frames only while measuring
    fg.present("first", swapchain)
    fg._drain()
    fg.close()
    assert generated == ["first"]
    assert fg.multiplier == 4  # 30 fps at 144 Hz


def test_a_new_multiplier_restarts_lsfg_on_its_own_thread(monkeypatch):
    fg = _auto_engine(monkeypatch)
    lsfg = FakeLsfg()
    fg._lsfg = fg._module = lsfg
    fg._context = 7
    fg._auto.current = 4
    fg.generated = 3
    swapchain = FakeSwapchain()

    monkeypatch.setattr(fg, "_generate", lambda tex, sc, *_: _Cycle([], [], tex, sc))
    fg._begin_restart(2)
    fg._auto.current = 2  # as if the automatic choice had asked for it
    assert fg.restarting
    assert fg.multiplier == 2
    assert lsfg.calls[0] == ("delete_context", 7)
    fg._restart.join()
    # While the restart runs, or until present() collects it, real frames only
    assert not fg._can_generate()
    fg._update_auto(now=0.0)
    assert not fg.restarting
    assert fg._lsfg is lsfg
    assert lsfg.calls[1:] == [("finalize",), ("initialize", 1)]
    fg.present("real", swapchain)  # starts a new context on the next frame
    fg.close()


def test_a_failed_restart_turns_generation_off(monkeypatch):
    fg = _auto_engine(monkeypatch)
    lsfg = FakeLsfg()

    def broken(*args, **kwargs):
        raise RuntimeError("no device")

    lsfg.initialize = broken
    fg._lsfg = fg._module = lsfg
    fg._auto.current = 4
    fg.generated = 3
    fg._begin_restart(2)
    fg._restart.join()
    swapchain = FakeSwapchain()
    fg.present("real", swapchain)
    assert not fg.enabled
    assert [t for t, _ in swapchain.presented] == ["real"]


def test_close_waits_for_a_restart(monkeypatch):
    fg = _auto_engine(monkeypatch)
    lsfg = FakeLsfg()
    release = threading.Event()
    original = lsfg.initialize

    def slow(*args, **kwargs):
        release.wait(1.0)
        return original(*args, **kwargs)

    lsfg.initialize = slow
    fg._lsfg = fg._module = lsfg
    fg._auto.current = 4
    fg.generated = 3
    fg._begin_restart(2)
    threading.Timer(0.05, release.set).start()
    fg.close()
    assert lsfg.calls[-1] == ("finalize",)  # after the restart's initialize
    assert ("initialize", 1) in lsfg.calls


def test_auto_does_not_start_high_on_a_still_image():
    """Round 1.3, T6.6: a blinking cursor (2 fps) chose x5 at 144 Hz."""
    auto = native_framegen.AutoMultiplier(144.0)
    assert auto.update(2.0, 0.0) == native_framegen.AUTO_MIN_MULTIPLIER


def test_auto_rises_once_the_game_moves():
    auto = native_framegen.AutoMultiplier(144.0)
    auto.update(2.0, 0.0)  # still image at the start: x2
    t = 0.0
    while t < 10.0:
        t += 0.1
        auto.update(30.0, t)
    assert auto.current == 4
