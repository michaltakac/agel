#!/usr/bin/env python3
"""Draw the desktop's sprites into an Agel sprite sheet (AGI1) for the compositor.

    build-sprites.py OUT.agi

Every sprite is drawn here with Pillow, at four times its size and scaled
down, so its edges are anti-aliased. White glyphs on transparency, tinted by
the compositor where a colour is wanted; the cursor carries its own colours.

    header  "AGI1", u16 count, u16 reserved, u32 reserved
    sprite  u16 width, u16 height, u32 pixel offset, u32 x2 reserved (16 bytes)
    pixels  RGBA8, row-major, straight alpha

Sprite ids, which the scene names:
    0 cursor 24x32        1 agel 32x32       2 terminal 32x32   3 files 32x32
    4 editor 32x32        5 settings 32x32   6 store 32x32      7 help 32x32
    8 minimize 16x16      9 maximize 16x16  10 close 16x16     11 search 20x20
"""

from __future__ import annotations

import math
import struct
import sys

try:
    from PIL import Image, ImageDraw
except ImportError:  # pragma: no cover
    print("build-sprites.py needs Pillow (pip install pillow)", file=sys.stderr)
    sys.exit(2)

SCALE = 4
WHITE = (255, 255, 255, 255)


def canvas(width: int, height: int):
    image = Image.new("RGBA", (width * SCALE, height * SCALE), (0, 0, 0, 0))
    return image, ImageDraw.Draw(image), SCALE


def finish(image: Image.Image, width: int, height: int) -> Image.Image:
    return image.resize((width, height), Image.LANCZOS)


def cursor() -> Image.Image:
    image, draw, s = canvas(24, 32)
    outline = [(2, 2), (2, 26), (8, 20), (12, 30), (16, 28), (12, 18), (20, 18)]
    points = [(x * s, y * s) for x, y in outline]
    draw.polygon(points, fill=(27, 27, 27, 255))
    inner = [(4, 6), (4, 21), (8.5, 17), (12.5, 26.5), (14, 26), (10.5, 17), (16, 17)]
    draw.polygon([(x * s, y * s) for x, y in inner], fill=WHITE)
    return finish(image, 24, 32)


def agel() -> Image.Image:
    image, draw, s = canvas(32, 32)
    draw.ellipse([4 * s, 4 * s, 28 * s, 28 * s], outline=WHITE, width=3 * s)
    draw.ellipse([12 * s, 12 * s, 20 * s, 20 * s], fill=WHITE)
    return finish(image, 32, 32)


def terminal() -> Image.Image:
    image, draw, s = canvas(32, 32)
    draw.rounded_rectangle([3 * s, 5 * s, 29 * s, 27 * s], radius=3 * s, outline=WHITE, width=2 * s)
    draw.line([(8 * s, 12 * s), (13 * s, 16 * s), (8 * s, 20 * s)], fill=WHITE, width=2 * s, joint="curve")
    draw.line([(15 * s, 21 * s), (23 * s, 21 * s)], fill=WHITE, width=2 * s)
    return finish(image, 32, 32)


def files() -> Image.Image:
    image, draw, s = canvas(32, 32)
    draw.rounded_rectangle([3 * s, 9 * s, 29 * s, 27 * s], radius=3 * s, fill=WHITE)
    draw.rounded_rectangle([3 * s, 5 * s, 15 * s, 12 * s], radius=2 * s, fill=WHITE)
    draw.rectangle([5 * s, 13 * s, 27 * s, 14 * s], fill=(0, 0, 0, 0))
    return finish(image, 32, 32)


def editor() -> Image.Image:
    image, draw, s = canvas(32, 32)
    draw.rounded_rectangle([6 * s, 3 * s, 26 * s, 29 * s], radius=3 * s, outline=WHITE, width=2 * s)
    for y in (11, 16, 21):
        draw.line([(11 * s, y * s), ((21 if y < 21 else 17) * s, y * s)], fill=WHITE, width=2 * s)
    return finish(image, 32, 32)


