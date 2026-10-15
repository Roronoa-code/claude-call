# PREVIEW (branch preview/chats-4): drives the chats directions on the headless test emulator only (emulator-5582,
# never a phone). Chats are local fixtures (chat_fixture/) and debug.callme.fake plays the PC on the emulator
# (phone/src/fake_chat.rs): nothing is sent to the PC or ntfy. debug.callme.chats picks the direction (0 as shipped).
# Usage: python chat4_capture.py <step> [args]   (fixture [working], start N, shot NAME DIR, tap X Y, key N, type ..., video N DIR)
import os, sys, time, json, subprocess, io
from PIL import Image

os.environ['MSYS_NO_PATHCONV'] = '1'
ADB = ['adb', '-s', 'emulator-5582']
PKG = 'fyi.mani.callme'
FILES = f'/data/data/{PKG}/files'
HERE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(HERE, 'chat_fixture')
OUT = r'D:\Outputs\CallMe\design\chats-4'
FF = r'B:\Tools\MediaTools\bin\ffmpeg.exe'
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


def swipe(x, y0, y1, ms=350, x1=None):
    adb('shell', 'input', 'swipe', *[str(round(v * SF)) for v in (x, y0, x if x1 is None else x1, y1)], str(ms))


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
    """Fresh chats on the emulator, written to its store with its own clock; files in filesDir/media. working: the
    Sonnet chat's last message is being worked on (thinking, in the list and its thread)."""
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
    L = lambda id, me, text, ago, state, media=(), choices=(): dict(id=id, me=me, text=text, at=now - ago, since=now - ago, state=state,
                                                                    parts=[], media=list(media), choices=list(choices))
    MIN, HR = 60_000, 3_600_000
    chats = [
        dict(id='ask-k1', agent='claude', title='Notebook sync', kind='ask', asked=now - 3 * MIN, updated=now - 3 * MIN, unread=1, lines=[
            L('k1', False, 'Your notes changed on both the phone and the PC since the last sync. **Which copy should win?**', 3 * MIN, 'done',
              choices=["Keep the phone's", "Keep the PC's", 'Merge both']),
        ]),
        dict(id='ca', agent='claude', model='sonnet-5.5', title='Can you send a screenshot of the new home screen?', updated=now - 5 * MIN, unread=0, lines=[
            L('a1', True, 'Can you send a screenshot of the new home screen?', 12 * MIN, 'answered'),
            L('a1', False, 'Here it is on the record stage. The tokens sit in a gentle curve under it.', 11 * MIN, 'done', [m('home.jpg', 'image/jpeg')]),
            L('a2', True, "This one's from the old spotlight stage, for comparison.", 7 * MIN, 'answered', [m('spotlight.jpg', 'image/jpeg')]),
            L('a2', False, 'The record reads cleaner: one light and no haze behind Clawd, so his edges stay crisp at small sizes.', 5 * MIN, 'done'),
        ]),
        dict(id='cb', agent='claude', model='haiku-5.5', title='Record the new picker opening', updated=now - 9 * MIN, unread=2, lines=[
            L('b1', True, 'Record the new picker opening', 11 * MIN, 'answered'),
            L('b1', False, "Here's the recording, five seconds from the home screen.", 9 * MIN, 'done', [m('picker.mp4', 'video/mp4')]),
        ]),
        dict(id='updates', agent='claude', title='Updates', kind='note', asked=now - 40 * MIN, updated=now - 40 * MIN, unread=0, lines=[
            L('u1', False, 'Nightly build passed. **142 tests**, no failures.', 3 * HR, 'done'),
            L('u2', False, 'The new home screen is on the emulator. This is how it looks on the record stage.', 40 * MIN, 'done', [m('home.jpg', 'image/jpeg')]),
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
        chats[1]['lines'].append(L('a3', True, 'why is the release build slow on the laptop', 20_000, 'working'))
        chats[1]['updated'] = now - 20_000
        chats.insert(0, chats.pop(1))
    tmp = os.path.join(HERE, 'chats.tmp.json')
    with open(tmp, 'w', encoding='utf-8') as f:
        # a stale test ask still on the throwaway topic's cache stays deleted
        json.dump({'chats': chats, 'gone': ['ask-6f1c2b3a-0000-4000-8000-0000000000a7/6f1c2b3a-0000-4000-8000-0000000000a7']}, f)
    adb('push', tmp, f'{FILES}/chats.json')
    os.remove(tmp)
    sh(f'chown -R {uid}:{uid} {FILES} && chmod 600 {FILES}/chats.json && restorecon -R {FILES}')
    print('fixture written; uid', uid)


def start(look=None):
    adb('shell', 'setprop', 'debug.callme.fake', '1')
    if look is not None:
        adb('shell', 'setprop', 'debug.callme.chats', str(look))
    key(111)  # a keyboard left up by the last run goes first
    adb('shell', 'am', 'force-stop', PKG)
    key(3)
    time.sleep(1)
    adb('shell', 'am', 'start', '-n', f'{PKG}/.MainActivity')
    time.sleep(8)


CHATS_BTN = (49, 841)  # home.slint: the Chats button in the bottom row


def video(look, d, row_xy, field_xy, settle=2.2):
    """The flow at real speed: open the list, open a chat, send, thinking, the answer arrives, back to the list.
    Saved as flow.mp4 (full) and flow-540.mp4 in d."""
    fixture()
    start(look)
    rec = subprocess.Popen(ADB + ['shell', 'screenrecord', '--time-limit', '40', '--bit-rate', '12000000', '/sdcard/flow.mp4'])
    time.sleep(1.5)
    tap(*CHATS_BTN)
    time.sleep(settle)
    tap(*row_xy)
    time.sleep(2.0)
    tap(*field_xy)
    time.sleep(1.4)
    text('why is the release build slow on the laptop')
    time.sleep(0.8)
    key(66)  # Enter sends
    time.sleep(10.0)  # sending, sent, thinking, the answer lands (fake_chat.rs)
    key(4)  # the keyboard goes
    time.sleep(1.6)
    key(4)  # to the list
    time.sleep(2.4)
    adb('shell', 'pkill', '-INT', 'screenrecord')
    rec.wait()
    time.sleep(1.5)
    os.makedirs(d, exist_ok=True)
    raw = os.path.join(d, 'flow-raw.mp4')
    subprocess.run(ADB + ['pull', '/sdcard/flow.mp4', raw], capture_output=True)
    adb('shell', 'rm', '/sdcard/flow.mp4')
    vid = os.path.join(d, 'flow.mp4')
    subprocess.run([FF, '-y', '-loglevel', 'error', '-i', raw, '-c:v', 'libx264', '-crf', '18', '-pix_fmt', 'yuv420p', '-an', vid])
    os.remove(raw)
    subprocess.run([FF, '-y', '-loglevel', 'error', '-i', vid, '-vf', 'scale=540:-2', '-c:v', 'libx264', '-crf', '26',
                    '-preset', 'slow', '-pix_fmt', 'yuv420p', '-an', os.path.join(d, 'flow-540.mp4')])
    print('video saved')


if __name__ == '__main__':
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == 'fixture':
        fixture(args == ['working'])
    elif cmd == 'start':
        start(int(args[0]) if args else None)
    elif cmd == 'shot':
        save(args[0], args[1] if len(args) > 1 else OUT)
    elif cmd == 'tap':
        tap(float(args[0]), float(args[1]))
    elif cmd == 'key':
        key(args[0])
    elif cmd == 'type':
        text(' '.join(args))
    elif cmd == 'swipe':
        swipe(float(args[0]), float(args[1]), float(args[2]), int(args[3]) if len(args) > 3 else 350)
    elif cmd == 'video':
        look, d = int(args[0]), args[1]
        video(look, d, (float(args[2]), float(args[3])), (float(args[4]), float(args[5])))
