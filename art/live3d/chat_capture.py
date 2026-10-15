# PREVIEW (branch preview/chats): captures each chat direction from the real app on the headless test emulator only
# (never a phone). The emulator's chats are replaced with local fixtures (chat_fixture/: a screenshot, a photo of
# mine, a screen recording, one that couldn't load, one not sent, an unread one, a long technical answer), and
# debug.callme.fake plays the PC on the emulator (phone/src/fake_chat.rs): nothing is sent to the PC or ntfy.
# Usage: python chat_capture.py fixture | shots <v> | video <v>      (v: 0-4; 0 is the chats as shipped)
import os, sys, time, json, subprocess, io
from PIL import Image

os.environ['MSYS_NO_PATHCONV'] = '1'
ADB = ['adb', '-s', 'emulator-5580']
PKG = 'fyi.mani.callme'
FILES = f'/data/data/{PKG}/files'
HERE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(HERE, 'chat_fixture')
OUT = r'D:\Outputs\CallMe\design\step5-chats'
NAMES = {0: '0-current', 1: '1-stage', 2: '2-beside', 3: '3-script', 4: '4-light'}
SF = 2.625


def adb(*a, out=False, check=False):
    r = subprocess.run(ADB + list(a), capture_output=True)
    if check and r.returncode:
        raise SystemExit(r.stderr.decode())
    return r.stdout if out else None


def sh(cmd):
    return adb('shell', cmd, out=True).decode(errors='replace').strip()


def shot():
    return Image.open(io.BytesIO(adb('exec-out', 'screencap', '-p', out=True))).convert('RGB')


def tap(x, y):
    """x, y in logical px (the app's), as the .slint files place things."""
    adb('shell', 'input', 'tap', str(round(x * SF)), str(round(y * SF)))


def swipe(x, y0, y1, ms=350):
    adb('shell', 'input', 'swipe', *[str(round(v * SF)) for v in (x, y0, x, y1)], str(ms))


def text(s):
    """Types word by word (a long burst loses letters; adb's capitals don't reach the field, so the lines are lower case)."""
    for w in s.split(' '):
        adb('shell', 'input', 'text', w)
        adb('shell', 'input', 'keyevent', '62')  # space
        time.sleep(0.35)


