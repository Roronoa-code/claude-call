# PREVIEW (branch preview/call-screen-2): captures each call screen direction from the real app on the headless test
# emulator only (emulator-5584, never a phone). Each run starts its own callme.exe on a private ntfy topic with a
# throwaway status file and a local link (CALLME_LOCAL, reached through adb reverse), so no real phone can ring and
# the real status.json is never touched. The call screen is opened over adb like the ringing notification does;
# replies are typed in the app's own reply field.
# Your live words are shown with debug.callme.hear (screen.rs), as the emulator's mic is silent.
# Per direction: stills of every state, call.mp4 (ring, answer, talking, your turn, you talking, typing, thinking,
# hold, talking, More, hang up) and a 540 px wide copy. Usage: python call_capture.py <look 0-4> [probe] [Codex]
# (debug.callme.call: 0 Comic as shipped, 1 Subtitles, 2 Script, 3 Your light, 4 Camera)
import io, json, os, re, subprocess, sys, threading, time, urllib.request

os.environ['MSYS_NO_PATHCONV'] = '1'
SER = 'emulator-5584'
EXE = r'B:\Projects\CallMe\target\release\callme.exe'
OUT = r'D:\Outputs\CallMe\design\step4-call-2'
NAMES = {0: '0-current', 1: '1-subtitles', 2: '2-script', 3: '3-your-light', 4: '4-camera'}
DP = 2.625
# Where each direction's buttons are, in dp from the top left (the bottom ones from the bottom edge, as `-y`).
TAPS = {
    0: dict(answer=(274, -68), type=(360, -159), hang=(79, -68)),
    1: dict(answer=(293, -74), type=(291, -74), more=(357, -74), shut=(357, -74), hang=(52, -74)),
    2: dict(answer=(359, -72), type=(206, -72), more=(254, -72), shut=(290, -72), hang=(52, -72)),
    3: dict(answer=(288, -86), type=(288, -86), more=(370, -86), shut=(370, -86), hang=(206, -86)),
    4: dict(answer=(289, -74), type=(377, -184), more=(377, -280), shut=(377, -280), hang=(88, -74)),
}
CF = 0x08000000  # no console window


def adb(*a, out=True):
    r = subprocess.run(['adb', '-s', SER, *a], capture_output=True, creationflags=CF)
    return r.stdout


def px(p, h=2400):
    x, y = p
    return round(x * DP), round(y * DP) if y >= 0 else h + round(y * DP)


class Run:
    def __init__(self, look, agent):
        self.dir = os.path.join(OUT, NAMES[look])
        os.makedirs(self.dir, exist_ok=True)
        self.look, self.t0 = look, time.time()
        stamp = str(int(time.time() * 1000))
        env = dict(os.environ, CALLME_TOPIC='claude-check-step4-' + stamp, CALLME_STATUS=os.path.join(os.environ['TEMP'], f'callme-step4-{stamp}.json'),
                   CALLME_LOCAL='1', CALLME_AGENT=agent, CALLME_DEBUG='1')
        self.p = subprocess.Popen([EXE], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1, creationflags=CF)
        self.link, self.pending, self.nid, self.lock = None, {}, 0, threading.Lock()
        threading.Thread(target=self._err, daemon=True).start()
        threading.Thread(target=self._out, daemon=True).start()
        self.req('initialize', {'protocolVersion': '2025-06-18', 'capabilities': {}, 'clientInfo': {'name': 'step4', 'version': '1'}})

    def say(self, m):
        print(f'[{time.time() - self.t0:5.1f}] {m}', flush=True)

    def _err(self):
        for line in self.p.stderr:
            m = re.search(r'call link (\S+)', line)
            if m:
                self.link = m.group(1)

    def _out(self):
        for line in self.p.stdout:
            try:
                v = json.loads(line)
            except Exception:
                continue
            if v.get('id') in self.pending:
                self.pending[v['id']].append(v)

    def req(self, method, params):
        with self.lock:
            self.nid += 1
            i = self.nid
            self.pending[i] = []
            self.p.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': i, 'method': method, 'params': params}) + '\n')
            self.p.stdin.flush()
        while not self.pending[i]:
            time.sleep(0.05)
        return self.pending[i][0]

    def call(self, name, msg):
        r = self.req('tools/call', {'name': name, 'arguments': {'message': msg}})
        try:
            return r['result']['content'][0]['text']
        except Exception:
            return str(r)

    def bg(self, name, msg):
        res = []
        t = threading.Thread(target=lambda: res.append(self.call(name, msg)), daemon=True)
        t.start()
        return t, res

    def shot(self, name):
        open(os.path.join(self.dir, name + '.png'), 'wb').write(adb('exec-out', 'screencap', '-p'))
        self.say('shot ' + name)

    def tap(self, what):
        focus = adb('shell', 'dumpsys', 'window').decode(errors='ignore')
        line = [l for l in focus.splitlines() if 'mCurrentFocus=' in l][0]
        assert 'CallActivity' in line, 'call screen not in front; not tapping ' + what
        x, y = px(TAPS[self.look][what])
        adb('shell', 'input', 'tap', str(x), str(y))
        self.say('tap ' + what)

    def record(self, secs, name):
        def go():
            adb('shell', 'screenrecord', '--time-limit', str(secs), '--bit-rate', '12000000', f'/sdcard/{name}.mp4')
            time.sleep(0.5)
            adb('pull', f'/sdcard/{name}.mp4', os.path.join(self.dir, name + '.mp4'))
            adb('shell', 'rm', f'/sdcard/{name}.mp4')
        t = threading.Thread(target=go, daemon=True)
        t.start()
        return t

    def ring(self, first):
        call = self.bg('call_user', first)
        while not self.link:
            time.sleep(0.2)
        port = re.search(r':(\d+)', self.link.split('//', 1)[1]).group(1)
        adb('reverse', f'tcp:{port}', f'tcp:{port}')
        self.say('link ' + self.link)
        adb('shell', f"am start -n fyi.mani.callme/.CallActivity --es link '{self.link}'")
        return call


