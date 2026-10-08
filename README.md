# Orange You Glad

A photo-collage slideshow for a TV. It runs on a small ARM board (built for the Orange Pi Zero 2W
with 1 GB of RAM), reads photos from a folder that rclone keeps in sync with a shared Google Drive
folder, and draws straight to the Linux framebuffer. No X server, no desktop.

## Features

- Subjects: `Config/subjects.json` maps each subject name to the uuids of its pictures (files in
  `Processed/`); one subject is picked per collage.
- Birthdays: `Config/family.json` gives family members a Hebrew birthday; on that
  day the collages show only the subject named after them.
- Collages with varied tile sizes, built by random binary splits of the screen. Several layouts are
  tried per collage and the one whose tiles best match the photos' shapes wins, so little is cropped.
- Smooth cross-fade from the previous collage to the next.
- Writes directly to `/dev/fb0`, or to a PNG file in dev mode.
- Low memory: one photo decoded at a time, frame buffers reused.
- Plain TOML configuration.

## Quick start (dev mode)

Dev mode renders to a PNG instead of the framebuffer, so it works on any machine with Rust.

```sh
cp deploy/config.example.toml config.local.toml
# edit config.local.toml: uncomment and set library_root to a folder holding Config/subjects.json and Processed/
cargo run -- --config config.local.toml --output out.png --size 1920x1080 --once
```

## Command line

| Option | Meaning |
| --- | --- |
| `--config FILE` | Path to the TOML configuration |
| `--output FILE` | Dev mode: write a PNG instead of drawing to the framebuffer |
| `--size WxH` | Frame size to render, for example `1920x1080` |
| `--once` | Render one collage and exit |

## Configuration

See `deploy/config.example.toml`.

| Key | Default | Meaning |
| --- | --- | --- |
| `library_root` | none (required) | Directory holding `Config/subjects.json` and `Processed/` |
| `interval_secs` | `300` | Seconds each collage stays on screen |
| `tiles` | `6` | Photos per collage |
| `gap_px` | `0` | Gap between tiles, in pixels |
| `fade_ms` | `1500` | Cross-fade duration, in milliseconds |
| `background` | `[0, 0, 0]` | Gap colour as `[r, g, b]` |
| `framebuffer` | `/dev/fb0` | Framebuffer device |

## Documentation

- [Architecture and memory budget](docs/ARCHITECTURE.md)
- [Deploying on an Orange Pi Zero 2W](docs/DEPLOY.md) (written but not yet tested on hardware)
- [Contributor and agent rules](AGENTS.md)

## License

MIT. See [LICENSE](LICENSE).