def fixture():
    """Fresh chats on the emulator: written to its store with its own clock, files in filesDir/media."""
    adb('root')
    time.sleep(1.5)
    adb('shell', 'am', 'force-stop', PKG)
    uid = sh(f'stat -c %u {FILES}')
    now = int(sh('date +%s')) * 1000
    media = f'{FILES}/media'
    sh(f'mkdir -p {media}')
    for f in ['home.jpg', 'spotlight.jpg', 'picker.mp4']:
        adb('push', os.path.join(FIX, f), f'{media}/{f}')
    adb('push', os.path.join(FIX, 'pick.jpg'), f'{FILES}/pick.jpg')
    sh(f'rm -f {media}/*.part')
    m = lambda name, mime, state='ready': dict(part=1, url='https://ntfy.sh/file/x' + name.split('.')[0], name=name, mime=mime,
                                                size=50000, sha256='0' * 64, path=f'{media}/{name}' if state == 'ready' else '', state=state)
    L = lambda id, me, text, ago, state, media=(): dict(id=id, me=me, text=text, at=now - ago, since=now - ago, state=state, parts=[], media=list(media))
    MIN, H = 60_000, 3_600_000
    chats = [
        dict(id='ca', agent='claude', model='sonnet-5.5', title='Can you send a screenshot of the new home screen?', updated=now - 2 * MIN, unread=0, lines=[
            L('a1', True, 'Can you send a screenshot of the new home screen?', 9 * MIN, 'answered'),
            L('a1', False, 'Here it is on the record stage. The tokens sit in a gentle curve under it.', 8 * MIN, 'done', [m('home.jpg', 'image/jpeg')]),
            L('a2', True, "This one's from the old spotlight stage, for comparison.", 4 * MIN, 'answered', [m('spotlight.jpg', 'image/jpeg')]),
            L('a2', False, 'The record reads cleaner: one light and no haze behind Clawd, so his edges stay crisp at small sizes.', 2 * MIN, 'done'),
        ]),
        dict(id='cb', agent='claude', model='haiku-5.5', title='Record the new picker opening', updated=now - 6 * MIN, unread=1, lines=[
            L('b1', True, 'Record the new picker opening', 8 * MIN, 'answered'),
            L('b1', False, "Here's the recording, five seconds from the home screen.", 6 * MIN, 'done', [m('picker.mp4', 'video/mp4')]),
        ]),
        dict(id='cc', agent='claude', model='opus-5.5', title='Why does the widget stop updating after a reboot?', updated=now - 63 * MIN, unread=0, lines=[
            L('c1', True, 'Why does the widget stop updating after a reboot?', 70 * MIN, 'answered'),
            L('c1', False, "Two things stop it after a reboot:\n\n1. The update alarm is set in onEnabled() only, so a reboot clears it and nothing sets it again.\n"
              "2. WidgetProvider doesn't listen for BOOT_COMPLETED, so it never gets the chance.\n\nFix: register for android.intent.action.BOOT_COMPLETED "
              "in AndroidManifest.xml and call schedule_next() from onReceive(). I've drafted it in widget.rs (about 20 lines). Want me to push it?", 63 * MIN, 'done'),
        ]),
        dict(id='cd', agent='codex', model=None, title='Tidy up the old branches.', updated=now - 2 * H, unread=0, lines=[
            L('d1', True, 'Tidy up the old branches.', 2 * H + 9 * MIN, 'answered'),
            L('d1', False, 'Deleted 14 merged branches; kept the two with open work. The graph is attached.', 2 * H + 5 * MIN, 'done', [m('graph.png', 'image/png', 'failed')]),
            L('d2', True, 'And the stale tags?', 2 * H, 'not_sent'),
        ]),
        dict(id='ce', agent='claude', model=None, title='Write the release notes for 1.4.', updated=now - 26 * H, unread=0, lines=[
            L('e1', True, 'Write the release notes for 1.4.', 26 * H + 2 * MIN, 'answered'),
            L('e1', False, 'Written and saved next to the changelog.', 26 * H, 'done'),
        ]),
    ]
    # Never on Mani's channel: the APK carries his real ntfy topic (res/values/topic.xml), so a real call rang this
    # emulator once mid-capture. A throwaway topic of its own in the app's settings keeps calls and chats away.
    prefs = f'/data/data/{PKG}/shared_prefs/callme.xml'
    xml = sh(f'cat {prefs}')
    if 'name="topic"' not in xml:
        xml = xml.replace('</map>', '    <string name="topic">callme-emulator-preview-7f3k2q</string>\n</map>')
        tmpx = os.path.join(FIX, '..', 'prefs.tmp.xml')
        with open(tmpx, 'w', encoding='utf-8', newline='\n') as f:
            f.write(xml + '\n')
        adb('push', tmpx, prefs)
        os.remove(tmpx)
    tmp = os.path.join(FIX, '..', 'chats.tmp.json')
    with open(tmp, 'w', encoding='utf-8') as f:
        json.dump({'chats': chats}, f)
    adb('push', tmp, f'{FILES}/chats.json')
    os.remove(tmp)
    sh(f'chown -R {uid}:{uid} {FILES} {prefs} && chmod 600 {FILES}/chats.json && restorecon -R {FILES} {prefs}')
    print('fixture written; uid', uid)


def start(v):
    adb('shell', 'setprop', 'debug.callme.chats', str(v))
    adb('shell', 'setprop', 'debug.callme.fake', '1')
    # Mani's picks: the Spotlight stage, today's controls, the Comic call
    adb('shell', 'setprop', 'debug.callme.stage', '1')
    adb('shell', 'setprop', 'debug.callme.controls', '0')
    adb('shell', 'setprop', 'debug.callme.call', '3')
    adb('shell', 'input', 'keyevent', '111')  # a keyboard left up by the last run goes first
    adb('shell', 'am', 'force-stop', PKG)
    adb('shell', 'input', 'keyevent', '3')  # home, so no stale window keeps the keyboard
    time.sleep(1)
    adb('shell', 'am', 'start', '-n', f'{PKG}/.MainActivity')
    time.sleep(6)
    for _ in range(10):  # until the home screen is up (its white Texts pill)
        if on_home():
            break
        time.sleep(1)
    time.sleep(1.5)


def on_home():
    return sum(shot().getpixel((round(160 * SF), round(589 * SF)))) > 600


def open_chats():
    """The home screen's Chats button, again if the first tap didn't open them."""
    for _ in range(3):
        tap(*CHATS)
        time.sleep(2.0)
        if not on_home():
            return


if __name__ == '__main__':
    cmd = sys.argv[1] if len(sys.argv) > 1 else 'fixture'
    if cmd == 'fixture':
        fixture()
    elif cmd == 'start':
        start(int(sys.argv[2]))
    elif cmd == 'shot':
        os.makedirs(OUT, exist_ok=True)
        shot().save(sys.argv[2])


