# PREVIEW (branch preview/home-controls-3): captures each round-three home-controls direction from the real app on a
# headless test emulator only (never a phone), on the Spotlight stage, with debug.callme.fake set so mode taps and
# Call stay on the emulator (nothing stored, published or asked of the PC). The emulator's build has a dummy ntfy
# topic. Per direction: home.png, each mode, Null picked, a call asked for, setup unfinished, a chat unread, the
# hidden controls open, the Widget sheet and Chats open, and flow.mp4 (reveal, change mode, open Chats, back) plus a
# 540 px copy and a strip of its frames for checking mid-animation.
# Usage: python controls_capture.py [direction ...]   (1-4)
import os, sys, time, subprocess, io
import numpy as np
from PIL import Image

ADB = ['adb', '-s', os.environ.get('EMU', 'emulator-5582')]
OUT = r'D:\Outputs\CallMe\design\step3-controls-3'
NAMES = {1: '1-unfold', 2: '2-drawer', 3: '3-steps', 4: '4-pages'}
D = 2.625  # px per dp on this emulator (1080 x 2400)
W, H, SAT, SAB = 1080 / D, 2400 / D, 49.8, 24.5  # dp
STATUS = (540, 1761)  # px: the status line's middle
PILL = (540, 2187)  # px: Call
CORNER = (996, 199)  # px: 1's Chats glyph, 3's "•••"
BUBBLE = (648, 870)  # px: 3's speech bubble
NULL = (930, 1330)  # px: Null
CLAWD = (420, 1330)
OUTSIDE = (150, 900)  # px: empty stage left of the pair (clear of every card and sheet)
ORDER = ['call', 'text', 'both', 'none', 'wake', 'sleep']


def px(x, y):
    return round(x * D), round(y * D)


def adb(*a, out=False):
    r = subprocess.run(ADB + list(a), capture_output=True)
    return r.stdout if out else None


def shot():
    return Image.open(io.BytesIO(adb('exec-out', 'screencap', '-p', out=True))).convert('RGB')


def tap(p, wait=0.0):
    adb('shell', 'input', 'tap', str(p[0]), str(p[1]))
    time.sleep(wait)


def swipe(a, b, ms=320, wait=0.0):
    adb('shell', 'input', 'swipe', str(a[0]), str(a[1]), str(b[0]), str(b[1]), str(ms))
    time.sleep(wait)


def back(wait=0.0):
    adb('shell', 'input', 'keyevent', '4')
    time.sleep(wait)


def start(v, fake):
    adb('shell', 'setprop', 'debug.callme.stage', '1')
    adb('shell', 'setprop', 'debug.callme.controls', str(v))
    adb('shell', 'setprop', 'debug.callme.fake', str(fake))
    adb('shell', 'am', 'force-stop', 'fyi.mani.callme')
    adb('shell', 'am', 'start', '-n', 'fyi.mani.callme/.MainActivity')
    time.sleep(11)


# ---- where things are in each direction (dp, from the layouts in controls*.slint) ----

def unfold_row(i, at, ready=True):
    """1: the middle of mode i's row in the list unfolded from mode `at`."""
    ch = 8 + (0 if ready else 84) + 6 * 60 + 17 + 8 + 56
    ry = lambda j: 8 + (0 if ready else 84) + 60 * j + (17 if j >= 4 else 0)
    sy = STATUS[1] / D
    top = max(SAT + 60, min(H - SAB - 16 - ch, sy - ry(at) - 30))
    return px(W / 2, top + ry(i) + 30), top, ch


def drawer_top(ready=True, rows=0):
    y_chats = 34 + (0 if ready else 96) + 152 + 12 + 34 + 48 + 20
    return H - (y_chats + 46 + max(1, min(2, rows)) * 64 + SAB + 10), 34 + (0 if ready else 96)


