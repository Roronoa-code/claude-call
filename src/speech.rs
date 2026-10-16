use crate::{call::call, voice};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
use tokio::sync::watch;

const PARTIAL_WINDOW: usize = 28 * 16_000; // Whisper's window is 30 s
const END_SLACK_SAMPLES: usize = 160;

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
    tick: watch::Sender<u32>,
}
impl UttCell {
    fn bump(&self) {
        self.tick.send_modify(|x| *x = x.wrapping_add(1));
    }
    async fn idle(&self) {
        let mut rx = self.tick.subscribe();
        while self.m.lock().unwrap().busy {
            let _ = rx.changed().await;
        }
    }
}
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
    let piece = buf.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    cell.m.lock().unwrap().pieces.insert(off, piece);
    cell.bump();
    cell
}

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
    let covering = || cell.m.lock().unwrap().partial.clone().filter(|p| p.from == 0 && p.up_to + END_SLACK_SAMPLES >= end);
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


pub fn without_tags(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('[') {
        let tag = rest[i + 1..].find(']').map(|j| &rest[i + 1..i + 1 + j]);
        out.push_str(&rest[..i]);
        match tag.filter(|t| t.starts_with(|c: char| c.is_ascii_alphabetic()) && t.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ' ')) {
            Some(t) => rest = &rest[i + t.len() + 2..],
            None => {
                out.push('[');
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whisper, prompted with Claude's line, sometimes "hears" that line in near-silence.
fn echoes(heard: &str, say: &str) -> bool {
    let words = |t: &str| -> Vec<String> {
        t.to_lowercase().split(|c: char| !(c.is_alphanumeric() || c == '\'')).filter(|w| !w.is_empty()).map(String::from).collect()
    };
    let (h, s) = (words(heard), words(say));
    h.len() >= 4 && s.windows(h.len()).any(|w| w == h.as_slice())
}
