"""
"Measure on my GPU": runs :mod:`upscaler.bench` in a separate process and
shows its progress and results.

The measurement runs in its own process so the GUI never waits for the GPU,
cancelling is stopping that process (SIGTERM, then SIGKILL if it does not
stop), and the built-in frame generation never shares lsfg's global state
with an upscaling session. The process stores the results per GPU
(``bench.json``); the effects tab shows them instead of the reference times.
"""

from __future__ import annotations

import json
import os
import sys
from typing import Any, Callable, Dict, List, Optional, TYPE_CHECKING

from PySide6.QtCore import QProcess, QProcessEnvironment, Qt, QTimer, Signal
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QProgressBar,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from ..styles import dialog_style, info_text_style
from ... import bench

if TYPE_CHECKING:
    from .. import GUIConfig
    from ...config import Config

INPUT_SIZES = ["1280x720", "1920x1080", "960x540", "640x480"]
TARGET_SIZES = ["2560x1440", "1920x1080", "3840x2160"]

_KILL_AFTER_MS = 5000  # after SIGTERM, a run that does not stop is killed
_SESSION_POLL_MS = 1000


def bench_command(
    sections: List[str],
    input_size: str,
    target_size: str,
    gpu: Optional[str] = None,
    dll: Optional[str] = None,
) -> List[str]:
    """Arguments of ``python -m upscaler.bench`` for one run (after the interpreter)."""
    args = ["-m", "upscaler.bench", "--json", "--save"]
    for section in sections:
        args += ["--section", section]
    args += ["--input-size", input_size, "--target-size", target_size]
    if gpu:
        args += ["--gpu", str(gpu)]
    if dll:
        args += ["--dll", str(dll)]
    return args


def bench_environment(base: Dict[str, str]) -> Dict[str, str]:
    """
    Environment of the measuring process: the package importable as in this
    process, and the lsfg-vk layer off (it only matters for swapchains, and
    the measurement must not depend on the user's lsfg-vk profiles).
    """
    env = dict(base)
    package_root = os.path.dirname(os.path.dirname(os.path.abspath(bench.__file__)))
    paths = [package_root] + [p for p in env.get("PYTHONPATH", "").split(os.pathsep) if p]
    env["PYTHONPATH"] = os.pathsep.join(dict.fromkeys(paths))
    env["DISABLE_LSFGVK"] = "1"
    env.pop("LSFGVK_CONFIG", None)
    env.pop("LSFGVK_ENV", None)
    env["PYTHONUNBUFFERED"] = "1"
    return env


class EventReader:
    """Splits the process output into JSON events (one per line)."""

    def __init__(self) -> None:
        self._buffer = b""

    def feed(self, data: bytes) -> List[Dict[str, Any]]:
        self._buffer += data
        *lines, self._buffer = self._buffer.split(b"\n")
        events = []
        for line in lines:
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if isinstance(event, dict) and "event" in event:
                events.append(event)
        return events


