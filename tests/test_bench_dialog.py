"""The Measure on my GPU dialog and the effects tab, without a GPU (offscreen Qt)."""

import copy
import os

import pytest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
QtWidgets = pytest.importorskip("PySide6.QtWidgets")

from upscaler import bench  # noqa: E402
from upscaler.bench import BenchResult  # noqa: E402

GPU = {"key": "1002:7550:Radeon Test", "name": "Radeon Test"}


@pytest.fixture(scope="module")
def app():
    return QtWidgets.QApplication.instance() or QtWidgets.QApplication([])


@pytest.fixture
def gui_config():
    from upscaler.gui.config.config import GUIStyleOverrides
    from upscaler.gui.config.palette import DARK
    from upscaler.gui.config.style import resolve_gui_config

    return resolve_gui_config(GUIStyleOverrides(palette=DARK))


@pytest.fixture
def no_framegen(monkeypatch):
    import upscaler.framegen as framegen

    monkeypatch.setattr(framegen, "native_available", lambda: False)


@pytest.fixture
def framegen_ready(monkeypatch, tmp_path):
    import upscaler.framegen as framegen

    dll = tmp_path / "Lossless.dll"
    dll.write_bytes(b"MZ")
    monkeypatch.setattr(framegen, "native_available", lambda: True)
    return dll


def make_dialog(app, gui_config, **kw):
    from upscaler.gui.dialogs.bench import BenchDialog

    dialog = BenchDialog(gui_config, **kw)
    app.processEvents()
    return dialog


# ----------------------------------------------------------------------
# Helpers
# ----------------------------------------------------------------------
def test_bench_command():
    from upscaler.gui.dialogs.bench import bench_command

    assert bench_command(["effects"], "1280x720", "2560x1440") == [
        "-m", "upscaler.bench", "--json", "--save",
        "--section", "effects",
        "--input-size", "1280x720", "--target-size", "2560x1440",
    ]
    args = bench_command(["effects", "framegen"], "1x1", "2x2", gpu="1", dll="/x/Lossless.dll")
    assert args[-4:] == ["--gpu", "1", "--dll", "/x/Lossless.dll"]
    assert args.count("--section") == 2


def test_bench_environment_turns_the_layer_off():
    from upscaler.gui.dialogs.bench import bench_environment

    root = os.path.dirname(os.path.dirname(os.path.abspath(bench.__file__)))
    env = bench_environment(
        {"LSFGVK_CONFIG": "/c.toml", "LSFGVK_ENV": "1", "PYTHONPATH": f"/other{os.pathsep}{root}", "KEEP": "y"}
    )
    assert env["DISABLE_LSFGVK"] == "1"
    assert "LSFGVK_CONFIG" not in env and "LSFGVK_ENV" not in env
    assert env["PYTHONPATH"].split(os.pathsep) == [root, "/other"]
    assert env["KEEP"] == "y"


def test_event_reader_handles_partial_lines_and_noise():
    from upscaler.gui.dialogs.bench import EventReader

    reader = EventReader()
    assert reader.feed(b'{"event": "sta') == []
    assert reader.feed(b'rt", "total": 2}\nnot json\n[1]\n\n{"no_event": 1}\n{"event": "do') == [
        {"event": "start", "total": 2}
    ]
    assert reader.feed(b'ne", "count": 2}\n') == [{"event": "done", "count": 2}]


# ----------------------------------------------------------------------
# Dialog
# ----------------------------------------------------------------------
def test_opens_on_the_requested_section(app, gui_config, framegen_ready):
    d = make_dialog(app, gui_config, section=bench.SECTION_FRAMEGEN, dll=str(framegen_ready))
    assert d._framegen_cb.isChecked() and not d._effects_cb.isChecked()
    assert d._sections() == [bench.SECTION_FRAMEGEN]
    assert d._dll == str(framegen_ready)
    assert d._start_btn.isEnabled()
    assert not d._cancel_btn.isEnabled()
    d.close()


def test_framegen_disabled_without_the_engine(app, gui_config, no_framegen):
    d = make_dialog(app, gui_config, section=bench.SECTION_FRAMEGEN)
    assert not d._framegen_cb.isEnabled()
    assert not d._framegen_cb.isChecked()
    assert d._framegen_note.text()
    assert d._sections() == []
    assert not d._start_btn.isEnabled()
    d.close()


def test_framegen_disabled_without_the_dll(app, gui_config, monkeypatch, tmp_path):
    import upscaler.framegen as framegen

    monkeypatch.setattr(framegen, "native_available", lambda: True)
    d = make_dialog(
        app, gui_config, section=bench.SECTION_FRAMEGEN, dll=str(tmp_path / "missing.dll")
    )
    assert not d._framegen_cb.isEnabled()
    assert "Lossless.dll" in d._framegen_note.text()
    d.close()


def test_refuses_to_start_during_a_session(app, gui_config, no_framegen):
    active = {"value": True}
    d = make_dialog(app, gui_config, session_active=lambda: active["value"])
    assert not d._start_btn.isEnabled()
    assert not d._warning.isHidden()
    d.start()
    assert not d.running
    active["value"] = False
    d._update_state()
    assert d._start_btn.isEnabled()
    assert d._warning.isHidden()
    d.close()


