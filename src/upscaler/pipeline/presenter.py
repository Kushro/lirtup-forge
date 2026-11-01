from __future__ import annotations

import logging
import os
import time
from typing import Dict, List, Optional, Tuple, TYPE_CHECKING

from ..shaders import (
    Anime4KUpscaleS,
    Bloom,
    CAS,
    CatmullRomScaler,
    Clear,
    CopyScaler,
    CRTEasymode,
    Deband,
    Delinearize,
    DitherCopy,
    FilmGrain,
    FSRScaler,
    LanczosScaler,
    Linearize,
    LumaSharpen,
    LUT,
    NISScaler,
    Vignette,
)
from ..magpiefx import MagpieEffect, MagpieFXError
from ..magpiefx.spec import EffectSpec, parse_effect_chain
from ..utils import calculate_scaling_rect
from ..vulkan import Texture2D, R16G16B16A16_FLOAT
from .stats import StatsHUD

if TYPE_CHECKING:
    from .osd import OSDManager
    from .swapchain import SwapchainManager
    from ..config import Config
    from ..shaders import Scaler

logger = logging.getLogger(__name__)

# Set UPSCALER_EFFECT_TIMING=1 to log the GPU time of the Magpie effects
# (Vulkan timestamp queries), averaged every `_TIMING_WINDOW` frames.
_EFFECT_TIMING = os.environ.get("UPSCALER_EFFECT_TIMING", "") not in ("", "0")
_TIMING_WINDOW = 120


