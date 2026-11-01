"""Shared fixtures: keep every test away from the user's real files."""

import pytest


@pytest.fixture(autouse=True)
def isolated_home(tmp_path, monkeypatch):
    """A fresh HOME and XDG directories for each test."""
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("XDG_CONFIG_HOME", str(home / ".config"))
    monkeypatch.setenv("XDG_DATA_HOME", str(home / ".local/share"))
    monkeypatch.setenv("XDG_DATA_DIRS", str(tmp_path / "no-system-data"))
    for var in (
        "LSFGVK_CONFIG",
        "LSFGVK_ENV",
        "DISABLE_LSFGVK",
        "LIRTUP_FORGE_FRAMEGEN",
        "LIRTUP_FORGE_RESUME_WINDOW",
        "LIRTUP_FORGE_RESUME_HIDDEN",
    ):
        monkeypatch.delenv(var, raising=False)
    return home
