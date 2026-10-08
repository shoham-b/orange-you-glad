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

/// Optional settings file inside `library_root` (the synced Drive folder).
pub const SETTINGS_FILE: &str = "Config/settings.toml";

/// Settings that may be changed from the Drive settings file. Device-level keys (`library_root`,
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
    /// This config with the Drive settings file applied on top.
    ///
    /// A missing file means no overrides.
    ///
    /// # Errors
    /// Fails when the file cannot be read or holds a typo or invalid value. The caller keeps the
    /// local settings meanwhile, so a mistake made in Drive never stops the show.
    pub fn drive_overrides(&self) -> Result<Config> {
        let path = self.library_root.join(SETTINGS_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => self.merged(&text).with_context(|| format!("ignoring {}", path.display())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(self.clone()),
            Err(err) => Err(err).with_context(|| format!("cannot read {}", path.display())),
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

    fn applied(c: &Config) -> Config {
        c.drive_overrides().unwrap_or_else(|_| c.clone())
    }

    fn base(root: &Path) -> Config {
        toml::from_str(&format!("library_root = {root:?}\ntiles = 4")).unwrap()
    }

    fn write_settings(root: &Path, text: &str) {
        let file = root.join(SETTINGS_FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    #[test]
    fn drive_file_overrides_only_the_keys_it_sets() {
        let dir = tempfile::tempdir().unwrap();
        write_settings(dir.path(), "interval_secs = 60\ngap_px = 0");
        let c = applied(&base(dir.path()));
        assert_eq!((c.interval_secs, c.gap_px, c.tiles), (60, 0, 4));
    }

    #[test]
    fn missing_drive_file_keeps_the_base_config() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(applied(&base(dir.path())).tiles, 4);
    }

    #[test]
    fn bad_drive_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["tiles = 0", "tiles = ", "framebuffer = \"/dev/null\"", "typo = 1"] {
            write_settings(dir.path(), bad);
            assert_eq!(applied(&base(dir.path())).tiles, 4, "{bad}");
            assert!(base(dir.path()).drive_overrides().is_err(), "{bad}");
            assert_eq!(applied(&base(dir.path())).framebuffer, PathBuf::from("/dev/fb0"));
        }
    }

    #[test]
    fn zero_tiles_is_invalid() {
        let c: Config = toml::from_str("library_root = \"/x\"\ntiles = 0").unwrap();
        assert!(c.validate().is_err());
    }
}
