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
    0
}
fn default_fade_ms() -> u64 {
    1500
}
fn default_framebuffer() -> PathBuf {
    PathBuf::from("/dev/fb0")
}

/// Name of the optional settings file looked up inside `library_root` (the synced Drive folder).
pub const DRIVE_CONFIG_FILE: &str = "orange-you-glad.toml";

/// Settings that may be changed from the Drive file. Device-level keys (`library_root`,
/// `framebuffer`) are excluded on purpose: a Drive edit must not be able to blank the screen.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Overrides {
    interval_secs: Option<u64>,
    tiles: Option<usize>,
    gap_px: Option<u32>,
    fade_ms: Option<u64>,
    background: Option<[u8; 3]>,
}

impl Config {
    /// This config with the Drive file's settings applied on top.
    ///
    /// A missing file means no overrides. A malformed or invalid one is logged and ignored, so a
    /// typo made in Drive keeps the show running with the local settings.
    #[must_use]
    pub fn with_drive_overrides(&self) -> Config {
        let path = self.library_root.join(DRIVE_CONFIG_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return self.clone(),
            Err(err) => {
                tracing::warn!("cannot read {}: {err}", path.display());
                return self.clone();
            }
        };
        match self.merged(&text) {
            Ok(config) => config,
            Err(err) => {
                tracing::warn!("ignoring {}: {err:#}", path.display());
                self.clone()
            }
        }
    }

    fn merged(&self, text: &str) -> Result<Config> {
        let o: Overrides = toml::from_str(text).context("invalid settings")?;
        let config = Config {
            interval_secs: o.interval_secs.unwrap_or(self.interval_secs),
            tiles: o.tiles.unwrap_or(self.tiles),
            gap_px: o.gap_px.unwrap_or(self.gap_px),
            fade_ms: o.fade_ms.unwrap_or(self.fade_ms),
            background: o.background.unwrap_or(self.background),
            ..self.clone()
        };
        config.validate()?;
        Ok(config)
    }

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

    fn base(root: &Path) -> Config {
        toml::from_str(&format!("library_root = {root:?}\ntiles = 4")).unwrap()
    }

    #[test]
    fn drive_file_overrides_only_the_keys_it_sets() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(DRIVE_CONFIG_FILE), "interval_secs = 60\ngap_px = 0")
            .unwrap();
        let c = base(dir.path()).with_drive_overrides();
        assert_eq!((c.interval_secs, c.gap_px, c.tiles), (60, 0, 4));
    }

    #[test]
    fn missing_drive_file_keeps_the_base_config() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(base(dir.path()).with_drive_overrides().tiles, 4);
    }

    #[test]
    fn bad_drive_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(DRIVE_CONFIG_FILE);
        for bad in ["tiles = 0", "tiles = ", "framebuffer = \"/dev/null\"", "typo = 1"] {
            std::fs::write(&file, bad).unwrap();
            assert_eq!(base(dir.path()).with_drive_overrides().tiles, 4, "{bad}");
            assert_eq!(
                base(dir.path()).with_drive_overrides().framebuffer,
                PathBuf::from("/dev/fb0")
            );
        }
    }

    #[test]
    fn zero_tiles_is_invalid() {
        let c: Config = toml::from_str("library_root = \"/x\"\ntiles = 0").unwrap();
        assert!(c.validate().is_err());
    }
}
