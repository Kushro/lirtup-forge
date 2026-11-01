"""GPU budget warning of the stats panel."""

from upscaler.pipeline.stats import gpu_budget_warning


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
