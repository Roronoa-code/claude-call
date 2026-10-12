// A simulated call, step by step, so every design can be watched without a phone or a mic.
// The copy and the order of states follow page.html: Claude talks sentence by sentence,
// the mic listens, "Hearing you…" while you speak, "Got it, transcribing…", "Claude is thinking…".

export const WORD_MS = 340; // about 175 words a minute, the pace of the cloned voice
const SENTENCE_GAP_MS = 420; // the next sentence's audio arriving
const TRANSCRIBE_MS = 900;
const REDUCE = matchMedia('(prefers-reduced-motion: reduce)');

export const SCRIPT = [
  { s: 'incoming', ms: 2800 },
  { s: 'connecting', ms: 1300 },
  { s: 'talking', parts: ["Hi Abdul, it's Claude.", 'The Notebook backup is finished and checked.', 'Before I install it, should I keep the old backups on D:, or clear them out?'] },
  { s: 'listening', ms: 2000 },
  { s: 'hearing', ms: 2600 },
  { s: 'thinking', said: 'Keep the last three and clear the rest.', ms: 3600 },
  { s: 'talking', parts: ["Okay, I'll keep the last three.", 'Give me a moment to check how much space that frees up.'] },
  { s: 'hold', label: 'Checking the backup folder', ms: 6000 },
  { s: 'talking', parts: ['Thanks for waiting.', 'Clearing the rest frees about 41 gigabytes.', 'Is there anything else you want me to look at while I am in there?'], cut: { part: 2, word: 4 } },
  { s: 'hearing', ms: 1900, cutIn: true },
  { s: 'thinking', said: "No, that's everything. Thanks, Claude.", ms: 2800 },
  { s: 'talking', parts: ["Great, I'll message you once it's installed.", 'Bye for now.'], end: true },
  { s: 'ended', ms: 1600 },
];

// Where each preview state button jumps to.
export const STATE_STEPS = { incoming: 0, talking: 6, listening: 3, hearing: 4, thinking: 5, hold: 7, ended: 12 };
export const STATE_LABELS = { incoming: 'Incoming', talking: 'Claude talking', listening: 'Listening', hearing: 'Hearing you', thinking: 'Thinking', hold: 'On hold', ended: 'Ended' };

const words = t => t.split(/\s+/).filter(Boolean);
function stepMs(st) {
  if (st.s !== 'talking') return st.ms || 0;
  const parts = st.cut ? st.parts.slice(0, st.cut.part + 1) : st.parts;
  let n = 0;
  parts.forEach((p, i) => { n += st.cut && i === st.cut.part ? st.cut.word : words(p).length; });
  return n * WORD_MS + parts.length * SENTENCE_GAP_MS;
}

// What a finished step leaves in the transcript.
function settle(msgs, st) {
  if (st.s === 'talking') {
    const cutPart = st.cut ? st.cut.part : Infinity;
    msgs.push({ id: msgs.length, who: 'claude', parts: st.parts, spoken: st.cut ? [cutPart, st.cut.word] : [st.parts.length, 0], cut: !!st.cut });
  }
  if (st.s === 'thinking') msgs.push({ id: msgs.length, who: 'me', text: st.said || st.typed });
}

export class CallSim {
  constructor() {
    this.listeners = new Set();
    this.timers = [];
    this.view = null;
    this.steps = SCRIPT.slice();
    this.muted = false;
    this.speaker = false;
    this.voice = null; // who is making sound: 'claude' | 'me' | null
    this.voiceSince = 0;
    this.jump(0, false);
  }
  subscribe = fn => { this.listeners.add(fn); return () => this.listeners.delete(fn); };
  getSnapshot = () => this.view;
  emit(patch) {
    this.view = { ...this.view, ...patch, muted: this.muted, speaker: this.speaker };
    this.listeners.forEach(fn => fn());
  }
  clear() { this.timers.forEach(clearTimeout); this.timers = []; clearInterval(this.ticker); }
  later(ms, fn) { this.timers.push(setTimeout(fn, ms)); }

