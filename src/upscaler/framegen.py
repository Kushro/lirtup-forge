"""
Frame generation through the lsfg-vk Vulkan layer.

lsfg-vk (Lossless Scaling Frame Generation on Linux) is an implicit Vulkan
layer that generates frames between the ones an application presents. The
upscaler presents through its own Vulkan swapchain, so the layer can run on
top of it. This module only prepares the layer for this process:

* The layer picks its profile once, when it loads, so everything here must
  run before the first Vulkan call of the process (:func:`prepare`).
  Turning frame generation on or off therefore needs a restart.
* lsfg-vk 2.x loads ``lsfg-vk.dll`` (the ``lsfg-vk`` branch of Lossless
  Scaling on Steam), whatever file name its configuration gives. It is
  looked up in the Steam libraries and never copied or redistributed.
* The layer is configured through a file of our own (``LSFGVK_CONFIG``),
  so the user's ``~/.config/lsfg-vk/conf.toml`` is never touched. When frame
  generation is off, ``DISABLE_LSFGVK=1`` keeps the user's own lsfg-vk
  profiles from matching the Python interpreter.
"""

from __future__ import annotations

import logging
import os
import re
from enum import Enum
from pathlib import Path
from typing import Any, Iterator, List, Mapping, Optional

from .config.yaml import default_config_path

logger = logging.getLogger(__name__)

LAYER_MANIFEST = "VkLayer_LSFGVK_frame_generation.json"
DLL_NAME = "lsfg-vk.dll"
CONFIG_FILE = "lsfg-vk.toml"
STEAM_APP_DIR = "Lossless Scaling"

# Environment variables that mean the user drives lsfg-vk by hand
_USER_ENV = ("LSFGVK_CONFIG", "LSFGVK_ENV", "DISABLE_LSFGVK")
# Marks the variables above as ours (inherited by a restarted process)
_MANAGED_MARK = "LIRTUP_FORGE_FRAMEGEN"


class FrameGenStatus(Enum):
    """State of frame generation in this process."""

    OFF = "off"  # Disabled in the configuration
    ACTIVE = "active"  # Layer configured for this process
    NO_LAYER = "no_layer"  # lsfg-vk is not installed
    NO_DLL = "no_dll"  # lsfg-vk.dll not found
    EXTERNAL = "external"  # The user set LSFGVK_* variables themselves


# Set once by prepare(), read by the pipeline and the GUI
_status: FrameGenStatus = FrameGenStatus.OFF
_multiplier: int = 1


def status() -> FrameGenStatus:
    """State decided by :func:`prepare` for this process."""
    return _status


def active_multiplier() -> int:
    """Multiplier the layer runs with (1 when frame generation is not active)."""
    return _multiplier if _status == FrameGenStatus.ACTIVE else 1


def is_active() -> bool:
    """True when lsfg-vk generates frames over our swapchain."""
    return _status in (FrameGenStatus.ACTIVE, FrameGenStatus.EXTERNAL)


# ----------------------------------------------------------------------
# Discovery
# ----------------------------------------------------------------------
def _data_home() -> Path:
    return Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")


def find_layer() -> Optional[Path]:
    """Return the lsfg-vk layer manifest, or None if it is not installed."""
    dirs = [_data_home()]
    data_dirs = os.environ.get("XDG_DATA_DIRS") or "/usr/local/share:/usr/share"
    dirs += [Path(d) for d in data_dirs.split(":") if d]
    candidates = [d / "vulkan/implicit_layer.d" for d in dirs]
    candidates += [Path("/etc/vulkan/implicit_layer.d")]
    for d in candidates:
        manifest = d / LAYER_MANIFEST
        if manifest.is_file():
            return manifest
    return None


def _steam_roots() -> List[Path]:
    home = Path.home()
    return [
        _data_home() / "Steam",
        home / ".steam/steam",
        home / ".steam/root",
        home / ".var/app/com.valvesoftware.Steam/.local/share/Steam",
    ]


def _steam_libraries() -> Iterator[Path]:
    """Steam library folders, the default ones first."""
    seen = set()
    for root in _steam_roots():
        libraries = [root]
        vdf = root / "steamapps/libraryfolders.vdf"
        try:
            text = vdf.read_text(encoding="utf-8", errors="replace")
            libraries += [Path(p) for p in re.findall(r'"path"\s+"([^"]+)"', text)]
        except OSError:
            pass
        for lib in libraries:
            try:
                key = lib.resolve()
            except OSError:
                continue
            if key not in seen:
                seen.add(key)
                yield lib


