use crate::cast::{Uniforms, Who};

pub struct Stage {
    pub k: f32,
    pub floor: f32,
    pub stage: [f32; 4],
    pub key: [f32; 4],
    pub key_col: [f32; 4],
    pub shadow: [f32; 4],
}

pub static SPOT: Stage = Stage {
    k: 1.08,
    floor: 110.0,
    stage: [0.32, 0.0, 0.08, 3.0],
    key: [0.0, 1.0, 0.15, 0.2],
    key_col: [-0.05, -0.12, -0.2, -0.45],
    shadow: [0.65, 3.0, 30.0, 0.35],
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
        let [a, _, b, c] = self.stage;
        let [st, soft, fade, bounce] = self.shadow;
        let [r, g, bl, amb] = self.key_col.map(|v| (1.0 + v) * lit - 1.0);
        let mut key = turn(self.key, az);
        key[3] = (1.0 + key[3]) * lit - 1.0;
        vec![
            ("uStage", [a, (view_h - res_h) as f32, b, c]),
            ("uKey", key),
            ("uKeyCol", [r, g, bl, amb]),
            ("uShadow", [st, soft, fade * self.k * sf, bounce * lit]),
        ]
    }
}
