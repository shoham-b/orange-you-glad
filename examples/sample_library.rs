//! Writes a synthetic library (many pictures of mixed shapes, a few subjects) for previewing
//! collages without Drive: `cargo run --example sample_library -- <dir>`.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use image::{Rgb, RgbImage};

const SUBJECTS: [&str; 5] = ["beach", "forest", "city", "snow", "party"];
const PICTURES_PER_SUBJECT: u32 = 12;
/// Width and height ratios cycled through, so tiles get portrait, landscape and square photos.
const SHAPES: [(u32, u32); 6] = [(4, 3), (3, 4), (16, 9), (1, 1), (3, 2), (2, 3)];
/// Long edge of each generated picture, in pixels.
const LONG_EDGE_PX: u32 = 900;

fn main() -> Result<()> {
    let root = PathBuf::from(std::env::args().nth(1).context("usage: sample_library <dir>")?);
    let processed = root.join("Processed");
    fs::create_dir_all(&processed)?;
    fs::create_dir_all(root.join("Config"))?;

    let mut subjects = String::from("{\n");
    for (s, name) in SUBJECTS.iter().enumerate() {
        let mut ids = Vec::new();
        for n in 0..PICTURES_PER_SUBJECT {
            let index = s as u32 * PICTURES_PER_SUBJECT + n;
            let id = format!("5a3b0000-0000-4000-8000-{index:012}");
            let (w, h) = SHAPES[index as usize % SHAPES.len()];
            let (w, h) = if w >= h {
                (LONG_EDGE_PX, LONG_EDGE_PX * h / w)
            } else {
                (LONG_EDGE_PX * w / h, LONG_EDGE_PX)
            };
            picture(w, h, index).save(processed.join(format!("{id}.png")))?;
            ids.push(format!("\"{id}\""));
        }
        let comma = if s + 1 < SUBJECTS.len() { "," } else { "" };
        writeln!(subjects, "  \"{name}\": [{}]{comma}", ids.join(", "))?;
    }
    subjects.push_str("}\n");
    fs::write(root.join("Config/subjects.json"), subjects)?;
    println!(
        "wrote {} pictures to {}",
        SUBJECTS.len() as u32 * PICTURES_PER_SUBJECT,
        root.display()
    );
    Ok(())
}

/// A gradient with diagonal stripes and a centre ring, so cropping and scaling are visible.
fn picture(w: u32, h: u32, index: u32) -> RgbImage {
    let hue = (index * 47 % 360) as f32;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let radius = w.min(h) as f32 / 3.0;
    RgbImage::from_fn(w, h, |x, y| {
        let shade = 0.35 + 0.65 * (y as f32 / h as f32);
        let dist = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
        let ring = (dist - radius).abs() < 12.0;
        let stripe = (x + y) / 40 % 2 == 0;
        let (h2, s2, v) = if ring {
            (hue, 0.0, 1.0)
        } else if stripe {
            (hue, 0.7, shade)
        } else {
            ((hue + 30.0) % 360.0, 0.7, shade * 0.85)
        };
        Rgb(hsv(h2, s2, v))
    })
}

fn hsv(hue: f32, sat: f32, val: f32) -> [u8; 3] {
    let chroma = val * sat;
    let mid = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let (red, green, blue) = match (hue / 60.0) as u32 {
        0 => (chroma, mid, 0.0),
        1 => (mid, chroma, 0.0),
        2 => (0.0, chroma, mid),
        3 => (0.0, mid, chroma),
        4 => (mid, 0.0, chroma),
        _ => (chroma, 0.0, mid),
    };
    let floor = val - chroma;
    [red, green, blue].map(|ch| ((ch + floor) * 255.0) as u8)
}
