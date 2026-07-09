use crate::audio::commands::InterpolationType;
use crate::audio::resampler;
use crate::audio::voice::Voice;
#[cfg(feature = "audio_debug")]
use crate::debug_log;
use crate::sequencer::effect::FilterType;
use crate::sequencer::sample::LoopType;

use crate::audio::playback_state::MAX_CHANNELS;

/// Maximum length (in samples) of the note-onset anti-click fade-in. When a
/// voice triggers, `smoothed_volume` starts at 0 and ramps to `final_volume`
/// over at most this many samples (≈1.3 ms at 48 kHz), regardless of the tick
/// chunk length, so the fade is always a quick de-click rather than a slow
/// swell. After the onset ramp completes, gentle inter-tick ramping takes over.
const ONSET_RAMP_SAMPLES: usize = 64;

/// Per-voice per-sample gain ramp state. Holds only copied `f32`/`bool` values
/// (no borrow of `Voice`), so it can be advanced inside the mix loop while an
/// immutable borrow of `voice.sample` is live. The final smoothed values are
/// written back to the voice via `finish` after the loop.
struct GainRamp {
    target_vol: f32,
    target_pan: f32,
    vol_step: f32,
    pan_step: f32,
    cur_vol: f32,
    cur_pan: f32,
    ramp: bool,
}

impl GainRamp {
    /// Build the ramp setup for a voice about to render `len` samples.
    #[inline]
    fn new(voice: &Voice, master_volume: f32, len: usize) -> Self {
        let target_vol = voice.final_volume * master_volume;
        let target_pan = voice.final_panning;
        if voice.ramp_enabled {
            // Onset ramp: cap the fade-in length so a long tick chunk doesn't
            // turn the de-click into a slow swell. Inter-tick ramping (once
            // smoothed_volume ≈ target) naturally uses the full chunk length.
            let ramp_len = if voice.smoothed_volume == 0.0 {
                len.clamp(1, ONSET_RAMP_SAMPLES)
            } else {
                len.max(1)
            };
            let inv = 1.0 / ramp_len as f32;
            let vol_step = (target_vol - voice.smoothed_volume) * inv;
            let pan_step = (target_pan - voice.smoothed_panning) * inv;
            GainRamp {
                target_vol,
                target_pan,
                vol_step,
                pan_step,
                cur_vol: voice.smoothed_volume,
                cur_pan: voice.smoothed_panning,
                ramp: true,
            }
        } else {
            // Flat-gain bit-exact path: start at the target, zero step.
            GainRamp {
                target_vol,
                target_pan,
                vol_step: 0.0,
                pan_step: 0.0,
                cur_vol: target_vol,
                cur_pan: target_pan,
                ramp: false,
            }
        }
    }

    /// Advance one sample and return `(vol, pan)`, clamped to the target so a
    /// fractional step never overshoots in either direction.
    #[inline]
    fn next(&mut self) -> (f32, f32) {
        if self.ramp {
            let nv = self.cur_vol + self.vol_step;
            self.cur_vol = if self.target_vol >= self.cur_vol {
                nv.min(self.target_vol)
            } else {
                nv.max(self.target_vol)
            };
            let np = self.cur_pan + self.pan_step;
            self.cur_pan = if self.target_pan >= self.cur_pan {
                np.min(self.target_pan)
            } else {
                np.max(self.target_pan)
            };
        }
        (self.cur_vol, self.cur_pan)
    }

    /// Write the final smoothed values back to the voice so the next chunk
    /// continues smoothly. Called once after the render loop.
    #[inline]
    fn finish(self, voice: &mut Voice) {
        voice.smoothed_volume = self.cur_vol;
        voice.smoothed_panning = self.cur_pan;
    }
}

