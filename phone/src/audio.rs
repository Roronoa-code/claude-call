//! Both streams use the voice-call path, so the earpiece and echo handling apply.
use ndk::audio::{
    AudioCallbackResult, AudioContentType, AudioDirection, AudioFormat, AudioInputPreset, AudioPerformanceMode, AudioStream,
    AudioStreamBuilder, AudioUsage,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub const MIC_RATE: usize = 16_000;
pub const FRAME_MS: usize = 40;
pub const FRAME: usize = MIC_RATE * FRAME_MS / 1000;
pub const CUSHION_MS: usize = 250;

/// AAudio streams may be used from any thread.
struct Stream {
    _stream: AudioStream,
}
unsafe impl Send for Stream {}

#[derive(Default)]
struct Buf {
    data: VecDeque<i16>,
    played: u64,
    primed: bool,
    cushion: usize,
    ending: bool,
    gaps: u32,
    paused: bool,
    quiet: bool,
    peak: i16,
}

pub struct Player {
    buf: Arc<Mutex<Buf>>,
    stream: Option<Stream>,
    rate: u32,
    out_rate: u32, // the phone may not honour the asked rate
}

impl Player {
    pub fn new() -> Self {
        Player { buf: Default::default(), stream: None, rate: 0, out_rate: 0 }
    }

    fn open(&mut self, rate: u32) {
        if self.rate == rate && self.stream.is_some() {
            return;
        }
        self.stream = None;
        let buf = self.buf.clone();
        let cb = Box::new(move |_: &AudioStream, data: *mut std::ffi::c_void, frames: i32| {
            let out = unsafe { std::slice::from_raw_parts_mut(data.cast::<i16>(), frames as usize) };
            let mut b = buf.lock().unwrap();
            if !b.primed && (b.data.len() >= b.cushion || (b.ending && !b.data.is_empty())) {
                b.primed = true;
            }
            let mut n = 0;
            let (play, quiet) = (b.primed && !b.paused, b.quiet);
            for s in out.iter_mut() {
                *s = if play { b.data.pop_front().map(|v| { n += 1; if quiet { 0 } else { v } }).unwrap_or(0) } else { 0 };
                b.peak = b.peak.max(s.saturating_abs());
            }
            b.played += n;
            if play && b.data.is_empty() && !b.ending {
                b.primed = false;
                b.gaps += 1;
            }
            AudioCallbackResult::Continue
        });
        let stream = AudioStreamBuilder::new().and_then(|b| {
            b.direction(AudioDirection::Output)
                .channel_count(1)
                .format(AudioFormat::PCM_I16)
                .sample_rate(rate as i32)
                .usage(AudioUsage::VoiceCommunication)
                .content_type(AudioContentType::Speech)
                .performance_mode(AudioPerformanceMode::None)
                .data_callback(cb)
                .open_stream()
        });
        match stream {
            Ok(s) => {
                self.out_rate = s.sample_rate() as u32;
                self.buf.lock().unwrap().cushion = self.out_rate as usize * CUSHION_MS / 1000;
                if self.out_rate != rate {
                    crate::android::log(&format!("speaker opened at {} Hz for a {rate} Hz voice: resampling", self.out_rate));
                }
                let _ = s.request_start();
                self.stream = Some(Stream { _stream: s });
                self.rate = rate;
            }
            Err(e) => crate::android::log(&format!("speaker stream failed: {e:?}")),
        }
    }

    pub fn push(&mut self, rate: u32, samples: &[i16]) -> (u64, u32) {
        self.open(rate);
        let out = resample(samples, rate, self.out_rate);
        let mut b = self.buf.lock().unwrap();
        let at = b.played + b.data.len() as u64;
        b.data.extend(out.iter().map(|&v| v.round() as i16));
        (at, self.out_rate)
    }

    pub fn position(&self) -> (u64, usize) {
        let b = self.buf.lock().unwrap();
        (b.played, b.data.len())
    }

    pub fn level(&self) -> f32 {
        let b = self.buf.lock().unwrap();
        let n = b.data.len().min(1200);
        if n == 0 {
            return 0.0;
        }
        let sum: f32 = b.data.iter().take(n).map(|&s| (s as f32 / 32768.0).powi(2)).sum();
        ((sum / n as f32).sqrt() * 4.0).min(1.0)
    }

    pub fn begin_line(&self) {
        let mut b = self.buf.lock().unwrap();
        (b.primed, b.ending, b.gaps) = (false, false, 0);
    }

    pub fn end_line(&self) -> u32 {
        let mut b = self.buf.lock().unwrap();
        b.ending = true;
        b.gaps
    }

    pub fn stop(&self) {
        let mut b = self.buf.lock().unwrap();
        b.data.clear();
        (b.primed, b.paused) = (false, false);
    }

    pub fn pause(&self, on: bool) {
        self.buf.lock().unwrap().paused = on;
    }

    pub fn quiet(&self, on: bool) {
        self.buf.lock().unwrap().quiet = on;
    }

    pub fn take_peak(&self) -> i16 {
        std::mem::take(&mut self.buf.lock().unwrap().peak)
    }

    pub fn paused(&self) -> bool {
        self.buf.lock().unwrap().paused
    }

    pub fn close(&mut self) {
        self.stop();
        self.stream = None;
        self.rate = 0;
    }
}

fn resample<T: Copy + Into<f32>>(x: &[T], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || to == 0 || x.is_empty() {
        return x.iter().map(|&s| s.into()).collect();
    }
    let n = (x.len() as u64 * to as u64 / from as u64) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * from as f64 / to as f64;
            let (j, k) = (p as usize, (p - p.floor()) as f32);
            let a: f32 = x[j.min(x.len() - 1)].into();
            let b: f32 = x[(j + 1).min(x.len() - 1)].into();
            a + (b - a) * k
        })
        .collect()
}

