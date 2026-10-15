# PREVIEW (branch preview/chats-2): drives the chat redesign on the headless test emulator only (emulator-5582, never
# a phone). The emulator's chats are replaced with local fixtures (chat_fixture/), and debug.callme.fake plays the PC
# on the emulator (phone/src/fake_chat.rs): nothing is sent to the PC or to ntfy. The build carries a throwaway topic.
# Usage: python chat2_capture.py <step> [args]   (steps: fixture, start, shot NAME, tap X Y, key N, type TEXT, ...)
import os, sys, time, json, subprocess, io
from PIL import Image

os.environ['MSYS_NO_PATHCONV'] = '1'
ADB = ['adb', '-s', 'emulator-5582']
PKG = 'fyi.mani.callme'
FILES = f'/data/data/{PKG}/files'
HERE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(HERE, 'chat_fixture')
OUT = r'D:\Outputs\CallMe\design\chats-2'
SF = 2.625
W, H = 1080 / SF, 2400 / SF


def adb(*a, out=False):
    r = subprocess.run(ADB + list(a), capture_output=True)
    return r.stdout if out else None


def sh(cmd):
    return adb('shell', cmd, out=True).decode(errors='replace').strip()


def shot():
    return Image.open(io.BytesIO(adb('exec-out', 'screencap', '-p', out=True))).convert('RGB')


def save(name, d=OUT):
    os.makedirs(d, exist_ok=True)
    im = shot()
    im.save(os.path.join(d, name + '.png'))
    return im


def tap(x, y):
    """x, y in logical px (the app's), as the .slint files place things."""
    adb('shell', 'input', 'tap', str(round(x * SF)), str(round(y * SF)))


def swipe(x, y0, y1, ms=350):
    adb('shell', 'input', 'swipe', *[str(round(v * SF)) for v in (x, y0, x, y1)], str(ms))


def key(n):
    adb('shell', 'input', 'keyevent', str(n))


def text(s):
    """Word by word (a long burst loses letters; adb's capitals don't reach the field)."""
    for i, w in enumerate(s.split(' ')):
        if i:
            key(62)
        adb('shell', 'input', 'text', w)
        time.sleep(0.25)


LONG = """Two things stop it after a reboot:

1. The update alarm is set in `onEnabled()` only, so a reboot clears it and nothing sets it again.
2. `WidgetProvider` doesn't listen for `BOOT_COMPLETED`, so it never gets the chance.

## The fix

Register for the boot broadcast in `AndroidManifest.xml`:

```xml
<receiver android:name=".WidgetProvider" android:exported="false">
  <intent-filter>
    <action android:name="android.intent.action.BOOT_COMPLETED" />
  </intent-filter>
</receiver>
```

Then call `schedule_next()` from `onReceive()`:

```rust
fn on_receive(env: &mut JNIEnv, intent: &JObject) -> Result<()> {
    if action(env, intent)? == BOOT_COMPLETED {
        schedule_next(env)?; // the alarm comes back
    }
    Ok(())
}
```

- About **20 lines** in `widget.rs`
- No new permission: `RECEIVE_BOOT_COMPLETED` is already in the manifest

Want me to push it?"""


def fixture(working=False):
    """Fresh chats on the emulator: written to its store with its own clock, files in filesDir/media. working: the
    Sonnet chat's last message is being worked on (for the shipped build, which has no fake PC to play it)."""
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
    MIN, HR = 60_000, 3_600_000
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
            L('c1', False, LONG, 63 * MIN, 'done'),
        ]),
        dict(id='cd', agent='codex', model=None, title='Tidy up the old branches.', updated=now - 2 * HR, unread=0, lines=[
            L('d1', True, 'Tidy up the old branches.', 2 * HR + 9 * MIN, 'answered'),
            L('d1', False, 'Deleted 14 merged branches; kept the two with open work. The graph is attached.', 2 * HR + 5 * MIN, 'done', [m('graph.png', 'image/png', 'failed')]),
            L('d2', True, 'And the stale tags?', 2 * HR, 'not_sent'),
        ]),
        dict(id='ce', agent='claude', model=None, title='Write the release notes for 1.4.', updated=now - 26 * HR, unread=0, lines=[
            L('e1', True, 'Write the release notes for 1.4.', 26 * HR + 2 * MIN, 'answered'),
            L('e1', False, 'Written and saved next to the changelog.', 26 * HR, 'done'),
        ]),
    ]
    if working:
        chats[0]['lines'].append(L('a3', True, 'why is the release build slow on the laptop', 20_000, 'working'))
        chats[0]['updated'] = now - 20_000
    tmp = os.path.join(HERE, 'chats.tmp.json')
    with open(tmp, 'w', encoding='utf-8') as f:
        json.dump({'chats': chats}, f)
    adb('push', tmp, f'{FILES}/chats.json')
    os.remove(tmp)
    sh(f'chown -R {uid}:{uid} {FILES} && chmod 600 {FILES}/chats.json && restorecon -R {FILES}')
    print('fixture written; uid', uid)


