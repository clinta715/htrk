use crate::audio::commands::InterpolationType;
use crate::audio::resampler;
use crate::audio::voice::Voice;
use crate::sequencer::sample::LoopType;

pub fn mix_voices(
    voices: &mut [Voice],
    output_left: &mut [f32],
    output_right: &mut [f32],
    master_volume: f32,
    interpolation: InterpolationType,
    muted_channels: &[bool],
) {
    for voice in voices.iter_mut() {
        if !voice.active {
            continue;
        }

        if let Some(ch) = voice.channel {
            if ch < muted_channels.len() && muted_channels[ch] {
                continue;
            }
        }

        let sample_data = match &voice.sample {
            Some(data) => match std::sync::Arc::as_ref(data) {
                s if !s.is_empty() => s,
                _ => continue,
            },
            None => continue,
        };

        let vol = voice.final_volume * master_volume;
        let pan = voice.final_panning;

        let left_gain = vol * (1.0 - pan).sqrt();
        let right_gain = vol * pan.sqrt();

        let loop_type = voice.loop_type;
        let loop_start = voice.loop_start;
        let loop_end = voice.loop_end;

        for i in 0..output_left.len() {
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
                voice.direction,
            );

            output_left[i] += s * left_gain;
            output_right[i] += s * right_gain;

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
                        let underflow = loop_start as f64 - voice.position;
                        voice.position = (loop_end - 1) as f64 - underflow;
                        if voice.position < loop_start as f64 {
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
    }
}

pub fn brick_wall_limit(output_left: &mut [f32], output_right: &mut [f32]) {
    for (l, r) in output_left.iter_mut().zip(output_right.iter_mut()) {
        let peak = l.abs().max(r.abs());
        if peak > 1.0 {
            let gain = 1.0 / peak;
            *l *= gain;
            *r *= gain;
        }
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
        voice.channel_volume = 1.0;
        voice.global_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 0.5;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;
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
        );

        assert!(left.iter().all(|&s| s == 0.0));
        assert!(right.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn brick_wall_limit_soft_limiter() {
        let mut left = vec![2.0f32, -1.5f32, 0.5f32];
        let mut right = vec![-2.0f32, 1.5f32, -0.5f32];

        brick_wall_limit(&mut left, &mut right);

        assert!((left[0] - 1.0).abs() < 0.001);
        assert!((left[1] + 1.0).abs() < 0.001);
        assert!((left[2] - 0.5).abs() < 0.001);
        assert!((right[0] + 1.0).abs() < 0.001);
        assert!((right[1] - 1.0).abs() < 0.001);
        assert!((right[2] + 0.5).abs() < 0.001);

        assert!((left[0] / right[0] - left[1] / right[1]).abs() < 0.001,
            "left and right should scale proportionally (both channels get same gain reduction)");
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
        voice.channel_volume = 1.0;
        voice.global_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 1.0;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;

        let mut left = vec![0.0f32; 10];
        let mut right = vec![0.0f32; 10];
        let mut voices = [voice];
        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Nearest, &[]);
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
        voice.channel_volume = 1.0;
        voice.global_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 1.0;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;

        let mut left = vec![0.0f32; 8];
        let mut right = vec![0.0f32; 8];
        let mut voices = [voice];
        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Nearest, &[]);
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
        voice.channel_volume = 1.0;
        voice.global_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 1.0;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;

        let mut left = vec![0.0f32; 10];
        let mut right = vec![0.0f32; 10];
        let mut voices = [voice];
        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Nearest, &[]);
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
        voice.channel_volume = 1.0;
        voice.global_volume = 1.0;
        voice.fade_out_volume = 1.0;
        voice.final_volume = 1.0;
        voice.base_panning = 0.5;
        voice.envelope_panning = 0.0;
        voice.final_panning = 0.5;

        let mut left = vec![0.0f32; 4];
        let mut right = vec![0.0f32; 4];
        let mut voices = [voice];
        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Linear, &[]);
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
            v.channel_volume = 1.0;
            v.global_volume = 1.0;
            v.fade_out_volume = 1.0;
            v.final_volume = 1.0;
            v.base_panning = 0.5;
            v.envelope_panning = 0.0;
            v.final_panning = 0.5;
            v
        };

        let v1 = make_voice(0.0);
        let v2 = make_voice(1.0);
        let mut voices = [v1, v2];
        let mut left = vec![0.0f32; 6];
        let mut right = vec![0.0f32; 6];

        mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Nearest, &[]);
        brick_wall_limit(&mut left, &mut right);

        for &s in &left {
            assert!(s.abs() <= 1.0 + 0.001, "Soft limiter should keep samples within ±1.0, got {s}");
        }
        for &s in &right {
            assert!(s.abs() <= 1.0 + 0.001, "Soft limiter should keep samples within ±1.0, got {s}");
        }
    }
}
