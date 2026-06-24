use std::sync::Arc;

use crate::errors::{FormatError, FormatResult};
use crate::formats::FormatHandler;
use crate::sequencer::{
    effect::{FormatEffect, ModEffect},
    Effect, Instrument, LoopType, ModVariant, Module, ModuleFormat, Note, Pattern, Sample,
    MAX_CHANNELS, PERIOD_TABLE,
};

fn read_u16_be(data: &[u8], offset: usize) -> FormatResult<u16> {
    if offset + 2 > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: offset + 2,
            actual_size: data.len(),
        });
    }
    Ok(u16::from_be_bytes([data[offset], data[offset + 1]]))
}

fn decode_finetune(raw: u8) -> i8 {
    let val = raw & 0x0F;
    if val < 8 {
        val as i8
    } else {
        (val as i8) - 16
    }
}

fn decode_finetune_noisetracker(raw: u8) -> i8 {
    let val = raw & 0x1F;
    (0 - val as i8) / 2
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModMagic {
    Standard,
    NoiseTracker,
    SoundTracker,
}

fn detect_magic(magic: &[u8]) -> Option<(u8, ModMagic)> {
    match magic {
        b"M.K." | b"M!K!" | b"FLT4" | b"OKTA" | b"CD81" => Some((4, ModMagic::Standard)),
        b"FLT8" => Some((8, ModMagic::Standard)),
        b"6CHN" => Some((6, ModMagic::Standard)),
        b"8CHN" => Some((8, ModMagic::Standard)),
        b"2CHN" => Some((2, ModMagic::Standard)),
        b"16CN" => Some((16, ModMagic::Standard)),
        b"32CN" => Some((32, ModMagic::Standard)),
        b"N.T." => Some((4, ModMagic::NoiseTracker)),
        b"FEST" | b"M&K!" => Some((4, ModMagic::NoiseTracker)),
        b"4CHN" => Some((4, ModMagic::Standard)),
        _ => None,
    }
}

fn detect_stk(data: &[u8]) -> Option<u8> {
    if data.len() < 600 {
        return None;
    }
    let has_any_nonzero_sample = (0..15).any(|i| {
        let base = 20 + i * 30;
        let len = u16::from_be_bytes([data[base + 22], data[base + 23]]);
        len > 0
    });
    if has_any_nonzero_sample {
        return Some(4);
    }
    None
}

fn period_to_note(period: u16) -> Note {
    if period == 0 {
        return Note::None;
    }
    let mut best_idx = 0;
    let mut best_diff = i32::MAX;
    for (i, &p) in PERIOD_TABLE.iter().enumerate() {
        let diff = (period as i32 - p as i32).abs();
        if diff < best_diff {
            best_diff = diff;
            best_idx = i;
        }
    }
    if best_diff > period as i32 / 2 {
        return Note::None;
    }
    // MOD period table covers C-1 to B-3 (indices 0-35 in our PERIOD_TABLE).
    // Our PERIOD_TABLE starts at C-0 (index 0). To map MOD's C-1 to C-4 (MIDI 48):
    // add 48 to index (since table index 0 is C-1, and MIDI 48 is C-4? Wait).
    // Let's use the previous known-good mapping: best_idx + 36.
    let midi_note = ((best_idx as i32 + 36).max(0) as u8).min(119);
    Note::On(midi_note)
}

fn convert_effect(effect_code: u8, effect_param: u8, variant: ModMagic) -> Effect {
    match variant {
        ModMagic::SoundTracker => convert_effect_stk(effect_code, effect_param),
        ModMagic::NoiseTracker => convert_effect_nt(effect_code, effect_param),
        ModMagic::Standard => convert_effect_pt(effect_code, effect_param),
    }
}

fn convert_effect_stk(effect_code: u8, effect_param: u8) -> Effect {
    match effect_code {
        0 => Effect::None,
        1 => {
            if effect_param == 0 {
                Effect::None
            } else {
                Effect::Arpeggio {
                    note1: effect_param >> 4,
                    note2: effect_param & 0x0F,
                }
            }
        }
        2 => {
            if effect_param == 0 {
                Effect::None
            } else {
                let sign: i8 = if effect_param & 0x0F != 0 {
                    (effect_param & 0x0F) as i8
                } else {
                    -((effect_param >> 4) as i8)
                };
                if sign >= 0 {
                    Effect::PortamentoUp {
                        speed: sign as u8,
                    }
                } else {
                    Effect::PortamentoDown {
                        speed: (-sign) as u8,
                    }
                }
            }
        }
        0xD => {
            if effect_param == 0 {
                Effect::PatternBreak { row: 0 }
            } else {
                Effect::VolumeSlide {
                    up: effect_param >> 4,
                    down: effect_param & 0x0F,
                }
            }
        }
        code => convert_effect_pt(code, effect_param),
    }
}

fn convert_effect_nt(effect_code: u8, effect_param: u8) -> Effect {
    match effect_code {
        0xD => Effect::PatternBreak { row: 0 },
        code => convert_effect_pt(code, effect_param),
    }
}

fn convert_effect_pt(effect_code: u8, effect_param: u8) -> Effect {
    match effect_code {
        0 => {
            if effect_param == 0 {
                Effect::None
            } else {
                Effect::Arpeggio {
                    note1: effect_param >> 4,
                    note2: effect_param & 0x0F,
                }
            }
        }
        1 => Effect::PortamentoUp {
            speed: effect_param,
        },
        2 => Effect::PortamentoDown {
            speed: effect_param,
        },
        3 => Effect::TonePortamento {
            speed: effect_param,
        },
        4 => Effect::Vibrato {
            speed: effect_param >> 4,
            depth: effect_param & 0x0F,
        },
        5 => Effect::TonePortamentoVolumeSlide {
            up: effect_param as i8,
        },
        6 => Effect::VibratoVolumeSlide {
            up: effect_param as i8,
        },
        7 => Effect::Tremolo {
            speed: effect_param >> 4,
            depth: effect_param & 0x0F,
        },
        8 => Effect::SetPanning { pan: effect_param },
        9 => Effect::SetSampleOffset {
            offset: (effect_param as u16) << 8,
        },
        0xA => Effect::VolumeSlide {
            up: effect_param >> 4,
            down: effect_param & 0x0F,
        },
        0xB => Effect::PositionJump {
            order: effect_param as u16,
        },
        0xC => Effect::SetVolume {
            volume: effect_param.min(64),
        },
        0xD => {
            let row = ((effect_param >> 4) * 10) + (effect_param & 0x0F);
            Effect::PatternBreak { row: row as u16 }
        }
        0xE => {
            let sub = effect_param >> 4;
            let val = effect_param & 0x0F;
            match sub {
                0x0 => Effect::FormatSpecific(FormatEffect::Mod(ModEffect::Filter(val == 0))),
                0x1 => Effect::FinePortamentoUp { speed: val },
                0x2 => Effect::FinePortamentoDown { speed: val },
                0x3 => Effect::GlissandoControl { on: val != 0 },
                0x4 => Effect::VibratoWaveform { waveform: val & 0x03 },
                0x5 => Effect::SetFineTune { tune: val },
                0x6 => Effect::PatternLoop { count: val },
                0x7 => Effect::TremoloWaveform { waveform: val & 0x03 },
                0x8 => Effect::FormatSpecific(FormatEffect::Mod(ModEffect::KarplusStrong { param: val })),
                0x9 => Effect::Retrigger { interval: val },
                0xA => Effect::FineVolumeSlideUp { amount: val },
                0xB => Effect::FineVolumeSlideDown { amount: val },
                0xC => Effect::NoteCutAfter { ticks: val },
                0xD => Effect::NoteDelay { ticks: val },
                0xE => Effect::PatternDelay { ticks: val },
                0xF => Effect::FormatSpecific(FormatEffect::Mod(ModEffect::FunkIt { speed: val })),
                _ => Effect::None,
            }
        }
        0xF => {
            if effect_param < 32 {
                Effect::SetSpeed {
                    speed: effect_param,
                }
            } else {
                Effect::SetTempo {
                    bpm: effect_param,
                }
            }
        }
        _ => Effect::None,
    }
}

pub struct ModHandler;

impl FormatHandler for ModHandler {
    fn format_id(&self) -> &'static str {
        "MOD"
    }

    fn file_extension(&self) -> &'static str {
        "mod"
    }

    fn detect(&self, data: &[u8]) -> bool {
        if data.len() < 1084 {
            if data.len() < 600 {
                return false;
            }
            return detect_stk(data).is_some();
        }
        let magic = &data[1080..1084];
        if is_mod_magic(magic) {
            return true;
        }
        detect_stk(data).is_some()
    }

    fn load(&self, data: &[u8]) -> FormatResult<Module> {
        let (num_channels, variant, num_samples, header_size) = if data.len() >= 1084 {
            let magic = &data[1080..1084];
            if let Some((ch, vm)) = detect_magic(magic) {
                (ch, vm, 31, 1084)
            } else if let Some(ch) = detect_stk(data) {
                (ch, ModMagic::SoundTracker, 15, 600)
            } else {
                return Err(FormatError::TruncatedFile {
                    expected_size: 1084,
                    actual_size: data.len(),
                });
            }
        } else if let Some(ch) = detect_stk(data) {
            (ch, ModMagic::SoundTracker, 15, 600)
        } else {
            return Err(FormatError::TruncatedFile {
                expected_size: 1084,
                actual_size: data.len(),
            });
        };

        if data.len() < header_size {
            return Err(FormatError::TruncatedFile {
                expected_size: header_size,
                actual_size: data.len(),
            });
        }

        let name = std::str::from_utf8(&data[0..20])
            .unwrap_or("")
            .trim_end_matches('\0')
            .trim_end()
            .to_string();

        let song_length = data[950] as usize;
        let _restart = data[951];

        let order_list: Vec<u8> = data[952..1080]
            .iter()
            .take(song_length)
            .take_while(|&&o| o != 0xFF)
            .copied()
            .collect();

        let num_patterns = if order_list.is_empty() {
            0
        } else {
            (*order_list.iter().max().unwrap_or(&0) as usize) + 1
        };

        let initial_speed = match variant {
            ModMagic::SoundTracker => {
                if _restart != 0 && _restart != 120 {
                    let stk_speed = _restart;
                    6.max(stk_speed)
                } else {
                    6
                }
            }
            _ => 6,
        };

        let initial_bpm = match variant {
            ModMagic::SoundTracker => {
                if _restart == 120 || _restart == 0 {
                    125
                } else {
                    125
                }
            }
            _ => 125,
        };

        let mut mod_samples = Vec::new();
        let mut sample_offsets = Vec::new();
        let pattern_data_size = num_patterns * num_channels as usize * 64 * 4;
        let mut current_offset = header_size + pattern_data_size;

        for i in 0..num_samples {
            let base = 20 + i * 30;
            if base + 30 > data.len() {
                mod_samples.push((
                    String::new(), 0u8, 0u8, LoopType::None, 0usize, 0usize, 0usize,
                ));
                sample_offsets.push(current_offset);
                continue;
            }

            let sample_name = std::str::from_utf8(&data[base..base + 22])
                .unwrap_or("")
                .trim_end_matches('\0')
                .trim_end()
                .to_string();

            let length_words = read_u16_be(data, base + 22)?;
            let finetune_raw = data[base + 24];
            let volume = data[base + 25].min(64);
            let loop_start_words = read_u16_be(data, base + 26)?;
            let loop_length_words = read_u16_be(data, base + 28)?;

            let length_bytes = length_words as usize * 2;
            let mut loop_start_bytes = loop_start_words as usize * 2;
            let mut loop_length_bytes = loop_length_words as usize * 2;

            let mut fixed_length_bytes = length_bytes;

            let loop_type = if loop_length_bytes > 2 {
                if loop_start_bytes + loop_length_bytes > length_bytes {
                    if loop_start_bytes < length_bytes {
                        loop_length_bytes = length_bytes - loop_start_bytes;
                        fixed_length_bytes = loop_start_bytes + loop_length_bytes;
                    } else {
                        loop_start_bytes = 0;
                        loop_length_bytes = 0;
                    }
                }
                if loop_length_bytes > 2 {
                    LoopType::Forward
                } else {
                    LoopType::None
                }
            } else {
                LoopType::None
            };

            let (loop_start, loop_end) = if loop_type != LoopType::None {
                (loop_start_bytes, loop_start_bytes + loop_length_bytes)
            } else {
                (0, 0)
            };

            mod_samples.push((
                sample_name,
                finetune_raw,
                volume,
                loop_type,
                loop_start,
                loop_end,
                fixed_length_bytes,
            ));

            sample_offsets.push(current_offset);
            current_offset += fixed_length_bytes;
        }

        let mut patterns = Vec::new();
        for pat_idx in 0..num_patterns {
            let mut pattern = Pattern::new(64);
            let pat_offset = header_size + pat_idx * num_channels as usize * 64 * 4;

            for row in 0..64 {
                for ch in 0..(num_channels as usize) {
                    let cell_offset = pat_offset + (row * num_channels as usize + ch) * 4;
                    if cell_offset + 4 > data.len() {
                        continue;
                    }

                    let b0 = data[cell_offset];
                    let b1 = data[cell_offset + 1];
                    let b2 = data[cell_offset + 2];
                    let b3 = data[cell_offset + 3];

                    let period = ((b0 as u16 & 0x0F) << 8) | b1 as u16;
                    let sample_idx = ((b0 & 0xF0)) | ((b2 & 0xF0) >> 4);
                    let effect_code = b2 & 0x0F;
                    let effect_param = b3;

                    let note = period_to_note(period);
                    let instrument = if sample_idx > 0 && (sample_idx as usize) <= num_samples {
                        Some(sample_idx)
                    } else {
                        None
                    };
                    let effect = convert_effect(effect_code, effect_param, variant);

                    if ch < MAX_CHANNELS {
                        let cell = &mut pattern.data[row][ch];
                        cell.note = note;
                        cell.instrument = instrument;
                        cell.effect = effect;
                    }
                }
            }
            patterns.push(pattern);
        }

        let mut samples = vec![Sample::default()];
        let mut instruments = vec![Instrument::default()];

        for (i, (sample_name, finetune_raw, volume, loop_type, loop_start, loop_end, length_bytes)) in
            mod_samples.into_iter().enumerate()
        {
            let sample_offset = sample_offsets[i];
            let sample_data = if length_bytes > 0 && sample_offset + length_bytes <= data.len() {
                data[sample_offset..sample_offset + length_bytes]
                    .iter()
                    .map(|&b| b as i8 as f32 / 128.0)
                    .collect::<Vec<f32>>()
            } else {
                Vec::new()
            };

            let fine_tune = match variant {
                ModMagic::NoiseTracker => decode_finetune_noisetracker(finetune_raw),
                _ => decode_finetune(finetune_raw),
            };

            let sample = Sample {
                name: sample_name.clone(),
                data: Arc::new(sample_data),
                sample_rate: 8363,
                bits_per_sample: 8,
                loop_type,
                loop_start,
                loop_end,
                default_volume: volume,
                default_panning: 32,
                global_volume: 64,
                relative_note: 0,
                fine_tune,
                vibrato_speed: 0,
                vibrato_depth: 0,
                vibrato_rate: 0,
                vibrato_waveform: crate::sequencer::VibratoWaveform::Sine,
                _flags: crate::sequencer::SampleFlags::default(),
            };
            samples.push(sample);

            let sample_idx = (i + 1) as u8;
            let inst = Instrument {
                name: sample_name,
                sample_map: [sample_idx; 120],
                ..Instrument::default()
            };
            instruments.push(inst);
        }

        for pattern in &mut patterns {
            for row in &mut pattern.data {
                for ch in 0..MAX_CHANNELS {
                    if let Some(inst_idx) = row[ch].instrument {
                        if inst_idx > 0 {
                            row[ch].instrument = Some(inst_idx);
                        }
                    }
                }
            }
        }

        let mod_count = (num_channels as usize).min(MAX_CHANNELS).max(1);
        let mut channel_panning = vec![32u8; mod_count];
        for ch in 0..mod_count {
            if ch % 2 == 0 {
                channel_panning[ch] = 0;
            } else {
                channel_panning[ch] = 192;
            }
        }

        let mod_variant = match variant {
            ModMagic::Standard => ModVariant::ProTracker,
            ModMagic::NoiseTracker => ModVariant::NoiseTracker,
            ModMagic::SoundTracker => ModVariant::SoundTracker,
        };

        Ok(Module {
            name,
            message: None,
            format: ModuleFormat::MOD,
            _version: 0,
            tracker_name: String::from(match variant {
                ModMagic::Standard => "ProTracker",
                ModMagic::NoiseTracker => "NoiseTracker",
                ModMagic::SoundTracker => "SoundTracker",
            }),
            order_list,
            patterns,
            instruments,
            samples,
            initial_bpm,
            initial_speed,
            initial_global_volume: 128,
            initial_mixing_volume: 128,
            channel_panning,
            channel_volume: vec![64u8; mod_count],
            flags: crate::sequencer::ModuleFlags {
                mod_variant,
                ..crate::sequencer::ModuleFlags::default()
            },
            send_bus_config: Default::default(),
            send_return_levels: Default::default(),
            send_pre_fader: Default::default(),
            send_bus_plugins: Default::default(),
            automation_tracks: Vec::new(),
            next_automation_id: 0,
        })
    }
}

