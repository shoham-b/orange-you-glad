use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use tracing::{info, warn};

use orange_you_glad::config::Config;
use orange_you_glad::display::{Display, DisplayError, FileDisplay, Framebuffer};
use orange_you_glad::pacing::{RETRY_SECS, next_sleep};
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
    /// Preview in a desktop window instead of the framebuffer (needs `--features window`).
    #[arg(long, conflicts_with = "output")]
    window: bool,
    /// Canvas size for `--output` and `--window`, as `WIDTHxHEIGHT`.
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
        None if args.window => open_window(args.size)?,
        None => Box::new(Framebuffer::open(&config.framebuffer)?),
    };

    let mut slideshow = Slideshow::new(display.size());
    let mut current = config.clone();
    let mut settings_error = None;
    let mut cycle_error = None;
    loop {
        refresh(&config, &mut current, &mut settings_error);
        let shown = slideshow.show_next(&current, display.as_mut());
        if shown.as_ref().is_err_and(is_window_closed) {
            return Ok(());
        }
        let failed = shown.is_err();
        log_change(&mut cycle_error, shown.err().map(|e| format!("{e:#}")), "collage");
        if args.once {
            return Ok(());
        }
        if failed {
            // Nothing is on screen yet (e.g. nothing synced); retry soon instead of waiting.
            if display.idle(Duration::from_secs(RETRY_SECS)).is_err() {
                return Ok(());
            }
            continue;
        }
        let shown_at = Instant::now();
        while let Some(nap) = next_sleep(
            shown_at.elapsed(),
            Duration::from_secs(current.interval_secs),
            settings_error.is_none(),
        ) {
            if display.idle(nap).is_err() {
                return Ok(());
            }
            refresh(&config, &mut current, &mut settings_error);
        }
    }
}

fn is_window_closed(err: &anyhow::Error) -> bool {
    matches!(err.downcast_ref::<DisplayError>(), Some(DisplayError::WindowClosed))
}

#[cfg(feature = "window")]
fn open_window(size: (u32, u32)) -> Result<Box<dyn Display>> {
    Ok(Box::new(orange_you_glad::display::WindowDisplay::open(size)?))
}

#[cfg(not(feature = "window"))]
fn open_window(_size: (u32, u32)) -> Result<Box<dyn Display>> {
    anyhow::bail!("--window needs a build with `--features window`")
}

/// Re-reads the Drive settings. On failure the last good settings stay in force.
fn refresh(base: &Config, current: &mut Config, last_error: &mut Option<String>) {
    match base.drive_overrides() {
        Ok(fresh) => {
            *current = fresh;
            log_change(last_error, None, "settings");
        }
        Err(err) => log_change(last_error, Some(format!("{err:#}")), "settings"),
    }
}

/// Logs only when the problem appears, changes or goes away, so fast retries do not flood the
/// journal.
fn log_change(last: &mut Option<String>, now: Option<String>, what: &str) {
    if *last == now {
        return;
    }
    if let Some(message) = &now {
        warn!("{what}: {message}");
    } else {
        info!("{what} recovered");
    }
    *last = now;
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
