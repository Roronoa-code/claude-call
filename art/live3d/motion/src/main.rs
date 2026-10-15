//! Dumps the cast's motion as uniform timelines (JSON) for the offline renderer: a 27 s life story per character at
//! 30 fps, and one settled frame per pose for the pose sheets. Usage: cast-motion <out dir>
#[allow(dead_code)] // moving() is for the app, not the dump
#[path = "../../../../phone/src/cast.rs"]
mod cast;
use cast::{Cast, Mat, State, Uniforms, Who};
use std::fmt::Write;

fn json(u: &Uniforms) -> String {
    let fields: Vec<String> = u.iter().map(|(n, v)| format!("\"{n}\":[{:.5},{:.5},{:.5},{:.5}]", v[0], v[1], v[2], v[3])).collect();
    format!("{{{}}}", fields.join(","))
}

/// The story: idle with a tap and a finger to follow, thinking, talking, a happy hop, idle, falling asleep, woken by a tap.
fn story(who: Who, seed: u64) -> String {
    let mut c = Cast::new(who, seed);
    let mut out = String::from("[");
    let mut now = 0.0f32;
    let mut frame = 0;
    let events: [(f32, &dyn Fn(&mut Cast, f32)); 8] = [
        (3.5, &|c, t| c.hop(t)),
        (8.0, &|c, _| c.set_state(State::Think)),
        (11.5, &|c, _| c.set_state(State::Talk)),
        (15.5, &|c, _| c.set_state(State::Happy)),
        (17.3, &|c, _| c.set_state(State::Idle)),
        (19.0, &|c, _| c.set_state(State::Sleep)),
        (24.5, &|c, t| c.hop(t)),
        (99.0, &|_, _| {}),
    ];
    let mut next = 0;
    while now < 27.0 {
        while now >= events[next].0 {
            events[next].1(&mut c, now);
            next += 1;
        }
        // 5.2 s to 7.2 s: a finger drags across in front of him, then lifts; the eyes track it, the body follows
        if (5.2..7.2).contains(&now) {
            let k = (now - 5.2) / 2.0;
            c.point(-170.0 + 340.0 * k, -30.0 + 60.0 * (k * 3.0).sin(), true, now);
        } else if (7.2..7.25).contains(&now) {
            c.point(170.0, 20.0, false, now);
        }
        c.tick(now);
        if frame % 2 == 0 {
            write!(out, "{{\"t\":{now:.4},\"u\":{}}},", json(&c.uniforms())).unwrap();
        }
        frame += 1;
        now = frame as f32 / 60.0;
    }
    out.pop();
    out + "]"
}

/// One frame per pose: idle, blink, think, talk, happy, sleep.
fn poses(who: Who, seed: u64) -> String {
    let settle = |st: State, secs: f32, pick: &dyn Fn(&Cast, &Uniforms) -> bool| {
        let mut c = Cast::new(who, seed);
        c.set_state(st);
        let mut now = 0.0;
        let mut best = None;
        while now < secs + 6.0 {
            now += 1.0 / 60.0;
            c.tick(now);
            let u = c.uniforms();
            if now >= secs && pick(&c, &u) {
                best = Some(u);
                break;
            }
            best.get_or_insert(u);
        }
        json(&best.unwrap())
    };
    let eyes = |u: &Uniforms| u.iter().find(|(n, _)| *n == "uEyes").unwrap().1;
    let calm = |u: &Uniforms| eyes(u)[2] > 0.99 && eyes(u)[0].abs() < 0.25 && eyes(u)[1].abs() < 0.2;
    let idle = settle(State::Idle, 2.0, &|_, u| calm(u));
    let blink = {
        // the most closed frame of the first blink
        let mut c = Cast::new(who, seed);
        let (mut now, mut best, mut low) = (0.0, String::new(), 2.0f32);
        while now < 9.0 {
            now += 1.0 / 60.0;
            c.tick(now);
            let o = eyes(&c.uniforms())[2];
            if o < low {
                (low, best) = (o, json(&c.uniforms()));
            } else if low < 0.3 {
                break;
            }
        }
        best
    };
    let think = settle(State::Think, 2.5, &|_, u| eyes(u)[2] > 0.99);
    let talk = settle(State::Talk, 1.5, &|_, u| {
        let m = u.iter().find(|(n, _)| *n == "uMood").unwrap().1;
        let h = u.iter().find(|(n, _)| *n == "uHands").map(|x| x.1[2]).unwrap_or(m[1]);
        h > 0.6 && eyes(u)[2] > 0.99
    });
    let happy = {
        let mut c = Cast::new(who, seed);
        let mut now = 0.0;
        while now < 1.0 {
            now += 1.0 / 60.0;
            c.tick(now);
        }
        c.set_state(State::Happy);
        while now < 1.55 {
            now += 1.0 / 60.0;
            c.tick(now);
        }
        json(&c.uniforms())
    };
    let sleep = settle(State::Sleep, 8.0, &|_, _| true);
    format!("{{\"idle\":{idle},\"blink\":{blink},\"think\":{think},\"talk\":{talk},\"happy\":{happy},\"sleep\":{sleep}}}")
}

fn main() {
    let dir = std::env::args().nth(1).expect("out dir");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, who, seed) in [
        ("null", Who::Null, 0x51ed_2701),
        ("haiku", Who::Clawd(Mat::Cartoon), 0x4a1c_0033),
        ("sonnet", Who::Clawd(Mat::Plush), 0x5a7e_1105),
        ("opus", Who::Clawd(Mat::Chrome), 0x0b05_9909),
        ("clawd", Who::Clawd(Mat::Vinyl), 0x9e37_79b9),
    ] {
        let shader = if who == Who::Null { "null" } else { "clawd_cast" };
        let s = format!("{{\"shader\":\"{shader}\",\"fps\":30,\"story\":{},\"poses\":{}}}", story(who, seed), poses(who, seed));
        std::fs::write(format!("{dir}/{name}.json"), s).unwrap();
        println!("{name}");
    }
}
