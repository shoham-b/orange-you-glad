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

The board needs a clock that is right at boot (it has no RTC), or Google rejects its tokens and
TLS fails. Check `timedatectl` shows `System clock synchronized: yes`; enable `systemd-timesyncd`
or `chrony` if not.

Find the shared folder's ID in its Google Drive URL
(`https://drive.google.com/drive/folders/<FOLDER_ID>`), then pick one way to authenticate.

### Option A: service account (recommended)

Nothing expires and nobody has to sign in again.

1. In Google Cloud Console, create a project, enable the Google Drive API, create a service
   account and download a JSON key.
2. Share the Drive folder with the service account's email address as Viewer.
3. Copy the key to the board and lock it down:

```sh
sudo install -m 0600 -o orange-you-glad -g orange-you-glad key.json \
  /var/lib/orange-you-glad/service-account.json
```

4. As the service user, run `rclone config`: new remote `gdrive`, type `drive`, scope
   `drive.readonly`, `service_account_file` set to the path above, `root_folder_id` set to the
   folder ID, no auto config.

```sh
sudo -u orange-you-glad env RCLONE_CONFIG=/var/lib/orange-you-glad/rclone.conf rclone config
```

Never commit the key. The repository ignores `*.sa.json` and `secrets/`; keep it outside the
checkout anyway.

### Option B: OAuth token authorized on another machine

The board has no browser, so create the remote on a machine that has rclone and a browser:

1. Create your own OAuth client ID in Google Cloud Console (type "Desktop app") and use it when
   `rclone config` asks for client id and secret. rclone's shared client is heavily rate limited.
2. Choose scope `drive.readonly`, set `root_folder_id` to the folder ID, and sign in. The scope
   is fixed when you consent, so choose it before signing in.
3. Copy the resulting `[gdrive]` section from that machine's `rclone config file` into
   `/var/lib/orange-you-glad/rclone.conf` on the board.

Warning: an OAuth consent screen left in "Testing" status makes refresh tokens expire after
7 days, and the sync then fails silently. Set the app to "In production" (fine for personal use)
or use Option A.

### Both options

Restrict the config file, which also holds the token that rclone refreshes:

```sh
sudo chmod 600 /var/lib/orange-you-glad/rclone.conf
```

Set `root_folder_id` to the folder that directly contains `Incoming/`, `Processed/`, `Errored/` and
`Config/`. Pictures are `Processed/<uuid>.<ext>`; `Config/subjects.json` maps subject names to
uuid lists, for example `{"dog": ["<uuid>", "<uuid>"], "ski": ["<uuid>"]}`. A uuid may appear
under several subjects.

`Config/family.json` gives the Hebrew birthday of family members, for example
`{"Dana": "15/5", "Omer": "27/1"}`. The name must match a subject in `subjects.json` (ignoring
case); that subject holds the pictures, so pictures are only ever added in one place. A birthday is
`day/month` with months counted from Tishrei = 1: Cheshvan 2, Kislev 3, Tevet 4, Shevat 5, Adar 6,
Nisan 7, Iyar 8, Sivan 9, Tamuz 10, Av 11, Elul 12 (so `15/5` is 15 Shevat). In a leap year write
`6a` for Adar I or `6b` for Adar II (plain `6` means Adar II); in a year with only one Adar all of
them fall in it. A 30 Cheshvan or 30 Kislev birthday is kept on the 29th in years when the month is
short. On that Hebrew date (by the board's local date, so set its timezone) only the birthday
person's subject is shown. A missing file means no birthdays; an invalid one is logged and ignored.

Test a dry run:

```sh
sudo -u orange-you-glad env RCLONE_CONFIG=/var/lib/orange-you-glad/rclone.conf \
  rclone sync gdrive: /var/lib/orange-you-glad/photos --dry-run --max-delete 50 \
  --ignore-case --include "/Processed/*.{jpg,jpeg,png,webp}" --include "/Config/*.{json,toml}" \
  --max-size 25M
```

The Drive folder holds `Incoming/`, `Processed/`, `Errored/` and `Config/`. The `--include` rules
mirror only `Processed/` and `Config/`; `Incoming/` and `Errored/` belong to the intake agent and
are never downloaded. Keep these rules in step with the sync service unit's command.

Note that `rclone sync` makes the destination match the source, deleting local files that are no
longer in Drive. Never point it at a directory holding anything else. `--max-delete 50` aborts a
run that would delete more than 50 files, which protects the library when the folder ID is wrong,
access is revoked or Drive lists empty by mistake. Raise it if you legitimately remove many
photos at once.

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

## Settings from Drive

Put a file named `settings.toml` in the `Config/` folder of the shared Drive folder. After the
next sync the slideshow picks it up, no restart needed. It may set `interval_secs`, `tiles`,
`gap_px`, `fade_ms` and `background` (same meaning as in `/etc/orange-you-glad.toml`); unset keys
keep the local values. `library_root` and `framebuffer` cannot be changed this way.

The board re-reads the file every 60 seconds, so a new `interval_secs` applies within a minute.
While the file has a typo or an invalid value, the last good settings stay in force and the board
re-reads it every 10 seconds until it is fixed. It retries at the same pace when no collage can be
built (for example `subjects.json` is missing or invalid). Each problem is logged once in the
journal, and again when it recovers.

## Troubleshooting

| Symptom | Things to check |
| --- | --- |
| Service exits immediately, permission denied on `/dev/fb0` | `ls -l /dev/fb0`; the user must be in `video`; `DeviceAllow=/dev/fb0 rw` in the unit |
| Screen is blank or the console reappears | Another process owns the display; stop any `getty` or desktop on tty1; verify the kernel arguments from step 2 |
| Screen blanks after a few minutes | `consoleblank=0` missing from `/proc/cmdline` |
| Garbled or shifted image | Resolution or stride mismatch; recheck `virtual_size`, `stride`, `bits_per_pixel` |
| No photos shown | `library_root` wrong, `Config/subjects.json` missing or invalid, or its uuids have no matching file in `Processed/` yet (unsynced uuids are logged as warnings); run with `RUST_LOG=debug` |
| Sync fails with auth errors | Wrong system clock (`timedatectl`); expired token (an OAuth app in "Testing" status expires after 7 days: run `rclone config reconnect gdrive:` or switch to a service account); wrong scope |
| Sync aborts with a max-delete error | Drive listed far fewer files than expected; check `root_folder_id` and that the folder is still shared, then raise `--max-delete` if the removal was intended |
| Sync copies nothing | Wrong `root_folder_id`, or the folder is not shared with the authorizing account |
| Out-of-memory kills | Check `journalctl -k`; reduce `tiles`; lower `--max-size`; confirm swap or zram |
| Blinking cursor visible | `vt.global_cursor_default=0` missing from the kernel arguments |

## Previewing on a PC

`scripts/preview.ps1` mimics the board on Windows: it syncs the Drive folder with the same rclone
flags as the sync unit into `.preview/photos`, runs the app with `--output .preview/screen.png`,
and opens `scripts/viewer.html` in an Edge app window that shows the frames live, fades included.
Needs rclone with a `gdrive` remote (step 6). Use `-SkipSync` to reuse downloaded photos and
`-IntervalSecs` to change the slide interval. It does not exercise the framebuffer code.
