//! Builds a collage image from picture files.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use image::imageops::{self, FilterType};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader, Limits, Rgb, RgbImage};
use tracing::warn;

use crate::layout::Rect;

/// Refuse to decode any single picture that would need more memory than this.
const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;

/// Fills each tile, in order, with the next candidate picture that decodes successfully.
/// Pictures are decoded and shrunk one at a time so peak memory stays small on a 1 GB board.
/// Tiles left over when candidates run out keep the background colour; the returned count is how
/// many tiles got a picture, so the caller can lay out again with fewer tiles and leave no holes.
pub fn build(
    width: u32,
    height: u32,
    background: [u8; 3],
    tiles: &[Rect],
    candidates: &[PathBuf],
) -> (RgbImage, usize) {
    let mut canvas = RgbImage::from_pixel(width, height, Rgb(background));
    let mut remaining = candidates.iter();
    let mut filled = 0;
    for tile in tiles {
        let picture = remaining.by_ref().find_map(|path| match load_tile(path, tile) {
            Ok(picture) => Some(picture),
            Err(err) => {
                warn!("skipping {}: {err:#}", path.display());
                None
            }
        });
        let Some(picture) = picture else { break };
        imageops::replace(&mut canvas, &picture, i64::from(tile.x), i64::from(tile.y));
        filled += 1;
    }
    (canvas, filled)
}

/// Width / height of the picture as it will be shown (EXIF rotation applied), read from the file
/// header only, so choosing a layout never needs a full decode.
pub fn probe_aspect(path: &Path) -> Result<f64> {
    let mut decoder = ImageReader::open(path)
        .context("cannot open")?
        .with_guessed_format()
        .context("cannot read")?
        .into_decoder()
        .context("cannot decode")?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 {
        anyhow::bail!("empty picture");
    }
    let turned = matches!(
        decoder.orientation().context("bad orientation")?,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    );
    Ok(if turned { f64::from(h) / f64::from(w) } else { f64::from(w) / f64::from(h) })
}

/// Decodes `path`, applies its EXIF rotation, and crops/scales it to exactly fill `tile`.
fn load_tile(path: &Path, tile: &Rect) -> Result<RgbImage> {
    let mut decoder = ImageReader::open(path)
        .context("cannot open")?
        .with_guessed_format()
        .context("cannot read")?
        .into_decoder()
        .context("cannot decode")?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    decoder.set_limits(limits).context("picture too large")?;
    let orientation = decoder.orientation().context("bad orientation")?;
    let mut picture = DynamicImage::from_decoder(decoder).context("cannot decode")?;
    picture.apply_orientation(orientation);
    // `into_rgb8` reuses the buffer when the picture is already RGB8, avoiding a full copy.
    Ok(fill(&picture.into_rgb8(), tile))
}

/// Scales `picture` to cover `tile` exactly, cropping the overflow. Works on a borrowed view of
/// the crop, so no full-size copy of the picture is made.
fn fill(picture: &RgbImage, tile: &Rect) -> RgbImage {
    let (crop_w, crop_h) = cover_crop(picture.width(), picture.height(), tile.w, tile.h);
    let x = (picture.width() - crop_w) / 2;
    // Crop nearer the top than the middle: in tall photos that keeps heads in frame.
    let y = (picture.height() - crop_h) / 3;
    let view = imageops::crop_imm(picture, x, y, crop_w, crop_h);
    imageops::resize(&*view, tile.w, tile.h, FilterType::Triangle)
}

/// Largest sub-rectangle of a `picture_w` x `picture_h` picture with the tile's aspect ratio.
fn cover_crop(picture_w: u32, picture_h: u32, tile_w: u32, tile_h: u32) -> (u32, u32) {
    let (pw, ph, tw, th) =
        (u64::from(picture_w), u64::from(picture_h), u64::from(tile_w), u64::from(tile_h));
    if pw * th > ph * tw {
        // Picture is wider than the tile: keep full height, trim the sides.
        (((ph * tw / th).max(1)) as u32, picture_h)
    } else {
        (picture_w, ((pw * th / tw).max(1)) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_solid(path: &Path, color: [u8; 3]) {
        RgbImage::from_pixel(40, 30, Rgb(color)).save(path).unwrap();
    }

    #[test]
    fn cover_crop_trims_the_overflowing_axis() {
        assert_eq!(cover_crop(4000, 3000, 100, 100), (3000, 3000));
        assert_eq!(cover_crop(3000, 4000, 100, 100), (3000, 3000));
        assert_eq!(cover_crop(4000, 3000, 200, 100), (4000, 2000));
        assert_eq!(cover_crop(100, 100, 100, 100), (100, 100));
    }

    #[test]
    fn cover_crop_never_returns_an_empty_crop() {
        let (w, h) = cover_crop(1000, 1, 1, 1000);
        assert!(w >= 1 && h >= 1);
    }

    #[test]
    fn corrupt_files_are_skipped_and_next_candidate_is_used() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.png");
        let good = dir.path().join("good.png");
        std::fs::write(&bad, b"not an image").unwrap();
        write_solid(&good, [200, 10, 10]);

        let tile = Rect { x: 0, y: 0, w: 20, h: 20 };
        let (out, filled) = build(20, 20, [0, 0, 0], &[tile], &[bad, good]);
        assert_eq!(filled, 1);
        assert_eq!(out.get_pixel(10, 10), &Rgb([200, 10, 10]));
    }

    #[test]
    fn probe_reads_the_shape_without_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.png");
        RgbImage::new(40, 30).save(&path).unwrap();
        assert!((probe_aspect(&path).unwrap() - 4.0 / 3.0).abs() < 1e-9);
        assert!(probe_aspect(&dir.path().join("missing.png")).is_err());
        std::fs::write(&path, b"not an image").unwrap();
        assert!(probe_aspect(&path).is_err());
    }

    #[test]
    fn missing_candidates_leave_background() {
        let tile = Rect { x: 0, y: 0, w: 10, h: 10 };
        let (out, filled) = build(10, 10, [1, 2, 3], &[tile], &[]);
        assert_eq!(filled, 0);
        assert_eq!(out.get_pixel(5, 5), &Rgb([1, 2, 3]));
    }

    #[test]
    fn pictures_land_in_their_tiles() {
        let dir = tempfile::tempdir().unwrap();
        let red = dir.path().join("r.png");
        let blue = dir.path().join("b.png");
        write_solid(&red, [255, 0, 0]);
        write_solid(&blue, [0, 0, 255]);
        let tiles = [Rect { x: 0, y: 0, w: 10, h: 10 }, Rect { x: 10, y: 0, w: 10, h: 10 }];
        let (out, _) = build(20, 10, [0, 0, 0], &tiles, &[red, blue]);
        assert_eq!(out.get_pixel(2, 2), &Rgb([255, 0, 0]));
        assert_eq!(out.get_pixel(17, 2), &Rgb([0, 0, 255]));
    }
}
