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
//! - In windowed mode, dragging the bar moves the window
//!   (`OverlayCommand::BeginMove`).
//! - Changing the multiplier applies it live and sends
//!   `Event::SaveSetting` so the GUI keeps it.
//!
//! Criterion (5.8): drawing it changes the GPU time of a frame by at most
//! 0.1 ms. The session thread owns it, next to the OSD and the panel.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lirtup_effects::Pass;
use lirtup_gpu::{Device, Texture};
use lirtup_x11::Rect;
use lirtup_x11::overlay_input::{PointerEvent, ToolbarZone};

use crate::Result;
use crate::tokens::Scheme;

/// Hide this long after the pointer leaves the bar.
pub const HIDE_AFTER: Duration = Duration::from_secs(2);

/// What a button of the bar does. The session thread turns each into what
/// a hotkey does (`Screenshot`, `ToggleStats`, …) or into its own action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolbarAction {
    /// Keep the bar shown.
    Pin,
    /// Show or hide the stats panel.
    Stats,
    /// Crop: the menu bar detection again, or the crop of the profile.
    Crop,
    /// Save a screenshot.
    Screenshot,
    /// Next frame generation multiplier (Off, ×2, ×3, ×4, Auto).
    Multiplier,
    /// Hide the bar (until the pointer comes back to the edge).
    Hide,
    /// Fullscreen ↔ windowed.
    ToWindow,
    /// Stop the session.
    Stop,
}

/// What the bar shows; the session thread fills it every frame it changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolbarState {
    /// Shown fps (the readout).
    pub fps: f32,
    /// Frame generation multiplier in use (`None` = off).
    pub multiplier: Option<u8>,
    /// The configured multiplier is Auto.
    pub auto: bool,
    /// The stats panel is visible.
    pub stats: bool,
    /// The session is windowed (the to-window button shows «fullscreen»).
    pub windowed: bool,
    /// Theme of the panel and the OSD.
    pub scheme: Scheme,
}

/// The bar.
pub struct Toolbar {
    _device: Arc<Device>,
}

impl Toolbar {
    /// Create the bar for a screen texture (the presenter's `screen`).
    pub fn new(device: &Arc<Device>, screen: &Texture) -> Result<Toolbar> {
        let _ = (device, screen);
        todo!()
    }

    /// The screen texture changed (resize).
    pub fn rebind(&mut self, device: &Arc<Device>, screen: &Texture) -> Result<()> {
        let _ = (device, screen);
        todo!()
    }

    /// Where the bar takes the pointer, in overlay coordinates, for
    /// `OverlayCommand::SetToolbarZone` (`None` when the bar is off).
    pub fn zone(&self, overlay: Rect) -> Option<ToolbarZone> {
        let _ = overlay;
        todo!()
    }

    /// A pointer event over the bar or its reveal band; returns the action
    /// of a click.
    pub fn pointer(&mut self, event: PointerEvent, now: Instant) -> Option<ToolbarAction> {
        let _ = (event, now);
        todo!()
    }

    /// Show or hide it (`ToggleToolbar`, the hide button).
    pub fn set_shown(&mut self, shown: bool, now: Instant) {
        let _ = (shown, now);
        todo!()
    }

    /// Advance the animation and redraw if the state changed; true when the
    /// screen must be presented again.
    pub fn update(&mut self, state: &ToolbarState, now: Instant) -> Result<bool> {
        let _ = (state, now);
        todo!()
    }

    /// The pass that blends it onto the screen (`None` while hidden).
    pub fn pass(&self) -> Option<&dyn Pass> {
        todo!()
    }
}
