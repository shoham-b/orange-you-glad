# Drive-backed development. One-time setup: copy .env.example to .env, point it at the service-account key, run `just drive-auth`.
# Pick the Drive copy with `just env=production render`; the default is development.

set dotenv-load

set windows-shell := ["powershell.exe", "-NoProfile", "-Command"]

env := "development"
# Drive folder IDs under "Orange You Glad".
folder := if env == "production" { "1foxxmTR49YCuahiHT8zbkNTgLo2DIKOS" } else if env == "development" { "1Ft5AAQ4Upz8UzcSyc2xIb1gThZaXgl_L" } else { error("env must be production or development") }
photos := "scratch/photos/" + env
config := "config." + env + ".local.toml"
export OYG_LIBRARY := replace(justfile_directory(), "\\", "/") + "/" + photos
root := replace(justfile_directory(), "\\", "/")

default:
    @just --list

# fmt, clippy and tests, as in CI
check:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# One-time: create the rclone `gdrive` remote from the service-account key named in .env (no browser).
drive-auth:
    rclone config create gdrive drive service_account_file=$env:GDRIVE_SERVICE_ACCOUNT_FILE scope=drive.readonly

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

# Sync, then keep running in a desktop window (Esc or close to quit).
window: sync config
    cargo run --release --features window -- --config {{config}} --window

# Window preview of a generated library (60 mixed-shape pictures), no Drive needed.
window-sample:
    cargo run --example sample_library -- scratch/photos/sample
    @echo 'library_root = "{{root}}/scratch/photos/sample"' | Out-File -Encoding ascii config.sample.local.toml
    cargo run --release --features window -- --config config.sample.local.toml --window

# Sync the development Drive copy, then run the ignored tests in tests/drive_integration.rs.
integration: sync
    cargo test --test drive_integration -- --ignored

# Generate the shape pictures into scratch/drive-upload to copy into the Drive development folder (Processed/*, and merge Config/subjects.json into the existing one).
sample-for-drive:
    cargo run --example sample_library -- scratch/drive-upload
