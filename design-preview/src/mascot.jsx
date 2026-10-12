import { createContext, useContext, useEffect, useRef } from 'react';

// Claude's mascot, in two looks, animated from the call's real state and the same level
// getters that drive the voice glow. Every pose is a target the mascot eases toward, so a
// change of state morphs instead of snapping. Reduced motion: a still pose per state.
export const MascotKind = createContext('clawd');
const REDUCE = matchMedia('(prefers-reduced-motion: reduce)');
export const ORANGE = '#D97757';
const INK = '#141416';

// ---- Pose: what the mascot is doing, as continuous numbers ----
function targets(v, t, te, lvC, lvM) {
  const p = { hop: 0, squash: 0, lean: 0, eyeDx: 0, eyeDy: 0, eyeOpen: 1, armL: 0, armR: 0, mouth: 0, reach: 0, tilt: 0, spin: 0, fade: 1, breath: 0.5 + 0.5 * Math.sin(t * Math.PI * 2 / 3.4) };
  const blink = (t % 3.9) < 0.13;
  switch (v.phase) {
    case 'incoming': { // a little hop on every ring
      const k = te % 1.5;
      if (k < 0.42) p.hop = Math.sin((Math.PI * k) / 0.42) * 2.6;
      else if (k < 0.58) p.squash = 1 - (k - 0.42) / 0.16;
      p.armR = k < 0.42 ? 1 : 0;
      break;
    }
    case 'talking': // bounces and squashes with Claude's voice; arms and mouth follow syllables
      p.squash = Math.min(1, lvC * 1.6);
      p.hop = lvC * 1.2;
      p.mouth = lvC;
      p.armL = lvC > 0.42 ? 1 : 0;
      p.armR = lvC > 0.55 ? 1 : 0;
      p.eyeOpen = Math.max(0.3, 1 - 2 * Math.max(0, lvC - 0.22)); // a happy squint on stressed syllables
      break;
    case 'listening': // attentive: leans toward you, eyes wide and down
      if (v.muted) break;
      p.eyeDy = 1; p.eyeOpen = 1.4; p.reach = 1; p.hop = -0.6;
      break;
    case 'hearing': { // nods along with your voice
      const nod = Math.sin(t * Math.PI * 2 * 1.5);
      p.eyeDy = 0.4 + nod * 0.6 * Math.min(1, lvM * 2); p.eyeOpen = 1.35; p.reach = 0.6;
      p.hop = -0.6 - lvM * 0.8 * (nod > 0 ? 1 : 0.3);
      break;
    }
    case 'thinking': // eyes up and to the side, a slow tilt, rays drifting round
      p.eyeDx = 1; p.eyeDy = -1; p.lean = 1; p.tilt = -7 + 3 * Math.sin(t * 0.9); p.spin = 10;
      break;
    case 'hold': // calm: looks slowly side to side while it waits
      p.eyeDx = Math.sin(t * 0.7) * 1.2; p.eyeOpen = 0.85; p.spin = 18; p.breath = 0.5 + 0.5 * Math.sin(t * Math.PI * 2 / 5);
      break;
    case 'ended': // a small wave, then it settles and fades
      p.armR = te < 1.1 ? (Math.floor(te * 6) % 2 ? 2 : 1) : 0;
      p.eyeOpen = 0.2;
      p.fade = te < 0.9 ? 1 : Math.max(0, 1 - (te - 0.9) / 0.6);
      break;
  }
  if (blink && p.eyeOpen > 0.5 && v.phase !== 'ended') p.eyeOpen = 0.1;
  return p;
}
// How quickly each value follows its target (per second): voice-driven ones are quick.
const RATE = { hop: 22, squash: 26, mouth: 30, armL: 30, armR: 30, eyeOpen: 30, eyeDx: 8, eyeDy: 10, lean: 6, reach: 7, tilt: 5, spin: 3, fade: 12, breath: 8 };

// ---- Clawd: Claude Code's pixel creature, drawn on its own pixel grid ----
// The welcome art is 18 x 5 terminal half-block pixels, each twice as tall as it is wide; this grid
// doubles it (36 x 10 cells, drawn 1 x 2) so eyes can blink and look one half-pixel at a time. Every rect sits on whole grid units and the SVG scales by a whole
// number of CSS px per unit, with crispEdges: never smoothed.
function clawdSvg(p) {
  const s = p.squash > 0.5 ? 1 : 0;
  const lean = Math.round(p.lean);
  const r = (x, y, w, h, c = ORANGE) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${c}"/>`;
  let out = '';
  out += r(6 - s + lean, s, 24 + 2 * s, 4 - s); // head rows
  out += r(6 - s, 4, 24 + 2 * s, 4); // body rows
  const arm = (x, a) => r(x, a >= 2 ? 0 : a >= 1 ? 2 : 4, 4, 2);
  out += arm(2 - s, Math.round(p.armL));
  out += arm(30 + s, Math.round(p.armR));
  const tuck = p.hop > 1.2;
  for (const x of [8, 12, 22, 26]) out += r(x, 8, 2, tuck ? 1 : 2);
  const h = p.eyeOpen < 0.35 ? 1 : p.eyeOpen > 1.25 ? 3 : 2;
  const dx = Math.max(-1, Math.min(1, Math.round(p.eyeDx))) + lean;
  const dy = Math.max(-1, Math.min(1, Math.round(p.eyeDy)));
  const top = Math.max(s, (h === 1 ? 3 : h === 3 ? 1 : 2) + dy);
  out += r(10 + dx, top, 2, h, INK) + r(24 + dx, top, 2, h, INK);
  return out;
}

