"""Frame generation controls of the Presentation tab (offscreen Qt)."""

import copy
import os

import pytest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
QtWidgets = pytest.importorskip("PySide6.QtWidgets")


@pytest.fixture(scope="module")
def app():
    return QtWidgets.QApplication.instance() or QtWidgets.QApplication([])


@pytest.fixture
def tab(app):
    from upscaler.config import DEFAULT_CONFIG
    from upscaler.gui.config.config import GUIStyleOverrides
    from upscaler.gui.config.palette import DARK
    from upscaler.gui.config.style import resolve_gui_config
    from upscaler.gui.sidebars.tabs import PresentationTab

    cfg = copy.deepcopy(DEFAULT_CONFIG)
    cfg.frame_gen_multiplier = 2
    base = copy.deepcopy(DEFAULT_CONFIG)
    tab = PresentationTab(resolve_gui_config(GUIStyleOverrides(palette=DARK)), cfg, base, base)
    tab.show()  # the rows are built when the tab is first shown
    app.processEvents()
    yield tab
    tab.close()


def test_defaults_are_full_flow_and_quality_model(tab):
    assert tab._config.frame_gen_flow_scale == 1.0
    assert tab._fg_flow_note.text().startswith("Maximum: motion estimated at 100%")
    assert tab._fg_performance.currentText() == "Quality (LSFG 3.1)"


@pytest.mark.parametrize(
    "scale, start",
    [(1.0, "Maximum: "), (0.8, "High: "), (0.5, "Balanced: "), (0.25, "Low: ")],
)
def test_flow_note_describes_each_range(tab, scale, start):
    text = tab._fg_flow_text(scale)
    assert text.startswith(start)
    assert f"{int(round(scale * 100))}%" in text


def test_model_selector_sets_performance(tab):
    tab._fg_performance.setCurrentText("Performance (LSFG 3.1P)")
    assert tab._config.frame_gen_performance is True
    tab._fg_performance.setCurrentText("Quality (LSFG 3.1)")
    assert tab._config.frame_gen_performance is False
