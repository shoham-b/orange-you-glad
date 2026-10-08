//! One slideshow cycle: pick a subject, build a collage, fade it in.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use image::RgbImage;
use rand::Rng;
use rand::rngs::ThreadRng;
use tracing::{info, warn};

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
        let (mut aspects, all) = probe(library::shuffled_images(subject, &mut self.rng), count);
        // If unreadable pictures leave tiles empty, lay out again with fewer tiles so no hole
        // shows. `filled < tiles.len() <= aspects.len()`, so `aspects` shrinks and the loop ends.
        let next = loop {
            let fit = layout::best_fit(
                width,
                height,
                &aspects,
                config.gap_px,
                FIT_ATTEMPTS,
                &mut self.rng,
            );
            let ordered = in_tile_order(&all, &fit.order);
            let (canvas, filled) =
                collage::build(width, height, config.background, &fit.tiles, &ordered);
            if filled == fit.tiles.len() || filled == 0 {
                break canvas;
            }
            aspects.truncate(filled);
        };

        let fade = Duration::from_millis(config.fade_ms);
        fade::transition(display, &self.current, &next, fade, &mut self.scratch)?;
        self.current = next;
        self.previous_subject = Some(subject.name.clone());
        Ok(())
    }
}

/// Candidate layouts tried per collage; each is cheap geometry, so this costs microseconds.
const FIT_ATTEMPTS: usize = 24;

/// Reads the shape of the first `count` pictures that open, from their headers. Returns their
/// aspect ratios and every picture to offer the collage: those first, then the untouched rest as
/// spares in case one fails to decode later. Pictures that cannot be opened are dropped.
fn probe(pool: Vec<PathBuf>, count: usize) -> (Vec<f64>, Vec<PathBuf>) {
    let mut aspects = Vec::with_capacity(count);
    let mut all = Vec::with_capacity(pool.len());
    let mut spares = Vec::new();
    for path in pool {
        if aspects.len() == count {
            spares.push(path);
        } else {
            match collage::probe_aspect(&path) {
                Ok(aspect) => {
                    aspects.push(aspect);
                    all.push(path);
                }
                Err(err) => warn!("skipping {}: {err:#}", path.display()),
            }
        }
    }
    all.extend(spares);
    (aspects, all)
}

/// `all` reordered so index `i` is the picture for tile `i`, followed by the pictures that got no
/// tile. `collage::build` fills tiles in order and falls through to the spares on failures.
fn in_tile_order(all: &[PathBuf], order: &[usize]) -> Vec<PathBuf> {
    let mut used = vec![false; all.len()];
    for &i in order {
        used[i] = true;
    }
    let placed = order.iter().map(|&i| all[i].clone());
    let unplaced = all.iter().zip(&used).filter(|(_, used)| !**used).map(|(p, _)| p.clone());
    placed.chain(unplaced).collect()
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
    fn tile_order_puts_assigned_pictures_first_then_spares() {
        let all: Vec<PathBuf> = ["a", "b", "c", "d"].iter().map(PathBuf::from).collect();
        let ordered = in_tile_order(&all, &[2, 0]);
        assert_eq!(ordered, ["c", "a", "b", "d"].iter().map(PathBuf::from).collect::<Vec<_>>());
    }

    #[test]
    fn probe_skips_unopenable_pictures_and_keeps_spares() {
        let dir = tempfile::tempdir().unwrap();
        let mut pool = Vec::new();
        for (name, w, h) in [("wide", 40, 20), ("tall", 20, 40), ("spare", 10, 10)] {
            let path = dir.path().join(format!("{name}.png"));
            RgbImage::new(w, h).save(&path).unwrap();
            pool.push(path);
        }
        pool.insert(1, dir.path().join("missing.png"));

        let (aspects, all) = probe(pool, 2);

        assert_eq!(aspects, [2.0, 0.5]);
        assert_eq!(all.len(), 3);
        assert!(all[2].ends_with("spare.png"));
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
}
