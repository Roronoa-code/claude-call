#version 300 es
precision highp float;
uniform vec2 uRes;
uniform vec4 uBody;
uniform vec4 uArms;
uniform vec4 uEyes;
uniform vec4 uMood;
uniform vec4 uMat;
uniform vec4 uStage;
uniform vec4 uKey;
uniform vec4 uKeyCol;
uniform vec4 uShadow;
out vec4 outColor;
#ifndef MAT_ID
#define MAT_ID int(uMat.x + 0.5)
#endif

const float FL = 5.0;
const float BELLY = 0.75;

int MAT;
vec3 KEY, KEYC;
float RIM, AMB;
vec3 BODY;
float BODY_Y;
float ROUND, PUFF;
vec2 EYE;
float ES;
float ESX;
vec3 LEG;
float LEGT;
float LEGK;
float TAPER;
vec3 SHOULDER;
float ARMYAW;
float REARX;
float SPLAY;
vec3 ARM;

mat2 rot(float a) { float c = cos(a), s = sin(a); return mat2(c, s, -s, c); }
float smin(float a, float b, float k) { float h = max(k - abs(a - b), 0.0) / k; return min(a, b) - h * h * k * 0.25; }
float sdRoundBox(vec3 p, vec3 b, float r) { vec3 q = abs(p) - b + r; return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0) - r; }
float sdEllipsoid(vec3 p, vec3 r) { float k0 = length(p / r), k1 = length(p / (r * r)); return k0 * (k0 - 1.0) / k1; }
vec3 lin(vec3 s) { return pow(s, vec3(2.2)); }
float hash(vec3 p) { p = fract(p * 0.3183099 + 0.1); p *= 17.0; return fract(p.x * p.y * p.z * (p.x + p.y + p.z)); }
float vnoise(vec3 x) {
    vec3 i = floor(x), f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(mix(hash(i), hash(i + vec3(1, 0, 0)), f.x), mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), f.x), f.y),
               mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), f.x), mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), f.x), f.y), f.z);
}

float sdLeg(vec3 p) {
    float ra = LEG.x + LEGT * clamp(p.y / 0.45, 0.0, 1.0);
    vec2 d = vec2(length(p.xz) - ra + 0.07, abs(p.y - 0.35) - 0.35 + 0.07);
    return min(max(d.x, d.y), 0.0) + length(max(d, 0.0)) - 0.07;
}

vec3 local(vec3 p) {
    p.y -= uBody.y;
    p.xy = rot(uBody.z) * p.xy;
    p.xz = rot(uBody.w) * p.xz;
    float sx = inversesqrt(uBody.x);
    return p / vec3(sx, uBody.x, sx);
}
vec3 worldDir(vec3 v) { v.xz = rot(-uBody.w) * v.xz; v.xy = rot(-uBody.z) * v.xy; return v; }

vec3 armSpace(vec3 q, float side, float ang) {
    q.x *= side;
    q -= vec3(BODY.x - 0.14, BODY_Y, 0.0) + SHOULDER;
    q.xz = rot(ARMYAW) * q.xz;
    q.xy = rot(-ang) * q.xy;
    return q - vec3(ARM.x - 0.01, 0.0, 0.0);
}
float arm(vec3 q, float side, float ang) { return sdEllipsoid(armSpace(q, side, ang), ARM); }

float blade(vec3 p, float a) { p.xy = rot(a) * p.xy; return sdEllipsoid(p - vec3(0.0, 0.05, 0.0), vec3(0.035, 0.1, 0.045)); }
float tufts(vec3 c) {
    if (c.y < BODY.y - 0.3 || abs(c.z) > BODY.z) return 1e3;
    vec3 m = vec3(abs(c.x), c.y, c.z);
    float d = min(blade(c - vec3(-0.1, BODY.y + 0.04, 0.2), -0.55), blade(c - vec3(0.0, BODY.y + 0.07, 0.2), -0.1));
    d = min(d, min(blade(c - vec3(0.1, BODY.y + 0.06, 0.2), 0.35), blade(c - vec3(0.2, BODY.y + 0.02, 0.2), 0.8)));
    return min(d, blade(m - vec3(BODY.x - 0.2, BODY.y - 0.14, 0.05), 1.0));
}

vec4 parts(vec3 q) {
    vec3 c = q - vec3(0.0, BODY_Y, 0.0);
    c.xz *= 1.0 + TAPER * clamp(c.y / BODY.y, -1.0, 1.0);
    vec3 u = clamp(c / BODY, -1.0, 1.0);
    vec3 puff = (1.0 - u * u) * (1.0 - u * u);
    float d = sdRoundBox(c, BODY, ROUND) - PUFF * (puff.y * puff.z + puff.x * puff.y + 0.6 * puff.x * puff.z);
    if (MAT == 1) d = smin(d, tufts(c), 0.05);
    float sp = SPLAY * (0.7 - q.y);
    float ax = abs(q.x) - sp, az = abs(q.z) - LEG.z - sp * 0.5;
    bool front = q.z > 0.0;
    float leg = sdLeg(vec3(ax - (front ? LEG.y : REARX), q.y, az));
    return vec4(d, front ? leg : 1e3, front ? 1e3 : leg, q.x < 0.0 ? arm(q, -1.0, uArms.x) : arm(q, 1.0, uArms.y));
}
float join(vec4 g) { return smin(smin(g.x, min(g.y, g.z), LEGK), g.w, 0.05); }
float scaleD(float d) { return 0.9 * d * min(uBody.x, inversesqrt(uBody.x)); }
float map(vec3 p) { return scaleD(join(parts(local(p)))); }

