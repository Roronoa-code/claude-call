# PREVIEW (branch preview/home-controls-2): captures each home-controls direction from the real app on a headless test
# emulator only (never a phone), on the Spotlight stage, with debug.callme.fake set so mode taps and Call stay on the
# emulator (nothing stored, published or asked of the PC). Per direction: home.png, controls-crop.png, stills of the
# other states (each mode, Sleep, Null picked, a call asked for, Widget sheet, setup card with an unread chat) and
# modes-and-buttons.mp4 (Both, Quiet, Wake me, Sleep, back to Texts, Widget opened and closed, Call pressed) plus a
# 540 px copy.  Usage: python controls_capture.py [direction ...]   (0-4; 0 is the controls as shipped)
import os, sys, time, subprocess, io
import numpy as np
from PIL import Image

ADB = ['adb', '-s', os.environ.get('EMU', 'emulator-5582')]
OUT = r'D:\Outputs\CallMe\design\step3-controls-2'
NAMES = {0: '0-current', 1: '1-hairline', 2: '2-panel', 3: '3-sheet', 4: '4-strip'}
NULL_DX = 486  # screen px from Clawd's middle to Null's
# Screen px of each mode (Calls, Texts, Both, Quiet, Wake me, Sleep) and action (Test ring, Widget, Chats, Call), read
# off each direction's home screenshot. `controls`: the crop's top and bottom. Sheet's modes are in its list (opened
# by tapping its status row, `open`); Strip's are steps along its line (`next`, `prev`, swipes).
TAPS = {
    0: dict(modes=[(180, 1547), (420, 1547), (660, 1547), (900, 1547), (330, 1710), (780, 1710)],
            acts=[(170, 2126), (404, 2126), (636, 2126), (890, 2126)], controls=(1280, 2330)),
    1: dict(modes=[(180, 1732), (420, 1732), (660, 1732), (900, 1732), (422, 1874), (658, 1874)],
            acts=[(174, 2220), (418, 2220), (661, 2220), (905, 2220)], controls=(1380, 2330)),
    2: dict(modes=[(160, 2016), (304, 2016), (444, 2016), (586, 2016), (772, 2016), (912, 2016)],
            acts=[(136, 2200), (262, 2200), (388, 2200), (740, 2200)], controls=(1560, 2330)),
    3: dict(open=(400, 1920), modes=[(540, 1140), (540, 1287), (540, 1434), (540, 1581), (540, 1833), (540, 1980)],
            acts=[(156, 2200), (412, 2200), (666, 2200), (922, 2200)], controls=(1640, 2400)),
    4: dict(next=(755, 1738), prev=(325, 1738), y=1738,
            acts=[(276, 2068), (540, 2068), (800, 2068), (540, 2220)], controls=(1380, 2330)),
}
ORDER = ['call', 'text', 'both', 'none', 'wake', 'sleep']


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
    orange[1350:] = False  # not the controls' orange
    ys, xs = np.nonzero(orange)
    return int(np.median(xs)), int(np.median(ys))


class Pick:
    """Chooses modes the way each direction does, tracking where Strip's line is."""

    def __init__(self, v):
        self.v, self.t, self.at = v, TAPS[v], 2  # the app opens on Both (this emulator's stored mode)

    def go(self, m, wait=1.6):
        t = self.t
        if self.v == 3:
            tap(t['open'])
            time.sleep(0.9)
            tap(t['modes'][m])
        elif self.v == 4:
            while self.at != m:
                step = 1 if m > self.at else -1
                if abs(m - self.at) > 1:  # far: a swipe (it steps one name)
                    x0, x1 = (800, 300) if step > 0 else (300, 800)
                    adb('shell', 'input', 'swipe', str(x0), str(t['y']), str(x1), str(t['y']), '120')
                else:
                    gap = (self.at == 3 and step > 0) or (self.at == 4 and step < 0)
                    dx = round((82 + (22 if gap else 0)) * 2.625)
                    tap((540 + step * dx, t['y']))
                self.at += step
                time.sleep(0.75)
        else:
            tap(t['modes'][m])
        self.at = m
        time.sleep(wait)


def start(v, fake):
    adb('shell', 'setprop', 'debug.callme.stage', '1')
    adb('shell', 'setprop', 'debug.callme.controls', str(v))
    adb('shell', 'setprop', 'debug.callme.fake', str(fake))
    adb('shell', 'am', 'force-stop', 'fyi.mani.callme')
    adb('shell', 'am', 'start', '-n', 'fyi.mani.callme/.MainActivity')
    time.sleep(10)


def capture(v):
    d = os.path.join(OUT, NAMES[v])
    os.makedirs(d, exist_ok=True)
    t = TAPS[v]
    y0, y1 = t['controls']
    crop = lambda im: im.crop((0, y0, im.width, y1))
    start(v, 1)
    p = Pick(v)
    p.go(1)  # Texts, the mode the stills start from
    im = shot()
    im.save(os.path.join(d, 'home.png'))
    crop(im).save(os.path.join(d, 'controls-crop.png'))
    rec = subprocess.Popen(ADB + ['shell', 'screenrecord', '--time-limit', '20', '--bit-rate', '12000000', '/sdcard/ctl.mp4'])
    time.sleep(1.5)
    for m in [2, 3, 4, 5, 1]:
        p.go(m)
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
    time.sleep(5)  # the fake call request ends
    # each mode
    for m in [0, 2, 3, 4, 5]:
        p.go(m, 1.4)
        crop(shot()).save(os.path.join(d, f'mode-{ORDER[m]}.png'))
    if v == 3:  # the list open
        tap(t['open'])
        time.sleep(1.2)
        shot().save(os.path.join(d, 'state-list-open.png'))
        tap((540, 400))
        time.sleep(0.8)
    p.go(1, 1.2)
    cx, cy = clawd_centre(shot())
    tap((cx + NULL_DX, cy - 10))  # Null: the Call button asks for Codex
    time.sleep(1.5)
    shot().save(os.path.join(d, 'state-codex.png'))
    tap(t['acts'][3])
    time.sleep(0.9)
    im = shot()
    im.save(os.path.join(d, 'state-calling.png'))
    time.sleep(6)
    tap((cx, cy))  # Clawd back
    time.sleep(0.8)
    tap(t['acts'][1])
    time.sleep(1.5)
    shot().save(os.path.join(d, 'state-widget.png'))
    tap((540, 420))
    time.sleep(1.0)
    start(v, 2)  # setup not finished, and a chat unread
    shot().save(os.path.join(d, 'state-setup-unread.png'))
    start(v, 1)
    print(NAMES[v], 'done; clawd at', cx, cy)


if __name__ == '__main__':
    for v in [int(a) for a in sys.argv[1:]] or [0, 1, 2, 3, 4]:
        capture(v)
