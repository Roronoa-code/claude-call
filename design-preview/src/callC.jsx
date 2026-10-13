import { useLayoutEffect, useRef, useState } from 'react';
import { VoiceBeam } from 'voice-glow';
import { Mascot } from '../../call-ui/src/mascot.jsx';
import { ThinkingOrb } from 'thinking-orbs';
import { clock, statusText } from './sim.js';
import { Icon, Spoken, StatusBar, Swap, callTime, useNow, useSim } from './ui.jsx';

// C. Chat and call in one: the conversation as a thread, Claude's avatar on its turns,
// a thinking orb while Claude works, and a reply dock that glows with your voice.
export default function CallC({ sim }) {
  const v = useSim(sim);
  const now = useNow(v.phase !== 'ended');
  const thread = useRef(null);
  const [draft, setDraft] = useState('');
  const incoming = v.phase === 'incoming', ended = v.phase === 'ended';
  const pending = (v.phase === 'thinking' && v.sub === 'thinking') || v.phase === 'hold';
  // One status for Claude's work: the orb row in the thread. The dock line is about you.
  const dockStatus = incoming ? 'Answer to start talking' : ended ? 'Call ended' : pending ? 'Type while you wait' : statusText(v);
  const send = e => { e.preventDefault(); sim.sendTyped(draft); setDraft(''); };
  const time = callTime(v, now);

  useLayoutEffect(() => {
    const el = thread.current;
    el.scrollTo({ top: el.scrollHeight, behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth' });
  }, [v.msgs.length, pending, v.phase === 'talking' && v.msgs.at(-1)?.spoken[0]]);

  return (
    <div className={`screen c ${v.closed ? 'closed' : ''}`} data-phase={v.phase}>
      <StatusBar />
      <header className="c-head">
        <div className="c-title">
          <strong>Claude</strong>
          <span className="c-sub"><Swap k={incoming ? 'i' : v.phase === 'connecting' ? 'c' : ended ? 'e' : 't'}>
            {incoming ? 'Incoming call' : v.phase === 'connecting' ? 'Connecting…' : ended ? `Call ended · ${time}` : `On call · ${time}`}
          </Swap></span>
        </div>
        <button className="c-icon" aria-pressed={v.muted} onClick={() => sim.toggleMute()} aria-label={v.muted ? 'Unmute mic' : 'Mute mic'} disabled={incoming || ended}>
          {v.muted ? <Icon.micOff /> : <Icon.mic />}
        </button>
        <button className="c-icon" aria-pressed={v.speaker} onClick={() => sim.toggleSpeaker()} aria-label="Speaker" disabled={incoming || ended}>
          <Icon.speaker />
        </button>
      </header>

      <div className="c-thread" ref={thread}>
        <div className={`c-ring ${incoming ? 'show' : ''}`} aria-hidden={!incoming}>
          <Mascot sim={sim} u={5} spark={110} room={60} />
          <p>Claude is calling</p>
        </div>
        {v.msgs.map(m => m.who === 'claude' ? (
          <div key={m.id} className="c-row claude">
            <p className="c-bubble"><Spoken msg={m} /></p>
          </div>
        ) : (
          <div key={m.id} className="c-row me"><p className="c-bubble">{m.text}</p></div>
        ))}
        {pending && (
          <div className="c-row claude pending" key={`p${v.idx}`}>
            <span className="c-orb"><ThinkingOrb state={v.phase === 'hold' ? 'searching' : 'breathing'} size={20} theme="dark" aria-hidden="true" /></span>
            <p className="c-note">{v.phase === 'hold' ? <>{v.holdLabel}<span className="c-dim"> · {clock(now - v.holdStart)}</span></> : 'Claude is thinking…'}</p>
          </div>
        )}
      </div>

      <div className={`c-perch ${incoming ? '' : 'show'}`}>{!incoming && <Mascot sim={sim} u={3} spark={48} room={6} />}</div>
      <div className="c-dockwrap">
        <VoiceBeam level={sim.meLevel} paused={ended}>
          <div className={`c-dock ${incoming ? 'incoming' : ''}`}>
            <div className="c-statusrow">
              <span className="c-status" role="status"><span className={`c-dot ${v.phase} ${v.muted ? 'muted' : ''}`} aria-hidden="true" />
                <Swap k={dockStatus}>{dockStatus}</Swap>
              </span>
              <button className={`c-main ${incoming ? 'answer' : 'done'}`} disabled={!incoming && v.phase !== 'hearing'} onClick={() => (incoming ? sim.answer() : sim.doneTalking())}>
                <Swap k={incoming ? 'a' : 'd'}>{incoming ? <><Icon.phone size={20} /> Answer</> : <><Icon.check size={20} /> Done talking</>}</Swap>
              </button>
            </div>
            <form className="c-form" onSubmit={send} inert={incoming || ended}>
              <input value={draft} onChange={e => setDraft(e.target.value)} placeholder="Type a reply" aria-label="Type a reply" autoComplete="off" disabled={incoming || ended} />
              <button className="c-send" disabled={!draft.trim()} aria-label="Send"><Icon.send size={20} /></button>
            </form>
          </div>
        </VoiceBeam>
      </div>
    </div>
  );
}
