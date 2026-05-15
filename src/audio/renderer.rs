use std::sync::Arc;
use crate::audio::commands::{InterpolationType, LimiterMode};
use crate::audio::mixer;
use crate::audio::sequencer_engine::SequencerEngine;
use crate::sequencer::Module;
use crate::errors::FormatResult;
use crate::ui::wav_export_window::{BitDepth, WavExportSettings};

pub struct WavRenderer {
    sequencer: SequencerEngine,
    sample_rate: u32,
    interpolation: InterpolationType,
    limiter_mode: LimiterMode,
    limiter_gain: f32,
    master_volume: f32,
    stereo: bool,
}

impl WavRenderer {
    pub fn new(module: Arc<Module>, sample_rate: u32) -> Self {
        let mut sequencer = SequencerEngine::new(sample_rate as f64);
        sequencer.load_module(module);
        sequencer.play();
        Self {
            sequencer,
            sample_rate,
            interpolation: InterpolationType::Linear,
            limiter_mode: LimiterMode::HardClip,
            limiter_gain: 1.0,
            master_volume: 0.5,
            stereo: true,
        }
    }

    pub fn set_interpolation(&mut self, interpolation: InterpolationType) {
        self.interpolation = interpolation;
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume;
    }

    pub fn set_limiter_mode(&mut self, mode: LimiterMode) {
        self.limiter_mode = mode;
        self.limiter_gain = 1.0;
    }

    pub fn set_channels(&mut self, stereo: bool) {
        self.stereo = stereo;
    }

    pub fn render_with_settings<W: std::io::Write + std::io::Seek, F>(&mut self, writer: &mut hound::WavWriter<W>, settings: &WavExportSettings, progress_cb: F) -> FormatResult<()> 
    where F: FnMut(f32) -> bool
    {
        self.stereo = settings.channel_mode == crate::ui::wav_export_window::ChannelMode::Stereo;
        self.render_with_bitdepth(writer, settings.bit_depth, progress_cb)
    }

    fn render_with_bitdepth<W: std::io::Write + std::io::Seek, F>(&mut self, writer: &mut hound::WavWriter<W>, bit_depth: BitDepth, mut progress_cb: F) -> FormatResult<()> 
    where F: FnMut(f32) -> bool
    {
        let buffer_size = 1024;
        let mut left = vec![0.0f32; buffer_size];
        let mut right = vec![0.0f32; buffer_size];

        let total_orders = self.sequencer.module.as_ref().map(|m| m.order_list.len()).unwrap_or(1) as f32;

        while self.sequencer.state.playing {
            let current_order = self.sequencer.state.current_order as f32;
            let current_row = self.sequencer.state.current_row as f32;
            let rows_in_pattern = self.sequencer.module.as_ref()
                .and_then(|m| {
                    let order_idx = self.sequencer.state.current_order as usize;
                    let order = m.order_list.get(order_idx)?;
                    m.patterns.get(*order as usize)
                })
                .map(|p| p.num_rows)
                .unwrap_or(64) as f32;
            
            let progress = (current_order + (current_row / rows_in_pattern)) / total_orders;
            if !progress_cb(progress.min(1.0)) {
                return Ok(());
            }

            left.fill(0.0);
            right.fill(0.0);

            let frame_count = left.len();
            let mut samples_done = 0;

            while samples_done < frame_count && self.sequencer.state.playing {
                let samples_per_tick = self.sequencer.state.samples_per_tick.max(1.0);
                if self.sequencer.state.sample_counter >= samples_per_tick {
                    self.sequencer.process_tick();
                    self.sequencer.state.sample_counter -= samples_per_tick;
                    if !self.sequencer.state.playing { break; }
                }

                let samples_remaining_in_tick = samples_per_tick - self.sequencer.state.sample_counter;
                let samples_remaining_in_buffer = (frame_count - samples_done) as f64;
                let chunk_f = samples_remaining_in_tick.min(samples_remaining_in_buffer);
                let chunk = chunk_f.ceil() as usize;
                let chunk = chunk.min(frame_count - samples_done);

                if chunk == 0 { break; }

                let muted: Vec<bool> = self.sequencer.state.channels.iter().map(|ch| ch.muted).collect();

                mixer::mix_voices(
                    &mut self.sequencer.voices,
                    &mut left[samples_done..samples_done + chunk],
                    &mut right[samples_done..samples_done + chunk],
                    self.master_volume,
                    self.interpolation,
                    &muted,
                    self.sample_rate as f32,
                );

                self.sequencer.state.sample_counter += chunk as f64;
                samples_done += chunk;
            }

            match self.limiter_mode {
                LimiterMode::HardClip => {
                    mixer::buffer_wide_limit(&mut left, &mut right);
                }
                LimiterMode::SoftKnee => {
                    mixer::soft_knee_limit(&mut left, &mut right);
                }
                LimiterMode::SoftKneeSmooth => {
                    mixer::soft_knee_limit_smoothed(&mut left, &mut right, &mut self.limiter_gain);
                }
            }

            match bit_depth {
                BitDepth::Bits8 => {
                    for i in 0..frame_count {
                        let mono = if self.stereo {
                            (left[i] + right[i]) / 2.0
                        } else {
                            left[i]
                        };
                        writer.write_sample((mono.clamp(-1.0, 1.0) * 127.0) as i8)?;
                        if self.stereo {
                            writer.write_sample((right[i].clamp(-1.0, 1.0) * 127.0) as i8)?;
                        }
                    }
                }
                BitDepth::Bits16 => {
                    for i in 0..frame_count {
                        let mono = if self.stereo {
                            (left[i] + right[i]) / 2.0
                        } else {
                            left[i]
                        };
                        writer.write_sample((mono.clamp(-1.0, 1.0) * 32767.0) as i16)?;
                        if self.stereo {
                            writer.write_sample((right[i].clamp(-1.0, 1.0) * 32767.0) as i16)?;
                        }
                    }
                }
                BitDepth::Bits24 => {
                    for i in 0..frame_count {
                        let mono = if self.stereo {
                            (left[i] + right[i]) / 2.0
                        } else {
                            left[i]
                        };
                        writer.write_sample((mono.clamp(-1.0, 1.0) * 8388607.0) as i32)?;
                        if self.stereo {
                            writer.write_sample((right[i].clamp(-1.0, 1.0) * 8388607.0) as i32)?;
                        }
                    }
                }
                BitDepth::Bits32 => {
                    for i in 0..frame_count {
                        let mono = if self.stereo {
                            (left[i] + right[i]) / 2.0
                        } else {
                            left[i]
                        };
                        writer.write_sample((mono.clamp(-1.0, 1.0) * 2147483647.0) as i32)?;
                        if self.stereo {
                            writer.write_sample((right[i].clamp(-1.0, 1.0) * 2147483647.0) as i32)?;
                        }
                    }
                }
                BitDepth::Bits32Float => {
                    for i in 0..frame_count {
                        let mono = if self.stereo {
                            (left[i] + right[i]) / 2.0
                        } else {
                            left[i]
                        };
                        writer.write_sample(mono.clamp(-1.0, 1.0))?;
                        if self.stereo {
                            writer.write_sample(right[i].clamp(-1.0, 1.0))?;
                        }
                    }
                }
            }
        }
        
        progress_cb(1.0);
        Ok(())
    }

    pub fn render<W: std::io::Write + std::io::Seek, F>(&mut self, writer: &mut hound::WavWriter<W>, progress_cb: F) -> FormatResult<()> 
    where F: FnMut(f32) -> bool
    {
        self.render_with_bitdepth(writer, BitDepth::Bits16, progress_cb)
    }
}
