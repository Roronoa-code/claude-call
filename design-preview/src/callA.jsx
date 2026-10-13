import { useRef, useState } from 'react';
import { VoiceBeam } from 'voice-glow';
import { ThinkingOrb } from 'thinking-orbs';
import { Mascot } from '../../call-ui/src/mascot.jsx';
import { clock, statusText } from './sim.js';
import { Icon, Morph, Spoken, StatusBar, Swap, callTime, useNow, useSim } from './ui.jsx';

// A. Phone call: a native in-call screen. Big avatar, name and status; round controls in a grid;
// the conversation sits behind "Captions"; Claude's and your voice glow from the bottom edge.
export default function CallA({ sim }) {
  const v = useSim(sim);
  const now = useNow(v.phase !== 'ended');
  const root = useRef(null), capBtn = useRef(null), typeBtn = useRef(null), input = useRef(null);
  const [caps, setCaps] = useState(false);
  const [typing, setTyping] = useState(false);
  const [draft, setDraft] = useState('');
  const incoming = v.phase === 'incoming', ended = v.phase === 'ended';
  const inCall = !incoming;
  // An orb only for waits of 2 s or more: thinking (not the quick transcribe) and hold.
  const orb = v.phase === 'hold' ? 'searching' : v.phase === 'thinking' && v.sub === 'thinking' ? 'breathing' : null;
  const time = callTime(v, now);
  const sub = v.phase === 'hold' ? `${v.holdLabel} · ${clock(now - v.holdStart)}` : time;
  const send = e => { e.preventDefault(); sim.sendTyped(draft); setDraft(''); setTyping(false); };
  const recent = v.msgs.slice(-6);

  return (
    <VoiceBeam type="mobile" level={sim.level} motion={sim.motion} paused={ended}>
      <div className={`screen a ${v.closed ? 'closed' : ''}`} ref={root} data-phase={v.phase}>
        <StatusBar />
        <header className="a-head">
          <div className="a-avatar">
            <Mascot sim={sim} u={5} spark={110} room={40} />
          </div>
          <h2 className="a-name">Claude</h2>
          <p className="a-status" role="status"><Swap k={statusText(v)}>{orb && <span className="a-orb"><ThinkingOrb state={orb} size={20} theme="dark" aria-hidden="true" /></span>}{statusText(v)}</Swap></p>
          <p className="a-sub"><Swap k={v.phase === 'hold' ? 'hold' : inCall ? 'time' : 'none'}>{sub}</Swap></p>
        </header>

        <Morph open={caps && inCall} from={capBtn} root={root} className="a-caps" label="Captions" radius={24}
          to={{ top: 0, right: 16, bottom: 106, left: 16 }} face={<Icon.captions />} onClose={() => setCaps(false)}>
          <div className="a-caps-list">
            {recent.length === 0 && <p className="a-caps-empty">Captions will appear here as you talk.</p>}
            {recent.map(m => (
              <p key={m.id} className={`a-line ${m.who}`}>
                <span className="who">{m.who === 'claude' ? 'Claude' : 'You'}</span>
                {m.who === 'claude' ? <Spoken msg={m} /> : m.text}
              </p>
            ))}
          </div>
        </Morph>

        <Morph open={typing && inCall && !ended} from={typeBtn} root={root} className="a-type" label="Type a reply" radius={32}
          to={{ top: 0, right: 16, bottom: 26, left: 16 }} face={<Icon.keyboard />} onClose={() => setTyping(false)}>
          <form className="a-form" onSubmit={send}>
            <input ref={input} value={draft} onChange={e => setDraft(e.target.value)} placeholder="Type a reply" aria-label="Type a reply" autoComplete="off" />
            <button className="a-send" disabled={!draft.trim()} aria-label="Send"><Icon.send /></button>
            <button type="button" className="a-x" onClick={() => setTyping(false)} aria-label="Close"><Icon.close /></button>
          </form>
        </Morph>

        <div className={`a-controls ${inCall ? 'on' : ''} ${typing && inCall ? 'typing' : ''}`}>
          <div className="a-grid">
            <Round i={0} label={v.muted ? 'Unmute' : 'Mute'} pressed={v.muted} onClick={() => sim.toggleMute()} icon={v.muted ? <Icon.micOff /> : <Icon.mic />} hidden={!inCall} disabled={ended} />
            <Round i={1} label="Speaker" pressed={v.speaker} onClick={() => sim.toggleSpeaker()} icon={<Icon.speaker />} hidden={!inCall} disabled={ended} />
            <Round i={2} label="Captions" pressed={caps} expanded={caps} onClick={() => setCaps(c => !c)} icon={<Icon.captions />} hidden={!inCall} btnRef={capBtn} disabled={ended} />
            <Round i={3} label="Type" pressed={typing} expanded={typing} onClick={() => { setTyping(t => !t); setTimeout(() => input.current?.focus({ preventScroll: true }), 200); }} icon={<Icon.keyboard />} hidden={!inCall} btnRef={typeBtn} disabled={ended} />
          </div>
          <div className="a-main">
            <button className={`a-big ${incoming ? 'answer' : 'done'}`} disabled={!incoming && v.phase !== 'hearing'}
              onClick={() => (incoming ? sim.answer() : sim.doneTalking())} aria-label={incoming ? 'Answer call' : 'Done talking'}>
              <span className={`a-glyph ${incoming ? 'show' : ''}`}><Icon.phone size={30} /></span>
              <span className={`a-glyph ${incoming ? '' : 'show'}`}><Icon.check size={30} /></span>
            </button>
            <span className="a-biglabel" aria-hidden="true"><Swap k={incoming ? 'a' : 'd'}>{incoming ? 'Answer' : 'Done talking'}</Swap></span>
          </div>
        </div>
      </div>
    </VoiceBeam>
  );
}

// The four round buttons grow out of the Answer button when the call connects.
function Round({ i, label, pressed, expanded, onClick, icon, hidden, btnRef, disabled }) {
  return (
    <div className="a-cell" style={{ '--i': i }}>
      <button ref={btnRef} className="a-round" aria-pressed={pressed} aria-expanded={expanded} onClick={onClick}
        tabIndex={hidden ? -1 : 0} disabled={disabled} aria-label={label}>{icon}</button>
      <span className="a-label" aria-hidden="true">{label}</span>
    </div>
  );
}
