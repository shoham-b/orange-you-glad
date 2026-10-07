//! Pure geometry: where each picture goes on the canvas.

use rand::seq::IndexedRandom;
use rand::{Rng, RngExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    /// Number of pixels covered.
    pub fn area(&self) -> u64 {
        u64::from(self.w) * u64::from(self.h)
    }
}

/// Smallest and largest share of a rectangle's long side given to the first half of a split.
const SPLIT_RANGE: (f64, f64) = (0.3, 0.7);

/// Cuts the canvas into `count` tiles of varied size: a random tile is repeatedly split
/// across its long side at a random point, so a few tiles end up big and the rest small.
/// Returns fewer tiles only if the canvas is too small to split further.
pub fn varied<R: Rng + ?Sized>(
    width: u32,
    height: u32,
    count: usize,
    gap: u32,
    rng: &mut R,
) -> Vec<Rect> {
    if count == 0 || width == 0 || height == 0 {
        return Vec::new();
    }
    let mut tiles = vec![Rect { x: 0, y: 0, w: width, h: height }];
    while tiles.len() < count {
        let Some(index) = random_splittable(&tiles, gap, rng) else { break };
        let ratio = rng.random_range(SPLIT_RANGE.0..SPLIT_RANGE.1);
        let (first, second) = split(tiles[index], gap, ratio);
        tiles[index] = first;
        tiles.push(second);
    }
    tiles
}

fn is_splittable(tile: &Rect, gap: u32) -> bool {
    tile.w.max(tile.h) >= gap.saturating_add(2)
}

fn random_splittable<R: Rng + ?Sized>(tiles: &[Rect], gap: u32, rng: &mut R) -> Option<usize> {
    let candidates: Vec<usize> =
        (0..tiles.len()).filter(|&i| is_splittable(&tiles[i], gap)).collect();
    candidates.choose(rng).copied()
}

/// Splits `tile` across its longer side, leaving `gap` pixels between the halves.
fn split(tile: Rect, gap: u32, ratio: f64) -> (Rect, Rect) {
    let wide = tile.w >= tile.h;
    let length = if wide { tile.w } else { tile.h };
    let usable = length - gap;
    let first_len = ((f64::from(usable) * ratio) as u32).clamp(1, usable - 1);
    let second_start = first_len + gap;
    let second_len = length - second_start;
    if wide {
        (Rect { w: first_len, ..tile }, Rect { x: tile.x + second_start, w: second_len, ..tile })
    } else {
        (Rect { h: first_len, ..tile }, Rect { y: tile.y + second_start, h: second_len, ..tile })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn overlaps(a: &Rect, b: &Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn produces_requested_count_inside_canvas_without_overlap() {
        let mut rng = StdRng::seed_from_u64(7);
        for count in 1..=12 {
            let tiles = varied(1920, 1080, count, 8, &mut rng);
            assert_eq!(tiles.len(), count);
            for (i, a) in tiles.iter().enumerate() {
                assert!(a.w > 0 && a.h > 0);
                assert!(a.x + a.w <= 1920 && a.y + a.h <= 1080, "{a:?} out of bounds");
                for b in &tiles[i + 1..] {
                    assert!(!overlaps(a, b), "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn without_gap_tiles_cover_the_whole_canvas() {
        let mut rng = StdRng::seed_from_u64(3);
        let tiles = varied(1920, 1080, 9, 0, &mut rng);
        assert_eq!(tiles.iter().map(Rect::area).sum::<u64>(), 1920 * 1080);
    }

    #[test]
    fn sizes_usually_vary_a_lot() {
        let clearly_varied = (0..20)
            .filter(|&seed| {
                let mut rng = StdRng::seed_from_u64(seed);
                let tiles = varied(1920, 1080, 6, 8, &mut rng);
                let areas = tiles.iter().map(Rect::area);
                areas.clone().max().unwrap() > 3 * areas.min().unwrap()
            })
            .count();
        assert!(clearly_varied >= 15, "only {clearly_varied}/20 layouts had a 3x size spread");
    }

    #[test]
    fn single_tile_fills_canvas() {
        let mut rng = StdRng::seed_from_u64(1);
        assert_eq!(varied(100, 50, 1, 8, &mut rng), vec![Rect { x: 0, y: 0, w: 100, h: 50 }]);
    }

    #[test]
    fn zero_count_is_empty() {
        let mut rng = StdRng::seed_from_u64(1);
        assert_eq!(varied(100, 50, 0, 8, &mut rng), vec![]);
    }

    #[test]
    fn tiny_canvas_stops_splitting_instead_of_panicking() {
        let mut rng = StdRng::seed_from_u64(1);
        let tiles = varied(10, 10, 50, 4, &mut rng);
        assert!(!tiles.is_empty() && tiles.len() <= 50);
    }
}
