import { useRef, useState } from 'react';
import { VoiceBeam } from 'voice-glow';
import { Mascot } from './mascot.jsx';
import { ThinkingOrb } from 'thinking-orbs';
import { clock, statusText } from './sim.js';
import { Icon, Morph, Spoken, StatusBar, Swap, callTime, useNow, useSim } from './ui.jsx';

// B. Captions first: what Claude is saying, large, word by word. A small avatar, three controls
// and a reply box. The glow follows whoever is talking.
export default function CallB({ sim }) {
  const v = useSim(sim);
  const now = useNow(v.phase !== 'ended');
  const root = useRef(null), tBtn = useRef(null);
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState('');
  const incoming = v.phase === 'incoming', ended = v.phase === 'ended';
  // While Claude works, the orb and its status sit up in the captions; this line, next to the glow, offers typing.
  const waiting = v.phase === 'hold' || (v.phase === 'thinking' && v.sub === 'thinking');
  const bottom = waiting ? 'Type while you wait' : statusText(v);
  const cap = caption(v, now);
  const send = e => { e.preventDefault(); sim.sendTyped(draft); setDraft(''); };

  return (
    <VoiceBeam type="mobile" level={sim.level} motion={sim.motion} paused={ended}>
      <div className={`screen b ${v.closed ? 'closed' : ''}`} ref={root} data-phase={v.phase}>
        <StatusBar />
        <header className="b-top">
          <div className="b-who">
            <span>
              <strong>Claude</strong>
              <span className="b-time"><Swap k={incoming ? 'i' : v.phase === 'connecting' ? 'c' : 't'}>{incoming ? 'Calling you' : v.phase === 'connecting' ? 'Connecting' : callTime(v, now)}</Swap></span>
            </span>
          </div>
          <button ref={tBtn} className="b-chip" aria-expanded={open} onClick={() => setOpen(o => !o)} disabled={incoming}>
            <Icon.transcript size={20} /> Transcript
          </button>
        </header>

        <main className="b-stage" aria-live="off">
          <Mascot sim={sim} u={4} spark={88} className="b-mascot" />
          <Swap k={cap.key} className="b-cap">
            <span className={`b-label ${cap.tone || ''}`}>
              {cap.orb && <ThinkingOrb state={cap.orb} size={20} theme="dark" aria-hidden="true" />}
              {cap.label}
            </span>
            <span className={`b-text ${cap.tone || ''}`}>{cap.text}</span>
          </Swap>
        </main>

        <Morph open={open && !incoming} from={tBtn} root={root} className="b-sheet" label="Transcript" radius={24} fromRadius={24}
          to={{ top: 60, right: 12, bottom: 0, left: 12 }} face={<span className="b-chip-face"><Icon.transcript size={20} /> Transcript</span>} onClose={() => setOpen(false)}>
          <div className="b-sheet-head"><strong>Transcript</strong>
            <button className="b-x" onClick={() => setOpen(false)} aria-label="Close transcript"><Icon.close /></button>
          </div>
          <div className="b-log">
            {v.msgs.map(m => (
              <p key={m.id} className={`b-line ${m.who}`}><span className="who">{m.who === 'claude' ? 'Claude' : 'You'}</span>{m.who === 'claude' ? <Spoken msg={m} /> : m.text}</p>
            ))}
          </div>
        </Morph>

        <footer className={`b-controls ${incoming ? 'incoming' : ''}`}>
          <p className="b-status" role="status"><Swap k={bottom}>{bottom}</Swap></p>
          <div className="b-row">
            <button className="b-round side l" aria-pressed={v.muted} onClick={() => sim.toggleMute()} aria-label={v.muted ? 'Unmute mic' : 'Mute mic'} disabled={incoming || ended} tabIndex={incoming ? -1 : 0}>
              {v.muted ? <Icon.micOff /> : <Icon.mic />}
            </button>
            <button className={`b-main ${incoming ? 'answer' : 'done'}`} disabled={!incoming && v.phase !== 'hearing'} onClick={() => (incoming ? sim.answer() : sim.doneTalking())}>
              <Swap k={incoming ? 'a' : 'd'}>{incoming ? <><Icon.phone /> Answer</> : <><Icon.check /> Done talking</>}</Swap>
            </button>
            <button className="b-round side r" aria-pressed={v.speaker} onClick={() => sim.toggleSpeaker()} aria-label="Speaker" disabled={incoming || ended} tabIndex={incoming ? -1 : 0}>
              <Icon.speaker />
            </button>
          </div>
          <form className="b-form" onSubmit={send} inert={incoming || ended}>
            <input value={draft} onChange={e => setDraft(e.target.value)} placeholder="Type a reply" aria-label="Type a reply" autoComplete="off" />
            <button className="b-send" disabled={!draft.trim()} aria-label="Send"><Icon.send /></button>
          </form>
        </footer>
      </div>
    </VoiceBeam>
  );
}

// What the big caption shows in each state.
function caption(v, now) {
  const lastClaude = [...v.msgs].reverse().find(m => m.who === 'claude');
  const lastMe = [...v.msgs].reverse().find(m => m.who === 'me');
  const said = m => {
    const p = Math.min(m.spoken[0], m.parts.length - 1);
    return <Spoken msg={m} part={p} />;
  };
  switch (v.phase) {
    case 'incoming': return { key: 'inc', label: 'Calling now', text: 'Claude would like to talk.' };
    case 'connecting': return { key: 'con', label: 'Connecting', text: 'One moment…', tone: 'dim' };
    case 'talking': {
      const m = v.msgs.find(x => x.live) || lastClaude;
      return { key: `c${m.id}-${Math.min(m.spoken[0], m.parts.length - 1)}`, label: 'Claude', text: said(m) };
    }
    case 'thinking':
      if (v.sub === 'thinking' && lastMe) return { key: `me${lastMe.id}`, orb: 'breathing', label: 'Claude is thinking…', text: lastMe.text, tone: 'me' };
    // falls through: while transcribing, Claude's last words stay up
    case 'listening':
    case 'hearing':
      return lastClaude
        ? { key: `c${lastClaude.id}-${Math.min(lastClaude.spoken[0], lastClaude.parts.length - 1)}`, label: 'Claude', text: said(lastClaude), tone: 'rest' }
        : { key: 'turn', label: 'Your turn', text: 'Go ahead, Claude is listening.', tone: 'dim' };
    case 'hold':
      return { key: 'hold', orb: 'searching', label: `On hold · ${clock(now - v.holdStart)}`, text: `${v.holdLabel}.`, tone: 'hold' };
    case 'ended':
      return { key: 'end', label: callTime(v, now), text: 'Call ended.', tone: 'dim' };
  }
  return { key: 'x', label: '', text: '' };
}
