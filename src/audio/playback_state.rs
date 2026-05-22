use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicU8};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::sequencer::player::PlayMode;

use crate::sequencer::note::TONE_NAMES;

static MASTER_VOLUME_DEFAULT: f32 = 0.25;

pub const CHANNEL_SCOPE_SIZE: usize = 512;
pub const MAX_CHANNELS: usize = 64;

pub const NUM_ENVELOPE_TYPES: usize = 4;
pub const ENV_POS_NONE: u32 = 0x7FC00000; // f32::NAN bits

pub struct AtomicPlaybackState {
    pub current_order: AtomicU16,
    pub current_row: AtomicU16,
    pub current_pattern: AtomicU16,
    pub current_tick: AtomicU8,
    pub bpm: AtomicU16,
    pub speed: AtomicU8,
    pub playing: AtomicBool,
    pub active_voices: AtomicU8,
    pub cpu_usage_pct: AtomicU8,
    master_volume_bits: AtomicU32,
    pub play_mode_bits: AtomicU8,

    pub master_peak_left: AtomicU32,
    pub master_peak_right: AtomicU32,
    pub channel_peaks: [AtomicU32; MAX_CHANNELS],

    pub channel_scope_left: [Arc<Vec<AtomicU32>>; MAX_CHANNELS],
    pub channel_scope_right: [Arc<Vec<AtomicU32>>; MAX_CHANNELS],
    pub channel_scope_write_pos: AtomicU32,
    pub channel_scope_available: AtomicU32,

    pub channel_note: [AtomicU16; MAX_CHANNELS],
    pub channel_instrument: [AtomicU16; MAX_CHANNELS],

    channel_sample_position: [AtomicU64; MAX_CHANNELS],
    channel_sample_index: [AtomicU16; MAX_CHANNELS],

    channel_env_pos: [[AtomicU32; MAX_CHANNELS]; NUM_ENVELOPE_TYPES],
    channel_env_instrument: [AtomicU16; MAX_CHANNELS],
}

impl AtomicPlaybackState {
    pub fn master_volume(&self) -> f32 {
        f32::from_bits(self.master_volume_bits.load(Ordering::Relaxed))
    }

    #[allow(dead_code)]
    pub fn set_master_volume(&self, vol: f32) {
        self.master_volume_bits.store(vol.to_bits(), Ordering::Relaxed);
    }

    pub fn play_mode(&self) -> PlayMode {
        match self.play_mode_bits.load(Ordering::Relaxed) {
            1 => PlayMode::Loop,
            2 => PlayMode::Pattern,
            3 => PlayMode::Order,
            _ => PlayMode::Once,
        }
    }

