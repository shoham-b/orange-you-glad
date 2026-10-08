use std::fmt::Write as _;
use std::fs;

use criterion::{Criterion, criterion_group, criterion_main};
use orange_you_glad::library;

fn scan(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("Config")).unwrap();
    let processed = dir.path().join("Processed");
    fs::create_dir(&processed).unwrap();
    let mut json = String::from("{");
    for subject in 0..10 {
        let uuids: Vec<String> = (0..200).map(|photo| format!("{subject:02}-{photo:03}")).collect();
        for uuid in &uuids {
            fs::write(processed.join(format!("{uuid}.jpg")), b"").unwrap();
        }
        let list = uuids.iter().map(|u| format!("\"{u}\"")).collect::<Vec<_>>().join(",");
        write!(json, "{}\"subject{subject}\":[{list}]", if subject > 0 { "," } else { "" })
            .unwrap();
    }
    json.push('}');
    fs::write(dir.path().join("Config/subjects.json"), json).unwrap();
    c.bench_function("library::scan_10_subjects_200_files", |b| {
        b.iter(|| library::scan(dir.path()).unwrap());
    });
}

criterion_group!(benches, scan);
criterion_main!(benches);
