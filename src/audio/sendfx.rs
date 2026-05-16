use crate::audio::filter::StateVariableFilter;
use crate::sequencer::effect::SendEffectType;

pub fn create_send_effect(effect_type: SendEffectType, sample_rate: f32) -> Option<Box<dyn SendEffect>> {
    match effect_type {
        SendEffectType::None => None,
        SendEffectType::Delay => Some(Box::new(DelayEffect::new(sample_rate))),
        SendEffectType::Reverb => Some(Box::new(ReverbEffect::new(sample_rate))),
        SendEffectType::Chorus => Some(Box::new(ChorusEffect::new(sample_rate))),
        SendEffectType::Flanger => Some(Box::new(FlangerEffect::new(sample_rate))),
        SendEffectType::Phaser => Some(Box::new(PhaserEffect::new(sample_rate))),
    }
}

pub trait SendEffect: Send {
    fn process(&mut self, left: &mut [f32], right: &mut [f32], bpm: u16, sample_rate: f32);
    fn set_param(&mut self, index: u32, value: f32);
    fn get_param(&self, index: u32) -> f32;
    fn param_count(&self) -> u32;
    fn param_label(&self, index: u32) -> &str;
    fn name(&self) -> &str;
}

pub struct DelayEffect {
    buffer_left: Vec<f32>,
    buffer_right: Vec<f32>,
    write_pos: usize,
    delay_samples: f64,
    feedback: f32,
    damping: f32,
    tempo_sync: bool,
    delay_beats: f32,
    svf_left: StateVariableFilter,
    svf_right: StateVariableFilter,
    sample_rate: f32,
    bpm: u16,
}

impl DelayEffect {
    pub fn new(sample_rate: f32) -> Self {
        let max_delay_samples = (sample_rate * 8.0) as usize;
        DelayEffect {
            buffer_left: vec![0.0; max_delay_samples],
            buffer_right: vec![0.0; max_delay_samples],
            write_pos: 0,
            delay_samples: (sample_rate * 0.5) as f64,
            feedback: 0.4,
            damping: 0.3,
            tempo_sync: true,
            delay_beats: 1.0,
            svf_left: StateVariableFilter::default(),
            svf_right: StateVariableFilter::default(),
            sample_rate,
            bpm: 125,
        }
    }

    fn update_delay_samples(&mut self) {
        if self.tempo_sync && self.bpm > 0 {
            self.delay_samples = (60.0 / self.bpm as f64) * self.delay_beats as f64 * self.sample_rate as f64;
        }
    }
}

impl SendEffect for DelayEffect {
    fn name(&self) -> &str {
        "Stereo Delay"
    }

    fn param_count(&self) -> u32 {
        4
    }

    fn param_label(&self, index: u32) -> &str {
        match index {
            0 => "Delay",
            1 => "Feedback",
            2 => "Damping",
            3 => "Tempo Sync",
            _ => "",
        }
    }

    fn get_param(&self, index: u32) -> f32 {
        match index {
            0 => self.delay_beats,
            1 => self.feedback,
            2 => self.damping,
            3 => if self.tempo_sync { 1.0 } else { 0.0 },
            _ => 0.0,
        }
    }

    fn set_param(&mut self, index: u32, value: f32) {
        match index {
            0 => { self.delay_beats = value.clamp(0.0625, 8.0); self.update_delay_samples(); }
            1 => { self.feedback = value.clamp(0.0, 1.0); }
            2 => { self.damping = value.clamp(0.0, 1.0); }
            3 => { self.tempo_sync = value > 0.5; self.update_delay_samples(); }
            _ => {}
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32], bpm: u16, sample_rate: f32) {
        if self.sample_rate != sample_rate {
            self.sample_rate = sample_rate;
            self.update_delay_samples();
        }
        self.bpm = bpm;

        let buf_len = self.buffer_left.len();
        let delay_samples = self.delay_samples as usize;
        let feedback = self.feedback;
        let damp = self.damping;
        let damp_cutoff = 200.0 + damp * 18000.0;

        for i in 0..left.len().min(right.len()) {
            let read_pos = if delay_samples > 0 && delay_samples < buf_len {
                (self.write_pos + buf_len - delay_samples) % buf_len
            } else {
                self.write_pos
            };

            let wet_l = self.buffer_left[read_pos];
            let wet_r = self.buffer_right[read_pos];

            let filtered_l = self.svf_left.process(wet_l, damp_cutoff, 0.707, sample_rate);
            let filtered_r = self.svf_right.process(wet_r, damp_cutoff, 0.707, sample_rate);

            self.buffer_left[self.write_pos] = left[i] + filtered_l * feedback;
            self.buffer_right[self.write_pos] = right[i] + filtered_r * feedback;

            left[i] = filtered_l;
            right[i] = filtered_r;

            self.write_pos = (self.write_pos + 1) % buf_len;
        }
    }
}