vec3 normal(vec3 p) {
    const vec2 k = vec2(1.0, -1.0);
    const float e = 0.0015;
    return normalize(k.xyy * map(p + k.xyy * e) + k.yyx * map(p + k.yyx * e) + k.yxy * map(p + k.yxy * e) + k.xxx * map(p + k.xxx * e));
}

float occlusion(vec3 p, vec3 n) {
    float o = 0.0, s = 1.0;
    for (int i = 0; i < 5; i++) {
        float h = 0.02 + 0.08 * float(i);
        o += (h - map(p + n * h)) * s;
        s *= 0.75;
    }
    return clamp(1.0 - 1.5 * o, 0.0, 1.0);
}

float sdEllipse(vec2 p, vec2 r) { float k0 = length(p / r), k1 = length(p / (r * r)); return k0 * (k0 - 1.0) / k1; }
float sdArc(vec2 p, float ap, float ra, float rb) {
    vec2 sc = vec2(sin(ap), cos(ap));
    p.x = abs(p.x);
    return ((sc.y * p.x > sc.x * p.y) ? length(p - sc * ra) : abs(length(p) - ra)) - rb;
}
float sdSeg(vec2 p, vec2 a, vec2 b, float r) { vec2 pa = p - a, ba = b - a; float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0); return length(pa - ba * h) - r; }
float eye(vec2 p) {
    p /= ES * vec2(ESX, 1.0);
    float happy = uMood.x, open = uEyes.z * clamp(1.0 - 2.0 * happy, 0.0, 1.0);
    float ry = 0.29 * mix(0.12, 1.0, open);
    float e = sdEllipse(p, vec2(0.14 * mix(1.12, 1.0, open), ry));
    e = max(e, p.y - 0.29 * (1.0 - 2.0 * uEyes.w));
    e = mix(sdArc(p + vec2(0.0, 0.12), 1.0, 0.17, 0.032), e, smoothstep(0.0, 0.4, open));
    e = mix(e, sdArc(vec2(p.x, -p.y) + vec2(0.0, 0.02), 1.0, 0.17, 0.03), smoothstep(0.75, 1.0, uEyes.w));
    float chev = min(sdSeg(p, vec2(-0.14, -0.07), vec2(0.0, 0.08), 0.05), sdSeg(p, vec2(0.14, -0.07), vec2(0.0, 0.08), 0.05));
    return mix(e, chev, smoothstep(0.5, 1.0, happy)) * ES * ESX;
}
vec2 lookOffset() { return uEyes.xy * vec2(0.17, 0.09); }
float face(vec2 f, out vec2 ep) {
    vec2 look = lookOffset();
    vec2 c = vec2(f.x > look.x ? EYE.x : -EYE.x, BODY_Y + EYE.y) + look;
    ep = (f - c) / ES;
    float d = eye(f - c);
    float talk = uMood.y;
    if (talk > 0.01) {
        vec2 m = f - vec2(look.x * 0.6, BODY_Y + EYE.y - 0.36 * ES + look.y * 0.5);
        d = min(d, sdEllipse(m, vec2(0.05 + 0.045 * talk, 0.012 + 0.085 * talk)) + 0.006 * (1.0 - smoothstep(0.0, 0.15, talk)));
    }
    return d;
}

vec3 env(vec3 r) { return mix(vec3(0.01, 0.01, 0.015), vec3(0.35, 0.3, 0.27), smoothstep(0.0, 0.9, r.y)); }

float softbox(vec3 r, vec3 dir, vec2 size, float soft) {
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
    c += vec3(0.9, 0.9, 0.95) * 0.6 * softbox(r, normalize(vec3(-0.35, 0.15, -0.95)), vec2(0.12, 1.2), 0.08);
    c += vec3(0.6, 0.5, 1.0) * 0.45 * softbox(r, normalize(vec3(0.45, 0.15, -0.9)), vec2(0.1, 1.2), 0.08);
    return c;
}
vec3 softClip(vec3 c) { return mix(c, 0.82 + 0.18 * (1.0 - exp(-(c - 0.82) / 0.18)), step(0.82, c)); }

float REFL = 0.0;
float LOW = 0.0;

