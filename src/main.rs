use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use tracing::error;

use orange_you_glad::config::Config;
use orange_you_glad::display::{Display, FileDisplay, Framebuffer};
use orange_you_glad::slideshow::Slideshow;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Path to the TOML config file.
    #[arg(short, long, default_value = "/etc/orange-you-glad.toml")]
    config: PathBuf,
    /// Develop without a TV: save frames to this image file instead of the framebuffer.
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,
    /// Canvas size for `--output`, as `WIDTHxHEIGHT`.
    #[arg(long, default_value = "1920x1080", value_parser = parse_size)]
    size: (u32, u32),
    /// Show one collage and exit.
    #[arg(long)]
    once: bool,
}

fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once('x').ok_or("expected WIDTHxHEIGHT")?;
    let parse = |v: &str| v.parse::<u32>().map_err(|e| e.to_string());
    match (parse(w)?, parse(h)?) {
        (0, _) | (_, 0) => Err("width and height must be positive".to_owned()),
        size => Ok(size),
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    let config = Config::load(&args.config)?;
    let mut display: Box<dyn Display> = match args.output {
        Some(path) => Box::new(FileDisplay::new(path, args.size)),
        None => Box::new(Framebuffer::open(&config.framebuffer)?),
    };

    let mut slideshow = Slideshow::new(display.size());
    loop {
        // Re-read each cycle so settings edited in Drive apply after the next sync.
        let current = config.with_drive_overrides();
        // A failed cycle (e.g. nothing synced yet) is logged and retried next time.
        if let Err(err) = slideshow.show_next(&current, display.as_mut()) {
            error!("{err:#}");
        }
        if args.once {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(current.interval_secs));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_width_by_height() {
        assert_eq!(parse_size("1920x1080"), Ok((1920, 1080)));
    }

    #[test]
    fn rejects_malformed_sizes() {
        for bad in ["1920", "axb", "1920x", "x1080", "-1x5", "0x10", "10x0"] {
            assert!(parse_size(bad).is_err(), "{bad} should be rejected");
        }
    }
}