pub struct ReverbEffect {
    comb_buf_l: [Vec<f32>; 4],
    comb_buf_r: [Vec<f32>; 4],
    comb_pos_l: [usize; 4],
    comb_pos_r: [usize; 4],
    comb_len: [usize; 4],
    allpass_buf_l: [Vec<f32>; 2],
    allpass_buf_r: [Vec<f32>; 2],
    allpass_pos_l: [usize; 2],
    allpass_pos_r: [usize; 2],
    allpass_len: [usize; 2],
    decay: f32,
    damping: f32,
    size: f32,
    stereo_width: f32,
    prev_l: f32,
    prev_r: f32,
    sample_rate: f32,
}

impl ReverbEffect {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        ReverbEffect {
            comb_buf_l: [vec![0.0; 8192], vec![0.0; 8192], vec![0.0; 8192], vec![0.0; 8192]],
            comb_buf_r: [vec![0.0; 8192], vec![0.0; 8192], vec![0.0; 8192], vec![0.0; 8192]],
            comb_pos_l: [0; 4],
            comb_pos_r: [0; 4],
            comb_len: [1323, 1632, 1808, 2205],
            allpass_buf_l: [vec![0.0; 2048], vec![0.0; 2048]],
            allpass_buf_r: [vec![0.0; 2048], vec![0.0; 2048]],
            allpass_pos_l: [0; 2],
            allpass_pos_r: [0; 2],
            allpass_len: [220, 441],
            decay: 0.7,
            damping: 0.5,
            size: 0.6,
            stereo_width: 0.5,
            prev_l: 0.0,
            prev_r: 0.0,
            sample_rate: sr,
        }
    }

    fn comb_process(buf: &mut [f32], pos: &mut usize, raw_len: usize, input: f32, feedback: f32, damp: f32, prev: &mut f32) -> f32 {
        let len = raw_len.min(buf.len()).max(1);
        let read = buf[*pos];
        let filtered = read * (1.0 - damp) + *prev * damp;
        *prev = filtered;
        buf[*pos] = input + filtered * feedback;
        *pos = (*pos + 1) % len;
        filtered
    }

    fn allpass_process(buf: &mut [f32], pos: &mut usize, raw_len: usize, input: f32, feedback: f32) -> f32 {
        let len = raw_len.min(buf.len()).max(1);
        let read = buf[*pos];
        buf[*pos] = input + read * feedback;
        *pos = (*pos + 1) % len;
        -input * feedback + read
    }
}

impl SendEffect for ReverbEffect {
    fn name(&self) -> &str {
        "Reverb"
    }

    fn param_count(&self) -> u32 {
        4
    }

    fn param_label(&self, index: u32) -> &str {
        match index {
            0 => "Decay",
            1 => "Damping",
            2 => "Size",
            3 => "Width",
            _ => "",
        }
    }

    fn get_param(&self, index: u32) -> f32 {
        match index {
            0 => self.decay,
            1 => self.damping,
            2 => self.size,
            3 => self.stereo_width,
            _ => 0.0,
        }
    }

