//! Tile bookkeeping on the CPU (`capture/lib/tile_cache.c` and the rectangle
//! expansion of `pipeline/utils.py`). Pure logic, unit tested.

use crate::DamageRect;

/// A tile of the frame grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    /// Column.
    pub col: u32,
    /// Row.
    pub row: u32,
}

/// Remembers the hash of every tile to tell which ones really changed.
#[derive(Debug, Clone, Default)]
pub struct TileCache {
    width: u32,
    height: u32,
    tile_size: u32,
    cols: u32,
    rows: u32,
    /// Hash per tile, row-major; `None` = unknown (reported as changed).
    hashes: Vec<Option<u64>>,
}

impl TileCache {
    /// A cache for a `width × height` frame cut in `tile_size` tiles.
    pub fn new(width: u32, height: u32, tile_size: u32) -> TileCache {
        let tile_size = tile_size.max(1);
        let cols = width.div_ceil(tile_size);
        let rows = height.div_ceil(tile_size);
        TileCache {
            width,
            height,
            tile_size,
            cols,
            rows,
            hashes: vec![None; (cols * rows) as usize],
        }
    }

    /// Tile edge in pixels.
    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }

    /// Grid size `(columns, rows)`.
    pub fn grid(&self) -> (u32, u32) {
        (self.cols, self.rows)
    }

    /// Pixel rectangle of a tile (edge tiles are smaller), hash 0.
    pub fn tile_rect(&self, tile: Tile) -> DamageRect {
        let x = tile.col * self.tile_size;
        let y = tile.row * self.tile_size;
        DamageRect {
            x: x as i32,
            y: y as i32,
            width: self.tile_size.min(self.width.saturating_sub(x)) as i32,
            height: self.tile_size.min(self.height.saturating_sub(y)) as i32,
            hash: 0,
        }
    }

    /// Tiles a rectangle touches: `(col0..col1, row0..row1)`, clamped.
    fn span(&self, r: &DamageRect) -> Option<(std::ops::Range<u32>, std::ops::Range<u32>)> {
        let x0 = r.x.max(0) as u32;
        let y0 = r.y.max(0) as u32;
        let x1 = (i64::from(r.x) + i64::from(r.width)).clamp(0, i64::from(self.width)) as u32;
        let y1 = (i64::from(r.y) + i64::from(r.height)).clamp(0, i64::from(self.height)) as u32;
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let ts = self.tile_size;
        Some((x0 / ts..x1.div_ceil(ts), y0 / ts..y1.div_ceil(ts)))
    }

    /// Forget everything (next update reports every tile).
    pub fn reset(&mut self) {
        self.hashes.fill(None);
    }

    /// Tiles touched by `damage` whose hash changed; updates the cache.
    ///
    /// A rectangle that is exactly one tile and carries a hash (what
    /// [`crate::Capture::grab`] reports) is compared with the cached hash. Any
    /// other rectangle marks every tile it touches as changed and forgets
    /// their hashes. `out` is cleared first and comes out row-major, without
    /// duplicates.
    pub fn changed_tiles(&mut self, damage: &[DamageRect], out: &mut Vec<Tile>) {
        out.clear();
        let mut changed = vec![false; self.hashes.len()];
        for r in damage {
            let Some((cols, rows)) = self.span(r) else {
                continue;
            };
            let single = cols.len() == 1 && rows.len() == 1;
            for row in rows {
                for col in cols.clone() {
                    let idx = (row * self.cols + col) as usize;
                    let tile = Tile { col, row };
                    if single
                        && r.hash != 0
                        && *r
                            == (DamageRect {
                                hash: r.hash,
                                ..self.tile_rect(tile)
                            })
                    {
                        if self.hashes[idx] != Some(r.hash) {
                            self.hashes[idx] = Some(r.hash);
                            changed[idx] = true;
                        }
                    } else {
                        self.hashes[idx] = None;
                        changed[idx] = true;
                    }
                }
            }
        }
        for (idx, &c) in changed.iter().enumerate() {
            if c {
                let idx = idx as u32;
                out.push(Tile {
                    col: idx % self.cols,
                    row: idx / self.cols,
                });
            }
        }
    }
}

