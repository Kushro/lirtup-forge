"""Relaunching the GUI process (without actually replacing it)."""

import os
import sys

from upscaler.gui.helpers import restart
from upscaler.gui.helpers.restart import RestartRequest


def test_strip_options():
    args = ["--frame-gen", "3", "-p", "game", "--frame-gen=2", "--frame-gen-dll", "x"]
    assert restart.strip_options(args) == ["-p", "game", "--frame-gen-dll", "x"]


def test_command_line_keeps_module_invocation(monkeypatch):
    monkeypatch.setattr(sys, "executable", "/venv/bin/python3")
    monkeypatch.setattr(
        sys, "orig_argv", ["python3", "-m", "upscaler.gui", "--frame-gen", "2", "-v"]
    )
    monkeypatch.setattr(sys, "argv", ["/src/upscaler/gui/__main__.py", "--frame-gen", "2", "-v"])
    assert restart.command_line() == ["/venv/bin/python3", "-m", "upscaler.gui", "-v"]


def test_command_line_for_entry_point_script(monkeypatch):
    monkeypatch.setattr(sys, "executable", "/venv/bin/python3")
    monkeypatch.setattr(sys, "orig_argv", ["/venv/bin/python3", "/venv/bin/upscale-gui"])
    monkeypatch.setattr(sys, "argv", ["/venv/bin/upscale-gui"])
    assert restart.command_line() == ["/venv/bin/python3", "/venv/bin/upscale-gui"]


def test_relaunch_passes_the_window(monkeypatch):
    calls = []
    monkeypatch.setattr(os, "execve", lambda *a: calls.append(a))
    monkeypatch.setattr(restart, "command_line", lambda: ["python3", "upscale-gui"])
    restart.relaunch(RestartRequest(window=0x4400007, hide_after=True))
    (path, argv, env), = calls
    assert argv == ["python3", "upscale-gui"]
    assert env["LIRTUP_FORGE_RESUME_WINDOW"] == str(0x4400007)
    assert env["LIRTUP_FORGE_RESUME_HIDDEN"] == "1"


def test_take_resume_reads_and_clears(monkeypatch):
    monkeypatch.setenv("LIRTUP_FORGE_RESUME_WINDOW", "123")
    monkeypatch.setenv("LIRTUP_FORGE_RESUME_HIDDEN", "0")
    assert restart.take_resume() == RestartRequest(window=123, hide_after=False)
    assert "LIRTUP_FORGE_RESUME_WINDOW" not in os.environ
    assert restart.take_resume() is None


def test_take_resume_ignores_garbage(monkeypatch):
    monkeypatch.setenv("LIRTUP_FORGE_RESUME_WINDOW", "not-a-number")
    assert restart.take_resume() is None