pub fn mix_voices_per_channel(
    voices: &mut [Voice],
    output_left: &mut [f32],
    output_right: &mut [f32],
    ch_mix: &mut [f32],        // flat: [ch0_L, ch0_R, ch1_L, ch1_R, ...] for each frame
    pre_ch_mix: &mut [f32],    // flat: [ch0_L, ch0_R, ch1_L, ch1_R, ...] for each frame
    offset: usize,
    len: usize,
    stride: usize,             // total buffer size per channel pair (frame_count)
    master_volume: f32,
    interpolation: InterpolationType,
    muted_channels: &[bool],
    sample_rate: f32,
    num_channels: usize,
) {
    #[cfg(feature = "audio_debug")]
    static VD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // Defensive bounds check: ch_mix / pre_ch_mix must be sized
    // num_channels * 2 * stride by the caller. A future buffer-sizing
    // change that breaks this invariant would otherwise panic in the
    // realtime audio thread; this catches it in debug builds only
    // (zero cost in release).
    debug_assert!(
        num_channels == 0 || num_channels * 2 * stride <= ch_mix.len(),
        "ch_mix buffer too small: need {} samples for {} channels, have {}",
        num_channels * 2 * stride, num_channels, ch_mix.len()
    );
    debug_assert!(
        num_channels == 0 || num_channels * 2 * stride <= pre_ch_mix.len(),
        "pre_ch_mix buffer too small: need {} samples for {} channels, have {}",
        num_channels * 2 * stride, num_channels, pre_ch_mix.len()
    );
    for voice in voices.iter_mut() {
        if !voice.active {
            continue;
        }
        #[cfg(feature = "audio_debug")]
        let vd = VD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        #[cfg(feature = "audio_debug")]
        if vd < 5 {
            debug_log!("[VOICE] ch={:?} base_vol={:.4} final_vol={:.4} pan={:.4}",
                voice.channel, voice.base_volume, voice.final_volume, voice.final_panning);
        }

        if let Some(ch) = voice.channel {
            if ch < muted_channels.len() && muted_channels[ch] {
                continue;
            }
        }

        if voice.karplus_strong {
            let delay_len = voice.ks_delay_line.len();
            if delay_len == 0 {
                voice.active = false;
                continue;
            }
            let mut ramp = GainRamp::new(voice, master_volume, len);
            let ch_idx = voice.channel.unwrap_or(MAX_CHANNELS);
            for i in 0..len {
                if voice.ks_pos >= delay_len {
                    voice.ks_pos = 0;
                }
                let s = voice.ks_delay_line[voice.ks_pos];
                let next = if voice.ks_pos + 1 < delay_len {
                    voice.ks_delay_line[voice.ks_pos + 1]
                } else {
                    voice.ks_delay_line[0]
                };
                voice.ks_delay_line[voice.ks_pos] = (s + next) * 0.5 * voice.ks_feedback;
                let (vol, pan) = ramp.next();
                let fl = s * (vol * (1.0 - pan));
                let fr = s * (vol * pan);
                output_left[i] += fl;
                output_right[i] += fr;
                if ch_idx < num_channels {
                    let base = ch_idx * 2 * stride;
                    ch_mix[base + offset + i] += fl;
                    ch_mix[base + stride + offset + i] += fr;
                    // Pre-fader uses the same ramped gain.
                    pre_ch_mix[base + offset + i] += fl;
                    pre_ch_mix[base + stride + offset + i] += fr;
                }
            }
            ramp.finish(voice);
            continue;
        }

        let sample_data = match &voice.sample {
            Some(data) => match std::sync::Arc::as_ref(data) {
                s if !s.is_empty() => s,
                _ => continue,
            },
            None => continue,
        };

        let loop_type = voice.loop_type;
        let loop_start = voice.loop_start;
        let loop_end = voice.loop_end;

        let mut ramp = GainRamp::new(voice, master_volume, len);

        let has_filter = voice.filter_cutoff < 65534.0
            && voice.filter_resonance > 0.001
            && (voice.filter_env.is_some() || voice.svf.filter_type != FilterType::LowPass)
            && voice.filter_enabled;

        let cutoff_hz = if has_filter {
            let base_cutoff = voice.filter_cutoff * voice.auto_cutoff_mult;
            let env_mod = voice.envelope_filter_cutoff;
            let cutoff_frac = (base_cutoff / 65535.0).clamp(0.0, 1.0);
            let env_cutoff_frac = cutoff_frac * env_mod;
            20.0 * (1000.0_f32).powf(env_cutoff_frac)
        } else {
            0.0
        };

        let ch_idx = voice.channel.unwrap_or(MAX_CHANNELS);

        for i in 0..len {
            if voice.position < 0.0 || voice.position as usize >= sample_data.len() {
                voice.active = false;
                break;
            }

            let s = resampler::resample(
                sample_data,
                voice.position,
                loop_start,
                loop_end,
                interpolation,
                loop_type,
            );

            let filtered = if has_filter {
                voice.svf.process(s, cutoff_hz, voice.filter_resonance, sample_rate)
            } else {
                s
            };

            let led_filtered = if voice.amiga_led_filter {
                voice.amiga_led_svf.process(filtered, 3100.0, 0.707, sample_rate)
            } else {
                filtered
            };

            let (vol, pan) = ramp.next();
            let fl = led_filtered * (vol * (1.0 - pan));
            let fr = led_filtered * (vol * pan);
            output_left[i] += fl;
            output_right[i] += fr;

            if ch_idx < num_channels {
                let base = ch_idx * 2 * stride;
                ch_mix[base + offset + i] += fl;
                ch_mix[base + stride + offset + i] += fr;
                // Pre-fader metering uses the same ramped gain.
                pre_ch_mix[base + offset + i] += fl;
                pre_ch_mix[base + stride + offset + i] += fr;
            }

            voice.position += voice.sample_delta * voice.direction;

            match loop_type {
                LoopType::Forward => {
                    if loop_end > loop_start && voice.position >= loop_end as f64 {
                        let loop_len = (loop_end - loop_start) as f64;
                        if loop_len > 0.0 {
                            voice.position = loop_start as f64 + (voice.position - loop_start as f64) % loop_len;
                        } else {
                            voice.active = false;
                            break;
                        }
                    }
                    if voice.position as usize >= sample_data.len() {
                        voice.active = false;
                        break;
                    }
                }
                LoopType::PingPong => {
                    if loop_end > loop_start {
                        if voice.direction > 0.0 && voice.position >= loop_end as f64 {
                            voice.position = 2.0 * loop_end as f64 - voice.position;
                            voice.direction = -1.0;
                        } else if voice.direction < 0.0 && voice.position < loop_start as f64 {
                            voice.position = 2.0 * loop_start as f64 - voice.position;
                            voice.direction = 1.0;
                        }
                        if voice.position < 0.0 {
                            voice.active = false;
                            break;
                        }
                    } else if voice.position as usize >= sample_data.len() {
                        voice.active = false;
                        break;
                    }
                }
                LoopType::Backward => {
                    if loop_end > loop_start && voice.position < loop_start as f64 {
                        let loop_len = (loop_end - loop_start) as f64;
                        let offset_inner = (loop_start as f64 - voice.position) % loop_len;
                        voice.position = (loop_end as f64) - offset_inner;
                        if voice.position >= loop_end as f64 {
                            voice.position = (loop_end - 1) as f64;
                        }
                    } else if voice.position < 0.0 || voice.position as usize >= sample_data.len() {
                        voice.active = false;
                        break;
                    }
                }
                LoopType::None => {
                    if voice.position as usize >= sample_data.len() || voice.position < 0.0 {
                        voice.active = false;
                        break;
                    }
                }
            }
        }
        ramp.finish(voice);
    }
}

