import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import { clock, words } from './text.js';

export const useSim = sim => useSyncExternalStore(sim.subscribe, sim.getSnapshot);

// A clock that re-renders once a second, for call and hold timers.
export function useNow(on = true) {
  const [now, setNow] = useState(performance.now());
  useEffect(() => {
    if (!on) return;
    const t = setInterval(() => setNow(performance.now()), 250);
    return () => clearInterval(t);
  }, [on]);
  return now;
}

export function callTime(v, now) {
  if (v.phase === 'incoming' || v.phase === 'connecting') return '';
  return clock((v.phase === 'ended' ? v.endedAt : now) - v.callStart);
}

// ---- Icons (24px, 2px strokes) ----
const I = ({ children, size = 24 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{children}</svg>
);
export const Icon = {
  mic: p => <I {...p}><rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21" /></I>,
  micOff: p => <I {...p}><path d="M15 9.3V6a3 3 0 0 0-5.7-1.3M9 9v2a3 3 0 0 0 4.9 2.3M18.5 11a6.5 6.5 0 0 1-1.1 3.6M5.5 11a6.5 6.5 0 0 0 10.4 5.2M12 17.5V21M3 3l18 18" /></I>,
  speaker: p => <I {...p}><path d="M11 5 6.5 9H3v6h3.5l4.5 4z" /><path d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13" /></I>,
  captions: p => <I {...p}><rect x="3" y="5" width="18" height="14" rx="3" /><path d="M7 10h4M13 10h4M7 14h7M16 14h1" /></I>,
  keyboard: p => <I {...p}><rect x="2.5" y="6" width="19" height="12" rx="2.5" /><path d="M6.5 10h.01M10 10h.01M14 10h.01M17.5 10h.01M7.5 14h9" /></I>,
  check: p => <I {...p}><path d="M5 12.5 10 17.5 19 7" /></I>,
  phone: p => <I {...p}><path d="M5 4h3.2l1.6 4-2 1.3a10 10 0 0 0 4.9 4.9l1.3-2 4 1.6V17a2 2 0 0 1-2 2A15 15 0 0 1 3 6a2 2 0 0 1 2-2z" /></I>,
  send: p => <I {...p}><path d="M5 12h13M13 6l6 6-6 6" /></I>,
  close: p => <I {...p}><path d="M6 6l12 12M18 6 6 18" /></I>,
  transcript: p => <I {...p}><path d="M4 6h16M4 11h16M4 16h10" /></I>,
};

// A word-by-word rendering of one of Claude's lines. Every word is laid out from the start,
// so the line never reflows while it is spoken; words light up as they are said.
export function Spoken({ msg, part }) {
  const [p, w] = msg.spoken;
  const parts = part === undefined ? msg.parts.map((t, i) => [t, i]) : [[msg.parts[part], part]];
  return parts.map(([text, i]) => (
    <span key={i}>
      {words(text).map((word, j) => {
        const lit = i < p || (i === p && j < w);
        return <span key={j} className={lit ? 'w lit' : msg.cut ? 'w unsaid' : 'w'}>{word} </span>;
      })}
    </span>
  ));
}

// Text that changes crossfades in place: the old line rises out as the new one rises in.
export function Swap({ k, children, className = '' }) {
  const [items, setItems] = useState([{ k, node: children }]);
  const nodes = useRef(new Map());
  nodes.current.set(k, children);
  useLayoutEffect(() => {
    setItems(prev => (prev.at(-1).k === k ? prev : [...prev.filter(i => i.k !== k).map(i => ({ ...i, out: true, node: nodes.current.get(i.k) })), { k }]));
    const t = setTimeout(() => setItems(prev => prev.filter(i => !i.out)), 360);
    return () => clearTimeout(t);
  }, [k]);
  return (
    <span className={`swap ${className}`}>
      {items.map(i => (
        <span key={i.k} className={i.out ? 'swap-item out' : 'swap-item in'} aria-hidden={i.out || undefined}>
          {i.out ? i.node : nodes.current.get(i.k)}
        </span>
      ))}
    </span>
  );
}

function offsetIn(el, root) {
  let x = 0, y = 0;
  for (let n = el; n && n !== root; n = n.offsetParent) { x += n.offsetLeft; y += n.offsetTop; }
  return { x, y };
}

// A surface that grows out of the button that opened it and goes back into it ("morph"):
// it starts as exactly the button, the button's face fades as the contents fade in, it springs
// out with a small bounce, and closes back into the button without one. Turning it round
// mid-way retargets from where it is; nothing restarts.
export function Morph({ open, from, root, to, radius = 24, fromRadius, face, className = '', label, onClose, children }) {
  const ref = useRef(null);
  const faceRef = useRef(null);
  useLayoutEffect(() => {
    const el = ref.current, btn = from.current;
    if (!el || !btn) return;
    const a = offsetIn(btn, root.current), b = offsetIn(el, root.current);
    const W = el.offsetWidth, H = el.offsetHeight;
    const x = a.x - b.x, y = a.y - b.y, w = btn.offsetWidth, h = btn.offsetHeight;
    const r = fromRadius ?? Math.min(w, h) / 2;
    const shut = `inset(${y}px ${W - x - w}px ${H - y - h}px ${x}px round ${r}px)`;
    const full = `inset(${to.top}px ${to.right}px ${to.bottom}px ${to.left}px round ${radius}px)`;
    Object.assign(faceRef.current.style, { left: `${x}px`, top: `${y}px`, width: `${w}px`, height: `${h}px` });
    if (open) {
      if (el.dataset.state !== 'closing') { // from rest: start as the button
        el.style.transition = 'none';
        el.style.clipPath = shut;
        el.style.visibility = 'visible';
        void el.offsetWidth;
      }
      el.style.transition = '';
      el.dataset.state = 'open';
      el.style.clipPath = full;
    } else if (el.dataset.state === 'open') {
      el.dataset.state = 'closing';
      el.style.clipPath = shut;
    }
  }, [open]);
  useEffect(() => {
    if (!open || !onClose) return;
    const k = e => e.key === 'Escape' && onClose();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, [open]);
  const ended = e => {
    if (e.target === ref.current && e.propertyName === 'clip-path' && ref.current.dataset.state === 'closing') {
      ref.current.dataset.state = 'shut';
      ref.current.style.visibility = 'hidden';
    }
  };
  return (
    <div ref={ref} className={`morph ${className}`} data-state="shut" style={{ visibility: 'hidden' }} onTransitionEnd={ended}
      role="region" aria-label={label} aria-hidden={!open} inert={!open}>
      <div ref={faceRef} className="morph-face" aria-hidden="true">{face}</div>
      <div className="morph-body" style={{ inset: `${to.top}px ${to.right}px ${to.bottom}px ${to.left}px` }}>{children}</div>
    </div>
  );
}

