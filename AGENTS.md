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
(Allwinner H618, 4 cores, 1 GB RAM, Debian/Armbian aarch64) with HDMI output. It reads a library
root synced from a shared Google Drive folder with rclone: `Processed/<uuid>.<ext>` pictures and
`Config/subjects.json`, which maps each "subject" to a list of uuids. It picks a subject, builds a collage with varied tile sizes (random binary
splits), cross-fades from the previous collage, and writes frames straight to `/dev/fb0`. There is no X
server. Target resolution is 1080p.

Hard constraints: 1 GB RAM total (decode one image at a time, reuse buffers), no GUI stack, only
well-known maintained crates.

## Module map

| Module | Responsibility |
| --- | --- |
| `config` | TOML config (`/etc/orange-you-glad.toml`), defaults and validation |
| `layout` | Pure geometry: random splits of a random tile into varied-size rectangles |
| `library` | Resolve subjects (uuid lists) to image files, pick a subject |
| `collage` | Decode one photo at a time, cover-crop and scale it into its tile |
| `display` | `Display` trait plus framebuffer and image-file implementations |
| `fade` | Cross-fade between two frames, written through a `Display` |
| `slideshow` | `Slideshow`: one cycle of rescan, pick, build, fade (generic over the RNG) |
| `pacing` | Pure timing: how long to sleep between settings probes (60 s, or 10 s after a failure) |
| `main` | CLI parsing, logging setup, display selection and the sleep loop |

See `docs/ARCHITECTURE.md` for data flow and the memory budget.

## Commands

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo bench                      # criterion benchmarks in benches/
cargo run -- --config config.local.toml --output out.png --size 1920x1080 --once
just check                       # fmt --check, clippy, tests
just integration                 # sync the Drive development copy, run tests/drive_integration.rs
just render                      # sync Drive (development copy) with rclone, render out.png
just env=production run          # same, against the production Drive copy
```

The `justfile` needs `just` and rclone (`rclone config create gdrive drive scope=drive.readonly`).

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
- Memory beats speed. The board has 1 GB and a new collage is needed only every few minutes, so
  prefer lower peak memory over fewer CPU cycles, and back memory claims with a measurement.
- Every module ships with its tests; performance- or memory-sensitive modules also ship with a
  benchmark or a heap-budget test in the same PR.

## Code rules (prunable)

Edit or delete these freely as the code changes.

- Memory is guarded by `tests/memory.rs` (`dhat`, one profiler per process, so keep it the only
  test in that file). If you change decoding or frame handling, run it and update the budget and
  the figures in `docs/ARCHITECTURE.md` from the measured value.
- Benchmarks live in `benches/` with `harness = false`; the lib and bin set `bench = false` so
  `cargo bench` runs only Criterion. Add a `[[bench]]` table for each new file.
- `[profile.dev] opt-level = 2` is deliberate: generic `image` code is compiled in this crate and
  is ~10x slower at opt-level 0, which makes the 12 MP tests crawl.
- Clippy `assert_is_empty` rejects `assert!(x.is_empty())`; use `assert_eq!(x, vec![])` or compare
  `.len()` with 0.
- `Display` implementations receive full `RgbImage` frames; only `display::Geometry::encode_into`
  knows the framebuffer pixel format.

- rand 0.10 gotcha: `random_range` and similar convenience methods live on `RngExt`, so write
  `use rand::{Rng, RngExt};`. Slice helpers are in `rand::seq::{IndexedRandom, SliceRandom}`.
- The framebuffer's `stride` can exceed the visible width times bytes per pixel. Read `stride`
  and `bits_per_pixel` from sysfs rather than assuming 4 * width.
- Reuse frame buffers between slides; do not allocate a new frame per fade step.
- Decode one photo at a time and drop it before decoding the next.
- Skip unreadable or corrupt images with a `tracing::warn!`; one bad file must not stop the show.
- Clippy pedantic is enabled with a short allow-list in `Cargo.toml`. Fix warnings rather than
  adding `#[allow]`; if an allow is justified, add a one-line reason.