pub fn mix_voices(
    voices: &mut [Voice],
    output_left: &mut [f32],
    output_right: &mut [f32],
    master_volume: f32,
    interpolation: InterpolationType,
    muted_channels: &[bool],
    sample_rate: f32,
) {
    #[cfg(feature = "audio_debug")]
    static VD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    for voice in voices.iter_mut() {
        if !voice.active {
            continue;
        }
        #[cfg(feature = "audio_debug")]
        let vd = VD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        #[cfg(feature = "audio_debug")]
        if vd < 5 {
            debug_log!("[VOICE] ch={:?} base_vol={:.4} final_vol={:.4} pan={:.4}",
                voice.channel, voice.base_volume, voice.final_volume, voice.final_panning);
        }

        if let Some(ch) = voice.channel {
            if ch < muted_channels.len() && muted_channels[ch] {
                continue;
            }
        }

        if voice.karplus_strong {
            let delay_len = voice.ks_delay_line.len();
            if delay_len == 0 {
                voice.active = false;
                continue;
            }
            let len = output_left.len();
            let mut ramp = GainRamp::new(voice, master_volume, len);
            for i in 0..len {
                if voice.ks_pos >= delay_len {
                    voice.ks_pos = 0;
                }
                let s = voice.ks_delay_line[voice.ks_pos];
                let next = if voice.ks_pos + 1 < delay_len {
                    voice.ks_delay_line[voice.ks_pos + 1]
                } else {
                    voice.ks_delay_line[0]
                };
                voice.ks_delay_line[voice.ks_pos] = (s + next) * 0.5 * voice.ks_feedback;
                let (vol, pan) = ramp.next();
                output_left[i] += s * (vol * (1.0 - pan));
                output_right[i] += s * (vol * pan);
                voice.ks_pos += 1;
            }
            ramp.finish(voice);
            continue;
        }

        let sample_data = match &voice.sample {
            Some(data) => match std::sync::Arc::as_ref(data) {
                s if !s.is_empty() => s,
                _ => continue,
            },
            None => continue,
        };

        let loop_type = voice.loop_type;
        let loop_start = voice.loop_start;
        let loop_end = voice.loop_end;

        let len = output_left.len();
        let mut ramp = GainRamp::new(voice, master_volume, len);

        let has_filter = voice.filter_cutoff < 65534.0
            && voice.filter_resonance > 0.001
            && (voice.filter_env.is_some() || voice.svf.filter_type != FilterType::LowPass)
            && voice.filter_enabled;

        let cutoff_hz = if has_filter {
            let base_cutoff = voice.filter_cutoff * voice.auto_cutoff_mult;
            let env_mod = voice.envelope_filter_cutoff;
            let cutoff_frac = (base_cutoff / 65535.0).clamp(0.0, 1.0);
            let env_cutoff_frac = cutoff_frac * env_mod;
            20.0 * (1000.0_f32).powf(env_cutoff_frac)
        } else {
            0.0
        };

        for i in 0..len {
            if voice.position < 0.0 || voice.position as usize >= sample_data.len() {
                voice.active = false;
                break;
            }

            let s = resampler::resample(
                sample_data,
                voice.position,
                loop_start,
                loop_end,
                interpolation,
                loop_type,
            );

            let filtered = if has_filter {
                voice.svf.process(s, cutoff_hz, voice.filter_resonance, sample_rate)
            } else {
                s
            };

            let led_filtered = if voice.amiga_led_filter {
                voice.amiga_led_svf.process(filtered, 3100.0, 0.707, sample_rate)
            } else {
                filtered
            };

            let (vol, pan) = ramp.next();
            output_left[i] += led_filtered * (vol * (1.0 - pan));
            output_right[i] += led_filtered * (vol * pan);

            voice.position += voice.sample_delta * voice.direction;

            match loop_type {
                LoopType::Forward => {
                    if loop_end > loop_start && voice.position >= loop_end as f64 {
                        let loop_len = (loop_end - loop_start) as f64;
                        if loop_len > 0.0 {
                            voice.position = loop_start as f64 + (voice.position - loop_start as f64) % loop_len;
                        } else {
                            voice.active = false;
                            break;
                        }
                    }
                    if voice.position as usize >= sample_data.len() {
                        voice.active = false;
                        break;
                    }
                }
                LoopType::PingPong => {
                    if loop_end > loop_start {
                        if voice.direction > 0.0 && voice.position >= loop_end as f64 {
                            voice.position = 2.0 * loop_end as f64 - voice.position;
                            voice.direction = -1.0;
                        } else if voice.direction < 0.0 && voice.position < loop_start as f64 {
                            voice.position = 2.0 * loop_start as f64 - voice.position;
                            voice.direction = 1.0;
                        }
                        if voice.position < 0.0 {
                            voice.active = false;
                            break;
                        }
                    } else if voice.position as usize >= sample_data.len() {
                        voice.active = false;
                        break;
                    }
                }
                LoopType::Backward => {
                    if loop_end > loop_start && voice.position < loop_start as f64 {
                        let loop_len = (loop_end - loop_start) as f64;
                        let offset = (loop_start as f64 - voice.position) % loop_len;
                        voice.position = (loop_end as f64) - offset;
                        if voice.position >= loop_end as f64 {
                            voice.position = (loop_end - 1) as f64;
                        }
                    } else if voice.position < 0.0 || voice.position as usize >= sample_data.len() {
                        voice.active = false;
                        break;
                    }
                }
                LoopType::None => {
                    if voice.position as usize >= sample_data.len() || voice.position < 0.0 {
                        voice.active = false;
                        break;
                    }
                }
            }
        }
        ramp.finish(voice);
    }
}

