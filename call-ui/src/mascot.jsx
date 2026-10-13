import { createContext, useContext, useEffect, useRef } from 'react';
import { ANTICS, IDLE, anticDeltas, clawdParticles, makeConfetti, sparkParticles } from './antics.js';

// Claude's mascot, in two looks and two personalities, animated from the call's real state and
// the same level getters that drive the voice glow. The base pose eases toward each state's
// target, antics play over it and always ease back, so nothing snaps. Reduced motion: no antics
// and no particles, just a still pose per state.
export const MascotKind = createContext('clawd');
// playful, wild, or mixed (the approved one): playful day to day, with wild's big moments.
export const Personality = createContext('mixed');
const BIG = new Set(['cartwheel', 'confetti']); // the moments that play at wild size in mixed
const REDUCE = matchMedia('(prefers-reduced-motion: reduce)');
export const ORANGE = '#D97757';
const INK = '#141416';
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));

// For the preview's screenshot script: lets it start an antic on every mascot.
const live = new Set();
window.__mascotAntic = name => live.forEach(m => m(name));

// ---- Base pose for each call state ----
function targets(v, t, te, lvC, lvM, pers) {
  const wild = pers === 'wild', dance = pers !== 'playful';
  const W = wild ? 1.5 : 1;
  const p = { hop: 0, squash: 0, lean: 0, eyeDx: 0, eyeDy: 0, eyeOpen: 1, armL: 0, armR: 0, mouth: 0, reach: 0, tilt: 0, spin: 0, fade: 1, breath: 0.5 + 0.5 * Math.sin(t * Math.PI * 2 / 3.4), legs: null, thought: false, sleepy: false };
  const blink = (t % 3.9) < 0.13;
  switch (v.phase) {
    case 'incoming': { // a squashy hop on every ring
      const k = (te % 1.5) / 0.8;
      if (k < 1) {
        const c = 0.16, l = 0.76;
        if (k < c) p.squash = 1.5 * (k / c);
        else if (k < l) { const u = (k - c) / (l - c), air = 4 * u * (1 - u); p.hop = air * 2.6 * W; p.squash = -1.2 * (1 - air); }
        else p.squash = 1.5 * Math.sin(Math.PI * (k - l) / (1 - l));
      }
      p.armR = k < 0.8 ? 1 : 0;
      break;
    }
    case 'talking':
      if (dance) { // a dance in time with Claude's voice: stomps (Clawd), a pinwheel (spark)
        const beat = Math.floor(t * 5) % 2;
        p.hop = lvC * 3 * (beat ? 1 : 0.35);
        p.squash = beat ? clamp(2.4 * lvC, 0, 2) : -lvC * 1.2;
        p.legs = lvC > 0.18 ? (beat ? [1, 0, 1, 0] : [0, 1, 0, 1]) : null;
        p.armL = lvC > 0.3 && beat ? 2 : lvC > 0.2 ? 1 : 0; p.armR = lvC > 0.3 && !beat ? 2 : lvC > 0.2 ? 1 : 0;
        p.spin = 50 + lvC * 1000;
      } else { // bounces with every stressed syllable, squash and stretch
        p.squash = clamp(2.4 * (lvC - 0.22), -0.8, 2);
        p.hop = lvC * 1.8;
        p.armL = lvC > 0.42 ? 1 : 0; p.armR = lvC > 0.55 ? 1 : 0;
        p.spin = 6;
      }
      p.mouth = lvC;
      p.eyeOpen = Math.max(0.3, 1 - 2 * Math.max(0, lvC - 0.22));
      break;
    case 'listening': // attentive: leans toward you, eyes wide and down
      if (v.muted) break;
      p.eyeDy = 1; p.eyeOpen = 1.4; p.reach = 1; p.hop = -0.6;
      break;
    case 'hearing': { // leans in and nods; its eyes follow your voice
      const nod = Math.sin(t * Math.PI * 2 * 1.6) > 0 ? 1 : 0;
      p.eyeDy = 1 - Math.min(1, lvM * 2.5) * 2; p.eyeOpen = 1.4; p.reach = 1; p.lean = 1;
      p.squash = nod * Math.min(1, lvM * 3) * W; p.hop = -0.6;
      p.tilt = 6 * W * nod * Math.min(1, lvM * 3);
      break;
    }
    case 'thinking': // eyes up to the side, a slow tilt, thought dots, a tapping foot
      p.eyeDx = 1; p.eyeDy = -1; p.lean = 0.55 + 0.5 * Math.sin(t * 0.9); p.tilt = -7 + 3 * Math.sin(t * 0.9);
      p.spin = wild ? 40 : 10; p.thought = true;
      p.legs = [0, 0, 0, Math.floor(t * (wild ? 6 : 4)) % 2];
      break;
    case 'hold': { // sleepy: drooping eyes, slow breathing, z's
      const peek = (t % 5) < 0.5;
      p.eyeOpen = peek ? 0.9 : 0.2; p.eyeDy = 0.5; p.spin = 6; p.sleepy = true;
      p.breath = 0.5 + 0.5 * Math.sin(t * Math.PI * 2 / 4);
      p.squash = p.breath * W;
      break;
    }
    case 'ended':
      p.eyeOpen = 0.2;
      p.fade = te < 0.75 ? 1 : Math.max(0, 1 - (te - 0.75) / 0.4); // gone before the app closes the screen at 1.2 s
      break;
  }
  if (blink && p.eyeOpen > 0.5 && v.phase !== 'ended') p.eyeOpen = 0.1;
  return p;
}
// How quickly each value follows its target (per second): voice-driven ones are quick.
const RATE = { hop: 24, squash: 28, mouth: 30, armL: 30, armR: 30, eyeOpen: 30, eyeDx: 8, eyeDy: 12, lean: 6, reach: 7, tilt: 6, spin: 6, fade: 12, breath: 8 };
const SMOOTH = Object.keys(RATE);

