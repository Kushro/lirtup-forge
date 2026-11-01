"""lsfg-vk discovery and process setup (no Vulkan involved)."""

import os

import pytest

from upscaler import framegen
from upscaler.framegen import FrameGenStatus


def _steam_library(root, with_dll=True):
    app = root / "steamapps/common" / framegen.STEAM_APP_DIR
    app.mkdir(parents=True)
    if with_dll:
        (app / framegen.DLL_NAME).write_bytes(b"MZ")
    return app / framegen.DLL_NAME


@pytest.fixture
def layer(monkeypatch, tmp_path):
    """Pretend the lsfg-vk layer is installed."""
    manifest = tmp_path / framegen.LAYER_MANIFEST
    manifest.write_text("{}")
    monkeypatch.setattr(framegen, "find_layer", lambda: manifest)
    return manifest


def test_find_dll_in_default_steam_library(isolated_home):
    dll = _steam_library(isolated_home / ".local/share/Steam")
    assert framegen.find_dll() == dll


def test_find_dll_in_extra_library_from_vdf(isolated_home, tmp_path):
    steam = isolated_home / ".local/share/Steam/steamapps"
    steam.mkdir(parents=True)
    extra = tmp_path / "Warehouse/SteamLibrary"
    dll = _steam_library(extra)
    (steam / "libraryfolders.vdf").write_text(
        '"libraryfolders"\n{\n "1"\n {\n  "path"  "%s"\n }\n}\n' % extra
    )
    assert framegen.find_dll() == dll


def test_find_dll_override_uses_the_folder(tmp_path):
    folder = tmp_path / "lsfg"
    folder.mkdir()
    (folder / framegen.DLL_NAME).write_bytes(b"MZ")
    # Any file name in that folder points at lsfg-vk.dll, as the layer does
    assert framegen.find_dll(str(folder / "Lossless.dll")) == folder / framegen.DLL_NAME
    assert framegen.find_dll(str(folder)) == folder / framegen.DLL_NAME
    assert framegen.find_dll(str(tmp_path / "missing")) is None


def test_off_disables_the_layer():
    assert framegen.prepare(1) == FrameGenStatus.OFF
    assert os.environ["DISABLE_LSFGVK"] == "1"
    assert framegen.active_multiplier() == 1
    assert not framegen.is_active()


def test_no_layer(monkeypatch):
    monkeypatch.setattr(framegen, "find_layer", lambda: None)
    assert framegen.prepare(2) == FrameGenStatus.NO_LAYER
    assert os.environ["DISABLE_LSFGVK"] == "1"


def test_no_dll(layer):
    assert framegen.prepare(2) == FrameGenStatus.NO_DLL
    assert os.environ["DISABLE_LSFGVK"] == "1"


def test_active_writes_own_config(layer, isolated_home):
    dll = _steam_library(isolated_home / ".local/share/Steam")
    assert framegen.prepare(3, flow_scale=0.5, performance=True) == FrameGenStatus.ACTIVE
    config = os.environ["LSFGVK_CONFIG"]
    assert config.startswith(str(isolated_home / ".config"))
    text = open(config, encoding="utf-8").read()
    assert f'dll = "{dll}"' in text
    assert "multiplier = 3" in text
    assert "flow_scale = 0.50" in text
    assert "performance_mode = true" in text
    assert "DISABLE_LSFGVK" not in os.environ
    assert framegen.active_multiplier() == 3
    assert framegen.is_active()


def test_user_environment_is_respected(monkeypatch):
    monkeypatch.setenv("LSFGVK_ENV", "1")
    assert framegen.prepare(2) == FrameGenStatus.EXTERNAL
    assert os.environ["LSFGVK_ENV"] == "1"
    assert "LSFGVK_CONFIG" not in os.environ


def test_user_disable_is_respected(monkeypatch):
    monkeypatch.setenv("DISABLE_LSFGVK", "1")
    assert framegen.prepare(2) == FrameGenStatus.OFF


def test_own_variables_are_replaced_on_relaunch(layer, isolated_home):
    """A relaunched process inherits our variables and must not take them as the user's."""
    _steam_library(isolated_home / ".local/share/Steam")
    assert framegen.prepare(1) == FrameGenStatus.OFF  # sets DISABLE_LSFGVK + mark
    assert framegen.prepare(2) == FrameGenStatus.ACTIVE
    assert "DISABLE_LSFGVK" not in os.environ
    assert framegen.prepare(1) == FrameGenStatus.OFF
    assert "LSFGVK_CONFIG" not in os.environ


def test_invalid_options_fall_back_to_off():
    status = framegen.prepare_from_options({"frame_gen_multiplier": "lots"})
    assert status == FrameGenStatus.OFF


# ----------------------------------------------------------------------
# Native engine
# ----------------------------------------------------------------------
def _native_dll(root):
    app = root / "steamapps/common" / framegen.STEAM_APP_DIR
    app.mkdir(parents=True, exist_ok=True)
    dll = app / framegen.NATIVE_DLL_NAME
    dll.write_bytes(b"MZ")
    return dll


def test_find_native_dll_in_steam(isolated_home):
    dll = _native_dll(isolated_home / ".local/share/Steam")
    assert framegen.find_dll(native=True) == dll


def test_find_native_dll_override_takes_the_file(tmp_path):
    dll = tmp_path / "copy-of-Lossless.dll"
    dll.write_bytes(b"MZ")
    assert framegen.find_dll(str(dll), native=True) == dll


def test_native_without_extension(monkeypatch):
    monkeypatch.setattr(framegen, "native_available", lambda: False)
    assert framegen.prepare(2, engine="native") == FrameGenStatus.NO_NATIVE
    assert os.environ["DISABLE_LSFGVK"] == "1"
    assert framegen.native_settings() is None


def test_native_without_dll(monkeypatch):
    monkeypatch.setattr(framegen, "native_available", lambda: True)
    assert framegen.prepare(2, engine="native") == FrameGenStatus.NO_DLL


def test_native_active_keeps_the_layer_off(monkeypatch, isolated_home):
    monkeypatch.setattr(framegen, "native_available", lambda: True)
    dll = _native_dll(isolated_home / ".local/share/Steam")
    status = framegen.prepare(3, flow_scale=0.5, engine="native")
    assert status == FrameGenStatus.ACTIVE
    assert framegen.engine() == "native"
    assert os.environ["DISABLE_LSFGVK"] == "1"
    assert "LSFGVK_CONFIG" not in os.environ
    assert framegen.native_settings() == framegen.NativeSettings(dll, 3, 0.5, False)
    assert framegen.active_multiplier() == 3


def test_native_extension_rejects_a_bad_dll(tmp_path):
    lsfg = pytest.importorskip("upscaler.lsfg.lsfg")
    fake = tmp_path / "Lossless.dll"
    fake.write_bytes(b"not a PE file")
    assert lsfg.probe(str(fake)) == "dll_unreadable"
    assert lsfg.probe(str(tmp_path / "missing.dll")) == "dll_unreadable"