    fn set_param(&mut self, index: u32, value: f32) {
        match index {
            0 => self.decay = value.clamp(0.0, 1.0),
            1 => self.damping = value.clamp(0.0, 1.0),
            2 => self.size = value.clamp(0.0, 1.0),
            3 => self.stereo_width = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32], _bpm: u16, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let max_comb = (0.050 * sample_rate) as usize;
        let scale = self.size * 0.8 + 0.2;
        let feedback = self.decay * 0.9 + 0.05;
        let damp = self.damping;
        let width = self.stereo_width;

        for i in 0..left.len().min(right.len()) {
            let input_l = left[i];
            let input_r = right[i];

            let mut out_l = 0.0;
            let mut out_r = 0.0;

            for c in 0..4 {
                let offset = if c % 2 == 0 { 0 } else { 100 };
                let cl = ((self.comb_len[c] as f32) * scale) as usize + offset;
                let cr = cl + (width * 200.0) as usize;
                let cl = cl.min(max_comb).max(1);
                let cr = cr.min(max_comb).max(1);
                out_l += Self::comb_process(
                    &mut self.comb_buf_l[c], &mut self.comb_pos_l[c], cl,
                    input_l, feedback, damp, &mut self.prev_l,
                );
                out_r += Self::comb_process(
                    &mut self.comb_buf_r[c], &mut self.comb_pos_r[c], cr,
                    input_r, feedback, damp, &mut self.prev_r,
                );
            }
            out_l *= 0.25;
            out_r *= 0.25;

            for a in 0..2 {
                let al = ((self.allpass_len[a] as f32) * scale) as usize;
                out_l = Self::allpass_process(&mut self.allpass_buf_l[a], &mut self.allpass_pos_l[a], al.max(1), out_l, 0.5);
                out_r = Self::allpass_process(&mut self.allpass_buf_r[a], &mut self.allpass_pos_r[a], al.max(1), out_r, 0.5);
            }

            left[i] = out_l;
            right[i] = out_r;
        }
    }
}

// ── Reusable LFO ──

struct Lfo {
    phase: f32,
    rate: f32,
    sample_rate: f32,
}

impl Lfo {
    fn new(sample_rate: f32) -> Self {
        Lfo { phase: 0.0, rate: 1.0, sample_rate }
    }

    fn set_rate(&mut self, rate_hz: f32) {
        self.rate = rate_hz;
    }

    fn next(&mut self) -> f32 {
        let value = (self.phase * 2.0 * std::f32::consts::PI).sin();
        self.phase += self.rate / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        value
    }
}

// ── Chorus ──

pub struct ChorusEffect {
    buffer_left: Vec<f32>,
    buffer_right: Vec<f32>,
    write_pos: usize,
    lfo: Lfo,
    rate: f32,
    depth: f32,
    feedback: f32,
    delay_ms: f32,
    sample_rate: f32,
}

impl ChorusEffect {
    pub fn new(sample_rate: f32) -> Self {
        let max_delay = (sample_rate * 0.05) as usize; // 50ms max
        ChorusEffect {
            buffer_left: vec![0.0; max_delay],
            buffer_right: vec![0.0; max_delay],
            write_pos: 0,
            lfo: Lfo::new(sample_rate),
            rate: 1.0,
            depth: 0.5,
            feedback: 0.3,
            delay_ms: 15.0,
            sample_rate,
        }
    }

    fn read_delay(&self, buf: &[f32], delay: usize) -> f32 {
        let len = buf.len();
        if delay >= len { return 0.0; }
        let pos = if delay <= self.write_pos {
            self.write_pos - delay
        } else {
            self.write_pos + len - delay
        };
        buf[pos.min(len - 1)]
    }
}

impl SendEffect for ChorusEffect {
    fn name(&self) -> &str { "Chorus" }

    fn param_count(&self) -> u32 { 4 }

    fn param_label(&self, index: u32) -> &str {
        match index {
            0 => "Rate",
            1 => "Depth",
            2 => "Feedback",
            3 => "Delay",
            _ => "",
        }
    }

    fn get_param(&self, index: u32) -> f32 {
        match index {
            0 => self.rate / 10.0,
            1 => self.depth,
            2 => self.feedback,
            3 => self.delay_ms / 30.0,
            _ => 0.0,
        }
    }

