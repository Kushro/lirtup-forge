//! The bar over the game (round 4, front T; 5.6 and the «Superposición»
//! artboard of the design): a floating toolbar drawn in Vulkan on the
//! screen texture, like the stats panel, with the tokens of [`crate::tokens`].
//!
//! - It shows when the pointer reaches the top edge of the overlay (the
//!   reveal band of [`ToolbarZone`]) or with `ToggleToolbar`, and hides
//!   [`HIDE_AFTER`] after the pointer leaves it, unless pinned.
//! - Buttons: pin, stats, crop, screenshot, frame generation multiplier,
//!   fps (a readout), hide, to window / fullscreen, stop.
//! - While it is shown, the pointer over it is not forwarded to the game
//!   (`OverlayCommand::SetToolbarZone`); its events come back as
//!   `OverlayNotice::Pointer`.
//! - In windowed mode, dragging the bar by its background (the readout, the
//!   dividers, the padding) moves the window ([`ToolbarAction::Move`] →
//!   `OverlayCommand::BeginMove`).
//! - Changing the multiplier applies it live and sends
//!   `Event::SaveSetting` so the GUI keeps it; so does «pin»
//!   ([`PINNED_KEY`]).
//!
//! Layout, in CSS pixels of the design (`lf.css`: `.ftb.glass`, `.ibtn`,
//! `.btn-xs`), times [`Toolbar::scale`]: 64 high, fully rounded, 16 from
//! the top, centered; 40 px round buttons with 24 px icons, 2 px apart; a
//! 1×28 divider with 6 px margins; the multiplier as a 32 px tonal button
//! in the tertiary color; the fps in JetBrains Mono 15/600.
//!
//! It shows with the `fast-spatial` spring (it overshoots and settles) and
//! hides with the `effects` fade.
//!
//! Criterion (5.8): drawing it changes the GPU time of a frame by at most
//! 0.1 ms. The session thread owns it, next to the OSD and the panel. The
//! image is rasterized on the CPU only when something changes, and the GPU
//! blends only the bar's box (`ui_draw`).

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use lirtup_effects::Pass;
use lirtup_effects::ui_draw::{Shape, UiDraw, UiFont, UiFonts};
use lirtup_gpu::{Device, Texture};
use lirtup_x11::Rect;
use lirtup_x11::overlay_input::{PointerEvent, ToolbarZone};

use crate::tokens::{self, Motion, Scheme, color, with_alpha};
use crate::{PipelineError, Result};

/// Hide this long after the pointer leaves the bar.
pub const HIDE_AFTER: Duration = Duration::from_secs(2);

/// The YAML key «pin» is saved under (top level), read from
/// `Config::extra` at the start of a session.
pub const PINNED_KEY: &str = "toolbar_pinned";

/// The Material Symbols the bar uses. `scripts/gen-tokens-icons.py` reads
/// the string literals of this file for the bar's font instances, so every
/// icon is named here.
pub const ICON_NAMES: [&str; 9] = [
    "push_pin",
    "monitoring",
    "crop",
    "photo_camera",
    "animation",
    "visibility_off",
    "close_fullscreen",
    "open_in_full",
    "close",
];

/// What a button of the bar does. The session thread turns each into what
/// a hotkey does (`Screenshot`, `ToggleStats`, …) or into its own action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolbarAction {
    /// Keep the bar shown (the bar already toggled its own state; the
    /// session saves it).
    Pin,
    /// Show or hide the stats panel.
    Stats,
    /// Crop: the menu bar detection again, or the crop of the profile.
    Crop,
    /// Save a screenshot.
    Screenshot,
    /// Next frame generation multiplier (Off, ×2, ×3, ×4, Auto).
    Multiplier,
    /// Hide the bar (until the pointer comes back to the edge). The bar
    /// already hid itself.
    Hide,
    /// Fullscreen ↔ windowed.
    ToWindow,
    /// Stop the session.
    Stop,
    /// A press on the bar's background in windowed mode: move the window
    /// (overlay coordinates of the press, and its button).
    Move {
        /// X button.
        button: u8,
        /// X in the overlay.
        x: i32,
        /// Y in the overlay.
        y: i32,
    },
}