class BenchDialog(QDialog):
    """Progress and results of a measurement; see the module docstring."""

    results_saved = Signal()

    def __init__(
        self,
        gui_config: GUIConfig,
        section: str = bench.SECTION_EFFECTS,
        gpu: Optional[str] = None,
        dll: Optional[str] = None,
        session_active: Callable[[], bool] = lambda: False,
        parent: Optional[QWidget] = None,
    ) -> None:
        super().__init__(parent)
        self._cfg = gui_config
        self._gpu = gpu
        self._dll = dll
        self._session_active = session_active
        self._process: Optional[QProcess] = None
        self._reader = EventReader()
        self._stderr = b""
        self._finished_event: Optional[str] = None
        self._cancel_requested = False
        self._saved = False

        self.setWindowTitle(self.tr("Measure on my GPU", "Benchmark dialog title"))
        self.setStyleSheet(dialog_style(gui_config))
        self.resize(640, 560)

        layout = QVBoxLayout(self)

        intro = QLabel(
            self.tr(
                "Runs each effect and the built-in frame generation on a test image "
                "and measures the GPU time per frame. The results are saved for this "
                "GPU and shown instead of the reference times.",
                "Benchmark dialog",
            )
        )
        intro.setWordWrap(True)
        layout.addWidget(intro)

        # What to measure
        row = QHBoxLayout()
        self._effects_cb = QCheckBox(self.tr("Magpie effects", "Benchmark section"))
        self._framegen_cb = QCheckBox(self.tr("Frame generation", "Benchmark section"))
        self._effects_cb.setChecked(section == bench.SECTION_EFFECTS)
        self._framegen_cb.setChecked(section == bench.SECTION_FRAMEGEN)
        row.addWidget(self._effects_cb)
        row.addWidget(self._framegen_cb)
        row.addStretch()
        layout.addLayout(row)
        self._framegen_note = QLabel()
        self._framegen_note.setWordWrap(True)
        self._framegen_note.setStyleSheet(info_text_style(gui_config))
        layout.addWidget(self._framegen_note)
        self._check_framegen()

        # Sizes
        row = QHBoxLayout()
        row.addWidget(QLabel(self.tr("Input", "Benchmark image size")))
        self._input_combo = QComboBox()
        self._input_combo.addItems(INPUT_SIZES)
        row.addWidget(self._input_combo)
        row.addWidget(QLabel(self.tr("Output", "Benchmark image size")))
        self._target_combo = QComboBox()
        self._target_combo.addItems(TARGET_SIZES)
        row.addWidget(self._target_combo)
        row.addStretch()
        layout.addLayout(row)
        sizes_note = QLabel(
            self.tr(
                "Effects read the input size and render scaling effects at the output "
                "size; frame generation runs at the output size.",
                "Benchmark dialog",
            )
        )
        sizes_note.setWordWrap(True)
        sizes_note.setStyleSheet(info_text_style(gui_config))
        layout.addWidget(sizes_note)

        self._warning = QLabel()
        self._warning.setWordWrap(True)
        self._warning.setStyleSheet("QLabel { color: #e0a030; }")
        layout.addWidget(self._warning)

        # Progress
        self._progress = QProgressBar()
        self._progress.setRange(0, 1)
        self._progress.setValue(0)
        layout.addWidget(self._progress)
        self._status = QLabel()
        self._status.setWordWrap(True)
        layout.addWidget(self._status)

        # Results
        self._table = QTableWidget(0, 4)
        self._table.setHorizontalHeaderLabels(
            [
                self.tr("Engine", "Benchmark table"),
                self.tr("Effect", "Benchmark table"),
                self.tr("ms/frame", "Benchmark table"),
                self.tr("p95", "Benchmark table"),
            ]
        )
        header = self._table.horizontalHeader()
        header.setSectionResizeMode(1, QHeaderView.Stretch)
        for col in (0, 2, 3):
            header.setSectionResizeMode(col, QHeaderView.ResizeToContents)
        self._table.verticalHeader().setVisible(False)
        self._table.setEditTriggers(QTableWidget.NoEditTriggers)
        self._table.setSelectionMode(QTableWidget.NoSelection)
        layout.addWidget(self._table, 1)

        # Buttons
        row = QHBoxLayout()
        row.addStretch()
        self._start_btn = QPushButton(self.tr("Start", "Benchmark button"))
        self._cancel_btn = QPushButton(self.tr("Cancel", "Benchmark button"))
        self._close_btn = QPushButton(self.tr("Close", "Close button"))
        self._start_btn.clicked.connect(self.start)
        self._cancel_btn.clicked.connect(self.cancel)
        self._close_btn.clicked.connect(self.reject)
        for b in (self._start_btn, self._cancel_btn, self._close_btn):
            row.addWidget(b)
        layout.addLayout(row)

        self._effects_cb.toggled.connect(self._update_state)
        self._framegen_cb.toggled.connect(self._update_state)

        self._session_timer = QTimer(self)
        self._session_timer.setInterval(_SESSION_POLL_MS)
        self._session_timer.timeout.connect(self._update_state)
        self._session_timer.start()

        self._show_stored()
        self._update_state()

    # ------------------------------------------------------------------
    # State
    # ------------------------------------------------------------------
    @property
    def running(self) -> bool:
        return self._process is not None

    @property
    def saved(self) -> bool:
        """True once a run stored new results."""
        return self._saved

    def _sections(self) -> List[str]:
        sections = []
        if self._effects_cb.isChecked():
            sections.append(bench.SECTION_EFFECTS)
        if self._framegen_cb.isChecked() and self._framegen_ok:
            sections.append(bench.SECTION_FRAMEGEN)
        return sections

    def _check_framegen(self) -> None:
        """Disable the frame generation section when it cannot run here."""
        from ...framegen import find_dll, native_available

        reason = None
        dll = find_dll(self._dll, native=True) if native_available() else None
        if not native_available():
            reason = self.tr(
                "The built-in frame generation is not built in this installation.",
                "Benchmark dialog",
            )
        elif dll is None:
            reason = self.tr(
                "Lossless.dll was not found: set it in the Presentation tab to "
                "measure frame generation.",
                "Benchmark dialog",
            )
        else:
            self._dll = str(dll)
        self._framegen_ok = reason is None
        self._framegen_cb.setEnabled(self._framegen_ok)
        if reason is not None:
            self._framegen_cb.setChecked(False)
        self._framegen_note.setText(reason or "")
        self._framegen_note.setVisible(reason is not None)

    def _update_state(self) -> None:
        running = self.running
        busy = bool(self._session_active()) and not running
        if busy:
            self._warning.setText(
                self.tr(
                    "An upscaling session (or daemon mode) is running. Stop it to "
                    "measure: both would share the GPU and the times would be wrong.",
                    "Benchmark dialog",
                )
            )
        self._warning.setVisible(busy)
        idle = not running
        self._start_btn.setEnabled(idle and not busy and bool(self._sections()))
        self._cancel_btn.setEnabled(running)
        for w in (self._effects_cb, self._input_combo, self._target_combo):
            w.setEnabled(idle)
        self._framegen_cb.setEnabled(idle and self._framegen_ok)

    # ------------------------------------------------------------------
    # Results table
    # ------------------------------------------------------------------
    def _engine_label(self, kind: str) -> str:
        return {
            bench.NATIVE: self.tr("Native", "Magpie effects engine"),
            bench.MAGPIEFX: self.tr("MagpieFX", "Magpie effects engine"),
            bench.FRAMEGEN: self.tr("Frame generation", "Benchmark section"),
        }.get(kind, kind)

    def _add_row(self, kind: str, name: str, value: Dict[str, Any]) -> None:
        row = self._table.rowCount()
        self._table.insertRow(row)
        self._table.setItem(row, 0, QTableWidgetItem(self._engine_label(kind)))
        self._table.setItem(row, 1, QTableWidgetItem(name))
        error = value.get("error")
        ms, p95 = value.get("ms"), value.get("p95")
        if error or ms is None:
            item = QTableWidgetItem(self.tr("failed", "Benchmark result"))
            item.setToolTip(str(error or ""))
            self._table.setItem(row, 2, item)
            self._table.setItem(row, 3, QTableWidgetItem(""))
        else:
            for col, v in ((2, ms), (3, p95)):
                item = QTableWidgetItem("" if v is None else f"{v:.3f}")
                item.setTextAlignment(Qt.AlignRight | Qt.AlignVCenter)
                self._table.setItem(row, col, item)
        self._table.scrollToBottom()

    def _show_stored(self) -> None:
        """Fill the table with the results stored for this GPU, if any."""
        entry = bench.results_for_gpu(self._gpu)
        if not entry:
            self._status.setText(
                self.tr("Nothing measured on this GPU yet.", "Benchmark dialog")
            )
            return
        for kind in (bench.NATIVE, bench.MAGPIEFX, bench.FRAMEGEN):
            for name, value in sorted((entry.get(kind) or {}).items()):
                self._add_row(kind, name, value)
        dates = sorted((entry.get("updated") or {}).values())
        self._status.setText(
            self.tr("Saved results for {0} ({1}).", "Benchmark dialog").format(
                entry.get("name", "?"), dates[-1][:10] if dates else "?"
            )
        )

    # ------------------------------------------------------------------
    # Process
    # ------------------------------------------------------------------
    def start(self) -> None:
        if self.running or self._session_active():
            self._update_state()
            return
        sections = self._sections()
        if not sections:
            return
        self._table.setRowCount(0)
        self._reader = EventReader()
        self._stderr = b""
        self._finished_event = None
        self._cancel_requested = False
        self._progress.setRange(0, 0)  # busy until the case count is known
        self._status.setText(self.tr("Starting…", "Benchmark dialog"))

        process = QProcess(self)
        env = QProcessEnvironment()
        for k, v in bench_environment(dict(os.environ)).items():
            env.insert(k, v)
        process.setProcessEnvironment(env)
        process.readyReadStandardOutput.connect(self._on_stdout)
        process.readyReadStandardError.connect(self._on_stderr)
        process.finished.connect(self._on_finished)
        process.errorOccurred.connect(self._on_process_error)
        self._process = process
        process.start(
            sys.executable,
            bench_command(
                sections,
                self._input_combo.currentText(),
                self._target_combo.currentText(),
                self._gpu,
                self._dll if bench.SECTION_FRAMEGEN in sections else None,
            ),
        )
        self._update_state()

    def cancel(self) -> None:
        """Ask the run to stop (it finishes the current frame and cleans up)."""
        process = self._process
        if process is None:
            return
        self._cancel_requested = True
        self._status.setText(self.tr("Cancelling…", "Benchmark dialog"))
        self._cancel_btn.setEnabled(False)
        process.terminate()
        QTimer.singleShot(_KILL_AFTER_MS, lambda p=process: self._kill_if_running(p))

    def _kill_if_running(self, process: QProcess) -> None:
        if process is self._process and process.state() != QProcess.NotRunning:
            process.kill()

    def _on_stdout(self) -> None:
        if self._process is None:
            return
        data = bytes(self._process.readAllStandardOutput())
        for event in self._reader.feed(data):
            self.handle_event(event)

    def _on_stderr(self) -> None:
        if self._process is None:
            return
        self._stderr = (self._stderr + bytes(self._process.readAllStandardError()))[-4000:]

    def handle_event(self, event: Dict[str, Any]) -> None:
        """Apply one event of the measuring process to the dialog."""
        kind = event.get("event")
        if kind == "start":
            self._progress.setRange(0, max(int(event.get("total", 0)), 1))
            self._progress.setValue(0)
            gpu = (event.get("gpu") or {}).get("name", "?")
            self._status.setText(self.tr("Measuring on {0}…", "Benchmark dialog").format(gpu))
        elif kind == "case":
            self._status.setText(
                self.tr("Measuring {0} ({1} of {2})…", "Benchmark dialog").format(
                    event.get("name", ""),
                    int(event.get("index", 0)) + 1,
                    event.get("total", "?"),
                )
            )
        elif kind == "result":
            self._progress.setValue(int(event.get("done", 0)))
            self._add_row(event.get("kind", ""), event.get("name", ""), event)
        elif kind in ("done", "cancelled"):
            self._finished_event = kind
            if int(event.get("count", 0)) > 0:
                self._saved = True
                self.results_saved.emit()
            if kind == "done":
                self._status.setText(self.tr("Done. Results saved.", "Benchmark dialog"))
            else:
                self._status.setText(
                    self.tr(
                        "Cancelled. The cases already measured were saved.",
                        "Benchmark dialog",
                    )
                )
        elif kind == "error":
            self._finished_event = kind
            self._status.setText(
                self.tr("The measurement failed: {0}", "Benchmark dialog").format(
                    event.get("error", "")
                )
            )

    def _on_finished(self, exit_code: int = 0, _status=None) -> None:
        if self._process is not None:
            self._on_stdout()
            self._on_stderr()
        process, self._process = self._process, None
        if process is not None:
            process.deleteLater()
        if self._finished_event is None and self._cancel_requested:
            # Stopped before it could report (for example while loading)
            self._status.setText(self.tr("Cancelled.", "Benchmark dialog"))
        elif self._finished_event is None:
            # Killed, or crashed before it could say anything
            tail = self._stderr.decode("utf-8", "replace").strip().splitlines()[-1:]
            self._status.setText(
                self.tr("The measurement stopped (code {0}). {1}", "Benchmark dialog").format(
                    exit_code, tail[0] if tail else ""
                )
            )
        self._progress.setRange(0, 1)
        self._progress.setValue(1 if self._finished_event == "done" else 0)
        self._update_state()

    def _on_process_error(self, error) -> None:
        if error == QProcess.FailedToStart:
            self._finished_event = "error"
            self._status.setText(
                self.tr("Cannot start the measurement.", "Benchmark dialog")
            )
            self._on_finished(-1)

    # ------------------------------------------------------------------
    # Closing
    # ------------------------------------------------------------------
    def reject(self) -> None:
        process = self._process
        if process is not None:
            # Give the run a moment to finish its frame and release lsfg
            self._cancel_requested = True
            process.terminate()
            if not process.waitForFinished(2000):
                process.kill()
                process.waitForFinished(1000)
            if self._process is process:  # finished was not delivered
                self._on_finished(process.exitCode())
        self._session_timer.stop()
        super().reject()


