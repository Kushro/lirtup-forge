from __future__ import annotations

import copy
import os
from typing import Any, Dict, List, Optional, TYPE_CHECKING

from PySide6.QtCore import Qt, QTimer
from PySide6.QtWidgets import (
    QLabel,
    QPushButton,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from ..sidebar import SettingsTab
from ...dialogs.bench import open_bench_dialog
from ...styles import (
    info_text_style,
    path_browse_button_style,
    setting_label_style,
)
from .... import bench
from ....magpiefx import MagpieFXError, find_effect, list_effects, parse_effect
from ....magpiefx.effect import BUILTIN_EFFECTS_DIR
from ....magpiefx.spec import parse_effect_chain
from ....shaders import LUT_PRESETS

def _bundled_effect_count() -> int:
    """Number of MagpieFX effects shipped with the package."""
    return sum(
        f.endswith(".hlsl")
        for _, _, files in os.walk(BUILTIN_EFFECTS_DIR)
        for f in files
    )


# Reference GPU times (ms/frame) of the effects available in both engines:
# headless, 1280x720 input, 2560x1440 target, AMD RX 9070 XT (RADV), dxc.
# From scripts/bench-magpie-effects.py and scripts/magpiefx-bench.py. Shown
# until the user measures their own GPU (upscaler.bench).
_REFERENCE_MS = (
    ("LumaSharpen", 0.016, 0.016),
    ("Anime4K_Upscale_S", 0.145, 0.142),
    ("CRT_Easymode", 0.122, 0.118),
)


def _fmt_ms(value: Optional[float]) -> str:
    return "&ndash;" if value is None else f"{value:.3f}"


def _fmt_size(size: Optional[List[int]]) -> str:
    return f"{size[0]}x{size[1]}" if size and len(size) == 2 else "?"


if TYPE_CHECKING:
    from ...config import GUIConfig
    from ....config import Config


class EffectsTab(SettingsTab):
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
            title=self.tr("Effects", "Name of a settings tab"),
            config=config,
            baseline_config=baseline_config,
            global_baseline=global_baseline,
            parent=parent,
        )

    def _build_content(self) -> None:
        # ---- Debanding ----
        self._add_section(self.tr("Debanding", "Settings section"))
        self._add_cb(
            self.tr("Enable deband", "Label of setting (must be short)"),
            self._config.deband_enabled,
            self._on_deband_enabled,
            baseline=self.baseline_config.deband_enabled,
            field="deband_enabled",
            help=self.tr(
                "Reduce banding in smooth gradients before upscaling.\n"
                "Only useful if you notice banding in skies, fog, and other large smooth areas.",
                "Description of a setting (tooltip)",
            ),
        )
        self._deband_str = self._add_slider(
            self.tr("Strength", "Label of setting (must be short)"),
            0,
            100,
            int(self._config.deband_strength * 100),
            scale_factor=100,
            float_slot=self._on_deband_strength,
            baseline=self.baseline_config.deband_strength,
            field="deband_strength",
            help=self.tr(
                "Debanding intensity.\nRecommended range: 0.10 - 0.30.",
                "Description of a setting (tooltip)",
            ),
        )
        self._deband_str.setEnabled(self._config.deband_enabled)

        # ---- Magpie effects ----
        self._add_section(self.tr("Magpie effects", "Settings section"))
        self._engine_labels = {
            "native": self.tr("Native", "Magpie effects engine"),
            "magpiefx": self.tr("MagpieFX", "Magpie effects engine"),
        }
        self._add_combo(
            self.tr("Engine", "Label of setting (must be short)"),
            list(self._engine_labels.values()),
            self._engine_labels.get(self._config.effects_engine),
            self._on_effects_engine,
            baseline=self._engine_labels.get(self.baseline_config.effects_engine),
            field="effects_engine",
            help=self.tr(
                "Native: effects ported by hand, precompiled.\n"
                "MagpieFX: Magpie's .hlsl effects compiled at runtime.\n"
                "Only the selected engine runs.",
                "Description of a setting (tooltip)",
            ),
        )
        self._engine_info = QLabel()
        self._engine_info.setWordWrap(True)
        self._engine_info.setTextFormat(Qt.RichText)
        self._engine_info.setStyleSheet(info_text_style(self._gui_config))
        self._engine_info.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Minimum)
        self.content_layout.addWidget(self._engine_info)
        bench_btn = QPushButton(self.tr("Measure on my GPU", "Benchmark button"))
        bench_btn.setStyleSheet(path_browse_button_style(self._gui_config))
        bench_btn.setToolTip(
            self.tr(
                "Measure every effect on this GPU and show those times here.",
                "Description of a setting (tooltip)",
            )
        )
        bench_btn.clicked.connect(self._on_measure)
        self.content_layout.addWidget(bench_btn)

        native_start = self.content_layout.count()
        self._add_cb(
            self.tr("Enable Anime4K", "Label of setting (must be short)"),
            self._config.anime4k_enabled,
            self._on_anime4k_enabled,
            baseline=self.baseline_config.anime4k_enabled,
            field="anime4k_enabled",
            help=self.tr(
                "Anime4K_Upscale_S: small 2x CNN upscaler for anime and line art,\n"
                "applied after the AI upscaler.",
                "Description of a setting (tooltip)",
            ),
        )
        self._add_cb(
            self.tr("Enable LumaSharpen", "Label of setting (must be short)"),
            self._config.luma_sharpen_enabled,
            self._on_luma_sharpen_enabled,
            baseline=self.baseline_config.luma_sharpen_enabled,
            field="luma_sharpen_enabled",
            help=self.tr(
                "Unsharp mask on luma, with a clamp that limits halos.",
                "Description of a setting (tooltip)",
            ),
        )
        self._luma_sharpen_str = self._add_slider(
            self.tr("Strength", "Label of setting (must be short)"),
            0,
            300,
            int(self._config.luma_sharpen_strength * 100),
            scale_factor=100,
            float_slot=self._on_luma_sharpen_strength,
            baseline=self.baseline_config.luma_sharpen_strength,
            field="luma_sharpen_strength",
            help=self.tr(
                "LumaSharpen strength.\nDefault: 0.65.",
                "Description of a setting (tooltip)",
            ),
        )
        self._luma_sharpen_str.setEnabled(self._config.luma_sharpen_enabled)

        self._add_cb(
            self.tr("Enable CRT", "Label of setting (must be short)"),
            self._config.crt_enabled,
            self._on_crt_enabled,
            baseline=self.baseline_config.crt_enabled,
            field="crt_enabled",
            help=self.tr(
                "CRT-Easymode: scanlines and RGB mask. Replaces the final scaler.\n"
                "Scanlines only appear on low resolution sources.",
                "Description of a setting (tooltip)",
            ),
        )
        self._crt_mask_str = self._add_slider(
            self.tr("Mask strength", "Label of setting (must be short)"),
            0,
            100,
            int(self._config.crt_mask_strength * 100),
            scale_factor=100,
            float_slot=self._on_crt_mask_strength,
            baseline=self.baseline_config.crt_mask_strength,
            field="crt_mask_strength",
            help=self.tr(
                "Strength of the RGB mask.\nDefault: 0.30.",
                "Description of a setting (tooltip)",
            ),
        )
        self._crt_mask_str.setEnabled(self._config.crt_enabled)
        self._native_widgets = [
            self.content_layout.itemAt(i).widget()
            for i in range(native_start, self.content_layout.count())
        ]

        self._magpiefx_box = QWidget()
        self._magpiefx_box.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Minimum)
        box_layout = QVBoxLayout(self._magpiefx_box)
        box_layout.setContentsMargins(0, 0, 0, 0)
        box_layout.setSpacing(self.content_layout.spacing())
        self.content_layout.addWidget(self._magpiefx_box)
        self._owned_fields.add("magpie_effects")
        self._build_magpiefx_chain()
        self._update_engine_ui()

        # ---- CAS ----
        self._add_section(self.tr("CAS sharpening", "Settings section"))
        self._add_cb(
            self.tr("Enable CAS", "Label of setting (must be short)"),
            self._config.cas_enabled,
            self._on_cas_enabled,
            baseline=self.baseline_config.cas_enabled,
            field="cas_enabled",
            help=self.tr(
                "Contrast Adaptive Sharpening: enhances text and line art contrast.",
                "Description of a setting (tooltip)",
            ),
        )
        self._cas_str = self._add_slider(
            self.tr("Strength", "Label of setting (must be short)"),
            0,
            100,
            int(self._config.cas_strength * 100),
            scale_factor=100,
            float_slot=self._on_cas_strength,
            baseline=self.baseline_config.cas_strength,
            field="cas_strength",
            help=self.tr(
                "Sharpening amount.\nRecommended range: 0.20 - 0.50.",
                "Description of a setting (tooltip)",
            ),
        )
        self._cas_str.setEnabled(self._config.cas_enabled)

        # ---- Bloom ----
        self._add_section(self.tr("Bloom (glow)", "Settings section"))
        self._add_cb(
            self.tr("Enable bloom", "Label of setting (must be short)"),
            self._config.bloom_enabled,
            self._on_bloom_enabled,
            baseline=self.baseline_config.bloom_enabled,
            field="bloom_enabled",
            help=self.tr(
                "Soft glow around bright areas, for a cinematic look.\n"
                "May introduce halos, especially with white text.",
                "Description of a setting (tooltip)",
            ),
        )
        self._bloom_str = self._add_slider(
            self.tr("Strength", "Label of setting (must be short)"),
            0,
            100,
            int(self._config.bloom_strength * 100),
            scale_factor=100,
            float_slot=self._on_bloom_strength,
            baseline=self.baseline_config.bloom_strength,
            field="bloom_strength",
            help=self.tr(
                "Bloom intensity.\nRecommended range: 0.02 - 0.06.",
                "Description of a setting (tooltip)",
            ),
        )
        self._bloom_str.setEnabled(self._config.bloom_enabled)

        self._bloom_thresh = self._add_slider(
            self.tr("Threshold", "Label of setting (must be short)"),
            0,
            100,
            int(self._config.bloom_threshold * 100),
            scale_factor=100,
            float_slot=self._on_bloom_threshold,
            baseline=self.baseline_config.bloom_threshold,
            field="bloom_threshold",
            help=self.tr(
                "Brightness cutoff for bloom.\n"
                "Only pixels brighter than this value will glow.",
                "Description of a setting (tooltip)",
            ),
        )
        self._bloom_thresh.setEnabled(self._config.bloom_enabled)

        self._bloom_radius = self._add_slider(
            self.tr("Radius", "Label of setting (must be short)"),
            1,
            16,
            self._config.bloom_radius,
            self._on_bloom_radius,
            baseline=self.baseline_config.bloom_radius,
            field="bloom_radius",
            help=self.tr(
                "Blur radius in pixels. Larger radii spread the glow further.",
                "Description of a setting (tooltip)",
            ),
        )
        self._bloom_radius.setEnabled(self._config.bloom_enabled)

        # ---- Vignette ----
        self._add_section(self.tr("Vignette", "Settings section"))
        self._add_cb(
            self.tr("Enable vignette", "Label of setting (must be short)"),
            self._config.vignette_enabled,
            self._on_vignette_enabled,
            baseline=self.baseline_config.vignette_enabled,
            field="vignette_enabled",
            help=self.tr(
                "Radial darkening of screen edges, drawing focus to the center.",
                "Description of a setting (tooltip)",
            ),
        )
        self._vignette_str = self._add_slider(
            self.tr("Strength", "Label of setting (must be short)"),
            0,
            100,
            int(self._config.vignette_strength * 100),
            scale_factor=100,
            float_slot=self._on_vignette_strength,
            baseline=self.baseline_config.vignette_strength,
            field="vignette_strength",
            help=self.tr(
                "Edge darkening intensity.\nRecommended range: 0.30 - 0.60.",
                "Description of a setting (tooltip)",
            ),
        )
        self._vignette_str.setEnabled(self._config.vignette_enabled)

        self._vignette_radius = self._add_slider(
            self.tr("Radius", "Label of setting (must be short)"),
            0,
            200,
            int(self._config.vignette_radius * 100),
            scale_factor=100,
            float_slot=self._on_vignette_radius,
            baseline=self.baseline_config.vignette_radius,
            field="vignette_radius",
            help=self.tr(
                "Distance from center where darkening begins.\n"
                "Higher values keep the center brighter.",
                "Description of a setting (tooltip)",
            ),
        )
        self._vignette_radius.setEnabled(self._config.vignette_enabled)

        self._vignette_falloff = self._add_slider(
            self.tr("Falloff", "Label of setting (must be short)"),
            10,
            1000,
            int(self._config.vignette_falloff * 100),
            scale_factor=100,
            float_slot=self._on_vignette_falloff,
            baseline=self.baseline_config.vignette_falloff,
            field="vignette_falloff",
            help=self.tr(
                "Softness of the vignette transition. Low values = gentle, "
                "high values = sharp ring.",
                "Description of a setting (tooltip)",
            ),
        )
        self._vignette_falloff.setEnabled(self._config.vignette_enabled)

        # ---- Film Grain ----
        self._add_section(self.tr("Film grain", "Settings section"))
        self._add_cb(
            self.tr("Enable grain", "Label of setting (must be short)"),
            self._config.grain_enabled,
            self._on_grain_enabled,
            baseline=self.baseline_config.grain_enabled,
            field="grain_enabled",
            help=self.tr(
                "Simulated film grain look.",
                "Description of a setting (tooltip)",
            ),
        )
        self._grain_str = self._add_slider(
            self.tr("Strength", "Label of setting (must be short)"),
            0,
            100,
            int(self._config.grain_strength * 100),
            scale_factor=100,
            float_slot=self._on_grain_strength,
            baseline=self.baseline_config.grain_strength,
            field="grain_strength",
            help=self.tr(
                "Grain intensity.\nRecommended range: 0.10 - 0.20.",
                "Description of a setting (tooltip)",
            ),
        )
        self._grain_str.setEnabled(self._config.grain_enabled)

        self._grain_size = self._add_slider(
            self.tr("Size", "Label of setting (must be short)"),
            100,
            1000,
            int(self._config.grain_size * 100),
            scale_factor=100,
            float_slot=self._on_grain_size_changed,
            baseline=self.baseline_config.grain_size,
            field="grain_size",
            help=self.tr(
                "Particle size of the grain.\n"
                "Larger values produce coarser, more visible grain.",
                "Description of a setting (tooltip)",
            ),
        )
        self._grain_size.setEnabled(self._config.grain_enabled)

        # ---- Color Grading (LUT) ----
        self._add_section(self.tr("Color grading", "Settings section"))
        self._add_cb(
            self.tr("Enable LUT", "Label of setting (must be short)"),
            self._config.lut_enabled,
            self._on_lut_enabled,
            baseline=self.baseline_config.lut_enabled,
            field="lut_enabled",
            help=self.tr(
                "Apply a color grading preset (LUT) to change the look of the window.",
                "Description of a setting (tooltip)",
            ),
        )
        self._lut_combo = self._add_combo(
            self.tr("Preset", "Label of setting (must be short)"),
            list(LUT_PRESETS.keys()),
            self._config.lut_preset,
            self._on_lut_preset,
            baseline=self.baseline_config.lut_preset,
            field="lut_preset",
            help=self.tr(
                "Built-in color grading preset (warm, cool, film, sepia, etc.).",
                "Description of a setting (tooltip)",
            ),
        )
        self._lut_combo.setEnabled(self._config.lut_enabled)

        self._lut_intensity = self._add_slider(
            self.tr("Intensity", "Label of setting (must be short)"),
            0,
            100,
            int(self._config.lut_intensity * 100),
            scale_factor=100,
            float_slot=self._on_lut_intensity,
            baseline=self.baseline_config.lut_intensity,
            field="lut_intensity",
            help=self.tr(
                "Blend between original and graded image.",
                "Description of a setting (tooltip)",
            ),
        )
        self._lut_intensity.setEnabled(self._config.lut_enabled)

    def _on_deband_enabled(self, state: int):
        enabled = bool(state)
        self._config.deband_enabled = enabled
        self._deband_str.setEnabled(enabled)
        self.config_changed.emit()

    # ------------------------------------------------------------------
    #  Magpie effects engine
    # ------------------------------------------------------------------
    def _on_effects_engine(self, text: str):
        for engine, label in self._engine_labels.items():
            if label == text:
                self._config.effects_engine = engine
        self._update_engine_ui()
        self.config_changed.emit()

    def _update_engine_ui(self) -> None:
        native = self._config.effects_engine == "native"
        for w in self._native_widgets:
            w.setVisible(native)
        self._magpiefx_box.setVisible(not native)
        self._engine_info.setText(self._engine_info_html(native))

    def _engine_info_html(self, native: bool) -> str:
        if native:
            pros = [
                self.tr("Precompiled: loads instantly", "Magpie engine pro"),
                self.tr("Bounds-checked writes on every GPU", "Magpie engine pro"),
                self.tr("Tuned controls in this tab", "Magpie engine pro"),
            ]
            cons = [
                self.tr("Only 3 effects for now", "Magpie engine con"),
                self.tr("Each new effect is ported by hand", "Magpie engine con"),
            ]
        else:
            pros = [
                self.tr("Runs Magpie's .hlsl effects unmodified", "Magpie engine pro"),
                self.tr(
                    "{0} effects from Magpie, plus your own in "
                    "~/.config/linux-rt-upscaler/effects",
                    "Magpie engine pro",
                ).format(_bundled_effect_count()),
                self.tr("Controls generated from each effect", "Magpie engine pro"),
            ]
            cons = [
                self.tr(
                    "Compiles on first load (needs dxc or glslangValidator)",
                    "Magpie engine con",
                ),
                self.tr(
                    "A few effects need dxc (glslang cannot compile them)",
                    "Magpie engine con",
                ),
            ]
        items = "<br>".join([f"+ {p}" for p in pros] + [f"&minus; {c}" for c in cons])
        measured = self._measured_rows()
        rows = "".join(
            f"<tr><td>{name}</td><td align='right'>{_fmt_ms(a)}</td>"
            f"<td align='right'>{_fmt_ms(b)}</td></tr>"
            for name, a, b in (measured or _REFERENCE_MS)
        )
        head = (
            f"<tr><th align='left'>{self.tr('Effect', 'Benchmark table')}</th>"
            f"<th align='right'>{self._engine_labels['native']}</th>"
            f"<th align='right'>{self._engine_labels['magpiefx']}</th></tr>"
        )
        if measured:
            entry = self._bench_entry or {}
            sizes = (entry.get("sizes") or {}).get(bench.NATIVE) or {}
            note = self.tr(
                "GPU time in ms/frame measured on {0} ({1} to {2}).",
                "Benchmark table caption",
            ).format(
                entry.get("name", "?"),
                _fmt_size(sizes.get("input")),
                _fmt_size(sizes.get("target")),
            )
        else:
            note = self.tr(
                "Reference GPU time in ms/frame (720p to 1440p, RX 9070 XT).",
                "Benchmark table caption",
            )
        return (
            f"<p>{items}</p>"
            f"<table cellspacing='0' cellpadding='3'>{head}{rows}</table>"
            f"<p>{note}</p>"
        )

    # ------------------------------------------------------------------
    #  Measured times (upscaler.bench)
    # ------------------------------------------------------------------
    @property
    def _bench_entry(self) -> Optional[Dict[str, Any]]:
        """Stored results for the GPU of this config (read once, see _reload_bench)."""
        if not hasattr(self, "_bench_cache"):
            try:
                self._bench_cache = bench.results_for_gpu(self._config.gpu)
            except Exception:
                self._bench_cache = None
        return self._bench_cache

    def _reload_bench(self) -> None:
        if hasattr(self, "_bench_cache"):
            del self._bench_cache

    def _measured_rows(self) -> List[tuple]:
        """(name, native ms, MagpieFX ms) rows from the stored results, or []."""
        entry = self._bench_entry
        rows = [
            (
                name,
                bench.stored_ms(entry, bench.NATIVE, name),
                bench.stored_ms(entry, bench.MAGPIEFX, name),
            )
            for name in bench.NATIVE_EFFECTS
        ]
        if all(a is None and b is None for _, a, b in rows):
            return []
        return rows

    def _on_measure(self) -> None:
        if open_bench_dialog(self._gui_config, self._config, bench.SECTION_EFFECTS, self):
            self._reload_bench()
            self._update_engine_ui()
            QTimer.singleShot(0, self._build_magpiefx_chain)

    def _magpiefx_chain(self) -> List[Dict[str, Any]]:
        """The configured MagpieFX chain as a list of {name, params} dicts."""
        try:
            specs = parse_effect_chain(self._config.magpie_effects or [])
        except ValueError:
            specs = []
        return [{"name": n, "params": dict(p)} for n, p in specs]

    def _set_magpiefx_chain(self, chain: List[Dict[str, Any]]) -> None:
        # Assign a new list so the dirty check sees the change
        self._config.magpie_effects = copy.deepcopy(chain)
        self.config_changed.emit()

    def _build_magpiefx_chain(self) -> None:
        """(Re)create the MagpieFX chain editor inside its container."""
        box_layout = self._magpiefx_box.layout()
        while box_layout.count():
            w = box_layout.takeAt(0).widget()
            if w is not None:
                w.deleteLater()

        # Reuse the row helpers by pointing them at the container
        main_layout, self.content_layout = self.content_layout, box_layout
        try:
            placeholder = self.tr("Add effect…", "MagpieFX effect picker")
            self._add_combo(
                self.tr("Effects", "Label of setting (must be short)"),
                [placeholder] + list_effects(),
                placeholder,
                lambda name: name != placeholder and self._on_add_magpiefx(name),
                help=self.tr(
                    "Effects run in the order they are added, before scaling.",
                    "Description of a setting (tooltip)",
                ),
            )
            baseline_chain = parse_effect_chain(
                self.baseline_config.magpie_effects or []
            )
            for index, entry in enumerate(self._magpiefx_chain()):
                base = baseline_chain[index] if index < len(baseline_chain) else None
                if base is not None and base[0] != entry["name"]:
                    base = None
                self._add_magpiefx_entry(index, entry, base[1] if base else None)
        finally:
            self.content_layout = main_layout

    def _add_magpiefx_entry(
        self, index: int, entry: Dict[str, Any], baseline: Optional[Dict[str, float]]
    ) -> None:
        remove = QPushButton(self.tr("Remove", "MagpieFX effect button"))
        remove.setStyleSheet(path_browse_button_style(self._gui_config))
        remove.clicked.connect(lambda: self._on_remove_magpiefx(index))
        label = entry["name"]
        ms = bench.stored_ms(self._bench_entry, bench.MAGPIEFX, entry["name"])
        if ms is not None:
            label += f"  ({_fmt_ms(ms)} ms)"
        self._add_row(label, remove)
        name_label = self.content_layout.itemAt(
            self.content_layout.count() - 1
        ).layout().itemAt(0).widget()
        name_label.setStyleSheet(
            setting_label_style(self._gui_config, self._gui_config.palette.text)
            + "QLabel { font-weight: bold; }"
        )

        try:
            with open(find_effect(entry["name"]), encoding="utf-8") as f:
                desc = parse_effect(f.read(), entry["name"])
        except (OSError, MagpieFXError) as e:
            error = QLabel(str(e))
            error.setWordWrap(True)
            error.setStyleSheet(info_text_style(self._gui_config))
            self.content_layout.addWidget(error)
            return

        for p in desc.params:
            scale = 1
            if p.type != "int":
                step = p.step if p.step > 0 else 0.01
                while scale < 1000 and abs(step * scale - round(step * scale)) > 1e-6:
                    scale *= 10
            value = entry["params"].get(p.name, p.default)
            base_value = baseline.get(p.name, p.default) if baseline is not None else None
            self._add_slider(
                p.label or p.name,
                round(p.min * scale),
                round(p.max * scale),
                round(value * scale),
                scale_factor=scale,
                float_slot=lambda v, i=index, n=p.name: self._on_magpiefx_param(i, n, v),
                baseline=base_value,
                help=f"{p.name}: {p.min:g} – {p.max:g}, "
                + self.tr("default", "MagpieFX parameter tooltip")
                + f" {p.default:g}",
            )

    def _on_add_magpiefx(self, name: str) -> None:
        chain = self._magpiefx_chain()
        chain.append({"name": name, "params": {}})
        self._set_magpiefx_chain(chain)
        QTimer.singleShot(0, self._build_magpiefx_chain)

    def _on_remove_magpiefx(self, index: int) -> None:
        chain = self._magpiefx_chain()
        if 0 <= index < len(chain):
            del chain[index]
            self._set_magpiefx_chain(chain)
        QTimer.singleShot(0, self._build_magpiefx_chain)

    def _on_magpiefx_param(self, index: int, name: str, value: float) -> None:
        chain = self._magpiefx_chain()
        if 0 <= index < len(chain):
            chain[index]["params"][name] = value
            self._set_magpiefx_chain(chain)

    def _on_anime4k_enabled(self, state: int):
        self._config.anime4k_enabled = bool(state)
        self.config_changed.emit()

    def _on_luma_sharpen_enabled(self, state: int):
        enabled = bool(state)
        self._config.luma_sharpen_enabled = enabled
        self._luma_sharpen_str.setEnabled(enabled)
        self.config_changed.emit()

    def _on_crt_enabled(self, state: int):
        enabled = bool(state)
        self._config.crt_enabled = enabled
        self._crt_mask_str.setEnabled(enabled)
        self.config_changed.emit()

    def _on_cas_enabled(self, state: int):
        enabled = bool(state)
        self._config.cas_enabled = enabled
        self._cas_str.setEnabled(enabled)
        self.config_changed.emit()

    def _on_bloom_enabled(self, state: int):
        enabled = bool(state)
        self._config.bloom_enabled = enabled
        self._bloom_str.setEnabled(enabled)
        self._bloom_thresh.setEnabled(enabled)
        self._bloom_radius.setEnabled(enabled)
        self.config_changed.emit()

    def _on_vignette_enabled(self, state: int):
        enabled = bool(state)
        self._config.vignette_enabled = enabled
        self._vignette_str.setEnabled(enabled)
        self._vignette_radius.setEnabled(enabled)
        self._vignette_falloff.setEnabled(enabled)
        self.config_changed.emit()

    def _on_lut_enabled(self, state: int):
        enabled = bool(state)
        self._config.lut_enabled = enabled
        self._lut_combo.setEnabled(enabled)
        self._lut_intensity.setEnabled(enabled)
        self.config_changed.emit()

    def _on_grain_enabled(self, state: int):
        enabled = bool(state)
        self._config.grain_enabled = enabled
        self._grain_str.setEnabled(enabled)
        self._grain_size.setEnabled(enabled)
        self.config_changed.emit()

    def _on_deband_strength(self, value: float):
        self._config.deband_strength = value
        self.config_changed.emit()

    def _on_luma_sharpen_strength(self, value: float):
        self._config.luma_sharpen_strength = value
        self.config_changed.emit()

    def _on_crt_mask_strength(self, value: float):
        self._config.crt_mask_strength = value
        self.config_changed.emit()

    def _on_cas_strength(self, value: float):
        self._config.cas_strength = value
        self.config_changed.emit()

    def _on_bloom_strength(self, value: float):
        self._config.bloom_strength = value
        self.config_changed.emit()

    def _on_bloom_threshold(self, value: float):
        self._config.bloom_threshold = value
        self.config_changed.emit()

    def _on_bloom_radius(self, value: int):
        self._config.bloom_radius = value
        self.config_changed.emit()

    def _on_vignette_strength(self, value: float):
        self._config.vignette_strength = value
        self.config_changed.emit()

    def _on_vignette_radius(self, value: float):
        self._config.vignette_radius = value
        self.config_changed.emit()

    def _on_vignette_falloff(self, value: float):
        self._config.vignette_falloff = value
        self.config_changed.emit()

    def _on_lut_preset(self, text: str):
        self._config.lut_preset = text
        self.config_changed.emit()

    def _on_lut_intensity(self, value: float):
        self._config.lut_intensity = value
        self.config_changed.emit()

    def _on_grain_strength(self, value: float):
        self._config.grain_strength = value
        self.config_changed.emit()

    def _on_grain_size_changed(self, value: float):
        self._config.grain_size = value
        self.config_changed.emit()