def main():
    look = int(sys.argv[1])
    probe = 'probe' in sys.argv
    agent = 'Codex' if 'Codex' in sys.argv else 'Claude'
    adb('shell', 'setprop', 'debug.callme.call', str(look))
    adb('shell', 'am', 'force-stop', 'fyi.mani.callme')
    time.sleep(1)
    r = Run(look, agent)
    who = 'Codex' if agent == 'Codex' else 'Claude'
    if probe:
        first, res = r.ring(f"Hi, it's {who}. Quick check of the new call screen, in the spotlight.")
        time.sleep(3)
        r.shot('probe-ringing')
        r.tap('answer')
        time.sleep(4)
        r.shot('probe-talking')
        time.sleep(6)
        r.shot('probe-listening')
        r.call('end_call', 'Bye.')
        time.sleep(0.5)
        r.shot('probe-ended')
        r.p.kill()
        return
    rec = r.record(100, 'call')
    time.sleep(1.0)
    first, res = r.ring(f"Hi, it's {who}. I finished the new call screen. Want to hear what changed before I push it?")
    time.sleep(1.6)
    r.shot('1-ringing')
    time.sleep(1.3)
    r.shot('1b-ringing')
    r.tap('answer')
    time.sleep(0.35)
    r.shot('2a-answer-mid')
    time.sleep(2.6)
    r.shot('2-talking')
    time.sleep(2.0)
    r.shot('2b-talking')
    time.sleep(6.5)  # the line ends: your turn
    r.shot('3-listening')
    # you talk (shown with debug.callme.hear), then stop and type instead
    for i, words in enumerate(['Yes', 'Yes please,', 'Yes please, the', 'Yes please, the short', 'Yes please, the short version.']):
        adb('shell', 'setprop', 'debug.callme.hear', f"'{words}'")
        time.sleep(0.45)
        if i == 2:
            r.shot('3a-hearing-mid')
    time.sleep(1.2)
    r.shot('3b-hearing')
    adb('shell', 'setprop', 'debug.callme.hear', "''")
    time.sleep(1.5)
    r.tap('type')
    time.sleep(1.6)
    for w in ['Yes', 'please,', 'the', 'short', 'version.']:  # a word at a time, as a person types (a burst can drop letters)
        adb('shell', 'input', 'text', w + '%s')
        time.sleep(0.35)
    time.sleep(1.0)
    r.shot('4-typing')
    adb('shell', 'input', 'keyevent', 'KEYCODE_ENTER')
    first.join()
    r.say('REPLY 1 ' + str(res))
    time.sleep(0.6)
    r.shot('5-thinking')
    time.sleep(2.2)
    r.shot('5b-thinking')
    t, res = r.bg('hold_call', 'One sec, let me pull up the list.')
    time.sleep(5.5)
    r.shot('6-hold')
    t.join()
    time.sleep(2.0)
    r.shot('6b-hold')
    t, res = r.bg('continue_call', "Okay. I'm standing in the light now, the words are part of the scene, and the buttons stay out of the way until you need them. Anything you'd change?")
    time.sleep(5.5)
    r.shot('7-talking-long')
    time.sleep(3.0)
    r.shot('7b-talking-long')
    if 'more' in TAPS[look]:  # the shipped Comic has no More
        r.tap('more')
        time.sleep(0.25)
        r.shot('7c-more-mid')
        time.sleep(1.0)
        r.shot('7d-more-open')
        r.tap('shut')
        time.sleep(1.2)
    r.tap('hang')
    time.sleep(0.35)
    r.shot('8-ended')
    rec.join()
    r.p.kill()
    vid = os.path.join(r.dir, 'call.mp4')
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-i', vid, '-vf', 'scale=540:-2', '-c:v', 'libx264', '-crf', '26',
                    '-preset', 'slow', '-pix_fmt', 'yuv420p', '-an', vid.replace('.mp4', '-small.mp4')], creationflags=CF)
    r.say('done')


if __name__ == '__main__':
    main()