vec3 light(vec3 n, vec3 v, vec3 alb, float gloss, float occ) {
    vec3 l1 = KEY;
    vec3 l2 = normalize(vec3(0.80, 0.30, 0.50));
    vec3 l3 = normalize(vec3(0.45, 0.20, -0.85));
    vec3 l4 = normalize(vec3(0.35, 0.90, 0.25));
    float d1 = dot(n, l1);
    float wrap = max((d1 + 0.4) / 1.4, 0.0);
    vec3 col = alb * wrap * 1.1 * KEYC;
    col += alb * KEYC * vec3(1.0, 0.3, 0.06) * 0.30 * smoothstep(0.0, 0.35, wrap) * (1.0 - smoothstep(0.35, 0.9, wrap));
    col += alb * max(dot(n, l2), 0.0) * 0.22;
    col += alb * (0.10 + 0.08 * n.y) * AMB;
    col += alb * KEYC * uShadow.w * max(-n.y, 0.0);
    col += alb * KEYC * uShadow.w * 2.0 * max(-n.y, 0.0) * LOW;
    col *= occ;
    gloss *= 1.0 - 0.85 * REFL;
    float ndv = max(dot(n, v), 0.0);
    float fr = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
    vec3 h1 = normalize(l1 + v), h4 = normalize(l4 + v);
    float s = pow(max(dot(n, h1), 0.0), 60.0) * 0.9 + pow(max(dot(n, h4), 0.0), 120.0) * 0.45 + pow(max(dot(n, h1), 0.0), 20.0) * 0.35;
    col += vec3(1.0, 0.96, 0.92) * KEYC * s * gloss * mix(0.6, 1.0, occ);
    col += vec3(0.55, 0.42, 1.0) * fr * 0.7 * RIM * smoothstep(0.0, 0.7, dot(n, l3)) * occ * (1.0 - 0.85 * REFL);
    col += fr * env(reflect(-v, n)) * gloss * occ;
    return col;
}

vec3 enamel(vec3 n, vec3 v, vec3 alb, float occ) {
    vec3 l1 = normalize(vec3(-0.55, 0.80, 0.55));
    float wrap = max((dot(n, l1) + 0.3) / 1.3, 0.0);
    vec3 col = alb * (wrap * 0.85 + max(dot(n, normalize(vec3(0.8, 0.3, 0.5))), 0.0) * 0.15 + 0.04) * occ;
    float fr = 0.05 + 0.95 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    float s = pow(max(dot(n, normalize(l1 + v)), 0.0), 90.0) * 0.8 + pow(max(dot(n, normalize(normalize(vec3(0.35, 0.9, 0.25)) + v)), 0.0), 160.0) * 0.5;
    return col + (fr * studio(reflect(-v, n)) + s) * mix(0.35, 1.0, occ);
}
vec3 chrome(vec3 n, vec3 v, float occ) {
    float ndv = max(dot(n, v), 0.0);
    vec3 f0 = vec3(0.93, 0.91, 0.87);
    vec3 fr = f0 + (1.0 - f0) * pow(1.0 - ndv, 5.0);
    return fr * studio(reflect(-v, n)) * mix(0.25, 1.0, occ);
}

vec3 toon(vec3 n, vec3 alb, float occ, float lift) {
    float d = dot(n, normalize(vec3(-0.7, 0.6, 0.4))) * 0.5 + 0.5;
    d = d * mix(0.5, 1.0, occ) + lift;
    vec3 c = mix(alb * vec3(0.62, 0.4, 0.36), alb, smoothstep(0.43, 0.47, d));
    return mix(c, alb * vec3(1.08, 1.25, 1.4), smoothstep(0.74, 0.78, d));
}

