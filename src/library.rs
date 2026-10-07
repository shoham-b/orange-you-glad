//! Finds subject folders and chooses what to show.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rand::Rng;
use rand::seq::{IndexedRandom, SliceRandom};
use walkdir::WalkDir;

const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "jfif", "png", "webp"];

/// A folder of pictures about one theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub name: String,
    pub images: Vec<PathBuf>,
}

/// Reads every immediate subfolder of `root` that contains at least one picture.
/// Rescanned on each cycle so newly synced pictures are picked up.
pub fn scan(root: &Path) -> Result<Vec<Subject>> {
    let entries = std::fs::read_dir(root)
        .with_context(|| format!("cannot read library root {}", root.display()))?;
    let mut subjects = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        let mut images = list_images(&entry.path());
        if images.is_empty() {
            continue;
        }
        images.sort();
        subjects.push(Subject { name, images });
    }
    subjects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(subjects)
}

fn list_images(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && is_image(e.path()))
        .map(walkdir::DirEntry::into_path)
        .collect()
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

    #[test]
    fn scan_finds_only_folders_with_pictures() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("cats")).unwrap();
        fs::write(dir.path().join("cats/one.JPG"), b"x").unwrap();
        fs::write(dir.path().join("cats/notes.txt"), b"x").unwrap();
        fs::create_dir_all(dir.path().join("empty")).unwrap();
        fs::create_dir_all(dir.path().join(".hidden")).unwrap();
        fs::write(dir.path().join(".hidden/x.jpg"), b"x").unwrap();
        fs::write(dir.path().join("loose.jpg"), b"x").unwrap();

        let subjects = scan(dir.path()).unwrap();
        assert_eq!(subjects.len(), 1);
        assert_eq!(subjects[0].name, "cats");
        assert_eq!(subjects[0].images.len(), 1);
    }

    #[test]
    fn scan_missing_root_is_an_error() {
        assert!(scan(Path::new("/definitely/not/here")).is_err());
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
