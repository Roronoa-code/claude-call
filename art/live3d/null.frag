#version 300 es
// Null (Codex), live: a dark gunmetal vinyl robot whose boxy head is a screen with red glyph eyes, a glossy
// question-mark antenna on a spring, and a tiny laptop held in both hands. Same pipeline as alive.rs: a raymarched
// SDF drawn into a texture, uniforms driven by the motion code. Units: about 2.9 tall to the antenna's top; y is up
// from the floor; he faces +z.
precision highp float;
uniform vec2 uRes;
uniform vec4 uBody;  // height scale (squash), lift, lean, turn
uniform vec4 uHead;  // roll, nod, turn (on top of the body's), laptop lift
uniform vec4 uAnt;   // antenna bend sideways, bend forwards, stretch; the face rolled between two faces (-0.5..0.5, home's mode dial)
uniform vec4 uEyes;  // look x, look y, open (blink), sleepy
uniform vec4 uMood;  // happy, think (dots), talk (wave), time in seconds (wrapped at 120)
uniform vec4 uHands; // tap left, tap right (typing), voice level, screen brightness
// The home stage's world (all 0 elsewhere: the studio as it was, no floor drawn).
uniform vec4 uStage; // floor reflection strength, px of floor below the picture, reflection blur, its fade with height
uniform vec4 uKey;   // key light direction (0: the studio's top-left key), rim light strength (added to 1)
uniform vec4 uKeyCol; // key light colour (added to white), ambient strength (added to 1)
uniform vec4 uShadow; // the key light's cast shadow on the floor: strength, softness; px over which the floor fades out at the bottom (0: 30 logical px); light bounced up off the floor
vec3 KEY, KEYC; // the key light's direction and colour
float RIM, AMB;
out vec4 outColor;

const float FL = 5.0;
const vec3 NECK = vec3(0.0, 1.1, 0.0);     // the head turns about here
const vec3 HEAD_C = vec3(0.0, 0.53, 0.0);   // head centre, from the neck
const vec3 HEAD_B = vec3(0.66, 0.53, 0.43); // head half size
const vec2 SCREEN = vec2(0.52, 0.4);        // screen half size: most of the face, inside a thick rounded bezel
const float ANT_Y = 1.06;                   // antenna base, from the neck
const float LAP_Y = 0.6;                    // the laptop's base, held low in both hands

