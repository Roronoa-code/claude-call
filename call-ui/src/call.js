// The live call, as one "view" the screen subscribes to. All the call logic is the shared engine
// (call-engine.js, loaded before this page's script as window.CallEngine); this file only turns its
// events into the states the design shows: incoming, connecting, talking, listening, hearing,
// thinking, hold and ended.
import { words } from './text.js';

const qs = new URLSearchParams(location.search);
const CLOSE_MS = 1100; // the app closes the call screen 1.2 s after the call ends

// Claude's line, split into sentences: the big caption shows one sentence at a time.
const sentences = text => (text.match(/[^.!?]+(?:[.!?]+["')\]]*|$)/g) || [text]).map(s => s.trim()).filter(Boolean);
// How far into the line `shown` (the text revealed in step with the real audio) has got, as [sentence, words].
function spokenAt(parts, shown) {
  let n = words(shown || '').length;
  for (let i = 0; i < parts.length; i++) {
    const w = words(parts[i]).length;
    if (n < w) return [i, n];
    n -= w;
  }
  return [parts.length, 0];
}

export class CallView {
  constructor() {
    this.listeners = new Set();
    this.view = { phase: 'incoming', sub: null, msgs: [], muted: false, speaker: false, speakerAvailable: true, canFinish: false, callStart: 0, endedAt: 0, closed: false, cutIn: false, holdStart: 0, holdLabel: '', notice: '', partial: '' };
    this.voice = null; this.voiceSince = 0; this.micLevel = 0;
    this.last = {}; this.holdAfter = false;
    this.call = window.CallEngine({
      token: qs.get('t'),
      on: {
        state: s => { this.last = s; },
        status: s => this.status(s),
        claude: l => {
          const parts = sentences(l.text);
          this.holdAfter = false;
          this.setVoice('claude');
          this.emit({ msgs: [...this.view.msgs, { id: l.id, who: 'claude', parts, spoken: [0, 0], cut: false, live: true }], phase: 'talking', sub: null, cutIn: false, partial: '', notice: '' });
        },
        playing: l => {
          const m = this.view.msgs.find(x => x.id === l.id);
          if (!m) return;
          const parts = m.parts.join(' ') === l.text ? m.parts : sentences(l.text);
          const spoken = spokenAt(parts, l.shown);
          if (parts !== m.parts || spoken[0] !== m.spoken[0] || spoken[1] !== m.spoken[1]) this.update(l.id, { parts, spoken });
        },
        spoken: l => {
          const m = this.view.msgs.find(x => x.id === l.id);
          if (m) this.update(l.id, { live: false, cut: l.cut, spoken: l.cut ? m.spoken : [m.parts.length, 0] });
          if (this.voice === 'claude') this.setVoice(null);
          if (this.last.hold && !l.cut) { // a hold line: once it is said, the call waits on hold
            this.holdAfter = true;
            this.emit({ holdStart: performance.now(), holdLabel: (this.last.holdLabel || '').replace(/[.!?…\s]+$/, '') });
          }
        },
        partial: u => this.emit({ partial: u.text || '' }),
        heard: u => {
          if (u.text) this.emit({ msgs: [...this.view.msgs, { id: 'u' + u.id, who: 'me', text: u.text }], partial: '' });
          else this.emit({ partial: '' });
        },
        interrupted: () => this.emit({ cutIn: true }),
        level: x => { this.micLevel = x; },
        canFinish: on => this.emit({ canFinish: on }),
        muted: on => this.emit({ muted: on }),
        ended: () => {
          this.setVoice(null);
          this.emit({ phase: 'ended', endedAt: performance.now(), canFinish: false, notice: '' });
          setTimeout(() => this.emit({ closed: true }), CLOSE_MS);
        },
      },
    });
    this.view.speakerAvailable = this.call.hasSpeaker;
    if (qs.has('auto')) queueMicrotask(() => this.answer()); // opened from the app's own call screen
  }
  subscribe = fn => { this.listeners.add(fn); return () => this.listeners.delete(fn); };
  getSnapshot = () => this.view;
  emit(patch) { this.view = { ...this.view, ...patch }; this.listeners.forEach(fn => fn()); }
  update(id, patch) { this.emit({ msgs: this.view.msgs.map(m => (m.id === id ? { ...m, ...patch } : m)) }); }
  setVoice(v) { if (v !== this.voice) { this.voice = v; this.voiceSince = performance.now(); } }

  // The engine's status line, as the design's states. "Claude is talking…" is left to the claude
  // event, so the screen only changes once Claude's voice is actually heard.
  status(s) {
    if (this.view.phase === 'ended') return;
    if (/^Listening|^Mic muted/.test(s)) return this.emit({ phase: this.holdAfter ? 'hold' : 'listening', sub: null, notice: '' });
    if (/^Type your reply/.test(s)) return this.emit({ phase: this.holdAfter ? 'hold' : 'listening', notice: s });
    if (/^Hearing you/.test(s)) { this.setVoice('me'); this.holdAfter = false; return this.emit({ phase: 'hearing', sub: null, notice: '' }); }
    if (/^Got it/.test(s)) { this.setVoice(null); return this.emit({ phase: 'thinking', sub: 'transcribing', notice: '' }); }
    if (/^Claude is thinking/.test(s)) { this.setVoice(null); this.holdAfter = false; return this.emit({ phase: 'thinking', sub: 'thinking', notice: '' }); }
    if (/^Connect/.test(s)) return this.view.phase === 'incoming' || this.view.phase === 'connecting' ? this.emit({ phase: 'connecting' }) : undefined;
    if (/^Reconnecting|^Could not|^Mic blocked/.test(s)) return this.emit({ notice: s });
  }

  // Levels for the voice glow and the mascot, 0-1. Claude's voice plays natively or through the
  // engine's own audio graph, out of this page's reach: a speech-shaped envelope while it sounds.
  claudeLevel = () => {
    if (this.voice !== 'claude') return 0;
    const t = (performance.now() - this.voiceSince) / 1000;
    return Math.max(0, 0.6 * Math.abs(Math.sin(t * Math.PI * 4.6)) ** 1.6 * (0.55 + 0.45 * Math.sin(t * 1.7)));
  };
  meLevel = () => (this.voice === 'me' && !this.view.muted ? this.micLevel : 0);
  level = () => (this.voice === 'claude' ? this.claudeLevel() : this.meLevel());

  // ---- The controls ----
  answer() {
    if (this.view.phase !== 'incoming') return;
    this.emit({ phase: 'connecting', callStart: performance.now() });
    this.call.answer();
  }
  doneTalking() { this.call.finish(); }
  async toggleMute() { const muted = await this.call.setMuted(!this.view.muted); this.emit({ muted }); }
  toggleSpeaker() { if (this.call.hasSpeaker) this.emit({ speaker: this.call.setSpeaker(!this.view.speaker) }); }
  sendTyped(text) { this.call.sendTyped(text); }
}
