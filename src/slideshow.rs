//! One slideshow cycle: pick a subject, build a collage, fade it in.

use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{Local, NaiveDate};
use image::RgbImage;
use rand::Rng;
use rand::rngs::ThreadRng;
use tracing::{info, warn};

use crate::config::Config;
use crate::display::Display;
use crate::{collage, fade, family, hebrew, layout, library};

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
        self.show_on(Local::now().date_naive(), config, display)
    }

    /// Like [`Self::show_next`] for a given local date. On a family member's Hebrew birthday
    /// only the birthday people's pictures are shown.
    pub fn show_on(
        &mut self,
        today: NaiveDate,
        config: &Config,
        display: &mut dyn Display,
    ) -> Result<()> {
        let mut subjects =
            match family::birthday_subjects(&config.library_root, hebrew::from_gregorian(today)) {
                Ok(birthdays) => birthdays,
                Err(err) => {
                    warn!("ignoring family birthdays: {err:#}");
                    Vec::new()
                }
            };
        if subjects.is_empty() {
            subjects = library::scan(&config.library_root)?;
        }
        let subject =
            library::pick_subject(&subjects, self.previous_subject.as_deref(), &mut self.rng)
                .with_context(|| {
                    format!("no pictures found under {}", config.library_root.display())
                })?;
        info!("showing '{}' ({} pictures)", subject.name, subject.images.len());

        let (width, height) = display.size();
        let mut count = config.tiles.min(subject.images.len());
        // Offer every picture, not just the first few, so unreadable files can be skipped.
        let candidates = library::shuffled_images(subject, &mut self.rng);
        // If unreadable pictures leave tiles empty, lay out again with fewer tiles so no hole
        // shows. `filled < tiles.len() <= count`, so `count` shrinks and the loop ends.
        let next = loop {
            let tiles = layout::varied(width, height, count, config.gap_px, &mut self.rng);
            let (canvas, filled) =
                collage::build(width, height, config.background, &tiles, &candidates);
            if filled == tiles.len() || filled == 0 {
                break canvas;
            }
            count = filled;
        };

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

    /// A picture named `<name>.png` in `Processed`.
    fn photo(dir: &Path, name: &str, color: [u8; 3]) {
        let processed = dir.join("Processed");
        std::fs::create_dir_all(&processed).unwrap();
        RgbImage::from_pixel(32, 32, Rgb(color))
            .save(processed.join(format!("{name}.png")))
            .unwrap();
    }

    fn list_subjects(dir: &Path, json: &str) {
        std::fs::create_dir_all(dir.join("Config")).unwrap();
        std::fs::write(dir.join("Config/subjects.json"), json).unwrap();
    }

    #[test]
    fn shows_a_collage_and_remembers_the_subject() {
        let dir = tempfile::tempdir().unwrap();
        photo(dir.path(), "red", [250, 0, 0]);
        list_subjects(dir.path(), r#"{"red": ["red"]}"#);
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
        list_subjects(dir.path(), r#"{"red": ["red"], "blue": ["blue"]}"#);
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
    fn unreadable_picture_means_fewer_tiles_not_a_hole() {
        let dir = tempfile::tempdir().unwrap();
        photo(dir.path(), "red", [250, 0, 0]);
        std::fs::write(dir.path().join("Processed/bad.png"), b"not an image").unwrap();
        list_subjects(dir.path(), r#"{"red": ["red", "bad"]}"#);
        let mut display = Recorder { size: (16, 16), frames: Vec::new() };
        let mut show = Slideshow::with_rng((16, 16), StdRng::seed_from_u64(1));

        show.show_next(&config(dir.path()), &mut display).unwrap();

        let frame = display.frames.last().unwrap();
        assert!(frame.pixels().all(|p| p == &Rgb([250, 0, 0])), "background showed through");
    }

    #[test]
    fn empty_library_is_an_error_and_leaves_the_show_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Processed")).unwrap();
        list_subjects(dir.path(), "{}");
        let mut display = Recorder { size: (16, 16), frames: Vec::new() };
        let mut show = Slideshow::with_rng((16, 16), StdRng::seed_from_u64(1));

        assert!(show.show_next(&config(dir.path()), &mut display).is_err());
        assert!(show.current_subject().is_none());
        assert_eq!(display.frames.len(), 0);
    }

    #[test]
    fn birthday_person_replaces_the_usual_subjects_for_the_day() {
        let dir = tempfile::tempdir().unwrap();
        photo(dir.path(), "red", [250, 0, 0]);
        photo(dir.path(), "blue", [0, 0, 250]);
        list_subjects(dir.path(), r#"{"blue": ["blue"]}"#);
        std::fs::write(
            dir.path().join("Config/family.json"),
            r#"{"Dana": {"birthday": "27/1", "pictures": ["red"]}}"#,
        )
        .unwrap();
        let mut display = Recorder { size: (16, 16), frames: Vec::new() };
        let mut show = Slideshow::with_rng((16, 16), StdRng::seed_from_u64(1));
        let birthday = NaiveDate::from_ymd_opt(2026, 10, 8).unwrap(); // 27 Tishrei 5787

        for _ in 0..3 {
            show.show_on(birthday, &config(dir.path()), &mut display).unwrap();
            assert_eq!(show.current_subject(), Some("Dana"));
        }
        show.show_on(birthday.succ_opt().unwrap(), &config(dir.path()), &mut display).unwrap();
        assert_eq!(show.current_subject(), Some("blue"));
    }

    #[test]
    fn broken_family_file_does_not_stop_the_show() {
        let dir = tempfile::tempdir().unwrap();
        photo(dir.path(), "blue", [0, 0, 250]);
        list_subjects(dir.path(), r#"{"blue": ["blue"]}"#);
        std::fs::write(dir.path().join("Config/family.json"), "{ nope").unwrap();
        let mut display = Recorder { size: (16, 16), frames: Vec::new() };
        let mut show = Slideshow::with_rng((16, 16), StdRng::seed_from_u64(1));

        show.show_next(&config(dir.path()), &mut display).unwrap();

        assert_eq!(show.current_subject(), Some("blue"));
    }
}
