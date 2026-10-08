//! Heap budget for building a collage. The target board has 1 GB of RAM, so memory is the
//! constraint that matters. Kept in its own test binary because `dhat` allows one profiler
//! per process and replaces the global allocator.

use image::{Rgb, RgbImage};
use orange_you_glad::{collage, layout};
use rand::SeedableRng;
use rand::rngs::StdRng;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// A 12 MP RGB photo decodes to 36 MB; decoder scratch space, the canvas and a tile come on top
/// (98 MB measured with two large tiles; the resize scratch grows with tile size). Only one photo is
/// alive at a time, so the peak does not grow with the number of tiles.
const PEAK_BUDGET_BYTES: u64 = 128 * 1024 * 1024;

#[test]
fn building_a_collage_of_12mp_photos_stays_within_budget() {
    let dir = tempfile::tempdir().unwrap();
    let photos: Vec<_> = (0..2u8)
        .map(|i| {
            let path = dir.path().join(format!("{i}.jpg"));
            RgbImage::from_fn(4000, 3000, |x, y| Rgb([(x / 16) as u8, (y / 12) as u8, i * 60]))
                .save(&path)
                .unwrap();
            path
        })
        .collect();
    let tiles = layout::varied(1920, 1080, photos.len(), 8, &mut StdRng::seed_from_u64(1));

    let _profiler = dhat::Profiler::builder().testing().build();
    let (canvas, _) = collage::build(1920, 1080, [0, 0, 0], &tiles, &photos);
    let stats = dhat::HeapStats::get();

    assert_eq!(canvas.dimensions(), (1920, 1080));
    assert!(
        stats.max_bytes as u64 <= PEAK_BUDGET_BYTES,
        "peak heap {} MB exceeds budget of {} MB",
        stats.max_bytes / (1024 * 1024),
        PEAK_BUDGET_BYTES / (1024 * 1024)
    );
}
