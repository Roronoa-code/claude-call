use crate::cast::{Uniforms, Who};

pub struct Stage {
    pub k: f32,     // the pair's scale
    pub floor: f32, // logical px under the feet line their reflections and shadows may reach
    pub stage: [f32; 4], // uStage: reflection strength, (px below the picture), blur, fade with height
    pub key: [f32; 4],   // uKey: key light direction (in the backdrop's square-on view), rim
    pub key_col: [f32; 4],
    pub shadow: [f32; 4], // uShadow: strength, softness, fade at the floor's front (logical px), bounce from the floor
}

// spot: one cone of light straight down on a glossy black floor
pub static SPOT: Stage = Stage {
    k: 1.08,
    floor: 110.0,
    stage: [0.32, 0.0, 0.08, 3.0],
    key: [0.0, 1.0, 0.15, 0.2],
    key_col: [-0.05, -0.12, -0.2, -0.45],
    shadow: [0.65, 3.0, 30.0, 0.35],
};

// Each character's camera is turned round the floor's vertical from the backdrop's square-on view (clawd_cast.frag,
// null.frag), so the key light is turned the same way to fall on them from where it falls on the backdrop.
fn turn(v: [f32; 4], a: f32) -> [f32; 4] {
    let (s, c) = a.sin_cos();
    [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c, v[3]]
}

/// A picture's height and its feet's height from its top, in logical px at home size (toys.slint).
fn picture(who: Who) -> (f32, f32) {
    if who == Who::Null { (230.0, 194.6) } else { (250.0, 221.25) }
}

impl Stage {
    /// Logical px of floor drawn under `who`'s picture (whose bottom is already a little under the feet), at k = 1.
    pub fn below(&self, who: Who) -> f32 {
        let (h, feet) = picture(who);
        (self.floor - (h - feet)).max(0.0)
    }

    /// The stage's uniforms for `who`, whose picture is `res_h` px tall in a `view_h` px viewport, on a screen of
    /// `sf` px per logical px, with its lights at `lit` (1, or less for the one the Call button doesn't ask for: in a
    /// world, dimmed in its light rather than see-through).
    pub fn uniforms(&self, who: Who, res_h: i32, view_h: i32, sf: f32, lit: f32) -> Uniforms {
        let az = if who == Who::Null { 0.24 } else { -0.17 };
        let [a, _, b, c] = self.stage;
        let [st, soft, fade, bounce] = self.shadow;
        let [r, g, bl, amb] = self.key_col.map(|v| (1.0 + v) * lit - 1.0); // the shaders add these to 1
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
