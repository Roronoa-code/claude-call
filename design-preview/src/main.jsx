import { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { CallSim, STATE_LABELS, STATE_STEPS } from './sim.js';
import { useSim } from './ui.jsx';
import { MascotKind } from './mascot.jsx';
import CallA from './callA.jsx';
import CallB from './callB.jsx';
import CallC from './callC.jsx';
import CallD from './callD.jsx';
import { HomeScreen, MODES, WIDGETS, hhmm } from './widgets.jsx';

// Liquid's open spring (from Halimah Apps lib/motion-fluid.ts: damping 0.75, decay 5.4,
// about 2.8% overshoot), sampled into a CSS linear() curve.
(function springCurve() {
  const D = 0.75, DECAY = 5.4, W = DECAY / D, WD = W * Math.sqrt(1 - D * D);
  const pts = [];
  for (let i = 0; i <= 40; i++) {
    const t = i / 40;
    const x = 1 - Math.exp(-D * W * t) * (Math.cos(WD * t) + ((D * W) / WD) * Math.sin(WD * t));
    pts.push(x.toFixed(4));
  }
  pts[40] = '1';
  document.documentElement.style.setProperty('--spring-liquid', `linear(${pts.join(', ')})`);
})();

const DESIGNS = [
  { id: 'd', name: 'D. Your mix', C: CallD, blurb: 'A’s controls, B’s live captions while Claude talks, C’s thread while you talk and Claude thinks. The caption flies into its bubble when you start talking, and grows back out when Claude answers.' },
  { id: 'a', name: 'A. Phone call', C: CallA, blurb: 'A native in-call screen: big avatar, round controls, the conversation behind Captions. The four buttons grow out of Answer, which becomes Done talking.' },
  { id: 'b', name: 'B. Captions first', C: CallB, blurb: 'Claude’s words, large, lighting up as they are said. While you talk, Claude’s question stays up. The full transcript grows out of its chip.' },
  { id: 'c', name: 'C. Chat and call', C: CallC, blurb: 'The conversation as a thread, a thinking orb while Claude works, and a reply dock that glows with your voice.' },
];

function useScale() {
  const calc = () => {
    const w = innerWidth - 32, h = innerHeight - 180;
    const perRow = w >= 3 * 300 + 64 ? 3 : 1;
    const s = perRow === 3 ? Math.min(1, (w - 64) / 3 / 412, h / 892) : Math.min(1, w / 412);
    return Math.max(0.5, s);
  };
  const [s, setS] = useState(calc);
  useEffect(() => { const f = () => setS(calc()); addEventListener('resize', f); return () => removeEventListener('resize', f); }, []);
  return s;
}

function Frame({ s, children, label }) {
  return (
    <div className="frame" style={{ width: 412 * s, height: 892 * s }} aria-label={label}>
      <div className="phone" style={{ transform: `scale(${s})` }}>{children}</div>
    </div>
  );
}

function CallFrame({ d, sim, s }) {
  const v = useSim(sim);
  const Design = d.C;
  return (
    <section className="col">
      <h2>{d.name}</h2>
      <p className="blurb">{d.blurb}</p>
      <Frame s={s} label={`${d.name} call screen`}><Design sim={sim} /></Frame>
      <div className="chips" role="group" aria-label={`${d.name} state`}>
        {Object.keys(STATE_STEPS).map(k => (
          <button key={k} className="chip" aria-pressed={v.phase === k && !(k === 'listening' && v.muted)} onClick={() => sim.jump(STATE_STEPS[k], false)}>{STATE_LABELS[k]}</button>
        ))}
      </div>
    </section>
  );
}

function WidgetFrame({ w, s }) {
  const [mode, setMode] = useState('both');
  const [since, setSince] = useState('9:12');
  const set = m => { setMode(m); setSince(hhmm(new Date())); };
  useEffect(() => { (window.__widgetSetters ||= {})[w.id] = setMode; }, []);
  return (
    <section className="col">
      <h2>{w.name} <span className="sizes">{w.sizes}</span></h2>
      <p className="blurb">{w.blurb}</p>
      <Frame s={s} label={`${w.name} widget on a home screen`}><HomeScreen design={w} mode={mode} set={set} since={since} /></Frame>
      <div className="chips" role="group" aria-label={`${w.name} mode`}>
        {MODES.map(m => <button key={m.id} className="chip" aria-pressed={mode === m.id} onClick={() => set(m.id)}>{m.label}</button>)}
      </div>
      <details className="map">
        <summary>How it maps to a real Android widget</summary>
        <ul>{w.map.map(t => <li key={t}>{t}</li>)}</ul>
      </details>
    </section>
  );
}

function App() {
  const sims = useMemo(() => DESIGNS.map(() => new CallSim()), []);
  const s = useScale();
  const [motion, setMotion] = useState('liquid');
  const [mascot, setMascot] = useState('clawd');
  useEffect(() => { document.documentElement.dataset.motion = motion; }, [motion]);
  const play = () => sims.forEach(sim => sim.jump(0, true));
  useEffect(() => {
    window.__preview = { mascot: setMascot, state: k => sims.forEach(sim => sim.jump(STATE_STEPS[k], false)), play, sims, mode: m => Object.values(window.__widgetSetters).forEach(f => f(m)) };
  }, []);
  return (
    <MascotKind.Provider value={mascot}>
      <header className="top">
        <div>
          <h1>Claude Call: design preview</h1>
          <p>Three directions for the call screen and the home-screen widget. Nothing here changes the app.</p>
        </div>
        <div className="top-actions">
          <div className="seg" role="group" aria-label="Mascot">
            {[['clawd', 'Clawd'], ['spark', 'Claude spark']].map(([k, l]) => <button key={k} aria-pressed={mascot === k} onClick={() => setMascot(k)}>{l}</button>)}
          </div>
          <div className="seg" role="group" aria-label="Motion style">
            {['liquid', 'gentle'].map(m => <button key={m} aria-pressed={motion === m} onClick={() => setMotion(m)}>{m === 'liquid' ? 'Liquid' : 'Gentle'}</button>)}
          </div>
          <button className="primary" onClick={play}>Auto play the call</button>
        </div>
      </header>
      <h2 className="section">Call screen</h2>
      <p className="section-note">Tap Answer in any phone to play its call from there, or use the state buttons to hold one state. Typing a reply and Send work too.</p>
      <div className="row">{<CallFrame d={DESIGNS[0]} sim={sims[0]} s={s} />}</div>
      <h2 className="section">For reference: the three directions</h2>
      <div className="row">{DESIGNS.slice(1).map((d, i) => <CallFrame key={d.id} d={d} sim={sims[i + 1]} s={s} />)}</div>
      <h2 className="section">Home-screen widget</h2>
      <p className="section-note">Contact mode: how Claude and other agents may reach you. Tap a mode in the widget or below it. The change crossfades, as a real Android widget can.</p>
      <div className="row">{WIDGETS.map(w => <WidgetFrame key={w.id} w={w} s={s} />)}</div>
    </MascotKind.Provider>
  );
}

createRoot(document.getElementById('root')).render(<App />);
