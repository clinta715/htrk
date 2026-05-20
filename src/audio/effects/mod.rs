mod xm;
mod legacy;

use crate::audio::voice::EnvelopeState;
use crate::sequencer::effect::Effect;
use crate::sequencer::module::{Module, BASE_NOTE_RATE};
use crate::sequencer::pattern::Cell;
use crate::sequencer::sample::{Sample, VibratoWaveform};

pub const VIBRATO_TABLE_SIZE: usize = 64;

pub const VIBRATO_SINE_TABLE: [f32; VIBRATO_TABLE_SIZE] = [
    0.0, 24.0, 49.0, 74.0, 97.0, 120.0, 141.0, 161.0, 180.0, 197.0, 212.0, 224.0, 235.0,
    244.0, 250.0, 253.0, 255.0, 253.0, 250.0, 244.0, 235.0, 224.0, 212.0, 197.0, 180.0,
    161.0, 141.0, 120.0, 97.0, 74.0, 49.0, 24.0, 0.0, -24.0, -49.0, -74.0, -97.0, -120.0,
    -141.0, -161.0, -180.0, -197.0, -212.0, -224.0, -235.0, -244.0, -250.0, -253.0, -255.0,
    -253.0, -250.0, -244.0, -235.0, -224.0, -212.0, -197.0, -180.0, -161.0, -141.0, -120.0,
    -97.0, -74.0, -49.0, -24.0,
];

pub const VIBRATO_RAMP_TABLE: [f32; VIBRATO_TABLE_SIZE] = {
    let mut table = [0f32; VIBRATO_TABLE_SIZE];
    let mut i = 0;
    while i < VIBRATO_TABLE_SIZE {
        table[i] = (i as f32) * 255.0 / 63.0 - 128.0;
        i += 1;
    }
    table
};

pub const FUNK_TRACK: [u8; 16] = [0, 5, 7, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21];

pub fn quantize_to_semitone(freq: f64) -> f64 {
    let nearest = (freq.log2() * 12.0).round();
    2.0_f64.powf(nearest / 12.0)
}

