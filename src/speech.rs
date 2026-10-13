//! What the user says arrives in pieces while they talk (16 kHz mono 16-bit PCM, POST /listen), so live captions,
//! the early end of their turn and the final transcript all work on audio this PC already has.
use crate::{call::call, voice};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
use tokio::sync::watch;

const PARTIAL_WINDOW: usize = 28 * 16_000; // live captions look at the last 28 s (Whisper's window is 30 s)

#[derive(Clone, Serialize)]
pub struct Partial {
    text: String,
    from: usize,
    #[serde(rename = "upTo")]
    up_to: usize,
}
#[derive(Default)]
struct Utt {
    pieces: BTreeMap<usize, Vec<i16>>,
    partial: Option<Partial>,
    busy: bool,
}
pub struct UttCell {
    m: Mutex<Utt>,
    tick: watch::Sender<u32>, // bumped when a piece arrives or a live transcript finishes
}
impl UttCell {
    fn bump(&self) {
        self.tick.send_modify(|x| *x = x.wrapping_add(1));
    }
    /// Waits until no live transcript is being made.
    async fn idle(&self) {
        let mut rx = self.tick.subscribe();
        while self.m.lock().unwrap().busy {
            let _ = rx.changed().await;
        }
    }
}
/// Clears `busy` however the live transcript ends (done, failed, or the phone's request went away).
struct Busy<'a>(&'a UttCell);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.m.lock().unwrap().busy = false;
        self.0.bump();
    }
}

pub static UTTS: LazyLock<Mutex<(HashMap<String, Arc<UttCell>>, VecDeque<String>)>> = LazyLock::new(Default::default);

pub fn take_audio(u: &str, off: usize, buf: &[u8]) -> Arc<UttCell> {
    let cell = {
        let mut utts = UTTS.lock().unwrap();
        let (map, order) = &mut *utts;
        if !map.contains_key(u) {
            map.insert(u.into(), Arc::new(UttCell { m: Default::default(), tick: watch::channel(0).0 }));
            order.push_back(u.into());
            if order.len() > 6 {
                let old = order.pop_front().unwrap();
                map.remove(&old);
            }
        }
        map[u].clone()
    };
    // Keep even an empty final piece: its offset tells us earlier audio is still missing.
    let piece = buf.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    cell.m.lock().unwrap().pieces.insert(off, piece);
    cell.bump();
    cell
}

/// The utterance so far as float PCM, and whether it has no gaps (a piece can still be in flight).
fn utt_pcm(u: &Utt) -> (Vec<f32>, bool) {
    let (mut len, mut whole) = (0, true);
    for (&o, p) in &u.pieces {
        whole &= o == len;
        len = len.max(o + p.len());
    }
    let mut pcm = vec![0f32; len];
    for (&o, p) in &u.pieces {
        for (i, &s) in p.iter().enumerate() {
            pcm[o + i] = s as f32 / 32768.0;
        }
    }
    (pcm, whole)
}

/// A live transcript of the utterance so far; one at a time (a request during one gets its result).
pub async fn partial(cell: &UttCell) -> Option<Partial> {
    let mine = {
        let mut u = cell.m.lock().unwrap();
        if u.busy {
            None
        } else {
            let (pcm, whole) = utt_pcm(&u);
            if !whole {
                return None;
            }
            u.busy = true;
            Some(pcm)
        }
    };
    let Some(pcm) = mine else {
        cell.idle().await;
        return cell.m.lock().unwrap().partial.clone();
    };
    let up_to = pcm.len();
    let _busy = Busy(cell);
    let from = up_to.saturating_sub(PARTIAL_WINDOW);
    let say = call().say.clone();
    let text = voice::hear_quick(&pcm[from..], &say).await?;
    let p = Partial { text: if voice::is_noise(&text) || echoes(&text, &say) { String::new() } else { text }, from, up_to };
    cell.m.lock().unwrap().partial = Some(p.clone());
    Some(p)
}

/// The user finished talking: wait for any late pieces, then the final transcript (reusing the live one if it
/// covers the whole turn). Returns (seconds of audio, text, reused).
pub async fn final_transcript(cell: &UttCell, end: Option<usize>) -> Result<(f32, String, bool), String> {
    let mut rx = cell.tick.subscribe();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let pcm = loop {
        let (pcm, whole) = utt_pcm(&cell.m.lock().unwrap());
        if whole {
            break pcm;
        }
        if tokio::time::timeout_at(deadline, rx.changed()).await.is_err() {
            return Err("incomplete speech audio".into());
        }
    };
    let end = end.unwrap_or(pcm.len());
    // A rolling caption may omit the start of a long reply; only reuse one covering the whole turn.
    let covering = || cell.m.lock().unwrap().partial.clone().filter(|p| p.from == 0 && p.up_to + 160 >= end);
    let live = match covering() {
        Some(p) => Some(p),
        None => {
            cell.idle().await;
            covering()
        }
    };
    let secs = pcm.len() as f32 / 16_000.0;
    Ok(match live {
        Some(p) => (secs, p.text, true),
        None => {
            let say = call().say.clone();
            let heard = voice::hear(&pcm, &say).await;
            (secs, if echoes(&heard, &say) { String::new() } else { heard }, false)
        }
    })
}


/// Whisper, prompted with Claude's line, sometimes "hears" that line in near-silence (or Claude's own voice leaks
/// back in). Four or more of its words, in order, are never the user.
fn echoes(heard: &str, say: &str) -> bool {
    let words = |t: &str| -> Vec<String> {
        t.to_lowercase().split(|c: char| !(c.is_alphanumeric() || c == '\'')).filter(|w| !w.is_empty()).map(String::from).collect()
    };
    let (h, s) = (words(heard), words(say));
    h.len() >= 4 && s.windows(h.len()).any(|w| w == h.as_slice())
}

#[cfg(test)]
mod tests {
    #[test]
    fn echo_of_claudes_line() {
        let say = "Great. So the plan is to move the settings into a side panel, and rename a few buttons.";
        assert!(super::echoes("So the plan is to move the settings into a side panel.", say));
        assert!(!super::echoes("Yes, rename them.", say));
        assert!(!super::echoes("No, keep the old buttons please", say));
    }
}
