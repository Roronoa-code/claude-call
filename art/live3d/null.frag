#version 300 es
precision highp float;
uniform vec2 uRes;
uniform vec4 uBody;
uniform vec4 uHead;
uniform vec4 uAnt;
uniform vec4 uEyes;
uniform vec4 uMood;
uniform vec4 uHands;
uniform vec4 uStage;
uniform vec4 uKey;
uniform vec4 uKeyCol;
uniform vec4 uShadow;
vec3 KEY, KEYC;
float RIM, AMB;
out vec4 outColor;

const float FL = 5.0;
const vec3 NECK = vec3(0.0, 1.1, 0.0);
const vec3 HEAD_C = vec3(0.0, 0.53, 0.0);
const vec3 HEAD_B = vec3(0.66, 0.53, 0.43);
const vec2 SCREEN = vec2(0.52, 0.4);
const float ANT_Y = 1.06;
const float LAP_Y = 0.6;

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
float limb(vec3 p, vec3 a, vec3 b, float r, float n) {
    vec3 pa = p - a, ba = b - a;
    float L = length(ba), h = clamp(dot(pa, ba) / (L * L), 0.0, 1.0);
    float g = abs(fract(h * n) - 0.5) * 2.0;
    float groove = 0.02 * smoothstep(0.7, 1.0, g) * step(0.5 / n, h) * step(h, 1.0 - 0.5 / n);
    return length(pa - ba * h) - r + groove;
}
vec3 lin(vec3 s) { return pow(s, vec3(2.2)); }

vec3 local(vec3 p) {
    p.y -= uBody.y;
    p.xy = rot(uBody.z) * p.xy;
    p.xz = rot(uBody.w) * p.xz;
    float sx = inversesqrt(uBody.x);
    return p / vec3(sx, uBody.x, sx);
}
vec3 worldDir(vec3 v) { v.xz = rot(-uBody.w) * v.xz; v.xy = rot(-uBody.z) * v.xy; return v; }
vec3 headSpace(vec3 q) {
    vec3 h = q - NECK;
    h.xz = rot(uHead.z) * h.xz;
    h.yz = rot(uHead.y) * h.yz;
    h.xy = rot(uHead.x) * h.xy;
    return h;
}
vec3 headDir(vec3 v) { v.xy = rot(-uHead.x) * v.xy; v.yz = rot(-uHead.y) * v.yz; v.xz = rot(-uHead.z) * v.xz; return worldDir(v); }

const vec2 ANT_C = vec2(0.08, 0.52);
const float ANT_R = 0.22;
float antenna(vec3 a) {
    float k = clamp(a.y / 0.7, 0.0, 1.0);
    k *= k;
    a.xy = rot(uAnt.x * k) * a.xy;
    a.zy = rot(uAnt.y * k) * a.zy;
    a.y /= 1.0 + uAnt.z * k;
    const float r = 0.03;
    vec2 s0 = ANT_C + ANT_R * vec2(-0.906, -0.423);
    float d = sdCapsule(a, vec3(0.0), vec3(0.0, 0.1, 0.0), r);
    d = min(d, length(vec2(sdBezier(a.xy, vec2(0.0, 0.1), vec2(0.0, s0.y + 2.142 * s0.x), s0), a.z)) - r);
    vec2 c = rot(-0.0873) * (a.xy - ANT_C);
    d = min(d, sdArc(vec3(c, a.z), vec2(0.94, -0.342), ANT_R, r));
    return min(d, length(a - vec3(ANT_C + vec2(0.2, -0.105), 0.0)) - 0.085);
}

float shell(vec3 hc) {
    float head = sdRoundBox(hc, HEAD_B, 0.24);
    return smax(head, -sdRoundBox(hc - vec3(0.0, 0.0, HEAD_B.z), vec3(SCREEN, 0.07), 0.12), 0.07);
}
float glass(vec3 hc) {
    vec2 u = clamp(hc.xy / SCREEN, -1.0, 1.0);
    float bulge = 0.03 * (1.0 - u.x * u.x) * (1.0 - u.y * u.y);
    return sdRoundBox(hc - vec3(0.0, 0.0, HEAD_B.z - 0.07), vec3(SCREEN - 0.005, 0.04), 0.11) - bulge;
}

float gM;
#define PART(dd, id) { float d_ = (dd); if (d_ < d) { d = d_; gM = id; } }