mat2 rot(float a) { float c = cos(a), s = sin(a); return mat2(c, s, -s, c); }
float smin(float a, float b, float k) { float h = max(k - abs(a - b), 0.0) / k; return min(a, b) - h * h * k * 0.25; }
float smax(float a, float b, float k) { return -smin(-a, -b, k); }
float sdRoundBox(vec3 p, vec3 b, float r) { vec3 q = abs(p) - b + r; return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0) - r; }
float sdEllipsoid(vec3 p, vec3 r) { float k0 = length(p / r), k1 = length(p / (r * r)); return k0 * (k0 - 1.0) / k1; }
float sdCapsule(vec3 p, vec3 a, vec3 b, float r) { vec3 pa = p - a, ba = b - a; float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0); return length(pa - ba * h) - r; }
float sdCylX(vec3 p, float r, float h) { vec2 d = abs(vec2(length(p.yz), p.x)) - vec2(r, h); return min(max(d.x, d.y), 0.0) + length(max(d, 0.0)); }
float sdCylY(vec3 p, float r, float h) { vec2 d = abs(vec2(length(p.xz), p.y)) - vec2(r, h); return min(max(d.x, d.y), 0.0) + length(max(d, 0.0)); }
float sdArc(vec3 p, vec2 sc, float ra, float rb) { p.x = abs(p.x); float k = (sc.y * p.x > sc.x * p.y) ? dot(p.xy, sc) : length(p.xy); return sqrt(max(dot(p, p) + ra * ra - 2.0 * ra * k, 0.0)) - rb; }
float sdSeg(vec2 p, vec2 a, vec2 b, float r) { vec2 pa = p - a, ba = b - a; float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0); return length(pa - ba * h) - r; }
// Distance to a quadratic bezier in 2D (Inigo Quilez's closed form).
float sdBezier(vec2 pos, vec2 A, vec2 B, vec2 C) {
    vec2 a = B - A, b = A - 2.0 * B + C, c = a * 2.0, d = A - pos;
    float kk = 1.0 / dot(b, b), kx = kk * dot(a, b), ky = kk * (2.0 * dot(a, a) + dot(d, b)) / 3.0, kz = kk * dot(d, a);
    float p = ky - kx * kx, q = kx * (2.0 * kx * kx - 3.0 * ky) + kz, h = q * q + 4.0 * p * p * p;
    if (h >= 0.0) {
        h = sqrt(h);
        vec2 x = (vec2(h, -h) - q) / 2.0;
        vec2 uv = sign(x) * pow(abs(x), vec2(1.0 / 3.0));
        float t = clamp(uv.x + uv.y - kx, 0.0, 1.0);
        return length(d + (c + b * t) * t);
    }
    float z = sqrt(-p), v = acos(q / (p * z * 2.0)) / 3.0, m = cos(v), n = sin(v) * 1.732050808;
    vec2 t = clamp(vec2(m + m, -n - m) * z - kx, 0.0, 1.0);
    vec2 e1 = d + (c + b * t.x) * t.x, e2 = d + (c + b * t.y) * t.y;
    return sqrt(min(dot(e1, e1), dot(e2, e2)));
}
// A vinyl limb from a to b, moulded in n rounded segments with shallow grooves between them.
float limb(vec3 p, vec3 a, vec3 b, float r, float n) {
    vec3 pa = p - a, ba = b - a;
    float L = length(ba), h = clamp(dot(pa, ba) / (L * L), 0.0, 1.0);
    float g = abs(fract(h * n) - 0.5) * 2.0;          // 0 mid-segment, 1 at a groove
    float groove = 0.02 * smoothstep(0.7, 1.0, g) * step(0.5 / n, h) * step(h, 1.0 - 0.5 / n);
    return length(pa - ba * h) - r + groove;
}
vec3 lin(vec3 s) { return pow(s, vec3(2.2)); }

// World to his own space: hop, lean on the feet, turn, then squash about the feet keeping his volume.
vec3 local(vec3 p) {
    p.y -= uBody.y;
    p.xy = rot(uBody.z) * p.xy;
    p.xz = rot(uBody.w) * p.xz;
    float sx = inversesqrt(uBody.x);
    return p / vec3(sx, uBody.x, sx);
}
vec3 worldDir(vec3 v) { v.xz = rot(-uBody.w) * v.xz; v.xy = rot(-uBody.z) * v.xy; return v; }
// His space to the head's: it nods, tilts and turns on the neck.
vec3 headSpace(vec3 q) {
    vec3 h = q - NECK;
    h.xz = rot(uHead.z) * h.xz;
    h.yz = rot(uHead.y) * h.yz;
    h.xy = rot(uHead.x) * h.xy;
    return h;
}
vec3 headDir(vec3 v) { v.xy = rot(-uHead.x) * v.xy; v.yz = rot(-uHead.y) * v.yz; v.xz = rot(-uHead.z) * v.xz; return worldDir(v); }

// The antenna, in its own space from the base: a question mark of glossy wire. The stem rises and leans into a loop
// that rolls over the top and down the far side to a ball tip. It bends along its length (more towards the tip), so
// it sways like a spring.
const vec2 ANT_C = vec2(0.08, 0.52); // the loop's centre
const float ANT_R = 0.22;             // the loop's radius
float antenna(vec3 a) {
    float k = clamp(a.y / 0.7, 0.0, 1.0);
    k *= k;
    a.xy = rot(uAnt.x * k) * a.xy;
    a.zy = rot(uAnt.y * k) * a.zy;
    a.y /= 1.0 + uAnt.z * k;
    const float r = 0.03;
    // the stem: straight up off the base, then easing into the loop where it starts (205 degrees round, heading up-left)
    vec2 s0 = ANT_C + ANT_R * vec2(-0.906, -0.423);
    float d = sdCapsule(a, vec3(0.0), vec3(0.0, 0.1, 0.0), r);
    d = min(d, length(vec2(sdBezier(a.xy, vec2(0.0, 0.1), vec2(0.0, s0.y + 2.142 * s0.x), s0), a.z)) - r);
    // the loop: from 205 degrees over the top to -15 degrees, an arc of 220 degrees centred on 95, into the ball
    vec2 c = rot(-0.0873) * (a.xy - ANT_C);
    d = min(d, sdArc(vec3(c, a.z), vec2(0.94, -0.342), ANT_R, r));
    return min(d, length(a - vec3(ANT_C + vec2(0.2, -0.105), 0.0)) - 0.085); // the ball, on the loop's end
}