/// Grow rectangles by `margin` (the tile context margin), clamp them to the
/// frame and merge overlaps.
///
/// Merged rectangles are the bounding box of the overlapping ones (hash 0);
/// one that did not merge keeps no hash either, since its pixels changed.
pub fn expand_rects(rects: &[DamageRect], margin: u32, width: u32, height: u32) -> Vec<DamageRect> {
    let m = i64::from(margin);
    let (w, h) = (i64::from(width), i64::from(height));
    // (x0, y0, x1, y1), exclusive ends.
    let mut boxes: Vec<[i64; 4]> = rects
        .iter()
        .filter(|r| r.width > 0 && r.height > 0)
        .filter_map(|r| {
            let x0 = (i64::from(r.x) - m).max(0);
            let y0 = (i64::from(r.y) - m).max(0);
            let x1 = (i64::from(r.x) + i64::from(r.width) + m).min(w);
            let y1 = (i64::from(r.y) + i64::from(r.height) + m).min(h);
            (x1 > x0 && y1 > y0).then_some([x0, y0, x1, y1])
        })
        .collect();
    // Merge until no two overlap (a merge can create new overlaps).
    let mut merged = true;
    while merged {
        merged = false;
        let mut i = 0;
        while i < boxes.len() {
            let mut j = i + 1;
            while j < boxes.len() {
                let (a, b) = (boxes[i], boxes[j]);
                if a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3] {
                    boxes[i] = [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])];
                    boxes.swap_remove(j);
                    merged = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
    boxes
        .into_iter()
        .map(|[x0, y0, x1, y1]| DamageRect {
            x: x0 as i32,
            y: y0 as i32,
            width: (x1 - x0) as i32,
            height: (y1 - y0) as i32,
            hash: 0,
        })
        .collect()
}

/// Fraction of the frame covered by `rects` (0 to 1), to compare with
/// `area_threshold`.
///
/// Overlapping rectangles count once (exact union area).
pub fn dirty_fraction(rects: &[DamageRect], width: u32, height: u32) -> f32 {
    let total = u64::from(width) * u64::from(height);
    if total == 0 {
        return 0.0;
    }
    let (w, h) = (i64::from(width), i64::from(height));
    let boxes: Vec<[i64; 4]> = rects
        .iter()
        .filter_map(|r| {
            let x0 = i64::from(r.x).clamp(0, w);
            let y0 = i64::from(r.y).clamp(0, h);
            let x1 = (i64::from(r.x) + i64::from(r.width.max(0))).clamp(0, w);
            let y1 = (i64::from(r.y) + i64::from(r.height.max(0))).clamp(0, h);
            (x1 > x0 && y1 > y0).then_some([x0, y0, x1, y1])
        })
        .collect();
    // Sweep over the distinct x edges; in each strip, union the y intervals.
    let mut xs: Vec<i64> = boxes.iter().flat_map(|b| [b[0], b[2]]).collect();
    xs.sort_unstable();
    xs.dedup();
    let mut area: u64 = 0;
    let mut spans: Vec<(i64, i64)> = Vec::new();
    for pair in xs.windows(2) {
        let (sx0, sx1) = (pair[0], pair[1]);
        spans.clear();
        spans.extend(
            boxes
                .iter()
                .filter(|b| b[0] <= sx0 && b[2] >= sx1)
                .map(|b| (b[1], b[3])),
        );
        spans.sort_unstable();
        let mut covered = 0i64;
        let mut cur: Option<(i64, i64)> = None;
        for &(a, b) in &spans {
            match cur {
                Some((ca, cb)) if a <= cb => cur = Some((ca, cb.max(b))),
                Some((ca, cb)) => {
                    covered += cb - ca;
                    cur = Some((a, b));
                }
                None => cur = Some((a, b)),
            }
        }
        if let Some((ca, cb)) = cur {
            covered += cb - ca;
        }
        area += (covered * (sx1 - sx0)) as u64;
    }
    (area as f64 / total as f64).min(1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, width: i32, height: i32) -> DamageRect {
        DamageRect {
            x,
            y,
            width,
            height,
            hash: 0,
        }
    }

    #[test]
    fn cache_reports_new_tiles_then_only_changes() {
        let mut cache = TileCache::new(100, 70, 32);
        assert_eq!(cache.grid(), (4, 3));
        assert_eq!(cache.tile_rect(Tile { col: 3, row: 2 }), r(96, 64, 4, 6));

        let t = Tile { col: 1, row: 0 };
        let mut out = Vec::new();
        cache.changed_tiles(
            &[DamageRect {
                hash: 7,
                ..cache.tile_rect(t)
            }],
            &mut out,
        );
        assert_eq!(out, vec![t]);
        cache.changed_tiles(
            &[DamageRect {
                hash: 7,
                ..cache.tile_rect(t)
            }],
            &mut out,
        );
        assert!(out.is_empty());
        cache.changed_tiles(
            &[DamageRect {
                hash: 8,
                ..cache.tile_rect(t)
            }],
            &mut out,
        );
        assert_eq!(out, vec![t]);
        cache.reset();
        cache.changed_tiles(
            &[DamageRect {
                hash: 8,
                ..cache.tile_rect(t)
            }],
            &mut out,
        );
        assert_eq!(out, vec![t]);
    }

    #[test]
    fn cache_marks_every_touched_tile_for_free_rects() {
        let mut cache = TileCache::new(100, 70, 32);
        let mut out = Vec::new();
        cache.changed_tiles(&[r(30, 30, 4, 4), r(31, 31, 1, 1)], &mut out);
        assert_eq!(
            out,
            vec![
                Tile { col: 0, row: 0 },
                Tile { col: 1, row: 0 },
                Tile { col: 0, row: 1 },
                Tile { col: 1, row: 1 }
            ]
        );
        cache.changed_tiles(&[r(-10, -10, 5, 5), r(200, 0, 5, 5)], &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn expand_clamps_and_merges() {
        let out = expand_rects(&[r(0, 0, 10, 10), r(15, 0, 10, 10), r(80, 80, 5, 5)], 4, 90, 90);
        assert_eq!(out.len(), 2);
        assert!(out.contains(&r(0, 0, 29, 14)));
        assert!(out.contains(&r(76, 76, 13, 13)));
        assert!(expand_rects(&[r(0, 0, 0, 5)], 4, 90, 90).is_empty());
    }

    #[test]
    fn expand_merges_transitively() {
        // a and c only overlap after a and b merged.
        let out = expand_rects(&[r(0, 0, 10, 10), r(8, 8, 10, 10), r(0, 15, 4, 4)], 0, 100, 100);
        assert_eq!(out, vec![r(0, 0, 18, 19)]);
    }

    #[test]
    fn fraction_counts_overlap_once() {
        assert_eq!(dirty_fraction(&[], 10, 10), 0.0);
        assert_eq!(dirty_fraction(&[r(0, 0, 10, 10)], 10, 10), 1.0);
        assert_eq!(dirty_fraction(&[r(0, 0, 5, 10), r(0, 0, 5, 10)], 10, 10), 0.5);
        assert_eq!(dirty_fraction(&[r(0, 0, 6, 10), r(4, 0, 6, 5)], 10, 10), 0.8);
        assert_eq!(dirty_fraction(&[r(-5, -5, 10, 10)], 10, 10), 0.25);
        assert_eq!(dirty_fraction(&[r(0, 0, 1, 1)], 0, 10), 0.0);
    }
}