def start():
    adb('shell', 'setprop', 'debug.callme.fake', '1')
    key(111)  # a keyboard left up by the last run goes first
    adb('shell', 'am', 'force-stop', PKG)
    key(3)
    time.sleep(1)
    adb('shell', 'am', 'start', '-n', f'{PKG}/.MainActivity')
    time.sleep(8)


# The home screen's Chats button (home.slint: the bottom row, 24px in), the list's "+", its rows, the field.
SAT = 51.6
ROW0 = SAT + 116 + 39  # list.slint: rows start under the big title, 78px each
PLUS = (W - 28, SAT + 30)


def row(i):
    return 200, ROW0 + 78 * i


def video():
    """The flow in one take: list, a thread, typing, sending, thinking, the answer landing, a photo full screen, the
    new-chat picker and an empty new chat. Saved as flow.mp4 and a 540px copy in OUT."""
    fixture()
    start()
    rec = subprocess.Popen(ADB + ['shell', 'screenrecord', '--time-limit', '75', '--bit-rate', '12000000', '/sdcard/flow.mp4'])
    time.sleep(2.0)
    tap(49, 841)  # Chats
    time.sleep(2.2)
    tap(*row(0))  # Sonnet
    time.sleep(2.4)
    tap(200, 853)  # the field
    time.sleep(1.6)
    text('why is the release build slow on the laptop')
    time.sleep(1.2)
    key(66)  # Enter sends
    time.sleep(10.5)  # sending, sent, thinking, the answer lands (fake_chat.rs)
    key(4)  # the keyboard goes
    time.sleep(1.5)
    swipe(200, 200, 860, 250)  # up to the top
    time.sleep(2.0)
    tap(100, 450)  # the first photo (the record stage)
    time.sleep(2.4)
    key(4)
    time.sleep(1.4)
    key(4)  # to the list
    time.sleep(1.8)
    tap(*PLUS)
    time.sleep(2.2)
    tap(280, 200)  # Opus
    time.sleep(3.0)
    key(4)  # the keyboard goes
    time.sleep(2.0)
    key(4)  # to the list
    time.sleep(2.0)
    adb('shell', 'pkill', '-INT', 'screenrecord')
    rec.wait()
    time.sleep(1.5)
    os.makedirs(OUT, exist_ok=True)
    raw = os.path.join(OUT, 'flow-raw.mp4')
    subprocess.run(ADB + ['pull', '/sdcard/flow.mp4', raw], capture_output=True)
    adb('shell', 'rm', '/sdcard/flow.mp4')
    vid = os.path.join(OUT, 'flow.mp4')
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-i', raw, '-c:v', 'libx264', '-crf', '18', '-pix_fmt', 'yuv420p', '-an', vid])
    os.remove(raw)
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-i', vid, '-vf', 'scale=540:-2', '-c:v', 'libx264', '-crf', '26',
                    '-preset', 'slow', '-pix_fmt', 'yuv420p', '-an', os.path.join(OUT, 'flow-540.mp4')])
    print('video saved')


if __name__ == '__main__':
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == 'fixture':
        fixture(args == ['working'])
    elif cmd == 'start':
        start()
    elif cmd == 'shot':
        save(args[0], args[1] if len(args) > 1 else OUT)
    elif cmd == 'tap':
        tap(float(args[0]), float(args[1]))
    elif cmd == 'row':
        tap(*row(int(args[0])))
    elif cmd == 'key':
        key(args[0])
    elif cmd == 'type':
        text(' '.join(args))
    elif cmd == 'swipe':
        swipe(float(args[0]), float(args[1]), float(args[2]), int(args[3]) if len(args) > 3 else 350)
    elif cmd == 'video':
        video()