// ---- Clawd: Claude Code's pixel creature, on its own pixel grid ----
// The welcome art is 18 x 5 terminal half-block pixels, each twice as tall as it is wide; this
// grid doubles it (36 x 10 cells, drawn 1 x 2). Every rect sits on whole cells and the SVG
// scales by a whole number of CSS px per cell, with crispEdges. Turns and flips are drawn as
// pixel-art frames (narrower bodies, an upside-down frame), never as a blurred rotation.
function clawdSvg(p, d, parts) {
  // Upside down at the top of a flip: plain Clawd mirrored row for row (legs up, eyes low).
  const flipped = d.flip > 0.3 && d.flip < 0.7;
  const s = flipped ? 0 : clamp(Math.round(p.squash + d.squash), -2, 2);
  const turn = d.turn % 1, f0 = Math.abs(Math.cos(2 * Math.PI * turn)), side = Math.sin(2 * Math.PI * turn);
  // Turning: full, three-quarter and side-on frames (no bare edge-on sliver).
  const f = f0 > 0.85 ? 1 : f0 > 0.5 ? 0.66 : 0.36;
  const dx = Math.round(d.dx), lean = f < 1 ? 0 : Math.round(clamp(p.lean + d.lean, 0, 1));
  const half = Math.max(2, Math.round((12 + s) * f)), X = 18 - half + dx, Wd = 2 * half;
  const rects = [];
  const r = (x, y, w, h, c = ORANGE, o) => rects.push({ x, y, w, h, c, o });
  r(X + lean, s, Wd, 4 - s); // head rows (a squash lowers the top, a stretch raises it)
  r(X, 4, Wd, 4); // body rows
  // Arms stay joined to the body: raised arms sit level with the top of the head, wherever that is.
  const lvl = a => (a >= 2 ? s - 1 : a >= 1 ? Math.max(2, s) : 4);
  const aL = flipped ? 0 : Math.max(Math.round(p.armL), d.arms), aR = flipped ? 0 : Math.max(Math.round(p.armR), d.arms, d.armWave);
  if (f > 0.5) { r(X - 4, lvl(aL), 4, 2); r(X + Wd, lvl(aR), 4, 2); }
  const legs = d.legs || p.legs || [0, 0, 0, 0], tuck = p.hop + d.hop > 1.4;
  const legX = f > 0.5 ? [-10, -6, 4, 8].map(o => 18 + dx + Math.round(o * f)) : [16 + dx, 18 + dx];
  legX.forEach((x, i) => r(x, 8, 2, tuck || legs[i] ? 1 : 2));
  // Eyes: happy (closed), popped (wide), or open as far as eyeOpen says, looking around.
  const ew = d.pop > 0.5 ? 3 : 2;
  const h = d.happy ? 1 : d.pop > 0.5 ? 3 : p.eyeOpen < 0.35 ? 1 : p.eyeOpen > 1.25 ? 3 : 2;
  const look = clamp(Math.round(p.eyeDx + d.eyeDx), -1, 1) + lean, ey = clamp(Math.round(p.eyeDy + d.eyeDy), -1, 1);
  // Eyes always keep a row of head above them, so they never notch the top edge.
  const top = Math.max(s + 1, (d.happy ? 2 : h === 1 ? 3 : h === 3 ? 1 : 2) + (d.happy ? 0 : ey));
  const eyes = f > 0.5 ? [18 + dx - Math.round(7 * f) - 1, 18 + dx + Math.round(7 * f) - 1] : f > 0.2 ? [18 + dx + (side > 0 ? 2 : -4)] : [];
  eyes.forEach(x => r(x + look - (ew - 2), top, ew, h, INK));
  let out = '';
  // Particles first, so they pass behind the body rather than over its face.
  for (const q of parts) out += `<rect x="${q.x}" y="${q.y}" width="${q.w}" height="${q.h}" fill="${q.c}"${q.o !== undefined && q.o < 1 ? ` fill-opacity="${Math.max(0, q.o).toFixed(2)}"` : ''}/>`;
  for (const q of rects) out += `<rect x="${q.x}" y="${flipped ? 10 - q.y - q.h : q.y}" width="${q.w}" height="${q.h}" fill="${q.c}"/>`;
  return out;
}

