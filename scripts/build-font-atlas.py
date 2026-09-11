#!/usr/bin/env python3
"""Rasterize a TrueType font into an Agel font atlas (AGF1) for the compositor.

    build-font-atlas.py FONT.ttf OUT.agf [--sizes 12,14,16,20,24,32]

The atlas is what the unprivileged compositor reads: for each pixel size, a
table of the 96 printable ASCII glyphs (32 to 127, the last drawn as the
replacement glyph) with their metrics, and an 8-bit coverage bitmap per
glyph. Everything is little-endian and every offset is absolute, so the
compositor bounds-checks each against the asset's length.

    header    "AGF1", u16 size count, u16 glyph count, u32 reserved
    per size  u16 px, i16 ascent, i16 descent, i16 line height, u32 glyph
              table offset, u32 reserved                       (16 bytes)
    glyph row u8 advance, i8 bearing x, i8 bearing y (above the baseline),
              u8 width, u8 height, u8 x3 reserved, u32 bitmap offset (12 bytes)

Needs Pillow with FreeType (`pip install pillow`, or Debian's python3-pil).
"""

from __future__ import annotations

import struct
import sys

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:  # pragma: no cover - reported to the operator
    print("build-font-atlas.py needs Pillow (pip install pillow)", file=sys.stderr)
    sys.exit(2)

GLYPHS = list(range(32, 128))
DEFAULT_SIZES = [12, 14, 16, 20, 24, 32]


def render(font: ImageFont.FreeTypeFont, codepoint: int) -> tuple[int, int, int, int, int, bytes]:
    character = chr(codepoint) if codepoint < 127 else "�"
    if character == "�":
        character = "?"
    # Measure against a generous canvas so descenders and overhangs fit.
    canvas = Image.new("L", (font.size * 3, font.size * 3), 0)
    draw = ImageDraw.Draw(canvas)
    origin = (font.size, font.size)
    draw.text(origin, character, font=font, fill=255)
    advance = int(round(font.getlength(character)))
    box = canvas.getbbox()
    if box is None:
        return advance, 0, 0, 0, 0, b""
    left, top, right, bottom = box
    ascent, _ = font.getmetrics()
    glyph = canvas.crop((left, top, right, bottom))
    width, height = glyph.size
    bearing_x = left - origin[0]
    # Pillow draws with the top of the em box at `origin`; the baseline is
    # `ascent` below it. bearing_y is the glyph's top above the baseline.
    bearing_y = (origin[1] + ascent) - top
    return advance, bearing_x, bearing_y, width, height, glyph.tobytes()


def build(path: str, sizes: list[int]) -> bytes:
    faces = []
    for px in sizes:
        font = ImageFont.truetype(path, px)
        ascent, descent = font.getmetrics()
        glyphs = [render(font, codepoint) for codepoint in GLYPHS]
        faces.append((px, ascent, descent, glyphs))
    header = 12
    size_headers = 16 * len(sizes)
    tables = 12 * len(GLYPHS) * len(sizes)
    bitmap_offset = header + size_headers + tables
    out = bytearray()
    out += b"AGF1" + struct.pack("<HHI", len(sizes), len(GLYPHS), 0)
    table_offset = header + size_headers
    bitmaps = bytearray()
    rows = bytearray()
    for px, ascent, descent, glyphs in faces:
        line_height = ascent + descent + max(2, px // 6)
        out += struct.pack("<HhhhII", px, ascent, descent, line_height, table_offset, 0)
        for advance, bearing_x, bearing_y, width, height, bitmap in glyphs:
            offset = bitmap_offset + len(bitmaps)
            rows += struct.pack(
                "<BbbBBBBBI",
                min(advance, 255),
                max(-128, min(127, bearing_x)),
                max(-128, min(127, bearing_y)),
                min(width, 255),
                min(height, 255),
                0, 0, 0,
                offset,
            )
            bitmaps += bitmap
        table_offset += 12 * len(GLYPHS)
    out += rows
    assert len(out) == bitmap_offset, (len(out), bitmap_offset)
    out += bitmaps
    return bytes(out)


def main() -> int:
    arguments = sys.argv[1:]
    sizes = DEFAULT_SIZES
    if "--sizes" in arguments:
        index = arguments.index("--sizes")
        sizes = [int(value) for value in arguments[index + 1].split(",")]
        del arguments[index : index + 2]
    if len(arguments) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    atlas = build(arguments[0], sizes)
    with open(arguments[1], "wb") as out:
        out.write(atlas)
    print(f"{arguments[1]}: {len(sizes)} sizes, {len(GLYPHS)} glyphs, {len(atlas)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
