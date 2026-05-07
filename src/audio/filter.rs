use crate::sequencer::effect::FilterType;

const PI: f32 = std::f32::consts::PI;

#[derive(Clone, Debug)]
pub struct StateVariableFilter {
    pub low: f32,
    pub band: f32,
    pub high: f32,
    pub filter_type: FilterType,
}

impl Default for StateVariableFilter {
    fn default() -> Self {
        StateVariableFilter {
            low: 0.0,
            band: 0.0,
            high: 0.0,
            filter_type: FilterType::LowPass,
        }
    }
}

impl StateVariableFilter {
    pub fn process(&mut self, input: f32, cutoff: f32, resonance: f32, sample_rate: f32) -> f32 {
        let cutoff = cutoff.clamp(10.0, sample_rate * 0.49);
        let q = resonance.max(0.01);
        let f = 2.0 * (PI * cutoff / sample_rate).sin();
        self.high = input - self.low - q * self.band;
        self.band += f * self.high;
        self.low += f * self.band;
        match self.filter_type {
            FilterType::LowPass => self.low,
            FilterType::HighPass => self.high,
            FilterType::BandPass => self.band,
            FilterType::Notch => self.high + self.low,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svf_low_pass_attenuates_high_frequency() {
        let mut svf = StateVariableFilter::default();
        let sr = 48000.0;
        let cutoff = 200.0;
        let resonance = 0.7;
        let mut low_energy = 0.0f32;
        let mut high_energy = 0.0f32;
        for i in 0..4800 {
            let lo = (2.0 * PI * 50.0 * i as f32 / sr).sin();
            let hi = (2.0 * PI * 8000.0 * i as f32 / sr).sin();
            let mixed = lo + hi;
            let out = svf.process(mixed, cutoff, resonance, sr);
            if i > 2400 {
                low_energy += out * out;
            }
        }
        assert!(low_energy > 0.0, "LP filter should produce output");
    }

    #[test]
    fn svf_default_filter_type_is_lowpass() {
        let svf = StateVariableFilter::default();
        assert_eq!(svf.filter_type, FilterType::LowPass);
    }
}