fn peak_stereo(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .chain(right.iter())
        .map(|s| s.abs())
        .fold(0.0f32, f32::max)
}

pub fn soft_knee_limit(output_left: &mut [f32], output_right: &mut [f32]) {
    const THRESHOLD: f32 = 0.8;
    const RANGE: f32 = 1.0 - THRESHOLD;

    let peak = peak_stereo(output_left, output_right);
    if peak > THRESHOLD {
        let excess = peak - THRESHOLD;
        let compressed_peak = THRESHOLD + RANGE * (1.0 - (-excess / RANGE).exp());
        let gain = compressed_peak / peak;
        for l in output_left.iter_mut() { *l *= gain; }
        for r in output_right.iter_mut() { *r *= gain; }
    }
}

pub fn soft_knee_limit_smoothed(
    output_left: &mut [f32],
    output_right: &mut [f32],
    limiter_gain: &mut f32,
) {
    const THRESHOLD: f32 = 0.8;
    const RANGE: f32 = 1.0 - THRESHOLD;
    const RELEASE: f32 = 0.05;

    let peak = peak_stereo(output_left, output_right);

    let target = if peak > THRESHOLD {
        let excess = peak - THRESHOLD;
        let compressed_peak = THRESHOLD + RANGE * (1.0 - (-excess / RANGE).exp());
        compressed_peak / peak
    } else {
        1.0
    };

    // Instant attack, smooth release
    if target < *limiter_gain {
        *limiter_gain = target;
    } else {
        *limiter_gain += (target - *limiter_gain) * RELEASE;
    }

    for l in output_left.iter_mut() { *l *= *limiter_gain; }
    for r in output_right.iter_mut() { *r *= *limiter_gain; }
}

pub fn brick_wall_limit(output_left: &mut [f32], output_right: &mut [f32]) {
    buffer_wide_limit(output_left, output_right)
}

