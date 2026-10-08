//! Resolves subjects from the synced Drive folder and chooses what to show.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rand::Rng;
use rand::seq::{IndexedRandom, SliceRandom};
use serde::Deserialize;
use tracing::warn;

const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "jfif", "png", "webp"];

/// Folder inside the library root that holds the pictures, each named `<uuid>.<ext>`.
pub const PROCESSED_DIR: &str = "Processed";
/// Maps each subject name to the uuids of its pictures, inside the library root.
pub const SUBJECTS_FILE: &str = "Config/subjects.json";

/// A named set of pictures about one theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub name: String,
    pub images: Vec<PathBuf>,
}

/// Wire format of [`SUBJECTS_FILE`]: `{"dog": ["uuid", ...], "ski": [...]}`. A uuid may be listed
/// under several subjects.
#[derive(Debug, Deserialize)]
struct SubjectsFile(BTreeMap<String, Vec<String>>);

/// Builds the subjects listed in `<root>/Config/subjects.json` from the pictures in
/// `<root>/Processed`. Rescanned on each cycle so newly synced pictures are picked up.
///
/// Uuids with no file yet (the intake agent may update the list before the sync catches up) are
/// skipped, and subjects left with no pictures are dropped.
pub fn scan(root: &Path) -> Result<Vec<Subject>> {
    let path = root.join(SUBJECTS_FILE);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read subjects file {}", path.display()))?;
    let SubjectsFile(listed) =
        serde_json::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
    let files = index_processed(&root.join(PROCESSED_DIR))?;

    let mut subjects = Vec::new();
    for (name, uuids) in listed {
        let mut images: Vec<PathBuf> = Vec::new();
        let mut missing = 0_usize;
        for uuid in &uuids {
            match files.get(&uuid.to_ascii_lowercase()) {
                Some(file) if !images.contains(file) => images.push(file.clone()),
                Some(_) => {}
                None => missing += 1,
            }
        }
        if missing > 0 {
            warn!("subject '{name}': {missing} listed picture(s) not synced yet");
        }
        if !images.is_empty() {
            subjects.push(Subject { name, images });
        }
    }
    Ok(subjects)
}

/// Lowercased file stem (the uuid) to path, for every picture directly inside `dir`.
/// Looking uuids up here, rather than joining them onto a path, keeps a bad entry in the
/// subjects file from pointing outside `Processed`.
fn index_processed(dir: &Path) -> Result<HashMap<String, PathBuf>> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read pictures folder {}", dir.display()))?;
    let mut files = HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_image(&path) {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            files.insert(stem.to_ascii_lowercase(), path);
        }
    }
    Ok(files)
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Picks a random subject, avoiding `previous` whenever there is an alternative.
pub fn pick_subject<'a, R: Rng + ?Sized>(
    subjects: &'a [Subject],
    previous: Option<&str>,
    rng: &mut R,
) -> Option<&'a Subject> {
    let fresh: Vec<&Subject> =
        subjects.iter().filter(|s| Some(s.name.as_str()) != previous).collect();
    fresh.choose(rng).copied().or_else(|| subjects.first())
}

/// Returns the subject's pictures in random order.
pub fn shuffled_images<R: Rng + ?Sized>(subject: &Subject, rng: &mut R) -> Vec<PathBuf> {
    let mut images = subject.images.clone();
    images.shuffle(rng);
    images
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::fs;

    fn subject(name: &str) -> Subject {
        Subject { name: name.into(), images: vec![PathBuf::from(format!("{name}/a.jpg"))] }
    }

    /// A library root with the given subjects file and files inside `Processed`.
    fn library(subjects_json: &str, files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Config")).unwrap();
        fs::create_dir_all(dir.path().join(PROCESSED_DIR)).unwrap();
        fs::write(dir.path().join(SUBJECTS_FILE), subjects_json).unwrap();
        for file in files {
            fs::write(dir.path().join(PROCESSED_DIR).join(file), b"x").unwrap();
        }
        dir
    }

    #[test]
    fn scan_resolves_uuids_to_files() {
        let dir = library(r#"{"dog": ["u1", "u2"], "ski": ["u2"]}"#, &["u1.jpg", "u2.PNG"]);

        let subjects = scan(dir.path()).unwrap();

        let names: Vec<_> = subjects.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["dog", "ski"]);
        assert_eq!(subjects[0].images.len(), 2);
        assert_eq!(subjects[1].images, [dir.path().join(PROCESSED_DIR).join("u2.PNG")]);
    }

    #[test]
    fn scan_skips_unsynced_uuids_and_empty_subjects() {
        let dir = library(r#"{"dog": ["u1", "later"], "ghost": ["nope"]}"#, &["u1.jpg"]);

        let subjects = scan(dir.path()).unwrap();

        assert_eq!(subjects.len(), 1);
        assert_eq!(subjects[0].name, "dog");
        assert_eq!(subjects[0].images.len(), 1);
    }

    #[test]
    fn scan_ignores_duplicates_other_files_and_path_tricks() {
        let dir = library(
            r#"{"dog": ["U1", "u1", "../Config/subjects", "notes"]}"#,
            &["u1.jpg", "notes.txt"],
        );

        let subjects = scan(dir.path()).unwrap();

        assert_eq!(subjects.len(), 1);
        assert_eq!(subjects[0].images.len(), 1);
    }

    #[test]
    fn scan_without_subjects_file_or_with_bad_json_is_an_error() {
        let dir = library("{ not json", &[]);
        assert!(scan(dir.path()).is_err());
        fs::remove_file(dir.path().join(SUBJECTS_FILE)).unwrap();
        assert!(scan(dir.path()).is_err());
        assert!(scan(Path::new("/definitely/not/here")).is_err());
    }

    #[test]
    fn scan_without_processed_folder_is_an_error() {
        let dir = library("{}", &[]);
        fs::remove_dir(dir.path().join(PROCESSED_DIR)).unwrap();
        assert!(scan(dir.path()).is_err());
    }

    #[test]
    fn pick_avoids_previous_when_possible() {
        let subjects = [subject("a"), subject("b")];
        let mut rng = StdRng::seed_from_u64(1);
        for _ in 0..20 {
            assert_eq!(pick_subject(&subjects, Some("a"), &mut rng).unwrap().name, "b");
        }
    }

    #[test]
    fn pick_falls_back_to_only_subject() {
        let subjects = [subject("a")];
        let mut rng = StdRng::seed_from_u64(1);
        assert_eq!(pick_subject(&subjects, Some("a"), &mut rng).unwrap().name, "a");
    }

    #[test]
    fn pick_from_nothing_is_none() {
        let mut rng = StdRng::seed_from_u64(1);
        assert!(pick_subject(&[], None, &mut rng).is_none());
    }
}
