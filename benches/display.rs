use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use image::{Rgb, RgbImage};
use orange_you_glad::display::Geometry;

fn encode(c: &mut Criterion) {
    let frame = RgbImage::from_fn(1920, 1080, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 7]));
    let mut group = c.benchmark_group("display::encode_into");
    group.throughput(Throughput::Elements(1920 * 1080));
    for bits_per_pixel in [32, 16] {
        let geometry = Geometry {
            width: 1920,
            height: 1080,
            stride: 1920 * bits_per_pixel / 8,
            bits_per_pixel,
        };
        let mut out = Vec::with_capacity(geometry.frame_len());
        group.bench_function(format!("{bits_per_pixel}bpp_1080p"), |b| {
            b.iter(|| geometry.encode_into(black_box(&frame), &mut out).unwrap());
        });
    }
    group.finish();
}

criterion_group!(benches, encode);
criterion_main!(benches);
