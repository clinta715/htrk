use std::sync::Arc;

use crate::errors::{FormatError, FormatResult};
use crate::formats::FormatHandler;
use crate::sequencer::{
    effect::{FormatEffect, UltEffect},
    Effect, Instrument, LoopType, Module, ModuleFormat, Note, Pattern, Sample,
};

const ULT_MAGIC_LEN: usize = 15;
const ULT_TITLE_LEN: usize = 32;
const ULT_MAX_CHANNELS: usize = 32;
const ULT_PATTERN_ROWS: usize = 64;

pub struct UltHandler;

impl FormatHandler for UltHandler {
    fn format_id(&self) -> &'static str {
        "ULT"
    }

    fn file_extension(&self) -> &'static str {
        "ult"
    }

    fn detect(&self, data: &[u8]) -> bool {
        if data.len() < ULT_MAGIC_LEN {
            return false;
        }
        &data[0..12] == b"MAS_UTrack_V"
    }

    fn load(&self, data: &[u8]) -> FormatResult<Module> {
        if data.len() < ULT_MAGIC_LEN + ULT_TITLE_LEN + 1 {
            return Err(FormatError::TruncatedFile {
                expected_size: ULT_MAGIC_LEN + ULT_TITLE_LEN + 1,
                actual_size: data.len(),
            });
        }

        let magic_str = &data[0..12];
        let version_char = data[12];
        if magic_str != b"MAS_UTrack_V" {
            return Err(FormatError::InvalidHeader {
                expected: "MAS_UTrack_V".to_string(),
                found: [b'M', b'A', b'S', b'_'],
            });
        }

        let version = match version_char {
            b'1' => 1,
            b'2' => 2,
            b'3' => 3,
            b'4' => 4,
            _ => 1,
        };

        let name = read_string(&data[15..], 0, ULT_TITLE_LEN)?;
        let msg_size = data[47];

        let mut offset = 48;
        let message = if msg_size > 0 {
            let msg_len = (msg_size as usize) * 32;
            if offset + msg_len > data.len() {
                None
            } else {
                let msg = read_string(&data[offset..], 0, msg_len)?;
                offset += msg_len;
                Some(msg)
            }
        } else {
            None
        };

        if offset >= data.len() {
            return Err(FormatError::TruncatedFile {
                expected_size: offset + 1,
                actual_size: data.len(),
            });
        }

        let num_samples = data[offset] as usize;
        offset += 1;

        let mut samples = vec![Sample::default()];
        let mut instruments = vec![Instrument::default()];

        for _ in 0..num_samples {
            if offset + 64 > data.len() {
                samples.push(Sample::default());
                instruments.push(Instrument::default());
                offset += 64;
                continue;
            }

            let inst_name = read_string(&data[offset..], 0, 32)?;
            let _dos_name = read_string(&data[offset..], 32, 12)?;
            let loop_start = u32_at(&data, offset + 44) as usize;
            let loop_end = u32_at(&data, offset + 48) as usize;
            let sample_size_start = u32_at(&data, offset + 52) as usize;
            let sample_size_end = u32_at(&data, offset + 56) as usize;
            let volume = data[offset + 60].min(64);
            let _bidi_flags = data[offset + 61];

            let c4_speed = if version >= 4 && offset + 64 <= data.len() {
                u16::from_le_bytes([data[offset + 62], data[offset + 63]]) as u32
            } else {
                8363
            };

            offset += 64;

            let sample_length = if sample_size_end > sample_size_start {
                sample_size_end - sample_size_start
            } else {
                0
            };

            let sample_data_offset = sample_size_start;
            let sample_data = if sample_length > 0 && sample_data_offset + sample_length <= data.len() {
                data[sample_data_offset..sample_data_offset + sample_length]
                    .iter()
                    .map(|&b| (b as i8 as f32) / 128.0)
                    .collect()
            } else {
                Vec::new()
            };

            let has_loop = loop_end > loop_start && loop_end <= sample_length;
            let loop_type = if has_loop { LoopType::Forward } else { LoopType::None };

            let sample = Sample {
                name: inst_name.clone(),
                data: Arc::new(sample_data),
                loop_start: if has_loop { loop_start } else { 0 },
                loop_end: if has_loop { loop_end } else { 0 },
                loop_type,
                default_volume: volume,
                sample_rate: c4_speed,
                ..Default::default()
            };
            samples.push(sample);

            let inst = Instrument {
                name: inst_name,
                sample_map: [(samples.len() - 1) as u8; 120],
                global_volume: 128,
                ..Default::default()
            };
            instruments.push(inst);
        }

        if offset >= data.len() {
            return Err(FormatError::TruncatedFile {
                expected_size: offset + 256,
                actual_size: data.len(),
            });
        }

        let orders_end = offset + 256;
        let orders_data = &data[offset..orders_end.min(data.len())];
        let order_list: Vec<u8> = orders_data.iter().take_while(|&&o| o != 0xFF).copied().collect();
        offset = orders_end;

        if offset >= data.len() {
            return Err(FormatError::TruncatedFile {
                expected_size: offset + 2,
                actual_size: data.len(),
            });
        }

        let num_channels_minus_one = data[offset] as usize;
        let num_channels = (num_channels_minus_one + 1).min(ULT_MAX_CHANNELS);
        offset += 1;

        let num_patterns_minus_one = data[offset] as usize;
        let num_patterns = (num_patterns_minus_one + 1).min(256);
        offset += 1;

        let mut channel_panning = vec![32u8; num_channels];
        if version >= 3 && offset + num_channels <= data.len() {
            for i in 0..num_channels {
                channel_panning[i] = data[offset + i] * 16 / 15;
                channel_panning[i] = channel_panning[i].min(64);
            }
            offset += num_channels;
        }

        let mut patterns = Vec::new();
        for pattern_idx in 0..num_patterns {
            let mut pattern = Pattern::new(ULT_PATTERN_ROWS);
            patterns.push(pattern);
        }

        for ch in 0..num_channels {
            for pattern_idx in 0..num_patterns {
                if offset >= data.len() {
                    break;
                }

                let mut row = 0;
                while row < ULT_PATTERN_ROWS && offset < data.len() {
                    let cmd = data[offset];
                    offset += 1;

                    if cmd == 0xFC {
                        if offset >= data.len() {
                            break;
                        }
                        let repeat_count = data[offset] as usize;
                        offset += 1;
                        if offset >= data.len() {
                            break;
                        }
                        let repeat_byte = data[offset];
                        offset += 1;

                        let count = repeat_count.min(ULT_PATTERN_ROWS - row);
                        for _ in 0..count {
                            if pattern_idx < patterns.len() && ch < ULT_MAX_CHANNELS {
                                patterns[pattern_idx].data[row][ch].note = Note::None;
                                patterns[pattern_idx].data[row][ch].instrument = None;
                                patterns[pattern_idx].data[row][ch].volume = Some(repeat_byte.min(64));
                                patterns[pattern_idx].data[row][ch].effect = Effect::None;
                            }
                            row += 1;
                        }
                    } else {
                        let note_val = cmd & 0x3F;
                        let inst_val = (cmd >> 6) & 0x03;

                        if offset >= data.len() {
                            break;
                        }

                        if pattern_idx < patterns.len() && ch < ULT_MAX_CHANNELS {
                            if note_val > 0 && note_val < 120 {
                                patterns[pattern_idx].data[row][ch].note = Note::On(note_val);
                            }

                            if inst_val > 0 {
                                patterns[pattern_idx].data[row][ch].instrument = Some((inst_val * 16) as u8);
                            }
                        }

                        let second_byte = data[offset];
                        offset += 1;

                        let vol_val = second_byte & 0x3F;
                        if vol_val > 0 && vol_val <= 64 && pattern_idx < patterns.len() && ch < ULT_MAX_CHANNELS {
                            patterns[pattern_idx].data[row][ch].volume = Some(vol_val);
                        }

                        if offset >= data.len() {
                            break;
                        }

                        let third_byte = data[offset];
                        offset += 1;

                        let effect_code = (third_byte >> 4) & 0x0F;
                        let effect_param = third_byte & 0x0F;

                        if pattern_idx < patterns.len() && ch < ULT_MAX_CHANNELS {
                            patterns[pattern_idx].data[row][ch].effect =
                                convert_ult_effect(effect_code, effect_param, second_byte >> 6);
                        }

                        row += 1;
                    }
                }
            }
        }

        Ok(Module {
            name,
            message,
            format: ModuleFormat::Ult,
            _version: version as u16,
            tracker_name: format!("Ultra Tracker V00{}", version_char as char),
            order_list,
            patterns,
            instruments,
            samples,
            initial_bpm: 125,
            initial_speed: 6,
            initial_global_volume: 128,
            initial_mixing_volume: 128,
            channel_panning,
            channel_volume: vec![64u8; num_channels],
            flags: crate::sequencer::ModuleFlags::default(),
            send_bus_config: Default::default(),
            send_return_levels: Default::default(),
            send_pre_fader: Default::default(),
            automation_tracks: Vec::new(),
            next_automation_id: 0,
        })
    }
}

