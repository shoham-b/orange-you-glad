use std::fs;

use criterion::{Criterion, criterion_group, criterion_main};
use orange_you_glad::library;

fn scan(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    for subject in 0..10 {
        let folder = dir.path().join(format!("subject{subject}"));
        fs::create_dir(&folder).unwrap();
        for photo in 0..200 {
            fs::write(folder.join(format!("{photo}.jpg")), b"").unwrap();
        }
    }
    c.bench_function("library::scan_10_subjects_200_files", |b| {
        b.iter(|| library::scan(dir.path()).unwrap());
    });
}

criterion_group!(benches, scan);
criterion_main!(benches);
