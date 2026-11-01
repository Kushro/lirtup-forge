//! Wine menu bar detection (`menubar.py`). Front S, round 2.
//!
//! With `auto_crop_menu_bar`, the session thread hands captured frames to a
//! [`MenuBarDetector`] until it decides. When it finds a bar, the session
//! thread recreates the capture with the crop and sends
//! `Event::MenuBarDetected` (the GUI saves it in the profile: the scaler
//! never writes YAML).

use std::time::{Duration, Instant};

use lirtup_config::Crop;

/// The bar and its separator fit in this many rows.
const MAX_HEIGHT: u32 = 64;
/// Rows of flat background (the menu text height).
const MIN_BAR: u32 = 10;
/// Rows of the line that closes the bar (or frame width).
const MAX_SEPARATOR: u32 = 2;
/// Sample every `STEP` pixels along a row.
const STEP: usize = 3;
/// Max channel difference from a row's median color.
const TOLERANCE: i32 = 16;
/// Share of samples near the median for a flat row.
const FLAT_SHARE: f32 = 0.9;
/// Min luma difference between bar and separator.
const SEPARATOR_CONTRAST: f32 = 24.0;
/// Min share of non-background samples in the items region.
const MIN_TEXT: f32 = 0.01;
/// Left part of the row where the menu items are.
const ITEMS_SHARE: f32 = 0.45;
/// Right part of the row that must be plain background.
const FLAT_FROM: f32 = 0.55;
/// Max channel spread of the gray separator and frame lines.
const GRAY: i32 = 12;
/// Max channel difference along a frame line.
const EDGE_TOLERANCE: i32 = 8;

/// Seconds between samples (`_MENU_BAR_INTERVAL`).
const INTERVAL: Duration = Duration::from_millis(250);
/// Samples in a row that must find the same crop (`_MENU_BAR_AGREE`).
const AGREE: usize = 3;
/// Samples before giving up (`_MENU_BAR_MAX_SAMPLES`).
const MAX_SAMPLES: usize = 12;

type Color = [i32; 3];

fn close(a: Color, b: Color, tolerance: i32) -> bool {
    a.iter().zip(b).all(|(p, q)| (p - q).abs() <= tolerance)
}

fn luma(c: Color) -> f32 {
    0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32
}

fn gray(c: Color) -> bool {
    c.iter().max().unwrap() - c.iter().min().unwrap() <= GRAY
}

/// The median color of `samples` if nearly all of them are close to it
/// (room for a cursor or an overlapping popup).
fn flat_color(samples: &[Color]) -> Option<Color> {
    if samples.is_empty() {
        return None;
    }
    let mid = samples.len() / 2;
    let mut median = [0; 3];
    for (i, m) in median.iter_mut().enumerate() {
        let mut channel: Vec<i32> = samples.iter().map(|s| s[i]).collect();
        channel.sort_unstable();
        *m = channel[mid];
    }
    let near = samples.iter().filter(|s| close(median, **s, TOLERANCE)).count();
    (near as f32 >= FLAT_SHARE * samples.len() as f32).then_some(median)
}

/// A BGRA frame with its row stride.
struct Frame<'a> {
    pixels: &'a [u8],
    width: usize,
    height: usize,
    stride: usize,
}

impl Frame<'_> {
    /// `(r, g, b)` of a pixel.
    fn px(&self, x: usize, y: usize) -> Color {
        let i = y * self.stride + x * 4;
        let p = &self.pixels[i..i + 3];
        [i32::from(p[2]), i32::from(p[1]), i32::from(p[0])]
    }

    fn flat(&self, xs: &[usize], y: usize) -> Option<Color> {
        let samples: Vec<Color> = xs.iter().map(|&x| self.px(x, y)).collect();
        flat_color(&samples)
    }
}

