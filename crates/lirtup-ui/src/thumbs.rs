//! Live previews and window icons (front B, 6.4 of the analysis).
//!
//! A thread captures each listed window with XComposite
//! (`NameWindowPixmap` + `GetImage`, `X11::thumbnail`), scales it to 320 px
//! wide, encodes it as JPEG in memory and keeps `xid → bytes`; a preview
//! whose pixels did not change keeps its version. `use_asset_handler("thumb",
//! …)` serves `/thumb/{xid}?v={n}` and `use_asset_handler("icon", …)` serves
//! `/icon/{xid}` (`_NET_WM_ICON` as PNG).
//!
//! The capture pauses while the main window is hidden or minimized: tao's
//! window events (`use_wry_event_handler`) and a slow check of the window
//! state set it, and the store hears it as `Action::WindowVisible`.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dioxus::desktop::tao::event::{Event as TaoEvent, WindowEvent};
use dioxus::desktop::wry::http::{Response, StatusCode};
use dioxus::desktop::{AssetRequest, RequestAsyncResponder, use_window, use_wry_event_handler};
use dioxus::prelude::*;
use lirtup_x11::{RgbaImage, X11};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::state::{Action, Store};

/// Width of a preview.
pub const THUMB_WIDTH: u32 = 320;
/// Side of a window icon.
pub const ICON_SIZE: u32 = 64;
/// JPEG quality of the previews.
const JPEG_QUALITY: u8 = 80;
/// Bounds of `tile_preview_interval_ms`.
const MIN_INTERVAL_MS: u32 = 100;
const MAX_INTERVAL_MS: u32 = 60_000;
/// How often the window state is checked besides tao's events (minimizing
/// does not always send one).
const VISIBILITY_CHECK: Duration = Duration::from_secs(1);

/// The previews' versions, so a tile's URL changes when its image does.
/// A context (`use_context::<Thumbs>()`).
#[derive(Clone, Copy, PartialEq)]
pub struct Thumbs {
    /// `xid → version`.
    pub versions: Signal<HashMap<u32, u64>>,
    /// Windows with an icon (`_NET_WM_ICON`), once read.
    pub icons: Signal<HashSet<u32>>,
}

impl Thumbs {
    /// `/thumb/{xid}?v={n}`, once a preview exists.
    pub fn url(&self, xid: u32) -> Option<String> {
        self.versions
            .read()
            .get(&xid)
            .map(|v| format!("/thumb/{xid}?v={v}"))
    }

    /// `/icon/{xid}`. Served when the window has an icon (404 otherwise);
    /// [`Thumbs::icon`] says whether it has one.
    pub fn icon_url(&self, xid: u32) -> String {
        format!("/icon/{xid}")
    }

    /// `/icon/{xid}` when the window is known to have an icon.
    pub fn icon(&self, xid: u32) -> Option<String> {
        self.icons.read().contains(&xid).then(|| self.icon_url(xid))
    }
}

/// What the capture thread shares with the asset handlers.
#[derive(Default)]
struct Cache {
    /// JPEG of each preview.
    thumbs: HashMap<u32, Arc<Vec<u8>>>,
    /// PNG of each icon (`None`: the window has none).
    icons: HashMap<u32, Option<Arc<Vec<u8>>>>,
}

/// Settings the UI changes while the thread runs.
struct Control {
    /// Windows to capture (the store's list).
    targets: Mutex<Vec<u32>>,
    /// The main window is hidden or minimized.
    paused: AtomicBool,
    /// `tile_preview_interval_ms`.
    interval_ms: AtomicU32,
}

/// Requests to the thread.
enum Request {
    /// Read an icon that is not cached and answer.
    Icon(u32, RequestAsyncResponder),
    /// The targets changed: capture now.
    Refresh,
}

/// What the thread tells the UI.
#[derive(Debug, Default, PartialEq)]
struct Update {
    versions: HashMap<u32, u64>,
    icons: HashSet<u32>,
}