def drawer_tile(i, ready=True, rows=0):
    top, yt = drawer_top(ready, rows)
    tw = (W - 32 - 16) / 3
    return px(16 + (tw + 8) * (i % 3) + tw / 2, top + yt + 80 * (i // 3) + 36)


class Mode:
    """Picks modes the way each direction does; the app opens on Both (this emulator's stored mode)."""

    def __init__(self, v):
        self.v, self.at = v, 2

    def go(self, m, wait=1.5):
        v = self.v
        if v == 1:
            tap(STATUS, 1.0)
            tap(unfold_row(m, self.at)[0])
        elif v == 2:
            tap((540, 2038), 1.0)
            tap(drawer_tile(m), 0.9)
            tap(OUTSIDE)
        elif v == 3:
            while self.at != m:
                tap((980, STATUS[1]) if m > self.at else (100, STATUS[1]), 0.7)
                self.at += 1 if m > self.at else -1
        else:
            tap(px(55.7, PILL[1] / D), 1.0)
            tap(px(200, SAT + 64 + 60 * m + (40 if m >= 4 else 0) + 30))
        self.at = m
        time.sleep(wait)


def reveal(v, ready=True, rows=0):
    """Opens the direction's hidden controls."""
    if v == 1:
        tap(STATUS)
    elif v == 2:
        tap((540, 2038))
    elif v == 3:
        tap(CORNER)
    else:
        tap(px(55.7, PILL[1] / D))


def widget(v, at=2, ready=True):
    """Opens the Widget sheet from wherever this direction keeps it (its hidden layer is open)."""
    if v == 1:
        _, top, ch = unfold_row(0, at, ready)
        tap(px(W / 2 + 86, top + ch - 28))
    elif v == 2:
        top, yt = drawer_top(ready)
        tap(px(302.6, top + yt + 152 + 12 + 34 + 24))
    elif v == 3:
        tap(px(277, SAT + 54 + 8 + (0 if ready else 112) + 52 + 26))
    else:
        tap(px(200, SAT + 64 + (0 if ready else 104) + 360 + 60 + 90))


def chats(v, ready=True, rows=0):
    if v == 1:
        tap(CORNER)
    elif v == 2:
        tap((540, 2038), 1.0)
        top, yt = drawer_top(ready, rows)
        tap(px(330, top + yt + 152 + 12 + 34 + 48 + 20 + 23))
    elif v == 3:
        tap(BUBBLE)
    else:
        swipe((900, 1500), (150, 1500), 260)


def video(v, d):
    """flow.mp4: reveal, change mode, open Chats, back (with an unread chat, so the chats show)."""
    start(v, 3)
    rec = subprocess.Popen(ADB + ['shell', 'screenrecord', '--time-limit', '60', '--bit-rate', '12000000', '/sdcard/flow.mp4'])
    time.sleep(2.0)
    if v == 1:
        tap(STATUS, 1.6)  # unfolds from Both
        tap(unfold_row(1, 2)[0], 2.2)  # Messages only: folds back into it
        tap(STATUS, 1.6)
        tap(unfold_row(4, 1)[0], 2.2)  # Asleep, wake me
        tap(CORNER, 2.2)  # Chats grows out of its glyph
        back(1.8)
    elif v == 2:
        swipe((540, 2150), (540, 1150), 420, 1.8)  # drag the drawer up
        tap(drawer_tile(1, rows=2), 1.4)  # Texts
        tap(drawer_tile(4, rows=2), 1.6)  # Wake me
        top, yt = drawer_top(rows=2)
        tap(px(200, top + yt + 152 + 12 + 34 + 48 + 20 + 46 + 32), 2.2)  # the latest chat
        back(1.8)  # Chats goes back into the row
        swipe((540, round((top + 14) * D)), (540, 2300), 360, 1.8)  # drag the drawer back down
    elif v == 3:
        swipe((760, STATUS[1]), (300, STATUS[1]), 380, 1.5)  # Quiet
        swipe((300, STATUS[1]), (760, STATUS[1]), 380, 1.5)  # back to Both
        swipe((300, STATUS[1]), (760, STATUS[1]), 380, 1.5)  # Messages only (the pair hop)
        tap((980, STATUS[1]), 1.2)
        tap(BUBBLE, 2.2)  # Chats grows out of the bubble
        back(1.8)
        tap(CORNER, 1.6)  # the "•••" card
        tap(OUTSIDE, 1.4)
    else:
        swipe((150, 1500), (900, 1500), 420, 1.8)  # the modes page, following the finger
        tap(px(200, SAT + 64 + 60 * 1 + 30), 2.2)  # Messages only: back home on its own
        swipe((900, 1500), (150, 1500), 420, 2.0)  # the chats page
        swipe((150, 1500), (900, 1500), 420, 1.6)  # and home
        tap(px(355.7, PILL[1] / D), 1.8)  # Chats from its glyph
        back(1.6)
    time.sleep(0.6)
    adb('shell', 'pkill', '-INT', 'screenrecord')
    rec.wait()
    time.sleep(1.5)
    vid = os.path.join(d, 'flow.mp4')
    subprocess.run(ADB + ['pull', '/sdcard/flow.mp4', vid], capture_output=True)
    adb('shell', 'rm', '/sdcard/flow.mp4')
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-i', vid, '-vf', 'scale=540:-2', '-c:v', 'libx264', '-crf', '26',
                    '-preset', 'slow', '-pix_fmt', 'yuv420p', '-an', vid.replace('.mp4', '-small.mp4')])


def capture(v):
    d = os.path.join(OUT, NAMES[v])
    os.makedirs(d, exist_ok=True)
    save = lambda name: shot().save(os.path.join(d, name + '.png'))
    video(v, d)
    # each mode (the app starts on Both)
    start(v, 1)
    save('mode-both')
    m = Mode(v)
    for i in [0, 3, 4, 5, 1]:
        m.go(i)
        save('home' if i == 1 else f'mode-{ORDER[i]}')
    # the hidden controls open (from Messages only)
    reveal(v)
    time.sleep(1.4)
    save('open')
    widget(v, at=1)
    time.sleep(1.6)
    save('state-widget')
    tap(OUTSIDE, 1.0)
    back(1.0)
    # Null picked, then a call asked for
    tap(NULL, 1.4)
    save('state-codex')
    tap(PILL, 0.8)
    save('state-calling')
    time.sleep(6)
    tap(CLAWD, 0.8)
    # setup unfinished (and a chat unread), opened
    start(v, 2)
    save('state-setup')
    reveal(v, ready=False, rows=2)
    time.sleep(1.4)
    save('state-setup-open')
    back(1.0)
    # only a chat unread, then Chats open
    start(v, 3)
    save('state-unread')
    if v == 2:
        reveal(v, rows=2)
        time.sleep(1.4)
        save('state-unread-open')
        back(1.0)
    chats(v, rows=2)
    time.sleep(2.0)
    save('state-chats')
    back(1.0)
    start(v, 1)
    print(NAMES[v], 'done')


if __name__ == '__main__':
    for v in [int(a) for a in sys.argv[1:]] or [1, 2, 3, 4]:
        capture(v)