  // Level of whoever is speaking, 0-1, sampled every frame by the voice glow.
  level = () => {
    if (!this.voice || (this.voice === 'me' && this.muted)) return 0;
    const t = (performance.now() - this.voiceSince) / 1000;
    const syllable = Math.abs(Math.sin(t * Math.PI * 4.6)) ** 1.6;
    const phrase = 0.55 + 0.45 * Math.sin(t * 1.7 + Math.sin(t * 0.6));
    const gap = Math.sin(t * 2.3) > 0.82 ? 0.25 : 1; // short pauses between words
    const base = this.voice === 'me' ? 0.72 : 0.6;
    return Math.max(0, Math.min(1, base * syllable * phrase * gap + 0.04 * Math.random()));
  };
  // "Working on it": the glow gathers into one beam that sweeps side to side while Claude is
  // transcribing or thinking. voice-glow 0.3 dropped its free `processing` prop, so this drives
  // the documented `motion` prop instead. Reduced motion keeps the beam centred and still.
  motion = () => {
    const on = this.view.phase === 'thinking';
    const now = performance.now();
    if (on !== this.pOn) { this.pOn = on; this.pFrom = this.gather || 0; this.pAt = now; }
    const k = Math.min(1, (now - this.pAt) / 420), e = 1 - (1 - k) ** 3;
    this.gather = this.pFrom + ((on ? 1 : 0) - this.pFrom) * e;
    if (this.gather < 0.002) return null;
    if (REDUCE.matches) return { gather: this.gather, offset: 0, heldLevel: 0.5 };
    const a = ((now - this.pAt) / 2600) * Math.PI * 2;
    return { gather: this.gather, offset: 0.6 * Math.sin(a), stretch: 0.3 * Math.abs(Math.cos(a)), heldLevel: 0.5 };
  };
  meLevel = () => (this.voice === 'me' ? this.level() : 0); // only your voice, for a reply box
  setVoice(v) { if (v !== this.voice) { this.voice = v; this.voiceSince = performance.now(); } }

  // Rebuild the call as it would be just as step `i` starts, then play it.
  // run=false holds the step at its natural resting point (the preview's state buttons).
  jump(i, run) {
    this.clear();
    this.steps = SCRIPT.slice();
    const msgs = [];
    let elapsed = 0;
    for (let j = 0; j < i; j++) {
      settle(msgs, this.steps[j]);
      if (j >= 2) elapsed += stepMs(this.steps[j]); // the timer starts on answering
    }
    this.run = run;
    this.idx = i;
    this.callStart = performance.now() - elapsed;
    this.view = { idx: i, msgs, phase: 'incoming', sub: null, live: null, holdStart: 0, callStart: this.callStart, run, closed: false };
    this.enter();
  }
  next() { if (this.idx < this.steps.length - 1) { this.idx++; this.enter(); } }

  enter() {
    this.clear();
    const st = this.steps[this.idx];
    const base = { idx: this.idx, sub: null, live: null, closed: false };
    const advance = ms => { if (this.run) this.later(ms, () => { settle(this.view.msgs, st); this.next(); }); };
    this.setVoice(null);
    switch (st.s) {
      case 'incoming':
      case 'connecting':
        this.emit({ ...base, phase: st.s });
        if (this.run) this.later(st.ms, () => this.next());
        break;
      case 'talking': return this.talk(st, base);
      case 'listening':
      case 'hearing':
        this.emit({ ...base, phase: st.s, cutIn: !!st.cutIn });
        if (this.muted) return this.emit({ phase: 'listening' }); // waits until unmuted
        if (st.s === 'hearing') this.setVoice('me');
        if (this.run && st.ms) this.later(st.ms, () => (st.s === 'hearing' ? this.doneTalking() : this.next()));
        break;
      case 'thinking': {
        const typed = !!st.typed;
        this.emit({ ...base, phase: 'thinking', sub: typed ? 'thinking' : 'transcribing', pendingMe: typed ? null : st.said });
        if (typed) { this.view.msgs = [...this.view.msgs, { id: this.view.msgs.length, who: 'me', text: st.typed }]; this.emit({}); }
        else this.later(TRANSCRIBE_MS, () => {
          this.view.msgs = [...this.view.msgs, { id: this.view.msgs.length, who: 'me', text: st.said }];
          this.emit({ sub: 'thinking', pendingMe: null });
        });
        if (this.run) this.later(st.ms, () => { this.next(); });
        break;
      }
      case 'hold':
        this.emit({ ...base, phase: 'hold', holdLabel: st.label, holdStart: performance.now() });
        advance(st.ms);
        break;
      case 'ended':
        this.emit({ ...base, phase: 'ended', endedAt: performance.now() });
        this.later(st.ms, () => this.emit({ closed: true })); // the real screen closes itself
        break;
    }
  }