/// Register the asset handlers and start the capture thread (called once
/// by `app.rs`, which provides the result as a context).
pub fn use_thumbnails() -> Thumbs {
    let thumbs = Thumbs {
        versions: use_signal(HashMap::new),
        icons: use_signal(HashSet::new),
    };
    let cache = use_hook(|| Arc::new(Mutex::new(Cache::default())));
    let control = use_hook(|| {
        Arc::new(Control {
            targets: Mutex::new(Vec::new()),
            paused: AtomicBool::new(false),
            interval_ms: AtomicU32::new(500),
        })
    });
    let requests = use_hook(|| {
        let (tx, rx) = mpsc::channel::<Request>();
        let (updates_tx, mut updates_rx) = unbounded_channel::<Update>();
        let (cache, control) = (cache.clone(), control.clone());
        let spawned = std::thread::Builder::new()
            .name("lirtup-thumbs".into())
            .spawn(move || run(&rx, &updates_tx, &cache, &control));
        if let Err(e) = spawned {
            tracing::error!("thumbnail thread: {e}");
        }
        let (mut versions, mut icons) = (thumbs.versions, thumbs.icons);
        spawn(async move {
            while let Some(update) = updates_rx.recv().await {
                if *versions.peek() != update.versions {
                    versions.set(update.versions);
                }
                if *icons.peek() != update.icons {
                    icons.set(update.icons);
                }
            }
        });
        tx
    });

    // `/thumb/{xid}`: from the cache, never blocking.
    let thumb_cache = cache.clone();
    dioxus::desktop::use_asset_handler("thumb", move |request: AssetRequest, responder| {
        let bytes = xid_of(&request).and_then(|xid| thumb_cache.lock().ok()?.thumbs.get(&xid).cloned());
        responder.respond(image_response(bytes, "image/jpeg"));
    });
    // `/icon/{xid}`: from the cache, else the thread reads it.
    let icon_cache = cache.clone();
    let icon_requests = requests.clone();
    dioxus::desktop::use_asset_handler("icon", move |request: AssetRequest, responder| {
        let Some(xid) = xid_of(&request) else {
            return responder.respond(image_response(None, "image/png"));
        };
        let cached = icon_cache.lock().ok().and_then(|c| c.icons.get(&xid).cloned());
        match cached {
            Some(bytes) => responder.respond(image_response(bytes, "image/png")),
            None => {
                if let Err(mpsc::SendError(Request::Icon(_, responder))) =
                    icon_requests.send(Request::Icon(xid, responder))
                {
                    responder.respond(image_response(None, "image/png"));
                }
            }
        }
    });

    // Targets and interval follow the store.
    let store = try_use_context::<Signal<Store>>();
    let follow = control.clone();
    let refresh = requests.clone();
    use_effect(move || {
        let Some(store) = store else { return };
        let s = store.read();
        let targets: Vec<u32> = s.windows.iter().map(|w| w.xid).collect();
        follow.interval_ms.store(
            s.gui
                .tile_preview_interval_ms
                .clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS),
            Ordering::Relaxed,
        );
        drop(s);
        let changed = follow.targets.lock().is_ok_and(|mut t| {
            let changed = *t != targets;
            *t = targets;
            changed
        });
        if changed {
            let _ = refresh.send(Request::Refresh);
        }
    });

    // Pause while hidden or minimized. tao's events update the pause at
    // once; the slow check also catches a minimize without an event and
    // tells the store.
    let window = use_window();
    let check = {
        let control = control.clone();
        let requests = requests.clone();
        move || {
            let shown = window.window.is_visible() && !window.window.is_minimized();
            let was_paused = control.paused.swap(!shown, Ordering::Relaxed);
            if was_paused && shown {
                let _ = requests.send(Request::Refresh);
            }
            shown
        }
    };
    let on_event = check.clone();
    use_wry_event_handler(move |event, _| {
        if let TaoEvent::WindowEvent {
            event: WindowEvent::Focused(_) | WindowEvent::Resized(_) | WindowEvent::Moved(_),
            ..
        } = event
        {
            on_event();
        }
    });
    let dispatch = try_use_context::<Callback<Action>>();
    use_hook(move || {
        spawn(async move {
            loop {
                let shown = check();
                if let (Some(store), Some(dispatch)) = (store, dispatch)
                    && store.peek().visible != shown
                {
                    dispatch(Action::WindowVisible(shown));
                }
                tokio::time::sleep(VISIBILITY_CHECK).await;
            }
        });
    });

    thumbs
}

