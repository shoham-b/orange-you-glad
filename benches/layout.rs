use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use orange_you_glad::layout;
use rand::SeedableRng;
use rand::rngs::StdRng;

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

criterion_group!(benches, varied);
criterion_main!(benches);