/// The crop of a detected menu bar (all zeros: none).
///
/// Port of `detect_menu_bar` of 1.x: from the top, at least [`MIN_BAR`]
/// rows whose right part is one flat background color and whose left part
/// holds some menu text, then 1 or 2 flat gray rows of a clearly different
/// brightness (the separator). The top crop includes the separator. When
/// the separator is the top side of a Win32 client-edge frame, the frame's
/// other sides are cropped too.
pub fn detect_menu_bar(pixels: &[u8], width: u32, height: u32, stride: u32) -> Crop {
    let none = Crop::default();
    if width < 64 || height < MAX_HEIGHT * 2 || (stride as usize) < width as usize * 4 {
        return none;
    }
    if pixels.len() < stride as usize * (height as usize - 1) + width as usize * 4 {
        return none;
    }
    let frame = Frame {
        pixels,
        width: width as usize,
        height: height as usize,
        stride: stride as usize,
    };
    let rows = MAX_HEIGHT.min(height / 4) as usize;
    let flat_xs: Vec<usize> = ((frame.width as f32 * FLAT_FROM) as usize..frame.width - 4)
        .step_by(STEP)
        .collect();
    let item_xs: Vec<usize> = (2..(frame.width as f32 * ITEMS_SHARE) as usize)
        .step_by(STEP)
        .collect();

    let Some(background) = frame.flat(&flat_xs, 0) else {
        return none;
    };

    // The bar: rows whose right part keeps the background color.
    let mut bar = 0;
    let mut text_samples = 0;
    let mut total_samples = 0;
    for y in 0..rows {
        match frame.flat(&flat_xs, y) {
            Some(color) if close(color, background, TOLERANCE) => {}
            _ => break,
        }
        text_samples += item_xs
            .iter()
            .filter(|&&x| !close(frame.px(x, y), background, 40))
            .count();
        total_samples += item_xs.len();
        bar += 1;
    }
    if bar < MIN_BAR as usize || bar >= rows {
        return none;
    }
    if total_samples == 0 || (text_samples as f32 / total_samples as f32) < MIN_TEXT {
        // A plain band with no menu items: not a menu bar.
        return none;
    }

    // The separator: at most MAX_SEPARATOR flat gray rows of a clearly
    // different brightness. More rows like that are the program's own
    // image (a dark scene).
    let mut separator: Vec<Color> = Vec::new();
    for y in bar..(bar + MAX_SEPARATOR as usize).min(rows) {
        match frame.flat(&flat_xs, y) {
            Some(color) if gray(color) && (luma(color) - luma(background)).abs() >= SEPARATOR_CONTRAST => {
                separator.push(color);
            }
            _ => break,
        }
    }
    if separator.is_empty() {
        return none;
    }
    let top = bar + separator.len();
    let edge = client_edge(&frame, top, &separator) as u32;
    Crop {
        top: top as u32,
        bottom: edge,
        left: edge,
        right: edge,
    }
}

/// Width of the client-edge frame whose top side is `separator` (0 if it is
/// not one): its left columns repeat the separator colors, and its right
/// columns and bottom rows are matching gray lines.
fn client_edge(frame: &Frame<'_>, top: usize, separator: &[Color]) -> usize {
    let e = separator.len();
    let (width, height) = (frame.width, frame.height);
    let ys: Vec<usize> = (top..height.saturating_sub(e)).step_by(STEP * 5).collect();
    let xs: Vec<usize> = (e..width.saturating_sub(e)).step_by(STEP * 5).collect();
    let line = |points: Vec<(usize, usize)>, color: Color| {
        points
            .into_iter()
            .all(|(x, y)| close(frame.px(x, y), color, EDGE_TOLERANCE))
    };
    for (i, &side) in separator.iter().enumerate() {
        // Left side: the same colors as the top side.
        if !line(ys.iter().map(|&y| (i, y)).collect(), side) {
            return 0;
        }
        // Right side and bottom: one gray color per line, the same on both.
        let color = frame.px(width - 1 - i, top);
        if !gray(color) {
            return 0;
        }
        if !line(ys.iter().map(|&y| (width - 1 - i, y)).collect(), color) {
            return 0;
        }
        if !line(xs.iter().map(|&x| (x, height - 1 - i)).collect(), color) {
            return 0;
        }
    }
    e
}

/// What the detector decided so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuBarCheck {
    /// Not this frame (between samples) or not sure yet.
    Pending,
    /// Found, the same in enough samples: crop this (added to the
    /// configured crop).
    Found(Crop),
    /// No menu bar; stop checking for this session.
    NotFound,
}

/// Samples frames over the first seconds of a session, like
/// `_check_menu_bar` of 1.x (interval, agreement of samples, give-up).
#[derive(Debug, Clone)]
pub struct MenuBarDetector {
    next_check: Instant,
    samples: Vec<Crop>,
    done: bool,
}

impl MenuBarDetector {
    /// Start detecting (call only with `auto_crop_menu_bar`).
    pub fn new(now: Instant) -> MenuBarDetector {
        MenuBarDetector {
            next_check: now,
            samples: Vec::new(),
            done: false,
        }
    }

