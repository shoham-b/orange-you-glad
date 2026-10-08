# Drive-backed development. One-time setup: rclone config create gdrive drive scope=drive.readonly
# Pick the Drive copy with `just env=production render`; the default is development.

set windows-shell := ["powershell.exe", "-NoProfile", "-Command"]

env := "development"
# Drive folder IDs under "Orange You Glad".
folder := if env == "production" { "1foxxmTR49YCuahiHT8zbkNTgLo2DIKOS" } else if env == "development" { "1Ft5AAQ4Upz8UzcSyc2xIb1gThZaXgl_L" } else { error("env must be production or development") }
photos := "scratch/photos/" + env
config := "config." + env + ".local.toml"
root := replace(justfile_directory(), "\\", "/")

default:
    @just --list

# fmt, clippy and tests, as in CI
check:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# Mirror Processed/ and Config/ of the Drive env into scratch/photos/<env>; rclone deletes anything else there.
sync:
    rclone sync "gdrive,root_folder_id={{folder}}:" {{photos}} --ignore-case --include "/Processed/*.{jpg,jpeg,png,webp}" --include "/Config/*.{json,toml}" --max-size 25M --max-delete 50 --drive-skip-gdocs --transfers 2

# Point a git-ignored config at the synced folder.
config:
    @echo 'library_root = "{{root}}/{{photos}}"' | Out-File -Encoding ascii {{config}}

# Sync, then render one collage to out.png (no TV needed).
render: sync config
    cargo run --release -- --config {{config}} --output out.png --once

# Sync, then keep running, rewriting out.png each cycle.
run: sync config
    cargo run --release -- --config {{config}} --output out.png
