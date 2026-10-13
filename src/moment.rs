//! Moments: short stories told by a few pictures in order (a ball passed from one person to the
//! next, everyone laughing at one joke), from `Config/moments.json`.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::warn;

use crate::library::{self, PROCESSED_DIR, Subject};

/// Moments file inside the library root, next to `subjects.json`.
pub const MOMENTS_FILE: &str = "Config/moments.json";

/// Fewest pictures that make a story.
const MIN_PICTURES: usize = 2;

/// Wire format: `{"Passing the ball": ["uuid1", "uuid2", "uuid3"], ...}`. The uuids are in story
/// order, first beat first. Each uuid is a file in `Processed`.
#[derive(Debug, Deserialize)]
struct MomentsFile(BTreeMap<String, Vec<String>>);

/// The moments whose pictures are synced, each as a [`Subject`] whose `images` keep the story
/// order. Moments left with fewer than two pictures are dropped.
///
/// A missing moments file means no moments.
///
/// # Errors
/// Fails when the file cannot be read or is invalid, or `Processed` cannot be listed.
pub fn scan(root: &Path) -> Result<Vec<Subject>> {
    let path = root.join(MOMENTS_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err).with_context(|| format!("cannot read {}", path.display())),
    };
    let MomentsFile(listed) =
        serde_json::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
    if listed.is_empty() {
        return Ok(Vec::new());
    }
    let files = library::index_processed(&root.join(PROCESSED_DIR))?;
    Ok(listed
        .into_iter()
        .filter_map(|(name, uuids)| library::resolve(name, &uuids, &files))
        .filter(|moment| {
            let enough = moment.images.len() >= MIN_PICTURES;
            if !enough {
                warn!("moment '{}': needs at least {MIN_PICTURES} synced pictures", moment.name);
            }
            enough
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library_with(moments: Option<&str>, pictures: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Processed")).unwrap();
        std::fs::create_dir_all(dir.path().join("Config")).unwrap();
        for name in pictures {
            std::fs::write(dir.path().join(format!("Processed/{name}.png")), b"").unwrap();
        }
        if let Some(json) = moments {
            std::fs::write(dir.path().join(MOMENTS_FILE), json).unwrap();
        }
        dir
    }

    #[test]
    fn keeps_story_order() {
        let dir = library_with(Some(r#"{"Pass": ["c", "a", "b"]}"#), &["a", "b", "c"]);
        let moments = scan(dir.path()).unwrap();
        let names: Vec<_> = moments[0]
            .images
            .iter()
            .map(|p| p.file_stem().unwrap().to_str().unwrap().to_owned())
            .collect();
        assert_eq!(names, ["c", "a", "b"]);
    }

    #[test]
    fn missing_file_means_no_moments() {
        let dir = library_with(None, &["a"]);
        assert_eq!(scan(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn drops_moments_that_are_too_short_or_unsynced() {
        let dir = library_with(Some(r#"{"One": ["a"], "Gone": ["x", "y"]}"#), &["a"]);
        assert_eq!(scan(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn invalid_file_is_an_error() {
        let dir = library_with(Some("not json"), &[]);
        assert!(scan(dir.path()).is_err());
    }
}