    /// Look at one captured frame (BGRA, `stride` bytes per row).
    ///
    /// Every 250 ms; once 3 samples in a row find the same crop with a bar,
    /// that crop is [`MenuBarCheck::Found`]; after 12 samples without
    /// agreement, [`MenuBarCheck::NotFound`]. After either, it always
    /// answers the same.
    pub fn check(
        &mut self,
        now: Instant,
        pixels: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) -> MenuBarCheck {
        if self.done {
            return match self.samples.last() {
                Some(crop) if self.agreed() => MenuBarCheck::Found(*crop),
                _ => MenuBarCheck::NotFound,
            };
        }
        if now < self.next_check {
            return MenuBarCheck::Pending;
        }
        self.next_check = now + INTERVAL;
        let crop = detect_menu_bar(pixels, width, height, stride);
        self.samples.push(crop);
        if self.agreed() {
            self.done = true;
            tracing::info!(
                "menu bar detected: cropping {} more rows at the top{}",
                crop.top,
                if crop.left > 0 {
                    format!(" and a {} px frame", crop.left)
                } else {
                    String::new()
                }
            );
            return MenuBarCheck::Found(crop);
        }
        if self.samples.len() >= MAX_SAMPLES {
            self.done = true;
            tracing::info!("no menu bar found at the top of the window");
            return MenuBarCheck::NotFound;
        }
        MenuBarCheck::Pending
    }

    /// The last [`AGREE`] samples are the same bar.
    fn agreed(&self) -> bool {
        let Some(last) = self.samples.len().checked_sub(AGREE).map(|i| &self.samples[i..]) else {
            return false;
        };
        last[0].top > 0 && last.iter().all(|c| *c == last[0])
    }
}

#[cfg(test)]
mod tests {
    //! The cases of 1.x `tests/test_menubar.py`.

    use super::*;

    const WIDTH: u32 = 400;
    const HEIGHT: u32 = 300;
    const BAR: Color = [240, 240, 240];
    const SEPARATOR: Color = [160, 160, 160];
    const TEXT: Color = [20, 20, 20];

