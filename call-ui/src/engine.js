// The call itself, as a store the screen subscribes to. This is page.html's call logic, unchanged in
// behaviour (polling /state, sentence-by-sentence playback, the mic with its silence detection and
// interrupting, /speech, /reply), reshaped to publish one "view" the design reads. When the shared
// call engine (call-engine.js) lands, this file becomes a thin adapter over it.
import { getAudioContext } from 'voice-glow';
import { words } from './text.js';

const qs = new URLSearchParams(location.search);
const t = qs.get('t');
const api = p => `${p}?t=${t}`;
const SILENCE_MS = 1800, PREROLL_MS = 400, MAX_MS = 90_000, CLOSE_MS = 1600;
const sleep = ms => new Promise(r => setTimeout(r, ms));

export class CallEngine {
  constructor() {
    this.app = window.CallMeApp; // inside the Claude Call app: native earpiece playback and speaker switch
    this.listeners = new Set();
    this.view = { phase: 'incoming', sub: null, msgs: [], muted: false, speaker: false, speakerAvailable: !!this.app, callStart: 0, endedAt: 0, closed: false, cutIn: false, holdStart: 0, holdLabel: '', notice: '', partial: '' };
    this.voice = null; this.voiceSince = 0;
    this.lastSeq = null; this.busy = false; this.ended = false;
    this.listening = false; this.talking = false; this.segment = []; this.preroll = []; this.quietSince = 0; this.startedAt = 0; this.noiseFloor = 0.01;
    this.ctx = null; this.rate = 48000; this.micStream = null; this.micLevel = 0;
    this.speaking = false; this.interrupted = false; this.bargeFrames = 0;
    this.player = new Audio(); this.analyser = null; this.buf = new Float32Array(1024);
    this.appWaits = {};
    window.__played = (id, ok) => { this.appWaits[id]?.(ok); delete this.appWaits[id]; };
    if (qs.has('auto')) queueMicrotask(() => this.answer()); // opened from the app's own call screen
  }
  subscribe = fn => { this.listeners.add(fn); return () => this.listeners.delete(fn); };
  getSnapshot = () => this.view;
  emit(patch) { this.view = { ...this.view, ...patch }; this.listeners.forEach(fn => fn()); }
  setVoice(v) { if (v !== this.voice) { this.voice = v; this.voiceSince = performance.now(); } }

  // Levels for the voice glow and the mascot, 0-1.
  claudeLevel = () => {
    if (!this.speaking) return 0;
    if (this.analyser && !this.app) {
      this.analyser.getFloatTimeDomainData(this.buf);
      let s = 0; for (const v of this.buf) s += v * v;
      return Math.min(1, Math.sqrt(s / this.buf.length) * 8);
    }
    // The app plays through the earpiece natively, out of the page's reach: a speech-shaped envelope.
    const tt = (performance.now() - this.voiceSince) / 1000;
    return Math.max(0, 0.6 * Math.abs(Math.sin(tt * Math.PI * 4.6)) ** 1.6 * (0.55 + 0.45 * Math.sin(tt * 1.7)));
  };
  meLevel = () => (this.talking && !this.view.muted ? this.micLevel : 0);
  level = () => (this.speaking ? this.claudeLevel() : this.meLevel());

