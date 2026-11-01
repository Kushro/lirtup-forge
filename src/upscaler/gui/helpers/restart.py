"""
Relaunching the GUI process.

Some settings (frame generation) are read once per process, so applying
them means starting a new one. :func:`relaunch` replaces the current
process with a fresh copy of itself; the window that was being upscaled
travels in the environment and :func:`take_resume` picks it up.
"""

from __future__ import annotations

import logging
import os
import sys
from dataclasses import dataclass
from typing import List, Optional, Sequence

logger = logging.getLogger(__name__)

_RESUME_WINDOW = "LIRTUP_FORGE_RESUME_WINDOW"
_RESUME_HIDDEN = "LIRTUP_FORGE_RESUME_HIDDEN"

# CLI options that would override what the relaunch is meant to apply
_DROPPED_OPTIONS = ("--frame-gen",)


@dataclass
class RestartRequest:
    """What the relaunched process should do."""

    window: Optional[int] = None  # window handle to upscale again
    hide_after: bool = False  # keep the GUI hidden once that session stops


def strip_options(args: Sequence[str], options: Sequence[str] = _DROPPED_OPTIONS) -> List[str]:
    """Remove *options* (``--opt value`` or ``--opt=value``) from *args*."""
    result: List[str] = []
    skip = False
    for arg in args:
        if skip:
            skip = False
            continue
        if arg in options:
            skip = True
            continue
        if any(arg.startswith(opt + "=") for opt in options):
            continue
        result.append(arg)
    return result


def command_line() -> List[str]:
    """
    The command that started this process, with the interpreter's own
    options (``-m upscaler.gui``) kept and the dropped options removed.
    """
    orig = list(getattr(sys, "orig_argv", []) or [sys.executable, *sys.argv])
    script_args = len(sys.argv) - 1
    head = orig[: len(orig) - script_args] if script_args else orig
    tail = orig[len(orig) - script_args :] if script_args else []
    return [sys.executable, *head[1:], *strip_options(tail)]


def relaunch(request: RestartRequest) -> None:
    """Replace this process with a new copy of itself (does not return)."""
    env = dict(os.environ)
    env.pop(_RESUME_WINDOW, None)
    env.pop(_RESUME_HIDDEN, None)
    if request.window is not None:
        env[_RESUME_WINDOW] = str(request.window)
        env[_RESUME_HIDDEN] = "1" if request.hide_after else "0"
    argv = command_line()
    logger.info("Relaunching: %s", " ".join(argv))
    sys.stdout.flush()
    sys.stderr.flush()
    os.execve(sys.executable, argv, env)


def take_resume() -> Optional[RestartRequest]:
    """The request left by :func:`relaunch`, removed from the environment."""
    window = os.environ.pop(_RESUME_WINDOW, None)
    hidden = os.environ.pop(_RESUME_HIDDEN, "0")
    if not window:
        return None
    try:
        return RestartRequest(window=int(window), hide_after=hidden == "1")
    except ValueError:
        return None
