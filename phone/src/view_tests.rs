use super::*;

fn fits(p: &[String]) -> bool {
    p.iter().all(|s| s.chars().count() <= PHRASE_CHARS || words(s).len() == 1)
}

#[test]
fn short_sentences_stay_whole() {
    assert_eq!(phrases("Sure, I found it. I'll point it back at the fast drive."), ["Sure, I found it.", "I'll point it back at the fast drive."]);
}

#[test]
fn long_sentences_break_at_clauses() {
    let p = phrases("The laptop rebuilds every crate, because the build folder sits on the USB disk.");
    assert_eq!(p[0], "The laptop rebuilds every crate,");
    assert!(p.len() >= 2 && fits(&p), "{p:?}");
}

#[test]
fn short_clauses_merge_up_to_the_limit() {
    let p = phrases("Okay, so, right, I checked the logs, the cache and the build folder on the laptop and the phone.");
    assert_eq!(p[0], "Okay, so, right, I checked the logs,");
    assert!(fits(&p), "{p:?}");
}

#[test]
fn unbroken_runs_split_evenly() {
    let p = phrases("because the build folder sits on the USB disk.");
    assert_eq!(p, ["because the build folder", "sits on the USB disk."]);
    let long = "and then it walks every single file in the folder and checks each one against the cache it keeps on the fast drive";
    let p = phrases(long);
    assert!(fits(&p), "{p:?}");
    let sizes: Vec<usize> = p.iter().map(|s| s.chars().count()).collect();
    assert!(sizes.iter().max().unwrap() - sizes.iter().min().unwrap() <= 12, "{sizes:?}");
}

#[test]
fn no_word_is_lost_or_moved() {
    for text in [
        "Hi.",
        "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen",
        "A — then a long tail of words that goes on well past the limit — and more after the dash, too!",
        "supercalifragilisticexpialidocious-and-then-some-more-letters-without-spaces ok",
        "  ",
    ] {
        let p = phrases(text);
        assert!(!p.is_empty());
        assert_eq!(p.join(" "), words(text).join(" "));
        assert!(fits(&p), "{p:?}");
    }
}

#[test]
fn progress_moves_phrase_by_phrase() {
    let parts = phrases("Sure, I found it. because the build folder sits on the USB disk.");
    assert_eq!(spoken_at(&parts, "Sure, I"), Spoken { phrase: 0, words: 2 });
    assert_eq!(spoken_at(&parts, "Sure, I found it."), Spoken { phrase: 1, words: 0 });
    assert_eq!(spoken_at(&parts, "Sure, I found it. because the build folder sits"), Spoken { phrase: 2, words: 1 });
}
