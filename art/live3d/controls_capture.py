# PREVIEW (branch preview/home-controls): captures each home-controls direction from the real app on the headless test
# emulator only (never a phone), on the Record stage, with debug.callme.fake set so mode taps and Call stay on the
# emulator (nothing stored, published or asked of the PC). Per direction: home.png, crops of the controls, stills of
# the other states (Sleep picked, Null picked, a call asked for, Widget sheet, setup not finished) and modes-and-
# buttons.mp4 (Both, Quiet, Wake me, Sleep, back to Texts, Widget opened and closed, Call pressed) plus a small copy.
# Usage: python controls_capture.py [direction ...]   (0-4; 0 is the controls as shipped)
import os, sys, time, subprocess, io
import numpy as np
from PIL import Image

ADB = ['adb', '-s', 'emulator-5580']
OUT = r'D:\Outputs\CallMe\design\step3-controls'
NAMES = {0: '0-current', 1: '1-tuner', 2: '2-amp', 3: '3-type', 4: '4-pucks'}
SF, K = 2.625, 0.84  # the emulator's px per logical px; the Record stage's scale
# Screen px of each mode (Calls, Texts, Both, Quiet, Wake me, Sleep) and action (Test ring, Widget, Chats, Call),
# read off each direction's home screenshot. `controls` is the crop round the block and the actions.
TAPS = {
    0: dict(modes=[(180, 1455), (420, 1455), (660, 1455), (900, 1455), (325, 1617), (780, 1617)],
            acts=[(170, 2126), (404, 2126), (636, 2126), (890, 2126)], controls=(1180, 2300)),
    1: dict(modes=[(151, 1705), (297, 1705), (443, 1705), (589, 1705), (782, 1705), (927, 1705)],
            acts=[(166, 2183), (415, 2183), (663, 2183), (910, 2183)], controls=(1300, 2300)),
    2: dict(modes=[(275, 1649), (346, 1527), (469, 1457), (610, 1457), (731, 1527), (801, 1649)],
            acts=[(165, 2133), (414, 2133), (662, 2133), (910, 2133)], controls=(1300, 2300)),
    3: dict(modes=[(126, 1686), (313, 1686), (500, 1686), (685, 1686), (181, 1804), (426, 1804)],
            acts=[(140, 2210), (346, 2210), (512, 2210), (950, 2210)], controls=(1300, 2300)),
    4: dict(modes=[(119, 1654), (281, 1670), (440, 1705), (600, 1705), (797, 1687), (956, 1654)],
            acts=[(148, 2208), (311, 2208), (473, 2208), (802, 2208)], controls=(1300, 2300)),
}


def adb(*a, out=False):
    r = subprocess.run(ADB + list(a), capture_output=True)
    return r.stdout if out else None


def shot():
    return Image.open(io.BytesIO(adb('exec-out', 'screencap', '-p', out=True))).convert('RGB')


def tap(p):
    adb('shell', 'input', 'tap', str(p[0]), str(p[1]))


def clawd_centre(im):
    a = np.asarray(im).astype(int)
    orange = (a[..., 0] > 170) & (a[..., 1] > 50) & (a[..., 1] < 140) & (a[..., 2] < 60)
    orange[:300] = False
    orange[1250:] = False  # not the controls' orange
    ys, xs = np.nonzero(orange)
    return int(np.median(xs)), int(np.median(ys))


def start(v, fake):
    adb('shell', 'setprop', 'debug.callme.stage', '4')
    adb('shell', 'setprop', 'debug.callme.controls', str(v))
    adb('shell', 'setprop', 'debug.callme.fake', str(fake))
    adb('shell', 'am', 'force-stop', 'fyi.mani.callme')
    adb('shell', 'am', 'start', '-n', 'fyi.mani.callme/.MainActivity')
    time.sleep(9)


def capture(v):
    d = os.path.join(OUT, NAMES[v])
    os.makedirs(d, exist_ok=True)
    t = TAPS[v]
    start(v, 1)
    tap(t['modes'][1])  # Texts, the mode the stills start from
    time.sleep(1.5)
    im = shot()
    im.save(os.path.join(d, 'home.png'))
    y0, y1 = t['controls']
    im.crop((0, y0, im.width, y1)).save(os.path.join(d, 'controls-crop.png'))
    rec = subprocess.Popen(ADB + ['shell', 'screenrecord', '--time-limit', '17', '--bit-rate', '12000000', '/sdcard/ctl.mp4'])
    time.sleep(1.5)
    for m in [2, 3, 4, 5, 1]:
        tap(t['modes'][m])
        time.sleep(1.7)
    tap(t['acts'][1])  # Widget: the sheet grows out of it
    time.sleep(2.0)
    tap((540, 420))  # a tap outside closes it
    time.sleep(1.6)
    tap(t['acts'][3])  # Call (fake: only the "Calling you…" state)
    rec.wait()
    time.sleep(1.0)
    vid = os.path.join(d, 'modes-and-buttons.mp4')
    subprocess.run(ADB + ['pull', '/sdcard/ctl.mp4', vid], capture_output=True)
    adb('shell', 'rm', '/sdcard/ctl.mp4')
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-i', vid, '-vf', 'scale=540:-2', '-c:v', 'libx264', '-crf', '26',
                    '-preset', 'slow', '-pix_fmt', 'yuv420p', '-an', vid.replace('.mp4', '-small.mp4')])
    # the other states
    time.sleep(6)  # the fake call request ends
    tap(t['modes'][5])
    time.sleep(1.5)
    shot().crop((0, y0, 1080, y1)).save(os.path.join(d, 'state-sleep.png'))
    tap(t['modes'][1])
    time.sleep(1.2)
    cx, cy = clawd_centre(shot())
    tap((cx + round(167 * K * SF), cy - round(10 * K * SF)))  # Null: the Call button asks for Codex
    time.sleep(1.5)
    shot().save(os.path.join(d, 'state-codex.png'))
    tap(t['acts'][3])
    time.sleep(0.8)
    shot().crop((0, y0, 1080, y1)).save(os.path.join(d, 'state-calling.png'))
    time.sleep(6)
    tap((cx, cy))  # Clawd back
    time.sleep(0.8)
    tap(t['acts'][1])
    time.sleep(1.5)
    shot().save(os.path.join(d, 'state-widget.png'))
    tap((540, 420))
    time.sleep(1.0)
    start(v, 2)  # setup not finished
    shot().save(os.path.join(d, 'state-setup.png'))
    print(NAMES[v], 'done; clawd at', cx, cy)


if __name__ == '__main__':
    for v in [int(a) for a in sys.argv[1:]] or [0, 1, 2, 3, 4]:
        capture(v)
