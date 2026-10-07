# AGENTS.md

Single source of rules for AI agents and humans working in this repository.

This file has two kinds of content:

- `## Style decisions` is PERMANENT. Never delete or rewrite an entry; only add new ones.
- `## Code rules (prunable)` holds implementation-level rules. Edit or delete them freely as the
  code evolves.

If you learn something that would save a future agent time (a gotcha, a changed convention, a
new command), update this file or `docs/` in the same PR, immediately.

## Purpose and constraints

Orange You Glad is a photo-collage slideshow for a TV. It runs on an Orange Pi Zero 2W
(Allwinner H618, 4 cores, 1 GB RAM, Debian/Armbian aarch64) with HDMI output. It scans a library
root whose immediate subfolders are "subjects" (synced from a shared Google Drive folder with
rclone), picks a subject, builds a collage with varied tile sizes (random binary splits),
cross-fades from the previous collage, and writes frames straight to `/dev/fb0`. There is no X
server. Target resolution is 1080p.

Hard constraints: 1 GB RAM total (decode one image at a time, reuse buffers), no GUI stack, only
well-known maintained crates.

## Module map

| Module | Responsibility |
| --- | --- |
| `config` | TOML config (`/etc/orange-you-glad.toml`) and CLI-derived overrides |
| `layout` | Pure geometry: random binary splits into tile rectangles |
| `library` | Scan the library root, list subjects and image files, pick a subject |
| `collage` | Decode, scale and place photos into a frame buffer following a layout |
| `display` | `Display` trait plus framebuffer and PNG-file implementations |
| `fade` | Cross-fade between two frames, written through a `Display` |
| `main` | CLI parsing, logging setup, `Slideshow` loop wiring the above together |

See `docs/ARCHITECTURE.md` for data flow and the memory budget.

## Commands

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo bench                      # criterion benchmarks in benches/
cargo run -- --config config.local.toml --output out.png --size 1920x1080 --once
```

`*.local.toml` and scratch images are git-ignored. On Windows, long paths break some build
scripts; use a short target dir, for example `CARGO_TARGET_DIR=/c/t/oyg` (bash) or
`$env:CARGO_TARGET_DIR = "C:\t\oyg"` (PowerShell).

## Workflow

- Every change is one GitHub issue, then small focused commit(s), then a small PR.
- Develop in a git worktree outside the repo directory, named `../orange-you-glad.wt/<branch>`.
- Never push to `main` directly. CI must be green before merge.
- Commit messages use conventional commits: `feat:`, `fix:`, `perf:`, `refactor:`, `test:`,
  `docs:`, `chore:`, `ci:`, with an optional scope, for example `feat(layout): add min tile size`.

## Testing

- Every new function or module gets unit tests in the same file (`#[cfg(test)] mod tests`).
- Performance-sensitive code (layout, scaling, blending) gets a criterion benchmark in `benches/`.
- Tests must be deterministic: seeded RNG (for example `StdRng::seed_from_u64`), `tempfile` for
  files, and no sleeping on real time beyond a few milliseconds.
- Test I/O logic through trait seams (a fake `Display`), not by touching `/dev/fb0`.

## Style decisions

PERMANENT. Never delete entries; only add.

- Rust edition 2024; `rustfmt.toml` sets `max_width = 100`.
- Errors: `thiserror` for typed library errors, `anyhow` with `.context()` at application
  boundaries (`main`).
- No `unwrap` or `expect` outside tests. No `unsafe` (`unsafe_code = "forbid"`).
- I/O goes behind trait seams (the `Display` trait) so logic is testable without hardware.
- Geometry is pure functions: data in, data out, no I/O, no global state.
- Inject randomness as a generic `R: Rng + ?Sized` parameter; never call a global RNG in logic.
- Doc comments explain why, not what. Comments are sparse.
- Keep modules small (roughly 250 lines or fewer, excluding tests); split when they grow.
- Only well-known, maintained dependencies. Justify any new one in the PR description.
- Log with `tracing`, never `println!` or `eprintln!` (except the CLI's deliberate output).
- Constants are named with their unit in the name or doc (`FADE_STEP_MS`, "pixels", "bytes").

## Code rules (prunable)

Edit or delete these freely as the code changes.

- rand 0.10 gotcha: `random_range` and similar convenience methods live on `RngExt`, so write
  `use rand::{Rng, RngExt};`. Slice helpers are in `rand::seq::{IndexedRandom, SliceRandom}`.
- Frames are tightly packed 32-bit pixel buffers sized to the framebuffer's `stride`, not the
  visible width. Read `stride` and `bits_per_pixel` from sysfs rather than assuming 4 * width.
- Reuse frame buffers between slides; do not allocate a new frame per fade step.
- Decode one photo at a time and drop it before decoding the next.
- Skip unreadable or corrupt images with a `tracing::warn!`; one bad file must not stop the show.
- Clippy pedantic is enabled with a short allow-list in `Cargo.toml`. Fix warnings rather than
  adding `#[allow]`; if an allow is justified, add a one-line reason.