/// What the bar shows; the session thread fills it every frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolbarState {
    /// Shown fps (the readout).
    pub fps: f32,
    /// Captured fps (the readout shows «captured → shown» while frames are
    /// generated).
    pub capture_fps: f32,
    /// Frame generation multiplier in use (`None` = off).
    pub multiplier: Option<u8>,
    /// The configured multiplier is Auto.
    pub auto: bool,
    /// The label of the multiplier button («×2 Auto», «No»); `None` hides
    /// the button (no frame generation in this session).
    pub multiplier_label: Option<String>,
    /// The stats panel is visible.
    pub stats: bool,
    /// The session is windowed (the to-window button shows «fullscreen»).
    pub windowed: bool,
    /// Theme of the panel and the OSD.
    pub scheme: Scheme,
}

/// The fonts of the bar, the panel and the OSD, embedded from
/// `lirtup-ui/assets/fonts/` and parsed once per process.
pub fn ui_fonts() -> Result<UiFonts> {
    static FONTS: OnceLock<std::result::Result<UiFonts, String>> = OnceLock::new();
    macro_rules! font {
        ($name:literal) => {
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../lirtup-ui/assets/fonts/",
                $name
            ))
        };
    }
    FONTS
        .get_or_init(|| {
            UiFonts::from_static(
                font!("GoogleSansFlex.ttf"),
                font!("JetBrainsMono.ttf"),
                font!("MaterialSymbolsRounded-Fill0.ttf"),
                font!("MaterialSymbolsRounded-Fill1.ttf"),
            )
            .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(|e| PipelineError::other(lirtup_proto::Stage::Effects, format!("bundled fonts: {e}")))
}

/// The design's sizes for a screen: 1 up to 1440 lines, then in
/// proportion (2 at 2880).
pub fn ui_scale(screen: (u32, u32)) -> f32 {
    (screen.1 as f32 / 1440.0).clamp(1.0, 2.0)
}

/// CSS px of the layout.
const BAR_H: f32 = 64.0;
const BAR_TOP: f32 = 16.0;
const BAR_PAD: f32 = 8.0;
const GAP: f32 = 2.0;
const BTN: f32 = 40.0;
const ICON: f32 = 24.0;
const DIVIDER_H: f32 = 28.0;
const DIVIDER_MARGIN: f32 = 6.0;
const CHIP_H: f32 = 32.0;
const CHIP_PAD: f32 = 12.0;
const CHIP_ICON: f32 = 18.0;
const CHIP_GAP: f32 = 8.0;
const CHIP_TEXT: f32 = 13.0;
const READOUT_PAD: f32 = 12.0;
const READOUT_TEXT: f32 = 15.0;
/// The band at the top edge that reveals the bar.
const REVEAL: f32 = 6.0;
/// The image's bound: the bar never gets wider than this (CSS px).
const MAX_W: f32 = 900.0;

/// One element of the bar, its box relative to the bar's top-left.
#[derive(Debug, Clone, PartialEq)]
struct Item {
    kind: ItemKind,
    rect: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
enum ItemKind {
    Button {
        action: ToolbarAction,
        icon: &'static str,
    },
    Divider,
    Multiplier(String),
    Readout(String),
}

impl Item {
    fn action(&self) -> Option<ToolbarAction> {
        match self.kind {
            ItemKind::Button { action, .. } => Some(action),
            ItemKind::Multiplier(_) => Some(ToolbarAction::Multiplier),
            _ => None,
        }
    }
}

/// A show or hide in progress.
#[derive(Debug, Clone, Copy)]
struct Anim {
    from: f32,
    to: f32,
    start: Instant,
    motion: Motion,
}

impl Anim {
    /// The shown amount at `now` (may pass 1 with a spring) and whether it
    /// ended.
    fn value(&self, now: Instant) -> (f32, bool) {
        let ms = self.motion.ms.max(1) as f32;
        let t = now.saturating_duration_since(self.start).as_secs_f32() * 1000.0 / ms;
        if t >= 1.0 {
            return (self.to, true);
        }
        (
            self.from + (self.to - self.from) * tokens::ease(&self.motion, t),
            false,
        )
    }
}

/// The bar.
pub struct Toolbar {
    draw: UiDraw,
    screen: (u32, u32),
    scale: f32,
    state: ToolbarState,
    items: Vec<Item>,
    /// Bar size, screen px.
    size: (f32, f32),
    /// Wanted visibility (the animation goes there).
    shown: bool,
    /// The shown amount, 0 (hidden) to 1 (a spring passes 1).
    amount: f32,
    anim: Option<Anim>,
    pinned: bool,
    hide_at: Option<Instant>,
    hover: Option<usize>,
    pressed: Option<usize>,
    /// The image must be drawn again.
    dirty: bool,
}

impl Toolbar {
    /// Create the bar for a screen texture (the presenter's `screen`).
    pub fn new(device: &Arc<Device>, screen: &Texture) -> Result<Toolbar> {
        let size = (screen.width(), screen.height());
        let scale = ui_scale(size);
        let draw = UiDraw::new(device, screen, ui_fonts()?, max_image(scale), "toolbar")?;
        let mut bar = Toolbar {
            draw,
            screen: size,
            scale,
            state: ToolbarState::default(),
            items: Vec::new(),
            size: (0.0, 0.0),
            shown: false,
            amount: 0.0,
            anim: None,
            pinned: false,
            hide_at: None,
            hover: None,
            pressed: None,
            dirty: true,
        };
        bar.layout();
        Ok(bar)
    }

    /// The screen texture changed (resize).
    pub fn rebind(&mut self, device: &Arc<Device>, screen: &Texture) -> Result<()> {
        self.screen = (screen.width(), screen.height());
        self.scale = ui_scale(self.screen);
        self.draw.rebind(device, screen, max_image(self.scale))?;
        self.layout();
        self.dirty = true;
        Ok(())
    }

    /// The factor of the design's sizes on this screen.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Pinned (shown until unpinned).
    pub fn pinned(&self) -> bool {
        self.pinned
    }

    /// Pin or unpin (the saved setting at the start of a session). A
    /// pinned bar shows at once.
    pub fn set_pinned(&mut self, pinned: bool, now: Instant) {
        self.pinned = pinned;
        self.dirty = true;
        if pinned {
            self.set_shown(true, now);
        }
    }

    /// Shown, or showing.
    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// Where the bar takes the pointer, in overlay coordinates, for
    /// `OverlayCommand::SetToolbarZone`. `overlay` is the window's rect
    /// (only its size counts: the screen texture covers it).
    pub fn zone(&self, overlay: Rect) -> Option<ToolbarZone> {
        let _ = overlay;
        let bar = self.shown.then(|| {
            let [x, y, w, h] = self.rect_at(1.0);
            Rect {
                x: x.floor() as i32,
                y: y.floor() as i32,
                width: w.ceil() as u32,
                height: h.ceil() as u32,
            }
        });
        Some(ToolbarZone {
            bar,
            reveal: (REVEAL * self.scale).ceil() as u32,
        })
    }

    /// A pointer event over the bar or its reveal band; returns the action
    /// of a click (or of a press on the background, in windowed mode).
    pub fn pointer(&mut self, event: PointerEvent, now: Instant) -> Option<ToolbarAction> {
        match event {
            PointerEvent::Motion { x, y } => {
                if !self.shown {
                    // Only the band reports motion while the bar is hidden.
                    self.set_shown(true, now);
                }
                self.hide_at = None;
                let hover = self.item_at(x, y);
                if hover != self.hover {
                    self.hover = hover;
                    self.dirty = true;
                }
                None
            }
            PointerEvent::Leave => {
                if self.hover.take().is_some() {
                    self.dirty = true;
                }
                if self.shown && !self.pinned {
                    self.hide_at = Some(now + HIDE_AFTER);
                }
                None
            }
            PointerEvent::Press { button: 1, x, y } => {
                if !self.shown {
                    return None;
                }
                match self.item_at(x, y) {
                    Some(i) if self.items[i].action().is_some() => {
                        self.pressed = Some(i);
                        self.dirty = true;
                        None
                    }
                    _ if self.state.windowed && self.contains(x, y) => {
                        Some(ToolbarAction::Move { button: 1, x, y })
                    }
                    _ => None,
                }
            }
            PointerEvent::Release { button: 1, x, y } => {
                let pressed = self.pressed.take()?;
                self.dirty = true;
                if self.item_at(x, y) != Some(pressed) {
                    return None;
                }
                let action = self.items[pressed].action()?;
                match action {
                    ToolbarAction::Pin => {
                        self.pinned = !self.pinned;
                        self.hide_at = None;
                    }
                    ToolbarAction::Hide => self.set_shown(false, now),
                    _ => {}
                }
                Some(action)
            }
            PointerEvent::Press { .. } | PointerEvent::Release { .. } => None,
        }
    }

    /// Show or hide it (`ToggleToolbar`, the hide button). Shown by the
    /// hotkey, it hides again [`HIDE_AFTER`] later unless the pointer comes
    /// over it or it is pinned.
    pub fn set_shown(&mut self, shown: bool, now: Instant) {
        if shown == self.shown {
            return;
        }
        self.shown = shown;
        self.hover = None;
        self.pressed = None;
        self.hide_at = (shown && !self.pinned).then(|| now + HIDE_AFTER);
        self.anim = Some(Anim {
            from: self.amount.clamp(0.0, 1.0),
            to: if shown { 1.0 } else { 0.0 },
            start: now,
            motion: if shown {
                tokens::MOTION_FAST_SPATIAL
            } else {
                tokens::MOTION_EFFECTS
            },
        });
        self.dirty = true;
    }

    /// Advance the animation and redraw if the state changed; true when the
    /// screen must be presented again.
    pub fn update(&mut self, state: &ToolbarState, now: Instant) -> Result<bool> {
        if *state != self.state {
            let relayout = state.multiplier_label != self.state.multiplier_label
                || readout(state) != readout(&self.state)
                || state.windowed != self.state.windowed;
            self.state = state.clone();
            if relayout {
                self.layout();
            }
            self.dirty = true;
        }
        if let Some(at) = self.hide_at
            && now >= at
            && !self.pinned
            && self.hover.is_none()
            && self.pressed.is_none()
        {
            self.set_shown(false, now);
        }
        if let Some(anim) = self.anim {
            let (amount, done) = anim.value(now);
            self.amount = amount;
            if done {
                self.anim = None;
            }
            self.dirty = true;
        }
        if !std::mem::take(&mut self.dirty) {
            return Ok(false);
        }
        let was = self.draw.is_shown();
        let shapes = self.shapes();
        self.draw.set_scene(&shapes)?;
        // Hidden before and after: nothing on screen changed.
        Ok(was || self.draw.is_shown())
    }

    /// The pass that blends it onto the screen (`None` while hidden).
    pub fn pass(&self) -> Option<&dyn Pass> {
        self.draw.is_shown().then(|| self.draw.pass())
    }

    // ---------------------------------------------------------------------
    // Layout and drawing
    // ---------------------------------------------------------------------

    /// Place the items for the current state.
    fn layout(&mut self) {
        let s = self.scale;
        let mut items = Vec::new();
        let mut x = BAR_PAD * s;
        let mut push = |kind: ItemKind, w: f32, h: f32| {
            let y = (BAR_H * s - h) / 2.0;
            items.push(Item {
                kind,
                rect: [x, y, w, h],
            });
            x += w + GAP * s;
        };
        let button = |action, icon| ItemKind::Button { action, icon };
        push(button(ToolbarAction::Pin, "push_pin"), BTN * s, BTN * s);
        push(button(ToolbarAction::Stats, "monitoring"), BTN * s, BTN * s);
        push(button(ToolbarAction::Crop, "crop"), BTN * s, BTN * s);
        push(
            button(ToolbarAction::Screenshot, "photo_camera"),
            BTN * s,
            BTN * s,
        );
        let divider = (1.0 + 2.0 * DIVIDER_MARGIN) * s;
        push(ItemKind::Divider, divider, DIVIDER_H * s);
        if let Some(label) = &self.state.multiplier_label {
            let w = (2.0 * CHIP_PAD + CHIP_ICON + CHIP_GAP) * s
                + self.draw.measure(label, CHIP_TEXT * s, UiFont::TextStrong);
            push(ItemKind::Multiplier(label.clone()), w, CHIP_H * s);
        }
        let text = readout(&self.state);
        // Wide enough for three digits, so the bar does not move as the fps
        // change.
        let template = if self.state.multiplier.is_some() {
            "888 → 888 fps"
        } else {
            "888 fps"
        };
        let size = READOUT_TEXT * s;
        let w = 2.0 * READOUT_PAD * s
            + self
                .draw
                .measure(&text, size, UiFont::MonoStrong)
                .max(self.draw.measure(template, size, UiFont::MonoStrong));
        push(ItemKind::Readout(text), w, BAR_H * s);
        push(ItemKind::Divider, divider, DIVIDER_H * s);
        push(button(ToolbarAction::Hide, "visibility_off"), BTN * s, BTN * s);
        let to_window = if self.state.windowed {
            "open_in_full"
        } else {
            "close_fullscreen"
        };
        push(button(ToolbarAction::ToWindow, to_window), BTN * s, BTN * s);
        push(button(ToolbarAction::Stop, "close"), BTN * s, BTN * s);
        let width = x - GAP * s + BAR_PAD * s;
        self.items = items;
        self.size = (width.min(MAX_W * s), BAR_H * s);
        self.hover = None;
        self.pressed = None;
    }

    /// The bar's box on the screen at a shown amount (1 = in place; less
    /// slides it up past the top edge).
    fn rect_at(&self, amount: f32) -> [f32; 4] {
        let (w, h) = self.size;
        let top = BAR_TOP * self.scale;
        let x = ((self.screen.0 as f32 - w) / 2.0).round();
        let y = top - (1.0 - amount) * (top + h);
        [x, y.round(), w, h]
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        let [bx, by, w, h] = self.rect_at(1.0);
        let (x, y) = (x as f32, y as f32);
        x >= bx && y >= by && x < bx + w && y < by + h
    }

    /// The item under an overlay point (the bar in place).
    fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.contains(x, y) {
            return None;
        }
        let [bx, by, ..] = self.rect_at(1.0);
        let (px, py) = (x as f32 - bx, y as f32 - by);
        self.items.iter().position(|item| {
            let [ix, iy, w, h] = item.rect;
            px >= ix && px < ix + w && py >= iy && py < iy + h
        })
    }

    /// What to draw now.
    fn shapes(&self) -> Vec<Shape> {
        let amount = self.amount;
        if amount <= 0.001 {
            return Vec::new();
        }
        let alpha = amount.clamp(0.0, 1.0);
        let dark = Scheme::Dark;
        let c = |role: &str| with_alpha(color(role, dark), alpha);
        let tint = |role: &str, a: f32| with_alpha(color(role, dark), a * alpha);
        let s = self.scale;
        let [bx, by, w, h] = self.rect_at(amount);
        let mut shapes = vec![Shape::RoundRect {
            rect: [bx, by, w, h],
            radius: h / 2.0,
            color: c("glass"),
        }];
        for (i, item) in self.items.iter().enumerate() {
            let [ix, iy, iw, ih] = item.rect;
            let rect = [bx + ix, by + iy, iw, ih];
            let center = [rect[0] + iw / 2.0, rect[1] + ih / 2.0];
            let state_layer = if self.pressed == Some(i) {
                Some("state-pressed")
            } else if self.hover == Some(i) {
                Some("state-hover")
            } else {
                None
            };
            match &item.kind {
                ItemKind::Button { action, icon } => {
                    let selected = *action == ToolbarAction::Pin && self.pinned;
                    let active = *action == ToolbarAction::Stats && self.state.stats;
                    // `.ibtn.sel`: primary, `shape-md`; pressed squares too.
                    let radius = if selected || self.pressed == Some(i) {
                        tokens::SHAPE_MD * s
                    } else {
                        iw / 2.0
                    };
                    if selected {
                        shapes.push(Shape::RoundRect {
                            rect,
                            radius,
                            color: c("primary"),
                        });
                    } else if active {
                        shapes.push(Shape::RoundRect {
                            rect,
                            radius,
                            color: tint("primary", 0.18),
                        });
                    }
                    if let Some(layer) = state_layer {
                        shapes.push(Shape::RoundRect {
                            rect,
                            radius,
                            color: c(layer),
                        });
                    }
                    let fg = if selected {
                        "on-primary"
                    } else if *action == ToolbarAction::Stop {
                        "error"
                    } else {
                        "glass-content"
                    };
                    if let Some(codepoint) = tokens::icon(icon) {
                        shapes.push(Shape::Icon {
                            center,
                            size: ICON * s,
                            codepoint,
                            filled: selected,
                            color: c(fg),
                        });
                    }
                }
                ItemKind::Divider => shapes.push(Shape::RoundRect {
                    rect: [center[0] - 0.5 * s, rect[1], s, ih],
                    radius: 0.0,
                    color: tint("glass-content", 0.18),
                }),
                ItemKind::Multiplier(label) => {
                    // `.btn.btn-xs` with the tertiary at 18 %.
                    shapes.push(Shape::RoundRect {
                        rect,
                        radius: ih / 2.0,
                        color: tint("tertiary", 0.18),
                    });
                    if let Some(layer) = state_layer {
                        shapes.push(Shape::RoundRect {
                            rect,
                            radius: ih / 2.0,
                            color: c(layer),
                        });
                    }
                    let fg = c("on-tertiary-container");
                    let icon_x = rect[0] + CHIP_PAD * s + CHIP_ICON * s / 2.0;
                    if let Some(codepoint) = tokens::icon("animation") {
                        shapes.push(Shape::Icon {
                            center: [icon_x, center[1]],
                            size: CHIP_ICON * s,
                            codepoint,
                            filled: false,
                            color: fg,
                        });
                    }
                    shapes.push(self.text_shape(
                        [rect[0] + (CHIP_PAD + CHIP_ICON + CHIP_GAP) * s, center[1]],
                        CHIP_TEXT * s,
                        UiFont::TextStrong,
                        label,
                        fg,
                    ));
                }
                ItemKind::Readout(text) => {
                    let size = READOUT_TEXT * s;
                    let tw = self.draw.measure(text, size, UiFont::MonoStrong);
                    shapes.push(self.text_shape(
                        [center[0] - tw / 2.0, center[1]],
                        size,
                        UiFont::MonoStrong,
                        text,
                        with_alpha([1.0; 4], alpha),
                    ));
                }
            }
        }
        shapes
    }

    /// Text whose line box is centered on `left_center[1]`.
    fn text_shape(
        &self,
        left_center: [f32; 2],
        size: f32,
        font: UiFont,
        text: &str,
        color: [f32; 4],
    ) -> Shape {
        let (ascent, descent) = self.draw.metrics(size, font);
        // Center the cap height region: ascent and descent of the font.
        let baseline = left_center[1] + (ascent + descent) / 2.0;
        Shape::Text {
            origin: [left_center[0], baseline],
            size,
            font,
            text: text.to_owned(),
            color,
        }
    }
}

/// The readout: «60 → 120 fps» while frames are generated, «60 fps»
/// without.
fn readout(state: &ToolbarState) -> String {
    let fps = state.fps.max(0.0).round();
    if state.multiplier.is_some() {
        format!("{} → {fps} fps", state.capture_fps.max(0.0).round())
    } else {
        format!("{fps} fps")
    }
}

/// The largest image of the bar (its upload buffer).
fn max_image(scale: f32) -> (u32, u32) {
    (
        (MAX_W * scale).ceil() as u32 + 2,
        ((BAR_H + BAR_TOP) * scale).ceil() as u32 + 2,
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn readout_follows_frame_generation() {
        let mut state = ToolbarState {
            fps: 119.6,
            capture_fps: 60.2,
            ..ToolbarState::default()
        };
        assert_eq!(readout(&state), "120 fps");
        state.multiplier = Some(2);
        assert_eq!(readout(&state), "60 → 120 fps");
    }

    #[test]
    fn scale_follows_the_screen() {
        assert_eq!(ui_scale((1920, 1080)), 1.0);
        assert_eq!(ui_scale((3840, 2160)), 1.5);
        assert_eq!(ui_scale((7680, 4320)), 2.0);
    }

    #[test]
    fn animation_reaches_its_end() {
        let start = Instant::now();
        let anim = Anim {
            from: 0.0,
            to: 1.0,
            start,
            motion: tokens::MOTION_FAST_SPATIAL,
        };
        assert_eq!(anim.value(start).0, 0.0);
        assert_eq!(anim.value(start + Duration::from_millis(400)), (1.0, true));
        let mid = anim.value(start + Duration::from_millis(175));
        assert!(!mid.1 && mid.0 > 0.5);
    }

    /// Draws the bar shown and pinned on a 4K screen, writes it to
    /// `toolbar-4k.png` in `$LIRTUP_TEST_OUT` or the temp folder (to look
    /// at), checks a few
    /// pixels and times its pass with Vulkan timestamps (5.8: ≤ 0.1 ms).
    #[test]
    #[ignore = "needs a Vulkan GPU"]
    fn bar_on_a_4k_screen() {
        use lirtup_gpu::{Format, Instance, InstanceOptions, SubmitOptions};

        let instance = Instance::new(&InstanceOptions {
            app_name: "lirtup-toolbar-test".into(),
            debug: std::env::var_os("LIRTUP_VULKAN_DEBUG").is_some(),
        })
        .expect("Vulkan instance");
        let gpu = std::env::var("LIRTUP_GPU").ok();
        let index = instance.select_adapter(gpu.as_deref()).expect("a Vulkan adapter");
        let device = instance.create_device(index).expect("Vulkan device");
        let (w, h) = (3840u32, 2160u32);
        let screen =
            lirtup_effects::util::texture(&device, w, h, Format::R8G8B8A8Unorm, "screen").expect("screen");
        // A mid gray game (BGRA bytes, as the capture).
        let gray: Vec<u8> = [96u8, 96, 96, 255].repeat((w * h) as usize);
        lirtup_effects::util::upload_texture(&device, &screen, &gray).expect("upload");

        let mut bar = Toolbar::new(&device, &screen).expect("toolbar");
        let start = Instant::now();
        bar.set_pinned(true, start);
        let state = ToolbarState {
            fps: 120.0,
            capture_fps: 60.0,
            multiplier: Some(2),
            auto: true,
            multiplier_label: Some("×2 Auto".to_owned()),
            stats: true,
            ..ToolbarState::default()
        };
        let cpu = Instant::now();
        assert!(
            bar.update(&state, start + Duration::from_secs(1))
                .expect("update")
        );
        let cpu_ms = cpu.elapsed().as_secs_f64() * 1000.0;
        assert!(
            !bar.update(&state, start + Duration::from_secs(1))
                .expect("update")
        );
        // A new fps (every 500 ms): the glyphs are cached.
        let cpu = Instant::now();
        let mut faster = state.clone();
        faster.fps = 121.0;
        assert!(
            bar.update(&faster, start + Duration::from_secs(1))
                .expect("update")
        );
        let cpu_again_ms = cpu.elapsed().as_secs_f64() * 1000.0;
        bar.update(&state, start + Duration::from_secs(1))
            .expect("update");
        let pass = bar.pass().expect("shown");

        let mut times = Vec::new();
        for _ in 0..50 {
            let mut recorder = device.recorder().expect("recorder");
            pass.record(&mut recorder).expect("record");
            let t = device
                .submit(
                    recorder,
                    &SubmitOptions {
                        timestamps: true,
                        ..SubmitOptions::default()
                    },
                )
                .expect("submit")
                .wait(10_000_000_000)
                .expect("wait");
            if let Some(t) = t {
                times.extend(
                    t.dispatches
                        .iter()
                        .filter(|(n, _)| n == "toolbar")
                        .map(|(_, ms)| *ms),
                );
            }
        }
        times.sort_by(f64::total_cmp);
        let median = times.get(times.len() / 2).copied().unwrap_or(f64::NAN);
        eprintln!(
            "toolbar: CPU raster {cpu_ms:.2} ms (first), {cpu_again_ms:.2} ms (new fps); GPU median {median:.4} ms ({} samples)",
            times.len()
        );
        assert!(median <= 0.1, "the bar takes {median} ms of GPU");

        // Each record blends again over the same texture: upload the gray
        // once more and blend once, to look at one blend.
        lirtup_effects::util::upload_texture(&device, &screen, &gray).expect("upload");
        let mut recorder = device.recorder().expect("recorder");
        pass.record(&mut recorder).expect("record");
        device
            .submit(recorder, &SubmitOptions::default())
            .expect("submit")
            .wait(10_000_000_000)
            .expect("wait");
        let data = screen.download().expect("download");
        let pitch = (screen.row_pitch() as usize).max(w as usize * 4);
        let px = |x: u32, y: u32| {
            let i = y as usize * pitch + x as usize * 4;
            // BGRA → RGBA.
            [data[i + 2], data[i + 1], data[i], data[i + 3]]
        };
        let [bx, by, bw, bh] = bar.rect_at(1.0);
        let mut rgba = Vec::with_capacity((bw as usize + 40) * (bh as usize + 40) * 4);
        let (x0, y0) = (bx as u32 - 20, 0u32);
        let (cw, ch) = (bw as u32 + 40, (by + bh) as u32 + 20);
        for y in y0..y0 + ch {
            for x in x0..x0 + cw {
                rgba.extend_from_slice(&px(x, y));
            }
        }
        let out = std::env::var_os("LIRTUP_TEST_OUT")
            .map_or_else(std::env::temp_dir, PathBuf::from)
            .join("toolbar-4k.png");
        image::save_buffer(&out, &rgba, cw, ch, image::ExtendedColorType::Rgba8).expect("png");
        eprintln!("toolbar image: {}", out.display());

        // Outside the bar the game is untouched.
        assert_eq!(px(10, 10), [96, 96, 96, 255]);
        // The glass over the gray, at the left padding: glass (29, 32, 36)
        // at 86 % over 96.
        let glass = px((bx + 4.0 * bar.scale) as u32, (by + bh / 2.0) as u32);
        let expect = |c: f32| (c * 219.0 / 255.0 + 96.0 * (1.0 - 219.0 / 255.0)).round() as i32;
        for (got, want) in glass[..3].iter().zip([expect(29.0), expect(32.0), expect(36.0)]) {
            assert!((i32::from(*got) - want).abs() <= 1, "glass {glass:?}");
        }
        // The pin is selected: primary under its icon's corner.
        let pin = &bar.items[0];
        let corner = px(
            (bx + pin.rect[0] + 6.0 * bar.scale) as u32,
            (by + pin.rect[1] + pin.rect[3] / 2.0) as u32,
        );
        assert_eq!(&corner[..3], &[167, 200, 255], "pin {corner:?}");
    }

    #[test]
    fn the_bar_is_not_wider_than_its_image() {
        let (w, h) = max_image(1.0);
        assert!(w as f32 >= MAX_W && h as f32 >= BAR_H + BAR_TOP);
    }
}