fn convert_ult_effect(effect_code: u8, param: u8, high_nybble: u8) -> Effect {
    match effect_code {
        0x03 => Effect::TonePortamento { speed: param },
        0x09 => Effect::SetSampleOffset { offset: param as u16 },
        0x0B => Effect::SetPanning {
            pan: (param << 4) | param,
        },
        0x0F => {
            if high_nybble == 0 {
                Effect::SetSpeed { speed: param }
            } else {
                Effect::FormatSpecific(FormatEffect::Ult(UltEffect::SpeedBPM { value: param }))
            }
        }
        _ => Effect::FormatSpecific(FormatEffect::Ult(UltEffect::Raw {
            effect: effect_code,
            param,
        })),
    }
}

fn read_string(data: &[u8], offset: usize, len: usize) -> FormatResult<String> {
    if offset + len > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: offset + len,
            actual_size: data.len(),
        });
    }
    let s = &data[offset..offset + len];
    Ok(std::str::from_utf8(s)
        .unwrap_or("")
        .trim_end_matches('\0')
        .trim_end()
        .to_string())
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    if offset + 4 > data.len() {
        return 0;
    }
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_format_id() {
        let handler = UltHandler;
        assert_eq!(handler.format_id(), "ULT");
    }

    #[test]
    fn handler_file_extension() {
        let handler = UltHandler;
        assert_eq!(handler.file_extension(), "ult");
    }

    #[test]
    fn detect_valid_ult() {
        let handler = UltHandler;
        let mut data = vec![0u8; 100];
        data[0..12].copy_from_slice(b"MAS_UTrack_V");
        data[12] = b'2';
        assert!(handler.detect(&data));
    }

    #[test]
    fn detect_invalid() {
        let handler = UltHandler;
        let data = vec![0u8; 100];
        assert!(!handler.detect(&data));
    }
}