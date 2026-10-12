import { useState } from 'react';
import { StatusBar } from './ui.jsx';

// Home-screen widget for the contact mode: how agents may reach you.
// Stored values match the app (Prefs.mode) and server.mjs: call / text / both / none.
export const MODES = [
  { id: 'call', label: 'Calls', title: 'Calls only', line: 'Claude can call you', color: '#F2A27E' },
  { id: 'text', label: 'Texts', title: 'Texts only', line: 'Claude can message you', color: '#86BEFF' },
  { id: 'both', label: 'Both', title: 'Calls and texts', line: 'Claude can call or message you', color: '#BBA2FF' },
  { id: 'none', label: 'None', title: 'Do not disturb', line: 'Claude saves its questions', color: '#A9B2C4' },
];

// Glyphs as they would be drawn by 24dp vector drawables.
const G = ({ children, size = 22 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{children}</svg>
);
export const Glyph = {
  call: p => <G {...p}><path d="M5.5 4h3l1.5 3.8-1.9 1.3a9.5 9.5 0 0 0 4.8 4.8l1.3-1.9 3.8 1.5v3a2 2 0 0 1-2 2A14.5 14.5 0 0 1 3.5 6a2 2 0 0 1 2-2z" /></G>,
  text: p => <G {...p}><path d="M4.5 6.5a2.5 2.5 0 0 1 2.5-2.5h10a2.5 2.5 0 0 1 2.5 2.5v7a2.5 2.5 0 0 1-2.5 2.5H10l-4 3.5V16h0a2.5 2.5 0 0 1-1.5-2.3z" /></G>,
  both: p => <G {...p}><path d="M3.5 8.5h2.2l1.1 2.7-1.4.9a6.8 6.8 0 0 0 3.4 3.4l.9-1.4 2.7 1.1v2.2a1.5 1.5 0 0 1-1.5 1.5A10.3 10.3 0 0 1 2 10a1.5 1.5 0 0 1 1.5-1.5z" /><path d="M12.5 4h7a1.8 1.8 0 0 1 1.8 1.8v4.4A1.8 1.8 0 0 1 19.5 12H17l-2.5 2v-2h-2a1.8 1.8 0 0 1-1.8-1.8V5.8A1.8 1.8 0 0 1 12.5 4z" /></G>,
  none: p => <G {...p}><path d="M19.5 14.6A8 8 0 1 1 9.4 4.5a6.3 6.3 0 0 0 10.1 10.1z" /></G>,
};

const hhmm = d => `${d.getHours()}:${String(d.getMinutes()).padStart(2, '0')}`;

// Each widget is a static frame per mode, stacked and crossfaded: exactly what a ViewFlipper
// with fade in/out animations does when setDisplayedChild() changes the mode.
function Flip({ mode, children, className = '' }) {
  return (
    <div className={`flip ${className}`}>
      {MODES.map(m => (
        <div key={m.id} className={`flip-frame ${m.id === mode ? 'on' : ''}`} style={{ '--c': m.color }} aria-hidden={m.id !== mode} inert={m.id !== mode}>
          {children(m)}
        </div>
      ))}
    </div>
  );
}

// A tap target with the platform ripple (selectableItemBackground).
function Tap({ className = '', on, label, onClick, children, current }) {
  const [r, setR] = useState(null);
  return (
    <button className={`tap ${className} ${on ? 'on' : ''}`} aria-label={label} aria-pressed={current}
      onPointerDown={e => { const b = e.currentTarget.getBoundingClientRect(); setR({ x: e.clientX - b.left, y: e.clientY - b.top, k: Date.now(), w: b.width, h: b.height }); }}
      onClick={onClick}>
      {r && <span key={r.k} className="ripple" style={{ left: `${(r.x / r.w) * 100}%`, top: `${(r.y / r.h) * 100}%` }} aria-hidden="true" />}
      {children}
    </button>
  );
}

// ---- 1. Glass: a frosted list and a segmented track ----
function GlassCompact({ mode, set, since }) {
  return (
    <div className="wg glass compact" role="group" aria-label="Claude contact mode">
      <Flip mode={mode}>
        {cur => MODES.map(m => (
          <Tap key={m.id} className={`g-row ${m.id === cur.id ? 'sel' : ''}`} label={m.title} current={m.id === cur.id} onClick={() => set(m.id)}>
            <span className="g-ico" style={{ color: m.id === cur.id ? m.color : undefined }}>{Glyph[m.id]({ size: 20 })}</span>
            <span className="g-name">{m.label}</span>
            {m.id === cur.id && <span className="g-since">{since}</span>}
          </Tap>
        ))}
      </Flip>
    </div>
  );
}
function GlassWide({ mode, set }) {
  return (
    <div className="wg glass wide1" role="group" aria-label="Claude contact mode">
      <Flip mode={mode}>
        {cur => MODES.map(m => (
          <Tap key={m.id} className={`g-seg ${m.id === cur.id ? 'sel' : ''}`} label={m.title} current={m.id === cur.id} onClick={() => set(m.id)}>
            <span className="g-ico" style={{ color: m.id === cur.id ? m.color : undefined }}>{Glyph[m.id]({ size: 22 })}</span>
            <span className="g-name">{m.label}</span>
          </Tap>
        ))}
      </Flip>
    </div>
  );
}

// ---- 2. Aura: four tiles, the chosen one lit in its colour ----
function AuraCompact({ mode, set }) {
  return (
    <div className="wg aura compact" role="group" aria-label="Claude contact mode">
      <Flip mode={mode}>
        {cur => <div className="a-quad">{MODES.map(m => (
          <Tap key={m.id} className={`a-tile ${m.id === cur.id ? 'sel' : ''}`} label={m.title} current={m.id === cur.id} onClick={() => set(m.id)}>
            <span style={{ '--c': m.color }} className="a-tile-in">
              {Glyph[m.id]({ size: m.id === cur.id ? 26 : 22 })}
              <span className="a-tile-name">{m.label}</span>
            </span>
          </Tap>
        ))}</div>}
      </Flip>
    </div>
  );
}
function AuraWide({ mode, set, since }) {
  return (
    <div className="wg aura wide2" role="group" aria-label="Claude contact mode">
      <Flip mode={mode}>
        {cur => (
          <div className="a-split">
            <div className="a-hero">
              <span className="a-hero-ico">{Glyph[cur.id]({ size: 30 })}</span>
              <span className="a-hero-title">{cur.title}</span>
              <span className="a-hero-line">{cur.line}</span>
              <span className="a-hero-since">Changed {since}</span>
            </div>
            <div className="a-quad small">{MODES.map(m => (
              <Tap key={m.id} className={`a-tile ${m.id === cur.id ? 'sel' : ''}`} label={m.title} current={m.id === cur.id} onClick={() => set(m.id)}>
                <span style={{ '--c': m.color }} className="a-tile-in row">{Glyph[m.id]({ size: 20 })}<span className="a-tile-name">{m.label}</span></span>
              </Tap>
            ))}</div>
          </div>
        )}
      </Flip>
    </div>
  );
}

// ---- 3. Ink: type first, one accent ----
function InkCompact({ mode, set, since }) {
  return (
    <div className="wg ink compact" role="group" aria-label="Claude contact mode">
      <Flip mode={mode}>
        {cur => (
          <>
            <div className="i-head"><span className="i-kicker">Reach me by</span><span className="i-since">{since}</span></div>
            <div className="i-word">{cur.label}<span className="i-dot" /></div>
            <div className="i-grid">{MODES.map(m => (
              <Tap key={m.id} className={`i-opt ${m.id === cur.id ? 'sel' : ''}`} label={m.title} current={m.id === cur.id} onClick={() => set(m.id)}>{m.label}</Tap>
            ))}</div>
          </>
        )}
      </Flip>
    </div>
  );
}
function InkWide({ mode, set }) {
  return (
    <div className="wg ink wide1" role="group" aria-label="Claude contact mode">
      <Flip mode={mode}>
        {cur => (
          <div className="i-wide">
            <div className="i-left">
              <span className="i-kicker">Reach me by</span>
              <span className="i-word small">{cur.label}<span className="i-dot" /></span>
            </div>
            <div className="i-row">{MODES.map(m => (
              <Tap key={m.id} className={`i-glyph ${m.id === cur.id ? 'sel' : ''}`} label={m.title} current={m.id === cur.id} onClick={() => set(m.id)}>
                {Glyph[m.id]({ size: 22 })}
              </Tap>
            ))}</div>
          </div>
        )}
      </Flip>
    </div>
  );
}

export const WIDGETS = [
  {
    id: 'glass', name: '1. Glass', Compact: GlassCompact, Wide: GlassWide, sizes: '2×2 list · 4×1 track',
    blurb: 'A frosted pane. The compact one lists the four modes; the wide one is a segmented track. The chosen mode carries a soft pill in its colour.',
    map: [
      'Root FrameLayout with a static shape drawable: translucent vertical gradient, 1dp light hairline, 26dp corners. The frosting is painted, not a live blur.',
      'A ViewFlipper holds four prebuilt states (one per mode), with an outAnimation (fade out, 120ms) and an inAnimation (fade in, 240ms, startOffset 80ms, PathInterpolator 0.16, 1, 0.3, 1), so old text never overlaps new. setDisplayedChild() runs it when the launcher reapplies the update; if it re-inflates instead (after a resize or reboot) the new state just appears.',
      'Each option is a LinearLayout (ImageView vector + TextView) with setOnClickPendingIntent and the selectableItemBackground ripple. The pill is a shape drawable inside each state, so it crossfades to the new option; it does not slide.',
    ],
  },
  {
    id: 'aura', name: '2. Aura', Compact: AuraCompact, Wide: AuraWide, sizes: '2×2 tiles · 4×2 card',
    blurb: 'Four tiles; the chosen one fills with its colour and a soft glow. The wide card adds a large status with the live line and when it changed.',
    map: [
      'Tiles are FrameLayouts with shape drawables: the chosen tile uses a layer-list of a linear gradient and a radial gradient glow in the mode colour; the others are a flat translucent fill.',
      'Same ViewFlipper crossfade between four prebuilt states. The glyphs are vector drawables, tinted in each state’s layout XML, so nothing is tinted at runtime.',
      '"Changed 9:12" is a TextView set on each update. Size-mapped RemoteViews pick the 2×2 or 4×2 layout on API 31+; API 29–30 choose by the widget options width.',
    ],
  },
  {
    id: 'ink', name: '3. Ink', Compact: InkCompact, Wide: InkWide, sizes: '2×2 type · 4×1 strip',
    blurb: 'Type first, nearly black, with one accent. The mode is the headline; the choices are quiet words or glyphs under it.',
    map: [
      'A solid near-black shape drawable with a 1dp hairline and 26dp corners. Text is TextView with the system sans-serif-light and sans-serif-medium families; the accent dot is a small oval shape drawable.',
      'Same ViewFlipper crossfade. Options are TextViews or ImageViews with setOnClickPendingIntent, each at least 48dp tall, with the platform ripple.',
      'No custom fonts, no canvas, nothing drawn at runtime.',
    ],
  },
];

export function HomeScreen({ design, mode, set, since }) {
  const { Compact, Wide } = design;
  return (
    <div className={`home home-${design.id}`}>
      <StatusBar />
      <div className="home-grid">
        <div className="cell c2x2"><Compact mode={mode} set={set} since={since} /></div>
        <div className="cell apps2x2" aria-hidden="true">
          {['Phone', 'Messages', 'Camera', 'Gallery'].map((n, i) => <span key={n} className="app"><i className={`appi a${i}`} />{n}</span>)}
        </div>
        <div className={`cell ${design.Wide === AuraWide ? 'c4x2' : 'c4x1'}`}><Wide mode={mode} set={set} since={since} /></div>
      </div>
      <div className="home-dots" aria-hidden="true"><i className="on" /><i /><i /></div>
      <div className="dock" aria-hidden="true">{[0, 1, 2, 3].map(i => <i key={i} className={`appi d${i}`} />)}</div>
    </div>
  );
}
export { hhmm };