def test_events_fill_the_table_and_progress(app, gui_config, no_framegen):
    d = make_dialog(app, gui_config)
    saved = []
    d.results_saved.connect(lambda: saved.append(True))
    d.handle_event({"event": "start", "total": 2, "gpu": GPU})
    assert d._progress.maximum() == 2
    assert "Radeon Test" in d._status.text()
    d.handle_event({"event": "case", "index": 0, "total": 2, "kind": "native", "name": "LumaSharpen"})
    assert "LumaSharpen" in d._status.text()
    d.handle_event({"event": "result", "done": 1, "total": 2, "kind": "native", "name": "LumaSharpen", "ms": 0.0162, "p95": 0.02, "error": None})
    d.handle_event({"event": "result", "done": 2, "total": 2, "kind": "magpiefx", "name": "Broken", "ms": None, "p95": None, "error": "not found"})
    assert d._progress.value() == 2
    assert d._table.rowCount() == 2
    assert d._table.item(0, 1).text() == "LumaSharpen"
    assert d._table.item(0, 2).text() == "0.016"
    assert d._table.item(1, 2).toolTip() == "not found"
    d.handle_event({"event": "done", "count": 2})
    assert d.saved and saved == [True]
    d.close()


def test_cancelled_run_with_results_counts_as_saved(app, gui_config, no_framegen):
    d = make_dialog(app, gui_config)
    d.handle_event({"event": "cancelled", "count": 0})
    assert not d.saved
    d.handle_event({"event": "cancelled", "count": 3})
    assert d.saved
    d.close()


def test_error_event_is_shown(app, gui_config, no_framegen):
    d = make_dialog(app, gui_config)
    d.handle_event({"event": "error", "error": "no Vulkan device"})
    assert "no Vulkan device" in d._status.text()
    d.close()


def test_shows_the_stored_results(app, gui_config, no_framegen):
    bench.save_results(
        GPU,
        [BenchResult(bench.NATIVE, "LumaSharpen", 0.5), BenchResult(bench.MAGPIEFX, "CAS", 0.25)],
        (1280, 720),
        (2560, 1440),
    )
    d = make_dialog(app, gui_config)
    names = [d._table.item(r, 1).text() for r in range(d._table.rowCount())]
    assert names == ["LumaSharpen", "CAS"]
    assert "Radeon Test" in d._status.text()
    d.close()


def test_session_check_reads_the_main_window():
    from upscaler.gui.dialogs.bench import _main_window_session_check

    class Window:
        manual_session = None
        daemon_ctrl = type("D", (), {"active": False})()

    class Widget:
        def __init__(self, window):
            self._w = window

        def window(self):
            return self._w

    w = Window()
    check = _main_window_session_check(Widget(w))
    assert check() is False
    w.manual_session = object()
    assert check() is True
    w.manual_session = None
    w.daemon_ctrl.active = True
    assert check() is True
    assert _main_window_session_check(None)() is False


# ----------------------------------------------------------------------
# Effects tab
# ----------------------------------------------------------------------
@pytest.fixture
def effects_tab_factory(app, gui_config):
    tabs = []

    def make():
        from upscaler.config import DEFAULT_CONFIG
        from upscaler.gui.sidebars.tabs import EffectsTab

        cfg = copy.deepcopy(DEFAULT_CONFIG)
        base = copy.deepcopy(DEFAULT_CONFIG)
        tab = EffectsTab(gui_config, cfg, base, base)
        tab.show()  # the rows are built when the tab is first shown
        app.processEvents()
        tabs.append(tab)
        return tab

    yield make
    for tab in tabs:
        tab.close()


def test_effects_tab_shows_reference_times_until_measured(effects_tab_factory):
    tab = effects_tab_factory()
    html = tab._engine_info.text()
    assert "RX 9070 XT" in html
    assert "0.145" in html  # reference Anime4K time
    assert tab._measured_rows() == []


def test_effects_tab_shows_measured_times(effects_tab_factory):
    bench.save_results(
        GPU,
        [
            BenchResult(bench.NATIVE, "Anime4K_Upscale_S", 0.777),
            BenchResult(bench.MAGPIEFX, "Anime4K_Upscale_S", 0.888),
        ],
        (1280, 720),
        (2560, 1440),
    )
    tab = effects_tab_factory()
    html = tab._engine_info.text()
    assert "Radeon Test" in html
    assert "0.777" in html and "0.888" in html
    assert "1280x720" in html and "2560x1440" in html
    # Effects not measured show a dash
    assert "&ndash;" in html


def test_effects_tab_has_the_measure_button(effects_tab_factory):
    tab = effects_tab_factory()
    buttons = [b.text() for b in tab.findChildren(QtWidgets.QPushButton)]
    assert "Measure on my GPU" in buttons


def test_measure_refreshes_the_tab(effects_tab_factory, monkeypatch):
    import upscaler.gui.sidebars.tabs.effects as effects_mod

    tab = effects_tab_factory()

    def fake_open(gui_config, config, section, parent):
        assert section == bench.SECTION_EFFECTS
        bench.save_results(GPU, [BenchResult(bench.NATIVE, "LumaSharpen", 0.321)], (1280, 720), (2560, 1440))
        return True

    monkeypatch.setattr(effects_mod, "open_bench_dialog", fake_open)
    tab._on_measure()
    assert "0.321" in tab._engine_info.text()