class Presenter:
    """
    Manages the screen texture and all post-processing passes.

    The processing chain (executed in :meth:`present`):
        1. Optional debanding (uses a temporary texture).
        1b. Optional Magpie effects, from the engine chosen with
            ``effects_engine``: "native" runs the hand-ported passes
            (Anime4K (2x) -> LumaSharpen -> CRT-Easymode, which renders at
            the destination size), "magpiefx" runs the MagpieFX chain
            (each effect owns its output texture).
        2. Lanczos scaling.
        3. Optional contrast adaptive sharpening (CAS).
        4. Optional bloom.
        5. Optional vignette.
        6. Optional color grading (3D LUT).
        7. Optional film grain.
        8. OSD blending.
        9. Swapchain presentation.

    All passes that operate on the final screen image (CAS through grain) work
    in-place on `self.screen_tex`. Debanding uses a dedicated intermediate
    texture because read-after-write would be unsafe.

    Effect parameters are taken directly from the :class:`Config` instance.
    """

    def __init__(
        self,
        screen_width: int,
        screen_height: int,
        content_width: int,
        content_height: int,
        scale_mode: str,
        config: Config,
        osd_manager: OSDManager,
        swapchain_manager: SwapchainManager,
        stats_hud: Optional[StatsHUD] = None,
    ) -> None:
        # --- dimensions & layout ------------------------------------------------
        self.screen_width = screen_width
        self.screen_height = screen_height
        self.content_width = content_width
        self.content_height = content_height
        self.scale_mode = scale_mode
        self.background_color = config.background_color
        self.offset_x = config.offset_x
        self.offset_y = config.offset_y

        # --- Saved objects ----------------------------------------------------
        self.config = config
        self.osd = osd_manager
        self.swapchain = swapchain_manager
        self.stats = stats_hud

        # --- Screen texture (rendered by Lanczos and all post-FX) ---------------
        self.screen_tex = Texture2D(
            screen_width, screen_height, format=R16G16B16A16_FLOAT
        )
        self.screen_tex_8bit = Texture2D(screen_width, screen_height)
        self._active_source_texture: Optional[Texture2D] = None

        # --- Helpers ------------------------------------------------------------
        # Linear/Delinear/Dither passes
        self._needs_delinearize: bool = True
        self._linear_tex = None
        self._linearize = Linearize()
        self._delinearize = Delinearize()
        self._delinearize.set_noise_texture()
        self._delinearize.set_target_texture(self.screen_tex_8bit)
        self._dither_copy = DitherCopy()
        self._dither_copy.set_noise_texture()
        self._dither_copy.set_target_texture(self.screen_tex_8bit)

        # Clear pass
        self._clear = Clear()
        self._clear.set_target_texture(self.screen_tex)

        # --- Scalers ------------------------------------------------------------
        # Copy
        self._copy = CopyScaler()

        # Upsampler
        if config.upsampler == "lanczos":
            self._upsampler = LanczosScaler()
        elif config.upsampler == "fsr":
            self._upsampler = FSRScaler()
        elif config.upsampler == "nis":
            self._upsampler = NISScaler()
        else:
            logger.warning(
                "Upsampler '%s' not recognized. Falling back to default.",
                config.upsampler,
            )
            self._upsampler = LanczosScaler()

        # Downsampler
        if config.downsampler == "catmull":
            self._downsampler = CatmullRomScaler()
        elif config.downsampler == "lanczos":
            self._downsampler = LanczosScaler()
        else:
            logger.warning(
                "Downsampler '%s' not recognized. Falling back to default.",
                config.upsampler,
            )
            self._upsampler = CatmullRomScaler()

        for sampler in (self._copy, self._upsampler, self._downsampler):
            if sampler is not None:
                sampler.set_target_texture(self.screen_tex)
                sampler.configure(self.config)

        # --- Post-processing passes (only created if config enables them) ------
        # Debanding
        self._deband: Optional[Deband] = None
        self._deband_tex: Optional[Texture2D] = None  # temp debanded output
        if config.deband_enabled:
            self._deband = Deband()
            logger.debug("Deband pass created")

        # Magpie effects (each one writes to its own texture)
        self._anime4k: Optional[Anime4KUpscaleS] = None
        self._luma_sharpen: Optional[LumaSharpen] = None
        self._luma_sharpen_tex: Optional[Texture2D] = None
        self._crt: Optional[CRTEasymode] = None
        self._crt_tex: Optional[Texture2D] = None
        self._effect_times: Dict[str, float] = {}
        self._effect_frames: int = 0
        self._sync_magpie_effects(config)

        # MagpieFX chain
        self._magpie_specs: List[EffectSpec] = []
        self._magpie: List[MagpieEffect] = []
        self._magpie_stats: List[List[float]] = []
        self._magpie_stats_since = time.perf_counter()
        self._load_magpie_effects(config)

        # CAS
        self._cas: Optional[CAS] = None
        if config.cas_enabled:
            self._cas = CAS()
            self._cas.set_target_texture(self.screen_tex)
            logger.debug("CAS pass created")

        # Bloom
        self._bloom: Optional[Bloom] = None
        if config.bloom_enabled:
            self._bloom = Bloom()
            self._bloom.set_target_texture(self.screen_tex)
            logger.debug("Bloom pass created")

        # Vignette
        self._vignette: Optional[Vignette] = None
        if config.vignette_enabled:
            self._vignette = Vignette()
            self._vignette.set_target_texture(self.screen_tex)
            logger.debug("Vignette pass created")

        # LUT
        self._lut: Optional[LUT] = None
        if config.lut_enabled:
            self._lut = LUT(preset=config.lut_preset)
            self._lut.set_target_texture(self.screen_tex_8bit)
            logger.debug("LUT pass created")

        # Film grain
        self._grain: Optional[FilmGrain] = None
        if config.grain_enabled:
            self._grain = FilmGrain()
            self._grain.set_target_texture(self.screen_tex_8bit)
            logger.debug("Film grain pass created")

        # Frame counter, incremented each frame for temporal effects
        self._frame_counter: int = 0

    # ------------------------------------------------------------------
    #  Public API
    # ------------------------------------------------------------------

    def set_upscaled_source(self, texture: Texture2D) -> None:
        """
        Store the raw upscaled texture (before any post-processing).

        This texture will be used as the starting point in the next
        :meth:`present` call; it may be optionally debanded before
        scaling.
        """
        self._raw_upscaled_tex = texture

    def present(self, wait_for_fence: bool = False) -> None:
        """
        Run the entire post-processing chain and present to the swapchain.

        Steps:
          1. Deband (if enabled) -> writes to `_deband_tex`.
          2. Lanczos scaling.
          3. CAS -> bloom -> vignette -> LUT -> grain (each only if enabled).
          4. OSD overlay blend.
          5. Swapchain present.
          6. Increment frame counter.
        """
        src = self._raw_upscaled_tex
        if src is None:
            logger.warning("No source texture set, skipping present")
            return

        # ---- Debanding ------------------------------------------------------
        src = self._apply_deband_if_enabled(src)

        # ---- Magpie effects -------------------------------------------------
        src = self._apply_magpie_effects(src)
        self._active_source_texture = src

        # ---- Linearize and scale --------------------------------------------
        r_x, r_y, r_w, r_h, dst_x, dst_y = self._compute_scaling_params(
            src.width, src.height
        )
        if r_w == src.width and r_h == src.height:
            scaler = self._copy
        elif r_w >= src.width or r_h >= src.height:
            scaler = self._upsampler
        else:
            scaler = self._downsampler
        self._scale(scaler, src, src.width, src.height, dst_x, dst_y, r_w, r_h)

        # ---- CAS ------------------------------------------------------------
        self._apply_cas_if_enabled()

        # ---- Bloom ----------------------------------------------------------
        self._apply_bloom_if_enabled()

        # ---- Vignette -------------------------------------------------------
        self._apply_vignette_if_enabled()

        # ---- Delinearize or Dither-Copy ------------------------------------
        if self._needs_delinearize:
            self._delinearize.update_constants()
            self._delinearize.set_source_texture(self.screen_tex)
            self._delinearize.dispatch_auto()
        else:
            self._dither_copy.update_constants()
            self._dither_copy.set_source_texture(self.screen_tex)
            self._dither_copy.dispatch_auto()

        # ---- LUT ------------------------------------------------------------
        self._apply_lut_if_enabled()

        # ---- Film grain -----------------------------------------------------
        self._apply_grain_if_enabled()

        # ---- Stats panel and OSD blend ---------------------------------------
        if self.stats is not None:
            self.stats.blend(self.screen_tex_8bit)
        self.osd.blend_active(self.screen_tex_8bit)

        # ---- Swapchain present ----------------------------------------------
        self.swapchain.present(self.screen_tex_8bit, wait_for_fence=wait_for_fence)

        # ---- Advance frame counter ------------------------------------------
        self._frame_counter += 1

    def present_unchanged(self):
        """Re-present the current screen texture without any processing."""
        self.swapchain.present(self.screen_tex_8bit, wait_for_fence=False)

    def update_config(self, config: Config) -> None:
        """Update post-processing passes to match a new configuration."""
        self.config = config
        self.scale_mode = config.output_geometry
        self.offset_x = config.offset_x
        self.offset_y = config.offset_y
        self.background_color = config.background_color

        # ---- Samplers ----
        for sampler in (self._copy, self._upsampler, self._downsampler):
            if sampler is not None:
                sampler.configure(self.config)

        # ---- Debanding ----
        if config.deband_enabled:
            if self._deband is None:
                self._deband = Deband()
        else:
            if self._deband is not None:
                self._deband = None
                self._deband_tex = None

        # ---- Magpie effects ----
        self._sync_magpie_effects(config)

        # ---- MagpieFX ----
        self._load_magpie_effects(config)

        # ---- CAS ----
        if config.cas_enabled:
            if self._cas is None:
                self._cas = CAS()
                self._cas.set_target_texture(self.screen_tex)
        else:
            if self._cas is not None:
                self._cas = None

        # ---- Bloom ----
        if config.bloom_enabled:
            if self._bloom is None:
                self._bloom = Bloom()
                self._bloom.set_target_texture(self.screen_tex)
        else:
            if self._bloom is not None:
                self._bloom = None

        # ---- Vignette ----
        if config.vignette_enabled:
            if self._vignette is None:
                self._vignette = Vignette()
                self._vignette.set_target_texture(self.screen_tex)
        else:
            if self._vignette is not None:
                self._vignette = None

        # ---- LUT ----
        if config.lut_enabled:
            self._lut = LUT(preset=config.lut_preset)
            self._lut.set_target_texture(self.screen_tex_8bit)
        else:
            self._lut = None

        # ---- Film Grain ----
        if config.grain_enabled:
            if self._grain is None:
                self._grain = FilmGrain()
                self._grain.set_target_texture(self.screen_tex_8bit)
        else:
            if self._grain is not None:
                self._grain = None

        # Reset cached compute pipelines that reference the old screen texture
        self.osd.clear_compute_cache()

    def get_scaling_rect(self, scale_factor: Optional[float]) -> List[float]:
        """
        Return the rectangle (in overlay widget coordinates) where content is
        drawn. Used by the overlay window for mouse-event mapping.
        """
        src_tex = self._active_source_texture
        if src_tex is None:
            return [0, 0, 0, 0]

        r_x, r_y, r_w, r_h = calculate_scaling_rect(
            src_tex.width,
            src_tex.height,
            self.content_width,
            self.content_height,
            self.scale_mode,
        )

        canvas_x = (self.screen_width - self.content_width) // 2
        canvas_y = (self.screen_height - self.content_height) // 2
        dst_x = canvas_x + r_x + self.offset_x
        dst_y = canvas_y + r_y + self.offset_y

        return [
            dst_x / scale_factor,
            dst_y / scale_factor,
            r_w / scale_factor,
            r_h / scale_factor,
        ]

    def resize(self, new_width: int, new_height: int) -> None:
        """
        Handle overlay window resize.
        Re-creates the screen texture and rebinds it to every in-place pass.
        """
        self.screen_width = new_width
        self.screen_height = new_height
        self.screen_tex = Texture2D(new_width, new_height, format=R16G16B16A16_FLOAT)
        self.screen_tex_8bit = Texture2D(new_width, new_height)

        # Rebind 16-bit passes
        for pass_ in (
            self._clear,
            self._copy,
            self._upsampler,
            self._downsampler,
            self._cas,
            self._bloom,
            self._vignette,
        ):
            if pass_ is not None:
                pass_.set_target_texture(self.screen_tex)

        # Rebind 8-bit passes
        for pass_ in (
            self._delinearize,
            self._dither_copy,
            self._lut,
            self._grain,
        ):
            if pass_ is not None:
                pass_.set_target_texture(self.screen_tex_8bit)

        self.osd.clear_compute_cache()

    def close(self) -> None:
        """Release all GPU resources while the Vulkan device is still alive."""
        self._copy = None
        self._upsampler = None
        self._downsampler = None

        self._linearize = None
        self._delinearize = None
        self._clear = None

        self._deband = None
        self._deband_tex = None
        self._anime4k = None
        self._luma_sharpen = None
        self._luma_sharpen_tex = None
        self._crt = None
        self._crt_tex = None
        self._magpie = []
        self._cas = None
        self._bloom = None
        self._vignette = None
        self._lut = None
        self._grain = None

        self._linear_tex = None
        self.screen_tex = None
        self.screen_tex_8bit = None

    # ------------------------------------------------------------------
    #  Internal helpers
    # ------------------------------------------------------------------

    def _scale(
        self,
        scaler: Scaler,
        src: Texture2D,
        src_width: int,
        src_height: int,
        dst_x: int,
        dst_y: int,
        r_w: int,
        r_h: int,
    ) -> None:
        # Remember whether the output is linear
        self._needs_delinearize = scaler.linear_output

        if not scaler.requires_linear_input:
            # Clear the whole screen with the background color
            self._clear.update_constants(self.background_color)
            self._clear.dispatch_auto()

            # Only scaler pass, no linearize and no background color
            scaler.set_source_texture(src)
            scaler.update_constants(
                src_width=src_width,
                src_height=src_height,
                dst_width=self.screen_width,
                dst_height=self.screen_height,
                dst_x=dst_x,
                dst_y=dst_y,
                dst_w=r_w,
                dst_h=r_h,
            )
            scaler.dispatch_auto()
            return

        # Ensure the intermediate linear texture exists
        if (
            self._linear_tex is None
            or self._linear_tex.width != src_width
            or self._linear_tex.height != src_height
        ):
            self._linear_tex = Texture2D(
                src_width, src_height, format=R16G16B16A16_FLOAT
            )

        # Linearization pass
        self._linearize.set_source_texture(src)
        self._linearize.set_target_texture(self._linear_tex)

        # Scaler pass
        scaler.set_source_texture(self._linear_tex)
        scaler.update_constants(
            background_color=self.background_color,
            src_width=src_width,
            src_height=src_height,
            dst_width=self.screen_width,
            dst_height=self.screen_height,
            dst_x=dst_x,
            dst_y=dst_y,
            dst_w=r_w,
            dst_h=r_h,
        )

        # Combined dispatch
        sequence = [
            (
                self._linearize.compute,
                (src_width + 15) // 16,
                (src_height + 15) // 16,
                1,
                b"",
            ),
            (
                scaler.compute,
                (self.screen_width + 15) // 16,
                (self.screen_height + 15) // 16,
                1,
                b"",
            ),
        ]
        self._linearize.compute.dispatch_sequence(sequence)

    def _apply_deband_if_enabled(self, src: Texture2D) -> Texture2D:
        """Run the debanding pass if enabled; return the (possibly debanded) texture."""
        if self._deband is None or not self.config.deband_enabled:
            return src

        # Ensure we have a compatible target texture
        if (
            self._deband_tex is None
            or self._deband_tex.width != src.width
            or self._deband_tex.height != src.height
        ):
            self._deband_tex = Texture2D(src.width, src.height)

        # Set source first, then target (order doesn't matter with the guard)
        self._deband.set_source_texture(src)
        self._deband.set_target_texture(self._deband_tex)

        self._deband.update_constants(
            strength=self.config.deband_strength,
            frame_index=self._frame_counter,  # temporal dither
        )
        self._deband.dispatch_auto()
        return self._deband_tex

    def _apply_magpie_effects(self, src: Texture2D) -> Texture2D:
        """Run the Magpie effects of the engine selected in the config."""
        if self.config.effects_engine == "magpiefx":
            return self._apply_magpiefx_effects(src)
        return self._apply_native_effects(src)

    def _sync_magpie_effects(self, config: Config) -> None:
        """Create or drop the Magpie effect passes to match `config`."""
        native = config.effects_engine == "native"
        if native and config.anime4k_enabled:
            if self._anime4k is None:
                self._anime4k = Anime4KUpscaleS()
                logger.debug("Anime4K_Upscale_S pass created")
        else:
            self._anime4k = None

        if native and config.luma_sharpen_enabled:
            if self._luma_sharpen is None:
                self._luma_sharpen = LumaSharpen()
                logger.debug("LumaSharpen pass created")
        else:
            self._luma_sharpen = None
            self._luma_sharpen_tex = None

        if native and config.crt_enabled:
            if self._crt is None:
                self._crt = CRTEasymode()
                logger.debug("CRT-Easymode pass created")
        else:
            self._crt = None
            self._crt_tex = None

    def _apply_native_effects(self, src: Texture2D) -> Texture2D:
        """
        Run the enabled Magpie effects on `src` in a single submission and
        return the resulting texture (or `src` if none is enabled).
        """
        sequence = []
        labels: List[str] = []

        if self._anime4k is not None:
            self._anime4k.set_source_texture(src)
            passes = self._anime4k.sequence()
            sequence += passes
            labels += ["anime4k"] * len(passes)
            src = self._anime4k.output_texture

        if self._luma_sharpen is not None:
            tex = self._luma_sharpen_tex
            if tex is None or tex.width != src.width or tex.height != src.height:
                tex = self._luma_sharpen_tex = Texture2D(src.width, src.height)
            c = self.config
            self._luma_sharpen.set_source_texture(src)
            self._luma_sharpen.set_target_texture(tex)
            self._luma_sharpen.update_constants(
                strength=c.luma_sharpen_strength,
                clamp=c.luma_sharpen_clamp,
                pattern=c.luma_sharpen_pattern,
                offset_bias=c.luma_sharpen_offset_bias,
            )
            sequence.append(
                (
                    self._luma_sharpen.compute,
                    (tex.width + 15) // 16,
                    (tex.height + 15) // 16,
                    1,
                    b"",
                )
            )
            labels.append("luma_sharpen")
            src = tex

        if self._crt is not None:
            # CRT-Easymode is also the scaler: render at the destination size
            _, _, r_w, r_h, _, _ = self._compute_scaling_params(src.width, src.height)
            r_w, r_h = max(r_w, 1), max(r_h, 1)
            tex = self._crt_tex
            if tex is None or tex.width != r_w or tex.height != r_h:
                tex = self._crt_tex = Texture2D(r_w, r_h)
            c = self.config
            self._crt.set_source_texture(src)
            self._crt.set_target_texture(tex)
            self._crt.update_constants(
                sharpness_h=c.crt_sharpness_h,
                sharpness_v=c.crt_sharpness_v,
                mask_strength=c.crt_mask_strength,
                mask_dot_width=c.crt_mask_dot_width,
                mask_dot_height=c.crt_mask_dot_height,
                mask_stagger=c.crt_mask_stagger,
                mask_size=c.crt_mask_size,
                scanline_strength=c.crt_scanline_strength,
                scanline_beam_width_min=c.crt_scanline_beam_width_min,
                scanline_beam_width_max=c.crt_scanline_beam_width_max,
                scanline_bright_min=c.crt_scanline_bright_min,
                scanline_bright_max=c.crt_scanline_bright_max,
                scanline_cutoff=c.crt_scanline_cutoff,
                gamma_input=c.crt_gamma_input,
                gamma_output=c.crt_gamma_output,
                bright_boost=c.crt_bright_boost,
                dilation=c.crt_dilation,
            )
            sequence.append(
                (self._crt.compute, (r_w + 15) // 16, (r_h + 15) // 16, 1, b"")
            )
            labels.append("crt_easymode")
            src = tex

        if not sequence:
            return src

        timing = _EFFECT_TIMING or self._stats_visible()
        result = sequence[0][0].dispatch_sequence(sequence, timestamps=timing)
        if result is not None:
            self._record_effect_times(labels, result[1])
        return src

    def _record_effect_times(self, labels: List[str], ts: List[float]) -> None:
        """Accumulate per-effect GPU time and log the average periodically."""
        # dispatch_sequence writes timestamps 3 + 2i / 4 + 2i around pass i
        spans: Dict[str, List[float]] = {}
        for i, label in enumerate(labels):
            start, end = ts[3 + 2 * i], ts[4 + 2 * i]
            span = spans.setdefault(label, [start, end])
            span[0], span[1] = min(span[0], start), max(span[1], end)
        if self.stats is not None:
            for label, (start, end) in spans.items():
                self.stats.set_gpu_ms(label, (end - start) / 1e6)
        if not _EFFECT_TIMING:
            return
        for label, (start, end) in spans.items():
            self._effect_times[label] = self._effect_times.get(label, 0.0) + (
                end - start
            )
        self._effect_frames += 1
        if self._effect_frames >= _TIMING_WINDOW:
            n = self._effect_frames
            logger.info(
                "Magpie effects GPU time (avg of %d frames): %s",
                n,
                ", ".join(
                    f"{k} {v / n / 1e6:.3f} ms" for k, v in self._effect_times.items()
                ),
            )
            self._effect_times.clear()
            self._effect_frames = 0

    def _load_magpie_effects(self, config: Config) -> None:
        """(Re)build the MagpieFX chain when the configured specs change."""
        try:
            specs = (
                parse_effect_chain(config.magpie_effects or [])
                if config.effects_engine == "magpiefx"
                else []
            )
        except ValueError as e:
            logger.error("Invalid MagpieFX configuration: %s", e)
            specs = []

        if specs == self._magpie_specs:
            return
        if (
            self._magpie
            and [n for n, _ in specs] == [n for n, _ in self._magpie_specs]
            and self._magpie_active == self._magpie_specs
        ):
            # Same effects, new parameter values: no recompile (GUI sliders)
            try:
                for fx, (_, params) in zip(self._magpie, specs):
                    fx.set_params(params)
                self._magpie_specs = self._magpie_active = specs
                return
            except MagpieFXError as e:
                logger.error("Invalid MagpieFX parameters: %s", e)
        self._magpie_specs = specs
        self._build_magpie_chain(specs)

    def _build_magpie_chain(self, specs: List[EffectSpec]) -> None:
        """
        Load the effects of *specs*. Frames travel as BGRA bytes in R8G8B8A8
        textures while MagpieFX effects expect RGBA, so the first effect
        swaps red/blue on read and the last one on write (folded into the
        generated shaders; between effects the data stays RGBA).
        """
        self._magpie = []
        self._magpie_active: List[EffectSpec] = []
        specs = list(specs)
        while specs:
            chain: List[MagpieEffect] = []
            failed = None
            last = len(specs) - 1
            for i, (name, params) in enumerate(specs):
                try:
                    start = time.perf_counter()
                    fx = MagpieEffect.load(
                        name, params, swap_input=i == 0, swap_output=i == last
                    )
                except MagpieFXError as e:
                    logger.error("MagpieFX effect '%s' disabled: %s", name, e)
                    failed = i
                    break
                logger.info(
                    "MagpieFX effect '%s' loaded (%d passes, %.0f ms)",
                    fx.desc.name,
                    len(fx.desc.passes),
                    (time.perf_counter() - start) * 1000,
                )
                chain.append(fx)
            if failed is None:
                self._magpie = chain
                self._magpie_active = specs
                break
            # The ends of the chain may have moved: reload without it
            del specs[failed]
        self._magpie_stats = []
        self._magpie_stats_since = time.perf_counter()
        if self.stats is not None:
            self.stats.clear_gpu_ms()  # the fx<i> labels moved

    def _apply_magpiefx_effects(self, src: Texture2D) -> Texture2D:
        """Run the MagpieFX chain; each effect's OUTPUT feeds the next one."""
        if not self._magpie:
            return src

        profile = self.config.magpie_profile or self._stats_visible()
        if len(self._magpie_stats) != len(self._magpie):
            self._magpie_stats = [[] for _ in self._magpie]

        input_tex = src
        for i, fx in enumerate(self._magpie):
            _, _, r_w, r_h = calculate_scaling_rect(
                src.width,
                src.height,
                self.content_width,
                self.content_height,
                self.scale_mode,
            )
            try:
                src = fx.process(
                    src,
                    (max(1, r_w), max(1, r_h)),
                    frame_count=self._frame_counter,
                    profile=profile,
                )
            except Exception as e:  # keep presenting without the effect
                logger.error(
                    "MagpieFX effect '%s' failed, disabling: %s", fx.desc.name, e
                )
                # Rebuild without it so the BGRA swaps move to the new ends
                specs = list(self._magpie_active)
                del specs[i]
                self._build_magpie_chain(specs)
                return input_tex
            if profile:
                self._magpie_stats[i].append(sum(fx.last_gpu_ms))
                if self.stats is not None:
                    self.stats.set_gpu_ms(f"fx{i}", sum(fx.last_gpu_ms))

        if self.config.magpie_profile:
            self._log_magpie_stats()
        elif profile:
            self._magpie_stats = [[] for _ in self._magpie]
        return src

    def _log_magpie_stats(self, interval: float = 5.0) -> None:
        now = time.perf_counter()
        if now - self._magpie_stats_since < interval or not self._magpie_stats[0]:
            return
        parts: List[Tuple[str, float]] = []
        for fx, samples in zip(self._magpie, self._magpie_stats):
            samples.sort()
            parts.append((fx.desc.name, samples[len(samples) // 2]))
        frames = len(self._magpie_stats[0])
        logger.info(
            "MagpieFX GPU median over %d frames: %s | total %.3f ms/frame",
            frames,
            ", ".join(f"{n} {ms:.3f} ms" for n, ms in parts),
            sum(ms for _, ms in parts),
        )
        self._magpie_stats = [[] for _ in self._magpie]
        self._magpie_stats_since = now

    def _stats_visible(self) -> bool:
        return self.stats is not None and self.stats.visible

    def active_effects(self) -> List[Tuple[str, str]]:
        """(display name, GPU-time label) of the Magpie effects that run."""
        if self.config.effects_engine == "magpiefx":
            return [(fx.desc.name, f"fx{i}") for i, fx in enumerate(self._magpie)]
        effects = []
        if self._anime4k is not None:
            effects.append(("Anime4K_Upscale_S", "anime4k"))
        if self._luma_sharpen is not None:
            effects.append(("LumaSharpen", "luma_sharpen"))
        if self._crt is not None:
            effects.append(("CRT_Easymode", "crt_easymode"))
        return effects

    def _apply_cas_if_enabled(self) -> None:
        if self._cas is not None and self.config.cas_enabled:
            self._cas.update_constants(strength=self.config.cas_strength)
            self._cas.dispatch_auto()

    def _apply_bloom_if_enabled(self) -> None:
        if self._bloom is not None and self.config.bloom_enabled:
            self._bloom.update_constants(
                strength=self.config.bloom_strength,
                threshold=self.config.bloom_threshold,
                radius=self.config.bloom_radius,
            )
            self._bloom.dispatch_auto()

    def _apply_vignette_if_enabled(self) -> None:
        if self._vignette is not None and self.config.vignette_enabled:
            self._vignette.update_constants(
                strength=self.config.vignette_strength,
                radius=self.config.vignette_radius,
                falloff=self.config.vignette_falloff,
            )
            self._vignette.dispatch_auto()

    def _apply_lut_if_enabled(self) -> None:
        if self._lut is not None and self.config.lut_enabled:
            self._lut.update_constants(intensity=self.config.lut_intensity)
            self._lut.dispatch_auto()

    def _apply_grain_if_enabled(self) -> None:
        if self._grain is not None and self.config.grain_enabled:
            self._grain.update_constants(
                strength=self.config.grain_strength,
                grain_size=self.config.grain_size,
                frame_index=self._frame_counter,
            )
            self._grain.dispatch_auto()

    def _compute_scaling_params(self, src_width: int, src_height: int):
        """
        Compute the destination rectangle and screen position.
        Returns (r_x, r_y, r_w, r_h, dst_x, dst_y).
        """
        r_x, r_y, r_w, r_h = calculate_scaling_rect(
            src_width,
            src_height,
            self.content_width,
            self.content_height,
            self.scale_mode,
        )
        canvas_x = (self.screen_width - self.content_width) // 2
        canvas_y = (self.screen_height - self.content_height) // 2
        dst_x = canvas_x + r_x + self.offset_x
        dst_y = canvas_y + r_y + self.offset_y
        return r_x, r_y, r_w, r_h, dst_x, dst_y