def peek(path, w=540):
    """Saves the screen at `path` and a small copy (for looking at) beside it."""
    im = shot()
    im.save(path)
    im.resize((w, round(im.height * w / im.width)), Image.LANCZOS).save(path.rsplit('.', 1)[0] + '-s.jpg', quality=88)
    return im


SAT, H, W = 51.6, 914.3, 411.4
CW = (W - 46) / 2


def row(v, i):
    """Where to tap the list's chat i (logical px)."""
    if v == 2:
        return 16 + CW2 / 2 + (i % 2) * (CW2 + 12), SAT + 60 + (i // 2) * 252 + 126
    if v == 1:
        return 220, SAT + 60 + (165 if i == 0 else 330 + 76 * (i - 1) + 38)
    if v == 4:
        return W / 2, SAT + 60 + 222 * i + 111
    top, h = {0: (61, 72), 3: (64, 98)}[v]
    return 220, SAT + top + h * i + h / 2


CW2 = (W - 44) / 2


def field(v):
    return (260 if v == 2 else 200), 846


def attach(v):
    return (136, 846) if v == 2 else (48, 846)


PLUS = (381, SAT + 30)
CHATS = (242, 810)  # the home screen's Chats button (today's controls)


def find_photo(im, v):
    """The tallest band of orange in the thread's words (the spotlight photo's Clawd), in logical px; None if none."""
    import numpy as np
    a = np.asarray(im).astype(int)
    orange = (a[..., 0] > 170) & (a[..., 1] > 60) & (a[..., 1] < 150) & (a[..., 2] < 70)
    top = round({0: 125, 1: 270, 2: 125, 3: 145, 4: 125}[v] * SF)
    orange[:top] = False
    orange[round((640 if v == 4 else 800) * SF):] = False  # not the agent over the field (4)
    if v == 2:
        orange[:, :round(110 * SF)] = False  # not the agent by the field
    bands = orange.reshape(-1, 40, orange.shape[1]).sum(axis=(1, 2))
    b = int(bands.argmax())
    if bands[b] < 400:
        return None
    ys, xs = np.nonzero(orange[b * 40:(b + 1) * 40])
    return float(np.median(xs)) / SF, (b * 40 + 20) / SF


def save(d, name, im=None):
    im = im or shot()
    im.save(os.path.join(d, name + '.png'))
    return im


def video(v):
    """open list -> thread -> type -> send -> thinking -> reply arrives -> open a photo, as chats.mp4 (+ a small copy).
    When typing on the test emulator crawls (it did, a word in 8-10 s, for the shipped screen too, as it ran low on
    memory), the long middle of the typing is cut out of the video (a visible jump cut); otherwise it is uncut."""
    d = os.path.join(OUT, NAMES[v])
    os.makedirs(d, exist_ok=True)
    fixture()
    start(v)
    rec = subprocess.Popen(ADB + ['shell', 'screenrecord', '--time-limit', '180', '--size', '720x1600', '--bit-rate', '8000000', '/sdcard/chats.mp4'])
    t0 = time.time()
    time.sleep(1.5)
    open_chats()
    tap(*row(v, 0))
    time.sleep(2.0)
    tap(*field(v))
    time.sleep(1.4)
    words = 'ok so why is the release build slow on the laptop'.split(' ')
    text(' '.join(words[:2]))
    cut0 = time.time() - t0 + 1.0
    text(' '.join(words[2:]))
    time.sleep(2.5)
    cut1 = time.time() - t0 - 2.0  # the finished line shows a moment before it goes
    adb('shell', 'input', 'keyevent', '66')  # Enter sends
    time.sleep(9.0)  # sending, sent, working, the answer arrives (fake_chat.rs)
    adb('shell', 'input', 'keyevent', '4')  # the keyboard goes
    time.sleep(1.5)
    t_swipe = time.time() - t0
    swipe(220, 300, 700, 900)
    time.sleep(3.0)  # the fling settles (a tap while it moves only stops it)
    p = find_photo(shot(), v)
    t_tap = time.time() - t0
    if p:
        tap(*p)
    time.sleep(2.5)
    adb('shell', 'input', 'keyevent', '4')
    time.sleep(1.5)
    end = min(time.time() - t0, t_tap + 7, t_swipe + 14)  # a step that hangs is not shown
    adb('shell', 'pkill', '-INT', 'screenrecord')
    rec.wait()
    time.sleep(1.5)
    raw = os.path.join(d, 'chats-raw.mp4')
    subprocess.run(ADB + ['pull', '/sdcard/chats.mp4', raw], capture_output=True)
    adb('shell', 'rm', '/sdcard/chats.mp4')
    vid = os.path.join(d, 'chats.mp4')
    # the recording starts about a second after Popen: shift the cut by that
    a, b = cut0 + 0.6, cut1 + 0.6
    e = end + 0.6
    f = f"[0:v]trim=0:{a:.2f},setpts=PTS-STARTPTS[a];[0:v]trim={b:.2f}:{e:.2f},setpts=PTS-STARTPTS[b];[a][b]concat=n=2:v=1[o]"
    if b - a < 5:  # typing kept up: no cut
        a = b = 0
        f = f'[0:v]trim=0:{e:.2f},setpts=PTS-STARTPTS[o]'
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-i', raw, '-filter_complex', f, '-map', '[o]', '-c:v', 'libx264', '-crf', '18',
                    '-pix_fmt', 'yuv420p', '-an', vid])
    os.remove(raw)
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-i', vid, '-vf', 'scale=540:-2', '-c:v', 'libx264', '-crf', '26',
                    '-preset', 'slow', '-pix_fmt', 'yuv420p', '-an', vid.replace('.mp4', '-small.mp4')])
    print(NAMES[v], f'video; typing cut {a:.1f}-{b:.1f} s of {end:.0f} s; photo at', p)


