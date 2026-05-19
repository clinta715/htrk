use std::sync::Arc;

use crate::errors::{FormatError, FormatResult};
use crate::formats::FormatHandler;
use crate::sequencer::{
    effect::{FormatEffect, S3mEffect},
    Effect, Instrument, LoopType, Module, ModuleFormat, Note, Pattern, Sample,
    MAX_CHANNELS,
};

const S3M_HEADER_SIZE: usize = 96;
const S3M_SAMPLE_HEADER_SIZE: usize = 80;
const S3M_PATTERN_ROWS: usize = 64;
const S3M_MAX_CHANNELS: usize = 32;

fn convert_s3m_effect(effect_code: u8, param: u8) -> Effect {
    match effect_code {
        1 => Effect::SetTempo { bpm: param },
        2 => Effect::PositionJump { order: param },
        3 => {
            let row = (param >> 4) * 10 + (param & 0x0F);
            Effect::PatternBreak { row }
        }
        4 => Effect::VolumeSlide {
            up: param >> 4,
            down: param & 0x0F,
        },
         5 => {
            if param >= 0xF0 {
                Effect::ExtraFinePortamentoDown { speed: param & 0x0F }
            } else if param >= 0xE0 {
                Effect::FinePortamentoDown { speed: param & 0x0F }
            } else {
                Effect::PortamentoDown { speed: param }
            }
        }
        6 => {
            if param >= 0xF0 {
                Effect::ExtraFinePortamentoUp { speed: param & 0x0F }
            } else if param >= 0xE0 {
                Effect::FinePortamentoUp { speed: param & 0x0F }
            } else {
                Effect::PortamentoUp { speed: param }
            }
        }
        7 => Effect::TonePortamento { speed: param },
        8 => Effect::Vibrato {
            speed: param >> 4,
            depth: param & 0x0F,
        },
        9 => Effect::Tremor { ontime: param >> 4, offtime: param & 0x0F },
        10 => Effect::Arpeggio {
            note1: param >> 4,
            note2: param & 0x0F,
        },
        11 => Effect::VibratoVolumeSlide { up: param as i8 },
        12 => Effect::TonePortamentoVolumeSlide { up: param as i8 },
        13 => Effect::VolSetVolume { vol: param.min(64) },
        14 => Effect::VolumeSlide {
            up: param >> 4,
            down: param & 0x0F,
        }, // N - Channel volume slide (if supported)
        15 => Effect::FormatSpecific(FormatEffect::S3m(S3mEffect::SetSampleOffset(
            (param as u16) << 8,
        ))),
        16 => Effect::None, // P - Panning slide
        17 => Effect::Retrigger { interval: param },
        18 => Effect::Tremolo {
            speed: param >> 4,
            depth: param & 0x0F,
        },
        19 => {
            let sub = param >> 4;
            let val = param & 0x0F;
            match sub {
                0x1 => Effect::GlissandoControl { on: val != 0 },
                0x2 => Effect::SetFineTune { tune: val },
                0x3 => Effect::VibratoWaveform { waveform: val & 0x03 },
                0x4 => Effect::TremoloWaveform { waveform: val & 0x03 },
                0x5 => Effect::SetPanPosition { pan: (val << 4) | val },
                0x6 => Effect::PatternLoop { count: val },
                0x7 => Effect::TremoloWaveform { waveform: val & 0x03 },
                0x8 => Effect::SetPanning { pan: (val << 4) | val },
                0x9 => Effect::None, // S9x - not used
                0xA => Effect::FormatSpecific(FormatEffect::S3m(S3mEffect::Raw { effect: 0x19A, param: val })), // SAx - High Sample Offset
                0xB => Effect::PatternLoop { count: val }, // SBx - Pattern Loop
                0xC => Effect::NoteCutAfter { ticks: val }, // SCx - Note Cut
                0xD => Effect::NoteDelay { ticks: val }, // SDx - Note Delay
                0xE => Effect::PatternDelay { ticks: val }, // SEx - Pattern Delay
                _ => Effect::None, // S0x, SFx and others - not used
            }
        }
        20 => Effect::SetSpeed { speed: param },
        21 => Effect::Vibrato {
            speed: param >> 4,
            depth: param & 0x0F,
        }, // U - Fine Vibrato (simplified as Vibrato for now)
        22 => Effect::SetGlobalVolume { volume: (param.min(64) as u16 * 128 / 64) as u8 }, // V - Set Global Volume
        23 => Effect::GlobalVolumeSlide {
            up: (param >> 4) as i8 * 2,
            down: (param & 0x0F) as i8 * 2,
        }, // W - Global Volume Slide
        24 => Effect::SetPanning { pan: (param as u16 * 255 / 128).min(255) as u8 }, // X - Set Panning
        _ if effect_code > 0 => Effect::FormatSpecific(FormatEffect::S3m(S3mEffect::Raw { effect: effect_code as u16, param })),
        _ => Effect::None,
    }
}

