<#
Run the slideshow on this PC the way the board does: pull photos from Google Drive with rclone
(same flags as deploy/orange-you-glad-sync.service), then run the real binary, writing frames to
.preview/screen.png. A window opens showing the screen live (F11 for fullscreen).

Prerequisite (once): install rclone and create a remote named "gdrive", see docs/DEPLOY.md step 6.
Use -SkipSync to reuse photos already in .preview/photos.
#>
param(
    [string]$Remote = "gdrive:",
    [int]$IntervalSecs = 20,
    [string]$Size = "1920x1080",
    [switch]$SkipSync
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$dir = Join-Path $root ".preview"
$photos = Join-Path $dir "photos"
New-Item -ItemType Directory -Force $photos | Out-Null

if (-not $SkipSync) {
    if (-not (Get-Command rclone -ErrorAction SilentlyContinue)) {
        throw "rclone not found. Install it (winget install Rclone.Rclone) and configure a 'gdrive' remote."
    }
    rclone sync $Remote $photos --ignore-case --include "/Processed/*.{jpg,jpeg,png,webp}" `
        --include "/Config/*.{json,toml}" --max-size 25M --drive-skip-gdocs --transfers 2 --log-level INFO
    if ($LASTEXITCODE -ne 0) { throw "rclone sync failed" }
}

$toml = Join-Path $dir "config.toml"
@"
library_root = "$($photos.Replace([char]92,[char]47))"
interval_secs = $IntervalSecs
"@ | Set-Content -Encoding utf8 $toml

if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = "C:\t\oyg" }
$viewer = "file:///" + (Join-Path $PSScriptRoot "viewer.html").Replace([char]92,[char]47)
# --app gives a bare window with no tabs or address bar, close to what the TV shows.
Start-Process msedge -ArgumentList "--app=$viewer", "--window-size=1280,760"
cargo run --release --manifest-path (Join-Path $root "Cargo.toml") -- `
    --config $toml --output (Join-Path $dir "screen.png") --size $Size