// The head's shell with the screen's recess (its lip a thick roll), and the glass in it, bulging a little.
float shell(vec3 hc) {
    float head = sdRoundBox(hc, HEAD_B, 0.24);
    return smax(head, -sdRoundBox(hc - vec3(0.0, 0.0, HEAD_B.z), vec3(SCREEN, 0.07), 0.12), 0.07);
}
float glass(vec3 hc) {
    vec2 u = clamp(hc.xy / SCREEN, -1.0, 1.0);
    float bulge = 0.03 * (1.0 - u.x * u.x) * (1.0 - u.y * u.y);
    return sdRoundBox(hc - vec3(0.0, 0.0, HEAD_B.z - 0.07), vec3(SCREEN - 0.005, 0.04), 0.11) - bulge;
}

// Material ids: 1 vinyl, 2 screen glass, 3 metal, 4 laptop, 5 ear caps, 6 soles and joints.
float gM;
#define PART(dd, id) { float d_ = (dd); if (d_ < d) { d = d_; gM = id; } }

float sdf(vec3 p) {
    vec3 q = local(p);
    float d = 1e9;
    gM = 1.0;
    // boots: big rounded domes, widest at the floor, toes a little forward, on a dark sole
    vec3 f = vec3(abs(q.x) - 0.26, q.y, q.z - 0.05);
    PART(max(sdEllipsoid(f, vec3(0.25, 0.31, 0.3)), 0.045 - q.y), 1.0);
    PART(max(sdRoundBox(f - vec3(0.0, 0.024, 0.0), vec3(0.26, 0.024, 0.31), 0.02), (length(f.xz / vec2(0.26, 0.31)) - 1.0) * 0.24), 6.0);
    // legs: short chunky posts into the boots, a ring at the knee
    vec3 lg = vec3(abs(q.x) - 0.2, q.y - 0.42, q.z);
    PART(sdCylY(lg, 0.1, 0.12) - 0.02, 1.0);
    PART(sdCylY(lg, 0.118, 0.025) - 0.012, 6.0);
    // torso and neck
    PART(sdRoundBox(q - vec3(0.0, 0.8, 0.0), vec3(0.31, 0.28, 0.26), 0.2), 1.0);
    PART(sdCylY(q - vec3(0.0, 1.06, 0.0), 0.15, 0.06) - 0.01, 6.0);
    // the laptop: a thin base held level, the lid up and leaning back towards his face (we see its back)
    float lift = uHead.w;
    vec3 l = q - vec3(0.0, LAP_Y + lift, 0.46 + lift * 0.3);
    l.yz = rot(-0.06) * l.yz;
    PART(sdRoundBox(l - vec3(0.0, 0.0, -0.17), vec3(0.34, 0.018, 0.17), 0.012), 4.0);
    vec3 lid = l;
    lid.yz = rot(-0.32) * lid.yz;
    PART(sdRoundBox(lid - vec3(0.0, 0.23, 0.0), vec3(0.34, 0.23, 0.016), 0.014), 4.0);
    // arms: a ball shoulder, a segmented upper arm down to the elbow, a segmented forearm forward to a round mitten
    // hand gripping the laptop's side (it taps when typing)
    for (int i = 0; i < 2; i++) {
        float s = i == 0 ? -1.0 : 1.0;
        float tap = i == 0 ? uHands.x : uHands.y;
        vec3 sh = vec3(0.4 * s, 0.96, 0.0);
        vec3 el = vec3(0.49 * s, 0.72 + lift * 0.6, 0.12);
        vec3 hand = vec3(0.37 * s, LAP_Y + 0.03 + lift + 0.05 * tap, 0.38 + lift * 0.3);
        float arm = min(limb(q, sh, el, 0.115, 2.0), limb(q, el, hand, 0.105, 3.0));
        arm = min(arm, length(q - sh) - 0.15);
        arm = smin(arm, length(q - el) - 0.12, 0.03);
        PART(arm, 1.0);
        PART(sdEllipsoid(q - hand, vec3(0.125, 0.13, 0.14)), 1.0);
    }
    // the head: only worth evaluating when the ray is near it
    vec3 h = headSpace(q);
    if (length(h - vec3(0.05, 0.85, 0.0)) - 1.15 < d) {
        vec3 hc = h - HEAD_C;
        PART(shell(hc), 1.0);
        PART(glass(hc), 2.0);
        vec3 e = vec3(abs(hc.x) - HEAD_B.x, hc.y + 0.02, hc.z - 0.02);
        PART(sdCylX(e, 0.19, 0.075) - 0.025, 5.0);
        PART(sdCylY(h - vec3(0.0, ANT_Y + 0.01, 0.0), 0.06, 0.025) - 0.012, 3.0);
        PART(antenna(h - vec3(0.0, ANT_Y + 0.03, 0.0)), 3.0);
    }
    return 0.9 * d * min(uBody.x, inversesqrt(uBody.x));
}
float map(vec3 p) { return sdf(p); }

