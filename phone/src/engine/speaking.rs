use super::*;

impl Engine {
    pub(super) async fn get_state(&self, v: i64) -> Option<Value> {
        let r = HTTP.get(format!("{}&v={v}", self.api("state"))).timeout(Duration::from_secs(30)).send().await.ok()?;
        r.json().await.ok()
    }

    pub(super) fn started(&self, s: &mut Inner) {
        let now = self.now();
        let Some(l) = &mut s.line else { return };
        if l.started {
            return;
        }
        l.started = true;
        l.start_at = now;
        let (id, text) = (l.id, l.text.clone());
        s.view.claude(id, &text);
    }

    pub fn line_progress(&self, s: &mut Inner) {
        let now = self.now();
        let Some(l) = &s.line else { return };
        if !l.started {
            return;
        }
        let ms = match l.start_frame {
            Some(f) if l.rate > 0 => {
                let (played, _) = s.player.position();
                (played.saturating_sub(f)) as f64 * 1000.0 / l.rate as f64
            }
            _ => now - l.start_at,
        };
        let len = l.text.chars().count() as f64;
        let total = if l.dur > 0.0 { l.dur } else { (len * MS_PER_CHAR).max(ms / 0.95) };
        let k = ((ms / total).min(1.0) * len).ceil() as usize;
        let chars: Vec<char> = l.text.chars().collect();
        let end = (k..chars.len()).find(|&i| chars[i] == ' ');
        let shown: String = match end {
            Some(e) => chars[..e].iter().collect(),
            None => l.text.clone(),
        };
        let (id, text) = (l.id, l.text.clone());
        s.view.playing(id, &text, &shown);
    }

    pub(super) async fn stream_line(self: &Arc<Self>, seq: u64) -> bool {
        let url = format!("{}&seq={seq}", self.api("stream"));
        let (mut from, mut carry, mut total) = (0usize, None::<u8>, 0u64);
        loop {
            if self.lock().line.as_ref().is_none_or(|l| l.cut) {
                return true;
            }
            let req = HTTP.get(format!("{url}&from={from}")).timeout(Duration::from_secs(60)).send();
            let r = tokio::select! {
                r = req => r,
                _ = self.line_stop.notified() => return true,
            };
            let Ok(r) = r else { return from > 0 };
            if !r.status().is_success() {
                if from == 0 {
                    return false;
                }
                break;
            }
            let rate: u32 = r.headers().get("x-sample-rate").and_then(|v| v.to_str().ok()?.parse().ok()).unwrap_or(24_000);
            let last = r.headers().get("x-done").is_some_and(|v| v == "1");
            let Ok(bytes) = r.bytes().await else { break };
            from += bytes.len();
            let mut b: Vec<u8> = carry.take().into_iter().chain(bytes.iter().copied()).collect();
            if b.len() % 2 == 1 {
                carry = b.pop();
            }
            let samples: Vec<i16> = b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            {
                let mut s = self.lock();
                let s = &mut *s;
                if s.line.as_ref().is_none_or(|l| l.cut) {
                    return true;
                }
                if !samples.is_empty() {
                    let (at, out_rate) = s.player.push(rate, &samples);
                    total += samples.len() as u64;
                    let l = s.line.as_mut().unwrap();
                    if l.start_frame.is_none() {
                        (l.start_frame, l.rate) = (Some(at), out_rate);
                        self.started(s);
                    }
                }
                if last {
                    let l = s.line.as_mut().unwrap();
                    l.dur = total as f64 * 1000.0 / rate as f64;
                }
            }
            if last {
                break;
            }
        }
        self.drain().await;
        true
    }

    pub(super) async fn drain(&self) {
        let mut until = {
            let mut s = self.lock();
            let gaps = s.player.end_line();
            if gaps > 0 {
                let msg = format!("voice ran dry {gaps} times in this line");
                self.dbg(&mut s, &msg);
            }
            self.now() + 500.0 + 250.0 + s.line.as_ref().map_or(0.0, |l| l.dur)
        };
        loop {
            {
                let s = self.lock();
                let (_, waiting) = s.player.position();
                if s.player.paused() {
                    until += 20.0;
                }
                if waiting == 0 || s.line.as_ref().is_none_or(|l| l.cut) || self.now() > until {
                    return;
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(20)) => {}
                _ = self.line_stop.notified() => return,
            }
        }
    }

    pub(super) async fn play_parts(self: &Arc<Self>, seq: u64) {
        let Some(mut st) = self.get_state(-1).await else { return };
        let words = |t: &str| t.split_whitespace().count() as u64;
        if st["seq"].as_u64() != Some(seq) || st["mode"] != "parts" {
            let say = st["say"].as_str().unwrap_or("").to_string();
            self.lock_started();
            tokio::time::sleep(Duration::from_millis(1500 + words(&say) * 330)).await;
            return;
        }
        let parts: Vec<String> = st["parts"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(String::from)).collect()).unwrap_or_default();
        for (i, part) in parts.iter().enumerate() {
            while st["seq"].as_u64() == Some(seq) && st["ready"].as_u64().unwrap_or(0) <= i as u64 && !self.cut() {
                tokio::time::sleep(Duration::from_millis(150)).await;
                st = match self.get_state(-1).await {
                    Some(s) => s,
                    None => return,
                };
            }
            if st["seq"].as_u64() != Some(seq) || self.cut() {
                return;
            }
            let url = format!("{}&seq={seq}&part={i}", self.api("audio"));
            let audio = async { HTTP.get(url).send().await.ok().filter(|r| r.status().is_success())?.bytes().await.ok() }.await;
            match audio.as_deref().and_then(audio::wav) {
                Some((rate, samples)) => {
                    {
                        let mut s = self.lock();
                        let s = &mut *s;
                        if s.line.as_ref().is_none_or(|l| l.cut) {
                            return;
                        }
                        let (at, out_rate) = s.player.push(rate, &samples);
                        if let Some(l) = &mut s.line {
                            if i == 0 {
                                (l.start_frame, l.rate) = (Some(at), out_rate);
                            }
                        }
                        self.started(s);
                    }
                    self.drain().await;
                }
                None => {
                    self.lock_started();
                    tokio::time::sleep(Duration::from_millis(1000 + words(part) * 330)).await;
                }
            }
        }
    }

    pub(super) fn lock_started(&self) {
        let mut s = self.lock();
        self.started(&mut s);
    }
    pub(super) fn cut(&self) -> bool {
        self.lock().line.as_ref().is_none_or(|l| l.cut)
    }

    pub(super) async fn play_line(self: &Arc<Self>, seq: u64, text: String) {
        {
            let mut s = self.lock();
            if s.ended {
                return;
            }
            s.line = Some(Line { id: seq, text, started: false, start_at: 0.0, start_frame: None, rate: 0, dur: 0.0, cut: false });
            s.player.begin_line();
            s.mode = Mode::Claude;
            s.hot.clear();
            s.loud_run = 0;
            s.view.status("Claude is talking…");
        }
        if !self.stream_line(seq).await && !self.cut() {
            self.play_parts(seq).await;
        }
        let mut s = self.lock();
        let s = &mut *s;
        self.started(s);
        if let Some(l) = s.line.take() {
            if !l.cut {
                s.view.playing(l.id, &l.text, &l.text);
            }
            let hold = s.last_hold.clone();
            s.view.spoken(l.id, l.cut, hold, self.now());
        }
        if s.mode == Mode::Claude {
            s.mode = Mode::Idle;
        }
    }

}
