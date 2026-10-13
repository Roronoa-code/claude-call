// The mascot's personality: short one-off "antics" laid over its base pose, plus the little
// particles (thought dots, sleepy z's, confetti). Every antic is a curve that starts and ends at
// zero, so it always eases back into the base pose; cutting one short fades it out instead.

const sm = t => (t <= 0 ? 0 : t >= 1 ? 1 : t * t * (3 - 2 * t));
const lerp = (a, b, t) => a + (b - a) * t;
const seg = (k, a, b) => sm((k - a) / (b - a));

// A hop with real squash and stretch: crouch, stretch on the way up, round at the top,
// stretch on the way down, squash on landing, settle.
function hop(k, H, S, a = 0.16, l = 0.76) {
  if (k < a) return { hop: 0, squash: S * sm(k / a) };
  if (k < l) {
    const t = (k - a) / (l - a), air = 4 * t * (1 - t);
    const blend = sm(Math.min(1, t / 0.18));
    return { hop: air * H, squash: lerp(S, -0.9 * S * (1 - air), blend) };
  }
  const u = (k - l) / (1 - l);
  return { hop: 0, squash: u < 0.35 ? lerp(-0.9 * S, S, sm(u / 0.35)) : S * (1 - sm((u - 0.35) / 0.65)) };
}
const add = (...ds) => Object.assign({}, ...ds);

export const ANTICS = {
  // Answered (playful): a happy jump with a full turn.
  joy: { dur: 1.15, f: k => add(hop(k, 6, 2), { turn: seg(k, 0.2, 0.74), arms: k > 0.16 && k < 0.82 ? 2 : 0, happy: k > 0.1 && k < 0.9 }) },
  // Answered / tapped (wild): a cartwheel, two turns and a flip at the top.
  cartwheel: { dur: 1.35, f: k => add(hop(k, 6, 2, 0.14, 0.8), { turn: 2 * seg(k, 0.18, 0.78), flip: seg(k, 0.3, 0.62), arms: k > 0.14 && k < 0.84 ? 2 : 0 }) },
  // Tapped (playful): three quick giggling bounces.
  giggle: { dur: 0.95, f: k => { const n = (k * 3) % 1; return add(hop(n, 1.6, 1), { happy: true, dx: Math.round(Math.sin(k * Math.PI * 6)) }); } },
  // Idle: looks one way, then the other.
  look: { dur: 1.7, f: k => ({ eyeDx: k < 0.45 ? -1.4 * seg(k, 0, 0.12) : k < 0.85 ? lerp(-1.4, 1.4, seg(k, 0.45, 0.55)) : 1.4 * (1 - seg(k, 0.85, 1)) }) },
  // Idle: wiggles its legs (Clawd) or rays (spark).
  wiggle: { dur: 1.1, f: k => ({ wig: Math.sin(Math.PI * k), legs: k > 0.08 && k < 0.92 ? (Math.floor(k * 14) % 2 ? [1, 0, 1, 0] : [0, 1, 0, 1]) : null }) },
  // Idle: peeks down at the captions.
  peek: { dur: 1.8, f: k => { const w = seg(k, 0, 0.2) * (1 - seg(k, 0.8, 1)); return { eyeDx: 1.3 * w, eyeDy: 1.3 * w, lean: w, hop: -0.6 * w }; } },
  // Idle: a tiny shuffle dance, side to side.
  shuffle: { dur: 1.7, f: k => { const w = seg(k, 0, 0.1) * (1 - seg(k, 0.9, 1)); const b = Math.floor(k * 8); return { dx: w * (b % 4 < 2 ? 1 : -1) * 1.4, hop: w * (b % 2 ? 0.8 : 0), legs: w > 0.3 ? (b % 2 ? [1, 0, 0, 1] : [0, 1, 1, 0]) : null, arms: w > 0.3 ? (b % 2 ? 1 : 0) : 0 }; } },
  // Wild idle: ducks down behind the edge it stands on, then pops back up surprised.
  hide: { dur: 2.3, f: k => {
    const sink = k < 0.28 ? sm(k / 0.28) : k < 0.62 ? 1 : 1 - sm((k - 0.62) / 0.14);
    const pop = k > 0.62 ? hop(Math.min(1, (k - 0.62) / 0.38), 3, 2) : { hop: 0, squash: 0 };
    return add(pop, { sink, pop: k > 0.66 && k < 0.92 ? 1 : 0, eyeDx: k > 0.7 ? 0 : -1 });
  } },
  // You cut in: eyes pop wide with a startled little jump.
  surprise: { dur: 0.85, f: k => add(hop(k, 2.2, 1.5, 0.08, 0.6), { pop: k < 0.8 ? 1 : 0 }) },
  // Ended (playful): a big wave, a hop, then a goodbye wave.
  wave: { dur: 1.45, f: k => add(k > 0.38 && k < 0.7 ? hop((k - 0.38) / 0.32, 3, 1.5) : {}, { armWave: k < 0.38 || k > 0.7 ? (Math.floor(k * 16) % 2 ? 2 : 1) : 2, happy: true }) },
  // Ended (mixed): the playful goodbye wave, with the wild confetti burst.
  farewell: { dur: 1.45, burst: true, f: k => ANTICS.wave.f(k) },
  // Ended (wild): a burst of confetti and a flip.
  confetti: { dur: 1.3, burst: true, f: k => add(hop(k, 6, 2, 0.1, 0.78), { flip: seg(k, 0.25, 0.6), turn: seg(k, 0.2, 0.75), arms: 2, happy: true }) },
};
export const IDLE = { playful: ['look', 'wiggle', 'peek', 'shuffle'], wild: ['hide', 'shuffle', 'look', 'wiggle', 'peek'] };