  // ---- Claude's voice ----
  play(seq, i, text) {
    const url = new URL(`${api('audio')}&seq=${seq}&part=${i}`, location.href).href;
    if (this.app) return new Promise(res => {
      const id = seq * 1000 + i;
      this.appWaits[id] = res;
      setTimeout(() => window.__played(id, true), 15000 + words(text).length * 600); // never hang the call
      this.app.play(url, id);
    });
    const p = this.player;
    return new Promise(res => {
      const backup = setTimeout(done, 5000 + words(text).length * 600);
      function done() { clearTimeout(backup); p.onended = p.onerror = null; res(true); }
      p.onended = done;
      p.onerror = () => { clearTimeout(backup); res(false); };
      p.src = url;
      p.play().catch(() => { clearTimeout(backup); res(false); });
    });
  }
  // Where in sentence i the voice is, 0-1: the real playback position in a browser; inside the app
  // (native playback, no position exposed yet) the time since it started against the sentence's length.
  progress(text) {
    const p = this.player;
    if (!this.app && p.duration > 0 && isFinite(p.duration)) return p.currentTime / p.duration;
    return Math.min(0.97, (performance.now() - this.partStart) / (300 + text.length * 62));
  }
  // Lights the words as they are said, weighting each by its length.
  track(msgId, i, text) {
    const ws = words(text), total = text.length;
    let lit = -1;
    const tick = () => {
      if (this.trackId !== msgId * 100 + i) return;
      const at = this.progress(text) * total;
      let n = 0, acc = 0;
      for (const w of ws) { if (acc + w.length * 0.4 > at) break; acc += w.length + 1; n++; }
      if (n !== lit) { lit = n; this.update(msgId, { spoken: [i, n] }); }
      requestAnimationFrame(tick);
    };
    this.trackId = msgId * 100 + i;
    requestAnimationFrame(tick);
  }
  getState = async () => (await fetch(api('state'), { cache: 'no-store' })).json();
  async speakLine(s, msgId) {
    this.interrupted = false; this.speaking = true; this.bargeFrames = 0; this.preroll = [];
    this.setVoice('claude');
    try {
      for (let i = 0; i < s.parts.length && !this.interrupted; i++) {
        let st = s;
        while (st.seq === s.seq && st.ready <= i && !this.interrupted) { await sleep(150); st = await this.getState(); }
        if (st.seq !== s.seq || this.interrupted) return;
        this.partStart = performance.now();
        this.update(msgId, { spoken: [i, 0] });
        this.track(msgId, i, s.parts[i]);
        if (!await this.play(s.seq, i, s.parts[i]) && !this.interrupted) await this.speakBrowser(s.parts[i]);
        this.trackId = null;
        if (!this.interrupted) this.update(msgId, { spoken: [i + 1, 0] });
      }
    } finally { this.speaking = false; this.trackId = null; this.setVoice(null); }
  }
  speakBrowser(text) {
    return new Promise(res => {
      const u = new SpeechSynthesisUtterance(text);
      u.lang = 'en-GB';
      const backup = setTimeout(res, 2000 + words(text).length * 450);
      u.onend = u.onerror = () => { clearTimeout(backup); res(); };
      speechSynthesis.cancel(); speechSynthesis.speak(u);
    });
  }
  stopPlayback() {
    if (this.app?.stop) this.app.stop();
    for (const id in this.appWaits) window.__played(Number(id), true);
    this.player.pause(); this.player.onended?.();
    speechSynthesis.cancel();
  }
  interrupt(now) {
    this.interrupted = true; this.speaking = false; this.stopPlayback();
    const live = this.view.msgs.find(m => m.live);
    if (live) this.update(live.id, { live: false, cut: true });
    this.listening = true; this.talking = true; this.segment = [...this.preroll]; this.preroll = [];
    this.startedAt = this.quietSince = now;
    this.setVoice('me');
    this.emit({ phase: 'hearing', cutIn: true, notice: '' });
  }

