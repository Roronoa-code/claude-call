# Usage: python call_spots.py [name ...]
import os, sys
import numpy as np
import stages as S
from PIL import Image

LOOKS = {
    'camera': (0.62, 2.4, 440, 640, 480),
}


def backdrop(k, scale, bw, above, below, ssaa=2):
    w, h = round(bw * scale), round((above + below) * scale)
    rng = np.random.default_rng(5)
    acc = np.zeros((h, w, 3))
    for sy in range(ssaa):
        for sx in range(ssaa):
            Y, X = np.mgrid[0:h, 0:w].astype(np.float64)
            X = (X + (sx + 0.5) / ssaa) / scale - bw / 2
            Y = (Y + (sy + 0.5) / ssaa) / scale - above
            acc += S.spot(X, Y, k, rng)
    return acc / (ssaa * ssaa)


if __name__ == '__main__':
    names = sys.argv[1:] or list(LOOKS)
    for n in names:
        k, scale, bw, above, below = LOOKS[n]
        im = Image.fromarray(S.to_srgb8(backdrop(k, scale, bw, above, below)))
        path = os.path.join(S.ASSETS, f'spot-{n}.png')
        im.save(path, optimize=True)
        print(path, im.size, os.path.getsize(path) // 1024, 'KB', flush=True)
