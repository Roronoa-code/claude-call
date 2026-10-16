use crate::cast::{Uniforms, Who};

pub struct Reflection {
    pub strength: f32,
    pub blur: f32,
    pub fade: f32,
}

pub struct Light {
    pub dir: [f32; 3],
    pub rim: f32,
}

pub struct Tint {
    pub rgb: [f32; 3],
    pub ambient: f32,
}

impl Tint {
    pub fn to_vec4(&self) -> [f32; 4] {
        let [r, g, b] = self.rgb;
        [r, g, b, self.ambient]
    }
}

pub struct Shadow {
    pub strength: f32,
    pub softness: f32,
    pub fade: f32,
    pub bounce: f32,
}

pub struct Stage {
    pub k: f32,
    pub floor: f32,
    pub reflection: Reflection,
    pub key: Light,
    pub key_col: Tint,
    pub shadow: Shadow,
}

pub static SPOT: Stage = Stage {
    k: 1.08,
    floor: 110.0,
    reflection: Reflection { strength: 0.32, blur: 0.08, fade: 3.0 },
    key: Light { dir: [0.0, 1.0, 0.15], rim: 0.2 },
    key_col: Tint { rgb: [-0.05, -0.12, -0.2], ambient: -0.45 },
    shadow: Shadow { strength: 0.65, softness: 3.0, fade: 30.0, bounce: 0.35 },
};

fn turn(v: [f32; 4], a: f32) -> [f32; 4] {
    let (s, c) = a.sin_cos();
    [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c, v[3]]
}

fn picture(who: Who) -> (f32, f32) {
    if who == Who::Null { (230.0, 194.6) } else { (250.0, 221.25) }
}

impl Stage {
    pub fn below(&self, who: Who) -> f32 {
        let (h, feet) = picture(who);
        (self.floor - (h - feet)).max(0.0)
    }

    pub fn uniforms(&self, who: Who, res_h: i32, view_h: i32, sf: f32, lit: f32) -> Uniforms {
        let az = if who == Who::Null { 0.24 } else { -0.17 };
        let Reflection { strength, blur, fade } = self.reflection;
        let sh = &self.shadow;
        let [r, g, bl, amb] = self.key_col.to_vec4().map(|v| (1.0 + v) * lit - 1.0);
        let [x, y, z] = self.key.dir;
        let mut key = turn([x, y, z, self.key.rim], az);
        key[3] = (1.0 + key[3]) * lit - 1.0;
        vec![
            ("uStage", [strength, (view_h - res_h) as f32, blur, fade]),
            ("uKey", key),
            ("uKeyCol", [r, g, bl, amb]),
            ("uShadow", [sh.strength, sh.softness, sh.fade * self.k * sf, sh.bounce * lit]),
        ]
    }
}
