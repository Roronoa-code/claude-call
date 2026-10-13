"""Builds null.png and null-small.png (Null, Codex's pet) from the Codex app's sprite sheet.

The sheet's art is noisy at the edges (lossy compression of a resampled render), so every frame is redrawn with
clean, smooth edges: its outline is traced at 4x (scaled up, softened, re-thresholded) and scaled down to the exact
pixel size the phone draws it at, so nothing is resampled again on the phone.
  python null_atlas.py <null-signal-spritesheet.webp>
"""
import sys

import numpy as np
from PIL import Image, ImageFilter

# (row in the Codex sheet, frames, px it is drawn too low): idle, waving, jumping, waiting ("?"), running (laptop), review
ROWS = [(0, 6, 0), (3, 4, 0), (4, 5, 0), (6, 6, 0), (7, 6, 0), (8, 6, 5)]
CELL_W, CELL_H, X0 = 144, 208, 24  # the part of each 192 x 208 cell Null uses
SIZES = {"null.png": 1.75, "null-small.png": 1.0}  # physical px per sheet px (the call screen draws them 1:1)
UP = 4


def clean(cell: Image.Image, scale: float) -> Image.Image:
    w, h = cell.size
    big = (w * UP, h * UP)
    colour = cell.convert("RGBa").resize(big, Image.BICUBIC)  # premultiplied: no light fringe
    alpha = cell.getchannel("A").resize(big, Image.BICUBIC).filter(ImageFilter.GaussianBlur(UP * 0.7))
    a = np.asarray(alpha, dtype=np.float32) / 255.0
    a = np.clip((a - 0.5) * 6 + 0.5, 0, 1)  # a crisp outline again, now smooth
    rgb = np.asarray(colour.convert("RGBA"), dtype=np.float32)
    rgb[..., 3] = a * 255
    out = Image.fromarray(rgb.astype(np.uint8), "RGBA")
    size = (round(w * scale), round(h * scale))
    return out.convert("RGBa").resize(size, Image.LANCZOS).convert("RGBA")


def main(src: str):
    sheet = Image.open(src).convert("RGBA")
    for name, scale in SIZES.items():
        cw, ch = round(CELL_W * scale), round(CELL_H * scale)
        atlas = Image.new("RGBA", (cw * 6, ch * len(ROWS)), (0, 0, 0, 0))
        for i, (row, frames, dy) in enumerate(ROWS):
            for c in range(frames):
                box = (c * 192 + X0, row * 208 + dy, c * 192 + X0 + CELL_W, row * 208 + dy + CELL_H)
                atlas.paste(clean(sheet.crop(box), scale), (c * cw, i * ch))
        atlas.save(name, optimize=True)
        print(name, atlas.size, "cell", (cw, ch))


if __name__ == "__main__":
    main(sys.argv[1])
