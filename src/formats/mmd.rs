use crate::errors::{FormatError, FormatResult};
use crate::formats::FormatHandler;
use crate::sequencer::{
    effect::{FormatEffect, MmdEffect},
    Effect, Instrument, LoopType, Module, ModuleFormat, Note, Pattern, Sample,
};

const MMD0_MAGIC: &[u8; 4] = b"MMD0";
const MMD1_MAGIC: &[u8; 4] = b"MMD1";

pub struct MmdHandler;

impl FormatHandler for MmdHandler {
    fn format_id(&self) -> &'static str {
        "MMD"
    }

    fn file_extension(&self) -> &'static str {
        "mmd"
    }

    fn detect(&self, data: &[u8]) -> bool {
        if data.len() < 4 {
            return false;
        }
        let magic = [data[0], data[1], data[2], data[3]];
        magic == *MMD0_MAGIC || magic == *MMD1_MAGIC
    }

    fn load(&self, data: &[u8]) -> FormatResult<Module> {
        if data.len() < 24 {
            return Err(FormatError::TruncatedFile {
                expected_size: 24,
                actual_size: data.len(),
            });
        }

        let is_mmd1 = data[0] == b'M' && data[1] == b'M' && data[2] == b'D' && data[3] == b'1';

        let _mod_len = read_be_u32(data, 4);
        let song_offset = read_be_u32(data, 8);
        let block_arr_offset = read_be_u32(data, 12);
        let sample_arr_offset = read_be_u32(data, 16);

        let mut offset = song_offset as usize;
        if offset >= data.len() || offset == 0 {
            return Err(FormatError::InvalidHeader {
                expected: "valid song offset".to_string(),
                found: [0, 0, 0, 0],
            });
        }

        let mut samples = vec![Sample::default()];
        let mut instruments = vec![Instrument::default()];

        for i in 0..63 {
            let sample_offset = (sample_arr_offset as usize + i * 29).min(data.len().saturating_sub(29));
            if sample_offset >= data.len() {
                samples.push(Sample::default());
                instruments.push(Instrument::default());
                continue;
            }

            let sample_type = read_be_u32(data, sample_offset);
            if sample_type == 0 {
                samples.push(Sample::default());
                instruments.push(Instrument::default());
                continue;
            }

            let name = read_string_direct(data, sample_offset + 4, 20)?;
            let addr = read_be_u32(data, sample_offset + 24);

            let mut sample_len: usize = 0;
            let loop_start: usize = 0;
            let mut loop_end: usize = 0;
            let default_vol: u8 = 64;
            let c4_speed: u32 = 8363;

            if addr > 0 && addr < data.len() as u32 {
                let addr_usize = addr as usize;
                if sample_type == 1 {
                    sample_len = data.len().saturating_sub(addr_usize);
                    loop_end = sample_len;
                }
            }

            let loop_type = if loop_end > loop_start && loop_end <= sample_len {
                LoopType::Forward
            } else {
                LoopType::None
            };

            let sample_data = if addr > 0 && (addr as usize) + sample_len <= data.len() {
                let start = addr as usize;
                let end = start + sample_len;
                if end <= data.len() {
                    data[start..end].iter().map(|&b| (b as i8 as f32) / 128.0).collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            let sample = Sample {
                name: name.clone(),
                data: std::sync::Arc::new(sample_data),
                loop_start,
                loop_end,
                loop_type,
                default_volume: default_vol,
                sample_rate: c4_speed,
                ..Default::default()
            };
            samples.push(sample);

            let inst = Instrument {
                name,
                sample_map: [(i + 1) as u8; 120],
                global_volume: 128,
                ..Default::default()
            };
            instruments.push(inst);
        }

        let num_blocks = read_be_u16(data, offset) as usize;
        offset = offset.saturating_add(2);

        let song_len = read_be_u16(data, offset) as usize;
        offset = offset.saturating_add(2);

        let mut order_list = Vec::new();
        for _ in 0..song_len.min(256) {
            if offset < data.len() {
                let o = data[offset];
                if o != 0xFF {
                    order_list.push(o);
                }
                offset += 1;
            }
        }

        let default_tempo = if offset < data.len() { data[offset] } else { 125 };
        // offset = offset.saturating_add(2); // Unused

        let mut patterns = Vec::new();
        let max_tracks = 64;

        if block_arr_offset > 0 && (block_arr_offset as usize) < data.len() {
            let mut block_ptr_offset = block_arr_offset as usize;

            for _block_idx in 0..num_blocks.min(256) {
                if block_ptr_offset + 4 > data.len() {
                    patterns.push(Pattern::new(64));
                    continue;
                }

                let block_ptr = read_be_u32(data, block_ptr_offset);
                block_ptr_offset += 4;

                if block_ptr == 0 || (block_ptr as usize) >= data.len() {
                    patterns.push(Pattern::new(64));
                    continue;
                }

                let mut block_offset = block_ptr as usize;

                let num_tracks = if block_offset < data.len() { data[block_offset] as usize } else { 0 };
                block_offset = block_offset.saturating_add(1);

                let num_lines = if block_offset < data.len() { data[block_offset] as usize } else { 64 };
                block_offset = block_offset.saturating_add(1);

                if is_mmd1 {
                    block_offset = block_offset.saturating_add(2);
                }

                let track_data_offset = if block_offset + 4 <= data.len() {
                    read_be_u32(data, block_offset) as usize
                } else {
                    0
                };
                // block_offset = block_offset.saturating_add(4); // Unused

                let mut pattern = Pattern::new(64);

                if track_data_offset > 0 && track_data_offset < data.len() {
                    let mut track_offset = track_data_offset;

                    for track_idx in 0..num_tracks.min(max_tracks) {
                        if track_offset + 4 > data.len() {
                            break;
                        }
                        let track_ptr = read_be_u32(data, track_offset) as usize;
                        track_offset += 4;

                        if track_ptr == 0 || track_ptr >= data.len() {
                            continue;
                        }

                        let mut line_offset = track_ptr;

                        for line_idx in 0..num_lines.min(64) {
                            if line_offset >= data.len() {
                                break;
                            }

                            let (event_data, bytes_read) = if is_mmd1 {
                                if line_offset + 4 > data.len() {
                                    break;
                                }
                                (read_be_u32(data, line_offset), 4)
                            } else {
                                if line_offset + 2 > data.len() {
                                    break;
                                }
                                (u32::from_be_bytes([0, 0, data[line_offset], data[line_offset + 1]]), 2)
                            };
                            line_offset += bytes_read;

                            let ch = track_idx.min(31);
                            let event_byte1 = ((event_data >> 16) & 0xFF) as u8;
                            let event_byte2 = ((event_data >> 8) & 0xFF) as u8;
                            let event_byte3 = (event_data & 0xFF) as u8;

                            let note_val = event_byte1 & 0x3F;
                            if note_val > 0 && note_val < 120 {
                                pattern.data[line_idx][ch].note = Note::On(note_val);
                            }

                            let inst_val = event_byte2 & 0x3F;
                            if inst_val > 0 && inst_val <= 63 {
                                pattern.data[line_idx][ch].instrument = Some(inst_val);
                            }

                            let vol_val = (event_byte2 >> 4) & 0x0F;
                            if vol_val > 0 && vol_val <= 15 {
                                pattern.data[line_idx][ch].volume = Some(vol_val * 4 + 3);
                            }

                            pattern.data[line_idx][ch].effect =
                                convert_mmd_effect(event_byte3 & 0x7F);
                        }
                    }
                }

                patterns.push(pattern);
            }
        }

        let num_channels = patterns.iter()
            .map(|p| p.data.iter().any(|row| row.iter().any(|c| !c.is_empty())))
            .count()
            .max(4)
            .min(32);

        let channel_panning = vec![32u8; num_channels];

        Ok(Module {
            name: String::new(),
            message: None,
            format: ModuleFormat::Mmd,
            _version: if is_mmd1 { 1 } else { 0 },
            tracker_name: if is_mmd1 { String::from("OctaMED Professional") } else { String::from("OctaMED") },
            order_list,
            patterns,
            instruments,
            samples,
            initial_bpm: default_tempo as u16,
            initial_speed: 6,
            initial_global_volume: 128,
            initial_mixing_volume: 128,
            channel_panning,
            channel_volume: vec![64u8; num_channels],
            flags: crate::sequencer::ModuleFlags::default(),
            send_bus_config: Default::default(),
            send_return_levels: Default::default(),
            send_pre_fader: Default::default(),
            send_bus_plugins: Default::default(),
            automation_tracks: Vec::new(),
            next_automation_id: 0,
        })
    }
}

fn convert_mmd_effect(effect_code: u8) -> Effect {
    match effect_code {
        0x00 => Effect::None,
        0x01 => Effect::PortamentoUp { speed: 1 },
        0x02 => Effect::PortamentoDown { speed: 1 },
        0x03 => Effect::TonePortamento { speed: 1 },
        0x04 => Effect::Vibrato { speed: 4, depth: 4 },
        0x08 => Effect::SetPanning { pan: 32 },
        0x09 => Effect::SetSampleOffset { offset: 0 },
        0x0A => Effect::Vibrato { speed: 4, depth: 4 },
        0x0D => Effect::VolumeSlide { up: 0, down: 1 },
        0x0F => Effect::SetTempo { bpm: 125 },
        0x11 => Effect::NoteCutAfter { ticks: 0 },
        0x12 => Effect::NoteDelay { ticks: 0 },
        0x1C => Effect::Retrigger { interval: 1 },
        0x1D => Effect::Retrigger { interval: 1 },
        _ => Effect::FormatSpecific(FormatEffect::Mmd(MmdEffect::Raw {
            effect: effect_code,
            param: 0,
        })),
    }
}

fn read_be_u32(data: &[u8], offset: usize) -> u32 {
    if offset + 4 > data.len() {
        return 0;
    }
    u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

fn read_be_u16(data: &[u8], offset: usize) -> u16 {
    if offset + 2 > data.len() {
        return 0;
    }
    u16::from_be_bytes([data[offset], data[offset + 1]])
}

fn read_string_direct(data: &[u8], offset: usize, len: usize) -> FormatResult<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_format_id() {
        let handler = MmdHandler;
        assert_eq!(handler.format_id(), "MMD");
    }

    #[test]
    fn handler_file_extension() {
        let handler = MmdHandler;
        assert_eq!(handler.file_extension(), "mmd");
    }

    #[test]
    fn detect_mmd0() {
        let handler = MmdHandler;
        let mut data = vec![0u8; 100];
        data[0..4].copy_from_slice(b"MMD0");
        assert!(handler.detect(&data));
    }

    #[test]
    fn detect_mmd1() {
        let handler = MmdHandler;
        let mut data = vec![0u8; 100];
        data[0..4].copy_from_slice(b"MMD1");
        assert!(handler.detect(&data));
    }

    #[test]
    fn detect_invalid() {
        let handler = MmdHandler;
        let data = vec![0u8; 100];
        assert!(!handler.detect(&data));
    }
}
