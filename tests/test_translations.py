"""The GUI translations ship compiled and up to date."""

from pathlib import Path
import xml.etree.ElementTree as ET

import pytest

LOCALE = Path(__file__).parent.parent / "src/upscaler/gui/locale"
SOURCES = sorted(LOCALE.glob("*.ts"))


@pytest.mark.parametrize("ts", SOURCES, ids=lambda p: p.name)
def test_every_translation_is_compiled(ts):
    qm = ts.with_suffix(".qm")
    assert qm.is_file(), f"run scripts/compile-translations.fish ({qm.name} missing)"


@pytest.mark.parametrize("ts", SOURCES, ids=lambda p: p.name)
def test_translation_sources_are_valid_xml(ts):
    ET.parse(ts)


def test_spanish_is_loaded():
    QtCore = pytest.importorskip("PySide6.QtCore")
    translator = QtCore.QTranslator()
    assert translator.load("upscale_gui_es", str(LOCALE))
    assert translator.translate("TrayController", "Frame generation", "Tray submenu (lsfg-vk)")