// Sums the running antics. Each one fades in over 80 ms and out over 200 ms if cut short.
export function anticDeltas(list, now) {
  const d = { hop: 0, squash: 0, dx: 0, eyeDx: 0, eyeDy: 0, lean: 0, sink: 0, pop: 0, wig: 0, turn: 0, flip: 0, arms: 0, armWave: 0, legs: null, happy: false };
  let lead = 0;
  for (const a of list) {
    const k = (now - a.start) / (a.dur * 1000);
    if (k >= 1) continue;
    const w = Math.min(1, (now - a.start) / 80) * (a.cutAt ? Math.max(0, 1 - (now - a.cutAt) / 200) : 1);
    const f = ANTICS[a.name].f(Math.max(0, k));
    for (const key of ['hop', 'squash', 'dx', 'eyeDx', 'eyeDy', 'lean', 'sink', 'wig']) if (f[key]) d[key] += f[key] * w * (key === 'hop' || key === 'squash' ? a.scale : 1);
    if (w > lead) { // turns, flips, arms and legs come from the strongest antic
      lead = w;
      d.turn = (f.turn || 0) * (w > 0.5 ? 1 : 0); d.flip = f.flip || 0; d.arms = f.arms || 0; d.armWave = f.armWave || 0;
      d.legs = f.legs || null; d.happy = !!f.happy && w > 0.5; d.pop = f.pop ? w : 0;
    }
  }
  return d;
}