fn effect_to_s3m(effect: &Effect) -> (u8, u8) {
    match effect {
        Effect::None => (0, 0),
        Effect::SetSpeed { speed } => (20, *speed),
        Effect::PositionJump { order } => (2, *order),
        Effect::PatternBreak { row } => (3, ((row / 10) << 4) | (row % 10)),
        Effect::VolumeSlide { up, down } => (4, (*up << 4) | *down),
        Effect::PortamentoDown { speed } => (5, *speed),
        Effect::PortamentoUp { speed } => (6, *speed),
        Effect::FinePortamentoDown { speed } => (5, 0xE0 | speed),
        Effect::FinePortamentoUp { speed } => (6, 0xE0 | speed),
        Effect::ExtraFinePortamentoDown { speed } => (5, 0xF0 | speed),
        Effect::ExtraFinePortamentoUp { speed } => (6, 0xF0 | speed),
        Effect::TonePortamento { speed } => (7, *speed),
        Effect::Vibrato { speed, depth } => (8, (*speed << 4) | *depth),
        Effect::Tremor { ontime, offtime } => (9, (*ontime << 4) | *offtime),
        Effect::Arpeggio { note1, note2 } => (10, (*note1 << 4) | *note2),
        Effect::VibratoVolumeSlide { up } => (11, *up as u8),
        Effect::TonePortamentoVolumeSlide { up } => (12, *up as u8),
        Effect::VolSetVolume { vol } => (13, *vol),
        Effect::SetVolume { volume } => (13, (*volume).min(64)),
        Effect::SetSampleOffset { offset } => (15, (*offset >> 8) as u8),
        Effect::Retrigger { interval } => (17, *interval),
        Effect::Tremolo { speed, depth } => (18, (*speed << 4) | *depth),
        Effect::SetTempo { bpm } => (1, *bpm),
        Effect::NoteCutAfter { ticks } => (19, 0xC0 | *ticks),
        Effect::NoteDelay { ticks } => (19, 0xD0 | *ticks),
        Effect::SetPanPosition { pan } => (19, 0x50 | (pan >> 4)),
        Effect::PatternDelay { ticks } => (19, 0xA0 | *ticks),
        Effect::GlissandoControl { on } => (19, 0x10 | if *on { 1 } else { 0 }),
        Effect::VibratoWaveform { waveform } => (19, 0x30 | (waveform & 0x03)),
        Effect::TremoloWaveform { waveform } => (19, 0x40 | (waveform & 0x03)),
        Effect::SetPanning16 { pan } => (19, 0x80 | (pan >> 4)),
        Effect::SetPanning { pan } => (19, 0x80 | (pan >> 4)),
        Effect::SetFineTune { tune } => (19, 0x20 | (tune & 0x0F)),
        Effect::PatternLoop { count } => (19, 0x60 | (count & 0x0F)),
        Effect::FormatSpecific(FormatEffect::S3m(S3mEffect::Raw { effect, param })) => {
            if *effect >= 0x190 {
                (19, (((*effect as u8) & 0x0F) << 4) | (param & 0x0F))
            } else {
                (*effect as u8, *param)
            }
        }
        Effect::FormatSpecific(FormatEffect::S3m(S3mEffect::SetSampleOffset(offset))) => (15, (offset >> 8) as u8),
        Effect::FormatSpecific(_) => (0, 0),
        _ => (0, 0),
    }
}

pub struct S3mHandler;