    fn set_param(&mut self, index: u32, value: f32) {
        match index {
            0 => { self.rate = (value * 10.0).clamp(0.1, 10.0); self.lfo.set_rate(self.rate); }
            1 => self.depth = value.clamp(0.0, 1.0),
            2 => self.feedback = value.clamp(0.0, 0.9),
            3 => self.delay_ms = (value * 30.0).clamp(1.0, 30.0),
            _ => {}
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32], _bpm: u16, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let center_delay = (self.delay_ms / 1000.0 * sample_rate) as usize;
        let depth_samples = (self.depth * 1.5 / 1000.0 * sample_rate) as usize;
        let buf_len = self.buffer_left.len();

        for i in 0..left.len().min(right.len()) {
            let lfo_val = self.lfo.next();

            // 3 taps with staggered LFO phases
            let tap_delays = [
                center_delay.saturating_add((lfo_val * depth_samples as f32) as usize),
                center_delay.saturating_add(((lfo_val * 0.7).sin() * depth_samples as f32) as usize),
                center_delay.saturating_add(((lfo_val * 0.3 + 1.0).sin() * depth_samples as f32) as usize),
            ];

            let mut wet_l = 0.0;
            let mut wet_r = 0.0;
            for &td in &tap_delays {
                wet_l += self.read_delay(&self.buffer_left, td.min(buf_len - 1));
                wet_r += self.read_delay(&self.buffer_right, td.min(buf_len - 1));
            }
            wet_l /= 3.0;
            wet_r /= 3.0;

            self.buffer_left[self.write_pos] = left[i] + wet_l * self.feedback;
            self.buffer_right[self.write_pos] = right[i] + wet_r * self.feedback;

            left[i] = wet_l;
            right[i] = wet_r;

            self.write_pos = (self.write_pos + 1) % buf_len;
        }
    }
}

// ── Flanger ──

pub struct FlangerEffect {
    buffer_left: Vec<f32>,
    buffer_right: Vec<f32>,
    write_pos: usize,
    lfo: Lfo,
    rate: f32,
    depth: f32,
    feedback: f32,
    delay_ms: f32,
    sample_rate: f32,
}

impl FlangerEffect {
    pub fn new(sample_rate: f32) -> Self {
        let max_delay = (sample_rate * 0.01) as usize; // 10ms max
        FlangerEffect {
            buffer_left: vec![0.0; max_delay],
            buffer_right: vec![0.0; max_delay],
            write_pos: 0,
            lfo: Lfo::new(sample_rate),
            rate: 0.5,
            depth: 0.7,
            feedback: 0.6,
            delay_ms: 1.0,
            sample_rate,
        }
    }

    fn read_delay(&self, buf: &[f32], delay: usize) -> f32 {
        let len = buf.len();
        if delay >= len { return 0.0; }
        let pos = if delay <= self.write_pos {
            self.write_pos - delay
        } else {
            self.write_pos + len - delay
        };
        buf[pos.min(len - 1)]
    }
}

impl SendEffect for FlangerEffect {
    fn name(&self) -> &str { "Flanger" }

    fn param_count(&self) -> u32 { 4 }

    fn param_label(&self, index: u32) -> &str {
        match index {
            0 => "Rate",
            1 => "Depth",
            2 => "Feedback",
            3 => "Delay",
            _ => "",
        }
    }

    fn get_param(&self, index: u32) -> f32 {
        match index {
            0 => self.rate / 5.0,
            1 => self.depth,
            2 => (self.feedback + 1.0) / 2.0,
            3 => self.delay_ms / 5.0,
            _ => 0.0,
        }
    }

    fn set_param(&mut self, index: u32, value: f32) {
        match index {
            0 => { self.rate = (value * 5.0).clamp(0.05, 5.0); self.lfo.set_rate(self.rate); }
            1 => self.depth = value.clamp(0.0, 1.0),
            2 => self.feedback = (value * 2.0 - 1.0).clamp(-0.95, 0.95),
            3 => self.delay_ms = (value * 5.0).clamp(0.1, 5.0),
            _ => {}
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32], _bpm: u16, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let base_delay = (self.delay_ms / 1000.0 * sample_rate) as usize;
        let depth_samples = (self.depth * 4.0 / 1000.0 * sample_rate) as usize;
        let buf_len = self.buffer_left.len();

        for i in 0..left.len().min(right.len()) {
            let lfo_val = self.lfo.next();

            let delay_l = base_delay + (lfo_val * depth_samples as f32) as usize;
            let delay_r = base_delay + ((-lfo_val) * depth_samples as f32) as usize;

            let wet_l = self.read_delay(&self.buffer_left, delay_l.min(buf_len - 1));
            let wet_r = self.read_delay(&self.buffer_right, delay_r.min(buf_len - 1));

            self.buffer_left[self.write_pos] = left[i] + wet_l * self.feedback;
            self.buffer_right[self.write_pos] = right[i] + wet_r * self.feedback;

            left[i] = wet_l;
            right[i] = wet_r;

            self.write_pos = (self.write_pos + 1) % buf_len;
        }
    }
}