    fn frame(pixel: impl Fn(u32, u32) -> Color) -> Vec<u8> {
        let mut data = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let [r, g, b] = pixel(x, y);
                let i = ((y * WIDTH + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&[b as u8, g as u8, r as u8, 255]);
            }
        }
        data
    }

    fn detect(data: &[u8]) -> Crop {
        detect_menu_bar(data, WIDTH, HEIGHT, WIDTH * 4)
    }

    fn crop(top: u32, edge: u32) -> Crop {
        Crop {
            top,
            bottom: edge,
            left: edge,
            right: edge,
        }
    }

    fn content(x: u32, y: u32) -> Color {
        let v = ((x * 37 + y * 91) % 256) as i32;
        [v, (v * 3) % 256, (v * 7) % 256]
    }

    fn menu_text(x: u32, y: u32) -> bool {
        (4..14).contains(&y) && x < 150 && matches!(x % 9, 1 | 2)
    }

    fn with_bar(bar_rows: u32, separator_rows: u32, text: bool, blur: bool) -> impl Fn(u32, u32) -> Color {
        move |x, mut y| {
            if y < bar_rows {
                return if text && menu_text(x, y) { TEXT } else { BAR };
            }
            y -= bar_rows;
            if blur {
                if y == 0 {
                    return [200, 200, 200];
                }
                y -= 1;
            }
            if y < separator_rows {
                SEPARATOR
            } else {
                content(x, y)
            }
        }
    }

    #[test]
    fn detects_bar_and_separator() {
        assert_eq!(detect(&frame(with_bar(18, 1, true, false))), crop(19, 0));
    }

    #[test]
    fn two_row_separator_is_included() {
        assert_eq!(detect(&frame(with_bar(18, 2, true, false))), crop(20, 0));
    }

    #[test]
    fn blurred_separator_is_included() {
        assert_eq!(detect(&frame(with_bar(18, 1, true, true))), crop(20, 0));
    }

    #[test]
    fn no_bar_in_plain_content() {
        assert_eq!(detect(&frame(content)), Crop::default());
    }

    #[test]
    fn plain_band_without_items_is_not_a_menu() {
        assert_eq!(detect(&frame(with_bar(18, 1, false, false))), Crop::default());
    }

    #[test]
    fn band_too_short_is_not_a_menu() {
        assert_eq!(detect(&frame(with_bar(6, 1, true, false))), Crop::default());
    }

    #[test]
    fn tiny_frames_are_ignored() {
        assert_eq!(detect_menu_bar(&[0; 32 * 32 * 4], 32, 32, 128), Crop::default());
    }

    #[test]
    fn padded_rows_are_read_with_the_stride() {
        let tight = frame(with_bar(18, 1, true, false));
        let stride = WIDTH * 4 + 64;
        let mut padded = vec![0u8; (stride * HEIGHT) as usize];
        for y in 0..HEIGHT as usize {
            let row = (WIDTH * 4) as usize;
            padded[y * stride as usize..y * stride as usize + row]
                .copy_from_slice(&tight[y * row..(y + 1) * row]);
        }
        assert_eq!(detect_menu_bar(&padded, WIDTH, HEIGHT, stride), crop(19, 0));
    }

    const EDGE_DARK: [Color; 2] = [[166, 166, 166], [106, 106, 106]];
    const EDGE_LIGHT: [Color; 2] = [[255, 255, 255], [227, 227, 227]];
    const NIGHT: Color = [29, 19, 47];

    /// A Wine window like G-senjou no Maou: white menu bar (19 rows), then
    /// a 2 px sunken client-edge frame around the program's image.
    fn wine_window(content: impl Fn(u32, u32) -> Color) -> impl Fn(u32, u32) -> Color {
        move |x, y| {
            if y < 19 {
                return if menu_text(x, y) { TEXT } else { [255, 255, 255] };
            }
            let dark = (y - 19).min(x);
            let light = (HEIGHT - 1 - y).min(WIDTH - 1 - x);
            if light < 2 && light <= dark {
                return EDGE_LIGHT[light as usize];
            }
            if dark < 2 {
                return EDGE_DARK[dark as usize];
            }
            content(x - 2, y - 21)
        }
    }

    #[test]
    fn client_edge_frame_is_cropped_on_every_side() {
        assert_eq!(detect(&frame(wine_window(content))), crop(21, 2));
    }

    #[test]
    fn flat_dark_scene_is_not_taken_for_the_separator() {
        let scene = |x, y| if y < 4 { NIGHT } else { content(x, y) };
        assert_eq!(detect(&frame(wine_window(scene))), crop(21, 2));
    }

    #[test]
    fn separator_is_at_most_two_rows() {
        let pixel = |x, y| match y {
            0..18 if menu_text(x, y) => TEXT,
            0..18 => BAR,
            18..20 => SEPARATOR,
            20..24 => [0, 0, 0],
            _ => content(x, y),
        };
        assert_eq!(detect(&frame(pixel)), crop(20, 0));
    }

    #[test]
    fn colored_line_is_not_a_separator() {
        let pixel = |x, y| match y {
            0..18 if menu_text(x, y) => TEXT,
            0..18 => BAR,
            18 => [200, 40, 40],
            _ => content(x, y),
        };
        assert_eq!(detect(&frame(pixel)), Crop::default());
    }

    #[test]
    fn detector_needs_three_agreeing_samples() {
        let data = frame(with_bar(18, 1, true, false));
        let t0 = Instant::now();
        let mut d = MenuBarDetector::new(t0);
        let check = |d: &mut MenuBarDetector, ms: u64| {
            d.check(t0 + Duration::from_millis(ms), &data, WIDTH, HEIGHT, WIDTH * 4)
        };
        assert_eq!(check(&mut d, 0), MenuBarCheck::Pending);
        assert_eq!(check(&mut d, 100), MenuBarCheck::Pending); // between samples
        assert_eq!(check(&mut d, 250), MenuBarCheck::Pending);
        assert_eq!(check(&mut d, 500), MenuBarCheck::Found(crop(19, 0)));
        assert_eq!(check(&mut d, 750), MenuBarCheck::Found(crop(19, 0)));
    }

    #[test]
    fn detector_gives_up() {
        let data = frame(content);
        let t0 = Instant::now();
        let mut d = MenuBarDetector::new(t0);
        let mut last = MenuBarCheck::Pending;
        for i in 0..MAX_SAMPLES as u64 {
            last = d.check(t0 + INTERVAL * i as u32, &data, WIDTH, HEIGHT, WIDTH * 4);
        }
        assert_eq!(last, MenuBarCheck::NotFound);
    }
}
