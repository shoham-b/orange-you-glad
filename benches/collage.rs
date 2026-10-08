use std::hint::black_box;
use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use image::{Rgb, RgbImage};
use orange_you_glad::{collage, layout};
use rand::SeedableRng;
use rand::rngs::StdRng;

/// Roughly a phone photo; decoding and shrinking these dominates a real collage.
const PHOTO_SIZE: (u32, u32) = (4000, 3000);

fn build(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let photos: Vec<PathBuf> = (0..6u8)
        .map(|i| {
            let path = dir.path().join(format!("photo{i}.jpg"));
            let photo = RgbImage::from_fn(PHOTO_SIZE.0, PHOTO_SIZE.1, |x, y| {
                Rgb([(x / 16) as u8, (y / 12) as u8, i * 40])
            });
            photo.save(&path).unwrap();
            path
        })
        .collect();
    let mut rng = StdRng::seed_from_u64(1);
    let tiles = layout::varied(1920, 1080, photos.len(), 8, &mut rng);

    let mut group = c.benchmark_group("collage::build");
    group.sample_size(10);
    group.bench_function("six_12mp_jpegs_1080p", |b| {
        b.iter(|| collage::build(1920, 1080, [0, 0, 0], black_box(&tiles), black_box(&photos)));
    });
    group.finish();
}

criterion_group!(benches, build);
criterion_main!(benches);
