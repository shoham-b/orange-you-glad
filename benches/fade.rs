use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use orange_you_glad::fade;

/// One 1080p RGB frame; this is the work done for every step of a cross-fade.
const FRAME_BYTES: usize = 1920 * 1080 * 3;

fn blend(c: &mut Criterion) {
    let from = vec![10u8; FRAME_BYTES];
    let to = vec![240u8; FRAME_BYTES];
    let mut out = vec![0u8; FRAME_BYTES];

    let mut group = c.benchmark_group("fade::blend");
    group.throughput(Throughput::Bytes(FRAME_BYTES as u64));
    group.bench_function("frame_1080p", |b| {
        b.iter(|| fade::blend(black_box(&from), black_box(&to), 128, &mut out));
    });
    group.finish();
}

criterion_group!(benches, blend);
criterion_main!(benches);