pub struct Mic {
    _stream: Stream,
}

impl Mic {
    pub fn open(on_frame: impl Fn(Vec<f32>) + Send + 'static) -> Option<Mic> {
        let mut acc: Vec<f32> = Vec::with_capacity(FRAME * 2);
        let rate = Arc::new(std::sync::atomic::AtomicU32::new(MIC_RATE as u32));
        let opened = rate.clone();
        let cb = Box::new(move |_: &AudioStream, data: *mut std::ffi::c_void, frames: i32| {
            let input = unsafe { std::slice::from_raw_parts(data.cast::<f32>(), frames as usize) };
            acc.extend(resample(input, opened.load(std::sync::atomic::Ordering::Relaxed), MIC_RATE as u32));
            while acc.len() >= FRAME {
                on_frame(acc.drain(..FRAME).collect());
            }
            AudioCallbackResult::Continue
        });
        let stream = AudioStreamBuilder::new()
            .and_then(|b| {
                b.direction(AudioDirection::Input)
                    .channel_count(1)
                    .format(AudioFormat::PCM_Float)
                    .sample_rate(MIC_RATE as i32)
                    .input_preset(AudioInputPreset::VoiceCommunication)
                    .performance_mode(AudioPerformanceMode::None)
                    .data_callback(cb)
                    .open_stream()
            })
            .map_err(|e| crate::android::log(&format!("mic failed: {e:?}")))
            .ok()?;
        let actual = stream.sample_rate() as u32;
        if actual != MIC_RATE as u32 {
            crate::android::log(&format!("mic opened at {actual} Hz: resampling to {MIC_RATE}"));
        }
        rate.store(actual, std::sync::atomic::Ordering::Relaxed);
        stream.request_start().ok()?;
        Some(Mic { _stream: Stream { _stream: stream } })
    }
}

pub fn wav(data: &[u8]) -> Option<(u32, Vec<i16>)> {
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let (mut i, mut rate, mut channels) = (12, 0u32, 1usize);
    while i + 8 <= data.len() {
        let id = &data[i..i + 4];
        let len = u32::from_le_bytes(data[i + 4..i + 8].try_into().ok()?) as usize;
        let body = data.get(i + 8..(i + 8 + len).min(data.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            channels = u16::from_le_bytes([body[2], body[3]]).max(1) as usize;
            rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            if u16::from_le_bytes([body[14], body[15]]) != 16 {
                return None;
            }
        } else if id == b"data" {
            let s: Vec<i16> = body.chunks_exact(2 * channels).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            return (rate > 0).then_some((rate, s));
        }
        i += 8 + len + (len & 1);
    }
    None
}
