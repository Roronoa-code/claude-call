//! The cast's motion tests (cast.rs): smooth, alive, and each move comes back to rest.
use super::*;

fn run(c: &mut Cast, from: f32, to: f32, mut each: impl FnMut(&Cast)) {
    let mut now = from;
    while now < to {
        now += 1.0 / 60.0;
        c.tick(now);
        each(c);
    }
}

#[test]
fn nothing_jumps_between_frames() {
    // every uniform moves smoothly at 60 fps through every state change, a hop and sleep
    for who in [Who::Null, Who::Clawd(Mat::Cartoon), Who::Clawd(Mat::Plush), Who::Clawd(Mat::Chrome)] {
        let mut c = Cast::new(who, 11);
        let mut prev: Option<Uniforms> = None;
        let mut worst = 0.0f32;
        let mut t = 0.0;
        for (until, st) in [(4.0, State::Idle), (8.0, State::Think), (12.0, State::Talk), (14.0, State::Happy), (16.0, State::Idle), (24.0, State::Sleep), (28.0, State::Idle)] {
            c.set_state(st);
            if st == State::Idle && t > 1.0 {
                c.hop(t);
            }
            run(&mut c, t, until, |c| {
                let u = c.uniforms();
                if let Some(p) = &prev {
                    for ((name, a), (_, b)) in u.iter().zip(p) {
                        for k in 0..4 {
                            if *name == "uMood" && k == 3 {
                                continue; // the clock
                            }
                            // blinks and the hop itself are meant to be quick
                            // (a typing tap moves the hand only 0.05; a saccade and a syllable's mouth are quick darts)
                            let limit = if *name == "uEyes" && k == 2 { 0.75 } else if *name == "uBody" && k == 1 { 0.1 } else if *name == "uHands" && k < 2 { 0.5 } else if *name == "uEyes" || (*name == "uMood" && k == 1 && c.who != Who::Null) { 0.45 } else { 0.35 };
                            let d = (a[k] - b[k]).abs();
                            worst = worst.max(d / limit);
                            assert!(d < limit, "{who:?} {name}[{k}] jumped {d} at {:.2}s", c.t);
                        }
                    }
                }
                prev = Some(u);
            });
            t = until;
        }
        assert!(worst > 0.0);
    }
}

#[test]
fn idle_is_never_frozen_and_never_repeats() {
    let mut c = Cast::new(Who::Clawd(Mat::Cartoon), 5);
    let mut looks = vec![];
    let mut blinks = 0;
    let mut was_open = true;
    run(&mut c, 0.0, 30.0, |c| {
        let u = c.uniforms();
        looks.push(u[2].1[0]);
        let open = u[2].1[2] > 0.5;
        if was_open && !open {
            blinks += 1;
        }
        was_open = open;
    });
    assert!((4..=20).contains(&blinks), "{blinks} blinks in 30 s");
    // the gaze visits several places, and no two seconds look alike
    let spread = looks.iter().cloned().fold(f32::MIN, f32::max) - looks.iter().cloned().fold(f32::MAX, f32::min);
    assert!(spread > 0.6, "gaze spread {spread}");
    let a: Vec<f32> = looks[600..660].to_vec();
    let b: Vec<f32> = looks[1200..1260].to_vec();
    assert!(a.iter().zip(&b).any(|(x, y)| (x - y).abs() > 0.02));
}

#[test]
fn a_hop_lands_squashed_and_settles() {
    let mut c = Cast::new(Who::Clawd(Mat::Plush), 3);
    run(&mut c, 0.0, 0.5, |_| {});
    c.hop(0.5);
    let (mut top, mut low) = (0.0f32, 2.0f32);
    run(&mut c, 0.5, 3.5, |c| {
        top = top.max(c.y);
        low = low.min(c.squash.x);
    });
    assert!(top > 0.1 && top < 0.3, "hop {top}");
    assert!(low < 0.93, "landing squash {low}");
    assert!((c.squash.x - 1.0).abs() < 0.02 && c.y == 0.0);
}

#[test]
fn null_antenna_swings_after_a_hop_then_settles() {
    let mut c = Cast::new(Who::Null, 9);
    run(&mut c, 0.0, 1.0, |_| {});
    c.hop(1.0);
    let mut swing = 0.0f32;
    run(&mut c, 1.0, 2.0, |c| swing = swing.max(c.ant.2.x.abs() + c.ant.0.x.abs()));
    assert!(swing > 0.03, "antenna {swing}");
    run(&mut c, 2.0, 8.0, |_| {});
    assert!(c.ant.2.x.abs() < 0.01);
}

#[test]
fn sleep_closes_and_dims_then_a_tap_wakes() {
    let mut c = Cast::new(Who::Null, 4);
    c.set_state(State::Sleep);
    run(&mut c, 0.0, 9.0, |_| {});
    let u = c.uniforms();
    assert!(u[3].1[3] > 0.9 && u[5].1[3] < 0.45 && u[2].1[0] > 0.9, "{u:?}");
    c.hop(9.0);
    run(&mut c, 9.0, 12.0, |_| {});
    assert_eq!(c.state(), State::Idle);
    assert!(c.uniforms()[5].1[3] > 0.8);
}

#[test]
fn a_real_voice_moves_the_mouth_smoothly_and_nods_come_back() {
    let mut c = Cast::new(Who::Clawd(Mat::Vinyl), 2);
    c.set_state(State::Talk);
    let mut prev = 0.0f32;
    let mut most = 0.0f32;
    let mut t = 0.0;
    // a loud word, silence, a loud word: the mouth follows, never jumping
    for (until, level) in [(0.5, 0.7), (1.0, 0.0), (1.5, 0.9), (2.5, 0.0)] {
        c.set_voice(Some(level));
        run(&mut c, t, until, |c| {
            let m = c.uniforms()[3].1[1];
            assert!((m - prev).abs() < 0.45, "mouth jumped {prev} -> {m}");
            most = most.max(m);
            prev = m;
        });
        t = until;
    }
    assert!(most > 0.6 && prev < 0.05, "mouth {most} {prev}");
    let mut n = Cast::new(Who::Null, 3);
    n.attend(true);
    run(&mut n, 0.0, 1.0, |_| {});
    let rest = n.uniforms()[1].1[1];
    n.nod(1.0);
    let mut dip = 0.0f32;
    run(&mut n, 1.0, 1.5, |c| dip = dip.max(c.uniforms()[1].1[1] - rest));
    run(&mut n, 1.5, 3.0, |_| {});
    assert!(dip > 0.05 && (n.uniforms()[1].1[1] - rest).abs() < 0.05, "nod {dip}");
    assert_eq!(Who::named("claude sonnet-5.5"), Who::Clawd(Mat::Plush));
    assert_eq!(Who::named("Codex").face(), "null");
    assert_eq!(Who::named("Claude").face(), "clawd");
}
