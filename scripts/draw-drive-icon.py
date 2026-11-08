#!/usr/bin/env python3
"""Draw the S3Drive Explorer drive icon (packaging/windows/s3drive.ico).

The committed .ico is the packaged artifact; this script only regenerates it.
It needs Pillow, which the installer build and CI do not require. Shapes are
drawn at 4x and downsampled so small Explorer sizes stay smooth.
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'packaging/windows/s3drive.ico'
SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
SCALE = 4
TOP, BOTTOM = (37, 99, 235), (30, 64, 175)


def gradient(size):
    """Vertical blue gradient filling a square of `size` pixels."""
    image = Image.new('RGBA', (size, size))
    draw = ImageDraw.Draw(image)
    for y in range(size):
        t = y / (size - 1)
        colour = tuple(round(a + (b - a) * t) for a, b in zip(TOP, BOTTOM))
        draw.line([(0, y), (size, y)], fill=colour + (255,))
    return image


def tile(size):
    """Rounded-square background tile with the gradient."""
    mask = Image.new('L', (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1], radius=size // 5, fill=255)
    image = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    image.paste(gradient(size), (0, 0), mask)
    return image


def bucket(draw, size):
    """White storage bucket: tapered body, open rim and a band."""
    u = size / 256
    body = [(64 * u, 92 * u), (192 * u, 92 * u), (172 * u, 206 * u), (84 * u, 206 * u)]
    draw.polygon(body, fill=(255, 255, 255, 255))
    draw.ellipse([64 * u, 72 * u, 192 * u, 112 * u], fill=(255, 255, 255, 255))
    draw.ellipse([76 * u, 80 * u, 180 * u, 104 * u], fill=(191, 219, 254, 255))
    draw.ellipse([84 * u, 192 * u, 172 * u, 218 * u], fill=(255, 255, 255, 255))
    draw.line([(70 * u, 140 * u), (186 * u, 140 * u)], fill=TOP + (255,), width=round(10 * u))


def main():
    size = 256 * SCALE
    image = tile(size)
    bucket(ImageDraw.Draw(image), size)
    image = image.resize((256, 256), Image.LANCZOS)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    image.save(OUTPUT, format='ICO', sizes=SIZES)
    print(OUTPUT.relative_to(ROOT))


if __name__ == '__main__':
    main()