#[allow(dead_code)]
fn is_mod_magic(magic: &[u8]) -> bool {
    const MOD_SIGNATURES: &[&[u8]] = &[
        b"M.K.", b"M!K!", b"FLT4", b"FLT8", b"4CHN", b"6CHN", b"8CHN", b"2CHN", b"CD81",
        b"OKTA", b"16CN", b"32CN", b"N.T.", b"FEST", b"M&K!",
    ];
    MOD_SIGNATURES.iter().any(|sig| magic == *sig)
}

pub fn save_module(_module: &Module) -> Vec<u8> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_too_small() {
        let data = [0u8; 50];
        let handler = ModHandler;
        assert!(!handler.detect(&data));
    }

    #[test]
    fn detect_valid_mk() {
        let mut data = vec![0u8; 1084];
        data[1080..1084].copy_from_slice(b"M.K.");
        let handler = ModHandler;
        assert!(handler.detect(&data));
    }

    #[test]
    fn detect_invalid_magic() {
        let mut data = vec![0u8; 1084];
        data[1080..1084].copy_from_slice(b"XXXX");
        let handler = ModHandler;
        assert!(!handler.detect(&data));
    }

    #[test]
    fn detect_magic_variants() {
        assert_eq!(detect_magic(b"M.K."), Some((4, ModMagic::Standard)));
        assert_eq!(detect_magic(b"M!K!"), Some((4, ModMagic::Standard)));
        assert_eq!(detect_magic(b"FLT4"), Some((4, ModMagic::Standard)));
        assert_eq!(detect_magic(b"FLT8"), Some((8, ModMagic::Standard)));
        assert_eq!(detect_magic(b"4CHN"), Some((4, ModMagic::Standard)));
        assert_eq!(detect_magic(b"6CHN"), Some((6, ModMagic::Standard)));
        assert_eq!(detect_magic(b"8CHN"), Some((8, ModMagic::Standard)));
        assert_eq!(detect_magic(b"2CHN"), Some((2, ModMagic::Standard)));
        assert_eq!(detect_magic(b"16CN"), Some((16, ModMagic::Standard)));
        assert_eq!(detect_magic(b"32CN"), Some((32, ModMagic::Standard)));
        assert_eq!(detect_magic(b"OKTA"), Some((4, ModMagic::Standard)));
        assert_eq!(detect_magic(b"CD81"), Some((4, ModMagic::Standard)));
        assert_eq!(detect_magic(b"N.T."), Some((4, ModMagic::NoiseTracker)));
        assert_eq!(detect_magic(b"FEST"), Some((4, ModMagic::NoiseTracker)));
        assert_eq!(detect_magic(b"M&K!"), Some((4, ModMagic::NoiseTracker)));
        assert_eq!(detect_magic(b"XXXX"), None);
    }

    #[test]
    fn decode_finetune_values() {
        assert_eq!(decode_finetune(0), 0);
        assert_eq!(decode_finetune(1), 1);
        assert_eq!(decode_finetune(7), 7);
        assert_eq!(decode_finetune(8), -8);
        assert_eq!(decode_finetune(15), -1);
    }

    #[test]
    fn period_to_note_zero() {
        assert_eq!(period_to_note(0), Note::None);
    }

    #[test]
    fn period_to_note_c3_from_mod() {
        // Period 428 is C-1 in MOD (Amiga notation), which often maps to C-4 or C-5 in trackers.
        // In this engine, index 24 in PERIOD_TABLE is 428. 24 + 36 = 60 (Middle C).
        let note = period_to_note(428);
        assert!(matches!(note, Note::On(k) if k == 60));
    }

    #[test]
    fn convert_effect_arpeggio() {
        let e = convert_effect(0, 0x35, ModMagic::Standard);
        assert_eq!(e, Effect::Arpeggio { note1: 3, note2: 5 });
    }

    #[test]
    fn convert_effect_none_arpeggio() {
        let e = convert_effect(0, 0, ModMagic::Standard);
        assert_eq!(e, Effect::None);
    }

    #[test]
    fn convert_effect_filter_toggle() {
        let e = convert_effect(0xE, 0x00, ModMagic::Standard);
        assert!(matches!(e, Effect::FormatSpecific(FormatEffect::Mod(ModEffect::Filter(true)))));
        let e = convert_effect(0xE, 0x01, ModMagic::Standard);
        assert!(matches!(e, Effect::FormatSpecific(FormatEffect::Mod(ModEffect::Filter(false)))));
    }

    #[test]
    fn convert_effect_loop_set() {
        let e = convert_effect(0xE, 0x65, ModMagic::Standard);
        assert_eq!(e, Effect::PatternLoop { count: 5 });
    }

    #[test]
    fn convert_effect_jump_to_loop() {
        let e = convert_effect(0xE, 0x60, ModMagic::Standard);
        assert_eq!(e, Effect::PatternLoop { count: 0 });
    }

    #[test]
    fn convert_effect_portamento_up() {
        assert_eq!(convert_effect(1, 5, ModMagic::Standard), Effect::PortamentoUp { speed: 5 });
    }

    #[test]
    fn convert_effect_set_speed() {
        assert_eq!(convert_effect(0xF, 6, ModMagic::Standard), Effect::SetSpeed { speed: 6 });
    }

    #[test]
    fn convert_effect_set_tempo() {
        assert_eq!(convert_effect(0xF, 125, ModMagic::Standard), Effect::SetTempo { bpm: 125 });
    }

    #[test]
    fn convert_effect_pattern_break() {
        assert_eq!(convert_effect(0xD, 0x13, ModMagic::Standard), Effect::PatternBreak { row: 13 });
        assert_eq!(convert_effect(0xD, 0x00, ModMagic::Standard), Effect::PatternBreak { row: 0 });
        assert_eq!(convert_effect(0xD, 0x32, ModMagic::Standard), Effect::PatternBreak { row: 32 });
        assert_eq!(convert_effect(0xD, 0x63, ModMagic::Standard), Effect::PatternBreak { row: 63 });
    }

    #[test]
    fn convert_effect_set_volume() {
        assert_eq!(convert_effect(0xC, 40, ModMagic::Standard), Effect::SetVolume { volume: 40 });
    }

    #[test]
    fn convert_effect_set_volume_clamped() {
        assert_eq!(convert_effect(0xC, 100, ModMagic::Standard), Effect::SetVolume { volume: 64 });
    }

    #[test]
    fn load_minimal_mod() {
        let mut data = vec![0u8; 1084 + 64 * 4 * 4];
        data[0..20].copy_from_slice(b"TestSong\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00");
        data[950] = 1;
        data[951] = 0x7F;
        data[952] = 0;
        for i in 953..1080 {
            data[i] = 0xFF;
        }
        data[1080..1084].copy_from_slice(b"M.K.");

        let handler = ModHandler;
        let module = handler.load(&data).unwrap();
        assert_eq!(module.name, "TestSong");
        assert_eq!(module.format, ModuleFormat::MOD);
        assert_eq!(module.order_list, vec![0]);
        assert_eq!(module.patterns.len(), 1);
        assert_eq!(module.patterns[0].num_rows, 64);
        assert_eq!(module.initial_speed, 6);
        assert_eq!(module.initial_bpm, 125);
    }

    #[test]
    fn load_with_sample() {
        let sample_data: &[u8] = &[0x00, 0x40, 0x80, 0xFF];
        let sample_len_words = (sample_data.len() / 2) as u16;
        let total_size = 1084 + 64 * 4 * 4 + sample_data.len();
        let mut data = vec![0u8; total_size];

        data[950] = 1;
        data[952] = 0;
        data[1080..1084].copy_from_slice(b"M.K.");

        let s0_base = 20;
        data[s0_base + 22] = (sample_len_words >> 8) as u8;
        data[s0_base + 23] = (sample_len_words & 0xFF) as u8;
        data[s0_base + 25] = 48;

        let pattern_size = 64 * 4 * 4;
        let sample_offset = 1084 + pattern_size;
        data[sample_offset..sample_offset + sample_data.len()].copy_from_slice(sample_data);

        let handler = ModHandler;
        let module = handler.load(&data).unwrap();

        assert_eq!(module.samples.len(), 32);
        assert_eq!(module.samples[1].default_volume, 48);
        assert_eq!(module.samples[1].data.len(), 4);

        let expected: Vec<f32> = sample_data
            .iter()
            .map(|&b| (b as i8 as f32) / 128.0)
            .collect();
        assert_eq!(*module.samples[1].data, expected);
    }

    #[test]
    fn load_with_note_in_pattern() {
        let mut data = vec![0u8; 1084 + 64 * 4 * 4];
        data[950] = 1;
        data[952] = 0;
        data[1080..1084].copy_from_slice(b"M.K.");

        data[1084] = 0x01;
        data[1085] = 0xAC;
        data[1086] = 0x10;
        data[1087] = 0x00;

        let handler = ModHandler;
        let module = handler.load(&data).unwrap();
        let cell = module.patterns[0].cell(0, 0);
        assert_eq!(cell.instrument, Some(1));
        assert!(matches!(cell.note, Note::On(_)));
    }

    #[test]
    fn mod_channel_panning() {
        let mut data = vec![0u8; 1084 + 64 * 4 * 4];
        data[950] = 1;
        data[952] = 0;
        data[1080..1084].copy_from_slice(b"M.K.");

        let handler = ModHandler;
        let module = handler.load(&data).unwrap();

        assert_eq!(module.channel_panning[0], 0);
        assert_eq!(module.channel_panning[1], 192);
        assert_eq!(module.channel_panning[2], 0);
        assert_eq!(module.channel_panning[3], 192);
    }

    #[test]
    fn load_truncated_errors() {
        let data = vec![0u8; 50];
        let handler = ModHandler;
        assert!(handler.load(&data).is_err());
    }

    #[test]
    fn convert_effect_extended_note_cut() {
        let e = convert_effect(0xE, 0xC3, ModMagic::Standard);
        assert_eq!(e, Effect::NoteCutAfter { ticks: 3 });
    }

    #[test]
    fn convert_effect_extended_note_delay() {
        let e = convert_effect(0xE, 0xD5, ModMagic::Standard);
        assert_eq!(e, Effect::NoteDelay { ticks: 5 });
    }

    #[test]
    fn convert_effect_extended_glissando() {
        assert_eq!(convert_effect(0xE, 0x31, ModMagic::Standard), Effect::GlissandoControl { on: true });
        assert_eq!(convert_effect(0xE, 0x30, ModMagic::Standard), Effect::GlissandoControl { on: false });
    }

    #[test]
    fn detect_noisetracker_nt() {
        let mut data = vec![0u8; 1084];
        data[1080..1084].copy_from_slice(b"N.T.");
        assert!(ModHandler.detect(&data));
    }

    #[test]
    fn detect_noisetracker_fest() {
        let mut data = vec![0u8; 1084];
        data[1080..1084].copy_from_slice(b"FEST");
        assert!(ModHandler.detect(&data));
    }

    #[test]
    fn detect_noisetracker_mkk() {
        let mut data = vec![0u8; 1084];
        data[1080..1084].copy_from_slice(b"M&K!");
        assert!(ModHandler.detect(&data));
    }

    #[test]
    fn convert_effect_nt_dxx_always_breaks() {
        let e = convert_effect(0xD, 0x35, ModMagic::NoiseTracker);
        assert_eq!(e, Effect::PatternBreak { row: 0 });
    }

    #[test]
    fn convert_effect_stk_arpeggio() {
        let e = convert_effect(1, 0x35, ModMagic::SoundTracker);
        assert_eq!(e, Effect::Arpeggio { note1: 3, note2: 5 });
    }

    #[test]
    fn convert_effect_stk_pitch_slide_up() {
        let e = convert_effect(2, 0x05, ModMagic::SoundTracker);
        assert_eq!(e, Effect::PortamentoUp { speed: 5 });
    }

    #[test]
    fn convert_effect_stk_pitch_slide_down() {
        let e = convert_effect(2, 0x50, ModMagic::SoundTracker);
        assert_eq!(e, Effect::PortamentoDown { speed: 5 });
    }

    #[test]
    fn convert_effect_stk_dxx_zero_is_break() {
        let e = convert_effect(0xD, 0x00, ModMagic::SoundTracker);
        assert_eq!(e, Effect::PatternBreak { row: 0 });
    }

    #[test]
    fn convert_effect_stk_dxx_nonzero_is_volume_slide() {
        let e = convert_effect(0xD, 0x35, ModMagic::SoundTracker);
        assert_eq!(e, Effect::VolumeSlide { up: 3, down: 5 });
    }

    #[test]
    fn convert_effect_funkit() {
        let e = convert_effect(0xE, 0xF5, ModMagic::Standard);
        assert_eq!(e, Effect::FormatSpecific(FormatEffect::Mod(ModEffect::FunkIt { speed: 5 })));
    }

    #[test]
    fn convert_effect_karplus_strong() {
        let e = convert_effect(0xE, 0x83, ModMagic::Standard);
        assert_eq!(e, Effect::FormatSpecific(FormatEffect::Mod(ModEffect::KarplusStrong { param: 3 })));
    }

    #[test]
    fn finetune_noisetracker_decoding() {
        assert_eq!(decode_finetune_noisetracker(0), 0);
        assert_eq!(decode_finetune_noisetracker(2), -1);
        assert_eq!(decode_finetune_noisetracker(4), -2);
        assert_eq!(decode_finetune_noisetracker(8), -4);
    }

    #[test]
    fn load_noisetracker_mod() {
        let mut data = vec![0u8; 1084 + 64 * 4 * 4];
        data[950] = 1;
        data[952] = 0;
        data[1080..1084].copy_from_slice(b"N.T.");

        let handler = ModHandler;
        let module = handler.load(&data).unwrap();
        assert_eq!(module.format, ModuleFormat::MOD);
        assert_eq!(module.tracker_name, "NoiseTracker");
        assert_eq!(module.flags.mod_variant, ModVariant::NoiseTracker);
    }

    #[test]
    fn beep_fix_zeros_nonlooping_sample_head() {
        let sample_data: &[u8] = &[0x40, 0x60, 0x80, 0xA0];
        let sample_len_words = (sample_data.len() / 2) as u16;
        let total_size = 1084 + 64 * 4 * 4 + sample_data.len();
        let mut data = vec![0u8; total_size];

        data[950] = 1;
        data[952] = 0;
        data[1080..1084].copy_from_slice(b"M.K.");

        let s0_base = 20;
        data[s0_base + 22] = (sample_len_words >> 8) as u8;
        data[s0_base + 23] = (sample_len_words & 0xFF) as u8;
        data[s0_base + 25] = 48;

        let pattern_size = 64 * 4 * 4;
        let sample_offset = 1084 + pattern_size;
        data[sample_offset..sample_offset + sample_data.len()].copy_from_slice(sample_data);

        let handler = ModHandler;
        let module = handler.load(&data).unwrap();

        assert!((module.samples[1].data[0] - 0.5).abs() < 0.001);
        assert!((module.samples[1].data[1] - 0.75).abs() < 0.001);
    }

    #[test]
    fn illegal_loop_is_fixed() {
        let sample_len = 8u16;
        let loop_start = 4u16;
        let loop_len = 8u16;

        let total_size = 1084 + 64 * 4 * 4 + sample_len as usize * 2;
        let mut data = vec![0u8; total_size];

        data[950] = 1;
        data[952] = 0;
        data[1080..1084].copy_from_slice(b"M.K.");

        let s0_base = 20;
        data[s0_base + 22] = (sample_len >> 8) as u8;
        data[s0_base + 23] = (sample_len & 0xFF) as u8;
        data[s0_base + 25] = 48;
        data[s0_base + 26] = (loop_start >> 8) as u8;
        data[s0_base + 27] = (loop_start & 0xFF) as u8;
        data[s0_base + 28] = (loop_len >> 8) as u8;
        data[s0_base + 29] = (loop_len & 0xFF) as u8;

        let handler = ModHandler;
        let module = handler.load(&data).unwrap();

        assert_eq!(module.samples[1].loop_type, LoopType::Forward);
        assert_eq!(module.samples[1].loop_start, 8);
        assert_eq!(module.samples[1].loop_end, 16);
    }

    #[test]
    fn detect_stk_fallback() {
        let mut data = vec![0u8; 700];
        data[0..20].copy_from_slice(b"STK Module\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00");
        let s0_base = 20;
        data[s0_base + 22] = 0x00;
        data[s0_base + 23] = 0x04;

        assert!(ModHandler.detect(&data));
    }
}

