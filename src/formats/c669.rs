use std::sync::Arc;

use crate::errors::{FormatError, FormatResult};
use crate::formats::FormatHandler;
use crate::sequencer::{
    effect::{FormatEffect, C669Effect},
    Effect, Instrument, LoopType, Module, ModuleFormat, Note, Pattern, Sample,
};

const C669_HEADER_SIZE: usize = 497;
const C669_INSTRUMENT_SIZE: usize = 25;
const C669_PATTERN_ROWS: usize = 64;
const C669_NUM_CHANNELS: usize = 8;

pub struct C669Handler;

impl FormatHandler for C669Handler {
    fn format_id(&self) -> &'static str {
        "669"
    }

    fn file_extension(&self) -> &'static str {
        "669"
    }

    fn detect(&self, data: &[u8]) -> bool {
        if data.len() < 2 {
            return false;
        }
        let magic = [data[0], data[1]];
        magic == [b'i', b'f'] || magic == [b'J', b'N']
    }

    fn load(&self, data: &[u8]) -> FormatResult<Module> {
        if data.len() < C669_HEADER_SIZE {
            return Err(FormatError::TruncatedFile {
                expected_size: C669_HEADER_SIZE,
                actual_size: data.len(),
            });
        }

        let is_unis = data[0] == b'J' && data[1] == b'N';

        let name = read_string(&data[2..], 0, 108)?;
        let num_samples = data[110] as usize;
        let num_patterns = data[111] as usize;
        let _loop_order = data[112];

        if num_samples > 64 || num_patterns > 128 {
            return Err(FormatError::InvalidHeader {
                expected: "valid sample/pattern count".to_string(),
                found: [num_samples as u8, num_patterns as u8, 0, 0],
            });
        }

        let order_list = data[113..241].to_vec();
        let mut order_list: Vec<u8> = order_list.iter().take_while(|&&o| o != 0xFF).copied().collect();

        if order_list.is_empty() {
            for i in 0..num_patterns.min(128) {
                if order_list.len() <= i && i < num_patterns {
                    order_list.push(i as u8);
                }
            }
        }

        let pattern_tempos = data[241..369].to_vec();
        let _pattern_breaks = data[369..497].to_vec();

        let mut offset = C669_HEADER_SIZE;

        let mut samples = vec![Sample::default()];
        let mut instruments = vec![Instrument::default()];

        for i in 0..num_samples {
            if offset + C669_INSTRUMENT_SIZE > data.len() {
                samples.push(Sample::default());
                instruments.push(Instrument::default());
                offset += C669_INSTRUMENT_SIZE;
                continue;
            }

            let inst_name = read_string(&data[offset..], 0, 13)?;
            let inst_length = u32_at(&data, offset + 13) as usize;
            let loop_start = u32_at(&data, offset + 17) as usize;
            let loop_end = u32_at(&data, offset + 21) as usize;

            offset += C669_INSTRUMENT_SIZE;

            let sample_data_offset = offset;
            offset += inst_length;

            let has_loop = loop_end > loop_start && loop_end <= inst_length;
            let loop_type = if has_loop {
                LoopType::Forward
            } else {
                LoopType::None
            };

            let sample_data = if inst_length > 0 && sample_data_offset + inst_length <= data.len() {
                data[sample_data_offset..sample_data_offset + inst_length]
                    .iter()
                    .map(|&b| (b as i8 as f32) / 128.0)
                    .collect()
            } else {
                Vec::new()
            };

            let sample = Sample {
                name: inst_name.clone(),
                data: Arc::new(sample_data),
                loop_start,
                loop_end,
                loop_type,
                default_volume: 64,
                sample_rate: 8363,
                ..Default::default()
            };
            samples.push(sample);

            let inst = Instrument {
                name: inst_name,
                sample_map: [(i + 1) as u8; 120],
                global_volume: 128,
                ..Default::default()
            };
            instruments.push(inst);
        }

        let mut patterns = Vec::new();
        for pattern_idx in 0..num_patterns.min(128) {
            let mut pattern = Pattern::new(C669_PATTERN_ROWS);

            let pattern_offset = C669_HEADER_SIZE
                + num_samples * C669_INSTRUMENT_SIZE
                + pattern_idx * C669_NUM_CHANNELS * C669_PATTERN_ROWS * 3;

            if pattern_offset >= data.len() {
                patterns.push(pattern);
                continue;
            }

            for row in 0..C669_PATTERN_ROWS {
                for ch in 0..C669_NUM_CHANNELS {
                    let event_offset = pattern_offset + (row * C669_NUM_CHANNELS + ch) * 3;
                    if event_offset + 3 > data.len() {
                        continue;
                    }

                    let byte1 = data[event_offset];
                    let byte2 = data[event_offset + 1];
                    let byte3 = data[event_offset + 2];

                    let note_val = byte1 & 0x7F;
                    let inst_val = if byte1 & 0x80 != 0 { Some(byte2 & 0x3F) } else { None };

                    let vol_val = if byte1 & 0x80 != 0 {
                        Some(byte2 >> 2)
                    } else {
                        None
                    };

                    let effect_code = byte3 & 0x1F;
                    let effect_param = byte3 >> 5;

                    if note_val > 0 && note_val < 120 {
                        pattern.data[row][ch].note = Note::On(note_val);
                    }

                    if let Some(inst) = inst_val {
                        pattern.data[row][ch].instrument = Some(inst.min(63) as u8);
                    }

                    if let Some(vol) = vol_val {
                        if vol <= 64 {
                            pattern.data[row][ch].volume = Some(vol);
                        }
                    }

                    pattern.data[row][ch].effect = convert_c669_effect(effect_code, effect_param, is_unis);
                }
            }

            patterns.push(pattern);
        }

        let initial_tempo = if !pattern_tempos.is_empty() { pattern_tempos[0].max(32) } else { 78 };
        let initial_speed = 6;

        let mut channel_panning = Vec::with_capacity(C669_NUM_CHANNELS);
        for i in 0..C669_NUM_CHANNELS {
            channel_panning.push(if i % 2 == 0 { 0 } else { 64 });
        }

        Ok(Module {
            name,
            message: None,
            format: ModuleFormat::C669,
            _version: if is_unis { 2 } else { 1 },
            tracker_name: if is_unis { String::from("UNIS 669") } else { String::from("Composer 669") },
            order_list,
            patterns,
            instruments,
            samples,
            initial_bpm: initial_tempo as u16,
            initial_speed,
            initial_global_volume: 128,
            initial_mixing_volume: 128,
            channel_panning,
            channel_volume: vec![64u8; C669_NUM_CHANNELS],
            flags: crate::sequencer::ModuleFlags {
                linear_slides: true,
                ..Default::default()
            },
            send_bus_config: Default::default(),
            send_return_levels: Default::default(),
            send_pre_fader: Default::default(),
            automation_tracks: Vec::new(),
            next_automation_id: 0,
        })
    }
}

