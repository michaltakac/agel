# Agel v0.2.47 — Typography and surfaces: the desktop starts toward COSMIC

The native desktop had a 5×7 bitmap font, flat rectangles and no
transparency. This release gives it the means to look like a desktop: real
typefaces, blended surfaces with anti-aliased corners, soft shadows, and a
scene styled with COSMIC's tokens. It is the first release of the graphics
track; the rest is listed at the end.

## What changed

- **An asset region on the disk.** Sectors 3072 through 6143 hold a table
  and files, like the program region; the x86-64 image is 6,144 sectors.
  `scripts/install-asset.py` writes them; `scripts/build-boot.sh` installs
  the font atlases on every rebuild. The installer repacks a region on every
  install, so replacing an entry no longer leaves a hole (the program region
  had the same flaw).
- **Font atlases.** `scripts/build-font-atlas.py` rasterizes a TrueType
  font with Pillow into `AGF1`: per pixel size, the 96 printable ASCII
  glyphs with metrics and 8-bit coverage bitmaps. Fira Sans Regular and
  Medium at 12 to 32 px and Fira Mono at 12 to 20 px, bundled under the SIL
  Open Font License.
- **The compositor draws text, surfaces and shadows.** The graphics
  supervisor reads each atlas through the storage driver, checks its CRC,
  maps it read-only into the compositor, and keeps the metrics for layout.
  Three record operations: a label (anti-aliased, alpha-blended text in a
  face and size), a surface (a rounded box blended with an alpha and
  anti-aliased corners), and a shadow (a linear falloff around a box). The
  compositor checks every atlas offset against the asset's length.
- **A restyled scene.** COSMIC's dark palette, 8 and 16 px radii and
  8/16/24 px spacing: a top panel with workspaces, a workshop window with a
  title bar, a sidebar of spaces and agents, two cards and three actions, a
  floating dock of six tiles, and a command field in Fira Mono. The accent
  intents recolour it; the frame budget is 160 records.

## Proof

`scripts/test-graphics.sh` boots the self-test image and requires the
three atlases to load, the frozen digest of the 82-record frame, rejection
without mutation, and fault containment; the live-desktop, keyboard,
console, workbench, dock and module suites run against the new scene. The
frame is in [`docs/native-graphics.md`](native-graphics.md).

## Not claimed

The display is still 1024×768. There are no icons beyond drawn shapes, no
windows a process owns, no pointer cursor beyond a square, and the workshop
is the only window. The shadow is a linear falloff, not a Gaussian blur;
the wallpaper is a gradient with soft glows, not an image. The look is a
first pass at COSMIC's, from its published tokens, not a port of it.