// ---- Particles, in the mascot's own units (Clawd: grid cells; spark: its 100-unit box) ----
const WARM = ['#D97757', '#F2C7AE', '#FFB38A', '#EADFD2', '#E99A73'];
export function makeConfetti(clawd, now) {
  return Array.from({ length: 24 }, (_, i) => {
    // A fan up and to the right, kept low so it never reaches text above or the caption below.
    const a = -Math.PI * (0.06 + (clawd ? 0.36 : 0.55) * (i / 23)) + (Math.random() - 0.5) * 0.2;
    const v = clawd ? 5 + Math.random() * 4 : 42 + Math.random() * 34;
    return { born: now, x: clawd ? 28 : 0, y: clawd ? -1 : -20, vx: Math.cos(a) * v * (clawd ? 1.3 : 1), vy: Math.sin(a) * v, c: WARM[i % WARM.length], big: i % 3 === 0, spin: (Math.random() - 0.5) * 720 };
  });
}
// Clawd's particles as whole-cell rects (cells are 1 wide, 2 tall on screen).
export function clawdParticles(state, t, now, confetti) {
  const out = [];
  if (state.thought) { // thought dots pop up to the upper right, one at a time
    const c = (t % 1.8) / 1.8;
    [[33, -1, 2, 1], [36, -3, 2, 1], [39, -6, 4, 2]].forEach(([x, y, w, h], i) => { if (c > 0.12 + i * 0.22) out.push({ x, y, w, h, c: '#F2C7AE' }); });
  }
  if (state.sleepy) { // z's drift up and fade
    for (let i = 0; i < 2; i++) {
      // Small z's just off the head, taking turns: each lives in its own half of the loop, so the
      // second only appears once the first has risen and gone. They never overlap.
      const k = ((t + i * 1.3) % 2.6) / 2.6;
      if (k >= 0.46) continue;
      const x = Math.round(31 + k * 8), y = Math.round(-1 - k * 6), o = (k < 0.06 ? k / 0.06 : 1) * (1 - Math.max(0, (k - 0.32) / 0.14));
      if (o <= 0) continue;
      // Drawn in half rows (each one exactly u px square on screen): a 5 x 5 Z with full bars and a
      // one-pixel diagonal stepping from top right to bottom left.
      const Z = ['#####', '...#.', '..#..', '.#...', '#####'];
      Z.forEach((row, j) => [...row].forEach((ch, i2) => ch === '#' && out.push({ x: x + i2, y: y + j * 0.5, w: 1, h: 0.5, c: '#F2C7AE', o })));
    }
  }
  for (const p of confetti) {
    const age = (now - p.born) / 1000;
    if (age > 1.7) continue;
    out.push({ x: Math.round(p.x + p.vx * age), y: Math.round(p.y + p.vy * age + 0.5 * 9 * age * age), w: p.big ? 2 : 1, h: 1, c: p.c, o: 1 - Math.max(0, (age - 1.2) / 0.5) });
  }
  return out;
}
export function sparkParticles(state, t, now, confetti) {
  let s = '';
  if (state.thought) {
    const c = (t % 1.8) / 1.8;
    [[42, -30, 3.2], [52, -42, 4.4], [64, -58, 6]].forEach(([x, y, r], i) => { if (c > 0.12 + i * 0.22) s += `<circle cx="${x}" cy="${y}" r="${r}" fill="#F2C7AE"/>`; });
  }
  if (state.sleepy) {
    for (let i = 0; i < 2; i++) {
      const k = ((t + i * 1.3) % 2.6) / 2.6, x = 34 + k * 24, y = -26 - k * 28, z = 6 + k * 4;
      if (k >= 0.46) continue;
      const o = (k < 0.06 ? k / 0.06 : 1) * (1 - Math.max(0, (k - 0.32) / 0.14));
      if (o <= 0) continue;
      s += `<path d="M${x} ${y}h${z}l-${z} ${z}h${z}" fill="none" stroke="#F2C7AE" stroke-width="2.6" stroke-linejoin="round" stroke-linecap="round" opacity="${o.toFixed(2)}"/>`;
    }
  }
  for (const p of confetti) {
    const age = (now - p.born) / 1000;
    if (age > 1.7) continue;
    const x = p.x + p.vx * age, y = p.y + p.vy * age + 0.5 * 70 * age * age, o = 1 - Math.max(0, (age - 1.2) / 0.5);
    s += p.big
      ? `<rect x="-6" y="-2" width="12" height="4" rx="2" fill="${p.c}" opacity="${o.toFixed(2)}" transform="translate(${x.toFixed(1)} ${y.toFixed(1)}) rotate(${(p.spin * age).toFixed(0)})"/>`
      : `<circle cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="3.4" fill="${p.c}" opacity="${o.toFixed(2)}"/>`;
  }
  return s;
}