fn convert_c669_effect(code: u8, param: u8, is_unis: bool) -> Effect {
    match code {
        0 => Effect::PortamentoUp { speed: param },
        1 => Effect::PortamentoDown { speed: param },
        2 => Effect::TonePortamento { speed: param },
        3 => {
            if is_unis {
                Effect::SetFineTune { tune: param }
            } else {
                Effect::FormatSpecific(FormatEffect::C669(C669Effect::Finetune { tune: param }))
            }
        }
        4 => Effect::Vibrato {
            speed: param >> 4,
            depth: param & 0x0F,
        },
        5 => Effect::SetSpeed { speed: param },
        _ => Effect::FormatSpecific(FormatEffect::C669(C669Effect::Raw { effect: code, param })),
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
        let handler = C669Handler;
        assert_eq!(handler.format_id(), "669");
    }

    #[test]
    fn handler_file_extension() {
        let handler = C669Handler;
        assert_eq!(handler.file_extension(), "669");
    }

    #[test]
    fn detect_standard_669() {
        let handler = C669Handler;
        let mut data = vec![0u8; 500];
        data[0] = b'i';
        data[1] = b'f';
        assert!(handler.detect(&data));
    }

    #[test]
    fn detect_unis_669() {
        let handler = C669Handler;
        let mut data = vec![0u8; 500];
        data[0] = b'J';
        data[1] = b'N';
        assert!(handler.detect(&data));
    }

    #[test]
    fn detect_invalid() {
        let handler = C669Handler;
        let data = vec![0u8; 500];
        assert!(!handler.detect(&data));
    }
}