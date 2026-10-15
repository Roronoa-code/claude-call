# PREVIEW (branch preview/call-screen): the record backdrop drawn for each call screen direction at its caller's size
# (caller.rs LOOK_K), so it is never scaled: phone/ui/stage/record-<look>.png. Usage: python call_records.py
import os
import stages as S
from PIL import Image

LOOKS = {'stage': 1.0, 'close': 1.5, 'comic': 0.85, 'lyrics': 0.6}
base = dict(S.STAGES['record'])
for name, k in LOOKS.items():
    S.STAGES['record'] = dict(base, k=k)
    im = Image.fromarray(S.to_srgb8(S.backdrop('record', S.ASSET_SCALE)))
    path = os.path.join(S.ASSETS, f'record-{name}.png')
    im.save(path, optimize=True)
    print(path, os.path.getsize(path) // 1024, 'KB')
