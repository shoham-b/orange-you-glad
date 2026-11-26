//! The [`PictureSource`] that downloads from an rclone remote by running the `rclone` program, so
//! no Drive client or TLS stack is linked into the slideshow.

use std::ffi::OsStr;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::library::{self, PROCESSED_DIR, PictureIndex};
use crate::source::{Fetched, PictureSource};

/// Hard limit on one rclone run, in seconds. A hung download would otherwise freeze the show.
const DEADLINE_SECS: u64 = 300;
/// How often a running rclone is checked, in milliseconds.
const POLL_MS: u64 = 50;
/// Largest picture listed, so never chosen. A listing filter only: rclone rejects other filters
/// next to `--files-from-raw`.
const MAX_PICTURE_SIZE: &str = "25M";
/// Flags for every run: give up quickly (the slideshow retries by itself) and skip Google Docs.
const COMMON_ARGS: [&str; 9] = [
    "--contimeout",
    "15s",
    "--timeout",
    "60s",
    "--retries",
    "1",
    "--low-level-retries",
    "3",
    "--drive-skip-gdocs",
];

/// Pictures in `<remote>/Processed`.
pub struct Rclone {
    processed: String,
}

impl Rclone {
    /// `remote` is anything rclone accepts as a folder: `gdrive:`, `gdrive:some/folder`.
    pub fn new(remote: &str) -> Self {
        let separator = if remote.ends_with([':', '/']) { "" } else { "/" };
        Self { processed: format!("{remote}{separator}{PROCESSED_DIR}") }
    }
}

impl PictureSource for Rclone {
    fn index(&self) -> Result<PictureIndex> {
        let work = tempfile::tempdir().context("cannot create a temporary folder")?;
        let args: [&OsStr; 5] = [
            "lsf".as_ref(),
            self.processed.as_ref(),
            "--files-only".as_ref(),
            "--max-size".as_ref(),
            MAX_PICTURE_SIZE.as_ref(),
        ];
        Ok(parse_listing(&run(work.path(), &args)?))
    }

    fn fetch(&self, pictures: &[PathBuf]) -> Result<Fetched> {
        let work = tempfile::tempdir().context("cannot create a temporary folder")?;
        let wanted = work.path().join("wanted.txt");
        let names: Vec<_> = pictures.iter().map(|p| p.to_string_lossy()).collect();
        std::fs::write(&wanted, names.join("\n")).context("cannot write the download list")?;
        let target = work.path().join("pictures");
        std::fs::create_dir(&target).context("cannot create the download folder")?;
        let args: [&OsStr; 8] = [
            "copy".as_ref(),
            self.processed.as_ref(),
            target.as_os_str(),
            // Raw: a file name starting with `#` or `;` is a name, not a comment.
            "--files-from-raw".as_ref(),
            wanted.as_os_str(),
            "--no-traverse".as_ref(),
            "--transfers".as_ref(),
            "2".as_ref(),
        ];
        run(work.path(), &args)?;
        // rclone skips names that vanished since the listing, so keep only what arrived.
        let paths: Vec<PathBuf> =
            pictures.iter().map(|p| target.join(p)).filter(|p| p.is_file()).collect();
        if paths.is_empty() {
            bail!("none of the {} chosen pictures could be downloaded", pictures.len());
        }
        Ok(Fetched { paths, downloads: Some(work) })
    }
}

/// Uuid to file name from `rclone lsf --files-only` output, keeping pictures only.
fn parse_listing(listing: &str) -> PictureIndex {
    listing
        .lines()
        .map(Path::new)
        // A slash in a Drive name must not let a download land outside the download folder.
        .filter(|name| name.file_name() == Some(name.as_os_str()) && library::is_image(name))
        .filter_map(|name| {
            let stem = name.file_stem()?.to_str()?.to_ascii_lowercase();
            Some((stem, name.to_path_buf()))
        })
        .collect()
}

