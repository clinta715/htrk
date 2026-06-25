use crate::audio::effects::compute_playback_frequency;
use crate::sequencer::effect::Effect;
use crate::sequencer::module::Module;
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::sequencer::player::SequencerState;
use crate::sequencer::sample::Sample;

// ─── Standalone helper functions ─────────────────────────────

pub(crate) fn compute_channel_volume(state: &SequencerState, channel: usize, use_xm_model: bool) -> f32 {
    if channel >= state.channels.len() {
        return 0.0;
    }
    let ch = &state.channels[channel];
    if use_xm_model {
        ch.channel_volume.min(64) as f32 / 64.0
    } else {
        let vol = ch.channel_volume.min(64) as f32 / 64.0;
        let global = state.global_volume as f32 / 128.0;
        vol * global
    }
}

pub(crate) fn compute_channel_panning(state: &SequencerState, channel: usize) -> f32 {
    if channel >= state.channels.len() {
        return 0.5;
    }
    state.channels[channel].channel_panning as f32 / 255.0
}

pub(crate) fn calculate_sample_offset(state: &SequencerState, channel: usize, cell: &Cell, sample: &Sample) -> usize {
    let ch = &state.channels[channel];
    let offset = match &cell.effect {
        Effect::SetSampleOffset { offset } => {
            let off = if *offset == 0 {
                ch.last_sample_offset as u32
            } else {
                *offset as u32
            };
            ((ch.high_sample_offset as u32) << 16) | off
        }
        Effect::FormatSpecific(fe) => {
            if let Some(offset) = fe.sample_offset() {
                let off = if offset == 0 {
                    ch.last_sample_offset as u32
                } else {
                    offset as u32
                };
                ((ch.high_sample_offset as u32) << 16) | off
            } else {
                0
            }
        }
        _ => 0,
    } as usize;

    offset.min(sample.data.len().saturating_sub(1))
}

pub(crate) fn compute_portamento_target(
    _channel: usize,
    _note_key: u8,
    remapped_key: u8,
    sample: Option<&Sample>,
    sample_idx: usize,
    module: &Module,
) -> (u16, f64) {
    let freq = match Note::On(remapped_key).frequency() {
        Some(f) => f,
        None => return (0, 0.0),
    };

    let (s, _playback_freq) = if sample_idx > 0 && sample_idx < module.samples.len() {
        let s = &module.samples[sample_idx];
        let pf = compute_playback_frequency(freq, s.sample_rate, s.relative_note, s.fine_tune);
        (s, pf)
    } else {
        match sample {
            Some(s) => {
                let pf = compute_playback_frequency(freq, s.sample_rate, s.relative_note, s.fine_tune);
                (s, pf)
            }
            None => return ((8363.0 * 428.0 / freq) as u16, freq),
        }
    };

    let pf = compute_playback_frequency(freq, s.sample_rate, s.relative_note, s.fine_tune);
    let period = (8363.0 * 428.0 / pf).max(1.0) as u16;
    (period, pf)
}
