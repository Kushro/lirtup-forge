"""Dialogs public module."""

from .about import AboutDialog
from .bench import BenchDialog, open_bench_dialog, open_framegen_bench
from .confirm import confirm_pending_changes
from .font import FontPickerDialog
from .profile import ProfileDialog

__all__ = [
    "AboutDialog",
    "BenchDialog",
    "FontPickerDialog",
    "ProfileDialog",
    "confirm_pending_changes",
    "open_bench_dialog",
    "open_framegen_bench",
]