pub fn buffer_wide_limit(output_left: &mut [f32], output_right: &mut [f32]) {
    let peak = peak_stereo(output_left, output_right);
    if peak > 1.0 {
        let gain = 1.0 / peak;
        for l in output_left.iter_mut() { *l *= gain; }
        for r in output_right.iter_mut() { *r *= gain; }
    }
}

pub fn interleave_to_stereo(left: &[f32], right: &[f32], output: &mut [f32]) {
    for (i, frame) in output.chunks_exact_mut(2).enumerate() {
        if i < left.len() && i < right.len() {
            frame[0] = left[i];
            frame[1] = right[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::voice::Voice;
    use crate::sequencer::sample::LoopType;
    use std::sync::Arc;

    fn make_sine_voice(freq: f64, sample_rate: f64, duration_secs: f64) -> Voice {
        let num_samples = (sample_rate * duration_secs) as usize;
        let data: Vec<f32> = (0..num_samples)
            .map(|i| {
                ((2.0 * std::f64::consts::PI * freq * i as f64 / sample_rate).sin() as f32) * 0.5
            })
            .collect();

        let mut voice = Voice::default();
        voice.active = true;
        voice.sample = Some(Arc::new(data));
        voice.sample_rate = sample_rate;
        voice.loop_type = LoopType::None;
        voice.position = 0.0;
        voice.position_end = num_samples as f64;
        voice.sample_delta = freq / sample_rate * 2.0;
        voice.base_volume = 0.5;
        voice.envelope_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 0.5;
        voice.smoothed_volume = 0.5;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;
        voice.smoothed_panning = 0.5;
        voice
    }

    #[test]
    fn mix_single_voice_produces_output() {
        let voice = make_sine_voice(440.0, 48000.0, 0.01);
        let buf_len = 256;
        let mut left = vec![0.0f32; buf_len];
        let mut right = vec![0.0f32; buf_len];

        let mut voices = [voice];
        mix_voices(
            &mut voices,
            &mut left,
            &mut right,
            1.0,
            InterpolationType::Linear,
            &[],
            48000.0,
        );

        let has_audio = left.iter().any(|&s| s.abs() > 0.001);
        assert!(has_audio);
    }

    #[test]
    fn mix_inactive_voice_silence() {
        let voice = Voice::default();
        let buf_len = 256;
        let mut left = vec![0.0f32; buf_len];
        let mut right = vec![0.0f32; buf_len];

        let mut voices = [voice];
        mix_voices(
            &mut voices,
            &mut left,
            &mut right,
            1.0,
            InterpolationType::Linear,
            &[],
            48000.0,
        );

        assert!(left.iter().all(|&s| s == 0.0));
        assert!(right.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn mix_muted_channel_silence() {
        let mut voice = make_sine_voice(440.0, 48000.0, 0.01);
        voice.channel = Some(0);
        let buf_len = 256;
        let mut left = vec![0.0f32; buf_len];
        let mut right = vec![0.0f32; buf_len];

        let muted = [true];

        let mut voices = [voice];
        mix_voices(
            &mut voices,
            &mut left,
            &mut right,
            1.0,
            InterpolationType::Linear,
            &muted,
            48000.0,
        );

        assert!(left.iter().all(|&s| s == 0.0));
        assert!(right.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn brick_wall_limit_soft_limiter() {
        let mut left = vec![2.0f32, -1.5f32, 0.5f32];
        let mut right = vec![-2.0f32, 1.5f32, -0.5f32];

        brick_wall_limit(&mut left, &mut right);

        // Buffer-wide: peak=2.0, gain=0.5 applied uniformly
        assert!((left[0] - 1.0).abs() < 0.001);
        assert!((left[1] + 0.75).abs() < 0.001);
        assert!((left[2] - 0.25).abs() < 0.001);
        assert!((right[0] + 1.0).abs() < 0.001);
        assert!((right[1] - 0.75).abs() < 0.001);
        assert!((right[2] + 0.25).abs() < 0.001);

        assert!((left[0] / right[0] - left[1] / right[1]).abs() < 0.001,
            "uniform gain preserves left/right ratio");
    }

    #[test]
    fn interleave_to_stereo_works() {
        let left = [0.1f32, 0.2, 0.3];
        let right = [0.4f32, 0.5, 0.6];
        let mut output = [0.0f32; 6];

        super::interleave_to_stereo(&left, &right, &mut output);

        assert_eq!(output[0], 0.1);
        assert_eq!(output[1], 0.4);
        assert_eq!(output[2], 0.2);
        assert_eq!(output[3], 0.5);
        assert_eq!(output[4], 0.3);
        assert_eq!(output[5], 0.6);
    }

    #[test]
    fn loop_forward_wraps_and_sustains() {
        let data: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let mut voice = Voice::default();
        voice.active = true;
        voice.sample = Some(Arc::new(data.clone()));
        voice.sample_rate = 48000.0;
        voice.loop_type = LoopType::Forward;
        voice.loop_start = 2;
        voice.loop_end = 6;
        voice.position = 5.0;
        voice.position_end = data.len() as f64;
        voice.sample_delta = 0.5;
        voice.base_volume = 1.0;
        voice.envelope_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 1.0;
        voice.smoothed_volume = 1.0;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;
        voice.smoothed_panning = 0.5;

        let mut left = vec![0.0f32; 10];
        let mut right = vec![0.0f32; 10];
        let mut voices = [voice];
        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Nearest, &[], 48000.0);
        let voice = &voices[0];

        assert!(voice.active, "Voice should still be active after loop wrap");
        assert!(voice.position >= 2.0, "Position should wrap into loop range");
        assert!(voice.position < 6.0, "Position should wrap into loop range, got {:.3}", voice.position);
        assert!(left.iter().any(|&s| s.abs() > 0.001), "Loop should produce audio");
    }

    #[test]
    fn loop_forward_reads_from_loop_region() {
        let data: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, -0.8, -0.9];
        let mut voice = Voice::default();
        voice.active = true;
        voice.sample = Some(Arc::new(data.clone()));
        voice.sample_rate = 48000.0;
        voice.loop_type = LoopType::Forward;
        voice.loop_start = 2;
        voice.loop_end = 6;
        voice.position = 0.0;
        voice.position_end = data.len() as f64;
        voice.sample_delta = 1.0;
        voice.base_volume = 1.0;
        voice.envelope_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 1.0;
        voice.smoothed_volume = 1.0;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;
        voice.smoothed_panning = 0.5;

        let mut left = vec![0.0f32; 8];
        let mut right = vec![0.0f32; 8];
        let mut voices = [voice];
        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Nearest, &[], 48000.0);
        let voice = &voices[0];

        assert!(voice.active);
        assert!(voice.position >= 2.0 && voice.position < 6.0,
            "Position should be wrapped into loop range [2,6), got {:.3}", voice.position);

        let left_nonzero_count = left.iter().filter(|&&s| s.abs() > 0.001).count();
        assert!(left_nonzero_count >= 6,
            "Should produce audio for at least 6 out of 8 output samples (got {})", left_nonzero_count);
    }

    #[test]
    fn non_looping_voice_stops_at_end() {
        let data: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4];
        let mut voice = Voice::default();
        voice.active = true;
        voice.sample = Some(Arc::new(data.clone()));
        voice.sample_rate = 48000.0;
        voice.loop_type = LoopType::None;
        voice.loop_start = 0;
        voice.loop_end = 0;
        voice.position = 3.0;
        voice.position_end = data.len() as f64;
        voice.sample_delta = 1.0;
        voice.base_volume = 1.0;
        voice.envelope_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 1.0;
        voice.smoothed_volume = 1.0;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;
        voice.smoothed_panning = 0.5;

        let mut left = vec![0.0f32; 10];
        let mut right = vec![0.0f32; 10];
        let mut voices = [voice];
        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Nearest, &[], 48000.0);
        let voice = &voices[0];

        assert!(!voice.active, "Non-looping voice should deactivate after reaching end");
    }

    #[test]
    fn loop_forward_starting_beyond_loop_end_wraps_before_resample() {
        let data: Vec<f32> = vec![0.0, 0.1, 0.5, 1.0, 0.5, 0.0, -2.0, -3.0];
        let mut voice = Voice::default();
        voice.active = true;
        voice.sample = Some(Arc::new(data.clone()));
        voice.sample_rate = 48000.0;
        voice.loop_type = LoopType::Forward;
        voice.loop_start = 2;
        voice.loop_end = 6;
        voice.position = 7.0;
        voice.position_end = data.len() as f64;
        voice.sample_delta = 1.0;
        voice.base_volume = 1.0;
        voice.envelope_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 1.0;
        voice.smoothed_volume = 1.0;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;
        voice.smoothed_panning = 0.5;

        let mut left = vec![0.0f32; 4];
        let mut right = vec![0.0f32; 4];
        let mut voices = [voice];
        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Linear, &[], 48000.0);
        let voice = &voices[0];

        assert!(voice.active, "Voice should stay active");
        assert!(voice.position >= 2.0 && voice.position < 6.0,
            "Position should be wrapped into loop range on first iteration, got {:.3}", voice.position);

        let first_sample = left[0] * 2.0_f32.sqrt();
        assert!(first_sample.abs() > 0.001, "First sample should come from loop region, got {:.6}", first_sample);
        assert_ne!(first_sample, -2.0_f32 / 2.0_f32.sqrt(),
            "First sample should NOT come from outside-loop region (index 6 or 7)");
    }

    #[test]
    fn two_voices_same_sample_soft_limited() {
        let data: Vec<f32> = vec![0.5, 0.6, 0.7, 0.8, -0.8, -0.7, -0.6, -0.5];
        let make_voice = |pos: f64| -> Voice {
            let mut v = Voice::default();
            v.active = true;
            v.sample = Some(Arc::new(data.clone()));
            v.sample_rate = 48000.0;
            v.loop_type = LoopType::None;
            v.position = pos;
            v.position_end = data.len() as f64;
            v.sample_delta = 1.0;
            v.base_volume = 1.0;
            v.envelope_volume = 1.0;
            v.fade_out_volume = 1.0;
            v.final_volume = 1.0;
            v.smoothed_volume = 1.0;
            v.base_panning = 0.5;
            v.envelope_panning = 0.0;
            v.final_panning = 0.5;
            v.smoothed_panning = 0.5;
            v
        };

        let v1 = make_voice(0.0);
        let v2 = make_voice(1.0);
        let mut voices = [v1, v2];
        let mut left = vec![0.0f32; 6];
        let mut right = vec![0.0f32; 6];

        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Nearest, &[], 48000.0);
        brick_wall_limit(&mut left, &mut right);

        for &s in &left {
            assert!(s.abs() <= 1.0 + 0.001, "Soft limiter should keep samples within ±1.0, got {s}");
        }
        for &s in &right {
            assert!(s.abs() <= 1.0 + 0.001, "Soft limiter should keep samples within ±1.0, got {s}");
        }
    }

    #[test]
    fn soft_knee_limit_test() {
        let mut left = vec![0.5f32; 120];
        let mut right = vec![0.5f32; 120];
        for i in 10..110 {
            left[i] = 1.6;
            right[i] = 1.4;
        }

        soft_knee_limit(&mut left, &mut right);

        // Buffer-wide: peak=1.6, target gain is uniform across all samples
        // compress(1.6) = 0.8 + 0.2*(1-exp(-0.8/0.2)) ≈ 0.9963
        // gain = 0.9963 / 1.6 ≈ 0.6227
        let expected_gain = {
            let excess: f32 = 1.6 - 0.8;
            let compressed = 0.8 + 0.2 * (1.0 - (-excess / 0.2).exp());
            compressed / 1.6
        };

        // All samples should be multiplied by the same gain
        assert!((left[0] - 0.5 * expected_gain).abs() < 0.001,
            "below-threshold samples also get uniform gain");
        assert!((left[50] - 1.6 * expected_gain).abs() < 0.001,
            "above-threshold samples get same uniform gain");

        // Uniform gain means ratio between channels is preserved
        let ratio = left[50] / right[50];
        let ratio_expected = 1.6 / 1.4;
        assert!((ratio - ratio_expected).abs() < 0.001,
            "uniform gain preserves left/right ratio");
    }

    /// Helper: a voice reading a constant-1.0 sample (DC), so the only thing
    /// that varies in the output is the applied gain — making ramp values
    /// trivial to assert. Center-panned, `final_volume` configurable.
    fn make_dc_voice(final_volume: f32) -> Voice {
        let data = vec![1.0_f32; 4096];
        let mut voice = Voice::default();
        voice.active = true;
        voice.sample = Some(Arc::new(data));
        voice.sample_rate = 48000.0;
        voice.loop_type = LoopType::None;
        voice.position = 0.0;
        voice.sample_delta = 1.0; // advance one sample per output sample
        voice.base_volume = final_volume;
        voice.envelope_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = final_volume;
        voice.final_panning = 0.5; // center
        voice.ramp_enabled = false;
        voice
    }

    #[test]
    fn mix_voice_ramp_onset_fades_in_from_silence() {
        // A freshly triggered voice starts smoothed_volume at 0 and ramps up.
        // With ramp_enabled, sample[0] should be near-silent and the final
        // sample of the onset ramp should reach the target.
        let mut voice = make_dc_voice(1.0);
        voice.ramp_enabled = true;
        voice.smoothed_volume = 0.0; // simulate post-trigger onset state
        voice.smoothed_panning = 0.5;

        let len = ONSET_RAMP_SAMPLES; // onset ramp completes within this
        let mut left = vec![0.0_f32; len];
        let mut right = vec![0.0_f32; len];
        mix_voices(
            std::slice::from_mut(&mut voice),
            &mut left, &mut right,
            1.0, // master_volume
            InterpolationType::Nearest,
            &[],
            48000.0,
        );

        // First sample is near-silence (ramp just started).
        assert!(left[0].abs() < 0.1, "onset sample[0] should be near 0, got {}", left[0]);
        // Last sample of the onset ramp reaches the target gain (1.0 * center pan = 0.5).
        assert!((left[len - 1] - 0.5).abs() < 0.05,
            "onset ramp end should reach target gain 0.5, got {}", left[len - 1]);
        // Monotonic increase across the ramp.
        assert!(left[len - 1] > left[0], "onset ramp must rise");
    }

    #[test]
    fn mix_voice_ramp_inter_tick_smooths_volume_change() {
        // smoothed_volume=0.5, final_volume=1.0 → ramp across the chunk from
        // 0.5 toward 1.0. Mid-sample should be ~midpoint, end should hit target.
        let mut voice = make_dc_voice(1.0);
        voice.ramp_enabled = true;
        voice.smoothed_volume = 0.5;
        voice.smoothed_panning = 0.5;

        let len = 100;
        let mut left = vec![0.0_f32; len];
        let mut right = vec![0.0_f32; len];
        mix_voices(
            std::slice::from_mut(&mut voice),
            &mut left, &mut right,
            1.0,
            InterpolationType::Nearest,
            &[],
            48000.0,
        );

        // gain at sample i (center pan) = cur_vol * 0.5 after stepping.
        // Start cur_vol 0.5, step = (1.0-0.5)/100 = 0.005. At sample 50,
        // cur_vol ≈ 0.5 + 51*0.005 ≈ 0.755 → output ≈ 0.755*0.5 ≈ 0.38.
        let mid_gain = left[50];
        assert!((mid_gain - 0.375).abs() < 0.02,
            "mid-ramp gain should be ~0.375 (0.75 vol * 0.5 center-pan), got {}", mid_gain);
        // End reaches target (center-panned: 1.0 * 0.5 = 0.5).
        assert!((left[len - 1] - 0.5).abs() < 0.01,
            "end of inter-tick ramp should reach target 0.5, got {}", left[len - 1]);
    }

    #[test]
    fn mix_voice_ramp_disabled_uses_flat_gain() {
        // ramp_enabled=false → bit-exact flat path. All samples equal the
        // target gain regardless of smoothed_volume's starting value.
        let mut voice = make_dc_voice(0.8);
        voice.ramp_enabled = false;
        voice.smoothed_volume = 0.0; // would ramp if enabled; ignored when disabled
        voice.smoothed_panning = 0.5;

        let len = 50;
        let mut left = vec![0.0_f32; len];
        let mut right = vec![0.0_f32; len];
        mix_voices(
            std::slice::from_mut(&mut voice),
            &mut left, &mut right,
            1.0,
            InterpolationType::Nearest,
            &[],
            48000.0,
        );

        let expected = 0.8 * 0.5; // vol * center-pan
        for i in 0..len {
            assert!((left[i] - expected).abs() < 0.001,
                "flat-path sample[{}] should be {}, got {}", i, expected, left[i]);
        }
    }

    #[test]
    fn mix_voice_ramp_clamps_no_overshoot() {
        // smoothed_volume already at 0.9, target 1.0, long chunk. The ramp
        // must land exactly on 1.0 and never overshoot.
        let mut voice = make_dc_voice(1.0);
        voice.ramp_enabled = true;
        voice.smoothed_volume = 0.9;
        voice.smoothed_panning = 0.5;

        let len = 200;
        let mut left = vec![0.0_f32; len];
        let mut right = vec![0.0_f32; len];
        mix_voices(
            std::slice::from_mut(&mut voice),
            &mut left, &mut right,
            1.0,
            InterpolationType::Nearest,
            &[],
            48000.0,
        );

        // Center-panned target gain = 1.0 * 0.5 = 0.5. No sample exceeds it.
        for i in 0..len {
            assert!(left[i] <= 0.5 + 0.001,
                "sample[{}] overshoots target: {}", i, left[i]);
        }
        assert!((left[len - 1] - 0.5).abs() < 0.01,
            "ramp end should be exactly target, got {}", left[len - 1]);
    }

    #[test]
    fn mix_voice_ramp_descending_volume() {
        // Target LOWER than current (volume slide down): ramp must decrease,
        // not be blocked by an ascending-only clamp.
        let mut voice = make_dc_voice(0.4);
        voice.ramp_enabled = true;
        voice.smoothed_volume = 0.8;
        voice.smoothed_panning = 0.5;

        let len = 80;
        let mut left = vec![0.0_f32; len];
        let mut right = vec![0.0_f32; len];
        mix_voices(
            std::slice::from_mut(&mut voice),
            &mut left, &mut right,
            1.0,
            InterpolationType::Nearest,
            &[],
            48000.0,
        );

        // Start ~0.8*0.5=0.4, end ~0.4*0.5=0.2 (descending).
        assert!(left[0] > left[len - 1], "descending ramp must decrease");
        assert!((left[len - 1] - 0.2).abs() < 0.01,
            "descending ramp end should reach 0.2, got {}", left[len - 1]);
    }
}
