# PREVIEW (branch preview/home-stage): captures each home stage from the real app on the headless test emulator only
# (never a phone): a full screenshot, a crop of the pair, and a short video of them idling, Clawd hopping on a tap,
# then Null picked (and hopping) and Clawd picked back. Never touches the contact-mode pills.
# Usage: python stage_capture.py [stage ...]   (stages 1-4; 0 is the shipped stage)
import os, sys, time, subprocess, io
import numpy as np
from PIL import Image

ADB = ['adb', '-s', 'emulator-5580']
OUT = r'D:\Outputs\CallMe\design\step2-stage'
NAMES = {0: '0-shipped', 1: '1-spotlight', 2: '2-desk', 3: '3-moonroom', 4: '4-record'}
K = {0: 1.0, 1: 1.08, 2: 1.0, 3: 0.9, 4: 0.84}  # stage.rs k
SF = 2.625  # the emulator's px per logical px


def adb(*a, out=False):
    r = subprocess.run(ADB + list(a), capture_output=True)
    return r.stdout if out else None


def shot():
    return Image.open(io.BytesIO(adb('exec-out', 'screencap', '-p', out=True))).convert('RGB')


def clawd_centre(im):
    a = np.asarray(im).astype(int)
    orange = (a[..., 0] > 170) & (a[..., 1] > 50) & (a[..., 1] < 140) & (a[..., 2] < 60)
    orange[:300] = False  # not the status bar
    ys, xs = np.nonzero(orange)
    return int(np.median(xs)), int(np.median(ys))


def tap(x, y):
    adb('shell', 'input', 'tap', str(x), str(y))


def capture(v):
    d = os.path.join(OUT, NAMES[v])
    os.makedirs(d, exist_ok=True)
    adb('shell', 'setprop', 'debug.callme.stage', str(v))
    adb('shell', 'am', 'force-stop', 'fyi.mani.callme')
    adb('shell', 'am', 'start', '-n', 'fyi.mani.callme/.MainActivity')
    time.sleep(9)
    im = shot()
    im.save(os.path.join(d, 'home.png'))
    cx, cy = clawd_centre(im)
    k = K[v]
    nx = cx + round(167 * k * SF)  # toys.slint null-dx
    ny = cy - round(10 * k * SF)
    # the pair: from Clawd's left arm to Null's right, head room for the antenna, the floor under them
    x0, x1 = cx - round(170 * k * SF), nx + round(80 * k * SF)
    y0, y1 = cy - round(200 * k * SF), cy + round(190 * k * SF)
    im.crop((max(x0, 0), max(y0, 0), min(x1, im.width), min(y1, im.height))).save(os.path.join(d, 'pair-crop.png'))
    # video: idle, Clawd hops, Null picked (hops, Clawd dims in his light), Clawd picked back
    rec = subprocess.Popen(ADB + ['shell', 'screenrecord', '--time-limit', '11', '--bit-rate', '12000000', '/sdcard/stage.mp4'])
    time.sleep(3.0)
    tap(cx, cy)
    time.sleep(3.0)
    tap(nx, ny)
    time.sleep(3.0)
    tap(cx, cy)
    rec.wait()
    time.sleep(1.0)
    subprocess.run(ADB + ['pull', '/sdcard/stage.mp4', os.path.join(d, 'idle-hop-pick.mp4')], capture_output=True)
    adb('shell', 'rm', '/sdcard/stage.mp4')
    print(NAMES[v], 'clawd at', cx, cy, 'null at', nx, ny)


if __name__ == '__main__':
    for v in [int(a) for a in sys.argv[1:]] or [1, 2, 3, 4]:
        capture(v)
