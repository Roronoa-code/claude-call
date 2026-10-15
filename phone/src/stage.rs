//! PREVIEW (branch preview/home-stage, not for main): the world Clawd and Null stand in on the home screen, one of
//! four stages picked at run time with `adb shell setprop debug.callme.stage <n>` and a restart of the app (0, or
//! unset: the stage as it shipped). Each stage is a backdrop (ui/stage/<name>.png, drawn by art/live3d/stages.py,
//! which has these same numbers) plus how the shaders light the pair in it: their scale, the key light and its cast
//! shadow, and a floor that mirrors them.
use crate::cast::{Uniforms, Who};

pub struct Stage {
    pub k: f32,     // the pair's scale
    pub floor: f32, // logical px under the feet line their reflections and shadows may reach
    pub stage: [f32; 4], // uStage: reflection strength, (px below the picture), blur, fade with height
    pub key: [f32; 4],   // uKey: key light direction (in the backdrop's square-on view), rim
    pub key_col: [f32; 4],
    pub shadow: [f32; 4], // uShadow: strength, softness, fade at the floor's front (logical px), bounce from the floor
}

const fn s(k: f32, floor: f32, stage: [f32; 4], key: [f32; 4], key_col: [f32; 4], shadow: [f32; 4]) -> Stage {
    Stage { k, floor, stage, key, key_col, shadow }
}

pub static STAGES: [Stage; 5] = [
    s(1.0, 0.0, [0.0; 4], [0.0; 4], [0.0; 4], [0.0; 4]),
    // spot: one cone of light straight down on a glossy black floor
    s(1.08, 110.0, [0.32, 0.0, 0.08, 3.0], [0.0, 1.0, 0.15, 0.2], [-0.05, -0.12, -0.2, -0.45], [0.3, 1.0, 30.0, 0.35]),
    // desk: a walnut desk under a warm lamp; its front edge cuts the reflections
    s(1.0, 60.0, [0.32, 0.0, 0.08, 2.2], [-0.4, 0.7, 0.42, 0.3], [0.1, -0.05, -0.2, -0.3], [0.5, 4.0, 3.0, 0.2]),
    // moon: a night room, moonlight through a window over the floor and up the wall
    s(0.9, 110.0, [0.30, 0.0, 0.08, 2.6], [0.22, 0.72, 0.66, -0.2], [-0.3, -0.12, 0.1, -0.35], [0.6, 6.0, 30.0, 0.15]),
    // record: a black vinyl record floating in the dark
    s(0.84, 70.0, [0.0; 4], [-0.3, 1.0, 0.45, 0.6], [0.04, -0.02, -0.08, -0.3], [0.45, 4.0, 20.0, 0.1]),
];

/// The stage picked with `debug.callme.stage`, or 1 (Spotlight, Mani's pick).
#[cfg(target_os = "android")]
pub fn pick() -> usize {
    prop_or(c"debug.callme.stage", 1, STAGES.len() - 1)
}

/// A preview's number from an Android system property (0 when unset), at most `max`.
#[cfg(target_os = "android")]
pub fn prop(name: &std::ffi::CStr, max: usize) -> usize {
    prop_or(name, 0, max)
}

/// A preview's number from an Android system property (`default` when unset), at most `max`.
#[cfg(target_os = "android")]
pub fn prop_or(name: &std::ffi::CStr, default: usize, max: usize) -> usize {
    let v = prop_text(name);
    if v.is_empty() { default } else { v.trim().parse::<usize>().unwrap_or(0).min(max) }
}

/// An Android system property's text ("" when unset).
#[cfg(target_os = "android")]
pub fn prop_text(name: &std::ffi::CStr) -> String {
    unsafe extern "C" {
        fn __system_property_get(name: *const std::ffi::c_char, value: *mut std::ffi::c_char) -> std::ffi::c_int;
    }
    let mut buf = [0 as std::ffi::c_char; 92]; // PROP_VALUE_MAX
    let n = unsafe { __system_property_get(name.as_ptr(), buf.as_mut_ptr()) };
    if n <= 0 { String::new() } else { unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned() }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast::Mat;

    #[test]
    fn floors_reach_the_same_line_under_both() {
        for st in &STAGES[1..] {
            let (c, n) = (st.below(Who::Clawd(Mat::Vinyl)), st.below(Who::Null));
            assert!((c + 28.75 - (n + 35.4)).abs() < 0.01 || c == 0.0 || n == 0.0);
        }
        // the shipped stage draws no floor
        assert_eq!(STAGES[0].below(Who::Null), 0.0);
    }
}