// ---- Spark: the Claude mark as a living starburst ----
const RAYS = [1, 0.8, 0.94, 0.72, 0.98, 0.84, 0.76, 0.97, 0.8, 0.9, 0.74, 0.88];
function sparkInit(svg) {
  svg.innerHTML = `<g class="sp-all"><g class="sp-rays">${RAYS.map(() => `<rect x="8" y="-4.25" height="8.5" rx="4.25" fill="${ORANGE}"/>`).join('')}</g>
    <circle r="15.5" fill="${ORANGE}"/><g class="sp-face"><ellipse cx="-5.8" rx="2.6"/><ellipse cx="5.8" rx="2.6"/><ellipse class="sp-mouth" cy="7" rx="2.6"/></g></g>`;
  svg.querySelectorAll('ellipse').forEach(e => e.setAttribute('fill', '#3a160b'));
}
function sparkDraw(svg, p, spinAngle) {
  const all = svg.firstChild, rays = all.firstChild.children, face = svg.querySelector('.sp-face');
  const sq = p.squash * 0.07;
  all.setAttribute('transform', `translate(0 ${(-p.hop * 4 + p.reach * 2).toFixed(2)}) rotate(${p.tilt.toFixed(2)}) scale(${(1 + sq * 0.6).toFixed(3)} ${(1 - sq).toFixed(3)})`);
  for (let i = 0; i < RAYS.length; i++) {
    const a = ((i * 30 + spinAngle) * Math.PI) / 180;
    const down = Math.max(0, Math.sin(a)); // rays pointing at the bottom reach a little toward you
    const wave = 0.5 + 0.5 * Math.sin(i * 2.1 + performance.now() / 90);
    const len = 44 * RAYS[i] * (0.94 + 0.06 * p.breath + 0.22 * p.squash * wave + 0.1 * p.reach * down) * (0.75 + 0.25 * p.fade);
    // Each ray is a filled capsule, rotated into place.
    rays[i].setAttribute('width', (len - 8 + 4.25).toFixed(2));
    rays[i].setAttribute('transform', `rotate(${((a * 180) / Math.PI).toFixed(2)})`);
  }
  const [l, rr, m] = face.children;
  const ry = Math.max(0.6, 4 * Math.min(1.25, p.eyeOpen));
  for (const e of [l, rr]) { e.setAttribute('ry', ry.toFixed(2)); e.setAttribute('cy', (-1.5 + p.eyeDy * 2.2).toFixed(2)); }
  l.setAttribute('cx', (-5.8 + p.eyeDx * 1.8).toFixed(2)); rr.setAttribute('cx', (5.8 + p.eyeDx * 1.8).toFixed(2));
  m.setAttribute('ry', (p.mouth * 2.8).toFixed(2)); m.setAttribute('opacity', p.mouth > 0.06 ? 1 : 0);
  svg.style.opacity = p.fade.toFixed(3);
}

// u: CSS px per Clawd grid unit (a whole number); spark: the spark's size in px.
export function Mascot({ sim, u, spark, className = '' }) {
  const kind = useContext(MascotKind);
  const host = useRef(null), svgRef = useRef(null);
  useEffect(() => {
    const svg = svgRef.current;
    if (kind === 'spark') sparkInit(svg);
    const P = targets(sim.getSnapshot(), 0, 0, 0, 0);
    let last = '', raf = 0, prev = performance.now(), phase = null, entered = prev, spinAngle = 0, lvC = 0, lvM = 0;
    const draw = () => {
      const now = performance.now(); // not the rAF timestamp, which can run behind the last frame
      const v = sim.getSnapshot();
      if (v.phase !== phase) { phase = v.phase; entered = now; }
      const dt = Math.max(0, Math.min(0.05, (now - prev) / 1000)); prev = now;
      const still = REDUCE.matches;
      lvC += ((sim.voice === 'claude' ? sim.level() : 0) - lvC) * (1 - Math.exp(-dt * 18));
      lvM += (sim.meLevel() - lvM) * (1 - Math.exp(-dt * 18));
      const T = still ? targets(v, 0, 9, 0, 0) : targets(v, now / 1000, (now - entered) / 1000, lvC, lvM);
      if (still && v.phase === 'ended') T.fade = 1;
      for (const k in T) P[k] = still ? T[k] : P[k] + (T[k] - P[k]) * (1 - Math.exp(-dt * (RATE[k] || 10)));
      spinAngle = still ? 0 : (spinAngle + P.spin * dt) % 360;
      const lift = Math.round(Math.max(-2, P.hop) * (kind === 'clawd' ? u : 4)); // whole CSS px only
      host.current.style.transform = `translateY(${-lift}px)`;
      if (kind === 'clawd') {
        const s = clawdSvg(P);
        if (s !== last) { svg.innerHTML = s; last = s; }
        svg.style.opacity = P.fade.toFixed(3);
      } else sparkDraw(svg, P, spinAngle);
      if (!still) raf = requestAnimationFrame(draw);
    };
    draw();
    // Reduced motion: no loop, just the still pose for each state.
    const unsub = sim.subscribe(() => REDUCE.matches && draw());
    return () => { cancelAnimationFrame(raf); unsub(); };
  }, [kind, sim, u]);
  const clawd = kind === 'clawd';
  const w = clawd ? 36 * u : spark, h = clawd ? 20 * u : spark;
  return (
    <span className={`mascot ${kind} ${className}`} style={{ width: Math.max(36 * u, spark), height: Math.max(20 * u, spark) }} aria-hidden="true">
      <span ref={host} className="mascot-host" style={{ width: w, height: h }}>
        <svg ref={svgRef} width={w} height={h} viewBox={clawd ? '0 0 36 10' : '-50 -50 100 100'} preserveAspectRatio={clawd ? 'none' : undefined} shapeRendering={clawd ? 'crispEdges' : 'geometricPrecision'} />
      </span>
    </span>
  );
}