    pub fn set_play_mode(&self, mode: PlayMode) {
        let bits: u8 = match mode {
            PlayMode::Once => 0,
            PlayMode::Loop => 1,
            PlayMode::Pattern => 2,
            PlayMode::Order => 3,
        };
        self.play_mode_bits.store(bits, Ordering::Relaxed);
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

    pub fn write_channel_scope(&self, ch: usize, left: &[f32], right: &[f32]) {
        if ch >= MAX_CHANNELS {
            return;
        }
        let len = left.len().min(right.len()).min(CHANNEL_SCOPE_SIZE);
        let pos = self.channel_scope_write_pos.load(Ordering::Relaxed) as usize;
        let ch_left = &self.channel_scope_left[ch];
        let ch_right = &self.channel_scope_right[ch];
        for i in 0..len {
            let idx = (pos + i) % CHANNEL_SCOPE_SIZE;
            ch_left[idx].store(left[i].to_bits(), Ordering::Relaxed);
            ch_right[idx].store(right[i].to_bits(), Ordering::Relaxed);
        }
    }

    pub fn finish_channel_scope_write(&self, frame_count: usize) {
        let len = frame_count.min(CHANNEL_SCOPE_SIZE);
        let pos = self.channel_scope_write_pos.load(Ordering::Relaxed) as usize;
        self.channel_scope_write_pos.store(((pos + len) % CHANNEL_SCOPE_SIZE) as u32, Ordering::Relaxed);
        self.channel_scope_available.store(
            (self.channel_scope_available.load(Ordering::Relaxed) + len as u32).min(CHANNEL_SCOPE_SIZE as u32),
            Ordering::Relaxed,
        );
    }

    pub fn read_channel_scope(&self, ch: usize) -> (Vec<f32>, Vec<f32>) {
        if ch >= MAX_CHANNELS {
            return (Vec::new(), Vec::new());
        }
        let available = self.channel_scope_available.load(Ordering::Relaxed) as usize;
        let available = available.min(CHANNEL_SCOPE_SIZE);
        if available == 0 {
            return (Vec::new(), Vec::new());
        }
        let pos = self.channel_scope_write_pos.load(Ordering::Relaxed) as usize;
        let ch_left = &self.channel_scope_left[ch];
        let ch_right = &self.channel_scope_right[ch];
        let mut left = Vec::with_capacity(available);
        let mut right = Vec::with_capacity(available);
        for i in 0..available {
            let idx = (pos + CHANNEL_SCOPE_SIZE - available + i) % CHANNEL_SCOPE_SIZE;
            left.push(f32::from_bits(ch_left[idx].load(Ordering::Relaxed)));
            right.push(f32::from_bits(ch_right[idx].load(Ordering::Relaxed)));
        }
        (left, right)
    }

    const SAMPLE_POS_NONE: u64 = u64::MAX;
    const SAMPLE_IDX_NONE: u16 = 0xFFFF;

    pub fn set_channel_sample_position(&self, ch: usize, pos: Option<f64>) {
        if ch < MAX_CHANNELS {
            let bits = pos.map(|p| p.to_bits()).unwrap_or(Self::SAMPLE_POS_NONE);
            self.channel_sample_position[ch].store(bits, Ordering::Relaxed);
        }
    }

    pub fn channel_sample_position(&self, ch: usize) -> Option<f64> {
        if ch < MAX_CHANNELS {
            let bits = self.channel_sample_position[ch].load(Ordering::Relaxed);
            if bits == Self::SAMPLE_POS_NONE {
                None
            } else {
                Some(f64::from_bits(bits))
            }
        } else {
            None
        }
    }

    pub fn set_channel_sample_index(&self, ch: usize, idx: Option<u8>) {
        if ch < MAX_CHANNELS {
            let val = idx.map(|i| i as u16).unwrap_or(Self::SAMPLE_IDX_NONE);
            self.channel_sample_index[ch].store(val, Ordering::Relaxed);
        }
    }

    pub fn channel_sample_index_val(&self, ch: usize) -> Option<u8> {
        if ch < MAX_CHANNELS {
            let val = self.channel_sample_index[ch].load(Ordering::Relaxed);
            if val == Self::SAMPLE_IDX_NONE {
                None
            } else {
                Some(val as u8)
            }
        } else {
            None
        }
    }

    pub fn sample_positions_for(&self, sample_idx: usize) -> Vec<f64> {
        let mut positions = Vec::new();
        let target = sample_idx as u16;
        for ch in 0..MAX_CHANNELS {
            let idx = self.channel_sample_index[ch].load(Ordering::Relaxed);
            if idx == target {
                let bits = self.channel_sample_position[ch].load(Ordering::Relaxed);
                if bits != Self::SAMPLE_POS_NONE {
                    positions.push(f64::from_bits(bits));
                }
            }
        }
        positions
    }

    pub fn clear_all_sample_positions(&self) {
        for ch in 0..MAX_CHANNELS {
            self.channel_sample_position[ch].store(Self::SAMPLE_POS_NONE, Ordering::Relaxed);
            self.channel_sample_index[ch].store(Self::SAMPLE_IDX_NONE, Ordering::Relaxed);
        }
    }

    pub fn set_channel_env_pos(&self, env_type: usize, ch: usize, pos: Option<f32>) {
        if ch < MAX_CHANNELS && env_type < NUM_ENVELOPE_TYPES {
            let bits = pos.map(|p| p.to_bits()).unwrap_or(ENV_POS_NONE);
            self.channel_env_pos[env_type][ch].store(bits, Ordering::Relaxed);
        }
    }

    pub fn channel_env_pos(&self, env_type: usize, ch: usize) -> Option<f32> {
        if ch < MAX_CHANNELS && env_type < NUM_ENVELOPE_TYPES {
            let bits = self.channel_env_pos[env_type][ch].load(Ordering::Relaxed);
            if bits == ENV_POS_NONE || f32::from_bits(bits).is_nan() {
                None
            } else {
                Some(f32::from_bits(bits))
            }
        } else {
            None
        }
    }

    pub fn clear_all_env_positions(&self) {
        for env_type in 0..NUM_ENVELOPE_TYPES {
            for ch in 0..MAX_CHANNELS {
                self.channel_env_pos[env_type][ch].store(ENV_POS_NONE, Ordering::Relaxed);
            }
        }
        for ch in 0..MAX_CHANNELS {
            self.channel_env_instrument[ch].store(Self::SAMPLE_IDX_NONE, Ordering::Relaxed);
        }
    }

    pub fn set_channel_env_instrument(&self, ch: usize, instr: Option<u8>) {
        if ch < MAX_CHANNELS {
            let val = instr.map(|i| i as u16).unwrap_or(Self::SAMPLE_IDX_NONE);
            self.channel_env_instrument[ch].store(val, Ordering::Relaxed);
        }
    }

    pub fn env_positions_for_instrument(&self, env_type: usize, instrument_idx: u8) -> Vec<f32> {
        let mut positions = Vec::new();
        if env_type >= NUM_ENVELOPE_TYPES {
            return positions;
        }
        let target = instrument_idx as u16;
        for ch in 0..MAX_CHANNELS {
            let instr = self.channel_env_instrument[ch].load(Ordering::Relaxed);
            if instr == target {
                let bits = self.channel_env_pos[env_type][ch].load(Ordering::Relaxed);
                if bits != ENV_POS_NONE && !f32::from_bits(bits).is_nan() {
                    positions.push(f32::from_bits(bits));
                }
            }
        }
        positions
    }
}

impl Default for AtomicPlaybackState {
    fn default() -> Self {
        let channel_scope_left = std::array::from_fn(|_| {
            Arc::new((0..CHANNEL_SCOPE_SIZE).map(|_| AtomicU32::new(0)).collect::<Vec<_>>())
        });
        let channel_scope_right = std::array::from_fn(|_| {
            Arc::new((0..CHANNEL_SCOPE_SIZE).map(|_| AtomicU32::new(0)).collect::<Vec<_>>())
        });
        AtomicPlaybackState {
            current_order: AtomicU16::new(0),
            current_row: AtomicU16::new(0),
            current_pattern: AtomicU16::new(0),
            current_tick: AtomicU8::new(0),
            bpm: AtomicU16::new(125),
            speed: AtomicU8::new(6),
            playing: AtomicBool::new(false),
            active_voices: AtomicU8::new(0),
            cpu_usage_pct: AtomicU8::new(0),
            master_volume_bits: AtomicU32::new(MASTER_VOLUME_DEFAULT.to_bits()),
            play_mode_bits: AtomicU8::new(0),
            master_peak_left: AtomicU32::new(0),
            master_peak_right: AtomicU32::new(0),
            channel_peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            channel_scope_left,
            channel_scope_right,
            channel_scope_write_pos: AtomicU32::new(0),
            channel_scope_available: AtomicU32::new(0),
            channel_note: std::array::from_fn(|_| AtomicU16::new(0)),
            channel_instrument: std::array::from_fn(|_| AtomicU16::new(0)),
            channel_sample_position: std::array::from_fn(|_| AtomicU64::new(u64::MAX)),
            channel_sample_index: std::array::from_fn(|_| AtomicU16::new(0xFFFF)),
            channel_env_pos: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(ENV_POS_NONE))),
            channel_env_instrument: std::array::from_fn(|_| AtomicU16::new(0xFFFF)),
        }
    }
}