// ---- Spark: the Claude mark as a living starburst ----
const RAYS = [1, 0.8, 0.94, 0.72, 0.98, 0.84, 0.76, 0.97, 0.8, 0.9, 0.74, 0.88];
const LEG_RAYS = [4, 3, 2, 1]; // the lower rays stand in for legs
function sparkInit(svg) {
  svg.innerHTML = `<g class="sp-parts"></g><g class="sp-all"><g class="sp-rays">${RAYS.map(() => `<rect x="8" y="-4.25" height="8.5" rx="4.25" fill="${ORANGE}"/>`).join('')}</g>
    <circle r="15.5" fill="${ORANGE}"/><g class="sp-face"><ellipse cx="-5.8" rx="2.6"/><ellipse cx="5.8" rx="2.6"/><ellipse class="sp-mouth" cy="7" rx="2.6"/></g></g>`;
  svg.querySelectorAll('ellipse').forEach(e => e.setAttribute('fill', '#3a160b'));
}
function sparkDraw(svg, p, d, spinAngle, parts, t) {
  const all = svg.lastChild, rays = all.firstChild.children, face = svg.querySelector('.sp-face');
  const sq = (p.squash + d.squash) * 0.09;
  const turn = Math.cos(2 * Math.PI * (d.turn % 1)), sx = (Math.abs(turn) < 0.3 ? 0.3 * Math.sign(turn || 1) : turn); // never a bare line side-on
  all.setAttribute('transform', `translate(${(d.dx * 6).toFixed(2)} ${(p.reach * 2).toFixed(2)}) rotate(${(p.tilt + d.flip * 360).toFixed(2)}) scale(${(sx * (1 + sq * 0.6)).toFixed(3)} ${(1 - sq).toFixed(3)})`);
  const legs = d.legs || p.legs || [0, 0, 0, 0];
  for (let i = 0; i < RAYS.length; i++) {
    let deg = i * 30 + spinAngle + (d.wig ? d.wig * 14 * Math.sin(t * 28 + i * 1.9) : 0);
    if (i === 10 && d.armWave) deg += d.armWave === 2 ? -28 : 14; // the upper-right ray waves
    if (i === 10 && d.arms) deg -= 18;
    const a = (deg * Math.PI) / 180, down = Math.max(0, Math.sin(a));
    const wave = 0.5 + 0.5 * Math.sin(i * 2.1 + t * 11);
    const li = LEG_RAYS.indexOf(i), tap = li >= 0 && legs[li] ? 0.78 : 1;
    const len = 44 * RAYS[i] * tap * (0.94 + 0.06 * p.breath + 0.12 * Math.abs(p.squash + d.squash) * wave + 0.1 * p.reach * down) * (0.75 + 0.25 * p.fade);
    rays[i].setAttribute('width', (len - 8 + 4.25).toFixed(2));
    rays[i].setAttribute('transform', `rotate(${deg.toFixed(2)})`);
  }
  const [l, rr, m] = face.children;
  const pop = 1 + 0.45 * d.pop;
  const ry = d.happy ? 0.9 : Math.max(0.6, 4 * Math.min(1.25, p.eyeOpen)) * pop;
  const cy = d.happy ? -2.5 : -1.5 + clamp(p.eyeDy + d.eyeDy, -1.4, 1.4) * 2.2;
  for (const e of [l, rr]) { e.setAttribute('ry', ry.toFixed(2)); e.setAttribute('rx', (2.6 * pop).toFixed(2)); e.setAttribute('cy', cy.toFixed(2)); }
  const ex = clamp(p.eyeDx + d.eyeDx, -1.5, 1.5) * 1.8;
  l.setAttribute('cx', (-5.8 + ex).toFixed(2)); rr.setAttribute('cx', (5.8 + ex).toFixed(2));
  m.setAttribute('ry', (Math.max(p.mouth, d.pop * 0.7) * 2.8).toFixed(2)); m.setAttribute('opacity', p.mouth > 0.06 || d.pop > 0.3 ? 1 : 0);
  svg.firstChild.innerHTML = parts;
  svg.style.opacity = p.fade.toFixed(3);
}

