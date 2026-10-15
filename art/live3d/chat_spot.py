# PREVIEW (branch preview/chats): the home screen's spotlight (phone/ui/stage/spot.png, stages.py 'spot') as light
# with a clear background (phone/ui/chats/spot.png), so the chat screens can stand an agent in it anywhere on their
# black: on black, light drawn with alpha = its brightness looks exactly like the backdrop. Cropped to where there is
# light; prints where the feet anchor is (logical px at the backdrop's k = 1.08) for chat_kit.slint's Spot.
import os
import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
src = os.path.join(HERE, '..', '..', 'phone', 'ui', 'stage', 'spot.png')
im = np.asarray(Image.open(src).convert('RGB')).astype(np.float64) / 255.0
scale = im.shape[1] / 500.0  # px per logical px (stages.py BW = 500)
a = im.max(axis=2)
rgb = im / np.maximum(a, 1e-6)[..., None]
a = np.where(a < 3 / 255, 0, a)  # the void's dither
out = np.dstack([np.clip(rgb, 0, 1) * 255, a * 255]).round().astype(np.uint8)
ys, xs = np.nonzero(out[..., 3])
y0, y1, x0, x1 = ys.min(), ys.max() + 1, xs.min(), xs.max() + 1
dst = os.path.join(HERE, '..', '..', 'phone', 'ui', 'chats', 'spot.png')
os.makedirs(os.path.dirname(dst), exist_ok=True)
pic = Image.fromarray(out[y0:y1, x0:x1], 'RGBA')
pic = pic.resize((round(pic.width * 0.6), round(pic.height * 0.6)), Image.LANCZOS)  # smooth light: fewer px is plenty
pic.save(dst, optimize=True)
print(dst, os.path.getsize(dst) // 1024, 'KB')
print(f'size {(x1 - x0) / scale:.1f} x {(y1 - y0) / scale:.1f}; anchor at {250 - x0 / scale:.1f}, {640 - y0 / scale:.1f}')
