from __future__ import annotations

from typing import Optional, TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QLabel, QSizePolicy, QWidget

from ..sidebar import SettingsTab
from ...styles import info_text_style
from ...utils import normalize_to_hex
from .... import framegen
from ....config import OverlayMode

if TYPE_CHECKING:
    from ...config import GUIConfig
    from ....config import Config


class PresentationTab(SettingsTab):
    def __init__(
        self,
        gui_config: GUIConfig,
        config: Config,
        baseline_config: Config,
        global_baseline: Config,
        parent: Optional[QWidget] = None,
    ) -> None:
        super().__init__(
            gui_config,
            title=self.tr("Presentation", "Name of a settings tab"),
            config=config,
            baseline_config=baseline_config,
            global_baseline=global_baseline,
            parent=parent,
        )

    def _build_content(self) -> None:
        # ---- Overlay ----
        self._add_section(self.tr("Overlay", "Settings section"))
        self._add_combo(
            self.tr("Overlay mode", "Label of setting (must be short)"),
            [e.value for e in OverlayMode],
            self._config.overlay_mode,
            self._on_overlay_mode,
            baseline=self.baseline_config.overlay_mode,
            field="overlay_mode",
            help=self.tr(
                "Overlay window behaviour:\n"
                "• always-on-top: always visible above other windows, keeps focus on the target window (recommended)\n"
                "• top-transparent: same as always-on-top, but click-through (mouse passes to window below)\n"
                "• fullscreen: covers entire monitor, keyboard may not reach the target window\n"
                "• windowed: normal window with decorations, keyboard may not reach the target window",
                "Description of a setting (tooltip). "
                "Do not translate 'always-on-top', 'top-transparent', 'fullscreen', 'windowed': "
                "they are internal overlay mode identifiers.",
            ),
        )
        self._add_combo(
            self.tr("Output geometry", "Label of setting (must be short)"),
            ["fit", "stretch", "cover"],
            self._config.output_geometry,
            self._on_geometry_changed,
            baseline=self.baseline_config.output_geometry,
            field="output_geometry",
            help=self.tr(
                "How the upscaled content fits the overlay:\n"
                "• fit: show the entire image, adding black bars if necessary\n"
                "• stretch: fill the whole area, aspect ratio may be distorted\n"
                "• cover: fill the whole area and crop any excess",
                "Description of a setting (tooltip). "
                "Do not translate 'fit', 'stretch', 'cover': "
                "they are internal output geometry identifiers.",
            ),
        )

        # ---- Cursor ----
        self._add_section(self.tr("Cursor", "Settings section"))
        self._add_cb(
            self.tr("Hide cursor", "Label of setting (must be short)"),
            self._config.hide_cursor is not None,
            self._on_hide_cursor_toggle,
            baseline=self.baseline_config.hide_cursor is not None,
            field="hide_cursor",
            help=self.tr(
                "Automatically hide the mouse cursor after a period of inactivity.",
                "Description of a setting (tooltip)",
            ),
        )
        bl_ms = self.baseline_config.hide_cursor
        if bl_ms is not None and bl_ms > 0:
            bl_seconds = bl_ms / 1000.0
        elif bl_ms == 0:
            bl_seconds = 0.0
        else:
            bl_seconds = 2.0
        self._hide_cursor_timeout = self._add_slider(
            self.tr("Hide timeout (s)", "Label of setting (must be short)"),
            0,
            10000,
            self._config.hide_cursor if self._config.hide_cursor is not None else 2000,
            scale_factor=1000,
            float_slot=self._on_hide_cursor_timeout,
            baseline=bl_seconds,
            field="hide_cursor",
            help=self.tr(
                "Time in seconds after which the cursor disappears.\n"
                "Set to 0.00 to always hide the cursor.",
                "Description of a setting (tooltip)",
            ),
        )
        self._hide_cursor_timeout.setEnabled(self._config.hide_cursor is not None)

        # ---- Crop ----
        self._add_section(self.tr("Crop", "Settings section"))
        self._add_slider(
            self.tr("Top", "Crop border label"),
            0,
            200,
            self._config.crop_top,
            self._on_crop_top,
            baseline=self.baseline_config.crop_top,
            field="crop_top",
            help=self.tr(
                "Pixels to crop from the top border of the target window.",
                "Description of a setting (tooltip)",
            ),
        )
        self._add_slider(
            self.tr("Bottom", "Crop border label"),
            0,
            200,
            self._config.crop_bottom,
            self._on_crop_bottom,
            baseline=self.baseline_config.crop_bottom,
            field="crop_bottom",
            help=self.tr(
                "Pixels to crop from the bottom border of the target window.",
                "Description of a setting (tooltip)",
            ),
        )
        self._add_slider(
            self.tr("Left", "Crop border label"),
            0,
            200,
            self._config.crop_left,
            self._on_crop_left,
            baseline=self.baseline_config.crop_left,
            field="crop_left",
            help=self.tr(
                "Pixels to crop from the left border of the target window.",
                "Description of a setting (tooltip)",
            ),
        )
        self._add_slider(
            self.tr("Right", "Crop border label"),
            0,
            200,
            self._config.crop_right,
            self._on_crop_right,
            baseline=self.baseline_config.crop_right,
            field="crop_right",
            help=self.tr(
                "Pixels to crop from the right border of the target window.",
                "Description of a setting (tooltip)",
            ),
        )

        # ---- Offsets ----
        self._add_section(self.tr("Offset", "Settings section"))
        for label, field, slot in [
            (
                self.tr("X offset", "Label of setting (must be short)"),
                "offset_x",
                self._on_offset_x,
            ),
            (
                self.tr("Y offset", "Label of setting (must be short)"),
                "offset_y",
                self._on_offset_y,
            ),
        ]:
            self._add_slider(
                label,
                -200,
                200,
                getattr(self._config, field),
                slot,
                baseline=getattr(self.baseline_config, field),
                field=field,
                help=(
                    self.tr(
                        "Horizontal offset in pixels (positive moves right, negative moves left).",
                        "Description of a setting (tooltip)",
                    )
                    if field == "offset_x"
                    else self.tr(
                        "Vertical offset in pixels (positive moves down, negative moves up).",
                        "Description of a setting (tooltip)",
                    )
                ),
            )

        # ---- Background Color ----
        self._add_section(self.tr("Background color", "Settings section"))
        bg = normalize_to_hex(self._config.background_color)
        baseline_bg = normalize_to_hex(self.baseline_config.background_color)
        self._add_color_picker(
            self.tr("Color", "Label of setting (must be short)"),
            bg,
            self._on_bg_color,
            baseline=baseline_bg,
            field="background_color",
            help=self.tr(
                "Background color behind the upscaled image (letterbox bars).\n"
                "Supports transparency.",
                "Description of a setting (tooltip)",
            ),
        )

        # ---- Frame generation ----
        self._add_section(self.tr("Frame generation", "Settings section"))
        self._fg_labels = {1: self.tr("Off", "Frame generation multiplier")}
        for m in range(2, 6):
            self._fg_labels[m] = f"\u00d7{m}"
        self._add_combo(
            self.tr("Multiplier", "Label of setting (must be short)"),
            list(self._fg_labels.values()),
            self._fg_labels.get(self._config.frame_gen_multiplier),
            self._on_fg_multiplier,
            baseline=self._fg_labels.get(self.baseline_config.frame_gen_multiplier),
            field="frame_gen_multiplier",
            help=self.tr(
                "Generate frames between the captured ones with lsfg-vk "
                "(Lossless Scaling Frame Generation).\n"
                "×4 turns 30 captured fps into 120 shown fps.\n"
                "Read at startup: restart to apply.",
                "Description of a setting (tooltip)",
            ),
        )
        self._fg_flow = self._add_slider(
            self.tr("Flow scale", "Label of setting (must be short)"),
            25,
            100,
            int(round(self._config.frame_gen_flow_scale * 100)),
            scale_factor=100,
            float_slot=self._on_fg_flow_scale,
            baseline=self.baseline_config.frame_gen_flow_scale,
            field="frame_gen_flow_scale",
            help=self.tr(
                "Resolution of the motion estimation. Lower is faster and "
                "less accurate; useful at 1440p and 4K.",
                "Description of a setting (tooltip)",
            ),
        )
        self._fg_performance = self._add_cb(
            self.tr("Performance mode", "Label of setting (must be short)"),
            self._config.frame_gen_performance,
            self._on_fg_performance,
            baseline=self.baseline_config.frame_gen_performance,
            field="frame_gen_performance",
            help=self.tr(
                "Use the lighter lsfg-vk model.",
                "Description of a setting (tooltip)",
            ),
        )
        self._fg_info = QLabel()
        self._fg_info.setWordWrap(True)
        self._fg_info.setTextFormat(Qt.RichText)
        self._fg_info.setStyleSheet(info_text_style(self._gui_config))
        self._fg_info.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Minimum)
        self.content_layout.addWidget(self._fg_info)
        self._update_fg_ui()

    def _on_overlay_mode(self, text: str):
        self._config.overlay_mode = text
        self.config_changed.emit()

    def _on_hide_cursor_toggle(self, state: int) -> None:
        enabled = bool(state)
        self._hide_cursor_timeout.setEnabled(enabled)
        if enabled:
            self._config.hide_cursor = self._hide_cursor_timeout.value()
        else:
            self._config.hide_cursor = None
        self.config_changed.emit()

    def _on_hide_cursor_timeout(self, value: int) -> None:
        if self._hide_cursor_timeout.isEnabled():
            self._config.hide_cursor = int(value * 1000)
            self.config_changed.emit()

    def _on_geometry_changed(self, text: str) -> None:
        self._config.output_geometry = text
        self.config_changed.emit()

    def _on_crop_top(self, value: int):
        self._config.crop_top = value
        self.config_changed.emit()

    def _on_crop_bottom(self, value: int):
        self._config.crop_bottom = value
        self.config_changed.emit()

    def _on_crop_left(self, value: int):
        self._config.crop_left = value
        self.config_changed.emit()

    def _on_crop_right(self, value: int):
        self._config.crop_right = value
        self.config_changed.emit()

    def _on_offset_x(self, value: int):
        self._config.offset_x = value
        self.config_changed.emit()

    def _on_offset_y(self, value: int):
        self._config.offset_y = value
        self.config_changed.emit()

    def _on_bg_color(self, text: str):
        self._config.background_color = text
        self.config_changed.emit()

    # ------------------------------------------------------------------
    #  Frame generation
    # ------------------------------------------------------------------
    def _on_fg_multiplier(self, text: str) -> None:
        for m, label in self._fg_labels.items():
            if label == text:
                self._config.frame_gen_multiplier = m
        self._update_fg_ui()
        self.config_changed.emit()

    def _on_fg_flow_scale(self, value: float) -> None:
        self._config.frame_gen_flow_scale = round(value, 2)
        self._update_fg_ui()
        self.config_changed.emit()

    def _on_fg_performance(self, state: int) -> None:
        self._config.frame_gen_performance = bool(state)
        self._update_fg_ui()
        self.config_changed.emit()

    def _update_fg_ui(self) -> None:
        on = self._config.frame_gen_multiplier > 1
        self._fg_flow.setEnabled(on)
        self._fg_performance.setEnabled(on)
        self._fg_info.setText(self._fg_info_html(on))

    def _fg_status_text(self, on: bool) -> str:
        """One line on what runs now and what the selection needs."""
        status = framegen.status()
        if status == framegen.FrameGenStatus.EXTERNAL:
            return self.tr(
                "lsfg-vk is set up by environment variables (LSFGVK_*); "
                "this setting is ignored.",
                "Frame generation status",
            )
        if on and framegen.find_layer() is None:
            return self.tr(
                "lsfg-vk is not installed.", "Frame generation status"
            )
        if on and framegen.find_dll(self._config.frame_gen_dll) is None:
            return self.tr(
                "lsfg-vk.dll was not found. In Steam, switch Lossless Scaling "
                "to the 'lsfg-vk' branch (Properties, Betas).",
                "Frame generation status",
            )
        running = framegen.active_multiplier()
        if running != self._config.frame_gen_multiplier:
            if running > 1:
                now = self.tr("Running at ×{0}.", "Frame generation status")
                now = now.format(running)
            else:
                now = self.tr("Not running.", "Frame generation status")
            restart = self.tr(
                "Save and restart LirtUp Forge to apply.",
                "Frame generation status",
            )
            return f"{now} {restart}"
        if running > 1:
            return self.tr(
                "Running at ×{0}. Flow scale and performance mode apply "
                "after a restart.",
                "Frame generation status",
            ).format(running)
        return ""

    def _fg_info_html(self, on: bool) -> str:
        items = [
            "+ "
            + self.tr(
                "Smoother motion: shows up to N times the captured frame rate",
                "Frame generation pro",
            ),
            "&minus; "
            + self.tr(
                "Adds about one captured frame of latency "
                "(33 ms at 30 fps, 17 ms at 60 fps)",
                "Frame generation con",
            ),
            "&minus; "
            + self.tr(
                "Generated frames can smear HUD edges and fast motion",
                "Frame generation con",
            ),
            "&minus; "
            + self.tr(
                "Global setting: profiles cannot change it",
                "Frame generation con",
            ),
        ]
        status = self._fg_status_text(on)
        status_html = f"<p><b>{status}</b></p>" if status else ""
        return f"{status_html}<p>{'<br>'.join(items)}</p>"
