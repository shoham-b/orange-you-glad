//! Where pictures come from: the synced `Processed` folder, or an rclone remote from which only the
//! pictures of the next collage are downloaded, so the card never has to hold the whole library.

use std::path::{Path, PathBuf};

use anyhow::Result;
use tempfile::TempDir;

use crate::config::Config;
use crate::library::{self, PROCESSED_DIR, PictureIndex};
use crate::rclone::Rclone;

/// Seam between the slideshow and wherever the pictures live.
pub trait PictureSource {
    /// Lowercased uuid to picture, for every picture that exists. The paths are only meaningful to
    /// [`Self::fetch`] of the same source.
    fn index(&self) -> Result<PictureIndex>;

    /// Makes `pictures` (paths from [`Self::index`]) readable as local files.
    fn fetch(&self, pictures: &[PathBuf]) -> Result<Fetched>;
}

/// Local files in the order they were asked for. Downloads are deleted when this is dropped.
pub struct Fetched {
    pub paths: Vec<PathBuf>,
    /// Owns the download folder, if there is one.
    pub downloads: Option<TempDir>,
}

/// Pictures already on disk in `<library_root>/Processed`.
pub struct Local {
    processed: PathBuf,
}

impl Local {
    pub fn new(library_root: &Path) -> Self {
        Self { processed: library_root.join(PROCESSED_DIR) }
    }
}

impl PictureSource for Local {
    fn index(&self) -> Result<PictureIndex> {
        library::index_processed(&self.processed)
    }

    fn fetch(&self, pictures: &[PathBuf]) -> Result<Fetched> {
        Ok(Fetched { paths: pictures.to_vec(), downloads: None })
    }
}

/// The source the config asks for: the rclone `remote` when set, else the local folder.
pub fn from_config(config: &Config) -> Box<dyn PictureSource> {
    match &config.remote {
        Some(remote) => Box::new(Rclone::new(remote)),
        None => Box::new(Local::new(&config.library_root)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_pictures_are_used_where_they_are() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(PROCESSED_DIR)).unwrap();
        std::fs::write(dir.path().join(PROCESSED_DIR).join("AB.jpg"), b"x").unwrap();
        let source = Local::new(dir.path());

        let picture = source.index().unwrap()["ab"].clone();
        let fetched = source.fetch(std::slice::from_ref(&picture)).unwrap();

        assert_eq!(fetched.paths, [picture]);
        assert!(fetched.downloads.is_none());
    }
}