def shots(v):
    """Every state as a still, from fresh fixtures."""
    d = os.path.join(OUT, NAMES[v])
    os.makedirs(d, exist_ok=True)
    fixture()
    start(v)
    open_chats()
    save(d, '01-list-unread')
    for i, name in [(2, '03-long-answer'), (3, '04-couldnt-load-not-sent'), (1, '05-video')]:  # Opus, Codex, Haiku
        tap(*row(v, i))
        time.sleep(2.0)
        save(d, name)
        adb('shell', 'input', 'keyevent', '4')
        time.sleep(1.2)
    tap(*row(v, 0))  # Sonnet: photos both ways
    time.sleep(2.0)
    save(d, '02-thread-photos')
    tap(*field(v))
    time.sleep(1.5)
    save(d, '06-keyboard')
    text('ok so why is the release build slow on the laptop')
    time.sleep(1.2)
    save(d, '07-typing')
    adb('shell', 'input', 'keyevent', '66')
    time.sleep(0.25)
    save(d, '08-sending')
    time.sleep(2.2)
    save(d, '09-thinking')
    time.sleep(1.6)
    save(d, '09b-thinking')
    time.sleep(5.5)  # the answer lands about 6.8 s after sending (its arrival mid-way is taken from the video)
    save(d, '11-answer')
    adb('shell', 'input', 'keyevent', '4')  # the keyboard goes
    time.sleep(1.2)
    save(d, '12-answer-no-keyboard')
    swipe(220, 300, 700, 500)
    time.sleep(1.4)
    p = find_photo(shot(), v)
    if p:
        tap(*p)
        time.sleep(1.5)
        save(d, '14-photo-open')
        adb('shell', 'input', 'keyevent', '4')
        time.sleep(1.0)
    adb('shell', 'input', 'keyevent', '4')  # to the list
    time.sleep(1.2)
    tap(*PLUS)
    time.sleep(0.08)
    save(d, '15a-picker-opening')
    time.sleep(1.2)
    save(d, '15-picker')
    pick = {0: (255, SAT + 60 + 168), 1: (250, SAT + 60 + 110), 2: (246, SAT + 60 + 110),
            3: (245, SAT + 60 + 168), 4: (114, SAT + 60 + 218)}[v]  # Opus
    tap(*pick)
    time.sleep(2.0)
    save(d, '16-new-chat-keyboard')
    adb('shell', 'input', 'keyevent', '4')
    time.sleep(1.4)
    save(d, '17-new-chat')
    adb('shell', 'input', 'keyevent', '4')
    time.sleep(1.2)
    save(d, '18-list-after')
    tap(*row(v, 1))  # Sonnet, second now
    time.sleep(2.0)
    tap(*attach(v))
    time.sleep(0.35)
    save(d, '13-chip-loading')
    time.sleep(1.4)
    save(d, '13b-chip')
    tap(*field(v))
    time.sleep(1.5)
    save(d, '13c-chip-keyboard')
    print(NAMES[v], 'stills; photo at', p)


if __name__ == '__main__' and len(sys.argv) > 2 and sys.argv[1] in ('shots', 'video', 'all'):
    for v in [int(a) for a in sys.argv[2:]]:
        if sys.argv[1] in ('video', 'all'):
            video(v)
        if sys.argv[1] in ('shots', 'all'):
            shots(v)
