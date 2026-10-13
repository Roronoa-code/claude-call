"""Slows speech down without changing its pitch (WSOLA), on a stream: feed pieces as they are made, get back the
stretched audio so far. The cloned voice talks a little fast for a phone call."""
import numpy as np

SLOWER = 1.1  # 10% slower


class Stretch:
    def __init__(self, rate: float = SLOWER, n: int = 1024, tol: int = 256):
        self.n, self.hs, self.tol = n, n // 2, tol
        self.ha = self.hs / rate  # how far the input moves per output step
        k = np.arange(n)
        self.win = (0.5 - 0.5 * np.cos(2 * np.pi * k / n)).astype(np.float32)  # periodic Hann: halves overlap to 1
        self.x = np.zeros(0, np.float32)  # input not yet used up; self.x[0] is input sample self.base
        self.base = 0
        self.end = 0  # input samples received
        self.pos = 0.0  # where the next frame would come from
        self.prev = None  # where the last frame came from
        self.tail = np.zeros(self.hs, np.float32)

    def feed(self, chunk, last: bool = False) -> np.ndarray:
        chunk = np.asarray(chunk, dtype=np.float32)
        self.x = np.concatenate([self.x, chunk])
        self.end += len(chunk)
        if last:  # pad so the final frames can be cut
            self.x = np.concatenate([self.x, np.zeros(self.n + 2 * self.tol, np.float32)])
        out = []
        while self.pos < self.end:
            want = int(self.pos)
            lo, hi = max(self.base, want - self.tol), want + self.tol
            if hi + self.n > self.base + len(self.x):
                break  # wait for more input
            if self.prev is None:
                best = want
            else:
                # The frame that best continues the last one's own next half, so the joins don't warble.
                t0 = self.prev + self.hs - self.base
                target = self.x[t0:t0 + self.n]
                seg = self.x[lo - self.base:hi - self.base + self.n]
                best = lo + int(np.argmax(np.correlate(seg, target, "valid")))
            f = self.x[best - self.base:best - self.base + self.n] * self.win
            out.append(f[:self.hs] + self.tail)
            self.tail = f[self.hs:].copy()
            self.prev = best
            self.pos += self.ha
            cut = min(self.prev, int(self.pos) - self.tol) - self.base
            if cut > 0:
                self.x, self.base = self.x[cut:], self.base + cut
        if last:
            out.append(self.tail)
        return np.concatenate(out) if out else np.zeros(0, np.float32)


if __name__ == "__main__":  # self-check: 10% longer, same pitch, streamed or whole
    sr = 24000
    t = np.arange(sr * 2) / sr
    tone = (0.5 * np.sin(2 * np.pi * 220 * t)).astype(np.float32)
    whole = Stretch().feed(tone, last=True)
    s = Stretch()
    streamed = np.concatenate([s.feed(tone[i:i + 1920]) for i in range(0, len(tone), 1920)] + [s.feed([], last=True)])
    for y in (whole, streamed):
        assert abs(len(y) / len(tone) - SLOWER) < 0.03, len(y) / len(tone)
        mid = y[sr // 2:sr // 2 + sr]
        peak = np.argmax(np.abs(np.fft.rfft(mid))) * sr / len(mid)
        assert abs(peak - 220) < 3, peak
    print("ok", round(len(whole) / len(tone), 3), round(len(streamed) / len(tone), 3))