/// The thread: a capture round every interval, icon requests in between.
fn run(
    requests: &mpsc::Receiver<Request>,
    updates: &UnboundedSender<Update>,
    cache: &Mutex<Cache>,
    control: &Control,
) {
    let x11 = match X11::connect() {
        Ok(x11) => x11,
        Err(e) => {
            tracing::warn!("thumbnails: no X connection: {e}");
            // Still answer icon requests, with 404.
            while let Ok(request) = requests.recv() {
                if let Request::Icon(_, responder) = request {
                    responder.respond(image_response(None, "image/png"));
                }
            }
            return;
        }
    };
    if let Err(e) = x11.enable_thumbnails() {
        // Without Composite, covered windows show what covers them.
        tracing::warn!("thumbnails: {e}");
    }
    let mut state = Capturer::default();
    let mut next = Instant::now();
    loop {
        let wait = next.saturating_duration_since(Instant::now());
        match requests.recv_timeout(wait) {
            Ok(Request::Icon(xid, responder)) => {
                let bytes = read_icon(&x11, xid);
                if let Ok(mut c) = cache.lock() {
                    c.icons.insert(xid, bytes.clone());
                }
                responder.respond(image_response(bytes, "image/png"));
                continue;
            }
            Ok(Request::Refresh) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if updates.is_closed() {
            return;
        }
        let targets = control.targets.lock().map(|t| t.clone()).unwrap_or_default();
        let update = state.round(&x11, &targets, cache, control.paused.load(Ordering::Relaxed));
        if let Some(update) = update
            && updates.send(update).is_err()
        {
            return;
        }
        let interval = control.interval_ms.load(Ordering::Relaxed);
        next = Instant::now() + Duration::from_millis(u64::from(interval));
    }
}

/// The thread's memory between rounds.
#[derive(Default)]
struct Capturer {
    /// `xid → (hash of the pixels, version)`.
    seen: HashMap<u32, (u64, u64)>,
    /// The last update sent.
    last: Update,
}

impl Capturer {
    /// Read the icons of new windows and, unless paused, capture every
    /// target. Returns an update when something changed for the UI.
    fn round(&mut self, x11: &X11, targets: &[u32], cache: &Mutex<Cache>, paused: bool) -> Option<Update> {
        let alive: HashSet<u32> = targets.iter().copied().collect();
        let mut new_icons = Vec::new();
        let mut new_thumbs = Vec::new();
        {
            let Ok(c) = cache.lock() else { return None };
            for &xid in targets {
                if !c.icons.contains_key(&xid) {
                    new_icons.push(xid);
                }
            }
        }
        let icons: Vec<(u32, Option<Arc<Vec<u8>>>)> = new_icons
            .into_iter()
            .map(|xid| (xid, read_icon(x11, xid)))
            .collect();
        if !paused {
            for &xid in targets {
                let image = match x11.thumbnail(xid, THUMB_WIDTH) {
                    Ok(Some(image)) => image,
                    Ok(None) => continue,
                    Err(e) => {
                        tracing::debug!("thumbnail of 0x{xid:x}: {e}");
                        continue;
                    }
                };
                let hash = pixel_hash(&image);
                let version = match self.seen.get(&xid) {
                    Some(&(old, version)) if old == hash => {
                        self.seen.insert(xid, (hash, version));
                        continue;
                    }
                    Some(&(_, version)) => version + 1,
                    None => 1,
                };
                match encode_jpeg(&image) {
                    Ok(bytes) => {
                        self.seen.insert(xid, (hash, version));
                        new_thumbs.push((xid, Arc::new(bytes)));
                    }
                    Err(e) => tracing::debug!("thumbnail of 0x{xid:x}: {e}"),
                }
            }
        }
        self.seen.retain(|xid, _| alive.contains(xid));

        let Ok(mut c) = cache.lock() else { return None };
        c.icons.extend(icons);
        c.thumbs.extend(new_thumbs);
        c.thumbs.retain(|xid, _| alive.contains(xid));
        c.icons.retain(|xid, _| alive.contains(xid));
        let update = Update {
            versions: self
                .seen
                .iter()
                .filter(|(xid, _)| c.thumbs.contains_key(xid))
                .map(|(&xid, &(_, v))| (xid, v))
                .collect(),
            icons: c
                .icons
                .iter()
                .filter(|(_, bytes)| bytes.is_some())
                .map(|(&xid, _)| xid)
                .collect(),
        };
        drop(c);
        if update == self.last {
            return None;
        }
        self.last = Update {
            versions: update.versions.clone(),
            icons: update.icons.clone(),
        };
        Some(update)
    }
}

/// `_NET_WM_ICON` of a window as PNG.
fn read_icon(x11: &X11, xid: u32) -> Option<Arc<Vec<u8>>> {
    let icon = match x11.window_icon(xid, ICON_SIZE) {
        Ok(icon) => icon?,
        Err(e) => {
            tracing::debug!("icon of 0x{xid:x}: {e}");
            return None;
        }
    };
    match encode_png(&icon) {
        Ok(bytes) => Some(Arc::new(bytes)),
        Err(e) => {
            tracing::debug!("icon of 0x{xid:x}: {e}");
            None
        }
    }
}

/// A preview as JPEG (opaque: alpha is dropped).
pub fn encode_jpeg(image: &RgbaImage) -> image::ImageResult<Vec<u8>> {
    let rgb: Vec<u8> = image
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|px| [px[0], px[1], px[2]])
        .collect();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode(
        &rgb,
        image.width,
        image.height,
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(out)
}

/// An icon as PNG (keeps alpha).
pub fn encode_png(image: &RgbaImage) -> image::ImageResult<Vec<u8>> {
    use image::ImageEncoder as _;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out).write_image(
        &image.data,
        image.width,
        image.height,
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(out)
}

fn pixel_hash(image: &RgbaImage) -> u64 {
    let mut hasher = DefaultHasher::new();
    (image.width, image.height).hash(&mut hasher);
    image.data.hash(&mut hasher);
    hasher.finish()
}

/// The xid of `/thumb/{xid}` or `/icon/{xid}` (decimal, or `0x` hex).
fn xid_of(request: &AssetRequest) -> Option<u32> {
    parse_xid(request.uri().path())
}

fn parse_xid(path: &str) -> Option<u32> {
    let last = path.trim_end_matches('/').rsplit('/').next()?;
    match last.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => last.parse().ok(),
    }
}

