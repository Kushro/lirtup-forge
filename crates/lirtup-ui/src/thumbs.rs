//! Live previews and window icons (front B, 6.4 of the analysis).
//!
//! A thread captures each visible window with XComposite
//! (`NameWindowPixmap` + `GetImage`), scales it to 320 px wide, encodes it
//! (WebP or JPEG) in memory and keeps `xid → (version, bytes)`.
//! `use_asset_handler("thumb", …)` serves `/thumb/{xid}?v={n}` and
//! `use_asset_handler("icon", …)` serves `/icon/{xid}` (`_NET_WM_ICON`).
//! It pauses while the main window is hidden or minimized.

use std::collections::HashMap;

use dioxus::prelude::*;

/// The previews' versions, so a tile's URL changes when its image does.
/// A context (`use_context::<Thumbs>()`).
#[derive(Clone, Copy, PartialEq)]
pub struct Thumbs {
    /// `xid → version`.
    pub versions: Signal<HashMap<u32, u64>>,
}

impl Thumbs {
    /// `/thumb/{xid}?v={n}`, once a preview exists.
    pub fn url(&self, xid: u32) -> Option<String> {
        self.versions
            .read()
            .get(&xid)
            .map(|v| format!("/thumb/{xid}?v={v}"))
    }

    /// `/icon/{xid}`.
    pub fn icon_url(&self, xid: u32) -> String {
        format!("/icon/{xid}")
    }
}

/// Register the asset handlers and start the capture thread (called once
/// by `app.rs`, which provides the result as a context). Until front B
/// writes it, there are no previews.
pub fn use_thumbnails() -> Thumbs {
    Thumbs {
        versions: use_signal(HashMap::new),
    }
}
