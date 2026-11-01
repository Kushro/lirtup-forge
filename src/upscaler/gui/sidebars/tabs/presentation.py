from __future__ import annotations

from typing import Optional, TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QLabel, QPushButton, QSizePolicy, QWidget

from ..sidebar import SettingsTab
from ...dialogs import open_framegen_bench
from ...styles import info_text_style, path_browse_button_style
from ...utils import normalize_to_hex
from .... import bench, framegen
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
        self._add_cb(
            self.tr("Detect menu bar", "Label of setting (must be short)"),
            self._config.auto_crop_menu_bar,
            self._on_auto_crop_menu_bar,
            baseline=self.baseline_config.auto_crop_menu_bar,
            field="auto_crop_menu_bar",
            help=self.tr(
                "Experimental. Look for the menu bar that Windows programs "
                "draw below the title bar (under Wine) during the first "
                "seconds, and crop it on top of the Top value.",
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
        self._fg_labels[framegen.AUTO] = self.tr(
            "Auto", "Frame generation multiplier"
        )
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
                "Auto (built-in engine only) picks ×2 to ×5 from the game's "
                "frame rate so the output fits the display refresh, and "
                "changes it when the frame rate stays different for a few "
                "seconds.\n"
                "Read at startup: restart to apply.",
                "Description of a setting (tooltip)",
            ),
        )
        self._fg_engine_labels = {
            "layer": self.tr("lsfg-vk layer", "Frame generation engine"),
            "native": self.tr("Built in (experimental)", "Frame generation engine"),
        }
        self._fg_engine = self._add_combo(
            self.tr("Engine", "Label of setting (must be short)"),
            list(self._fg_engine_labels.values()),
            self._fg_engine_labels.get(self._config.frame_gen_engine),
            self._on_fg_engine,
            baseline=self._fg_engine_labels.get(
                self.baseline_config.frame_gen_engine
            ),
            field="frame_gen_engine",
            help=self.tr(
                "lsfg-vk layer: the installed lsfg-vk Vulkan layer generates "
                "frames over the upscaler's window (needs lsfg-vk.dll).\n"
                "Built in: the upscaler runs the lsfg core itself on the same "
                "GPU and paces the frames (needs Lossless.dll; no layer).\n"
                "With heavy effects (such as the Anime4K UL ones) the layer "
                "keeps a higher frame rate: the built-in engine shares the "
                "GPU time of each frame with them.\n"
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
                "Resolution at which lsfg estimates the motion between two "
                "frames, as a share of the output resolution.\n"
                "1.00 (default): full resolution, the most accurate motion "
                "and the heaviest for the GPU.\n"
                "Lower values are lighter and less precise on small or fast "
                "details; 0.50 to 0.75 is usually enough at 1440p and 4K.",
                "Description of a setting (tooltip)",
            ),
        )
        # What the chosen value means, updated as the slider moves
        self._fg_flow_note = QLabel()
        self._fg_flow_note.setWordWrap(True)
        self._fg_flow_note.setStyleSheet(info_text_style(self._gui_config))
        self._fg_flow_note.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Minimum)
        self.content_layout.addWidget(self._fg_flow_note)
        self._fg_model_labels = {
            False: self.tr("Quality (LSFG 3.1)", "Frame generation model"),
            True: self.tr("Performance (LSFG 3.1P)", "Frame generation model"),
        }
        self._fg_performance = self._add_combo(
            self.tr("Model", "Label of setting (must be short)"),
            list(self._fg_model_labels.values()),
            self._fg_model_labels[bool(self._config.frame_gen_performance)],
            self._on_fg_model,
            baseline=self._fg_model_labels[
                bool(self.baseline_config.frame_gen_performance)
            ],
            field="frame_gen_performance",
            help=self.tr(
                "The two models lsfg has.\n"
                "Quality (default): the full model, smoother results.\n"
                "Performance: a lighter model, faster on the GPU, with more "
                "artifacts around edges and fast motion.",
                "Description of a setting (tooltip)",
            ),
        )
        # Cost of the chosen setting on this GPU, from "Measure on my GPU"
        self._fg_cost = QLabel()
        self._fg_cost.setWordWrap(True)
        self._fg_cost.setStyleSheet(info_text_style(self._gui_config))
        self._fg_cost.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Minimum)
        self.content_layout.addWidget(self._fg_cost)
        fg_bench = QPushButton(self.tr("Measure on my GPU", "Benchmark button"))
        fg_bench.setStyleSheet(path_browse_button_style(self._gui_config))
        fg_bench.setToolTip(
            self.tr(
                "Measure what frame generation costs on this GPU (×2 to ×4, "
                "flow scale 1.0 and 0.5, both models).",
                "Description of a setting (tooltip)",
            )
        )
        fg_bench.clicked.connect(self._on_fg_bench)
        self.content_layout.addWidget(fg_bench)
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

    def _on_auto_crop_menu_bar(self, state: int):
        self._config.auto_crop_menu_bar = bool(state)
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

    def _on_fg_engine(self, text: str) -> None:
        for engine, label in self._fg_engine_labels.items():
            if label == text:
                self._config.frame_gen_engine = engine
        self._update_fg_ui()
        self.config_changed.emit()

    def _on_fg_flow_scale(self, value: float) -> None:
        self._config.frame_gen_flow_scale = round(value, 2)
        self._update_fg_ui()
        self.config_changed.emit()

    def _on_fg_model(self, text: str) -> None:
        for performance, label in self._fg_model_labels.items():
            if label == text:
                self._config.frame_gen_performance = performance
        self._update_fg_ui()
        self.config_changed.emit()

    def _fg_flow_text(self, scale: float) -> str:
        """What a flow scale value means, in one line."""
        if scale >= 0.99:
            text = self.tr(
                "Maximum: motion estimated at {0}% of the output resolution. "
                "The most accurate and the heaviest for lsfg (default).",
                "Frame generation flow scale note; {0} is a percentage",
            )
        elif scale >= 0.75:
            text = self.tr(
                "High: motion estimated at {0}% of the output resolution. "
                "Close to the maximum, somewhat lighter.",
                "Frame generation flow scale note; {0} is a percentage",
            )
        elif scale >= 0.5:
            text = self.tr(
                "Balanced: motion estimated at {0}% of the output resolution. "
                "Much lighter; small details move a bit less precisely. "
                "Good at 1440p and 4K.",
                "Frame generation flow scale note; {0} is a percentage",
            )
        else:
            text = self.tr(
                "Low: motion estimated at {0}% of the output resolution. "
                "The lightest, with visible artifacts on fine or fast detail.",
                "Frame generation flow scale note; {0} is a percentage",
            )
        return text.format(int(round(scale * 100)))

    def _update_fg_ui(self) -> None:
        on = framegen.multiplier_on(self._config.frame_gen_multiplier)
        self._fg_engine.setEnabled(on)
        self._fg_flow.setEnabled(on)
        self._fg_flow_note.setEnabled(on)
        self._fg_flow_note.setText(self._fg_flow_text(self._config.frame_gen_flow_scale))
        self._fg_performance.setEnabled(on)
        self._fg_cost.setText(self._fg_cost_text())
        self._fg_info.setText(self._fg_info_html(on))

    def _on_fg_bench(self) -> None:
        if open_framegen_bench(self._gui_config, self._config, self):
            self._update_fg_ui()

    def _fg_cost_text(self) -> str:
        """Measured GPU cost of the chosen setting, or a hint to measure it."""
        c = self._config
        entry = bench.results_for_gpu(c.gpu)
        if c.frame_gen_multiplier == "auto":
            multipliers = [2, 3, 4]
        elif framegen.multiplier_on(c.frame_gen_multiplier):
            multipliers = [int(c.frame_gen_multiplier)]
        else:
            return ""
        times = [
            bench.stored_framegen_ms(
                entry, m, c.frame_gen_flow_scale, bool(c.frame_gen_performance)
            )
            for m in multipliers
        ]
        if any(t is None for t in times):
            return self.tr(
                "Not measured on this GPU with this flow scale and model yet.",
                "Frame generation cost",
            )
        if len(times) == 1:
            return self.tr(
                "Measured: {0:.1f} ms per real frame on this GPU.",
                "Frame generation cost",
            ).format(times[0])
        return self.tr(
            "Measured: {0:.1f} to {1:.1f} ms per real frame on this GPU (×2 to ×4).",
            "Frame generation cost",
        ).format(min(times), max(times))

    def _fg_status_text(self, on: bool) -> str:
        """One line on what runs now and what the selection needs."""
        status = framegen.status()
        if status == framegen.FrameGenStatus.EXTERNAL:
            return self.tr(
                "lsfg-vk is set up by environment variables (LSFGVK_*); "
                "this setting is ignored.",
                "Frame generation status",
            )
        native = self._config.frame_gen_engine == "native"
        if on and native and not framegen.native_available():
            return self.tr(
                "The built-in engine is not part of this installation.",
                "Frame generation status",
            )
        if (
            on
            and native
            and framegen.find_dll(self._config.frame_gen_dll, native=True) is None
        ):
            return self.tr(
                "Lossless.dll was not found. Install Lossless Scaling from "
                "Steam, or set the path of Lossless.dll.",
                "Frame generation status",
            )
        if on and not native and framegen.find_layer() is None:
            return self.tr(
                "lsfg-vk is not installed.", "Frame generation status"
            )
        if (
            on
            and not native
            and framegen.find_dll(self._config.frame_gen_dll) is None
        ):
            return self.tr(
                "lsfg-vk.dll was not found. In Steam, switch Lossless Scaling "
                "to the 'lsfg-vk' branch (Properties, Betas).",
                "Frame generation status",
            )
        auto = self._config.frame_gen_multiplier == framegen.AUTO
        auto_note = ""
        if on and auto and not native:
            auto_note = self.tr(
                "Auto needs the built-in engine; the lsfg-vk layer runs at ×{0}.",
                "Frame generation status",
            ).format(framegen.AUTO_START_MULTIPLIER)
        running_setting = framegen.active_setting()
        running = framegen.active_multiplier()
        if running_setting != self._config.frame_gen_multiplier or (
            on and running > 1 and framegen.engine() != self._config.frame_gen_engine
        ):
            if running_setting == framegen.AUTO:
                now = self.tr(
                    "Running with the automatic multiplier.",
                    "Frame generation status",
                )
            elif running > 1:
                now = self.tr("Running at ×{0}.", "Frame generation status")
                now = now.format(running)
            else:
                now = self.tr("Not running.", "Frame generation status")
            restart = self.tr(
                "Save and restart LirtUp Forge to apply.",
                "Frame generation status",
            )
            return " ".join(t for t in (now, restart, auto_note) if t)
        if running_setting == framegen.AUTO:
            text = self.tr(
                "Running with the automatic multiplier. Flow scale and model "
                "apply after a restart.",
                "Frame generation status",
            )
            return " ".join(t for t in (text, auto_note) if t)
        if running > 1:
            return self.tr(
                "Running at ×{0}. Flow scale and model apply after a "
                "restart.",
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
