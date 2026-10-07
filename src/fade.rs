//! Cross-fade between two frames.

use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use image::RgbImage;

use crate::display::Display;

/// Target pace of the fade. The TV does not need more, and the board cannot do much more.
const FRAME_TIME: Duration = Duration::from_millis(50);

/// Mixes `from` and `to` into `out`; `weight` runs from 0 (all `from`) to 256 (all `to`).
pub fn blend(from: &[u8], to: &[u8], weight: u32, out: &mut [u8]) {
    let inverse = 256 - weight;
    for ((o, &a), &b) in out.iter_mut().zip(from).zip(to) {
        *o = ((u32::from(a) * inverse + u32::from(b) * weight) >> 8) as u8;
    }
}

/// Fades `display` from `from` to `to` over `duration`, then shows `to` exactly.
/// `scratch` is reused between steps so the fade allocates nothing.
pub fn transition(
    display: &mut dyn Display,
    from: &RgbImage,
    to: &RgbImage,
    duration: Duration,
    scratch: &mut RgbImage,
) -> Result<()> {
    let steps = (duration.as_millis() / FRAME_TIME.as_millis()) as u32;
    for step in 1..steps {
        let started = Instant::now();
        blend(from.as_raw(), to.as_raw(), 256 * step / steps, scratch);
        display.present(scratch)?;
        thread::sleep(FRAME_TIME.saturating_sub(started.elapsed()));
    }
    display.present(to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    struct Recorder {
        frames: Vec<u8>,
    }

    impl Display for Recorder {
        fn size(&self) -> (u32, u32) {
            (1, 1)
        }
        fn present(&mut self, frame: &RgbImage) -> Result<()> {
            self.frames.push(frame.get_pixel(0, 0)[0]);
            Ok(())
        }
    }

    #[test]
    fn blend_endpoints_and_midpoint() {
        let mut out = [0u8; 2];
        blend(&[0, 200], &[100, 100], 0, &mut out);
        assert_eq!(out, [0, 200]);
        blend(&[0, 200], &[100, 100], 128, &mut out);
        assert_eq!(out, [50, 150]);
        blend(&[0, 200], &[100, 100], 256, &mut out);
        assert_eq!(out, [100, 100]);
    }

    #[test]
    fn transition_ramps_up_and_ends_on_target() {
        let from = RgbImage::from_pixel(1, 1, Rgb([0, 0, 0]));
        let to = RgbImage::from_pixel(1, 1, Rgb([200, 0, 0]));
        let mut scratch = from.clone();
        let mut display = Recorder { frames: Vec::new() };
        transition(&mut display, &from, &to, Duration::from_millis(200), &mut scratch).unwrap();
        assert_eq!(display.frames.last(), Some(&200));
        assert!(display.frames.windows(2).all(|w| w[0] <= w[1]));
        assert!(display.frames.len() > 2);
    }

    #[test]
    fn zero_duration_just_shows_target() {
        let from = RgbImage::from_pixel(1, 1, Rgb([0, 0, 0]));
        let to = RgbImage::from_pixel(1, 1, Rgb([9, 0, 0]));
        let mut scratch = from.clone();
        let mut display = Recorder { frames: Vec::new() };
        transition(&mut display, &from, &to, Duration::ZERO, &mut scratch).unwrap();
        assert_eq!(display.frames, vec![9]);
    }
}
