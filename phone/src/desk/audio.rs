pub const FRAME_MS: usize = 40;
pub const CUSHION_MS: usize = 250;

pub struct Player;

impl Player {
    pub fn new() -> Self {
        Player
    }

    pub fn push(&mut self, rate: u32, _: &[i16]) -> (u64, u32) {
        (0, rate)
    }

    pub fn position(&self) -> (u64, usize) {
        (0, 0)
    }

    pub fn level(&self) -> f32 {
        0.0
    }

    pub fn begin_line(&self) {}

    pub fn end_line(&self) -> u32 {
        0
    }

    pub fn stop(&self) {}
    pub fn pause(&self, _: bool) {}
    pub fn quiet(&self, _: bool) {}

    pub fn take_peak(&self) -> i16 {
        0
    }

    pub fn paused(&self) -> bool {
        false
    }

    pub fn close(&mut self) {}
}

pub struct Mic;

impl Mic {
    pub fn open(_: impl Fn(Vec<f32>) + Send + 'static) -> Option<Mic> {
        None
    }
}

pub fn wav(_: &[u8]) -> Option<(u32, Vec<i16>)> {
    None
}
