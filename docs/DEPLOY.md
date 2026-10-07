# Deploying on an Orange Pi Zero 2W

> Status: these steps are written from documentation and general Linux practice. They have NOT
> been tested on hardware. Expect to adjust paths, kernel settings and package names. Please
> open an issue or PR with corrections.

Assumes Debian or Armbian (aarch64), HDMI connected to the TV, and a user with `sudo`.

## 1. Verify the framebuffer

```sh
ls -l /dev/fb0
cat /sys/class/graphics/fb0/virtual_size
cat /sys/class/graphics/fb0/stride
cat /sys/class/graphics/fb0/bits_per_pixel
```

Expect something like `1920,1080`, a stride of at least `width * bits_per_pixel / 8`, and 32.

If `/dev/fb0` does not exist:

- Check the TV was connected and powered at boot, and try another HDMI port or cable.
- Check `dmesg | grep -i -E "drm|fb|hdmi"` for display driver messages.
- Use a recent Armbian image whose kernel enables the sunxi DRM driver with fbdev emulation
  (`CONFIG_DRM_FBDEV_EMULATION`). Check with `zcat /proc/config.gz | grep FBDEV` where available.
- If the sysfs files exist but report a resolution other than 1080p, set the mode with a
  kernel `video=` parameter, for example `video=HDMI-A-1:1920x1080@60`.

## 2. Keep the console from blanking or drawing a cursor

Add kernel arguments. On Armbian, edit `/boot/armbianEnv.txt`:

```
extraargs=consoleblank=0 vt.global_cursor_default=0
```

On other images, append them to the kernel command line used by your bootloader. Reboot and check
with `cat /proc/cmdline`.

## 3. Create a service user

```sh
sudo useradd --system --home-dir /var/lib/orange-you-glad --create-home \
  --shell /usr/sbin/nologin --groups video orange-you-glad
sudo install -d -o orange-you-glad -g orange-you-glad /var/lib/orange-you-glad/photos
```

## 4. Install the binary

Download the aarch64 tarball and its `.sha256` from the GitHub release, verify, and install:

```sh
sha256sum -c orange-you-glad-*.tar.gz.sha256
tar -xzf orange-you-glad-*.tar.gz
sudo install -m 0755 orange-you-glad-*/orange-you-glad /usr/local/bin/
```

Or build from source on a faster machine with
`cargo build --release --target aarch64-unknown-linux-gnu` (needs `gcc-aarch64-linux-gnu`).

## 5. Configure

```sh
sudo install -m 0644 deploy/config.example.toml /etc/orange-you-glad.toml
sudoedit /etc/orange-you-glad.toml   # uncomment library_root and any other keys you want
```

## 6. Set up rclone for a shared Google Drive folder (headless)

Install rclone (`sudo apt install rclone`; the packaged version may be old, so consider the
official install script from rclone.org if the options below are rejected).

The board has no browser, so authorize on another machine that has rclone and a browser:

```sh
rclone authorize "drive" "scope=drive.readonly"
```

Sign in, approve, and copy the JSON token it prints. Find the shared folder's ID in its Google
Drive URL (`https://drive.google.com/drive/folders/<FOLDER_ID>`). Then on the board, as the
service user:

```sh
sudo -u orange-you-glad env RCLONE_CONFIG=/var/lib/orange-you-glad/rclone.conf rclone config
```

Choose a new remote named `gdrive`, type `drive`, leave client id and secret blank, scope
`drive.readonly` (read-only access), set `root_folder_id` to the folder ID, answer "no" to
auto config, and paste the token. Then restrict the file:

```sh
sudo chmod 600 /var/lib/orange-you-glad/rclone.conf
```

Alternative if you do not know the folder ID: skip `root_folder_id` and add
`--drive-shared-with-me` to the sync command; the remote root then lists everything shared with
the account, so each shared folder becomes a subject.

Test a dry run:

```sh
sudo -u orange-you-glad env RCLONE_CONFIG=/var/lib/orange-you-glad/rclone.conf \
  rclone sync gdrive: /var/lib/orange-you-glad/photos --dry-run \
  --ignore-case --include "*.{jpg,jpeg,png,webp}" --max-size 25M
```

Note that `rclone sync` makes the destination match the source, deleting local files that are no
longer in Drive. Never point it at a directory holding anything else.

## 7. Install the systemd units

```sh
sudo install -m 0644 deploy/orange-you-glad.service \
  deploy/orange-you-glad-sync.service deploy/orange-you-glad-sync.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl start orange-you-glad-sync.service      # first sync, may take a while
sudo systemctl enable --now orange-you-glad-sync.timer
sudo systemctl enable --now orange-you-glad.service
```

The slideshow unit runs as the service user with `SupplementaryGroups=video`, restarts always,
and applies sandboxing options. If it fails to start after a hardening change, relax options one
at a time while reading `journalctl -u orange-you-glad`.

## Troubleshooting

| Symptom | Things to check |
| --- | --- |
| Service exits immediately, permission denied on `/dev/fb0` | `ls -l /dev/fb0`; the user must be in `video`; `DeviceAllow=/dev/fb0 rw` in the unit |
| Screen is blank or the console reappears | Another process owns the display; stop any `getty` or desktop on tty1; verify the kernel arguments from step 2 |
| Screen blanks after a few minutes | `consoleblank=0` missing from `/proc/cmdline` |
| Garbled or shifted image | Resolution or stride mismatch; recheck `virtual_size`, `stride`, `bits_per_pixel` |
| No photos shown | `library_root` wrong, or no subfolders containing `.jpg`, `.jpeg`, `.png` or `.webp` files; run with `RUST_LOG=debug` |
| Sync fails with auth errors | Token expired or wrong scope; redo `rclone authorize` and `rclone config reconnect gdrive:` |
| Sync copies nothing | Wrong `root_folder_id`, or the folder is not shared with the authorizing account |
| Out-of-memory kills | Check `journalctl -k`; reduce `tiles`; lower `--max-size`; confirm swap or zram |
| Blinking cursor visible | `vt.global_cursor_default=0` missing from the kernel arguments |