impl FormatHandler for S3mHandler {
    fn format_id(&self) -> &'static str {
        "S3M"
    }

    fn file_extension(&self) -> &'static str {
        "s3m"
    }

    fn detect(&self, data: &[u8]) -> bool {
        if data.len() < S3M_HEADER_SIZE {
            return false;
        }
        &data[44..48] == b"SCRM"
    }

    fn load(&self, data: &[u8]) -> FormatResult<Module> {
        if data.len() < S3M_HEADER_SIZE {
            return Err(FormatError::TruncatedFile {
                expected_size: S3M_HEADER_SIZE,
                actual_size: data.len(),
            });
        }

        check_magic(data, 44, b"SCRM")?;

        let name = read_string(data, 0, 28)?;
        let order_count = u16_at(data, 32) as usize;
        let sample_count = u16_at(data, 34) as usize;
        let pattern_count = u16_at(data, 36) as usize;
        let sample_format = u16_at(data, 42);
        let global_volume = data[48];
        let initial_speed = data[49];
        let initial_tempo = data[50];
        let master_volume = data[51];
        let default_pan_flag = data[53];

        let mut num_channels = 0usize;
        let mut channel_settings = [0u8; 32];
        for i in 0..32 {
            channel_settings[i] = data[60 + i];
            if channel_settings[i] != 0xFF {
                num_channels = num_channels.max((channel_settings[i] & 0x1F) as usize + 1);
            }
        }
        let _num_channels = if num_channels == 0 { 4 } else { num_channels.min(MAX_CHANNELS).max(1) };

        let mut offset = S3M_HEADER_SIZE;

        let orders = read_bytes(data, &mut offset, order_count)?.to_vec();
        let _filtered_orders: Vec<u8> = orders.iter().filter(|&&o| o != 0xFF && o != 0xFE).copied().collect();

        let sample_paraptrs = read_u16_vec(data, &mut offset, sample_count)?;
        let pattern_paraptrs = read_u16_vec(data, &mut offset, pattern_count)?;

        let mut default_panning = [0u8; 32];
        if default_pan_flag == 0xFC || default_pan_flag == 0xFF {
            let pan_data = read_bytes(data, &mut offset, 32)?;
            default_panning.copy_from_slice(pan_data);
        }

        let mut samples = vec![Sample::default()];

        for i in 0..sample_count {
            let para = sample_paraptrs[i] as u32;
            let hdr_offset = para as usize * 16;
            if hdr_offset + S3M_SAMPLE_HEADER_SIZE > data.len() {
                samples.push(Sample::default());
                continue;
            }

            let sample_type = data[hdr_offset];
            let sample_filename = read_string(data, hdr_offset + 1, 12)?;
            let _ = sample_filename;
            let data_length = u32_at(data, hdr_offset + 16) as usize;
            let loop_start_raw = u32_at(data, hdr_offset + 20) as usize;
            let loop_end_raw = u32_at(data, hdr_offset + 24) as usize;
            let default_volume = data[hdr_offset + 28].min(64);
            let is_16bit = hdr_offset + 32 <= data.len() && (data[hdr_offset + 31] & 0x04) != 0;
            let c2speed = u32_at(data, hdr_offset + 32);
            let sample_name = read_string(data, hdr_offset + 48, 28)?;
            let sample_rate = if c2speed > 0 { c2speed } else { 8363u32 };

            let data_offset = if sample_type == 1 {
                let hi = data[hdr_offset + 13] as u32;
                let lo = u16_at(data, hdr_offset + 14) as u32;
                ((hi << 16) | lo) as usize * 16
            } else {
                0
            };

            let has_loop = hdr_offset + 32 <= data.len() && (data[hdr_offset + 31] & 0x01) != 0;
            let is_ping_pong = hdr_offset + 32 <= data.len() && (data[hdr_offset + 31] & 0x02) != 0;
            let loop_type = if has_loop && loop_end_raw > loop_start_raw && loop_end_raw <= data_length {
                if is_ping_pong {
                    LoopType::PingPong
                } else {
                    LoopType::Forward
                }
            } else {
                LoopType::None
            };

            let is_unsigned = sample_format == 2;

            let sample_data = if sample_type == 1 && data_length > 0 && data_offset + data_length <= data.len() {
                if is_16bit {
                    data[data_offset..data_offset + data_length]
                        .chunks_exact(2)
                        .map(|chunk| {
                            let val = i16::from_le_bytes([chunk[0], chunk[1]]);
                            val as f32 / 32768.0
                        })
                        .collect::<Vec<f32>>()
                } else {
                    data[data_offset..data_offset + data_length]
                        .iter()
                        .map(|&b| {
                            let val = if is_unsigned {
                                (b as i16) - 128
                            } else {
                                b as i8 as i16
                            };
                            val as f32 / 128.0
                        })
                        .collect::<Vec<f32>>()
                }
            } else {
                Vec::new()
            };

            let loop_start = if is_16bit { loop_start_raw / 2 } else { loop_start_raw };
            let loop_end = if loop_type != LoopType::None {
                if is_16bit { loop_end_raw / 2 } else { loop_end_raw }
            } else {
                0
            };

            let pan = if i < 32 && (default_panning[i] & 0x80) != 0 {
                ((default_panning[i] & 0x0F) as u8) * 4
            } else {
                32
            };

            samples.push(Sample {
                name: sample_name,
                data: Arc::new(sample_data),
                sample_rate,
                bits_per_sample: if is_16bit { 16 } else { 8 },
                loop_type,
                loop_start,
                loop_end,
                default_volume,
                default_panning: pan,
                global_volume: 64,
                relative_note: 0,
                fine_tune: 0,
                vibrato_speed: 0,
                vibrato_depth: 0,
                vibrato_rate: 0,
                vibrato_waveform: crate::sequencer::VibratoWaveform::Sine,
                _flags: crate::sequencer::SampleFlags::default(),
            });
        }

        let mut patterns = Vec::new();
        for pat_idx in 0..pattern_count {
            let para = pattern_paraptrs[pat_idx] as u32;
            let pat_offset = para as usize * 16;

            let mut pattern = Pattern::new(S3M_PATTERN_ROWS);

            if pat_offset + 2 > data.len() {
                patterns.push(pattern);
                continue;
            }

            let packed_len = u16_at(data, pat_offset) as usize;
            if pat_offset + 2 + packed_len > data.len() {
                patterns.push(pattern);
                continue;
            }

            let packed = &data[pat_offset + 2..pat_offset + 2 + packed_len];
            let mut pos = 0usize;

            for row in 0..S3M_PATTERN_ROWS {
                if row >= pattern.num_rows {
                    break;
                }
                while pos < packed.len() {
                    let byte = packed[pos];
                    if byte == 0 {
                        pos += 1;
                        break;
                    }
                    let channel = (byte & 0x1F) as usize;
                    let what = byte >> 5;
                    pos += 1;

                    if what & 0x01 != 0 {
                        if pos + 2 > packed.len() {
                            break;
                        }
                        let note_byte = packed[pos];
                        let instrument = if packed[pos + 1] > 0 {
                            Some(packed[pos + 1])
                        } else {
                            None
                        };
                        pos += 2;
                        let note = match note_byte {
                            0xFF | 0xFE => Note::Off,
                            0 => Note::None,
                            v => {
                                let octave = v >> 4;
                                let semitone = v & 0x0F;
                                if octave < 12 && semitone < 12 {
                                    let key = octave as u8 * 12 + semitone as u8 + 12;
                                    if key < 120 {
                                        Note::On(key)
                                    } else {
                                        Note::None
                                    }
                                } else {
                                    Note::None
                                }
                            }
                        };
                        if channel < MAX_CHANNELS && channel < S3M_MAX_CHANNELS {
                            let cell = &mut pattern.data[row][channel];
                            cell.note = note;
                            cell.instrument = instrument;
                        }
                    }

                    if what & 0x02 != 0 {
                        if pos >= packed.len() {
                            break;
                        }
                        let vol_byte = packed[pos];
                        pos += 1;
                        if channel < MAX_CHANNELS && channel < S3M_MAX_CHANNELS && vol_byte <= 64 {
                            pattern.data[row][channel].volume = Some(vol_byte);
                        }
                    }

                    if what & 0x04 != 0 {
                        if pos + 2 > packed.len() {
                            break;
                        }
                        let effect_code = packed[pos];
                        let effect_param = packed[pos + 1];
                        pos += 2;
                        if channel < MAX_CHANNELS && channel < S3M_MAX_CHANNELS {
                            pattern.data[row][channel].effect = convert_s3m_effect(effect_code, effect_param);
                        }
                    }
                }
            }

            patterns.push(pattern);
        }

        let mut instruments = vec![Instrument::default()];

        for i in 0..samples.len() - 1 {
            let sample_idx = (i + 1) as u8;
            let mut inst = Instrument {
                name: samples[i + 1].name.clone(),
                sample_map: [sample_idx; 120],
                ..Instrument::default()
            };
            inst.global_volume = 128;
            instruments.push(inst);
        }

        let s3m_count = _num_channels;
        let mut channel_panning = vec![32u8; s3m_count];
        for ch in 0..s3m_count.min(S3M_MAX_CHANNELS) {
            if channel_settings[ch] == 0xFF {
                continue;
            }
            if (default_panning[ch] & 0x80) != 0 {
                channel_panning[ch] = ((default_panning[ch] & 0x0F) as u8) * 4;
            } else if ch % 2 == 0 {
                channel_panning[ch] = 0;
            } else {
                channel_panning[ch] = 64;
            }
        }

        Ok(Module {
            name,
            message: None,
            format: ModuleFormat::S3M,
            _version: 0,
            tracker_name: String::from("ScreamTracker 3"),
            order_list: _filtered_orders,
            patterns,
            instruments,
            samples,
            initial_bpm: initial_tempo as u16,
            initial_speed: if initial_speed == 0 { 6 } else { initial_speed },
            initial_global_volume: if global_volume == 0 { 128 } else { (global_volume as u16 * 128 / 64).min(128) as u8 },
            initial_mixing_volume: if master_volume == 0 { 48 } else { master_volume },
            channel_panning,
            channel_volume: vec![64u8; s3m_count],
            flags: crate::sequencer::ModuleFlags::default(),
            send_bus_config: Default::default(),
            send_return_levels: Default::default(),
            automation_tracks: Vec::new(),
            next_automation_id: 0,
        })
    }
}

fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

fn read_string(data: &[u8], offset: usize, len: usize) -> FormatResult<String> {
    if offset + len > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: offset + len,
            actual_size: data.len(),
        });
    }
    Ok(std::str::from_utf8(&data[offset..offset + len])
        .unwrap_or("")
        .trim_end_matches('\0')
        .trim_end()
        .to_string())
}

fn read_bytes<'a>(data: &'a [u8], offset: &mut usize, len: usize) -> FormatResult<&'a [u8]> {
    if *offset + len > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: *offset + len,
            actual_size: data.len(),
        });
    }
    let slice = &data[*offset..*offset + len];
    *offset += len;
    Ok(slice)
}

fn read_u16_vec(data: &[u8], offset: &mut usize, count: usize) -> FormatResult<Vec<u16>> {
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        if *offset + 2 > data.len() {
            return Err(FormatError::TruncatedFile {
                expected_size: *offset + 2,
                actual_size: data.len(),
            });
        }
        result.push(u16_at(data, *offset));
        *offset += 2;
    }
    Ok(result)
}

fn check_magic(data: &[u8], offset: usize, expected: &'static [u8; 4]) -> FormatResult<()> {
    if offset + 4 > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: offset + 4,
            actual_size: data.len(),
        });
    }
    let found = &data[offset..offset + 4];
    if found != expected {
        let mut arr = [0u8; 4];
        arr.copy_from_slice(found);
        return Err(FormatError::InvalidHeader {
            expected: std::str::from_utf8(expected).unwrap_or("????").to_string(),
            found: arr,
        });
    }
    Ok(())
}

