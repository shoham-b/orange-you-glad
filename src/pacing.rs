//! When to wake up: how often the Drive settings are re-read, and how soon a failure is retried.

use std::time::Duration;

/// How often the settings are re-read while everything works, in seconds.
pub const PROBE_SECS: u64 = 60;
/// How often they are re-read while loading them or building a collage keeps failing, in seconds.
/// Short, so a fix made in Drive shows up soon after the next sync.
pub const RETRY_SECS: u64 = 10;

/// How long to sleep before the next probe, or `None` when a new collage is due now.
///
/// `elapsed` is the time since the current collage appeared. `healthy` is false while the settings
/// file is broken. After a failed cycle nothing new is on screen, so the caller retries every
/// `RETRY_SECS` without asking.
#[must_use]
pub fn next_sleep(elapsed: Duration, interval: Duration, healthy: bool) -> Option<Duration> {
    let probe = Duration::from_secs(if healthy { PROBE_SECS } else { RETRY_SECS });
    let remaining = interval.checked_sub(elapsed).filter(|r| !r.is_zero())?;
    Some(remaining.min(probe))
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn healthy_show_probes_every_minute() {
        assert_eq!(next_sleep(secs(0), secs(300), true), Some(secs(60)));
        assert_eq!(next_sleep(secs(240), secs(300), true), Some(secs(60)));
    }

    #[test]
    fn broken_settings_probe_faster() {
        assert_eq!(next_sleep(secs(0), secs(300), false), Some(secs(10)));
    }

    #[test]
    fn last_sleep_is_cut_to_the_deadline() {
        assert_eq!(next_sleep(secs(290), secs(300), true), Some(secs(10)));
        assert_eq!(next_sleep(secs(295), secs(300), false), Some(secs(5)));
    }

    #[test]
    fn collage_is_due_once_the_interval_has_passed() {
        assert_eq!(next_sleep(secs(300), secs(300), true), None);
        assert_eq!(next_sleep(secs(999), secs(300), false), None);
    }

    #[test]
    fn shorter_interval_from_drive_applies_at_the_next_probe() {
        assert_eq!(next_sleep(secs(120), secs(60), true), None);
    }
}