def find_dll(override: Optional[str] = None) -> Optional[Path]:
    """
    Locate ``lsfg-vk.dll``.

    Args:
        override: A file or folder given by the user. lsfg-vk always loads
            ``lsfg-vk.dll`` from the folder, so only the folder matters.
    """
    if override:
        path = Path(os.path.expanduser(override))
        folder = path if path.is_dir() else path.parent
        dll = folder / DLL_NAME
        return dll if dll.is_file() else None
    for lib in _steam_libraries():
        dll = lib / "steamapps/common" / STEAM_APP_DIR / DLL_NAME
        if dll.is_file():
            return dll
    return None


# ----------------------------------------------------------------------
# Process setup
# ----------------------------------------------------------------------
def _toml_str(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def _process_name() -> str:
    """Executable name lsfg-vk matches profiles against (/proc/self/exe)."""
    try:
        return os.path.basename(os.readlink("/proc/self/exe"))
    except OSError:
        return "python3"


def _write_config(
    dll: Path, multiplier: int, flow_scale: float, performance: bool
) -> Path:
    path = Path(default_config_path(CONFIG_FILE))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "# Written by LirtUp Forge at startup; edits are overwritten.\n"
        "version = 2\n"
        "\n"
        "[global]\n"
        "allow_fp16 = true\n"
        f"dll = {_toml_str(str(dll))}\n"
        "\n"
        "[[profile]]\n"
        'name = "LirtUp Forge"\n'
        f"active_in = {_toml_str(_process_name())}\n"
        f"multiplier = {multiplier}\n"
        f"flow_scale = {flow_scale:.2f}\n"
        f"performance_mode = {'true' if performance else 'false'}\n"
        'pacing_mode = "vsync"\n',
        encoding="utf-8",
    )
    return path


def prepare(
    multiplier: int,
    flow_scale: float = 1.0,
    performance: bool = False,
    dll_path: Optional[str] = None,
) -> FrameGenStatus:
    """
    Configure lsfg-vk for this process. Must run before any Vulkan call.

    Args:
        multiplier: Output frames per captured frame (1 = off, 2 to 5).
        flow_scale: Optical flow resolution (0.25 to 1.0).
        performance: Use the lighter model.
        dll_path: Folder (or file inside it) holding ``lsfg-vk.dll``.
    """
    global _status, _multiplier

    if os.environ.pop(_MANAGED_MARK, None):
        for var in _USER_ENV:
            os.environ.pop(var, None)
    if any(var in os.environ for var in _USER_ENV):
        _status = (
            FrameGenStatus.OFF
            if os.environ.get("DISABLE_LSFGVK") == "1"
            else FrameGenStatus.EXTERNAL
        )
        logger.info("lsfg-vk is configured by the environment, leaving it alone")
        return _status

    _multiplier = multiplier
    if multiplier <= 1:
        _status = FrameGenStatus.OFF
    elif find_layer() is None:
        _status = FrameGenStatus.NO_LAYER
        logger.warning("Frame generation needs lsfg-vk, which is not installed")
    elif (dll := find_dll(dll_path)) is None:
        _status = FrameGenStatus.NO_DLL
        logger.warning(
            "Frame generation needs %s (the 'lsfg-vk' branch of Lossless "
            "Scaling on Steam), which was not found",
            DLL_NAME,
        )
    else:
        config = _write_config(dll, multiplier, flow_scale, performance)
        os.environ["LSFGVK_CONFIG"] = str(config)
        os.environ[_MANAGED_MARK] = "1"
        _status = FrameGenStatus.ACTIVE
        logger.info("Frame generation x%d through lsfg-vk", multiplier)
        return _status

    os.environ["DISABLE_LSFGVK"] = "1"
    os.environ[_MANAGED_MARK] = "1"
    return _status


def prepare_from_options(options: Mapping[str, Any]) -> FrameGenStatus:
    """
    :func:`prepare` from global options (YAML merged with CLI overrides).

    Profiles cannot change frame generation: the layer is set up once per
    process, before any window is matched.
    """
    from .config import DEFAULT_CONFIG

    def opt(name: str) -> Any:
        value = options.get(name)
        return getattr(DEFAULT_CONFIG, name) if value is None else value

    try:
        return prepare(
            int(opt("frame_gen_multiplier")),
            float(opt("frame_gen_flow_scale")),
            bool(opt("frame_gen_performance")),
            opt("frame_gen_dll"),
        )
    except (TypeError, ValueError):
        logger.warning("Invalid frame generation options, frame generation is off")
        return prepare(1)