// The normal, and the surface's curvature from the same samples (the SDF's laplacian: 1/r on a wire of radius r).
vec4 normalCurv(vec3 p) {
    const vec2 k = vec2(1.0, -1.0);
    const float e = 0.0012;
    float a = map(p + k.xyy * e), b = map(p + k.yyx * e), c = map(p + k.yxy * e), d = map(p + k.xxx * e);
    return vec4(normalize(k.xyy * a + k.yyx * b + k.yxy * c + k.xxx * d), (a + b + c + d - 4.0 * map(p)) / (2.0 * e * e));
}
float occlusion(vec3 p, vec3 n) {
    float o = 0.0, s = 1.0;
    for (int i = 0; i < 5; i++) {
        float h = 0.015 + 0.06 * float(i);
        o += (h - map(p + n * h)) * s;
        s *= 0.75;
    }
    return clamp(1.0 - 1.6 * o, 0.0, 1.0);
}

// The same dark studio as the chrome Clawd, for the metal and the glass. gBlur widens every light by how far the
// reflection turns across one pixel, so thin wire and small balls show smooth highlights instead of sparkle.
float gBlur = 0.0;
float softbox(vec3 r, vec3 dir, vec2 size, float soft) {
    soft += gBlur;
    vec3 up = abs(dir.y) > 0.9 ? vec3(0.0, 0.0, 1.0) : vec3(0.0, 1.0, 0.0);
    vec3 u = normalize(cross(dir, up)), v = cross(u, dir);
    float k = dot(r, dir);
    if (k <= 0.0) return 0.0;
    vec2 q = abs(vec2(dot(r, u), dot(r, v)) / k) - size;
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0);
    return 1.0 - smoothstep(-soft, soft, d);
}
vec3 studio(vec3 r) {
    vec3 c = mix(vec3(0.006, 0.006, 0.008), vec3(0.07, 0.07, 0.08), smoothstep(-0.1, 0.9, r.y));
    c += vec3(0.11, 0.07, 0.05) * smoothstep(0.0, -0.25, r.y) * smoothstep(-0.9, -0.3, r.y);
    c += vec3(0.75, 0.72, 0.7) * 0.3 * smoothstep(0.08, 0.0, abs(r.y + 0.03));
    c += vec3(1.0, 0.98, 0.95) * 0.9 * softbox(r, normalize(vec3(0.05, 0.2, -1.0)), vec2(0.35, 0.1), 0.1);
    c += vec3(1.0, 0.96, 0.9) * 1.2 * softbox(r, normalize(vec3(-0.6, 0.7, 0.55)), vec2(0.6, 0.38), 0.14);
    c += vec3(1.0, 0.97, 0.94) * 0.7 * softbox(r, normalize(vec3(-0.15, 0.3, 0.95)), vec2(0.55, 0.12), 0.1);
    c += vec3(1.0) * 0.8 * softbox(r, normalize(vec3(0.05, 1.0, 0.1)), vec2(0.55, 0.45), 0.2);
    c += vec3(0.85, 0.9, 1.0) * 0.85 * softbox(r, normalize(vec3(0.95, 0.1, 0.25)), vec2(0.07, 1.2), 0.05);
    c += vec3(0.9, 0.92, 1.0) * 0.75 * softbox(r, normalize(vec3(-0.9, 0.1, -0.3)), vec2(0.09, 1.2), 0.06);
    c += vec3(0.6, 0.5, 1.0) * 0.45 * softbox(r, normalize(vec3(0.45, 0.15, -0.9)), vec2(0.1, 1.2), 0.08);
    return c;
}
vec3 softClip(vec3 c) { return mix(c, 0.82 + 0.18 * (1.0 - exp(-(c - 0.82) / 0.18)), step(0.82, c)); }