// u: CSS px per Clawd grid cell (a whole number); spark: the spark's size in px.
// room: how far (in its own px) it may rise before reaching text above it; hops scale to fit.
export function Mascot({ sim, u, spark, room = 60, className = '' }) {
  const kind = useContext(MascotKind);
  const pers = useContext(Personality);
  const host = useRef(null), svgRef = useRef(null), poke = useRef(null);
  useEffect(() => {
    const svg = svgRef.current, clawd = kind === 'clawd', wild = pers === 'wild', big = pers !== 'playful';
    if (!clawd) sparkInit(svg);
    const P = targets(sim.getSnapshot(), 0, 0, 0, 0, pers);
    let last = '', raf = 0, prev = performance.now(), phase = sim.getSnapshot().phase, entered = prev, spinAngle = 0, lvC = 0, lvM = 0;
    let antics = [], confetti = [], nextIdle = prev + 2500, lastIdle = '';
    const start = name => {
      if (REDUCE.matches) return;
      const now = performance.now();
      antics.forEach(a => { a.cutAt ??= now; });
      antics.push({ name, start: now, dur: ANTICS[name].dur, scale: wild || (big && BIG.has(name)) ? 1.5 : 1 });
      if (ANTICS[name].burst) confetti = makeConfetti(clawd, now);
    };
    poke.current = () => start(wild ? 'cartwheel' : 'giggle');
    live.add(start);
    const draw = () => {
      const now = performance.now(); // not the rAF timestamp, which can run behind the last frame
      const v = sim.getSnapshot(), still = REDUCE.matches;
      if (v.phase !== phase) { // state changes: cut what is playing, celebrate what deserves it
        const was = phase; phase = v.phase; entered = now;
        antics.forEach(a => { a.cutAt ??= now; });
        if (was === 'incoming' && phase !== 'ended') start(big ? 'cartwheel' : 'joy');
        else if (phase === 'hearing' && was === 'talking') start('surprise');
        else if (phase === 'ended') start(wild ? 'confetti' : big ? 'farewell' : 'wave');
        nextIdle = now + 2500;
      }
      if (!still && phase === 'listening' && !antics.some(a => now - a.start < a.dur * 1000) && now > nextIdle) {
        const pool = IDLE[wild ? 'wild' : 'playful'].filter(n => n !== lastIdle);
        lastIdle = pool[Math.floor(Math.random() * pool.length)];
        start(lastIdle);
        nextIdle = now + (wild ? 3000 : 4000) + Math.random() * 2000;
      }
      antics = antics.filter(a => now - a.start < a.dur * 1000 && !(a.cutAt && now - a.cutAt > 200));
      const dt = Math.max(0, Math.min(0.05, (now - prev) / 1000)); prev = now;
      lvC += ((sim.voice === 'claude' ? sim.level() : 0) - lvC) * (1 - Math.exp(-dt * 18));
      lvM += (sim.meLevel() - lvM) * (1 - Math.exp(-dt * 18));
      const t = now / 1000;
      const T = still ? targets(v, 0, 9, 0, 0, pers) : targets(v, t, (now - entered) / 1000, lvC, lvM, pers);
      if (still && v.phase === 'ended') T.fade = 1;
      for (const k of SMOOTH) P[k] = still ? T[k] : P[k] + (T[k] - P[k]) * (1 - Math.exp(-dt * RATE[k]));
      P.legs = T.legs; P.thought = T.thought && !still; P.sleepy = T.sleepy && !still;
      const d = anticDeltas(antics, now);
      spinAngle = still ? 0 : (spinAngle + P.spin * dt) % 360;
      const boxH = clawd ? 20 * u : spark;
      // Whole CSS px, in steps of two so a half-size mascot (D's perch) still lands on whole pixels.
      const fit = Math.min(1, room / (9 * (clawd ? u : 4))); // the biggest hop is 9 units
      const lift = 2 * Math.round((Math.max(-2, P.hop + d.hop) * (clawd ? u : 4) * fit) / 2);
      const sink = 2 * Math.round((d.sink * (boxH + 14)) / 2);
      host.current.style.transform = `translateY(${sink - lift}px)`;
      confetti = confetti.filter(c => now - c.born < 1700);
      if (clawd) {
        const s = clawdSvg(P, d, still ? [] : clawdParticles(P, t, now, confetti));
        if (s !== last) { svg.innerHTML = s; last = s; }
        svg.style.opacity = P.fade.toFixed(3);
      } else sparkDraw(svg, P, d, spinAngle, still ? '' : sparkParticles(P, t, now, confetti), t);
      if (!still) raf = requestAnimationFrame(draw);
    };
    draw();
    // Reduced motion: no loop, just the still pose for each state.
    const unsub = sim.subscribe(() => REDUCE.matches && draw());
    return () => { cancelAnimationFrame(raf); unsub(); live.delete(start); };
  }, [kind, pers, sim, u, room]);
  const clawd = kind === 'clawd';
  const w = clawd ? 36 * u : spark, h = clawd ? 20 * u : spark;
  return (
    <span className={`mascot ${kind} ${className}`} style={{ width: Math.max(36 * u, spark), height: Math.max(20 * u, spark) }} aria-hidden="true">
      <span ref={host} className="mascot-host" style={{ width: w, height: h }} onPointerDown={() => poke.current?.()}>
        <svg ref={svgRef} width={w} height={h} viewBox={clawd ? '0 0 36 10' : '-50 -50 100 100'} preserveAspectRatio={clawd ? 'none' : undefined} shapeRendering={clawd ? 'crispEdges' : 'geometricPrecision'} />
      </span>
    </span>
  );
}
