use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU8};
use std::sync::atomic::Ordering;
use std::sync::Arc;

static MASTER_VOLUME_DEFAULT: f32 = 0.25;

pub const SCOPE_SIZE: usize = 2048;
pub const MAX_CHANNELS: usize = 64;

pub struct AtomicPlaybackState {
    pub current_order: AtomicU16,
    pub current_row: AtomicU16,
    pub current_pattern: AtomicU16,
    pub bpm: AtomicU16,
    pub speed: AtomicU8,
    pub playing: AtomicBool,
    pub active_voices: AtomicU8,
    pub cpu_usage_pct: AtomicU8,
    master_volume_bits: AtomicU32,

    pub master_peak_left: AtomicU32,
    pub master_peak_right: AtomicU32,
    pub channel_peaks: [AtomicU32; MAX_CHANNELS],

    pub scope_left: Arc<Vec<AtomicU32>>,
    pub scope_right: Arc<Vec<AtomicU32>>,
    pub scope_write_pos: AtomicU32,
    pub scope_available: AtomicU32,
}

impl AtomicPlaybackState {
    pub fn master_volume(&self) -> f32 {
        f32::from_bits(self.master_volume_bits.load(Ordering::Relaxed))
    }

    #[allow(dead_code)]
    pub fn set_master_volume(&self, vol: f32) {
        self.master_volume_bits.store(vol.to_bits(), Ordering::Relaxed);
    }

    pub fn master_peak(&self) -> (f32, f32) {
        let l = f32::from_bits(self.master_peak_left.load(Ordering::Relaxed));
        let r = f32::from_bits(self.master_peak_right.load(Ordering::Relaxed));
        (l, r)
    }

    pub fn channel_peak(&self, ch: usize) -> f32 {
        if ch < MAX_CHANNELS {
            f32::from_bits(self.channel_peaks[ch].load(Ordering::Relaxed))
        } else {
            0.0
        }
    }

    pub fn write_scope(&self, left: &[f32], right: &[f32]) {
        let len = left.len().min(right.len()).min(SCOPE_SIZE);
        let pos = self.scope_write_pos.load(Ordering::Relaxed) as usize;
        for i in 0..len {
            let idx = (pos + i) % SCOPE_SIZE;
            self.scope_left[idx].store(left[i].to_bits(), Ordering::Relaxed);
            self.scope_right[idx].store(right[i].to_bits(), Ordering::Relaxed);
        }
        self.scope_write_pos.store(((pos + len) % SCOPE_SIZE) as u32, Ordering::Relaxed);
        self.scope_available.store((self.scope_available.load(Ordering::Relaxed) + len as u32).min(SCOPE_SIZE as u32), Ordering::Relaxed);
    }

    pub fn read_scope(&self) -> (Vec<f32>, Vec<f32>) {
        let available = self.scope_available.load(Ordering::Relaxed) as usize;
        let available = available.min(SCOPE_SIZE);
        if available == 0 {
            return (Vec::new(), Vec::new());
        }
        let pos = self.scope_write_pos.load(Ordering::Relaxed) as usize;
        let mut left = Vec::with_capacity(available);
        let mut right = Vec::with_capacity(available);
        for i in 0..available {
            let idx = (pos + SCOPE_SIZE - available + i) % SCOPE_SIZE;
            left.push(f32::from_bits(self.scope_left[idx].load(Ordering::Relaxed)));
            right.push(f32::from_bits(self.scope_right[idx].load(Ordering::Relaxed)));
        }
        (left, right)
    }
}

impl Default for AtomicPlaybackState {
    fn default() -> Self {
        let scope_left = Arc::new((0..SCOPE_SIZE).map(|_| AtomicU32::new(0)).collect::<Vec<_>>());
        let scope_right = Arc::new((0..SCOPE_SIZE).map(|_| AtomicU32::new(0)).collect::<Vec<_>>());
        AtomicPlaybackState {
            current_order: AtomicU16::new(0),
            current_row: AtomicU16::new(0),
            current_pattern: AtomicU16::new(0),
            bpm: AtomicU16::new(125),
            speed: AtomicU8::new(6),
            playing: AtomicBool::new(false),
            active_voices: AtomicU8::new(0),
            cpu_usage_pct: AtomicU8::new(0),
            master_volume_bits: AtomicU32::new(MASTER_VOLUME_DEFAULT.to_bits()),
            master_peak_left: AtomicU32::new(0),
            master_peak_right: AtomicU32::new(0),
            channel_peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            scope_left,
            scope_right,
            scope_write_pos: AtomicU32::new(0),
            scope_available: AtomicU32::new(0),
        }
    }
}