// Glossy vinyl, lit like the vinyl Clawd: warm key top left, fill, violet rim behind, two sharp highlights.
vec3 light(vec3 n, vec3 v, vec3 alb, float gloss, float occ) {
    vec3 l1 = KEY, l2 = normalize(vec3(0.80, 0.30, 0.50));
    vec3 l3 = normalize(vec3(0.45, 0.20, -0.85)), l4 = normalize(vec3(0.35, 0.90, 0.25));
    float wrap = max((dot(n, l1) + 0.3) / 1.3, 0.0);
    vec3 col = alb * (wrap * 1.15 * vec3(1.0, 0.95, 0.88) * KEYC + max(dot(n, l2), 0.0) * 0.25 + (0.1 + 0.06 * n.y) * AMB + KEYC * uShadow.w * max(-n.y, 0.0)) * occ;
    float ndv = max(dot(n, v), 0.0);
    float fr = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
    vec3 h1 = normalize(l1 + v), h4 = normalize(l4 + v);
    float s = pow(max(dot(n, h1), 0.0), 50.0) * 0.8 + pow(max(dot(n, h4), 0.0), 110.0) * 0.5 + pow(max(dot(n, h1), 0.0), 12.0) * 0.16;
    col += vec3(1.0, 0.94, 0.86) * KEYC * s * gloss * mix(0.5, 1.0, occ);
    col += vec3(0.55, 0.42, 1.0) * fr * 0.8 * RIM * smoothstep(0.25, 0.8, dot(n, l3)) * occ;
    col += fr * studio(reflect(-v, n)) * 0.5 * gloss * occ;
    return col;
}

// The glyph eyes on the screen (2D distance in head space, negative inside): ^ ^ that flatten into - - to blink
// and when sleepy, grow when happy, shrink away into three bouncing dots when thinking or a sound wave when talking.
// Returns the distance and the tight glow, summed over the strokes so it has no crease where two meet.
const float GLOW = 0.03;
vec2 glyphs(vec2 s) {
    float happy = uMood.x, think = uMood.y, talk = uMood.z, t = uMood.w;
    vec2 look = uEyes.xy * vec2(0.1, 0.065);
    float sleepy = uEyes.w;
    float keep = max((1.0 - think) * (1.0 - talk), 1e-3); // the eyes give way to the dots and the wave
    float w = mix(0.125, 0.15, happy) * mix(1.0, 0.75, sleepy);
    float hgt = mix(0.13, 0.17, happy) * max(clamp(uEyes.z, 0.0, 1.0), happy) * (1.0 - sleepy);
    float th = mix(0.025, 0.031, happy) * mix(1.0, 0.65, sleepy);
    float ey = (s.y - look.y + 0.03 + 0.02 * sleepy) / keep;
    float dl = sdSeg(vec2(abs((s.x - look.x + 0.2) / keep), ey), vec2(w, -hgt * 0.5), vec2(0.0, hgt * 0.5), th) * keep;
    float dr = sdSeg(vec2(abs((s.x - look.x - 0.2) / keep), ey), vec2(w, -hgt * 0.5), vec2(0.0, hgt * 0.5), th) * keep;
    float d = min(dl, dr);
    float g = exp(-max(dl, 0.0) / GLOW) + exp(-max(dr, 0.0) / GLOW);
    // thinking: three dots hopping in turn
    if (think > 0.01) {
        for (int i = 0; i < 3; i++) {
            float fi = float(i);
            float hop = max(sin(t * 5.236 - fi * 1.1), 0.0);
            vec2 c = vec2((fi - 1.0) * 0.21, -0.02 + 0.045 * hop * hop) + look * 0.6;
            float dd = length(s - c) - 0.038 * think;
            d = min(d, dd);
            g += exp(-max(dd, 0.0) / GLOW) * think;
        }
    }
    // talking: a wave across the screen, its height the voice
    if (talk > 0.01) {
        float L = 0.3 * talk;
        float x = s.x - look.x * 0.5;
        float A = (0.012 + 0.05 * uHands.z) * talk;
        float env = 1.0 - pow(clamp(abs(x) / max(L, 1e-3), 0.0, 1.0), 2.0);
        float ph = x * 30.0 - t * 9.4248;
        float y = A * env * sin(ph);
        float dy = A * env * 30.0 * cos(ph);
        float dv = abs(s.y + 0.02 - y);
        float dw = mix(dv / sqrt(1.0 + dy * dy), dv, smoothstep(0.02, 0.08, dv)); // the slope fix only holds near the line
        if (abs(x) > L) dw = length(vec2(abs(x) - L, s.y + 0.02));
        d = min(d, dw - 0.02 * talk);
        g += exp(-max(dw - 0.02 * talk, 0.0) / GLOW) * talk;
    }
    return vec2(d, g);
}

