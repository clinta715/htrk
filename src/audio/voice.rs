use crate::sequencer::note::Note;
use crate::sequencer::sample::{LoopType, VibratoWaveform};
use crate::sequencer::instrument::{Envelope, NewNoteAction};
use crate::audio::filter::StateVariableFilter;
use crate::sequencer::effect::FilterType;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct EnvelopeState {
    pub envelope: Arc<Envelope>,
    pub current_point: usize,
    pub position: f32,
    pub released: bool,
    pub finished: bool,
}

#[derive(Clone, Debug)]
pub struct Voice {
    pub active: bool,

    pub sample: Option<Arc<Vec<f32>>>,
    pub sample_rate: f64,
    pub loop_type: LoopType,
    pub loop_start: usize,
    pub loop_end: usize,

    pub position: f64,
    pub position_end: f64,

    pub base_frequency: f64,
    pub current_frequency: f64,
    pub sample_delta: f64,

    pub base_volume: f32,
    pub envelope_volume: f32,
    pub tremolo_volume: f32,
    pub channel_volume: f32,
    pub global_volume: f32,
    pub fade_out_volume: f32,
    pub final_volume: f32,

    pub smoothed_volume: f32,

    pub base_panning: f32,
    pub envelope_panning: f32,
    pub final_panning: f32,

    pub smoothed_panning: f32,

    pub vol_env: Option<EnvelopeState>,
    pub pan_env: Option<EnvelopeState>,
    pub pitch_env: Option<EnvelopeState>,
    pub filter_env: Option<EnvelopeState>,

    pub filter_type: FilterType,
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub envelope_filter_cutoff: f32,
    pub svf: StateVariableFilter,
    pub amiga_led_svf: StateVariableFilter,
    pub filter_enabled: bool,
    pub amiga_led_filter: bool,

    pub vibrato_phase: f32,
    pub vibrato_speed: u8,
    pub vibrato_depth: u8,
    pub vibrato_waveform: VibratoWaveform,

    pub tremolo_phase: f32,
    pub tremolo_speed: u8,
    pub tremolo_depth: u8,
    pub tremolo_waveform: VibratoWaveform,

    pub panbrello_phase: f32,
    pub panbrello_speed: u8,
    pub panbrello_depth: u8,

    pub tremor_mute: bool,

    pub portamento_target: Option<f64>,
    pub portamento_speed: f64,

    pub fading: bool,
    pub note_off: bool,
    pub cutoff_tick: Option<u16>,
    pub delay_tick: Option<u16>,

    pub instrument_index: Option<u8>,
    pub sample_index: Option<u8>,
    pub note: Note,
    pub nna: NewNoteAction,
    pub fade_out_rate: u16,
    pub channel: Option<usize>,

    pub direction: f64,

    pub auto_vib_pos: u8,
    pub auto_vib_amp: i32,
    pub auto_vib_sweep: i32,
    pub env_sustain_active: bool,
    pub fade_out_amp: i32,
    pub fade_out_speed_i32: i32,
    pub auto_vib_period_base: u16,
}

impl Voice {
    pub fn trigger(
        &mut self,
        sample_data: Arc<Vec<f32>>,
        sample_rate: f64,
        loop_type: LoopType,
        loop_start: usize,
        loop_end: usize,
        frequency: f64,
        output_rate: f64,
        volume: f32,
        panning: f32,
        sample_offset: usize,
        instrument_index: Option<u8>,
        sample_index: Option<u8>,
        note: Note,
        nna: NewNoteAction,
        fade_out_rate: u16,
    ) {
        self.active = true;
        self.sample = Some(sample_data);
        self.sample_rate = sample_rate;
        self.loop_type = loop_type;
        self.loop_start = loop_start;
        self.loop_end = if loop_end > loop_start { loop_end } else { 0 };
        self.position = sample_offset as f64;
        self.position_end = match &self.sample {
            Some(data) => data.len() as f64,
            None => 0.0,
        };
        self.base_frequency = frequency;
        self.current_frequency = frequency;
        self.sample_delta = if output_rate > 0.0 && sample_rate > 0.0 {
            frequency / sample_rate * (sample_rate / output_rate)
        } else {
            0.0
        };
        self.base_volume = volume;
        self.envelope_volume = 1.0;
        self.tremolo_volume = 0.0;
        self.channel_volume = 1.0;
        self.global_volume = 1.0;
        self.fade_out_volume = 1.0;
        self.final_volume = volume;
        self.smoothed_volume = volume;
        self.base_panning = panning;
        self.envelope_panning = 0.0;
        self.final_panning = panning;
        self.smoothed_panning = panning;
        self.vol_env = None;
        self.pan_env = None;
        self.pitch_env = None;
        self.filter_env = None;
self.filter_type = FilterType::LowPass;
        self.filter_cutoff = 0xFFFF as f32;
        self.filter_resonance = 0.0;
        self.envelope_filter_cutoff = 1.0;
        self.svf = StateVariableFilter::default();
        self.amiga_led_svf = StateVariableFilter::default();
        self.filter_enabled = true;
        self.amiga_led_filter = false;
        self.vibrato_phase = 0.0;
        self.vibrato_speed = 0;
        self.vibrato_depth = 0;
        self.tremolo_phase = 0.0;
        self.tremolo_speed = 0;
        self.tremolo_depth = 0;
        self.panbrello_phase = 0.0;
        self.panbrello_speed = 0;
        self.panbrello_depth = 0;
self.tremor_mute = false;
        self.portamento_target = None;
        self.fading = false;
        self.note_off = false;
        self.cutoff_tick = None;
        self.delay_tick = None;
        self.instrument_index = instrument_index;
        self.sample_index = sample_index;
        self.note = note;
        self.nna = nna;
        self.fade_out_rate = fade_out_rate;
        self.channel = None;

        self.direction = 1.0;
        self.auto_vib_pos = 0;
        self.auto_vib_amp = 0;
        self.auto_vib_sweep = 0;
        self.env_sustain_active = true;
        self.fade_out_amp = 32768i32;
        self.fade_out_speed_i32 = 0;
        self.auto_vib_period_base = 0;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        self.sample = None;
        self.position = 0.0;
    }
}

