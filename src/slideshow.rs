//! One slideshow cycle: pick a subject, build a collage, fade it in.

use std::time::Duration;

use anyhow::{Context, Result};
use image::RgbImage;
use rand::Rng;
use rand::rngs::ThreadRng;
use tracing::info;

use crate::config::Config;
use crate::display::Display;
use crate::{collage, fade, layout, library};

/// What the show remembers between cycles.
pub struct Slideshow<R: Rng = ThreadRng> {
    rng: R,
    previous_subject: Option<String>,
    /// The frame currently on screen, kept so the next one can fade from it.
    current: RgbImage,
    scratch: RgbImage,
}

impl Slideshow {
    /// A show for a display of the given size, using the thread-local RNG.
    pub fn new(size: (u32, u32)) -> Self {
        Self::with_rng(size, rand::rng())
    }
}

impl<R: Rng> Slideshow<R> {
    pub fn with_rng((width, height): (u32, u32), rng: R) -> Self {
        Self {
            rng,
            previous_subject: None,
            current: RgbImage::new(width, height),
            scratch: RgbImage::new(width, height),
        }
    }

    /// Name of the subject currently on screen, if any.
    pub fn current_subject(&self) -> Option<&str> {
        self.previous_subject.as_deref()
    }

    /// Builds one collage from a different subject than last time and fades it in.
    ///
    /// # Errors
    /// Fails when the library cannot be read or is empty, or when the display rejects a frame.
    /// The show is left unchanged, so the caller can simply retry later.
    pub fn show_next(&mut self, config: &Config, display: &mut dyn Display) -> Result<()> {
        let subjects = library::scan(&config.library_root)?;
        let subject =
            library::pick_subject(&subjects, self.previous_subject.as_deref(), &mut self.rng)
                .with_context(|| {
                    format!("no pictures found under {}", config.library_root.display())
                })?;
        info!("showing '{}' ({} pictures)", subject.name, subject.images.len());

        let (width, height) = display.size();
        let count = config.tiles.min(subject.images.len());
        let tiles = layout::varied(width, height, count, config.gap_px, &mut self.rng);
        // Offer every picture, not just the first few, so unreadable files can be skipped.
        let candidates = library::shuffled_images(subject, &mut self.rng);
        let next = collage::build(width, height, config.background, &tiles, &candidates);

        let fade = Duration::from_millis(config.fade_ms);
        fade::transition(display, &self.current, &next, fade, &mut self.scratch)?;
        self.current = next;
        self.previous_subject = Some(subject.name.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::path::Path;

    struct Recorder {
        size: (u32, u32),
        frames: Vec<RgbImage>,
    }

    impl Display for Recorder {
        fn size(&self) -> (u32, u32) {
            self.size
        }
        fn present(&mut self, frame: &RgbImage) -> Result<()> {
            self.frames.push(frame.clone());
            Ok(())
        }
    }

    fn config(root: &Path) -> Config {
        toml::from_str(&format!(
            "library_root = {root:?}\ntiles = 2\ngap_px = 0\nfade_ms = 0\nbackground = [0, 0, 0]"
        ))
        .unwrap()
    }

    fn photo(dir: &Path, subject: &str, color: [u8; 3]) {
        let folder = dir.join(subject);
        std::fs::create_dir_all(&folder).unwrap();
        RgbImage::from_pixel(32, 32, Rgb(color)).save(folder.join("a.png")).unwrap();
    }

    #[test]
    fn shows_a_collage_and_remembers_the_subject() {
        let dir = tempfile::tempdir().unwrap();
        photo(dir.path(), "red", [250, 0, 0]);
        let mut display = Recorder { size: (16, 16), frames: Vec::new() };
        let mut show = Slideshow::with_rng((16, 16), StdRng::seed_from_u64(1));

        show.show_next(&config(dir.path()), &mut display).unwrap();

        assert_eq!(show.current_subject(), Some("red"));
        assert_eq!(display.frames.last().unwrap().get_pixel(0, 0), &Rgb([250, 0, 0]));
    }

    #[test]
    fn alternates_between_two_subjects() {
        let dir = tempfile::tempdir().unwrap();
        photo(dir.path(), "red", [250, 0, 0]);
        photo(dir.path(), "blue", [0, 0, 250]);
        let mut display = Recorder { size: (16, 16), frames: Vec::new() };
        let mut show = Slideshow::with_rng((16, 16), StdRng::seed_from_u64(1));

        let mut seen = Vec::new();
        for _ in 0..4 {
            show.show_next(&config(dir.path()), &mut display).unwrap();
            seen.push(show.current_subject().unwrap().to_owned());
        }
        assert!(seen.windows(2).all(|pair| pair[0] != pair[1]), "repeated subject in {seen:?}");
    }

    #[test]
    fn empty_library_is_an_error_and_leaves_the_show_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let mut display = Recorder { size: (16, 16), frames: Vec::new() };
        let mut show = Slideshow::with_rng((16, 16), StdRng::seed_from_u64(1));

        assert!(show.show_next(&config(dir.path()), &mut display).is_err());
        assert!(show.current_subject().is_none());
        assert_eq!(display.frames.len(), 0);
    }
}