// Black glass: the studio's reflection, the glyphs' glow spreading in it.
vec3 screenCol(vec3 h, vec3 n, vec3 v, float occ, float foot) {
    float ndv = max(dot(n, v), 0.0);
    float fr = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
    vec3 col = vec3(0.0012) + fr * studio(reflect(-v, n)) * 0.6 * occ;
    // the mode dial: the face rolls up or down with the finger, and dims as it passes into the next face
    vec2 dg = glyphs(h.xy + vec2(0.0, uAnt.w * 0.55));
    float rolled = 1.0 - 0.85 * smoothstep(0.12, 0.5, abs(uAnt.w));
    float d = dg.x;
    float aa = foot * 1.2;
    float core = smoothstep(aa, -aa, d);
    float hot = smoothstep(0.0, -0.014, d);
    // a tight glow round the strokes, and a faint wide haze round the eyes
    vec2 look = uEyes.xy * vec2(0.1, 0.065);
    vec2 c = h.xy - look + vec2(0.0, 0.03);
    float wide = exp(-length(c - vec2(0.2, 0.0)) / 0.1) + exp(-length(c + vec2(0.2, 0.0)) / 0.1);
    float glow = min(dg.y, 1.5) * 0.45 + wide * 0.02;
    vec3 red = vec3(1.0, 0.09, 0.03);
    col += (red * (core + glow) + vec3(1.0, 0.45, 0.3) * hot * 0.6) * uHands.w * rolled;
    return col * (1.0 - 0.6 * smoothstep(0.3, 0.5, max(abs(h.x) - SCREEN.x + 0.45, abs(h.y) - SCREEN.y + 0.45))); // darker in the corners
}