float FUZZ = 0.0;
vec3 localDir(vec3 v) { v.xy = rot(uBody.z) * v.xy; v.xz = rot(uBody.w) * v.xz; return v; }
vec4 vnoised(vec3 x) {
    vec3 i = floor(x), f = fract(x);
    vec3 u = f * f * (3.0 - 2.0 * f), du = 6.0 * f * (1.0 - f);
    float a = hash(i), b = hash(i + vec3(1, 0, 0)), c = hash(i + vec3(0, 1, 0)), d = hash(i + vec3(1, 1, 0));
    float e = hash(i + vec3(0, 0, 1)), g = hash(i + vec3(1, 0, 1)), h = hash(i + vec3(0, 1, 1)), k = hash(i + vec3(1, 1, 1));
    float k1 = b - a, k2 = c - a, k3 = e - a, k4 = a - b - c + d, k5 = a - c - e + h, k6 = a - b - e + g, k7 = -a + b + c - d + e - g - h + k;
    return vec4(du * vec3(k1 + k4 * u.y + k6 * u.z + k7 * u.y * u.z, k2 + k5 * u.z + k4 * u.x + k7 * u.z * u.x, k3 + k6 * u.x + k5 * u.y + k7 * u.x * u.y),
                a + k1 * u.x + k2 * u.y + k3 * u.z + k4 * u.x * u.y + k5 * u.y * u.z + k6 * u.z * u.x + k7 * u.x * u.y * u.z);
}
const mat3 LOCKS = mat3(0.88, 0.0, -0.48, 0.12, 0.97, 0.22, 0.46, -0.25, 0.85) * mat3(105.0, 0.0, 0.0, 0.0, 40.0, 0.0, 0.0, 0.0, 105.0);
vec4 pile(vec3 x, float foot, float clump) {
    vec4 a = vnoised(LOCKS * x);
    float fade = smoothstep(0.004, 0.009, foot);
    return vec4((a.xyz * LOCKS) * 0.65 * (1.0 - fade), smoothstep(0.15, 0.85, mix(a.w, 0.5, fade) * 0.72 + clump * 0.28));
}
vec3 plush(vec3 q, vec3 c, vec3 n, vec3 v, vec3 rd, float foot, float occ, float isLeg) {
    vec3 alb = lin(vec3(0.83, 0.37, 0.1));
    vec3 nl = localDir(n), rl = localDir(rd);
    vec3 dn = mix(vec3(0.0, -1.0, 0.0), normalize(vec3(c.x, 0.0, c.z) + 1e-4), smoothstep(0.35, 0.85, nl.y));
    vec3 F = normalize(dn - nl * dot(dn, nl) + vec3(0.0, 0.0, 1e-3));
    vec4 lump = vnoised(q * vec3(4.0, 3.0, 4.0));
    vec3 lt = lump.xyz - nl * dot(lump.xyz, nl);
    vec3 ns = normalize(n - worldDir(lt) * 0.14);
    vec3 qw = q + lt * 0.03 + 0.012 * vec3(sin(q.y * 25.0 + q.z * 9.0 + lump.w * 6.0), 0.0, sin(q.y * 22.0 + q.x * 9.0 + lump.w * 6.0));
    float clump = vnoise(qw * vec3(38.0, 16.0, 38.0) + 7.3);
    vec3 bq = abs(c) - BODY + ROUND;
    float seam = 1e3, along = c.y;
    if (bq.x > 0.0 && bq.z > 0.0 && bq.y < 0.5 * (bq.x + bq.z)) seam = abs(bq.x - bq.z) * 0.707;
    if (bq.y > 0.0 && bq.z > 0.0 && c.y > 0.0) { float s2 = abs(bq.y - bq.z) * 0.707; if (s2 < seam) { seam = s2; along = c.x; } }
    if (isLeg > 0.5) { seam = abs(q.y - 0.1); along = q.x + q.z; }
    float part = mix(0.55, 1.0, smoothstep(0.0, 0.016, seam));
    vec3 l1 = normalize(vec3(-0.55, 0.80, 0.55));
    float ndv = max(dot(ns, v), 0.0);
    float wrap0 = max((dot(ns, l1) + 0.25) / 1.25, 0.0);
    float wrap = wrap0 * wrap0 * (3.0 - 2.0 * wrap0);
    float topLit = mix(0.58, 1.05, smoothstep(0.1, BODY_Y + BODY.y, q.y));
    vec3 lit = alb * (wrap * 1.45 + max(dot(ns, normalize(vec3(0.8, 0.3, 0.5))), 0.0) * 0.1 + 0.035) * occ * topLit;
    float sw = (lump.w - 0.5) * 2.6;
    vec3 Fs = F * cos(sw) + cross(nl, F) * sin(sw);
    vec3 T = worldDir(normalize(nl * 0.5 + Fs * 0.87));
    vec3 hv = normalize(l1 + v);
    float t1 = dot(normalize(T + n * 0.25), hv), t2 = dot(normalize(T - n * 0.3), hv);
    vec3 sheen = vec3(1.0, 0.78, 0.55) * pow(max(1.0 - t1 * t1, 0.0), 40.0) * 0.14 + alb * pow(max(1.0 - t2 * t2, 0.0), 5.0) * 0.25;
    sheen *= occ * smoothstep(-0.2, 0.4, dot(n, l1)) * topLit;
    float cosv = max(-dot(rl, nl), 0.3);
    vec3 drift = (rl - nl * dot(rl, nl)) / cosv;
    const float L = 0.02;
    float footV = foot * inversesqrt(cosv);
    float w = 0.12 + 9.0 * footV;
    vec3 col = vec3(0.0);
    float acc = 0.0;
    for (int k = 0; k < 2; k++) {
        float h = 0.75 - 0.4 * float(k);
        vec4 pk = pile(qw + drift * (1.0 - h) * L - F * h * L * 0.4, footV, clump);
        float a = smoothstep(h - w, h + w, pk.w * part);
        vec3 gt = pk.xyz - nl * dot(pk.xyz, nl);
        vec3 nb = normalize(ns - worldDir(gt) * 0.0028);
        float bl = max((dot(nb, l1) + 0.25) / 1.25, 0.0) / max(wrap0, 0.05);
        vec3 ck = lit * clamp(bl, 0.4, 1.6) * mix(0.78, 1.04, h) * mix(vec3(0.9, 0.75, 0.66), vec3(1.0), h) + sheen * h * h;
        col += (1.0 - acc) * a * ck;
        acc += (1.0 - acc) * a;
    }
    col += (1.0 - acc) * lit * vec3(0.66, 0.46, 0.36);
    float gr = pow(1.0 - ndv, 3.0);
    col += lin(vec3(1.0, 0.56, 0.26)) * gr * 0.32 * (0.3 + 0.7 * wrap) * occ;
    col += lin(vec3(0.6, 0.5, 1.0)) * gr * 0.2 * smoothstep(0.0, 0.7, dot(n, normalize(vec3(0.45, 0.2, -0.85))));
    col = mix(col, lit * 1.05 + sheen + lin(vec3(1.0, 0.56, 0.26)) * 0.1 * (0.3 + 0.7 * wrap), FUZZ * 0.7);
    col *= mix(0.55, 1.0, smoothstep(0.0, 0.016, seam)) * mix(0.82, 1.0, smoothstep(0.0, 0.045, seam));
    float dash = smoothstep(0.32, 0.16, abs(fract(along / 0.032) - 0.5)) * smoothstep(0.01, 0.004, seam);
    col = mix(col, lit * vec3(1.2, 1.05, 0.85), dash * 0.7);
    return col;
}

