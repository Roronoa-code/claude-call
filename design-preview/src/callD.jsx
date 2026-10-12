import { useContext, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { VoiceBeam } from 'voice-glow';
import { ThinkingOrb } from 'thinking-orbs';
import { clock, statusText } from './sim.js';
import { Icon, Morph, Spoken, StatusBar, Swap, callTime, useNow, useSim } from './ui.jsx';
import { Mascot, MascotKind } from './mascot.jsx';

// D. Your mix: A's controls, B's live captions while Claude talks (and on hold, and at the end),
// C's thread while you talk and Claude thinks. One stage that changes shape: the caption flies
// into its bubble when you start talking and grows back out of the thread when Claude answers.
const REDUCE = matchMedia('(prefers-reduced-motion: reduce)');
const LIQUID = 'cubic-bezier(0.16, 1, 0.3, 1)';
const ms = () => (REDUCE.matches ? 0 : document.documentElement.dataset.motion === 'gentle' ? 420 : 480);

function offsetIn(el, root) {
  let x = 0, y = 0;
  for (let n = el; n && n !== root; n = n.offsetParent) { x += n.offsetLeft; y += n.offsetTop; }
  return { x, y, w: el.offsetWidth, h: el.offsetHeight };
}

export default function CallD({ sim }) {
  const v = useSim(sim);
  const kind = useContext(MascotKind);
  const now = useNow(v.phase !== 'ended');
  const root = useRef(null), stage = useRef(null), capBtn = useRef(null), typeBtn = useRef(null), input = useRef(null);
  const capLayer = useRef(null), bubble = useRef(null), orbRow = useRef(null);
  const [caps, setCaps] = useState(false);
  const [typing, setTyping] = useState(false);
  const [draft, setDraft] = useState('');
  const incoming = v.phase === 'incoming', ended = v.phase === 'ended', inCall = !incoming;
  const thread = v.phase === 'hearing' || v.phase === 'thinking';
  const lastClaude = [...v.msgs].reverse().find(m => m.who === 'claude');
  const lastMe = [...v.msgs].reverse().find(m => m.who === 'me');
  const pending = v.phase === 'thinking' && v.sub === 'thinking';
  // The thread's own content is frozen when it hides, so it can fade out instead of vanishing.
  const held = useRef({ meText: null, live: false, pending: false, claude: null });
  if (thread || !held.current.claude) held.current = { meText: pending ? lastMe?.text : null, live: v.phase === 'hearing', pending, claude: lastClaude };
  const bubbleMsg = thread ? lastClaude : held.current.claude;
  const { meText, live } = held.current;
  const showPending = held.current.pending;

  // The big caption is frozen while the thread shows, so it can fly back from where it was.
  const capHold = useRef(null);
  if (!thread || !capHold.current) capHold.current = caption(v, now);
  const cap = capHold.current;

  // One status line above the controls, about the mic. Claude's own work has its orb elsewhere.
  const status = ['connecting', 'talking', 'listening', 'hearing'].includes(v.phase) || (v.phase === 'thinking' && !pending) ? statusText(v) : '';

  // ---- The morph between caption and thread ----
  const rects = useRef({});
  const [perch, setPerch] = useState(null);
  useLayoutEffect(() => { // remember where things are, for the next hand-over
    if (thread) {
      if (bubble.current) rects.current.bubble = offsetIn(bubble.current, stage.current);
      rects.current.orb = orbRow.current ? offsetIn(orbRow.current, stage.current) : null;
    } else {
      const t = capLayer.current?.querySelector('.swap-item.in .d-text');
      if (t) rects.current.cap = offsetIn(t, stage.current);
    }
  });
  const first = useRef(true);
  useLayoutEffect(() => {
    // New animations replace older ones on the same element, so nothing is cancelled mid-flight:
    // an outgoing caption stays hidden instead of flashing back for a frame.
    const d = ms(), fly = (el, kf) => el && d && el.animate(kf, { duration: d, easing: LIQUID, fill: 'both' });
    if (thread) {
      const b = offsetIn(bubble.current, stage.current);
      setPerch({ x: b.x + 14, y: b.y + 1 });
      const src = rects.current.cap, srcEl = capLayer.current.querySelector('.swap-item.in .d-text');
      if (src && !first.current) {
        const k = b.w / src.w;
        srcEl?.setAttribute('data-flown', ''); // stays hidden until it is the caption again
        fly(srcEl, [{ transform: 'none', opacity: 1 }, { opacity: 0, offset: 0.3 }, { transform: `translate(${b.x - src.x}px, ${b.y - src.y}px) scale(${k})`, opacity: 0 }]);
        fly(bubble.current, [{ transform: `translate(${src.x - b.x}px, ${src.y - b.y}px) scale(${1 / k})`, opacity: 0 }, { opacity: 0, offset: 0.18 }, { opacity: 1, offset: 0.45 }, { transform: 'none', opacity: 1 }]);
      } else bubble.current?.animate([{ opacity: 1 }, { opacity: 1 }], { duration: 1, fill: 'both' });
    } else {
      setPerch(null);
      bubble.current?.animate([{ opacity: 1 }, { opacity: 0 }], { duration: d * 0.35 || 1, easing: 'linear', fill: 'both' });
      const from = rects.current.orb || rects.current.bubble, isFirst = first.current;
      // The caption that flew into the bubble must not fade out again where it started.
      capLayer.current.classList.add('fresh');
      setTimeout(() => capLayer.current?.classList.remove('fresh'), 450);
      // Swap puts the new caption in on its own re-render, which React flushes before paint:
      // wait for it, so the new line (not the old one) grows out of the thread.
      queueMicrotask(() => {
        const dst = capLayer.current?.querySelector('.swap-item.in .d-text');
        dst?.removeAttribute('data-flown');
        if (!from || !dst || isFirst) return;
        const c = offsetIn(dst, stage.current), k = from.w / c.w;
        fly(dst, [{ transform: `translate(${from.x - c.x}px, ${from.y - c.y}px) scale(${k})`, opacity: 0 }, { opacity: 1, offset: 0.45 }, { transform: 'none', opacity: 1 }]);
      });
    }
    first.current = false;
  }, [thread]);

  const send = e => { e.preventDefault(); sim.sendTyped(draft); setDraft(''); setTyping(false); };
  const mascotAt = perch ? `translate(${perch.x}px, ${perch.y - 88}px)` : 'translate(24px, 20px)';

  return (
    <VoiceBeam type="mobile" level={sim.level} paused={ended}>
      <div className={`screen d ${v.closed ? 'closed' : ''} ${thread ? 'threaded' : ''}`} ref={root} data-phase={v.phase}>
        <StatusBar />
        <header className="d-head">
          <strong>Claude</strong>
          <span className="d-time"><Swap k={incoming ? 'i' : v.phase === 'connecting' ? 'c' : ended ? 'e' : 't'}>
            {incoming ? 'Calling you' : v.phase === 'connecting' ? 'Connecting…' : ended ? `Call ended · ${callTime(v, now)}` : `On call · ${callTime(v, now)}`}
          </Swap></span>
        </header>

        <div className="d-stage" ref={stage}>
          <div className="d-mascot" style={{ transform: mascotAt }}>
            <div className={`d-mascot-scale ${kind} ${perch ? 'small' : ''}`}><Mascot sim={sim} u={4} spark={88} /></div>
          </div>

          <div className={`d-caps ${thread ? '' : 'on'}`} ref={capLayer} aria-hidden={thread}>
            <Swap k={cap.key} className="d-cap">
              <span className={`d-label ${cap.tone || ''}`}>
                {cap.orb && <ThinkingOrb state={cap.orb} size={20} theme="dark" aria-hidden="true" />}
                {cap.label}
              </span>
              <span className={`d-text ${cap.tone || ''}`}>{cap.text}</span>
            </Swap>
          </div>

          <div className={`d-thread ${thread ? 'on' : ''}`} aria-hidden={!thread}>
            {bubbleMsg && <p ref={bubble} key={bubbleMsg.id} className="d-bubble claude flier"><Spoken msg={bubbleMsg} /></p>}
            {(thread || held.current.live || meText) && <MeBubble sim={sim} text={meText} live={live} />}
            {showPending && (
              <div ref={orbRow} className="d-pending">
                <ThinkingOrb state="breathing" size={20} theme="dark" aria-hidden="true" />
                <span>Claude is thinking…</span>
              </div>
            )}
          </div>
        </div>

        <p className="d-status" role="status"><Swap k={status || 'none'}>{status}</Swap></p>

        <Morph open={caps && inCall} from={capBtn} root={root} className="d-transcript" label="Transcript" radius={24}
          to={{ top: 0, right: 12, bottom: 106, left: 12 }} face={<Icon.captions />} onClose={() => setCaps(false)}>
          <div className="b-sheet-head"><strong>Transcript</strong>
            <button className="b-x" onClick={() => setCaps(false)} aria-label="Close transcript"><Icon.close /></button>
          </div>
          <div className="d-log">
            {v.msgs.map(m => (
              <div key={m.id} className={`c-row ${m.who}`}><p className="c-bubble">{m.who === 'claude' ? <Spoken msg={m} /> : m.text}</p></div>
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

function Round({ i, label, pressed, expanded, onClick, icon, hidden, btnRef, disabled }) {
  return (
    <div className="a-cell" style={{ '--i': i }}>
      <button ref={btnRef} className="a-round" aria-pressed={pressed} aria-expanded={expanded} onClick={onClick}
        tabIndex={hidden ? -1 : 0} disabled={disabled} aria-label={label}>{icon}</button>
      <span className="a-label" aria-hidden="true">{label}</span>
    </div>
  );
}

// Your bubble: three dots that move with your voice while you talk, then it reshapes around
// what you said once it is transcribed.
function MeBubble({ sim, text, live }) {
  const ref = useRef(null), dots = useRef(null), size = useRef(null);
  useLayoutEffect(() => {
    const el = ref.current, w = el.offsetWidth, h = el.offsetHeight, prev = size.current;
    size.current = { w, h };
    if (prev && (prev.w !== w || prev.h !== h) && ms()) el.animate([{ width: `${prev.w}px`, height: `${prev.h}px` }, { width: `${w}px`, height: `${h}px` }], { duration: ms() * 0.75, easing: LIQUID });
  }, [text]);
  useEffect(() => {
    if (text || REDUCE.matches) return;
    let raf, lv = 0;
    const tick = () => {
      lv += (sim.meLevel() - lv) * 0.25;
      [...(dots.current?.children || [])].forEach((d, i) => { d.style.transform = `translateY(${(-lv * 7 * (0.6 + 0.4 * Math.sin(performance.now() / 120 + i * 1.7))).toFixed(1)}px)`; });
      raf = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(raf);
  }, [text]);
  return (
    <p ref={ref} className={`d-bubble me ${text ? '' : 'dots'}`}>
      {text ? <span key="t" className="d-me-text">{text}</span>
        : <span ref={dots} className={`d-dots ${live ? '' : 'idle'}`} aria-label={live ? 'You are talking' : 'Transcribing'}><i /><i /><i /></span>}
    </p>
  );
}

// The big caption: B's captions for Claude talking, listening, on hold and the end of the call.
function caption(v, now) {
  const lastClaude = [...v.msgs].reverse().find(m => m.who === 'claude');
  const part = m => Math.min(m.spoken[0], m.parts.length - 1);
  switch (v.phase) {
    case 'incoming': return { key: 'inc', label: 'Calling now', text: 'Claude would like to talk.' };
    case 'connecting': return { key: 'con', label: 'Connecting', text: 'One moment…', tone: 'dim' };
    case 'talking': {
      const m = v.msgs.find(x => x.live) || lastClaude;
      return { key: `c${m.id}-${part(m)}`, label: 'Claude', text: <Spoken msg={m} part={part(m)} /> };
    }
    case 'hold':
      return { key: 'hold', orb: 'searching', label: `On hold · ${clock(now - v.holdStart)}`, text: `${v.holdLabel}.`, tone: 'hold' };
    case 'ended':
      return { key: 'end', label: callTime(v, now), text: 'Call ended.', tone: 'dim' };
    default: // listening (and the frozen caption under the thread)
      return lastClaude
        ? { key: `c${lastClaude.id}-${part(lastClaude)}`, label: 'Claude', text: <Spoken msg={lastClaude} part={part(lastClaude)} />, tone: 'rest' }
        : { key: 'turn', label: 'Your turn', text: 'Go ahead, Claude is listening.', tone: 'dim' };
  }
}