float sdf(vec3 p) {
    vec3 q = local(p);
    float d = 1e9;
    gM = 1.0;
    vec3 f = vec3(abs(q.x) - 0.26, q.y, q.z - 0.05);
    PART(max(sdEllipsoid(f, vec3(0.25, 0.31, 0.3)), 0.045 - q.y), 1.0);
    PART(max(sdRoundBox(f - vec3(0.0, 0.024, 0.0), vec3(0.26, 0.024, 0.31), 0.02), (length(f.xz / vec2(0.26, 0.31)) - 1.0) * 0.24), 6.0);
    vec3 lg = vec3(abs(q.x) - 0.2, q.y - 0.42, q.z);
    PART(sdCylY(lg, 0.1, 0.12) - 0.02, 1.0);
    PART(sdCylY(lg, 0.118, 0.025) - 0.012, 6.0);
    PART(sdRoundBox(q - vec3(0.0, 0.8, 0.0), vec3(0.31, 0.28, 0.26), 0.2), 1.0);
    PART(sdCylY(q - vec3(0.0, 1.06, 0.0), 0.15, 0.06) - 0.01, 6.0);
    float lift = uHead.w;
    vec3 l = q - vec3(0.0, LAP_Y + lift, 0.46 + lift * 0.3);
    l.yz = rot(-0.06) * l.yz;
    PART(sdRoundBox(l - vec3(0.0, 0.0, -0.17), vec3(0.34, 0.018, 0.17), 0.012), 4.0);
    vec3 lid = l;
    lid.yz = rot(-0.32) * lid.yz;
    PART(sdRoundBox(lid - vec3(0.0, 0.23, 0.0), vec3(0.34, 0.23, 0.016), 0.014), 4.0);
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

const float GLOW = 0.03;
vec2 glyphs(vec2 s) {
    float happy = uMood.x, think = uMood.y, talk = uMood.z, t = uMood.w;
    vec2 look = uEyes.xy * vec2(0.1, 0.065);
    float sleepy = uEyes.w;
    float keep = max((1.0 - think) * (1.0 - talk), 1e-3);
    float w = mix(0.125, 0.15, happy) * mix(1.0, 0.75, sleepy);
    float hgt = mix(0.13, 0.17, happy) * max(clamp(uEyes.z, 0.0, 1.0), happy) * (1.0 - sleepy);
    float th = mix(0.025, 0.031, happy) * mix(1.0, 0.65, sleepy);
    float ey = (s.y - look.y + 0.03 + 0.02 * sleepy) / keep;
    float dl = sdSeg(vec2(abs((s.x - look.x + 0.2) / keep), ey), vec2(w, -hgt * 0.5), vec2(0.0, hgt * 0.5), th) * keep;
    float dr = sdSeg(vec2(abs((s.x - look.x - 0.2) / keep), ey), vec2(w, -hgt * 0.5), vec2(0.0, hgt * 0.5), th) * keep;
    float d = min(dl, dr);
    float g = exp(-max(dl, 0.0) / GLOW) + exp(-max(dr, 0.0) / GLOW);
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
    if (talk > 0.01) {
        float L = 0.3 * talk;
        float x = s.x - look.x * 0.5;
        float A = (0.012 + 0.05 * uHands.z) * talk;
        float env = 1.0 - pow(clamp(abs(x) / max(L, 1e-3), 0.0, 1.0), 2.0);
        float ph = x * 30.0 - t * 9.4248;
        float y = A * env * sin(ph);
        float dy = A * env * 30.0 * cos(ph);
        float dv = abs(s.y + 0.02 - y);
        float dw = mix(dv / sqrt(1.0 + dy * dy), dv, smoothstep(0.02, 0.08, dv));
        if (abs(x) > L) dw = length(vec2(abs(x) - L, s.y + 0.02));
        d = min(d, dw - 0.02 * talk);
        g += exp(-max(dw - 0.02 * talk, 0.0) / GLOW) * talk;
    }
    return vec2(d, g);
}

vec3 screenCol(vec3 h, vec3 n, vec3 v, float occ, float foot) {
    float ndv = max(dot(n, v), 0.0);
    float fr = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
    vec3 col = vec3(0.0012) + fr * studio(reflect(-v, n)) * 0.6 * occ;
    vec2 dg = glyphs(h.xy + vec2(0.0, uAnt.w * 0.55));
    float rolled = 1.0 - 0.85 * smoothstep(0.12, 0.5, abs(uAnt.w));
    float d = dg.x;
    float aa = foot * 1.2;
    float core = smoothstep(aa, -aa, d);
    float hot = smoothstep(0.0, -0.014, d);
    vec2 look = uEyes.xy * vec2(0.1, 0.065);
    vec2 c = h.xy - look + vec2(0.0, 0.03);
    float wide = exp(-length(c - vec2(0.2, 0.0)) / 0.1) + exp(-length(c + vec2(0.2, 0.0)) / 0.1);
    float glow = min(dg.y, 1.5) * 0.45 + wide * 0.02;
    vec3 red = vec3(1.0, 0.09, 0.03);
    col += (red * (core + glow) + vec3(1.0, 0.45, 0.3) * hot * 0.6) * uHands.w * rolled;
    return col * (1.0 - 0.6 * smoothstep(0.3, 0.5, max(abs(h.x) - SCREEN.x + 0.45, abs(h.y) - SCREEN.y + 0.45)));
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
    vec3 vinyl = lin(vec3(0.215, 0.2, 0.19));
    vec3 col;
    if (m < 2.5) {
        float w = m > 1.5 ? 1.0 : 0.0;
        if (h.z > HEAD_B.z - 0.2 && abs(h.x) < SCREEN.x + 0.1 && abs(h.y) < SCREEN.y + 0.1) {
            vec2 e = abs(h.xy) - SCREEN + 0.005 + 0.11;
            w = clamp(0.5 - (length(max(e, 0.0)) + min(max(e.x, e.y), 0.0) - 0.11) / (1.5 * foot), 0.0, 1.0);
        }
        col = w < 1.0 ? light(n, v, vinyl * mix(0.8, 1.0, smoothstep(0.0, 1.6, q.y)), 0.95, occ) : vec3(0.0);
        if (w > 0.0) col = mix(col, screenCol(h, n, v, occ, foot), w);
    } else if (m < 3.5) {
        float ndv = max(dot(n, v), 0.0);
        vec3 f0 = vec3(0.46, 0.39, 0.33);
        col = (f0 + (1.0 - f0) * pow(1.0 - ndv, 5.0)) * studio(reflect(-v, n)) * mix(0.3, 1.0, occ);
    } else if (m < 4.5) col = light(n, v, lin(vec3(0.19, 0.185, 0.185)), 0.55, occ);
    else if (m < 5.5) {
        float r = length(h.yz - vec2(-0.02, 0.02));
        float dish = smoothstep(0.15, 0.13, r) * smoothstep(0.05, 0.07, r);
        col = light(n, v, vinyl * mix(1.0, 0.5, dish), mix(0.95, 0.5, dish), occ * mix(1.0, 0.75, dish));
    } else col = light(n, v, vinyl * 0.5, 0.5, occ);
    float spill = uHands.w * (1.0 - uMood.y * 0.3) * smoothstep(0.9, 0.0, length(vec3(h.x * 0.8, h.y + 0.1, max(h.z - HEAD_B.z, 0.0) * 2.0)));
    if (h.z > 0.0 && abs(h.x) < SCREEN.x + 0.12 && abs(h.y) < SCREEN.y + 0.12) spill *= 0.25;
    if (m != 2.0) col += vec3(0.06, 0.006, 0.002) * spill * occ * max(dot(n, normalize(headDir(vec3(0.0, 0.0, 1.0)))), 0.0);
    return softClip(col);
}

float pool(vec3 f, vec2 c, vec2 r) {
    vec2 q = (f.xz - c) / (r * (1.0 + uBody.y));
    return 0.6 * uShadow.x * exp(-0.7 * dot(q, q)) / (1.0 + 5.0 * uBody.y);
}

float castShadow(vec3 f, vec3 c, float r) {
    vec3 o = f - c;
    float b = dot(o, KEY), m2 = dot(o, o) - b * b;
    if (uShadow.x <= 0.0 || m2 > r * r || (b > 0.0 && dot(o, o) > r * r)) return 0.0;
    float res = 1.0, t = 0.02, ph = 1e10;
    for (int i = 0; i < 40; i++) {
        float d = map(f + KEY * t);
        float y = d * d / (2.0 * ph), e = sqrt(max(d * d - y * y, 0.0));
        res = min(res, uShadow.y * e / max(t - y, 1e-3));
        ph = d;
        t += clamp(d, 0.02, 0.3);
        if (res < 0.004 || t > 6.0) break;
    }
    res = clamp(res, 0.0, 1.0);
    return uShadow.x * (1.0 - res * res * (3.0 - 2.0 * res)) * smoothstep(r * r, 0.5 * r * r, m2);
}

void main() {
    KEY = normalize(dot(uKey.xyz, uKey.xyz) > 0.0 ? uKey.xyz : vec3(-0.55, 0.80, 0.55));
    KEYC = 1.0 + uKeyCol.rgb; RIM = 1.0 + uKey.w; AMB = 1.0 + uKeyCol.w;
    vec2 fc = gl_FragCoord.xy - vec2(0.0, uStage.y);
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
            t += px < 2.0 ? d * 0.5 : d;
            if (t > tEnd) break;
        }
        cover = clamp(1.0 - best, 0.0, 1.0);
        if (cover > 0.0) {
            tHit = tBest;
            col = shade(ro + rd * tBest, rd, pix * tBest);
        }
    }
    float shadow = 0.0;
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
        if (tf < tHit || cover < 1.0) shadow = max(shadow, max(castShadow(f, bc, 1.85), pool(f, vec2(0.0, 0.05), vec2(0.6, 0.5))) * edge);
    }
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
    float a2 = ra + shadow * (1.0 - ra);
    float a = cover + a2 * (1.0 - cover);
    col = (col * cover + rcol * ra * (1.0 - cover)) / max(a, 1e-4);
    outColor = vec4(pow(clamp(col, 0.0, 1.0), vec3(1.0 / 2.2)), a);
}
