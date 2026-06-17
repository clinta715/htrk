/// Tracks timing state for the sequencer: BPM, speed, tick counter, and sample accumulation.
///
/// Encapsulates the fields previously inline in `SequencerState` and the
/// `compute_samples_per_tick` function from `effects/mod.rs`.
#[derive(Clone, Debug)]
pub struct SequencerClock {
    /// Beats per minute
    pub bpm: u16,
    /// Ticks per row
    pub speed: u8,
    /// Current tick within the row (0 .. speed-1)
    pub current_tick: u8,
    /// Samples per tick (computed from BPM and sample rate)
    pub samples_per_tick: f64,
    /// Accumulated sample counter for tick scheduling
    pub sample_counter: f64,
    /// Automation multiplier for tempo
    pub auto_tempo_factor: f32,
    /// Sample rate for internal recalculations
    sample_rate: f64,
}

impl SequencerClock {
    pub fn new(bpm: u16, speed: u8, sample_rate: f64) -> Self {
        let mut clock = SequencerClock {
            bpm,
            speed,
            current_tick: 0,
            samples_per_tick: 0.0,
            sample_counter: 0.0,
            auto_tempo_factor: 1.0,
            sample_rate,
        };
        clock.recalculate();
        clock
    }

    pub fn reset(&mut self) {
        self.current_tick = 0;
        self.sample_counter = 0.0;
    }

    /// Advance the counter by `samples` and return the number of complete
    /// tick boundaries crossed.  The caller should call `on_tick_processed`
    /// for each tick it processes.
    pub fn advance(&mut self, samples: f64) -> u32 {
        if self.samples_per_tick <= 0.0 {
            return 0;
        }
        self.sample_counter += samples;
        let mut ticks = 0;
        while self.sample_counter >= self.samples_per_tick {
            self.sample_counter -= self.samples_per_tick;
            ticks += 1;
        }
        ticks
    }

    /// Call after the engine has processed one tick.  Returns `true` when
    /// the row should advance (current_tick wrapped around).
    pub fn on_tick_processed(&mut self) -> bool {
        self.current_tick += 1;
        if self.current_tick >= self.speed {
            self.current_tick = 0;
            true
        } else {
            false
        }
    }

    pub fn set_bpm(&mut self, bpm: u16) {
        self.bpm = bpm;
        self.recalculate();
    }

    pub fn set_speed(&mut self, speed: u8) {
        self.speed = speed;
    }

    /// XM-compatible tempo command: `value >= 32` sets BPM, `value < 32`
    /// sets speed (ticks per row).  Returns `true` if BPM was changed,
    /// `false` if speed was changed.
    pub fn set_tempo(&mut self, value: u8) -> bool {
        if value >= 32 {
            self.set_bpm(value as u16);
            true
        } else if value > 0 {
            self.set_speed(value);
            false
        } else {
            false
        }
    }

    fn recalculate(&mut self) {
        let safe_bpm = if self.bpm == 0 { 125.0 } else { self.bpm as f64 };
        self.samples_per_tick = self.sample_rate * 5.0 / (safe_bpm * 2.0);
    }

    /// Temporarily expose sample_rate for places that need it during migration.
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }
}
