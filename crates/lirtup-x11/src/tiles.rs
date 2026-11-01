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
    _private: (),
}

impl TileCache {
    /// A cache for a `width × height` frame cut in `tile_size` tiles.
    pub fn new(width: u32, height: u32, tile_size: u32) -> TileCache {
        todo!("front X")
    }

    /// Forget everything (next update reports every tile).
    pub fn reset(&mut self) {
        todo!("front X")
    }

    /// Tiles touched by `damage` whose hash changed; updates the cache.
    pub fn changed_tiles(&mut self, damage: &[DamageRect], out: &mut Vec<Tile>) {
        todo!("front X")
    }
}

/// Grow rectangles by `margin` (the tile context margin), clamp them to the
/// frame and merge overlaps.
pub fn expand_rects(rects: &[DamageRect], margin: u32, width: u32, height: u32) -> Vec<DamageRect> {
    todo!("front X")
}

/// Fraction of the frame covered by `rects` (0 to 1), to compare with
/// `area_threshold`.
pub fn dirty_fraction(rects: &[DamageRect], width: u32, height: u32) -> f32 {
    todo!("front X")
}