fn image_response(bytes: Option<Arc<Vec<u8>>>, mime: &str) -> Response<Vec<u8>> {
    let builder = Response::builder();
    let response = match bytes {
        Some(bytes) => builder
            .status(StatusCode::OK)
            .header("Content-Type", mime)
            // The URL carries the version.
            .header("Cache-Control", "max-age=3600")
            .body(bytes.as_ref().clone()),
        None => builder
            .status(StatusCode::NOT_FOUND)
            .header("Cache-Control", "no-store")
            .body(Vec::new()),
    };
    response.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: u32, height: u32) -> RgbaImage {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                data.extend_from_slice(&[(x * 255 / width) as u8, (y * 255 / height) as u8, 128, 255]);
            }
        }
        RgbaImage { width, height, data }
    }

    #[test]
    fn xids_from_paths() {
        assert_eq!(parse_xid("/thumb/1234"), Some(1234));
        assert_eq!(parse_xid("/icon/0x4a00007"), Some(0x4a0_0007));
        assert_eq!(parse_xid("/thumb/1234/"), Some(1234));
        assert_eq!(parse_xid("/thumb/"), None);
        assert_eq!(parse_xid("/thumb/abc"), None);
    }

    #[test]
    fn encodes_jpeg_and_png() {
        let image = gradient(64, 48);
        let jpeg = encode_jpeg(&image).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 48));
        let png = encode_png(&image).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.as_raw(), &image.data);
    }

    #[test]
    fn same_pixels_same_hash() {
        let a = gradient(8, 8);
        let mut b = a.clone();
        assert_eq!(pixel_hash(&a), pixel_hash(&b));
        b.data[0] ^= 1;
        assert_ne!(pixel_hash(&a), pixel_hash(&b));
    }

    /// Against the real X server, when there is one: a round over the
    /// listed windows gives JPEG previews at most 320 px wide, and a second
    /// round of unchanged windows sends no update.
    #[test]
    fn real_round() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let Ok(x11) = X11::connect() else { return };
        let _ = x11.enable_thumbnails();
        let targets: Vec<u32> = x11
            .list_windows()
            .expect("list_windows")
            .iter()
            .map(|w| w.xid)
            .take(4)
            .collect();
        if targets.is_empty() {
            return;
        }
        let cache = Mutex::new(Cache::default());
        let mut capturer = Capturer::default();
        let update = capturer
            .round(&x11, &targets, &cache, false)
            .expect("first round updates");
        let c = cache.lock().unwrap();
        eprintln!(
            "{} windows, {} previews, {} icons",
            targets.len(),
            c.thumbs.len(),
            c.icons.len()
        );
        for (xid, bytes) in &c.thumbs {
            assert!(update.versions.contains_key(xid));
            let image = image::load_from_memory(bytes).unwrap();
            assert!(
                image.width() <= THUMB_WIDTH,
                "0x{xid:x} is {} wide",
                image.width()
            );
        }
        assert!(c.icons.keys().all(|xid| targets.contains(xid)));
        drop(c);
        // Paused: no capture, so nothing new.
        assert!(capturer.round(&x11, &targets, &cache, true).is_none());
    }
}