// ── Phaser ──

struct AllpassStage {
    x1: f32,
    y1: f32,
}

impl AllpassStage {
    fn new() -> Self { AllpassStage { x1: 0.0, y1: 0.0 } }

    fn process(&mut self, input: f32, a: f32) -> f32 {
        let output = -a * input + self.x1 + a * self.y1;
        self.x1 = input;
        self.y1 = output;
        output
    }
}

pub struct PhaserEffect {
    stages_l: Vec<AllpassStage>,
    stages_r: Vec<AllpassStage>,
    lfo: Lfo,
    rate: f32,
    depth: f32,
    feedback: f32,
    num_stages: u32,
    fb_l: f32,
    fb_r: f32,
    sample_rate: f32,
}

impl PhaserEffect {
    pub fn new(sample_rate: f32) -> Self {
        let num = 6;
        PhaserEffect {
            stages_l: (0..num).map(|_| AllpassStage::new()).collect(),
            stages_r: (0..num).map(|_| AllpassStage::new()).collect(),
            lfo: Lfo::new(sample_rate),
            rate: 0.5,
            depth: 0.5,
            feedback: 0.4,
            num_stages: num,
            fb_l: 0.0,
            fb_r: 0.0,
            sample_rate,
        }
    }
}

impl SendEffect for PhaserEffect {
    fn name(&self) -> &str { "Phaser" }

    fn param_count(&self) -> u32 { 4 }

    fn param_label(&self, index: u32) -> &str {
        match index {
            0 => "Rate",
            1 => "Depth",
            2 => "Feedback",
            3 => "Stages",
            _ => "",
        }
    }

    fn get_param(&self, index: u32) -> f32 {
        match index {
            0 => self.rate / 10.0,
            1 => self.depth,
            2 => self.feedback,
            3 => self.num_stages as f32 / 12.0,
            _ => 0.0,
        }
    }

    fn set_param(&mut self, index: u32, value: f32) {
        match index {
            0 => { self.rate = (value * 10.0).clamp(0.05, 10.0); self.lfo.set_rate(self.rate); }
            1 => self.depth = value.clamp(0.0, 1.0),
            2 => self.feedback = value.clamp(0.0, 0.95),
            3 => {
                let new_stages = ((value * 12.0).round() as u32).clamp(2, 12);
                while (self.stages_l.len() as u32) < new_stages {
                    self.stages_l.push(AllpassStage::new());
                    self.stages_r.push(AllpassStage::new());
                }
                self.num_stages = new_stages;
            }
            _ => {}
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32], _bpm: u16, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let pi = std::f32::consts::PI;
        let base_freq = 400.0;
        let sweep = 2000.0;

        fn calc_a(freq: f32, sr: f32) -> f32 {
            let w = 2.0 * std::f32::consts::PI * freq / sr;
            let tan_half = (w / 2.0).tan();
            if tan_half == 0.0 { -1.0 } else { (1.0 - tan_half) / (1.0 + tan_half) }
        }

        for i in 0..left.len().min(right.len()) {
            let phase = self.lfo.phase;
            let lfo_val = (phase * 2.0 * pi).sin();
            let lfo_val_r = ((phase + 0.25) * 2.0 * pi).sin();

            let freq = base_freq + self.depth * lfo_val * sweep;
            let freq_r = base_freq + self.depth * lfo_val_r * sweep;
            let a = calc_a(freq, sample_rate).clamp(-1.0, 1.0);
            let a_r = calc_a(freq_r, sample_rate).clamp(-1.0, 1.0);

            let mut sig_l = left[i] + self.fb_l * self.feedback;
            for stage in self.stages_l[..self.num_stages as usize].iter_mut() {
                sig_l = stage.process(sig_l, a);
            }
            self.fb_l = sig_l;
            left[i] = sig_l;

            let mut sig_r = right[i] + self.fb_r * self.feedback;
            for stage in self.stages_r[..self.num_stages as usize].iter_mut() {
                sig_r = stage.process(sig_r, a_r);
            }
            self.fb_r = sig_r;
            right[i] = sig_r;

            self.lfo.phase += self.rate / sample_rate;
            if self.lfo.phase >= 1.0 { self.lfo.phase -= 1.0; }
        }
    }
}