  // ---- Your voice: recorded here, transcribed on the PC ----
  async startMic() {
    const stream = this.micStream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } });
    const ctx = this.ctx = new AudioContext(); this.rate = ctx.sampleRate;
    const src = ctx.createMediaStreamSource(stream);
    const node = ctx.createScriptProcessor(4096, 1, 1);
    src.connect(node); node.connect(ctx.destination);
    node.onaudioprocess = e => {
      const buf = new Float32Array(e.inputBuffer.getChannelData(0));
      let sum = 0; for (const v of buf) sum += v * v;
      const rms = Math.sqrt(sum / buf.length);
      if (!this.listening) {
        if (this.speaking && !this.view.muted) {
          // Claude's own voice leaks back a little even with echo cancelling, so this needs clearly louder speech.
          this.preroll.push(buf);
          while (this.preroll.length * 4096 / this.rate * 1000 > PREROLL_MS) this.preroll.shift();
          this.bargeFrames = rms > Math.max(0.035, this.noiseFloor * 6) ? this.bargeFrames + 1 : 0;
          if (this.bargeFrames >= 3) this.interrupt(performance.now());
        } else if (!this.speaking) this.noiseFloor = this.noiseFloor * 0.95 + rms * 0.05;
        return;
      }
      const speech = rms > Math.max(0.012, this.noiseFloor * 3);
      this.micLevel = this.talking ? Math.min(1, rms * 12) : 0;
      const now = performance.now();
      if (!this.talking) {
        if (!speech) this.noiseFloor = this.noiseFloor * 0.95 + rms * 0.05;
        this.preroll.push(buf);
        while (this.preroll.length * 4096 / this.rate * 1000 > PREROLL_MS) this.preroll.shift();
        if (speech) {
          this.talking = true; this.segment = [...this.preroll]; this.preroll = []; this.startedAt = now; this.quietSince = now;
          this.setVoice('me');
          this.emit({ phase: 'hearing', cutIn: false, notice: '' });
        }
        return;
      }
      this.segment.push(buf);
      if (speech) this.quietSince = now;
      if (now - this.quietSince > SILENCE_MS || now - this.startedAt > MAX_MS) this.finishSpeech();
    };
  }
  listen() {
    if (this.view.muted || this.ended || !this.ctx) { this.emit({ phase: 'listening', notice: this.ctx ? '' : 'Type your reply' }); return; }
    this.ctx.resume(); this.listening = true; this.talking = false; this.segment = []; this.preroll = [];
    this.emit({ phase: 'listening', notice: '' });
  }
  stopListening() { this.listening = false; this.talking = false; this.segment = []; this.micLevel = 0; this.setVoice(null); }
  to16k(chunks) {
    const len = chunks.reduce((n, c) => n + c.length, 0), all = new Float32Array(len);
    let o = 0; for (const c of chunks) { all.set(c, o); o += c.length; }
    const ratio = this.rate / 16000, out = new Float32Array(Math.floor(len / ratio));
    for (let i = 0; i < out.length; i++) {
      const a = Math.floor(i * ratio), b = Math.min(len, Math.floor((i + 1) * ratio));
      let s = 0; for (let j = a; j < b; j++) s += all[j];
      out[i] = s / Math.max(1, b - a);
    }
    return out;
  }
  async finishSpeech() {
    const chunks = this.segment, cutIn = this.interrupted; this.stopListening(); this.interrupted = false;
    this.emit({ phase: 'thinking', sub: 'transcribing', notice: '' });
    try {
      const r = await fetch(`${api('speech')}${cutIn ? '&interrupted=1' : ''}`, { method: 'POST', body: this.to16k(chunks).buffer });
      const { text } = await r.json();
      if (text) this.addMe(text);
      else this.listen(); // only noise: keep listening
    } catch { this.emit({ phase: 'listening', notice: 'Could not send audio, type instead' }); }
  }
  addMe(text) {
    this.emit({ msgs: [...this.view.msgs, { id: this.view.msgs.length, who: 'me', text }], phase: 'thinking', sub: 'thinking', notice: '' });
  }
  update(id, patch) { this.emit({ msgs: this.view.msgs.map(m => (m.id === id ? { ...m, ...patch } : m)) }); }

  async poll() {
    if (this.ended) return;
    try {
      const s = await this.getState();
      if (!s.say) this.emit({ phase: this.view.phase === 'incoming' ? 'incoming' : 'connecting' });
      else if (s.seq !== this.lastSeq && !this.busy) {
        this.busy = true; this.lastSeq = s.seq; this.stopListening();
        const id = this.view.msgs.length;
        this.emit({ msgs: [...this.view.msgs, { id, who: 'claude', parts: s.parts, spoken: [0, 0], cut: false, live: true }], phase: 'talking', sub: null, cutIn: false, notice: '' });
        await this.speakLine(s, id);
        this.busy = false;
        if (this.interrupted && !s.end) { setTimeout(() => this.poll(), 700); return; } // already listening to you
        this.update(id, { live: false, spoken: this.interrupted ? this.view.msgs[id].spoken : [s.parts.length, 0] });
        if (s.end) return this.end();
        if (s.hold) this.emit({ phase: 'hold', holdStart: performance.now(), holdLabel: s.holdLabel || 'Checking something' }); // when the server says so
        else this.listen();
      }
    } catch { this.emit({ notice: 'Reconnecting…' }); }
    setTimeout(() => this.poll(), 700);
  }
  end() {
    this.ended = true;
    this.emit({ phase: 'ended', endedAt: performance.now(), notice: '' });
    this.micStream?.getTracks().forEach(tr => tr.stop()); this.ctx?.close(); // release the mic
    setTimeout(() => { this.emit({ closed: true }); this.app?.ended(); }, CLOSE_MS);
  }

  // ---- The controls ----
  async answer() {
    if (this.view.phase !== 'incoming') return;
    // Audio playback must first start from a tap on mobile; the glow follows Claude's voice in a browser.
    const p = this.player;
    p.src = 'data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQAAAAA=';
    p.play().catch(() => {});
    if (!this.app) try {
      const ctx = getAudioContext();
      if (ctx) { const src = ctx.createMediaElementSource(p); this.analyser = ctx.createAnalyser(); this.analyser.fftSize = 1024; src.connect(this.analyser); this.analyser.connect(ctx.destination); }
    } catch { this.analyser = null; }
    this.emit({ phase: 'connecting', callStart: performance.now() });
    try { await this.startMic(); } catch { this.emit({ muted: true, notice: 'Mic blocked, type a reply' }); }
    fetch(api('answer'), { method: 'POST' });
    this.poll();
  }
  doneTalking() { if (this.talking) this.finishSpeech(); }
  async toggleMute() {
    if (!this.ctx) { try { await this.startMic(); } catch { this.emit({ notice: 'Mic blocked, type a reply' }); return; } }
    const muted = !this.view.muted;
    this.emit({ muted });
    if (muted) { this.stopListening(); if (['listening', 'hearing'].includes(this.view.phase)) this.emit({ phase: 'listening' }); }
    else if (!this.busy && ['listening', 'hearing'].includes(this.view.phase)) this.listen();
  }
  toggleSpeaker() {
    if (!this.app) return;
    const on = !this.app.isSpeaker(); this.app.setSpeaker(on);
    this.emit({ speaker: on });
  }
  async sendTyped(text) {
    text = text.trim(); if (!text || this.ended) return;
    this.stopListening(); this.addMe(text);
    await fetch(api('reply'), { method: 'POST', body: text });
  }
}