vec3 shade(vec3 p, vec3 rd, float foot) {
    vec4 nc = normalCurv(p);
    vec3 n = nc.xyz, v = -rd;
    gBlur = clamp(2.0 * foot * max(nc.w, 0.0), 0.0, 1.5);
    sdf(p);
    float m = gM;
    vec3 q = local(p);
    vec3 h = headSpace(q) - HEAD_C;
    float occ = occlusion(p, n);
    vec3 vinyl = lin(vec3(0.215, 0.2, 0.19));  // dark gunmetal with a warm cast
    vec3 col;
    if (m < 2.5) {
        // vinyl, or the glass; where the glass meets the bezel the two blend across a pixel so the seam is smooth
        float w = m > 1.5 ? 1.0 : 0.0;
        // (measured across the face, as the wall beside the glass is seen edge-on and gives no gradient in depth)
        if (h.z > HEAD_B.z - 0.2 && abs(h.x) < SCREEN.x + 0.1 && abs(h.y) < SCREEN.y + 0.1) {
            vec2 e = abs(h.xy) - SCREEN + 0.005 + 0.11;
            w = clamp(0.5 - (length(max(e, 0.0)) + min(max(e.x, e.y), 0.0) - 0.11) / (1.5 * foot), 0.0, 1.0);
        }
        col = w < 1.0 ? light(n, v, vinyl * mix(0.8, 1.0, smoothstep(0.0, 1.6, q.y)), 0.95, occ) : vec3(0.0);
        if (w > 0.0) col = mix(col, screenCol(h, n, v, occ, foot), w);
    } else if (m < 3.5) {
        float ndv = max(dot(n, v), 0.0);
        vec3 f0 = vec3(0.46, 0.39, 0.33); // dark warm gunmetal wire
        col = (f0 + (1.0 - f0) * pow(1.0 - ndv, 5.0)) * studio(reflect(-v, n)) * mix(0.3, 1.0, occ);
    } else if (m < 4.5) col = light(n, v, lin(vec3(0.19, 0.185, 0.185)), 0.55, occ);
    else if (m < 5.5) {
        // the ear caps: a raised rim round a darker dished middle with a small boss
        float r = length(h.yz - vec2(-0.02, 0.02));
        float dish = smoothstep(0.15, 0.13, r) * smoothstep(0.05, 0.07, r);
        col = light(n, v, vinyl * mix(1.0, 0.5, dish), mix(0.95, 0.5, dish), occ * mix(1.0, 0.75, dish));
    } else col = light(n, v, vinyl * 0.5, 0.5, occ);
    // the screen's red light falls on the laptop, the hands and the bezel a little
    float spill = uHands.w * (1.0 - uMood.y * 0.3) * smoothstep(0.9, 0.0, length(vec3(h.x * 0.8, h.y + 0.1, max(h.z - HEAD_B.z, 0.0) * 2.0)));
    if (h.z > 0.0 && abs(h.x) < SCREEN.x + 0.12 && abs(h.y) < SCREEN.y + 0.12) spill *= 0.25; // not a red ring in the bezel
    if (m != 2.0) col += vec3(0.06, 0.006, 0.002) * spill * occ * max(dot(n, normalize(headDir(vec3(0.0, 0.0, 1.0)))), 0.0);
    return softClip(col);
}