impl Default for Voice {
    fn default() -> Self {
        Voice {
            active: false,
            sample: None,
            sample_rate: 0.0,
            loop_type: LoopType::None,
            loop_start: 0,
            loop_end: 0,
            position: 0.0,
            position_end: 0.0,
            base_frequency: 0.0,
            current_frequency: 0.0,
            sample_delta: 0.0,
            base_volume: 0.0,
            envelope_volume: 1.0,
            tremolo_volume: 0.0,
            channel_volume: 1.0,
            global_volume: 1.0,
            fade_out_volume: 0.0,
            final_volume: 1.0,
            smoothed_volume: 1.0,
            base_panning: 0.5,
            envelope_panning: 0.0,
            final_panning: 0.0,
            smoothed_panning: 0.0,
            vol_env: None,
            pan_env: None,
            pitch_env: None,
            filter_env: None,
            filter_type: FilterType::LowPass,
            filter_cutoff: 0xFFFF as f32,
            filter_resonance: 0.0,
            envelope_filter_cutoff: 1.0,
            svf: StateVariableFilter::default(),
            amiga_led_svf: StateVariableFilter::default(),
            filter_enabled: true,
            amiga_led_filter: false,
            vibrato_phase: 0.0,
            vibrato_speed: 0,
            vibrato_depth: 0,
            vibrato_waveform: VibratoWaveform::Sine,
            tremolo_phase: 0.0,
            tremolo_speed: 0,
            tremolo_depth: 0,
            tremolo_waveform: VibratoWaveform::Sine,
            panbrello_phase: 0.0,
            panbrello_speed: 0,
            panbrello_depth: 0,
            tremor_mute: false,
            portamento_target: Option::None,
            portamento_speed: 0.0,
            fading: false,
            note_off: false,
            cutoff_tick: None,
            delay_tick: None,
            instrument_index: None,
            sample_index: None,
            note: Note::None,
            nna: NewNoteAction::NoteCut,
            fade_out_rate: 0,
            channel: None,
            direction: 1.0,
            auto_vib_pos: 0,
            auto_vib_amp: 0,
            auto_vib_sweep: 0,
            env_sustain_active: false,
            fade_out_amp: 0,
            fade_out_speed_i32: 0,
auto_vib_period_base: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::instrument::EnvelopePoint;

    #[test]
    fn voice_default_inactive() {
        let v = Voice::default();
        assert!(!v.active);
        assert!(v.sample.is_none());
        assert_eq!(v.base_panning, 0.5);
        assert!(!v.fading);
        assert!(!v.note_off);
    }

    #[test]
    fn envelope_state_default() {
        let env = Envelope {
            points: vec![EnvelopePoint { tick: 0, value: 64 }],
            sustain_point: None,
            loop_start: None,
            loop_end: None,
            flags: crate::sequencer::instrument::EnvelopeFlags::default(),
        };
        let es = EnvelopeState {
            envelope: Arc::new(env),
            current_point: 0,
            position: 0.0,
            released: false,
            finished: false,
        };
        assert_eq!(es.current_point, 0);
        assert!(!es.released);
    }
}
