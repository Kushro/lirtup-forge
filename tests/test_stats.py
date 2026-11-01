"""GPU budget warning and latency records of the stats panel."""

import csv

import pytest

from upscaler.pipeline import stats
from upscaler.pipeline.stats import LatencyLog, gpu_budget_warning


def test_no_warning_within_budget():
    assert gpu_budget_warning([("Anime4K", 10.0), ("CAS", 1.0)], 30.0) is None


def test_warning_names_the_heaviest_effect():
    warning = gpu_budget_warning([("CAS", 1.0), ("Anime4K UL", 35.0)], 30.0)
    assert warning is not None
    assert "36.0 ms > 33.3 ms" in warning
    assert "Anime4K UL" in warning


def test_untimed_effects_are_ignored():
    assert gpu_budget_warning([("Anime4K", None)], 60.0) is None


def test_lost_frames_do_not_hide_the_problem():
    """Slow effects lower the measured rate; the budget stays at 30 fps."""
    warning = gpu_budget_warning([("Anime4K UL", 34.6)], 19.0)
    assert warning is not None
    assert "34.6 ms > 33.3 ms" in warning


def test_still_image_is_not_checked():
    assert gpu_budget_warning([("Anime4K", 500.0)], 0.5) is None


# ----------------------------------------------------------------------
# The 30 fps floor
# ----------------------------------------------------------------------
def test_slow_content_keeps_the_30_fps_floor():
    """24 fps: 35 ms warns. The rate alone cannot tell slow content from
    content the effects slow down (round 1.3, T3.8)."""
    warning = gpu_budget_warning([("Anime4K UL", 35.0)], 24.0)
    assert warning is not None
    assert "35.0 ms > 33.3 ms" in warning


def test_slow_content_within_the_floor_does_not_warn():
    assert gpu_budget_warning([("Anime4K UL", 30.0)], 24.0) is None


def test_fast_content_uses_its_interval():
    warning = gpu_budget_warning([("Anime4K UL", 20.0)], 60.0)
    assert warning is not None
    assert "20.0 ms > 16.7 ms" in warning


# ----------------------------------------------------------------------
# Latency
# ----------------------------------------------------------------------
@pytest.fixture
def hud(monkeypatch):
    """A StatsHUD without fonts or GPU objects, on a fake clock."""
    clock = {"now": 100.0}
    monkeypatch.setattr(stats.time, "perf_counter", lambda: clock["now"])
    monkeypatch.setattr(stats, "OverlayBlender", lambda: None)
    monkeypatch.setattr(stats.StatsHUD, "_set_font", lambda self, height: None)

    def make(**kwargs):
        return stats.StatsHUD(1080, **kwargs)

    return make, clock


def test_latency_is_averaged_per_kind(hud):
    make, clock = hud
    h = make()
    h.record_latency(99.96, "real")  # 40 ms
    h.record_latency(99.98, "real")  # 20 ms
    h.record_latency(99.99, "generated")  # 10 ms
    latency = h.latency()
    assert latency["real"] == pytest.approx((30.0, 40.0))
    assert latency["generated"] == pytest.approx((10.0, 10.0))


def test_old_latency_samples_expire(hud):
    make, clock = hud
    h = make()
    h.record_latency(99.9, "real")
    clock["now"] = 102.0
    assert h.latency() == {}


def test_an_explicit_present_time_is_used(hud):
    make, clock = hud
    h = make()
    h.record_latency(99.0, "generated", present_time=99.025)
    assert h.latency()["generated"][0] == pytest.approx(25.0)


def test_latency_log_writes_every_present(hud, tmp_path):
    make, clock = hud
    path = tmp_path / "latency.csv"
    h = make(latency_log=str(path))
    h.record_latency(99.95, "generated", 2)
    h.record_latency(99.95, "real", 2, present_time=100.01)
    h.close()
    rows = list(csv.DictReader(path.open()))
    assert [r["kind"] for r in rows] == ["generated", "real"]
    assert [float(r["latency_ms"]) for r in rows] == pytest.approx([50.0, 60.0])
    assert rows[0]["capture_s"] == rows[1]["capture_s"]
    assert rows[1]["multiplier"] == "2"


def test_latency_log_in_a_missing_folder_is_skipped(hud, tmp_path):
    make, clock = hud
    h = make(latency_log=str(tmp_path / "missing" / "latency.csv"))
    h.record_latency(99.9, "real")  # no log, no error
    assert "real" in h.latency()


def test_latency_log_ignores_writes_after_close(tmp_path):
    log = LatencyLog(str(tmp_path / "latency.csv"))
    log.close()
    log.write(1.0, 1.02, "real", 1)
    assert (tmp_path / "latency.csv").read_text() == LatencyLog.HEADER
