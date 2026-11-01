"""Options the GUI saves on its own (tray and menu bar detection)."""

import pytest
import yaml

from upscaler.gui.config.manager import ConfigManager


@pytest.fixture
def config_file(tmp_path):
    path = tmp_path / "config.yaml"
    path.write_text(
        yaml.safe_dump(
            {
                "model": "cunny",
                "profiles": {
                    "game": {
                        "match": {"title": "Game"},
                        "options": {"crop_top": 4, "auto_crop_menu_bar": True},
                    },
                    "other": {"match": {"title": "Other"}, "options": {}},
                },
            }
        )
    )
    return path


def _load(path):
    return yaml.safe_load(path.read_text())


def test_set_global_option_keeps_other_pending_edits(config_file):
    manager = ConfigManager(str(config_file))
    manager.persistent_config.show_stats = True  # pending, unsaved
    manager.set_global_option("frame_gen_multiplier", 3)

    data = _load(config_file)
    assert data["frame_gen_multiplier"] == 3
    assert data["model"] == "cunny"
    assert "show_stats" not in data
    assert set(data["profiles"]) == {"game", "other"}
    assert manager.persistent_config.frame_gen_multiplier == 3
    assert manager.saved_persistent_config.frame_gen_multiplier == 3
    assert manager.persistent_config.show_stats is True
    assert manager.is_dirty()


def test_set_global_option_with_a_profile_active(config_file):
    manager = ConfigManager(str(config_file))
    manager.set_active_profile("other")
    manager.set_global_option("frame_gen_multiplier", 2)
    assert manager.global_baseline.frame_gen_multiplier == 2
    assert manager.persistent_config.frame_gen_multiplier == 2


def test_update_profile_options_saves_crop(config_file):
    manager = ConfigManager(str(config_file))
    manager.set_active_profile("game")
    manager.update_profile_options(
        "game", {"crop_top": 27, "auto_crop_menu_bar": False}
    )

    options = _load(config_file)["profiles"]["game"]["options"]
    assert options == {"crop_top": 27, "auto_crop_menu_bar": False}
    assert manager.persistent_config.crop_top == 27
    assert manager.persistent_config.auto_crop_menu_bar is False
    assert not manager.is_dirty()


def test_update_profile_options_unknown_profile(config_file):
    before = config_file.read_text()
    ConfigManager(str(config_file)).update_profile_options("nope", {"crop_top": 1})
    assert config_file.read_text() == before
