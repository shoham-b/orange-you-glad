use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use orange_you_glad::layout;
use rand::SeedableRng;
use rand::rngs::StdRng;

fn best_fit(c: &mut Criterion) {
    let aspects = [0.5, 2.0, 1.0, 0.75, 1.5, 1.0];
    c.bench_function("layout::best_fit_6_pictures_24_attempts_1080p", |b| {
        let mut rng = StdRng::seed_from_u64(1);
        b.iter(|| layout::best_fit(1920, 1080, black_box(&aspects), 0, 24, &mut rng));
    });
}

fn varied(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout::varied");
    for count in [6, 12, 24] {
        group.bench_function(format!("{count}_tiles_1080p"), |b| {
            let mut rng = StdRng::seed_from_u64(1);
            b.iter(|| layout::varied(1920, 1080, black_box(count), 8, &mut rng));
        });
    }
    group.finish();
}

criterion_group!(benches, varied, best_fit);
criterion_main!(benches);
