use super::*;

const FRAME: f32 = 1.0 / 60.0;

fn settled_at(mode: usize) -> (Pair, f32) {
    let mut p = Pair::new(7);
    let mut now = 0.0;
    for _ in 0..30 {
        now += FRAME;
        p.tick(now, mode);
    }
    (p, now)
}

#[test]
fn snaps_to_the_nearest_mode() {
    assert_eq!(snap(1.4, 0.0), 1);
    assert_eq!(snap(1.6, 0.0), 2);
    assert_eq!(snap(1.3, 2.0), 2);
    assert_eq!(snap(2.7, -2.0), 2);
    assert_eq!(snap(-0.3, -5.0), 0);
    assert_eq!(snap(5.2, 3.0), 5);
}

#[test]
fn release_sets_the_mode_only_once_settled() {
    let (mut p, mut now) = settled_at(2);
    for f in 1..=30 {
        now += FRAME;
        p.drag(STEP * 1.3 * f as f32 / 30.0, now);
        p.tick(now, 2);
    }
    for _ in 0..12 {
        now += FRAME;
        p.tick(now, 2);
    }
    p.release(true, now);
    assert!(p.drum().1 > 0.0);
    let mut chosen = Vec::new();
    for _ in 0..120 {
        now += FRAME;
        chosen.extend(p.tick(now, 2));
        if chosen.is_empty() {
            assert!(p.drum().1 > 0.0);
        }
    }
    assert_eq!(chosen, [3]);
    for _ in 0..180 {
        now += FRAME;
        p.tick(now, 3);
    }
    assert_eq!(p.drum(), (3.0, 0.0));
    assert!(!p.turning());
}

#[test]
fn cancel_turns_back_without_choosing() {
    let (mut p, mut now) = settled_at(1);
    now += FRAME;
    p.drag(STEP * 2.0, now);
    p.release(false, now + 0.5);
    for _ in 0..240 {
        now += FRAME;
        assert_eq!(p.tick(now, 1), None);
    }
    assert_eq!(p.drum(), (1.0, 0.0));
}

#[test]
fn a_mode_changed_elsewhere_turns_the_drum() {
    let (mut p, mut now) = settled_at(2);
    now += FRAME;
    assert_eq!(p.tick(now, 4), None);
    let first = p.drum().0;
    assert!(first > 2.0 && first < 2.5);
    let mut peak = first;
    for _ in 0..240 {
        now += FRAME;
        assert_eq!(p.tick(now, 4), None);
        peak = peak.max(p.drum().0);
    }
    assert!(peak > 4.0, "small overshoot");
    assert_eq!(p.drum(), (4.0, 0.0));
}

#[test]
fn wheel_steps_one_mode_per_notch_and_stops_at_the_ends() {
    let (mut p, mut now) = settled_at(4);
    p.step(1);
    p.step(1);
    let mut chosen = None;
    for _ in 0..180 {
        now += FRAME;
        chosen = p.tick(now, 4).or(chosen);
    }
    assert_eq!(chosen, Some(5));
}

#[test]
fn dozes_after_two_idle_minutes_and_wakes_on_input() {
    let (mut p, mut now) = settled_at(2);
    while now < DOZE_AFTER + 2.0 {
        now += 0.05;
        p.tick(now, 2);
    }
    assert!(p.dozing());
    assert_eq!(p.clawd.state(), State::Sleep);
    p.active(now);
    now += FRAME;
    p.tick(now, 2);
    assert!(!p.dozing());
    assert_ne!(p.clawd.state(), State::Sleep);
}

#[test]
fn idle_life_plays_while_awake_but_not_while_hovering_call() {
    let (mut p, mut now) = settled_at(2);
    p.hover_call(true, now);
    for _ in 0..(60.0 / 0.05) as usize {
        now += 0.05;
        p.tick(now, 2);
        p.active(now);
    }
    assert!(p.script.is_empty());
    p.hover_call(false, now);
    let mut played = false;
    for _ in 0..(100.0 / 0.05) as usize {
        now += 0.05;
        p.tick(now, 2);
        p.active(now);
        played |= !p.script.is_empty();
    }
    assert!(played);
}
