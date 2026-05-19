use std::sync::Arc;

use crate::errors::{FormatError, FormatResult};
use crate::formats::FormatHandler;
use crate::sequencer::{
    effect::{FormatEffect, StmEffect},
    Effect, Instrument, LoopType, Module, ModuleFormat, Note, Pattern, Sample,
};

const STM_MAGIC: &[u8; 8] = b"!Scream!";
const STM_HEADER_SIZE: usize = 32;
const STM_INSTRUMENT_SIZE: usize = 32;
const STM_PATTERN_ROWS: usize = 64;
const STM_NUM_CHANNELS: usize = 4;

pub struct StmHandler;

impl FormatHandler for StmHandler {
    fn format_id(&self) -> &'static str {
        "STM"
    }

    fn file_extension(&self) -> &'static str {
        "stm"
    }

    fn detect(&self, data: &[u8]) -> bool {
        if data.len() < 29 {
            return false;
        }
        &data[20..28] == STM_MAGIC
    }

    fn load(&self, data: &[u8]) -> FormatResult<Module> {
        if data.len() < STM_HEADER_SIZE {
            return Err(FormatError::TruncatedFile {
                expected_size: STM_HEADER_SIZE,
                actual_size: data.len(),
            });
        }

        if &data[20..28] != STM_MAGIC {
            return Err(FormatError::InvalidHeader {
                expected: "!Scream!".to_string(),
                found: [data[20], data[21], data[22], data[23]],
            });
        }

        let name = read_string(&data[0..], 0, 20)?;
        let eof_char = data[28];
        let _type = data[29];
        let major_ver = data[30];
        let minor_ver = data[31];

        if eof_char != 0x1A && eof_char != 0x02 {
            return Err(FormatError::InvalidHeader {
                expected: "0x1A or 0x02".to_string(),
                found: [eof_char, 0, 0, 0],
            });
        }

        let mut offset = STM_HEADER_SIZE;

        let mut samples = vec![Sample::default()];
        let mut instruments = vec![Instrument::default()];

        for inst_idx in 0..31 {
            if offset + STM_INSTRUMENT_SIZE > data.len() {
                samples.push(Sample::default());
                instruments.push(Instrument::default());
                offset += STM_INSTRUMENT_SIZE;
                continue;
            }

            let inst_name = read_string(&data[offset..], 0, 13)?;
            let _disk = data[offset + 13];
            let _reserved1 = u16_at(&data, offset + 14);
            let _reserved2 = u16_at(&data, offset + 16);

            let sample_length = u16_at(&data, offset + 18) as usize;
            let loop_begin = u16_at(&data, offset + 20) as usize;
            let loop_end = u16_at(&data, offset + 22) as usize;
            let volume = data[offset + 24].min(64);
            let _reserved3 = data[offset + 25];
            let c4_speed = u16_at(&data, offset + 26) as u32;

            offset += 31;

            let sample_data_offset = offset;
            offset += sample_length;

            if c4_speed == 0 {
            }

            let has_loop = loop_end > loop_begin && loop_end <= sample_length;
            let loop_type = if has_loop { LoopType::Forward } else { LoopType::None };

            let sample_data = if sample_length > 0 && sample_data_offset + sample_length <= data.len() {
                data[sample_data_offset..sample_data_offset + sample_length]
                    .iter()
                    .map(|&b| (b as i8 as f32) / 128.0)
                    .collect()
            } else {
                Vec::new()
            };

            let sample = Sample {
                name: inst_name.clone(),
                data: Arc::new(sample_data),
                loop_start: if has_loop { loop_begin } else { 0 },
                loop_end: if has_loop { loop_end } else { 0 },
                loop_type,
                default_volume: volume,
                sample_rate: if c4_speed > 0 { c4_speed } else { 8363 },
                ..Default::default()
            };
            samples.push(sample);

            let inst = Instrument {
                name: inst_name,
                sample_map: [(inst_idx + 1) as u8; 120],
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

        let song_len = data[offset] as usize;
        offset += 1;

        let _unknown1 = data[offset];
        offset += 1;

        let num_patterns = data[offset] as usize;
        offset += 1;

        let _unknown2 = data[offset];
        offset += 1;

        let mut order_list = Vec::new();
        for i in 0..song_len.min(128) {
            if offset < data.len() {
                let o = data[offset];
                if o != 0xFF && order_list.len() <= i {
                    order_list.push(o);
                }
                offset += 1;
            }
        }

        if order_list.is_empty() {
            for i in 0..num_patterns.min(128) {
                order_list.push(i as u8);
            }
        }

        let mut patterns = Vec::new();
        for pattern_idx in 0..num_patterns.min(128) {
            let mut pattern = Pattern::new(STM_PATTERN_ROWS);

            let pattern_offset = offset + pattern_idx * STM_NUM_CHANNELS * STM_PATTERN_ROWS * 4;

            for row in 0..STM_PATTERN_ROWS {
                for ch in 0..STM_NUM_CHANNELS {
                    let event_offset = pattern_offset + (row * STM_NUM_CHANNELS + ch) * 4;
                    if event_offset + 4 > data.len() {
                        continue;
                    }

                    let note_byte = data[event_offset];
                    let vol_ins_byte = data[event_offset + 1];
                    let volume_byte = data[event_offset + 2];
                    let effect_byte = data[event_offset + 3];

                    let note_val = note_byte & 0x3F;
                    let inst_val = (note_byte >> 6) & 0x03;

                    if note_val > 0 && note_val < 120 {
                        let octave = if note_byte & 0x40 != 0 { 1 } else { 0 };
                        pattern.data[row][ch].note = Note::On((note_val & 0x1F) + octave * 12 + 1);
                    }

                    if inst_val > 0 {
                        let ins = (inst_val << 4) | (vol_ins_byte & 0x0F);
                        if ins > 0 && ins <= 31 {
                            pattern.data[row][ch].instrument = Some(ins);
                        }
                    }

                    let vol_val = volume_byte & 0x40;
                    if vol_val != 0 {
                        let vol = (volume_byte & 0x3F).min(64);
                        pattern.data[row][ch].volume = Some(vol);
                    }

                    let effect_code = effect_byte >> 4;
                    let effect_param = effect_byte & 0x0F;

                    pattern.data[row][ch].effect = convert_stm_effect(effect_code, effect_param);

                    if vol_ins_byte & 0x40 != 0 {
                        let vol = (vol_ins_byte & 0x3F).min(64);
                        pattern.data[row][ch].volume = Some(vol);
                    }
                }
            }

            patterns.push(pattern);
        }

        let default_tempo = 125u8;
        let default_speed = 6u8;

        let mut channel_panning = Vec::with_capacity(STM_NUM_CHANNELS);
        for i in 0..STM_NUM_CHANNELS {
            channel_panning.push(if i < 2 { 0 } else { 64 });
        }

        Ok(Module {
            name,
            message: None,
            format: ModuleFormat::Stm,
            _version: ((major_ver as u16) << 8) | (minor_ver as u16),
            tracker_name: format!("Scream Tracker {}x", major_ver),
            order_list,
            patterns,
            instruments,
            samples,
            initial_bpm: default_tempo as u16,
            initial_speed: default_speed,
            initial_global_volume: 128,
            initial_mixing_volume: 128,
            channel_panning,
            channel_volume: vec![64u8; STM_NUM_CHANNELS],
            flags: crate::sequencer::ModuleFlags::default(),
            send_bus_config: Default::default(),
            send_return_levels: Default::default(),
            automation_tracks: Vec::new(),
            next_automation_id: 0,
        })
    }
}

fn convert_stm_effect(effect_code: u8, param: u8) -> Effect {
    match effect_code {
        10 => Effect::SetTempo { bpm: param },
        11 => Effect::PositionJump { order: param },
        12 => Effect::PatternBreak { row: param },
        13 => Effect::VolumeSlide {
            up: param >> 4,
            down: param & 0x0F,
        },
        14 => Effect::PortamentoDown { speed: param },
        15 => Effect::PortamentoUp { speed: param },
        16 => Effect::TonePortamento { speed: param },
        17 => Effect::Vibrato {
            speed: param >> 4,
            depth: param & 0x0F,
        },
        18 => Effect::Tremor { ontime: param >> 4, offtime: param & 0x0F },
        19 => Effect::Arpeggio {
            note1: param >> 4,
            note2: param & 0x0F,
        },
        _ => Effect::FormatSpecific(FormatEffect::Stm(StmEffect::Raw {
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

fn u16_at(data: &[u8], offset: usize) -> u16 {
    if offset + 2 > data.len() {
        return 0;
    }
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_format_id() {
        let handler = StmHandler;
        assert_eq!(handler.format_id(), "STM");
    }

    #[test]
    fn handler_file_extension() {
        let handler = StmHandler;
        assert_eq!(handler.file_extension(), "stm");
    }

    #[test]
    fn detect_valid_stm() {
        let handler = StmHandler;
        let mut data = vec![0u8; 100];
        data[20..28].copy_from_slice(b"!Scream!");
        data[28] = 0x1A;
        assert!(handler.detect(&data));
    }

    #[test]
    fn detect_invalid() {
        let handler = StmHandler;
        let data = vec![0u8; 100];
        assert!(!handler.detect(&data));
    }
}