def settings() -> Image.Image:
    image, draw, s = canvas(32, 32)
    center = 16 * s
    for tooth in range(8):
        angle = tooth * math.pi / 4
        x = center + math.cos(angle) * 11 * s
        y = center + math.sin(angle) * 11 * s
        draw.ellipse([x - 3 * s, y - 3 * s, x + 3 * s, y + 3 * s], fill=WHITE)
    draw.ellipse([center - 9 * s, center - 9 * s, center + 9 * s, center + 9 * s], fill=WHITE)
    draw.ellipse([center - 4 * s, center - 4 * s, center + 4 * s, center + 4 * s], fill=(0, 0, 0, 0))
    return finish(image, 32, 32)


def store() -> Image.Image:
    image, draw, s = canvas(32, 32)
    draw.rounded_rectangle([5 * s, 11 * s, 27 * s, 28 * s], radius=3 * s, fill=WHITE)
    draw.arc([10 * s, 3 * s, 22 * s, 17 * s], start=180, end=360, fill=WHITE, width=2 * s)
    draw.rectangle([9 * s, 10 * s, 23 * s, 12 * s], fill=(0, 0, 0, 0))
    return finish(image, 32, 32)


def help_icon() -> Image.Image:
    image, draw, s = canvas(32, 32)
    draw.ellipse([3 * s, 3 * s, 29 * s, 29 * s], outline=WHITE, width=2 * s)
    draw.arc([10 * s, 8 * s, 22 * s, 18 * s], start=200, end=360 + 20, fill=WHITE, width=2 * s)
    draw.line([(19 * s, 16 * s), (16 * s, 18 * s), (16 * s, 21 * s)], fill=WHITE, width=2 * s)
    draw.ellipse([14.5 * s, 23 * s, 17.5 * s, 26 * s], fill=WHITE)
    return finish(image, 32, 32)


def minimize() -> Image.Image:
    image, draw, s = canvas(16, 16)
    draw.line([(3 * s, 8 * s), (13 * s, 8 * s)], fill=WHITE, width=2 * s)
    return finish(image, 16, 16)


def maximize() -> Image.Image:
    image, draw, s = canvas(16, 16)
    draw.rectangle([3 * s, 3 * s, 13 * s, 13 * s], outline=WHITE, width=2 * s)
    return finish(image, 16, 16)


def close() -> Image.Image:
    image, draw, s = canvas(16, 16)
    draw.line([(3 * s, 3 * s), (13 * s, 13 * s)], fill=WHITE, width=2 * s)
    draw.line([(13 * s, 3 * s), (3 * s, 13 * s)], fill=WHITE, width=2 * s)
    return finish(image, 16, 16)


def search() -> Image.Image:
    image, draw, s = canvas(20, 20)
    draw.ellipse([2 * s, 2 * s, 13 * s, 13 * s], outline=WHITE, width=2 * s)
    draw.line([(12 * s, 12 * s), (18 * s, 18 * s)], fill=WHITE, width=3 * s)
    return finish(image, 20, 20)


SPRITES = [cursor, agel, terminal, files, editor, settings, store, help_icon, minimize, maximize, close, search]


def build() -> bytes:
    images = [draw() for draw in SPRITES]
    header = 8 + 16 * len(images)
    table = bytearray()
    pixels = bytearray()
    for image in images:
        width, height = image.size
        table += struct.pack("<HHIII", width, height, header + len(pixels), 0, 0)
        pixels += image.tobytes()
    out = bytearray(b"AGI1" + struct.pack("<HHI", len(images), 0, 0)[:4])
    out += table + pixels
    return bytes(out)


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    sheet = build()
    with open(sys.argv[1], "wb") as out:
        out.write(sheet)
    print(f"{sys.argv[1]}: {len(SPRITES)} sprites, {len(sheet)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