  // Plays a line sentence by sentence; words light up as they are said.
  talk(st, base) {
    const msg = { id: this.view.msgs.length, who: 'claude', parts: st.parts, spoken: [0, 0], cut: false, live: true };
    this.view.msgs = [...this.view.msgs, msg];
    this.emit({ ...base, phase: 'talking', live: msg.id });
    const lastPart = st.cut ? st.cut.part : st.parts.length - 1;
    const sayPart = p => {
      this.setVoice(null);
      this.later(SENTENCE_GAP_MS, () => {
        this.setVoice('claude');
        const n = st.cut && p === lastPart ? st.cut.word : words(st.parts[p]).length;
        let w = 0;
        const tick = () => {
          w++;
          this.update(msg.id, { spoken: [p, w] });
          if (w < n) return this.later(WORD_MS, tick);
          this.later(WORD_MS, () => {
            if (p < lastPart) { this.update(msg.id, { spoken: [p + 1, 0] }); return sayPart(p + 1); }
            this.setVoice(null);
            if (st.cut) { this.update(msg.id, { cut: true, live: false }); return this.next(); } // you cut in
            this.update(msg.id, { spoken: [st.parts.length, 0], live: false });
            if (!this.run && !st.end) return this.holdTalking(msg.id, st); // a held preview keeps talking
            if (st.end) return this.next();
            this.next();
          });
        };
        tick();
      });
    };
    sayPart(0);
  }
  // Held "Claude talking": the last sentence stays lit and the voice keeps moving.
  holdTalking(id, st) {
    this.update(id, { spoken: [st.parts.length - 1, words(st.parts.at(-1)).length], live: true });
    this.setVoice('claude');
  }
  update(id, patch) {
    this.view.msgs = this.view.msgs.map(m => (m.id === id ? { ...m, ...patch } : m));
    this.emit({});
  }

  // ---- The real controls ----
  answer() { if (this.view.phase === 'incoming') { this.run = true; this.callStart = performance.now() + 1300; this.emit({ run: true, callStart: this.callStart }); this.next(); } }
  doneTalking() {
    if (this.view.phase !== 'hearing') return;
    this.run = true;
    this.clear();
    this.setVoice(null);
    this.next();
  }
  toggleMute() {
    this.muted = !this.muted;
    const p = this.view.phase;
    if (this.muted && p === 'hearing') { this.clear(); this.setVoice(null); this.emit({ phase: 'listening' }); return; }
    if (!this.muted && p === 'listening') return this.enter(); // listening again
    this.emit({});
  }
  toggleSpeaker() { this.speaker = !this.speaker; this.emit({}); }
  sendTyped(text) {
    text = text.trim();
    const p = this.view.phase;
    if (!text || p === 'incoming' || p === 'connecting' || p === 'ended') return;
    this.clear();
    this.setVoice(null);
    const cur = this.view.msgs.find(m => m.live);
    if (cur) this.update(cur.id, { live: false, cut: cur.spoken[0] < cur.parts.length });
    const reply = { s: 'talking', parts: ['Thanks, I have read that.', "I'll carry on from there."] };
    // A typed reply gets a short answer, then the call waits for you.
    this.steps = [...this.steps.slice(0, this.idx + 1), { s: 'thinking', typed: text, ms: 2600 }, reply, { s: 'listening' }];
    this.run = true;
    this.next();
  }
}

// What the status line says, matching page.html's copy.
export function statusText(v) {
  switch (v.phase) {
    case 'incoming': return 'Incoming call';
    case 'connecting': return 'Connecting…';
    case 'talking': return 'Claude is talking…';
    case 'listening': return v.muted ? 'Mic muted, type a reply' : 'Listening… take your time';
    case 'hearing': return 'Hearing you…';
    case 'thinking': return v.sub === 'transcribing' ? 'Got it, transcribing…' : 'Claude is thinking…';
    case 'hold': return 'On hold';
    case 'ended': return 'Call ended';
  }
  return '';
}

export const clock = ms => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};
export { words };