impl AtomicPlaybackState {
    pub fn set_channel_note(&self, ch: usize, note: u16) {
        if ch < MAX_CHANNELS {
            self.channel_note[ch].store(note, Ordering::Relaxed);
        }
    }

    pub fn channel_note(&self, ch: usize) -> u16 {
        if ch < MAX_CHANNELS { self.channel_note[ch].load(Ordering::Relaxed) } else { 0 }
    }

    pub fn set_channel_instrument(&self, ch: usize, instr: u16) {
        if ch < MAX_CHANNELS {
            self.channel_instrument[ch].store(instr, Ordering::Relaxed);
        }
    }

    pub fn channel_instrument(&self, ch: usize) -> u16 {
        if ch < MAX_CHANNELS { self.channel_instrument[ch].load(Ordering::Relaxed) } else { 0 }
    }

    pub fn channel_note_str(&self, ch: usize) -> String {
        let note_val = self.channel_note(ch);
        if note_val == 0 {
            "---".to_string()
        } else if note_val == 0xFF {
            "^^^".to_string()
        } else if note_val == 0xFE {
            "===".to_string()
        } else {
            let key = note_val as u8;
            let tone = key % 12;
            let octave = key / 12;
            format!("{}{}", TONE_NAMES[tone as usize], octave)
        }
    }

    pub fn channel_instrument_str(&self, ch: usize) -> String {
        let instr = self.channel_instrument(ch);
        if instr == 0 { "..".to_string() } else { format!("{:02}", instr) }
    }
}