vec3 shade(vec3 p, vec3 rd, float foot, out float ink) {
    vec3 n = normal(p), v = -rd;
    vec3 q = local(p);
    vec3 c = q - vec3(0.0, BODY_Y, 0.0);
    float occ = occlusion(p, n);
    LOW = 1.0 - smoothstep(0.2, BODY_Y, p.y);
    vec4 g = parts(q);
    float isBody = step(g.x, min(min(g.y, g.z), g.w) + 0.02);
    float isLeg = step(min(g.y, g.z), min(g.x, g.w));
    ink = 0.0;
    vec3 col;
    vec3 vin = vec3(0.95, 0.2, 0.011);
    vin *= mix(0.62, 1.0, smoothstep(0.0, 1.5, q.y)) * mix(0.6, 1.0, smoothstep(-0.7, 0.1, n.y));
    if (MAT == 1) {
        vec3 alb = lin(vec3(0.91, 0.42, 0.14));
        alb *= mix(0.86, 1.0, smoothstep(0.1, 0.9, q.y));
        col = toon(n, alb, occ, 0.0);
        float rimL = smoothstep(0.42, 0.3, max(dot(n, v), 0.0)) * smoothstep(0.1, 0.4, dot(n, normalize(vec3(-0.7, 0.6, 0.4))));
        col = mix(col, alb * vec3(1.1, 1.3, 1.5), rimL * 0.85);
        bool side = abs(n.x) > 0.7;
        vec2 fp = side ? q.zy : q.xy;
        vec2 cell = floor(fp / 0.2);
        vec2 fc = fp - (cell + 0.5) * 0.2 - (vec2(hash(vec3(cell, 4.1)), hash(vec3(cell, 9.3))) - 0.5) * 0.09;
        vec2 s = rot((hash(vec3(cell, 2.9)) - 0.5) * 1.0) * fc;
        float w = (0.011 + 0.5 * foot) * (1.0 - smoothstep(-0.04, 0.05, s.y) * 0.8);
        float stroke = smoothstep(w, w * 0.4, abs(s.x - 3.0 * s.y * s.y)) * smoothstep(0.05, 0.035, abs(s.y + 0.005));
        float edge = side ? 1.0 : smoothstep(0.5, 0.95, length(c.xy / BODY.xy));
        float where = edge * step(0.45, hash(vec3(cell, 1.7))) * isBody * step(0.0, n.z + 0.3);
        col = mix(col, alb * vec3(0.5, 0.26, 0.2), stroke * where * 0.85);
        if (isLeg > 0.5 && q.z > 0.0 && q.y < 0.24) {
            float fx = (abs(q.x) - LEG.y) / LEG.x;
            float toe = abs(fract(fx * 1.5) - 0.5) / 1.5;
            ink = max(ink, smoothstep(0.04, 0.02, toe) * step(abs(fx), 0.55) * smoothstep(0.24, 0.15, q.y) * smoothstep(0.0, 0.3, n.z));
        }
        if (isBody + isLeg < 0.5) {
            vec3 a = armSpace(q, sign(q.x), q.x < 0.0 ? uArms.x : uArms.y);
            float claw = abs(fract(a.z / ARM.z * 1.5) - 0.5) / 1.5;
            ink = max(ink, smoothstep(0.045, 0.025, claw) * step(abs(a.z), ARM.z * 0.55) * smoothstep(ARM.x * 0.55, ARM.x * 0.75, a.x));
        }
    } else if (MAT == 2) {
        col = plush(q, c, n, v, rd, foot, occ, isLeg);
    } else if (MAT == 3) {
        vec3 alb = lin(vec3(1.0, 0.44, 0.03)) * mix(0.62, 1.0, smoothstep(0.3, 1.8, q.y));
        float bandZ = BODY.z - 0.27;
        float chromeAmt = isBody * (1.0 - smoothstep(bandZ - 0.012, bandZ + 0.012, abs(c.z)));
        float lineAmt = isBody * smoothstep(0.026, 0.0, abs(abs(c.z) - bandZ));
        if (isLeg > 0.5) {
            chromeAmt = smoothstep(0.135, 0.125, q.y);
            lineAmt = smoothstep(0.02, 0.0, abs(q.y - 0.13));
        }
        float isArm = 1.0 - max(isBody, isLeg);
        if (isArm > 0.5) {
            vec3 a = armSpace(q, sign(q.x), q.x < 0.0 ? uArms.x : uArms.y);
            chromeAmt = smoothstep(0.1, 0.12, a.x);
            lineAmt = smoothstep(0.02, 0.0, abs(a.x - 0.11));
        }
        col = mix(enamel(n, v, alb, occ), chrome(n, v, occ), chromeAmt);
        col *= 1.0 - 0.75 * lineAmt;
    } else {
        vec3 gq = q * 55.0;
        float g0 = vnoise(gq);
        vec3 gg = vec3(vnoise(gq + vec3(0.25, 0.0, 0.0)), vnoise(gq + vec3(0.0, 0.25, 0.0)), vnoise(gq + vec3(0.0, 0.0, 0.25))) - g0;
        n = normalize(n + worldDir(gg) * 0.08 * (1.0 - smoothstep(0.008, 0.02, foot)));
        col = light(n, v, vin, 0.55, occ);
    }

    if (q.z > 0.3) {
        vec2 ep;
        float e = face(q.xy, ep);
        vec2 dummy;
        vec2 grad = vec2(face(q.xy + vec2(0.002, 0.0), dummy) - e, face(q.xy + vec2(0.0, 0.002), dummy) - e) / 0.002;
        vec3 out2 = normalize(worldDir(vec3(grad, 0.0)) + 1e-5);
        float aa = foot / max(dot(n, v), 0.35);
        float inside = smoothstep(aa, -aa, e);
        if (MAT == 1) {
            vec3 eyeCol = lin(vec3(0.05, 0.035, 0.03));
            float hl = smoothstep(aa / ES, -aa / ES, sdEllipse(ep - vec2(0.045, 0.14), vec2(0.03, 0.055)));
            eyeCol = mix(eyeCol, lin(vec3(0.85, 0.66, 0.52)), hl * step(0.6, uEyes.z) * (1.0 - step(0.5, uMood.x)));
            col = mix(col, eyeCol, inside);
        } else {
            float w = MAT == 3 ? 0.012 : (MAT == 0 ? 0.032 : (MAT == 2 ? 0.035 : 0.02));
            float rim = clamp(e / (w + aa), 0.0, 1.0);
            float groove = 1.0 - smoothstep(0.0, 1.0, rim);
            vec3 ng = normalize(n - out2 * (MAT == 0 ? 0.9 * (1.0 - rim) : 0.7 * groove));
            if (MAT == 0) col = light(ng, v, vin * mix(0.25, 1.0, rim), 0.55, occ * mix(0.2, 1.0, rim));
            else {
                vec3 rimCol = MAT == 3 ? chrome(ng, v, occ) : light(ng, v, col * 0.2, 0.2, occ * 0.4);
                if (MAT == 2) rimCol = col * vec3(0.2, 0.13, 0.1);
                col = mix(col, rimCol, groove * (MAT == 2 ? 0.85 : 0.9));
            }
            float s = clamp(-e / 0.12, 0.0, 1.0);
            float slope = min((1.0 - s) / sqrt(max(1.0 - (1.0 - s) * (1.0 - s), 1e-4)), 3.0) * 0.9;
            vec3 ne = normalize(n + out2 * slope);
            if (MAT == 2) {
                float open = uEyes.z * clamp(1.0 - 2.0 * uMood.x, 0.0, 1.0);
                vec2 r = ES * vec2(0.14 * ESX * mix(1.12, 1.0, open), 0.29 * mix(0.12, 1.0, open));
                vec2 u = ep * ES / r;
                vec3 nb = worldDir(normalize(vec3(u / r, sqrt(max(1.0 - dot(u, u), 0.0)) / 0.1)));
                ne = normalize(mix(ne, nb, smoothstep(0.3, 0.7, open) * (1.0 - smoothstep(0.2, 0.6, uEyes.w)) * step(dot(u, u), 1.1)));
            }
            vec3 black = light(ne, v, vec3(0.004), MAT == 2 ? 0.55 : 1.0, 1.0) * mix(0.5, 1.0, s);
            vec3 h = normalize(normalize(vec3(-0.55, 0.80, 0.55)) + v);
            black += vec3(1.0) * smoothstep(0.93, 0.97, dot(ne, h)) * 1.4;
            col = mix(col, black, inside);
        }
    }
    return MAT == 3 ? softClip(col) : col;
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
    MAT = MAT_ID;
    KEY = normalize(dot(uKey.xyz, uKey.xyz) > 0.0 ? uKey.xyz : vec3(-0.55, 0.80, 0.55));
    KEYC = 1.0 + uKeyCol.rgb; RIM = 1.0 + uKey.w; AMB = 1.0 + uKeyCol.w;
    float bottom = 0.45;
    BODY = vec3(1.0, 0.82, 0.80); ROUND = 0.52; PUFF = 0.08; EYE = vec2(0.36, 0.15); ES = 1.0; ESX = 1.0;
    LEG = vec3(0.25, 0.58, 0.42); LEGT = -0.035; ARM = vec3(0.37, 0.26, 0.28); LEGK = 0.13; TAPER = 0.035; SHOULDER = vec3(0.0, -0.1, 0.16);
    ARMYAW = 0.5; REARX = LEG.y; SPLAY = 0.0;
    if (MAT == 0) {
        BODY = vec3(1.04, 0.86, 0.82); ROUND = 0.5; PUFF = 0.07; EYE = vec2(0.37, 0.1); bottom = 0.37;
        LEG = vec3(0.27, 0.6, 0.4); LEGT = -0.03; LEGK = 0.08; REARX = 0.46; SPLAY = 0.12; TAPER = 0.025;
        ARM = vec3(0.47, 0.28, 0.31); ARMYAW = 0.45; SHOULDER = vec3(-0.08, 0.0, 0.26);
    }
    if (MAT == 1) {
        BODY = vec3(1.0, 0.88, 0.80); ROUND = 0.52; PUFF = 0.13; EYE = vec2(0.31, 0.2); ES = 1.16; bottom = 0.5;
        LEG = vec3(0.27, 0.6, 0.4); LEGT = -0.03; ARM = vec3(0.46, 0.28, 0.28); LEGK = 0.035; TAPER = 0.12; SHOULDER = vec3(-0.04, -0.12, 0.3);
    } else if (MAT == 2) {
        BODY = vec3(0.96, 0.9, 0.84); ROUND = 0.46; PUFF = 0.07; EYE = vec2(0.33, 0.31); ES = 0.78; ESX = 0.95;
        LEG = vec3(0.33, 0.55, 0.42); LEGT = 0.0; LEGK = 0.11; ARM = vec3(0.6, 0.3, 0.32); TAPER = 0.12; SHOULDER = vec3(-0.08, 0.16, 0.26); ARMYAW = 0.35; bottom = 0.34;
    } else if (MAT == 3) {
        BODY = vec3(1.06, 0.92, 0.76); ROUND = 0.46; PUFF = 0.12; EYE = vec2(0.34, 0.1); ES = 1.12; ESX = 0.82; bottom = 0.42;
        LEG = vec3(0.23, 0.68, 0.4); LEGT = -0.08; ARM = vec3(0.42, 0.22, 0.24); TAPER = 0.0; SHOULDER = vec3(0.0, -0.08, 0.2);
    }
    BODY_Y = bottom + BODY.y;

    vec2 fc = gl_FragCoord.xy - vec2(0.0, uStage.y);
    vec2 sp = (2.0 * fc - uRes) / uRes.y;
    vec3 ta = vec3(0.0, 1.27, 0.0);
    vec3 ro = ta + 9.5 * vec3(sin(-0.17) * cos(0.18), sin(0.18), cos(-0.17) * cos(0.18));
    vec3 ww = normalize(ta - ro), uu = normalize(cross(ww, vec3(0.0, 1.0, 0.0))), vv = cross(uu, ww);
    vec3 rd = normalize(sp.x * uu + sp.y * vv + FL * ww);
    float pix = 2.0 / (uRes.y * FL);
    float logical = uRes.y / 250.0;

    vec3 col = vec3(0.0);
    float cover = 0.0, tHit = 1e9, ink = 0.0;
    vec3 bc = vec3(0.0, 1.0 + uBody.y, 0.0);
    vec3 oc = ro - bc;
    float b = dot(oc, rd), disc = b * b - dot(oc, oc) + 2.3 * 2.3;
    if (disc > 0.0) {
        float t = max(-b - sqrt(disc), 0.0), tEnd = -b + sqrt(disc);
        float best = 1e9, tBest = t;
        vec4 bestPart = vec4(1e9), tPart = vec4(0.0);
        bool hit = false;
        for (int i = 0; i < 96; i++) {
            vec4 g = parts(local(ro + rd * t));
            float d = scaleD(join(g));
            float px = d / (pix * t);
            if (MAT == 1) { vec4 gp = g * (0.9 / (pix * t)); tPart = mix(tPart, vec4(t), vec4(lessThan(gp, bestPart))); bestPart = min(bestPart, gp); }
            if (px < best) { best = px; tBest = t; }
            if (d < 0.25 * pix * t) { best = 0.0; tBest = t; hit = true; break; }
            t += d;
            if (t > tEnd) break;
        }
        if (!hit && MAT != 0 && best < 12.0) {
            float h = best * pix * tBest;
            for (int k = 0; k < 4; k++) {
                for (int j = -1; j <= 1; j += 2) {
                    float tt = tBest + float(j) * h;
                    float px = map(ro + rd * tt) / (pix * tt);
                    if (px < best) { best = px; tBest = tt; }
                }
                h *= 0.5;
            }
        }
        float inkW = 3.2 * logical;
        if (MAT == 1) {
            cover = clamp(1.0 - (best - inkW), 0.0, 1.0);
            if (cover > 0.0) {
                vec3 p = ro + rd * tBest;
                col = shade(p, rd, pix * tBest, ink);
                ink = max(ink, smoothstep(0.0, 1.0, best));
                if (hit) {
                    vec4 line = smoothstep(inkW * 0.7, inkW * 0.7 - 1.0, bestPart) * step(0.04, tBest - tPart);
                    ink = max(ink, max(max(line.x, line.y), max(line.z, line.w)));
                }
                col = mix(col, lin(vec3(0.06, 0.03, 0.02)), ink);
            }
        } else if (MAT == 2) {
            vec3 p = ro + rd * tBest;
            float foot = pix * tBest;
            float len = 0.02 / foot;
            float tuft = smoothstep(0.15, 0.85, vnoise(LOCKS * local(p)) * 0.72 + 0.14);
            cover = clamp(len * (0.15 + 0.85 * tuft * tuft) - best + 0.5, 0.0, 1.0);
            FUZZ = clamp(best / len, 0.0, 1.0);
            if (cover > 0.0) col = shade(p, rd, foot, ink);
        } else {
            cover = clamp(1.0 - best, 0.0, 1.0);
            if (cover > 0.0) col = shade(ro + rd * tBest, rd, pix * tBest, ink);
        }
        if (cover > 0.0) tHit = tBest;
    }
    float shadow = 0.0;
    float edge = smoothstep(-uStage.y, -uStage.y + (uShadow.z > 0.0 ? uShadow.z : 30.0 * logical), fc.y)
        * smoothstep(0.0, 16.0 * logical, fc.x) * smoothstep(0.0, 16.0 * logical, uRes.x - fc.x);
    if (rd.y < 0.0) {
        float tf = -ro.y / rd.y;
        vec3 f = ro + rd * tf;
        if ((tf < tHit || cover < 1.0) && length(f.xz) < 2.6) {
            float o = 0.0, s = 1.0, hs = uShadow.x > 0.0 ? 0.45 : 1.0;
            for (int i = 0; i < 5; i++) {
                float h = (0.05 + 0.17 * float(i)) * hs;
                o += max(h - map(f + vec3(0.0, h, 0.0)), 0.0) * s;
                s *= 0.7;
            }
            shadow = clamp(o * 0.85, 0.0, 0.82);
        }
        if (tf < tHit || cover < 1.0) shadow = max(shadow, max(castShadow(f, bc, 2.3), pool(f, vec2(0.0), vec2(0.95, 0.75))) * edge);
    }
    vec3 rcol = vec3(0.0);
    float ra = 0.0;
    if (uStage.x > 0.0 && rd.y < 0.0) {
        float tf = -ro.y / rd.y;
        vec3 f = ro + rd * tf, r = vec3(rd.x, -rd.y, rd.z);
        vec3 of = f - bc;
        float rb = dot(of, r), rdisc = rb * rb - dot(of, of) + 2.3 * 2.3;
        if ((tf < tHit || cover < 1.0) && rdisc > 0.0) {
            float t = max(-rb - sqrt(rdisc), 0.0), tEnd = -rb + sqrt(rdisc);
            float best = 1e9, tBest = t;
            for (int i = 0; i < 64; i++) {
                float d = map(f + r * t);
                float px = d / (pix * (tf + t));
                if (px < best) { best = px; tBest = t; }
                if (d < 0.25 * pix * (tf + t)) { best = 0.0; tBest = t; break; }
                t += d;
                if (t > tEnd) break;
            }
            vec3 p = f + r * tBest;
            float blur = 1.0 + uStage.z * p.y / (pix * (tf + tBest));
            ra = clamp(1.0 - best / blur, 0.0, 1.0) * uStage.x * exp(-p.y * uStage.w);
            ra *= edge;
            float ink2;
            REFL = 1.0;
            if (ra > 0.0) rcol = shade(p, r, pix * (tf + tBest), ink2);
        }
        vec3 lf = local(f);
        vec2 fq = pow(abs(lf.xz) / BODY.xz, vec2(4.0));
        float fp = 1.0 - smoothstep(0.45, 0.95 + 0.5 * uBody.y, pow(fq.x + fq.y, 0.25));
        float rv = fp * uStage.x * BELLY * edge * (1.0 - ra / uStage.x) / (1.0 + 3.0 * uBody.y);
        if ((tf < tHit || cover < 1.0) && rv > 0.002) {
            LOW = 1.0;
            rv *= exp(-(uBody.y + (BODY_Y - BODY.y) * uBody.x) * uStage.w * 0.5);
            vec3 bcol = light(vec3(0.0, -1.0, 0.0), vec3(0.0, -1.0, 0.0), vec3(0.95, 0.2, 0.011) * 0.62 * 0.6, 0.0, 1.0);
            rcol = (rcol * ra + bcol * rv) / max(ra + rv, 1e-4);
            ra += rv;
        }
    }
    float a2 = ra + shadow * (1.0 - ra);
    float a = cover + a2 * (1.0 - cover);
    col = (col * cover + rcol * ra * (1.0 - cover)) / max(a, 1e-4);
    outColor = vec4(pow(clamp(col, 0.0, 1.0), vec3(1.0 / 2.2)), a);
}
