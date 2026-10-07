# Architecture

## Data flow

```mermaid
flowchart LR
    GD[Shared Google Drive folder] -- rclone sync --> LIB[(Library root on disk)]
    LIB --> L[library: pick subject and files]
    CFG[config] --> S
    L --> S[Slideshow loop in main]
    S --> LY[layout: random binary splits]
    LY --> C[collage: decode one photo at a time, scale, place]
    C --> F[fade: blend previous and next frame]
    F --> D{{Display trait}}
    D --> FB[/dev/fb0]
    D --> PNG[PNG file in dev mode]
```

Per slide: pick a subject, choose `tiles` images from it, compute a layout for the frame size,
render the collage into the "next" frame, cross-fade from "previous" to "next" through the
`Display`, then hold for `interval_secs`. "Next" becomes "previous" and the cycle repeats.

## Modules

| Module | Role |
| --- | --- |
| `config` | Parse TOML, apply defaults, validate |
| `layout` | Pure function from (size, tiles, gap, RNG) to tile rectangles |
| `library` | Walk the library root; subjects are its immediate subfolders |
| `collage` | Decode, scale and place photos into a frame |
| `display` | `Display` trait; framebuffer and PNG implementations |
| `fade` | Blend two frames into a scratch frame, push each step to a `Display` |
| `main` | CLI, logging, `Slideshow` |

## Memory budget at 1920x1080

These are estimates from pixel counts, not measurements.

| Item | Size |
| --- | --- |
| Previous frame, RGB (1920 x 1080 x 3) | about 6.2 MB |
| Next frame, RGB | about 6.2 MB |
| Blend scratch frame, RGB | about 6.2 MB |
| Encode buffer in framebuffer pixel format (32 bpp) | about 8.3 MB |
| One decoded photo at a time (a 12 MP photo is about 36 MB as RGB) | transient, up to roughly 40 MB |
| Total working set | roughly 70 MB |

Decoding one photo at a time and dropping it before the next keeps the peak well under the
1 GB board limit, leaving room for the OS and page cache. The systemd unit also sets
`MemoryMax` as a safety net.

## Why the framebuffer

The board only has to show pictures on a TV. Writing to `/dev/fb0` needs no X server, no
Wayland compositor and no GPU stack, which saves RAM, boot time and moving parts. The cost is
that the program must respect the framebuffer's geometry (`virtual_size`, `stride`,
`bits_per_pixel`) and convert its frames to that pixel format.

## Why the fade is designed this way

- Both frames already exist in memory when the fade starts, so each step is a per-pixel linear
  blend of two buffers into a reused scratch frame. No decoding happens during the fade.
- The scratch frame is converted into the encode buffer and written to the display once per step,
  so the fade cost is bounded by memory bandwidth, not by image size on disk.
- The `Display` trait receives finished frames, so the fade is testable with a fake display and
  the same code drives both the framebuffer and the PNG dev output.
