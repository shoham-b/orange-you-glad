use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Runtime settings, loaded from a TOML file. Every field has a default except `library_root`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Directory whose immediate subfolders are the "subjects".
    pub library_root: PathBuf,
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    /// Number of pictures per collage.
    #[serde(default = "default_tiles")]
    pub tiles: usize,
    /// Gap between pictures, in pixels.
    #[serde(default = "default_gap_px")]
    pub gap_px: u32,
    /// Cross-fade duration between collages; 0 switches instantly.
    #[serde(default = "default_fade_ms")]
    pub fade_ms: u64,
    /// Background colour (shows in gaps), as `[r, g, b]`.
    #[serde(default)]
    pub background: [u8; 3],
    #[serde(default = "default_framebuffer")]
    pub framebuffer: PathBuf,
}

fn default_interval_secs() -> u64 {
    300
}
fn default_tiles() -> usize {
    6
}
fn default_gap_px() -> u32 {
    8
}
fn default_fade_ms() -> u64 {
    1500
}
fn default_framebuffer() -> PathBuf {
    PathBuf::from("/dev/fb0")
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config {}", path.display()))?;
        let config: Config =
            toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.interval_secs == 0 {
            bail!("interval_secs must be at least 1");
        }
        if self.tiles == 0 {
            bail!("tiles must be at least 1");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_config_gets_defaults() {
        let c: Config = toml::from_str(r#"library_root = "/srv/photos""#).unwrap();
        assert_eq!(c.interval_secs, 300);
        assert_eq!(c.tiles, 6);
        assert_eq!(c.fade_ms, 1500);
        assert_eq!(c.framebuffer, PathBuf::from("/dev/fb0"));
        c.validate().unwrap();
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<Config>("library_root = \"/x\"\ntypo = 1").is_err());
    }

    #[test]
    fn zero_tiles_is_invalid() {
        let c: Config = toml::from_str("library_root = \"/x\"\ntiles = 0").unwrap();
        assert!(c.validate().is_err());
    }
}
