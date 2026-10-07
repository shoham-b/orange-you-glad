//! Output devices. `Display` is a trait so the rest of the app never touches `/dev/fb0` directly.

use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use image::RgbImage;
use thiserror::Error;

pub trait Display {
    /// Visible size in pixels.
    fn size(&self) -> (u32, u32);
    /// Replaces the screen contents. `frame` must match `size()`.
    fn present(&mut self, frame: &RgbImage) -> Result<()>;
}

#[derive(Debug, Error)]
pub enum DisplayError {
    #[error("unsupported framebuffer depth: {0} bits per pixel (need 16 or 32)")]
    UnsupportedDepth(u32),
    #[error("frame is {got:?} but the display is {expected:?}")]
    SizeMismatch { expected: (u32, u32), got: (u32, u32) },
    #[error("cannot parse {what}: {value:?}")]
    BadSysfsValue { what: &'static str, value: String },
}

/// Memory layout of a Linux framebuffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    /// Bytes per scanline (may exceed `width * bytes_per_pixel`).
    pub stride: u32,
    pub bits_per_pixel: u32,
}

impl Geometry {
    /// Reads the `virtual_size`, `stride` and `bits_per_pixel` attributes of a
    /// `/sys/class/graphics/fbN` directory.
    pub fn from_sysfs(dir: &Path) -> Result<Self> {
        let read = |name: &str| -> Result<String> {
            let path = dir.join(name);
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            Ok(text.trim().to_owned())
        };
        let parse = |what: &'static str, value: &str| -> Result<u32> {
            value
                .trim()
                .parse()
                .map_err(|_| DisplayError::BadSysfsValue { what, value: value.to_owned() }.into())
        };

        let size = read("virtual_size")?;
        let (width, height) = size.split_once(',').ok_or_else(|| DisplayError::BadSysfsValue {
            what: "virtual_size",
            value: size.clone(),
        })?;
        let geometry = Geometry {
            width: parse("virtual_size width", width)?,
            height: parse("virtual_size height", height)?,
            stride: parse("stride", &read("stride")?)?,
            bits_per_pixel: parse("bits_per_pixel", &read("bits_per_pixel")?)?,
        };
        match geometry.bits_per_pixel {
            16 | 32 => Ok(geometry),
            other => Err(DisplayError::UnsupportedDepth(other).into()),
        }
    }

    /// Size in bytes of one full frame in framebuffer layout.
    pub fn frame_len(&self) -> usize {
        self.stride as usize * self.height as usize
    }

    /// Converts `frame` into framebuffer bytes, writing into `out` (resized to `frame_len()`).
    /// 32 bpp is little-endian XRGB8888 (bytes B, G, R, X); 16 bpp is RGB565.
    pub fn encode_into(&self, frame: &RgbImage, out: &mut Vec<u8>) -> Result<(), DisplayError> {
        if frame.dimensions() != (self.width, self.height) {
            return Err(DisplayError::SizeMismatch {
                expected: (self.width, self.height),
                got: frame.dimensions(),
            });
        }
        out.clear();
        out.resize(self.frame_len(), 0);
        let bytes_per_pixel = (self.bits_per_pixel / 8) as usize;
        let stride = self.stride as usize;
        for (y, row) in frame.rows().enumerate() {
            let line = &mut out[y * stride..y * stride + self.width as usize * bytes_per_pixel];
            for (pixel, dst) in row.zip(line.chunks_exact_mut(bytes_per_pixel)) {
                let [r, g, b] = pixel.0;
                if bytes_per_pixel == 4 {
                    dst.copy_from_slice(&[b, g, r, 0xFF]);
                } else {
                    let v =
                        (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3);
                    dst.copy_from_slice(&v.to_le_bytes());
                }
            }
        }
        Ok(())
    }
}

/// Writes frames straight to a Linux framebuffer device such as `/dev/fb0`.
pub struct Framebuffer {
    file: File,
    geometry: Geometry,
    /// Reused for every frame so a fade does not allocate.
    encoded: Vec<u8>,
}

impl Framebuffer {
    pub fn open(device: &Path) -> Result<Self> {
        let name = device
            .file_name()
            .with_context(|| format!("bad framebuffer path {}", device.display()))?;
        let geometry = Geometry::from_sysfs(&Path::new("/sys/class/graphics").join(name))?;
        let file = OpenOptions::new().write(true).open(device).with_context(|| {
            format!("cannot open {} (is the user in the 'video' group?)", device.display())
        })?;
        Ok(Self { file, geometry, encoded: Vec::with_capacity(geometry.frame_len()) })
    }
}

impl Display for Framebuffer {
    fn size(&self) -> (u32, u32) {
        (self.geometry.width, self.geometry.height)
    }

    fn present(&mut self, frame: &RgbImage) -> Result<()> {
        self.geometry.encode_into(frame, &mut self.encoded)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&self.encoded).context("framebuffer write failed")
    }
}

/// Saves each frame as an image file; for developing without a TV.
pub struct FileDisplay {
    path: PathBuf,
    size: (u32, u32),
}

impl FileDisplay {
    pub fn new(path: PathBuf, size: (u32, u32)) -> Self {
        Self { path, size }
    }
}

impl Display for FileDisplay {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn present(&mut self, frame: &RgbImage) -> Result<()> {
        frame.save(&self.path).with_context(|| format!("cannot save {}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn geometry(bpp: u32, stride: u32) -> Geometry {
        Geometry { width: 2, height: 1, stride, bits_per_pixel: bpp }
    }

    fn encode(g: Geometry, frame: &RgbImage) -> Result<Vec<u8>, DisplayError> {
        let mut out = Vec::new();
        g.encode_into(frame, &mut out).map(|()| out)
    }

    #[test]
    fn encodes_32bpp_as_bgrx_with_stride_padding() {
        let frame =
            RgbImage::from_fn(2, 1, |x, _| if x == 0 { Rgb([1, 2, 3]) } else { Rgb([4, 5, 6]) });
        assert_eq!(
            encode(geometry(32, 12), &frame).unwrap(),
            vec![3, 2, 1, 255, 6, 5, 4, 255, 0, 0, 0, 0]
        );
    }

    #[test]
    fn encodes_16bpp_as_rgb565() {
        let frame = RgbImage::from_pixel(2, 1, Rgb([255, 0, 0]));
        assert_eq!(encode(geometry(16, 4), &frame).unwrap(), vec![0x00, 0xF8, 0x00, 0xF8]);
    }

    #[test]
    fn rejects_wrong_frame_size() {
        let frame = RgbImage::new(3, 3);
        assert!(matches!(encode(geometry(32, 8), &frame), Err(DisplayError::SizeMismatch { .. })));
    }

    #[test]
    fn reads_geometry_from_sysfs_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("virtual_size"), "1920,1080\n").unwrap();
        std::fs::write(dir.path().join("stride"), "7680\n").unwrap();
        std::fs::write(dir.path().join("bits_per_pixel"), "32\n").unwrap();
        let g = Geometry::from_sysfs(dir.path()).unwrap();
        assert_eq!(g, Geometry { width: 1920, height: 1080, stride: 7680, bits_per_pixel: 32 });
    }

    #[test]
    fn rejects_unsupported_depth() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("virtual_size"), "10,10").unwrap();
        std::fs::write(dir.path().join("stride"), "30").unwrap();
        std::fs::write(dir.path().join("bits_per_pixel"), "24").unwrap();
        assert!(Geometry::from_sysfs(dir.path()).is_err());
    }
}