fn write_u16_le(buf: &mut Vec<u8>, val: u16) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn write_u32_le(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn pack_pattern(pattern: &Pattern) -> Vec<u8> {
    let mut packed = Vec::new();
    let rows = pattern.num_rows.min(S3M_PATTERN_ROWS);

    for row in 0..rows {
        for ch in 0..S3M_MAX_CHANNELS {
            let cell = if ch < MAX_CHANNELS && row < pattern.data.len() {
                &pattern.data[row][ch]
            } else {
                continue;
            };

            if cell.is_empty() {
                continue;
            }

            let mut what: u8 = 0;
            let has_note = cell.note != Note::None || cell.instrument.is_some();
            let has_vol = cell.volume.is_some();
            let has_fx = cell.effect != Effect::None;

            if has_note {
                what |= 0x20;
            }
            if has_vol {
                what |= 0x40;
            }
            if has_fx {
                what |= 0x80;
            }

            if what == 0 {
                continue;
            }

            packed.push(what | (ch as u8 & 0x1F));

            if has_note {
                match cell.note {
                    Note::Off => packed.push(0xFF),
                    Note::On(key) => {
                        let adjusted = key as i16 - 12;
                        if adjusted >= 0 && adjusted < 144 {
                            let octave = (adjusted / 12) as u8;
                            let semitone = (adjusted % 12) as u8;
                            if octave < 12 {
                                packed.push((octave << 4) | semitone);
                            } else {
                                packed.push(0);
                            }
                        } else {
                            packed.push(0);
                        }
                    }
                    _ => packed.push(0),
                }
                packed.push(cell.instrument.unwrap_or(0));
            }

            if has_vol {
                packed.push(cell.volume.unwrap_or(0).min(64));
            }

            if has_fx {
                let (code, param) = effect_to_s3m(&cell.effect);
                packed.push(code);
                packed.push(param);
            }
        }
        packed.push(0);
    }

    let mut result = Vec::with_capacity(2 + packed.len());
    write_u16_le(&mut result, packed.len() as u16);
    result.extend_from_slice(&packed);
    result
}

pub fn save_module(module: &Module) -> Vec<u8> {
    let mut buf = Vec::new();

    let name_bytes = module.name.as_bytes();
    let mut name_buf = [0u8; 28];
    let copy_len = name_bytes.len().min(28);
    name_buf[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
    buf.extend_from_slice(&name_buf);

    buf.push(0x1A);
    buf.push(16);
    buf.extend_from_slice(&[0, 0]);

    let order_count = module.order_list.len().min(256) as u16;
    write_u16_le(&mut buf, order_count);

    let num_samples = module.samples.len().min(99) as u16;
    write_u16_le(&mut buf, num_samples);

    let num_patterns = module.patterns.len().min(256) as u16;
    write_u16_le(&mut buf, num_patterns);

    write_u16_le(&mut buf, 0);
    write_u16_le(&mut buf, 0x1300);
    write_u16_le(&mut buf, 2);

    buf.extend_from_slice(b"SCRM");

    buf.push(module.initial_global_volume.min(128) as u8);
    buf.push(if module.initial_speed == 0 { 6 } else { module.initial_speed });
    buf.push(module.initial_bpm.min(255) as u8);
    buf.push(48);
    buf.push(0);
    buf.push(0xFC);
    buf.extend_from_slice(&[0, 0]);
    write_u32_le(&mut buf, 0);

    let mut channel_settings = [0xFFu8; 32];
    let active_channels = S3M_MAX_CHANNELS.min(MAX_CHANNELS);
    for ch in 0..active_channels {
        channel_settings[ch] = ch as u8;
    }
    buf.extend_from_slice(&channel_settings);

    buf.extend_from_slice(&module.order_list[..order_count as usize]);

    let sample_para_base = buf.len() + (num_samples as usize) * 2 + (num_patterns as usize) * 2 + 32;
    let first_sample_para = (sample_para_base + 15) / 16;

    let mut sample_paras = Vec::new();
    for i in 0..num_samples as usize {
        sample_paras.push((first_sample_para + i) as u16);
        write_u16_le(&mut buf, (first_sample_para + i) as u16);
    }

    let pattern_data_start_para = first_sample_para + num_samples as usize;
    for i in 0..num_patterns as usize {
        write_u16_le(&mut buf, (pattern_data_start_para + i) as u16);
    }

    let mut default_panning = [0u8; 32];
    for ch in 0..S3M_MAX_CHANNELS {
        let pan = if ch < module.channel_panning.len() {
            module.channel_panning[ch]
        } else if ch % 2 == 0 {
            0
        } else {
            64
        };
        default_panning[ch] = (pan >> 2) & 0x0F;
        if default_panning[ch] != 0 || pan > 0 {
            default_panning[ch] |= 0x80;
        }
    }
    buf.extend_from_slice(&default_panning);

    while buf.len() < first_sample_para as usize * 16 {
        buf.push(0);
    }

    let mut sample_data_offsets: Vec<usize> = Vec::new();
    for i in 0..num_samples as usize {
        let sample = if i < module.samples.len() {
            &module.samples[i]
        } else {
            &module.samples[0]
        };

        let mut shdr = [0u8; S3M_SAMPLE_HEADER_SIZE];

        let data_len = sample.data.len();
        if data_len > 0 {
            shdr[0] = 1;
        }

        let sn = sample.name.as_bytes();
        let sn_len = sn.len().min(28);
        shdr[48..48 + sn_len].copy_from_slice(&sn[..sn_len]);

        shdr[76..80].copy_from_slice(b"SCRS");

        write_u32_le_to_slice(&mut shdr[16..20], data_len as u32);

        if sample.loop_type != LoopType::None {
            shdr[31] |= 0x01;
            write_u32_le_to_slice(&mut shdr[20..24], sample.loop_start as u32);
            write_u32_le_to_slice(&mut shdr[24..28], sample.loop_end as u32);
        }

        shdr[28] = sample.default_volume.min(64);

        let c2speed = sample.sample_rate;
        shdr[32..36].copy_from_slice(&c2speed.to_le_bytes());

        let data_para = (buf.len() + S3M_SAMPLE_HEADER_SIZE + 15) / 16;
        let data_offset = data_para * 16;
        let data_offset_from_hdr = data_offset - buf.len() - S3M_SAMPLE_HEADER_SIZE;

        let hi = ((buf.len() + S3M_SAMPLE_HEADER_SIZE + data_offset_from_hdr) / 65536) as u8;
        let lo = (((buf.len() + S3M_SAMPLE_HEADER_SIZE + data_offset_from_hdr) / 16) & 0xFFFF) as u16;
        shdr[13] = hi;
        shdr[14..16].copy_from_slice(&lo.to_le_bytes());

        let data_abs_offset = buf.len() + S3M_SAMPLE_HEADER_SIZE + data_offset_from_hdr;
        sample_data_offsets.push(data_abs_offset);

        buf.extend_from_slice(&shdr);
    }

    for i in 0..num_samples as usize {
        let target = sample_data_offsets[i];
        while buf.len() < target {
            buf.push(0);
        }

        let sample = if i < module.samples.len() {
            &module.samples[i]
        } else {
            &module.samples[0]
        };

        for &s in sample.data.iter() {
            let val = (s * 128.0).round() as i32;
            buf.push(val.clamp(-128, 127) as u8);
        }
    }

    let _pattern_area_start = buf.len();
    for i in 0..num_patterns as usize {
        let para = pattern_data_start_para + i;
        let target = para * 16;
        while buf.len() < target {
            buf.push(0);
        }

        let packed = if i < module.patterns.len() {
            pack_pattern(&module.patterns[i])
        } else {
            let mut empty = Vec::new();
            write_u16_le(&mut empty, 0);
            empty
        };

        let current = buf.len();
        if current < target {
            buf.resize(target, 0);
        }
        buf.extend_from_slice(&packed);
    }

    buf
}

fn write_u32_le_to_slice(slice: &mut [u8], val: u32) {
    slice.copy_from_slice(&val.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_s3m() -> Vec<u8> {
        let mut buf = vec![0u8; S3M_HEADER_SIZE];

        let name = b"TestS3M\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        buf[0..28].copy_from_slice(name);
        buf[28] = 0x1A;
        buf[29] = 16;
        buf[32..34].copy_from_slice(&(1u16).to_le_bytes());
        buf[34..36].copy_from_slice(&(0u16).to_le_bytes());
        buf[36..38].copy_from_slice(&(1u16).to_le_bytes());
        buf[42..44].copy_from_slice(&(1u16).to_le_bytes());
        buf[44..48].copy_from_slice(b"SCRM");
        buf[48] = 64;
        buf[49] = 6;
        buf[50] = 125;
        buf[51] = 48;

        for i in 0..16 {
            buf[60 + i] = i as u8;
        }
        for i in 16..32 {
            buf[60 + i] = 0xFF;
        }

        buf.push(0);
        let pat_para: u16 = 16;
        buf.extend_from_slice(&pat_para.to_le_bytes());

        for _ in 0..32 {
            buf.push(0x80);
        }

        while buf.len() < 256 {
            buf.push(0);
        }

        let mut packed = Vec::new();
        packed.push(0);
        let mut pat_data = Vec::new();
        write_u16_le(&mut pat_data, packed.len() as u16);
        pat_data.extend_from_slice(&packed);

        buf.extend_from_slice(&pat_data);

        buf
    }

    #[test]
    fn detect_valid_s3m() {
        let data = build_minimal_s3m();
        let handler = S3mHandler;
        assert!(handler.detect(&data));
    }

    #[test]
    fn detect_too_small() {
        let data = [0u8; 50];
        let handler = S3mHandler;
        assert!(!handler.detect(&data));
    }

    #[test]
    fn detect_wrong_magic() {
        let mut data = vec![0u8; S3M_HEADER_SIZE];
        data[44..48].copy_from_slice(b"XXXX");
        let handler = S3mHandler;
        assert!(!handler.detect(&data));
    }

    #[test]
    fn load_minimal_s3m() {
        let data = build_minimal_s3m();
        let handler = S3mHandler;
        let module = handler.load(&data).unwrap();
        assert_eq!(module.name, "TestS3M");
        assert_eq!(module.format, ModuleFormat::S3M);
        assert_eq!(module.initial_speed, 6);
        assert_eq!(module.initial_bpm, 125);
        assert_eq!(module.order_list.len(), 1);
        assert_eq!(module.patterns.len(), 1);
    }

    #[test]
    fn load_truncated_errors() {
        let data = vec![0u8; 50];
        let handler = S3mHandler;
        assert!(handler.load(&data).is_err());
    }

    #[test]
    fn convert_effect_set_speed() {
        assert_eq!(convert_s3m_effect(20, 6), Effect::SetSpeed { speed: 6 });
    }

    #[test]
    fn convert_effect_position_jump() {
        assert_eq!(convert_s3m_effect(2, 5), Effect::PositionJump { order: 5 });
    }

    #[test]
    fn convert_effect_pattern_break() {
        assert_eq!(convert_s3m_effect(3, 0x13), Effect::PatternBreak { row: 13 });
    }

    #[test]
    fn convert_effect_volume_slide() {
        assert_eq!(
            convert_s3m_effect(4, 0x52),
            Effect::VolumeSlide { up: 5, down: 2 }
        );
    }

    #[test]
    fn convert_effect_tone_portamento() {
        assert_eq!(convert_s3m_effect(7, 10), Effect::TonePortamento { speed: 10 });
    }

    #[test]
    fn convert_effect_vibrato() {
        assert_eq!(
            convert_s3m_effect(8, 0x46),
            Effect::Vibrato { speed: 4, depth: 6 }
        );
    }

    #[test]
    fn convert_effect_arpeggio() {
        assert_eq!(
            convert_s3m_effect(10, 0x37),
            Effect::Arpeggio { note1: 3, note2: 7 }
        );
    }

    #[test]
    fn convert_effect_set_tempo() {
        assert_eq!(convert_s3m_effect(1, 140), Effect::SetTempo { bpm: 140 });
    }

    #[test]
    fn convert_effect_s_note_cut() {
        assert_eq!(convert_s3m_effect(19, 0xC3), Effect::NoteCutAfter { ticks: 3 });
    }

    #[test]
    fn convert_effect_s_note_delay() {
        assert_eq!(convert_s3m_effect(19, 0xD5), Effect::NoteDelay { ticks: 5 });
    }

    #[test]
    fn convert_effect_s_pattern_delay() {
        assert_eq!(convert_s3m_effect(19, 0xE2), Effect::PatternDelay { ticks: 2 });
    }

    #[test]
    fn convert_effect_retrigger() {
        assert_eq!(convert_s3m_effect(17, 4), Effect::Retrigger { interval: 4 });
    }

    #[test]
    fn convert_effect_vol_set_volume() {
        assert_eq!(convert_s3m_effect(13, 48), Effect::VolSetVolume { vol: 48 });
    }

    #[test]
    fn convert_effect_sample_offset() {
        assert_eq!(
            convert_s3m_effect(15, 0x20),
            Effect::FormatSpecific(FormatEffect::S3m(S3mEffect::SetSampleOffset(0x2000)))
        );
    }

    #[test]
    fn effect_to_s3m_roundtrip_speed() {
        let e = Effect::SetSpeed { speed: 8 };
        let (code, param) = effect_to_s3m(&e);
        assert_eq!(convert_s3m_effect(code, param), e);
    }

    #[test]
    fn effect_to_s3m_roundtrip_tempo() {
        let e = Effect::SetTempo { bpm: 130 };
        let (code, param) = effect_to_s3m(&e);
        assert_eq!(convert_s3m_effect(code, param), e);
    }

    #[test]
    fn effect_to_s3m_roundtrip_position_jump() {
        let e = Effect::PositionJump { order: 3 };
        let (code, param) = effect_to_s3m(&e);
        assert_eq!(convert_s3m_effect(code, param), e);
    }

    #[test]
    fn effect_to_s3m_roundtrip_tone_portamento() {
        let e = Effect::TonePortamento { speed: 12 };
        let (code, param) = effect_to_s3m(&e);
        assert_eq!(convert_s3m_effect(code, param), e);
    }

    #[test]
    fn effect_to_s3m_none() {
        assert_eq!(effect_to_s3m(&Effect::None), (0, 0));
    }

    #[test]
    fn pack_empty_pattern() {
        let p = Pattern::new(64);
        let packed = pack_pattern(&p);
        assert!(packed.len() >= 2);
        let len = u16::from_le_bytes([packed[0], packed[1]]) as usize;
        assert_eq!(len, 64);
        for i in 2..packed.len() {
            assert_eq!(packed[i], 0);
        }
    }

    #[test]
    fn pack_pattern_with_note() {
        let mut p = Pattern::new(64);
        p.data[0][0].note = Note::On(48);
        p.data[0][0].instrument = Some(1);
        let packed = pack_pattern(&p);
        assert!(packed.len() > 2);
        let len = u16::from_le_bytes([packed[0], packed[1]]) as usize;
        assert_eq!(packed.len(), 2 + len);
        assert!(len > 64);
    }

    #[test]
    fn pack_pattern_note_off() {
        let mut p = Pattern::new(64);
        p.data[0][0].note = Note::Off;
        let packed = pack_pattern(&p);
        assert!(packed.len() > 2);
        let mut found_ff = false;
        for &b in &packed[2..] {
            if b == 0xFF {
                found_ff = true;
                break;
            }
        }
        assert!(found_ff);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut module = Module::default();
        module.name = "RoundTrip".to_string();
        module.format = ModuleFormat::S3M;
        module.initial_speed = 6;
        module.initial_bpm = 125;
        module.order_list = vec![0];

        let mut pattern = Pattern::new(64);
        pattern.data[0][0].note = Note::On(48);
        pattern.data[0][0].instrument = Some(1);
        pattern.data[0][0].volume = Some(40);
        pattern.data[0][0].effect = Effect::SetSpeed { speed: 6 };
        module.patterns.push(pattern);

        module.samples.push(Sample::default());
        module.samples[0].name = "TestSample".to_string();
        module.samples[0].data = Arc::new(vec![0.0, 0.5, -0.5, 1.0]);
        module.samples[0].default_volume = 64;
        module.samples[0].loop_type = LoopType::None;

        module.instruments.push(Instrument {
            name: "TestInst".to_string(),
            sample_map: [1u8; 120],
            global_volume: 128,
            ..Instrument::default()
        });

        let saved = save_module(&module);

        assert!(saved.len() > S3M_HEADER_SIZE);
        assert_eq!(&saved[44..48], b"SCRM");

        let handler = S3mHandler;
        assert!(handler.detect(&saved));

        let loaded = handler.load(&saved).unwrap();
        assert_eq!(loaded.name, "RoundTrip");
        assert_eq!(loaded.format, ModuleFormat::S3M);
        assert_eq!(loaded.initial_speed, 6);
        assert_eq!(loaded.initial_bpm, 125);
        assert_eq!(loaded.order_list.len(), 1);
        assert_eq!(loaded.patterns.len(), 1);
        assert!(loaded.samples.len() >= 1);
    }

    #[test]
    fn save_module_empty() {
        let module = Module::default();
        let data = save_module(&module);
        assert!(data.len() >= S3M_HEADER_SIZE);
        assert_eq!(&data[44..48], b"SCRM");
    }

    #[test]
    fn load_s3m_with_sample_data() {
        let mut buf = vec![0u8; S3M_HEADER_SIZE];

        buf[0..7].copy_from_slice(b"TestS3M");
        buf[28] = 0x1A;
        buf[29] = 16;
        buf[32..34].copy_from_slice(&(1u16).to_le_bytes());
        buf[34..36].copy_from_slice(&(1u16).to_le_bytes());
        buf[36..38].copy_from_slice(&(1u16).to_le_bytes());
        buf[42..44].copy_from_slice(&(1u16).to_le_bytes());
        buf[44..48].copy_from_slice(b"SCRM");
        buf[48] = 64;
        buf[49] = 6;
        buf[50] = 125;
        buf[51] = 48;

        for i in 0..16 {
            buf[60 + i] = i as u8;
        }
        for i in 16..32 {
            buf[60 + i] = 0xFF;
        }

        let _order_offset = buf.len();
        buf.push(0);

        let sample_para_offset = buf.len();
        buf.extend_from_slice(&[0u8, 0]);
        let pattern_para_offset = buf.len();
        buf.extend_from_slice(&[0u8, 0]);

        let _pan_offset = buf.len();
        for _ in 0..32 {
            buf.push(0x80);
        }

        while buf.len() % 16 != 0 {
            buf.push(0);
        }

        let sample_hdr_para = (buf.len() / 16) as u16;
        buf[sample_para_offset..sample_para_offset + 2]
            .copy_from_slice(&sample_hdr_para.to_le_bytes());

        let sample_data = [0x00u8, 0x40, 0x80, 0xC0];

        let mut shdr = [0u8; S3M_SAMPLE_HEADER_SIZE];
        shdr[0] = 1;
        write_u32_le_to_slice(&mut shdr[16..20], sample_data.len() as u32);
        shdr[28] = 64;
        let c2speed: u32 = 8363;
        shdr[32..36].copy_from_slice(&c2speed.to_le_bytes());
        shdr[48..53].copy_from_slice(b"Samp\x00");
        shdr[76..80].copy_from_slice(b"SCRS");

        let shdr_end = buf.len() + S3M_SAMPLE_HEADER_SIZE;
        let data_para = ((shdr_end + 15) / 16) as u32;

        let hi = (data_para >> 16) as u8;
        let lo = (data_para & 0xFFFF) as u16;
        shdr[13] = hi;
        shdr[14..16].copy_from_slice(&lo.to_le_bytes());

        buf.extend_from_slice(&shdr);

        while buf.len() < data_para as usize * 16 {
            buf.push(0);
        }
        buf.extend_from_slice(&sample_data);

        while buf.len() % 16 != 0 {
            buf.push(0);
        }
        let pat_hdr_para = (buf.len() / 16) as u16;
        buf[pattern_para_offset..pattern_para_offset + 2]
            .copy_from_slice(&pat_hdr_para.to_le_bytes());

        let mut packed = Vec::new();
        packed.push(0);
        let mut pat_data = Vec::new();
        write_u16_le(&mut pat_data, packed.len() as u16);
        pat_data.extend_from_slice(&packed);
        buf.extend_from_slice(&pat_data);

        let handler = S3mHandler;
        let module = handler.load(&buf).unwrap();

        assert!(module.samples.len() >= 2);
        assert_eq!(module.samples[1].data.len(), 4);
        let expected: Vec<f32> = sample_data
            .iter()
            .map(|&b| (b as i8 as f32) / 128.0)
            .collect();
        for i in 0..4 {
            assert!(
                (module.samples[1].data[i] - expected[i]).abs() < 0.01,
                "sample[{}] = {} expected {}",
                i,
                module.samples[1].data[i],
                expected[i]
            );
        }
    }

    #[test]
    fn load_s3m_with_effect_in_pattern() {
        let mut buf = build_minimal_s3m();
        let pat_offset = 256;

        let mut packed = Vec::new();
        packed.push(0x20 | 0);
        packed.push(0x40);
        packed.push(1);
        packed.push(0x80);
        packed.push(1);
        packed.push(140);
        packed.push(0);

        let mut pat_data = Vec::new();
        write_u16_le(&mut pat_data, packed.len() as u16);
        pat_data.extend_from_slice(&packed);

        buf.truncate(pat_offset);
        buf.extend_from_slice(&pat_data);

        let handler = S3mHandler;
        let module = handler.load(&buf).unwrap();
        let cell = module.patterns[0].cell(0, 0);
        assert!(matches!(cell.note, Note::On(60)));
        assert_eq!(cell.instrument, Some(1));
        assert_eq!(cell.effect, Effect::SetTempo { bpm: 140 });
    }

    #[test]
    fn s3m_channel_panning_default() {
        let data = build_minimal_s3m();
        let handler = S3mHandler;
        let module = handler.load(&data).unwrap();
        assert_eq!(module.channel_panning[0], 0);
        assert_eq!(module.channel_panning[1], 64);
    }

    #[test]
    fn format_id_and_extension() {
        let handler = S3mHandler;
        assert_eq!(handler.format_id(), "S3M");
        assert_eq!(handler.file_extension(), "s3m");
    }
}