pub fn fastrand() -> f32 {
    static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(123456789);
    let mut x = SEED.load(std::sync::atomic::Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    SEED.store(x, std::sync::atomic::Ordering::Relaxed);
    (x >> 40) as f32 / 16777216.0
}

pub fn compute_samples_per_tick(bpm: u16, sample_rate: f64) -> f64 {
    let safe_bpm = if bpm == 0 { 125.0 } else { bpm as f64 };
    sample_rate * 5.0 / (safe_bpm * 2.0)
}

pub fn get_vibrato_value(waveform: VibratoWaveform, phase: f32) -> f32 {
    let idx = (phase as usize) % VIBRATO_TABLE_SIZE;
    match waveform {
        VibratoWaveform::Sine => VIBRATO_SINE_TABLE[idx],
        VibratoWaveform::Square => {
            if idx < VIBRATO_TABLE_SIZE / 2 {
                255.0
            } else {
                -255.0
            }
        }
        VibratoWaveform::Ramp => VIBRATO_RAMP_TABLE[idx],
        VibratoWaveform::Random => {
            let val = (idx as u64)
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((val >> 33) as i32 - 256) as f32
        }
    }
}

pub fn advance_single_envelope(env: &mut EnvelopeState) {
    if env.finished {
        return;
    }

    let points = &env.envelope.points;
    if points.is_empty() {
        env.finished = true;
        return;
    }

    env.position += 1.0;

    if env.position < 0.0 {
        env.position = 0.0;
    }

    if env.current_point + 1 < points.len() {
        let next_point = points[env.current_point + 1];
        if env.position >= next_point.tick as f32 {
            env.current_point += 1;
        }
    }

    if !env.released && env.envelope.flags.sustain {
        if let Some(sustain_idx) = env.envelope.sustain_point {
            if env.current_point >= sustain_idx && sustain_idx < points.len() {
                let sustain_tick = points[sustain_idx].tick as f32;
                if env.position >= sustain_tick {
                    env.position = sustain_tick;
                    return;
                }
            }
        }
    }

    if env.envelope.flags.loop_ {
        if let (Some(loop_start), Some(loop_end)) = (env.envelope.loop_start, env.envelope.loop_end) {
            if loop_start < loop_end && loop_end < points.len() {
                let loop_end_tick = points[loop_end].tick as f32;
                if env.position >= loop_end_tick {
                    env.position = points[loop_start].tick as f32;
                    env.current_point = loop_start;
                }
            }
        }
    }

    if !env.envelope.flags.loop_ && env.current_point >= points.len() - 1 {
        let last_tick = points.last().map(|p| p.tick).unwrap_or(0) as f32;
        if env.position >= last_tick {
            env.finished = true;
        }
    }
}

pub fn evaluate_envelope(env: &EnvelopeState) -> f32 {
    let points = &env.envelope.points;
    if points.is_empty() {
        return 64.0;
    }

    let current_idx = env.current_point.min(points.len() - 1);
    let current_point = points[current_idx];

    if current_idx + 1 >= points.len() {
        return current_point.value as f32;
    }

    let next_point = points[current_idx + 1];
    if next_point.tick <= current_point.tick {
        return current_point.value as f32;
    }
    let tick_range = (next_point.tick - current_point.tick) as f32;

    let t = ((env.position - current_point.tick as f32) / tick_range).clamp(0.0, 1.0);
    let val_range = next_point.value as f32 - current_point.value as f32;
    current_point.value as f32 + val_range * t
}

pub fn compute_playback_frequency(
    note_freq: f64,
    sample_c5speed: u32,
    relative_note: i8,
    fine_tune: i8,
) -> f64 {
    let base_rate = BASE_NOTE_RATE;
    let sample_rate = sample_c5speed as f64;
    let pitch_multiplier = 2.0_f64.powf(
        (relative_note as f64 + fine_tune as f64 / 128.0) / 12.0,
    );
    (note_freq / base_rate) * sample_rate * pitch_multiplier
}

pub enum EffectProcessor {
    Xm(xm::XmProcessor),
    Legacy(legacy::LegacyProcessor),
}

impl EffectProcessor {
    pub fn from_module(module: &Module) -> Self {
        if module.flags.xm_period_model {
            EffectProcessor::Xm(xm::XmProcessor::new())
        } else {
            EffectProcessor::Legacy(legacy::LegacyProcessor::new())
        }
    }

    pub fn apply_effect(&mut self, engine: &mut super::sequencer_engine::SequencerEngine, channel: usize, effect: &Effect, is_row_start: bool) {
        match self {
            EffectProcessor::Xm(p) => p.apply_effect(engine, channel, effect, is_row_start),
            EffectProcessor::Legacy(p) => p.apply_effect(engine, channel, effect, is_row_start),
        }
    }

    pub fn process_tick(&mut self, engine: &mut super::sequencer_engine::SequencerEngine, tick: u8) {
        match self {
            EffectProcessor::Xm(p) => p.process_tick(engine, tick),
            EffectProcessor::Legacy(p) => p.process_tick(engine, tick),
        }
    }

    pub fn trigger_note(
        &mut self,
        engine: &mut super::sequencer_engine::SequencerEngine,
        channel: usize,
        note_key: u8,
        remapped_key: u8,
        sample: Option<&Sample>,
        sample_idx: usize,
        cell: &Cell,
        instrument_idx: usize,
    ) {
        match self {
            EffectProcessor::Xm(p) => p.trigger_note(engine, channel, note_key, remapped_key, sample, sample_idx, cell, instrument_idx),
            EffectProcessor::Legacy(p) => p.trigger_note(engine, channel, note_key, remapped_key, sample, sample_idx, cell, instrument_idx),
        }
    }

    pub fn trigger_delayed_note(&mut self, engine: &mut super::sequencer_engine::SequencerEngine, channel: usize) {
        match self {
            EffectProcessor::Xm(p) => p.trigger_delayed_note(engine, channel),
            EffectProcessor::Legacy(p) => p.trigger_delayed_note(engine, channel),
        }
    }

    pub fn process_volume_column(&mut self, engine: &mut super::sequencer_engine::SequencerEngine, channel: usize, vol: u8) {
        match self {
            EffectProcessor::Xm(p) => p.process_volume_column(engine, channel, vol),
            EffectProcessor::Legacy(p) => p.process_volume_column(engine, channel, vol),
        }
    }

    pub fn handle_note_off(&mut self, engine: &mut super::sequencer_engine::SequencerEngine, channel: usize) {
        match self {
            EffectProcessor::Xm(p) => p.handle_note_off(engine, channel),
            EffectProcessor::Legacy(p) => p.handle_note_off(engine, channel),
        }
    }
}