def _main_window_session_check(widget: Optional[QWidget]) -> Callable[[], bool]:
    """A callable telling whether the main window runs an upscaling session."""
    window = widget.window() if widget is not None else None

    def active() -> bool:
        if window is None:
            return False
        if getattr(window, "manual_session", None) is not None:
            return True
        daemon = getattr(window, "daemon_ctrl", None)
        return bool(getattr(daemon, "active", False))

    return active


def open_bench_dialog(
    gui_config: GUIConfig,
    config: Config,
    section: str = bench.SECTION_EFFECTS,
    parent: Optional[QWidget] = None,
) -> bool:
    """
    Open the measurement dialog on *section* (``bench.SECTION_EFFECTS`` or
    ``bench.SECTION_FRAMEGEN``) for the GPU and Lossless.dll of *config*.
    Returns True when new results were saved (refresh what shows them).
    """
    dialog = BenchDialog(
        gui_config,
        section=section,
        gpu=getattr(config, "gpu", None),
        dll=getattr(config, "frame_gen_dll", None),
        session_active=_main_window_session_check(parent),
        parent=parent,
    )
    dialog.exec()
    saved = dialog.saved
    dialog.deleteLater()
    return saved


def open_framegen_bench(
    gui_config: GUIConfig, config: Config, parent: Optional[QWidget] = None
) -> bool:
    """The dialog opened on the frame generation section (Presentation tab)."""
    return open_bench_dialog(gui_config, config, bench.SECTION_FRAMEGEN, parent)