/// Runs rclone and returns its standard output. Output goes to files in `work` rather than pipes,
/// so waiting with a deadline cannot deadlock on a full pipe.
fn run(work: &Path, args: &[&OsStr]) -> Result<String> {
    let (out, err) = (work.join("stdout.txt"), work.join("stderr.txt"));
    let mut child = Command::new("rclone")
        .args(args)
        .args(COMMON_ARGS)
        .stdin(Stdio::null())
        .stdout(File::create(&out).context("cannot capture rclone output")?)
        .stderr(File::create(&err).context("cannot capture rclone errors")?)
        .spawn()
        .context("cannot run rclone (is it installed?)")?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().context("cannot wait for rclone")? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(DEADLINE_SECS) {
            // Best effort: the process may have just exited by itself.
            let _ = child.kill();
            let _ = child.wait();
            bail!("rclone did not finish within {DEADLINE_SECS} seconds");
        }
        std::thread::sleep(Duration::from_millis(POLL_MS));
    };
    if !status.success() {
        let errors = std::fs::read_to_string(&err).unwrap_or_default();
        // The last line is rclone's summary of what failed. Its timestamp is dropped so that an
        // unchanged problem is logged once, not on every retry.
        let last = errors.lines().next_back().unwrap_or_default();
        bail!("rclone failed ({status}): {}", without_timestamp(last));
    }
    std::fs::read_to_string(&out).context("cannot read rclone output")
}

/// `line` without a leading `2026/01/31 12:00:00 ` stamp, if it has one.
fn without_timestamp(line: &str) -> &str {
    let mut parts = line.splitn(3, ' ');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(date), Some(_), Some(rest)) if date.split('/').count() == 3 => rest,
        _ => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processed_folder_is_appended_to_the_remote() {
        assert_eq!(Rclone::new("gdrive:").processed, "gdrive:Processed");
        assert_eq!(Rclone::new("gdrive:show").processed, "gdrive:show/Processed");
        assert_eq!(Rclone::new("gdrive:show/").processed, "gdrive:show/Processed");
    }

    #[test]
    fn listing_keeps_plain_picture_names_by_lowercase_uuid() {
        let files = parse_listing("AB.JPG\ncd.webp\nnotes.txt\n../evil.png\na/b.png\n");

        assert_eq!(files.len(), 2);
        assert_eq!(files["ab"], PathBuf::from("AB.JPG"));
        assert_eq!(files["cd"], PathBuf::from("cd.webp"));
    }

    #[test]
    fn timestamps_are_stripped_from_errors() {
        assert_eq!(without_timestamp("2026/01/31 12:00:00 NOTICE: no"), "NOTICE: no");
        assert_eq!(without_timestamp("plain error text"), "plain error text");
    }

    /// rclone treats a plain path as a remote, so this runs the real program without a network.
    #[test]
    #[ignore = "needs rclone installed: run `cargo test -- --ignored rclone`"]
    fn real_rclone_downloads_only_the_chosen_pictures_and_cleans_up() {
        let remote = tempfile::tempdir().unwrap();
        let processed = remote.path().join(PROCESSED_DIR);
        std::fs::create_dir(&processed).unwrap();
        for name in ["a.png", "#b.png", "c.png"] {
            std::fs::write(processed.join(name), name).unwrap();
        }
        let source = Rclone::new(&remote.path().to_string_lossy());

        let files = source.index().unwrap();
        assert_eq!(files.len(), 3);
        let wanted = ["#b", "a"].map(|uuid| files[uuid].clone());
        let fetched = source.fetch(&wanted).unwrap();

        let downloads = fetched.downloads.as_ref().unwrap().path().to_path_buf();
        assert_eq!(std::fs::read_dir(downloads.join("pictures")).unwrap().count(), 2);
        assert_eq!(std::fs::read(&fetched.paths[0]).unwrap(), b"#b.png");
        assert!(source.fetch(&[PathBuf::from("gone.png")]).is_err());
        drop(fetched);
        assert!(!downloads.exists());
    }
}