void main() {
    KEY = normalize(dot(uKey.xyz, uKey.xyz) > 0.0 ? uKey.xyz : vec3(-0.55, 0.80, 0.55));
    KEYC = 1.0 + uKeyCol.rgb; RIM = 1.0 + uKey.w; AMB = 1.0 + uKeyCol.w;
    vec2 fc = gl_FragCoord.xy - vec2(0.0, uStage.y); // the stage's floor is drawn below the picture
    vec2 sp = (2.0 * fc - uRes) / uRes.y;
    vec3 ta = vec3(0.0, 1.45, 0.0);
    float az = 0.24, el = 0.14;
    vec3 ro = ta + 11.0 * vec3(sin(az) * cos(el), sin(el), cos(az) * cos(el));
    vec3 ww = normalize(ta - ro), uu = normalize(cross(ww, vec3(0.0, 1.0, 0.0))), vv = cross(uu, ww);
    vec3 rd = normalize(sp.x * uu + sp.y * vv + FL * ww);
    float pix = 2.0 / (uRes.y * FL);

    vec3 col = vec3(0.0);
    float cover = 0.0, tHit = 1e9;
    vec3 bc = vec3(0.0, 1.5 + uBody.y, 0.0);
    vec3 oc = ro - bc;
    float b = dot(oc, rd), disc = b * b - dot(oc, oc) + 1.85 * 1.85;
    if (disc > 0.0) {
        float t = max(-b - sqrt(disc), 0.0), tEnd = -b + sqrt(disc);
        float best = 1e9, tBest = t;
        for (int i = 0; i < 110; i++) {
            float d = map(ro + rd * t);
            float px = d / (pix * t);
            if (px < best) { best = px; tBest = t; }
            if (d < 0.25 * pix * t) { best = 0.0; tBest = t; break; }
            t += px < 2.0 ? d * 0.5 : d; // short steps past a near miss, so thin wire keeps a clean edge
            if (t > tEnd) break;
        }
        cover = clamp(1.0 - best, 0.0, 1.0);
        if (cover > 0.0) {
            tHit = tBest;
            col = shade(ro + rd * tBest, rd, pix * tBest);
        }
    }
    // the soft contact shadow on an invisible floor, fading as he leaves the ground
    float shadow = 0.0;
    // the stage's shadow and reflection fade out before the picture's edges (and the floor's front, at its bottom)
    float edge = smoothstep(-uStage.y, -uStage.y + (uShadow.z > 0.0 ? uShadow.z : 30.0 * (uRes.y / 230.0)), fc.y)
        * smoothstep(0.0, 16.0 * (uRes.y / 230.0), fc.x) * smoothstep(0.0, 16.0 * (uRes.y / 230.0), uRes.x - fc.x);
    if (rd.y < 0.0) {
        float tf = -ro.y / rd.y;
        vec3 f = ro + rd * tf;
        if ((tf < tHit || cover < 1.0) && length(f.xz) < 1.6) {
            float o = 0.0, s = 1.0;
            for (int i = 0; i < 5; i++) {
                float h = 0.03 + 0.07 * float(i);
                o += max(h - map(f + vec3(0.0, h, 0.0)), 0.0) * s;
                s *= 0.7;
            }
            shadow = clamp(o * 1.4, 0.0, 0.75);
        }
        // the stage's key light casts his shadow on the floor
        vec3 os = f - bc;
        float sb = dot(os, KEY), sc = dot(os, os) - 1.85 * 1.85;
        if (uShadow.x > 0.0 && (tf < tHit || cover < 1.0) && sb * sb - sc > 0.0 && (sb < 0.0 || sc < 0.0)) {
            float res = 1.0, t = 0.04;
            for (int i = 0; i < 40; i++) {
                float d = map(f + KEY * t);
                res = min(res, uShadow.y * d / t);
                t += clamp(d, 0.03, 0.4);
                if (res < 0.01 || t > 6.0) break;
            }
            shadow = max(shadow, uShadow.x * edge * (1.0 - smoothstep(0.0, 1.0, res)));
        }
    }
    // The stage's glossy floor: the ray mirrored at the floor, marched the same way; softer and fainter the higher the
    // point it sees, and faded out before the bottom of the picture.
    vec3 rcol = vec3(0.0);
    float ra = 0.0;
    if (uStage.x > 0.0 && rd.y < 0.0) {
        float tf = -ro.y / rd.y;
        vec3 f = ro + rd * tf, r = vec3(rd.x, -rd.y, rd.z);
        vec3 of = f - bc;
        float rb = dot(of, r), rdisc = rb * rb - dot(of, of) + 1.85 * 1.85;
        if ((tf < tHit || cover < 1.0) && rdisc > 0.0) {
            float t = max(-rb - sqrt(rdisc), 0.0), tEnd = -rb + sqrt(rdisc);
            float best = 1e9, tBest = t;
            for (int i = 0; i < 72; i++) {
                float d = map(f + r * t);
                float px = d / (pix * (tf + t));
                if (px < best) { best = px; tBest = t; }
                if (d < 0.25 * pix * (tf + t)) { best = 0.0; tBest = t; break; }
                t += px < 2.0 ? d * 0.5 : d;
                if (t > tEnd) break;
            }
            vec3 p = f + r * tBest;
            float blur = 1.0 + uStage.z * p.y / (pix * (tf + tBest));
            ra = clamp(1.0 - best / blur, 0.0, 1.0) * uStage.x * exp(-p.y * uStage.w);
            ra *= edge;
            if (ra > 0.0) rcol = shade(p, r, pix * (tf + tBest));
        }
    }
    // floor shadow, then the reflection, then him
    float a2 = ra + shadow * (1.0 - ra);
    float a = cover + a2 * (1.0 - cover);
    col = (col * cover + rcol * ra * (1.0 - cover)) / max(a, 1e-4);
    outColor = vec4(pow(clamp(col, 0.0, 1.0), vec3(1.0 / 2.2)), a);
}
