use std::sync::Arc;

use crate::errors::{FormatError, FormatResult};
use crate::formats::common::*;
use crate::formats::FormatHandler;
use crate::sequencer::{
    effect::{FormatEffect, XmEffect},
    Cell, DuplicateCheckAction, DuplicateCheckType, Effect, Envelope, EnvelopeFlags,
    EnvelopePoint, Instrument, LoopType, Module, ModuleFlags, ModuleFormat,
    NewNoteAction, Note, Pattern, Sample, SampleFlags, VibratoWaveform,
    MAX_CHANNELS,
};

const XM_HEADER_MAGIC: &[u8; 17] = b"Extended Module: ";
const XM_FILE_TYPE_MARKER: u8 = 0x1A;
const XM_VERSION: u16 = 0x0104;
const XM_MAX_CHANNELS: usize = 32;
const XM_MAX_ENVELOPE_POINTS: usize = 12;

pub struct XmHandler;

impl FormatHandler for XmHandler {
    fn format_id(&self) -> &'static str {
        "XM"
    }

    fn file_extension(&self) -> &'static str {
        "xm"
    }

    fn detect(&self, data: &[u8]) -> bool {
        data.len() >= 17 && &data[0..17] == b"Extended Module: "
    }

    fn load(&self, data: &[u8]) -> FormatResult<Module> {
        if data.len() < 17 || &data[0..17] != XM_HEADER_MAGIC {
            return Err(FormatError::InvalidHeader {
                expected: "Extended Module: ",
                found: {
                    let mut arr = [0u8; 4];
                    if data.len() >= 4 {
                        arr.copy_from_slice(&data[0..4]);
                    }
                    arr
                },
            });
        }

        let mut offset = 17;
        let name = read_string(data, &mut offset, 20)?;
        let file_type = read_u8(data, &mut offset)?;
        if file_type != XM_FILE_TYPE_MARKER {
            return Err(FormatError::InvalidHeader {
                expected: "0x1A",
                found: [file_type, 0, 0, 0],
            });
        }

        let tracker_name = read_string(data, &mut offset, 20)?;
        let version = read_u16_le(data, &mut offset)?;
        let header_size = read_u32_le(data, &mut offset)? as usize;
        let song_length = read_u16_le(data, &mut offset)? as usize;
        let _restart_position = read_u16_le(data, &mut offset)?;
        let num_channels = read_u16_le(data, &mut offset)? as usize;
        let num_patterns = read_u16_le(data, &mut offset)? as usize;
        let num_instruments = read_u16_le(data, &mut offset)? as usize;
        let flags = read_u16_le(data, &mut offset)?;
        let default_tempo = read_u16_le(data, &mut offset)?;
        let default_bpm = read_u16_le(data, &mut offset)?;

        let order_list_len = song_length.min(256);
        let mut order_list = Vec::with_capacity(order_list_len);
        for _ in 0..order_list_len {
            order_list.push(read_u8(data, &mut offset)?);
        }

        let num_channels = num_channels.clamp(1, XM_MAX_CHANNELS);
        let linear_slides = (flags & 0x01) != 0;

        let patterns_base = 60 + header_size;
        let _patterns: Vec<Pattern> = (0..num_patterns)
            .map(|i| {
                let pat_offset = if i == 0 {
                    patterns_base
                } else {
                    0
                };
                if pat_offset == 0 && i > 0 {
                    Ok(Pattern::new(64))
                } else {
                    parse_xm_pattern(data, if i == 0 { patterns_base } else { 0 }, i, num_channels)
                }
            })
            .collect::<FormatResult<Vec<_>>>()?;

        let mut pos = patterns_base;
        let mut pattern_offsets = Vec::new();
        for _ in 0..num_patterns {
            if pos + 9 > data.len() {
                break;
            }
            let tmp = pos;
            let ph_len = u32::from_le_bytes([data[tmp], data[tmp + 1], data[tmp + 2], data[tmp + 3]]) as usize;
            let _packing = data[tmp + 4];
            let _rows = u16::from_le_bytes([data[tmp + 5], data[tmp + 6]]);
            let packed_size = u16::from_le_bytes([data[tmp + 7], data[tmp + 8]]) as usize;
            pattern_offsets.push(pos);
            pos += ph_len + packed_size;
        }

        let patterns: Vec<Pattern> = pattern_offsets
            .iter()
            .map(|&off| parse_xm_pattern_at(data, off, num_channels))
            .collect::<FormatResult<Vec<_>>>()?;

        let mut instruments = vec![Instrument::default()];
        let mut all_samples: Vec<Sample> = vec![Sample::default()];

        for _ in 0..num_instruments {
            if pos + 4 > data.len() {
                instruments.push(Instrument::default());
                continue;
            }
            let inst_start = pos;
            let inst_header_size = read_u32_le(data, &mut pos)? as usize;
            let inst_name = read_string(data, &mut pos, 22)?;
            let _inst_type = read_u8(data, &mut pos)?;
            let num_samples = read_u16_le(data, &mut pos)? as usize;

            let mut sample_map = [0u8; 120];
            if num_samples > 0 {
                if pos + 96 > data.len() {
                    instruments.push(Instrument {
                        name: inst_name,
                        ..Instrument::default()
                    });
                    continue;
                }
                let key_map = read_bytes(data, &mut pos, 96)?;
                for i in 0..96.min(120) {
                    sample_map[i] = key_map[i];
                }
                let _global_vol_inst = all_samples.len() as u8;
                for i in 0..96.min(120) {
                    if sample_map[i] as usize >= num_samples {
                        sample_map[i] = if num_samples > 0 { 0 } else { 0 };
                    }
                }

                let mut vol_points_raw = Vec::with_capacity(12);
                for _ in 0..12 {
                    let tick = read_u16_le(data, &mut pos)?;
                    let value = read_u16_le(data, &mut pos)?;
                    vol_points_raw.push(EnvelopePoint { tick, value: (value & 0xFF) as u8 });
                }

                let mut pan_points_raw = Vec::with_capacity(12);
                for _ in 0..12 {
                    let tick = read_u16_le(data, &mut pos)?;
                    let value = read_u16_le(data, &mut pos)?;
                    pan_points_raw.push(EnvelopePoint { tick, value: (value & 0xFF) as u8 });
                }

                let num_vol_points = read_u8(data, &mut pos)?;
                let num_pan_points = read_u8(data, &mut pos)?;
                let vol_sustain = read_u8(data, &mut pos)?;
                let vol_loop_start = read_u8(data, &mut pos)?;
                let vol_loop_end = read_u8(data, &mut pos)?;
                let pan_sustain = read_u8(data, &mut pos)?;
                let pan_loop_start = read_u8(data, &mut pos)?;
                let pan_loop_end = read_u8(data, &mut pos)?;
                let vol_env_type = read_u8(data, &mut pos)?;
                let pan_env_type = read_u8(data, &mut pos)?;
                let vibrato_type = read_u8(data, &mut pos)?;
                let vibrato_sweep = read_u8(data, &mut pos)?;
                let vibrato_depth = read_u8(data, &mut pos)?;
                let vibrato_rate = read_u8(data, &mut pos)?;
                let fade_out = read_u16_le(data, &mut pos)?;
                let _reserved = read_u16_le(data, &mut pos)?;

                let vol_points = (num_vol_points as usize).min(12).min(vol_points_raw.len());
                vol_points_raw.truncate(vol_points);
                let pan_points = (num_pan_points as usize).min(12).min(pan_points_raw.len());
                pan_points_raw.truncate(pan_points);

                let vol_env = if vol_env_type == 0 && vol_points_raw.is_empty() {
                    None
                } else {
                    Some(Envelope {
                        points: vol_points_raw,
                        sustain_point: if (vol_env_type & 0x02) != 0 && (vol_sustain as usize) < vol_points { Some(vol_sustain as usize) } else { None },
                        loop_start: if (vol_env_type & 0x04) != 0 && (vol_loop_start as usize) < vol_points { Some(vol_loop_start as usize) } else { None },
                        loop_end: if (vol_env_type & 0x04) != 0 && (vol_loop_end as usize) < vol_points { Some(vol_loop_end as usize) } else { None },
                        flags: EnvelopeFlags {
                            enabled: (vol_env_type & 0x01) != 0,
                            sustain: (vol_env_type & 0x02) != 0,
                            loop_: (vol_env_type & 0x04) != 0,
                            carry: (vol_env_type & 0x20) != 0,
                        },
                    })
                };

                let pan_env = if pan_env_type == 0 && pan_points_raw.is_empty() {
                    None
                } else {
                    Some(Envelope {
                        points: pan_points_raw,
                        sustain_point: if (pan_env_type & 0x02) != 0 && (pan_sustain as usize) < pan_points { Some(pan_sustain as usize) } else { None },
                        loop_start: if (pan_env_type & 0x04) != 0 && (pan_loop_start as usize) < pan_points { Some(pan_loop_start as usize) } else { None },
                        loop_end: if (pan_env_type & 0x04) != 0 && (pan_loop_end as usize) < pan_points { Some(pan_loop_end as usize) } else { None },
                        flags: EnvelopeFlags {
                            enabled: (pan_env_type & 0x01) != 0,
                            sustain: (pan_env_type & 0x02) != 0,
                            loop_: (pan_env_type & 0x04) != 0,
                            carry: (pan_env_type & 0x20) != 0,
                        },
                    })
                };

                let mut nna = NewNoteAction::NoteCut;
                let mut dct = DuplicateCheckType::Disabled;
                let mut dca = DuplicateCheckAction::NoteCut;
                let extended_offset = inst_start + 241;
                if inst_header_size >= extended_offset + 6 {
                    let mut ext_pos = extended_offset;
                    let nna_val = read_u16_le(data, &mut ext_pos)?;
                    let dct_val = read_u16_le(data, &mut ext_pos)?;
                    let dca_val = read_u16_le(data, &mut ext_pos)?;
                    nna = match nna_val {
                        1 => NewNoteAction::Continue,
                        2 => NewNoteAction::NoteOff,
                        3 => NewNoteAction::NoteFade,
                        _ => NewNoteAction::NoteCut,
                    };
                    dct = match dct_val {
                        1 => DuplicateCheckType::Note,
                        2 => DuplicateCheckType::Sample,
                        3 => DuplicateCheckType::Instrument,
                        _ => DuplicateCheckType::Disabled,
                    };
                    dca = match dca_val {
                        1 => DuplicateCheckAction::NoteOff,
                        2 => DuplicateCheckAction::NoteFade,
                        _ => DuplicateCheckAction::NoteCut,
                    };
                }

                pos = inst_start + inst_header_size;

                let mut sample_headers = Vec::with_capacity(num_samples);
                for _ in 0..num_samples {
                    if pos + 40 > data.len() {
                        sample_headers.push(XmSampleHeader::default());
                        continue;
                    }
                    let sh = read_xm_sample_header(data, &mut pos)?;
                    sample_headers.push(sh);
                }

                let sample_base_offset = all_samples.len();
                for (_si, sh) in sample_headers.iter().enumerate() {
                    let sample = decode_xm_sample(data, &mut pos, sh)?;
                    all_samples.push(sample);
                }

                for i in 0..96.min(120) {
                    sample_map[i] = (sample_base_offset + sample_map[i] as usize).min(255) as u8;
                }

                instruments.push(Instrument {
                    name: inst_name,
                    sample_map,
                    note_map: Instrument::default().note_map,
                    volume_envelope: vol_env,
                    panning_envelope: pan_env,
                    pitch_envelope: None,
                    filter_envelope: None,
                    fade_out,
                    nna,
                    duplicate_check_type: dct,
                    duplicate_check_action: dca,
                    pitch_pan_separation: 0,
                    pitch_pan_center: 60,
                    global_volume: 128,
                    filter_cutoff: 0xFFFF,
                    filter_resonance: 0,
                    filter_type: crate::sequencer::effect::FilterType::LowPass,
                    random_volume: 0,
                    random_panning: 0,
                    filter_random_cutoff: 0,
                    vib_type: vibrato_type,
                    vib_sweep: vibrato_sweep,
                    vib_depth: vibrato_depth,
                    vib_rate: vibrato_rate,
                });
            } else {
                instruments.push(Instrument {
                    name: inst_name,
                    sample_map,
                    ..Instrument::default()
                });
                pos = inst_start + inst_header_size;
            }
        }

        if instruments.len() == 1 && all_samples.len() > 1 {
            let mut inst = Instrument::default();
            for i in 0..96.min(120) {
                if i + 1 < all_samples.len() {
                    inst.sample_map[i] = (i + 1) as u8;
                }
            }
            instruments.push(inst);
        }

        let mut channel_panning = vec![32u8; MAX_CHANNELS];
        let mut channel_volume = vec![64u8; MAX_CHANNELS];
        for i in 0..num_channels.min(MAX_CHANNELS) {
            channel_panning[i] = 128;
            channel_volume[i] = 64;
        }

        let module_flags = ModuleFlags {
            stereo: true,
            use_instruments: true,
            linear_slides,
            old_effects: false,
            compatible_gxx: false,
            midi_enabled: false,
            request_embed: false,
            fast_volume_slides: false,
            xm_envelope_model: true,
            xm_period_model: true,
        };

        Ok(Module {
            name,
            message: None,
            format: ModuleFormat::XM,
            _version: version,
            tracker_name,
            order_list,
            patterns,
            instruments,
            samples: all_samples,
            initial_bpm: default_bpm,
            initial_speed: default_tempo as u8,
            initial_global_volume: 64,
            initial_mixing_volume: 48,
            channel_panning,
            channel_volume,
            flags: module_flags,
        })
    }
}

#[derive(Default)]
struct XmSampleHeader {
    length: u32,
    loop_start: u32,
    loop_length: u32,
    volume: u8,
    finetune: i8,
    type_: u8,
    panning: u8,
    relative_note: i8,
    _reserved: u8,
    name: String,
}

fn read_xm_sample_header(data: &[u8], pos: &mut usize) -> FormatResult<XmSampleHeader> {
    let length = read_u32_le(data, pos)?;
    let loop_start = read_u32_le(data, pos)?;
    let loop_length = read_u32_le(data, pos)?;
    let volume = read_u8(data, pos)?;
    let finetune = read_u8(data, pos)? as i8;
    let type_ = read_u8(data, pos)?;
    let panning = read_u8(data, pos)?;
    let relative_note = read_u8(data, pos)? as i8;
    let _reserved = read_u8(data, pos)?;
    let name = read_string(data, pos, 22)?;
    Ok(XmSampleHeader {
        length,
        loop_start,
        loop_length,
        volume,
        finetune,
        type_,
        panning,
        relative_note,
        _reserved,
        name,
    })
}

fn decode_xm_sample(data: &[u8], pos: &mut usize, sh: &XmSampleHeader) -> FormatResult<Sample> {
    let is_16bit = (sh.type_ & 0x10) != 0;
    let loop_type_val = sh.type_ & 0x03;

    let loop_type = match loop_type_val {
        1 => LoopType::Forward,
        2 => LoopType::PingPong,
        _ => LoopType::None,
    };

    let sample_data_bytes = if is_16bit {
        sh.length as usize * 2
    } else {
        sh.length as usize
    };

    let sample_data = if sample_data_bytes > 0 && *pos + sample_data_bytes <= data.len() {
        let raw = &data[*pos..*pos + sample_data_bytes];
        *pos += sample_data_bytes;

        let samples = if is_16bit {
            decode_delta_16bit(raw)
        } else {
            decode_delta_8bit(raw)
        };
        Arc::new(samples)
    } else {
        if sample_data_bytes > 0 {
            *pos += sample_data_bytes;
        }
        Arc::new(Vec::new())
    };

    let loop_end = if loop_type != LoopType::None {
        (sh.loop_start + sh.loop_length) as usize
    } else {
        0
    };

    let sample_rate = 8363;

    Ok(Sample {
        name: sh.name.clone(),
        data: sample_data,
        sample_rate,
        bits_per_sample: if is_16bit { 16 } else { 8 },
        loop_type,
        loop_start: sh.loop_start as usize,
        loop_end,
        default_volume: sh.volume.min(64),
        default_panning: sh.panning,
        global_volume: 64,
        relative_note: sh.relative_note,
        fine_tune: sh.finetune,
        vibrato_speed: 0,
        vibrato_depth: 0,
        vibrato_rate: 0,
        vibrato_waveform: VibratoWaveform::Sine,
        _flags: SampleFlags {
            is_stereo: false,
            is_16bit,
            is_compressed: false,
            has_trailing_byte: false,
        },
    })
}

fn decode_delta_8bit(raw: &[u8]) -> Vec<f32> {
    let mut result = Vec::with_capacity(raw.len());
    let mut acc: u8 = 0;
    for &b in raw {
        acc = acc.wrapping_add(b);
        result.push((acc as i8) as f32 / 128.0);
    }
    result
}

fn decode_delta_16bit(raw: &[u8]) -> Vec<f32> {
    let num_samples = raw.len() / 2;
    let mut result = Vec::with_capacity(num_samples);
    let mut acc: u16 = 0;
    for chunk in raw.chunks_exact(2) {
        let delta = i16::from_le_bytes([chunk[0], chunk[1]]);
        acc = acc.wrapping_add(delta as u16);
        result.push((acc as i16) as f32 / 32768.0);
    }
    result
}

#[allow(dead_code)]
fn parse_xm_envelope(
    data: &[u8],
    pos: &mut usize,
    is_volume: bool,
) -> FormatResult<Option<Envelope>> {
    let num_points_to_read = XM_MAX_ENVELOPE_POINTS;

    let mut points = Vec::with_capacity(num_points_to_read);
    for _ in 0..num_points_to_read {
        if *pos + 4 > data.len() {
            break;
        }
        let tick = read_u16_le(data, pos)?;
        let value = read_u16_le(data, pos)?;
        points.push(EnvelopePoint {
            tick,
            value: (value & 0xFF) as u8,
        });
    }

    let num_vol_points = read_u8(data, pos)?;
    let num_pan_points = read_u8(data, pos)?;
    let vol_sustain = read_u8(data, pos)?;
    let vol_loop_start = read_u8(data, pos)?;
    let vol_loop_end = read_u8(data, pos)?;
    let pan_sustain = read_u8(data, pos)?;
    let pan_loop_start = read_u8(data, pos)?;
    let pan_loop_end = read_u8(data, pos)?;
    let vol_env_type = read_u8(data, pos)?;
    let pan_env_type = read_u8(data, pos)?;

    let _vib_type = read_u8(data, pos)?;
    let _vib_sweep = read_u8(data, pos)?;
    let _vib_depth = read_u8(data, pos)?;
    let _vib_rate = read_u8(data, pos)?;
    let _fade_out = read_u16_le(data, pos)?;
    let _reserved2 = read_u16_le(data, pos)?;

    let (env_type_byte, num_points, sustain, loop_start, loop_end) = if is_volume {
        (vol_env_type, num_vol_points, vol_sustain, vol_loop_start, vol_loop_end)
    } else {
        (pan_env_type, num_pan_points, pan_sustain, pan_loop_start, pan_loop_end)
    };

    let actual_points = (num_points as usize).min(XM_MAX_ENVELOPE_POINTS).min(points.len());
    points.truncate(actual_points);

    if env_type_byte == 0 && points.is_empty() {
        return Ok(None);
    }

    Ok(Some(Envelope {
        points,
        sustain_point: if (env_type_byte & 0x02) != 0 && (sustain as usize) < actual_points {
            Some(sustain as usize)
        } else {
            None
        },
        loop_start: if (env_type_byte & 0x04) != 0 && (loop_start as usize) < actual_points {
            Some(loop_start as usize)
        } else {
            None
        },
        loop_end: if (env_type_byte & 0x04) != 0 && (loop_end as usize) < actual_points {
            Some(loop_end as usize)
        } else {
            None
        },
        flags: EnvelopeFlags {
            enabled: (env_type_byte & 0x01) != 0,
            sustain: (env_type_byte & 0x02) != 0,
            loop_: (env_type_byte & 0x04) != 0,
            carry: false,
        },
    }))
}

fn parse_xm_pattern_at(data: &[u8], offset: usize, num_channels: usize) -> FormatResult<Pattern> {
    let mut pos = offset;

    if pos + 9 > data.len() {
        return Ok(Pattern::new(64));
    }

    let _pattern_header_length = read_u32_le(data, &mut pos)?;
    let _packing_type = read_u8(data, &mut pos)?;
    let num_rows = read_u16_le(data, &mut pos)? as usize;
    let packed_data_size = read_u16_le(data, &mut pos)? as usize;

    let num_rows = if num_rows == 0 { 64 } else { num_rows.min(1024) };

    let mut pattern = Pattern::new(num_rows);

    if packed_data_size == 0 {
        return Ok(pattern);
    }

    let packed_start = pos;
    let packed_end = packed_start + packed_data_size;

    for row in 0..num_rows {
        if pos >= packed_end {
            break;
        }
        for ch in 0..num_channels {
            if pos >= packed_end {
                break;
            }
            if ch >= MAX_CHANNELS {
                let b = data[pos];
                if b & 0x80 != 0 {
                    pos += 1;
                    if b & 0x01 != 0 { pos += 1; }
                    if b & 0x02 != 0 { pos += 1; }
                    if b & 0x04 != 0 { pos += 1; }
                    if b & 0x08 != 0 { pos += 1; }
                    if b & 0x10 != 0 { pos += 1; }
                } else {
                    pos += 5;
                }
                continue;
            }

            let cell = decode_xm_cell(data, &mut pos, packed_end);
            pattern.data[row][ch] = cell;
        }
    }

    Ok(pattern)
}

fn parse_xm_pattern(
    _data: &[u8],
    _offset: usize,
    _index: usize,
    _num_channels: usize,
) -> FormatResult<Pattern> {
    Ok(Pattern::new(64))
}

fn decode_xm_cell(data: &[u8], pos: &mut usize, end: usize) -> Cell {
    if *pos >= end {
        return Cell::default();
    }

    let first = data[*pos];
    let mut note = 0u8;
    let mut instrument = 0u8;
    let mut volume = 0u8;
    let mut effect_type = 0u8;
    let mut effect_param = 0u8;
    let mut has_note = false;
    let mut has_inst = false;
    let mut has_vol = false;
    let mut has_fx = false;
    let mut has_fx_param = false;

    if first & 0x80 != 0 {
        *pos += 1;
        if first & 0x01 != 0 {
            if *pos >= end { return Cell::default(); }
            note = data[*pos];
            *pos += 1;
            has_note = true;
        }
        if first & 0x02 != 0 {
            if *pos >= end { return Cell::default(); }
            instrument = data[*pos];
            *pos += 1;
            has_inst = true;
        }
        if first & 0x04 != 0 {
            if *pos >= end { return Cell::default(); }
            volume = data[*pos];
            *pos += 1;
            has_vol = true;
        }
        if first & 0x08 != 0 {
            if *pos >= end { return Cell::default(); }
            effect_type = data[*pos];
            *pos += 1;
            has_fx = true;
        }
        if first & 0x10 != 0 {
            if *pos >= end { return Cell::default(); }
            effect_param = data[*pos];
            *pos += 1;
            has_fx_param = true;
        }
    } else {
        note = first;
        *pos += 1;
        has_note = true;
        if *pos < end { instrument = data[*pos]; *pos += 1; has_inst = true; }
        if *pos < end { volume = data[*pos]; *pos += 1; has_vol = true; }
        if *pos < end { effect_type = data[*pos]; *pos += 1; has_fx = true; }
        if *pos < end { effect_param = data[*pos]; *pos += 1; has_fx_param = true; }
    }

    let decoded_note = if has_note {
        decode_xm_note(note)
    } else {
        Note::None
    };

    let decoded_inst = if has_inst && instrument > 0 {
        Some(instrument)
    } else {
        None
    };

    let (decoded_vol, vol_column_effect) = if has_vol {
        if volume >= 0x10 && volume <= 0x50 {
            (Some(volume - 0x10), None)
        } else if volume > 0 {
            let eff = decode_xm_volume_column(volume);
            // Store raw byte in volume (for vol_kol backward compat in XM path)
            // and decoded effect in volume_effect (for unified sequencer)
            (Some(volume), if eff != Effect::None { Some(eff) } else { None })
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let decoded_effect = if has_fx {
        decode_xm_effect(effect_type, effect_param, has_fx_param)
    } else {
        Effect::None
    };

    Cell {
        note: decoded_note,
        instrument: decoded_inst,
        volume: decoded_vol,
        volume_effect: vol_column_effect,
        effect: decoded_effect,
    }
}

fn decode_xm_note(raw: u8) -> Note {
    match raw {
        0 => Note::None,
        97 => Note::Off,
        n if n >= 1 && n <= 96 => Note::On(n - 1),
        _ => Note::None,
    }
}

fn decode_xm_volume_column(vol: u8) -> Effect {
    if vol == 0 {
        return Effect::None;
    }

    if vol >= 0x10 && vol <= 0x50 {
        return Effect::VolSetVolume { vol: vol - 0x10 };
    }
    if vol >= 0x60 && vol <= 0x6F {
        return Effect::VolSlideDown { amount: vol - 0x60 };
    }
    if vol >= 0x70 && vol <= 0x7F {
        return Effect::VolSlideUp { amount: vol - 0x70 };
    }
    if vol >= 0x80 && vol <= 0x8F {
        return Effect::VolFineSlideDown { amount: vol - 0x80 };
    }
    if vol >= 0x90 && vol <= 0x9F {
        return Effect::VolFineSlideUp { amount: vol - 0x90 };
    }
    if vol >= 0xA0 && vol <= 0xAF {
        return Effect::Vibrato { speed: vol - 0xA0, depth: 0 };
    }
    if vol >= 0xB0 && vol <= 0xBF {
        return Effect::Vibrato { speed: 0, depth: vol - 0xB0 };
    }
    if vol >= 0xC0 && vol <= 0xCF {
        let pan_val = ((vol - 0xC0) as u16 * 255 / 15) as u8;
        return Effect::SetPanning { pan: pan_val };
    }
    if vol >= 0xD0 && vol <= 0xEF {
        // 0xD0-0xDF = pan slide left, 0xE0-0xEF = pan slide right
        // No Effect variant exists for continuous pan slides;
        // handled via vol_kol in the XM sequencer path
        return Effect::None;
    }
    if vol >= 0xF0 {
        return Effect::TonePortamento { speed: vol - 0xF0 };
    }

    Effect::None
}

fn decode_xm_effect(fx: u8, param: u8, has_param: bool) -> Effect {
    let param = if has_param { param } else { 0 };
    match fx {
        0 => {
            if param == 0 {
                Effect::None
            } else {
                Effect::Arpeggio {
                    note1: param >> 4,
                    note2: param & 0x0F,
                }
            }
        }
        1 => Effect::PortamentoUp { speed: param },
        2 => Effect::PortamentoDown { speed: param },
        3 => Effect::TonePortamento { speed: param },
        4 => Effect::Vibrato {
            speed: param >> 4,
            depth: param & 0x0F,
        },
        5 => Effect::TonePortamentoVolumeSlide { up: param as i8 },
        6 => Effect::VibratoVolumeSlide { up: param as i8 },
        7 => Effect::Tremolo {
            speed: param >> 4,
            depth: param & 0x0F,
        },
        8 => Effect::SetPanning { pan: param },
        9 => Effect::FormatSpecific(FormatEffect::Xm(XmEffect::SetSampleOffset(
            (param as u16) << 8,
        ))),
        0xA => Effect::VolumeSlide {
            up: param >> 4,
            down: param & 0x0F,
        },
        0xB => Effect::PositionJump { order: param },
        0xC => Effect::SetVolume {
            volume: param.min(64),
        },
        0xD => Effect::PatternBreak {
            row: ((param >> 4) * 10) + (param & 0x0F),
        },
        0xE => decode_xm_extended_effect(param),
        0xF => {
            if param < 32 {
                Effect::SetSpeed { speed: param }
            } else {
                Effect::SetTempo { bpm: param }
            }
        }
        0x10 => Effect::SetGlobalVolume { volume: param },
        0x11 => Effect::GlobalVolumeSlide {
            up: (param >> 4) as i8,
            down: -((param & 0x0F) as i8),
        },
        0x14 => Effect::NoteCutAfter { ticks: param },
        0x15 => Effect::NoteDelay { ticks: param },
        0x12 => Effect::FormatSpecific(FormatEffect::Xm(XmEffect::KeyOff { fade_rate: param })),
        0x19 => Effect::Panbrello {
            speed: param >> 4,
            depth: param & 0x0F,
        },
        0x1D => Effect::Tremor {
            ontime: param >> 4,
            offtime: param & 0x0F,
        },
        _ if fx > 0 => Effect::FormatSpecific(FormatEffect::Xm(XmEffect::Raw { effect: fx, param })),
        _ => Effect::None,
    }
}

fn decode_xm_extended_effect(param: u8) -> Effect {
    let sub = param >> 4;
    let val = param & 0x0F;
    match sub {
        0x1 => Effect::PortamentoUp { speed: val << 4 },
        0x2 => Effect::PortamentoDown { speed: val << 4 },
        0x3 => Effect::GlissandoControl { on: val != 0 },
        0x4 => Effect::VibratoWaveform { waveform: val },
        0x5 => Effect::SetFineTune { tune: val },
        0x6 => Effect::PatternLoop { count: val },
        0x7 => Effect::TremoloWaveform { waveform: val },
        0x8 => Effect::SetPanning16 { pan: val << 4 },
        0x9 => Effect::Retrigger { interval: val },
        0xA => Effect::NoteCutAfter { ticks: val },
        0xB => Effect::NoteDelay { ticks: val },
        0xC => Effect::NoteCutAfter { ticks: val },
        0xD => Effect::NoteDelay { ticks: val },
        0xE => Effect::PatternDelay { ticks: val },
        0xF => Effect::ExtendedEffect { param },
        _ if sub > 0 => Effect::FormatSpecific(FormatEffect::Xm(XmEffect::Raw { effect: 0xE0 | sub, param: val })),
        _ => Effect::None,
    }
}

pub fn save_module(module: &Module) -> Vec<u8> {
    let mut out = Vec::new();

    let num_channels = XM_MAX_CHANNELS.min(
        module.patterns.iter().map(|p| {
            let mut max_ch = 1;
            for row in &p.data {
                for (ch, cell) in row.iter().enumerate() {
                    if !cell.is_empty() && ch + 1 > max_ch {
                        max_ch = ch + 1;
                    }
                }
            }
            max_ch
        }).max().unwrap_or(1)
    );

    let name_bytes = pad_string(&module.name, 20);
    let tracker_bytes = pad_string(&module.tracker_name, 20);

    out.extend_from_slice(XM_HEADER_MAGIC);
    out.extend_from_slice(&name_bytes);
    out.push(XM_FILE_TYPE_MARKER);
    out.extend_from_slice(&tracker_bytes);
    out.extend_from_slice(&XM_VERSION.to_le_bytes());

    let song_length = module.order_list.len().min(256) as u16;
    let num_patterns = module.patterns.len() as u16;

    let xm_instruments = build_xm_instruments(module);

    let num_instruments = xm_instruments.len() as u16;

    let _header_data_start = out.len() + 4;
    let order_list_bytes = song_length as usize;
    let header_size = (4 + 2 + 2 + 2 + 2 + 2 + 2 + 2 + 2 + order_list_bytes) as u32;

    out.extend_from_slice(&header_size.to_le_bytes());
    out.extend_from_slice(&song_length.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(num_channels as u16).to_le_bytes());
    out.extend_from_slice(&num_patterns.to_le_bytes());
    out.extend_from_slice(&num_instruments.to_le_bytes());

    let flags: u16 = if module.flags.linear_slides { 0x01 } else { 0x00 };
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&(module.initial_speed as u16).to_le_bytes());
    out.extend_from_slice(&module.initial_bpm.to_le_bytes());

    for i in 0..song_length as usize {
        if i < module.order_list.len() {
            out.push(module.order_list[i]);
        } else {
            out.push(0);
        }
    }

    for pattern in &module.patterns {
        write_xm_pattern(&mut out, pattern, num_channels);
    }

    for xm_inst in &xm_instruments {
        write_xm_instrument(&mut out, xm_inst, module);
    }

    out
}

struct XmInstrumentInfo {
    instrument_index: usize,
    sample_indices: Vec<usize>,
}

fn build_xm_instruments(module: &Module) -> Vec<XmInstrumentInfo> {
    if module.instruments.is_empty() {
        let mut sample_indices: Vec<usize> = (0..module.samples.len()).collect();
        if sample_indices.is_empty() {
            sample_indices.push(0);
        }
        return vec![XmInstrumentInfo {
            instrument_index: 0,
            sample_indices: sample_indices,
        }];
    }

    module
        .instruments
        .iter()
        .enumerate()
        .map(|(i, inst)| {
            let mut sample_set = std::collections::HashSet::new();
            for &si in &inst.sample_map {
                if (si as usize) < module.samples.len() && si > 0 {
                    sample_set.insert(si as usize);
                }
            }
            let mut sample_indices: Vec<usize> = sample_set.into_iter().collect();
            sample_indices.sort();
            if sample_indices.is_empty() {
                if !module.samples.is_empty() {
                    sample_indices.push(0);
                }
            }
            XmInstrumentInfo {
                instrument_index: i,
                sample_indices,
            }
        })
        .collect()
}

fn write_xm_pattern(out: &mut Vec<u8>, pattern: &Pattern, num_channels: usize) {
    let num_rows = pattern.num_rows.min(1024);
    let nc = num_channels.min(XM_MAX_CHANNELS);

    let mut packed = Vec::new();
    for row in 0..num_rows {
        for ch in 0..nc {
            let cell = if row < pattern.data.len() && ch < MAX_CHANNELS {
                &pattern.data[row][ch]
            } else {
                &Cell::default()
            };
            encode_xm_cell(&mut packed, cell);
        }
    }

    let header_length: u32 = 9;
    out.extend_from_slice(&header_length.to_le_bytes());
    out.push(0);
    out.extend_from_slice(&(num_rows as u16).to_le_bytes());
    out.extend_from_slice(&(packed.len() as u16).to_le_bytes());
    out.extend_from_slice(&packed);
}

fn encode_xm_cell(out: &mut Vec<u8>, cell: &Cell) {
    let note = encode_xm_note(cell.note);
    let inst = cell.instrument.unwrap_or(0);
    let vol = if let Some(v) = cell.volume {
        v.min(64).saturating_add(0x10)
    } else {
        encode_xm_volume_column(&cell.effect)
    };
    let (fx, fx_param) = encode_xm_effect(&cell.effect);

    let need_note = note != 0;
    let need_inst = inst != 0;
    let need_vol = vol != 0;
    let need_fx = fx != 0 || fx_param != 0;

    if !need_note && !need_inst && !need_vol && !need_fx {
        out.push(0x80);
        return;
    }

    let mut mask = 0x80u8;
    if need_note { mask |= 0x01; }
    if need_inst { mask |= 0x02; }
    if need_vol { mask |= 0x04; }
    if need_fx { mask |= 0x08 | 0x10; }

    out.push(mask);
    if need_note { out.push(note); }
    if need_inst { out.push(inst); }
    if need_vol { out.push(vol); }
    if need_fx {
        out.push(fx);
        out.push(fx_param);
    }
}

fn encode_xm_note(note: Note) -> u8 {
    match note {
        Note::None => 0,
        Note::Off => 97,
        Note::On(key) => {
            if key < 96 {
                key + 1
            } else {
                0
            }
        }
        Note::Cut => 97,
        Note::Fade => 0,
    }
}

fn encode_xm_volume_column(effect: &Effect) -> u8 {
    match effect {
        Effect::VolSetVolume { vol } => (*vol).min(64).saturating_add(0x10) as u8,
        Effect::VolSlideUp { amount } => {
            if *amount <= 15 { 0x70 + *amount } else { 0 }
        }
        Effect::VolSlideDown { amount } => {
            if *amount <= 15 { 0x60 + *amount } else { 0 }
        }
        Effect::VolFineSlideUp { amount } => {
            if *amount <= 15 { 0x90 + *amount } else { 0 }
        }
        Effect::VolFineSlideDown { amount } => {
            if *amount <= 15 { 0x80 + *amount } else { 0 }
        }
        Effect::TonePortamento { speed } => {
            if *speed <= 15 { 0xD0 + *speed } else { 0 }
        }
        Effect::Vibrato { speed, depth } => {
            if *speed > 0 && *depth == 0 { 0xA0 + *speed.min(&15) }
            else if *depth > 0 && *speed == 0 { 0xB0 + *depth.min(&15) }
            else { 0 }
        }
        Effect::SetPanning { pan } => {
            let v = (*pan as u16 * 15 / 255) as u8;
            0xC0 + v.min(15)
        }
        _ => 0,
    }
}

fn encode_xm_effect(effect: &Effect) -> (u8, u8) {
    match effect {
        Effect::None => (0, 0),
        Effect::Arpeggio { note1, note2 } => (0, (note1 << 4) | note2),
        Effect::PortamentoUp { speed } => (1, *speed),
        Effect::PortamentoDown { speed } => (2, *speed),
        Effect::TonePortamento { speed } => (3, *speed),
        Effect::Vibrato { speed, depth } => (4, (speed << 4) | depth),
        Effect::TonePortamentoVolumeSlide { up } => (5, *up as u8),
        Effect::VibratoVolumeSlide { up } => (6, *up as u8),
        Effect::Tremolo { speed, depth } => (7, (speed << 4) | depth),
        Effect::SetPanning { pan } => (8, *pan),
        Effect::SetSampleOffset { offset } => (9, (offset >> 8) as u8),
        Effect::VolumeSlide { up, down } => (0xA, (up << 4) | down),
        Effect::PositionJump { order } => (0xB, *order),
        Effect::SetVolume { volume } => (0xC, (*volume).min(64)),
        Effect::PatternBreak { row } => {
            let tens = row / 10;
            let ones = row % 10;
            (0xD, (tens << 4) | ones)
        }
        Effect::SetSpeed { speed } => (0xF, *speed),
        Effect::SetTempo { bpm } => (0xF, *bpm),
        Effect::SetGlobalVolume { volume } => (0x10, *volume),
        Effect::GlobalVolumeSlide { up, down } => {
            let u = (*up).max(0) as u8;
            let d = (*down).unsigned_abs().min(15) as u8;
            (0x11, (u << 4) | d)
        }
        Effect::NoteCutAfter { ticks } => (0x14, *ticks),
        Effect::NoteDelay { ticks } => (0x15, *ticks),
        Effect::Panbrello { speed, depth } => (0x19, (speed << 4) | depth),
        Effect::FinePortamentoUp { speed } => (0xE, 0x10 | (speed >> 4).min(0x0F) as u8),
        Effect::FinePortamentoDown { speed } => (0xE, 0x20 | (speed >> 4).min(0x0F) as u8),
        Effect::GlissandoControl { on } => (0xE, if *on { 0x31 } else { 0x30 }),
        Effect::VibratoWaveform { waveform } => (0xE, 0x40 | (waveform & 0x03)),
        Effect::SetFineTune { tune } => (0xE, 0x50 | (tune & 0x0F)),
        Effect::PatternLoop { count } => (0xE, 0x60 | (count & 0x0F)),
        Effect::TremoloWaveform { waveform } => (0xE, 0x70 | (waveform & 0x03)),
        Effect::SetPanning16 { pan } => (0xE, 0x80 | (pan >> 4)),
        Effect::Retrigger { interval } => (0xE, 0x90 | (interval & 0x0F)),
        Effect::PatternDelay { ticks } => (0xE, 0xE0 | (ticks & 0x0F)),
        Effect::FineVolumeSlideUp { amount } => (0xE, 0xA0 | (amount & 0x0F)),
        Effect::FineVolumeSlideDown { amount } => (0xE, 0xB0 | (amount & 0x0F)),
        Effect::ExtendedEffect { param } => (0xE, *param),
        Effect::VolSetVolume { vol: _ } => (0, 0),
        Effect::VolSlideUp { amount: _ } => (0, 0),
        Effect::VolSlideDown { amount: _ } => (0, 0),
        Effect::VolFineSlideUp { amount: _ } => (0, 0),
        Effect::VolFineSlideDown { amount: _ } => (0, 0),
        Effect::VolPortamento { speed: _ } => (0, 0),
        Effect::VolVibrato { speed: _ } => (0, 0),
        Effect::Tremor { ontime, offtime } => (0x1D, (ontime << 4) | (offtime & 0x0F)),
        Effect::FormatSpecific(FormatEffect::Xm(XmEffect::Raw { effect, param })) => {
            if *effect >= 0xE0 {
                (0x0E, (*effect << 4) as u8 | (param & 0x0F))
            } else {
                (*effect, *param)
            }
        }
        Effect::FormatSpecific(FormatEffect::Xm(XmEffect::KeyOff { fade_rate })) => (0x12, *fade_rate),
        Effect::FormatSpecific(FormatEffect::Xm(XmEffect::SetSampleOffset(offset))) => (9, (offset >> 8) as u8),
        Effect::FormatSpecific(_) => (0, 0),
        _ => (0, 0),
    }
}

fn write_xm_instrument(out: &mut Vec<u8>, xm_inst: &XmInstrumentInfo, module: &Module) {
    let inst = if xm_inst.instrument_index < module.instruments.len() {
        &module.instruments[xm_inst.instrument_index]
    } else {
        &Instrument::default()
    };

    let num_samples = if xm_inst.sample_indices.is_empty() { 0 } else { xm_inst.sample_indices.len() } as u16;

    let name_bytes = pad_string(&inst.name, 22);

    let header_data_size = 4 + 22 + 1 + 2 +
        if num_samples > 0 { 96 + 48 + 48 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 2 + 2 } else { 0 };
    let header_size = header_data_size as u32;

    out.extend_from_slice(&header_size.to_le_bytes());
    out.extend_from_slice(&name_bytes);
    out.push(0);
    out.extend_from_slice(&num_samples.to_le_bytes());

    if num_samples > 0 {
        let mut key_map = [0u8; 96];
        for note in 0..96 {
            if note < 120 {
                let si = inst.sample_map[note] as usize;
                if let Some(local_idx) = xm_inst.sample_indices.iter().position(|&x| x == si) {
                    key_map[note] = local_idx as u8;
                }
            }
        }
        out.extend_from_slice(&key_map);

        write_xm_envelope_points(out, &inst.volume_envelope);
        write_xm_envelope_points(out, &inst.panning_envelope);

        let vol_points = inst.volume_envelope.as_ref().map(|e| e.points.len()).unwrap_or(0);
        let pan_points = inst.panning_envelope.as_ref().map(|e| e.points.len()).unwrap_or(0);
        let vol_sustain = inst.volume_envelope.as_ref().and_then(|e| e.sustain_point).unwrap_or(0);
        let vol_loop_start = inst.volume_envelope.as_ref().and_then(|e| e.loop_start).unwrap_or(0);
        let vol_loop_end = inst.volume_envelope.as_ref().and_then(|e| e.loop_end).unwrap_or(0);
        let pan_sustain = inst.panning_envelope.as_ref().and_then(|e| e.sustain_point).unwrap_or(0);
        let pan_loop_start = inst.panning_envelope.as_ref().and_then(|e| e.loop_start).unwrap_or(0);
        let pan_loop_end = inst.panning_envelope.as_ref().and_then(|e| e.loop_end).unwrap_or(0);

        let vol_env_type = encode_envelope_flags(&inst.volume_envelope);
        let pan_env_type = encode_envelope_flags(&inst.panning_envelope);

        out.push(vol_points.min(XM_MAX_ENVELOPE_POINTS) as u8);
        out.push(pan_points.min(XM_MAX_ENVELOPE_POINTS) as u8);
        out.push(vol_sustain as u8);
        out.push(vol_loop_start as u8);
        out.push(vol_loop_end as u8);
        out.push(pan_sustain as u8);
        out.push(pan_loop_start as u8);
        out.push(pan_loop_end as u8);
        out.push(vol_env_type);
        out.push(pan_env_type);

        out.push(inst.vib_type);
        out.push(inst.vib_sweep);
        out.push(inst.vib_depth);
        out.push(inst.vib_rate);
        out.extend_from_slice(&inst.fade_out.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());

        for &sample_idx in &xm_inst.sample_indices {
            let sample = if sample_idx < module.samples.len() {
                &module.samples[sample_idx]
            } else {
                &Sample::default()
            };
            write_xm_sample_header(out, sample);
        }

        for &sample_idx in &xm_inst.sample_indices {
            let sample = if sample_idx < module.samples.len() {
                &module.samples[sample_idx]
            } else {
                &Sample::default()
            };
            write_xm_sample_data(out, sample);
        }
    }
}

fn write_xm_envelope_points(out: &mut Vec<u8>, envelope: &Option<Envelope>) {
    let points = envelope.as_ref().map(|e| &e.points);
    for i in 0..XM_MAX_ENVELOPE_POINTS {
        if let Some(pts) = points {
            if i < pts.len() {
                out.extend_from_slice(&pts[i].tick.to_le_bytes());
                out.extend_from_slice(&(pts[i].value as u16).to_le_bytes());
            } else {
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
            }
        } else {
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
        }
    }
}

fn encode_envelope_flags(envelope: &Option<Envelope>) -> u8 {
    if let Some(env) = envelope {
        let mut flags = 0u8;
        if env.flags.enabled { flags |= 0x01; }
        if env.flags.sustain { flags |= 0x02; }
        if env.flags.loop_ { flags |= 0x04; }
        flags
    } else {
        0
    }
}

fn write_xm_sample_header(out: &mut Vec<u8>, sample: &Sample) {
    let is_16bit = sample.bits_per_sample >= 16;
    let sample_byte_length = if is_16bit {
        sample.data.len() as u32 * 2
    } else {
        sample.data.len() as u32
    };

    let loop_type_bits: u8 = match sample.loop_type {
        LoopType::None => 0,
        LoopType::Forward => 1,
        LoopType::PingPong => 2,
        LoopType::Backward => 1,
    };
    let type_byte = loop_type_bits | if is_16bit { 0x10 } else { 0 };

    out.extend_from_slice(&sample_byte_length.to_le_bytes());
    out.extend_from_slice(&(sample.loop_start as u32).to_le_bytes());

    let loop_length = if sample.loop_end > sample.loop_start {
        (sample.loop_end - sample.loop_start) as u32
    } else {
        0
    };
    out.extend_from_slice(&loop_length.to_le_bytes());

    out.push(sample.default_volume.min(64));
    out.push(sample.fine_tune as u8);
    out.push(type_byte);
    out.push(sample.default_panning);
    out.push(sample.relative_note as u8);
    out.push(0);

    let name_bytes = pad_string(&sample.name, 22);
    out.extend_from_slice(&name_bytes);
}

fn write_xm_sample_data(out: &mut Vec<u8>, sample: &Sample) {
    if sample.data.is_empty() {
        return;
    }

    let is_16bit = sample.bits_per_sample >= 16;

    if is_16bit {
        let mut prev: i32 = 0;
        for &s in sample.data.iter() {
            let val = (s * 32768.0).clamp(-32768.0, 32767.0) as i16;
            let delta = val.wrapping_sub(prev as i16);
            out.extend_from_slice(&delta.to_le_bytes());
            prev = val as i32;
        }
    } else {
        let mut prev: i16 = 0;
        for &s in sample.data.iter() {
            let val = (s * 128.0).clamp(-128.0, 127.0) as i8;
            let delta = val.wrapping_sub(prev as i8);
            out.push(delta as u8);
            prev = val as i16;
        }
    }
}

fn pad_string(s: &str, len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    let src = s.as_bytes();
    let copy_len = src.len().min(len);
    bytes[..copy_len].copy_from_slice(&src[..copy_len]);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xm_handler_detects_valid() {
        let handler = XmHandler;
        let mut data = vec![0u8; 100];
        data[0..17].copy_from_slice(b"Extended Module: ");
        assert!(handler.detect(&data));
    }

    #[test]
    fn xm_handler_detects_invalid() {
        let handler = XmHandler;
        assert!(!handler.detect(b"NOPE"));
    }

    #[test]
    fn decode_xm_note_values() {
        assert_eq!(decode_xm_note(0), Note::None);
        assert_eq!(decode_xm_note(97), Note::Off);
        assert_eq!(decode_xm_note(1), Note::On(0));
        assert_eq!(decode_xm_note(96), Note::On(95));
        assert_eq!(decode_xm_note(60), Note::On(59));
    }

    #[test]
    fn encode_decode_note_roundtrip() {
        assert_eq!(decode_xm_note(encode_xm_note(Note::None)), Note::None);
        assert_eq!(decode_xm_note(encode_xm_note(Note::Off)), Note::Off);
        assert_eq!(decode_xm_note(encode_xm_note(Note::On(0))), Note::On(0));
        assert_eq!(decode_xm_note(encode_xm_note(Note::On(59))), Note::On(59));
    }

    #[test]
    fn decode_xm_volume_column_set_volume() {
        assert_eq!(decode_xm_volume_column(0x10), Effect::VolSetVolume { vol: 0 });
        assert_eq!(decode_xm_volume_column(0x50), Effect::VolSetVolume { vol: 64 });
        assert_eq!(decode_xm_volume_column(0x30), Effect::VolSetVolume { vol: 32 });
    }

    #[test]
    fn decode_xm_volume_column_slide() {
        assert_eq!(decode_xm_volume_column(0x75), Effect::VolSlideUp { amount: 5 });
        assert_eq!(decode_xm_volume_column(0x63), Effect::VolSlideDown { amount: 3 });
    }

    #[test]
    fn decode_xm_volume_column_fine() {
        assert_eq!(decode_xm_volume_column(0x93), Effect::VolFineSlideUp { amount: 3 });
        assert_eq!(decode_xm_volume_column(0x87), Effect::VolFineSlideDown { amount: 7 });
    }

    #[test]
    fn decode_xm_volume_column_portamento() {
        // 0xD0-0xDF = pan slide left (vol_kol handled), returns None
        assert_eq!(decode_xm_volume_column(0xD5), Effect::None);
        // 0xF0-0xFF = tone portamento (fixed from old PortamentoDown bug)
        assert_eq!(decode_xm_volume_column(0xF5), Effect::TonePortamento { speed: 5 });
    }

    #[test]
    fn decode_xm_effect_arpeggio() {
        assert_eq!(decode_xm_effect(0, 0x35, true), Effect::Arpeggio { note1: 3, note2: 5 });
    }

    #[test]
    fn decode_xm_effect_speed_tempo() {
        assert_eq!(decode_xm_effect(0xF, 6, true), Effect::SetSpeed { speed: 6 });
        assert_eq!(decode_xm_effect(0xF, 125, true), Effect::SetTempo { bpm: 125 });
    }

    #[test]
    fn decode_xm_effect_volume_slide() {
        assert_eq!(decode_xm_effect(0xA, 0x52, true), Effect::VolumeSlide { up: 5, down: 2 });
    }

    #[test]
    fn decode_xm_effect_position_jump() {
        assert_eq!(decode_xm_effect(0xB, 3, true), Effect::PositionJump { order: 3 });
    }

    #[test]
    fn decode_xm_effect_pattern_break() {
        assert_eq!(decode_xm_effect(0xD, 0x23, true), Effect::PatternBreak { row: 23 });
    }

    #[test]
    fn decode_xm_ext_effect() {
        assert_eq!(super::decode_xm_extended_effect(0x15), Effect::PortamentoUp { speed: 0x50 });
        assert_eq!(super::decode_xm_extended_effect(0x25), Effect::PortamentoDown { speed: 0x50 });
        assert_eq!(super::decode_xm_extended_effect(0x90), Effect::Retrigger { interval: 0 });
        assert_eq!(super::decode_xm_extended_effect(0x95), Effect::Retrigger { interval: 5 });
    }

    #[test]
    fn delta_8bit_encoding() {
        let raw = [0x10, 0x10, -16i8 as u8];
        let decoded = decode_delta_8bit(&raw);
        assert_eq!(decoded.len(), 3);
        assert!((decoded[0] - 16.0 / 128.0).abs() < 0.01);
        assert!((decoded[1] - 32.0 / 128.0).abs() < 0.01);
        assert!((decoded[2] - 16.0 / 128.0).abs() < 0.01);
    }

    #[test]
    fn delta_16bit_encoding() {
        let raw = [0x10, 0x00, 0x10, 0x00, 0xF0, 0xFF];
        let decoded = decode_delta_16bit(&raw);
        assert_eq!(decoded.len(), 3);
        assert!((decoded[0] - 16.0 / 32768.0).abs() < 0.001);
        assert!((decoded[1] - 32.0 / 32768.0).abs() < 0.001);
        assert!((decoded[2] - (32.0 - 16.0) / 32768.0).abs() < 0.001);
    }

    #[test]
    fn pad_string_truncates() {
        let padded = pad_string("hello", 10);
        assert_eq!(padded.len(), 10);
        assert_eq!(&padded[..5], b"hello");
        assert_eq!(&padded[5..], &[0, 0, 0, 0, 0]);
    }

    #[test]
    fn pad_string_pads_short() {
        let padded = pad_string("test", 4);
        assert_eq!(padded.len(), 4);
        assert_eq!(&padded, b"test");
    }

    #[test]
    fn save_and_detect_roundtrip() {
        let module = Module::default();
        let data = save_module(&module);
        assert!(XmHandler.detect(&data));
    }

    #[test]
    fn encode_xm_note_values() {
        assert_eq!(encode_xm_note(Note::None), 0);
        assert_eq!(encode_xm_note(Note::Off), 97);
        assert_eq!(encode_xm_note(Note::On(0)), 1);
        assert_eq!(encode_xm_note(Note::On(95)), 96);
    }

    #[test]
    fn encode_decode_effect_roundtrip_arpeggio() {
        let (fx, param) = encode_xm_effect(&Effect::Arpeggio { note1: 3, note2: 5 });
        assert_eq!(fx, 0);
        assert_eq!(param, 0x35);
    }

    #[test]
    fn encode_decode_effect_roundtrip_tempo() {
        let (fx, param) = encode_xm_effect(&Effect::SetTempo { bpm: 125 });
        assert_eq!(fx, 0xF);
        assert_eq!(param, 125);
    }

    #[test]
    fn encode_decode_effect_volume_column() {
        let vol = encode_xm_volume_column(&Effect::VolSetVolume { vol: 64 });
        assert_eq!(vol, 0x50);
        assert_eq!(decode_xm_volume_column(vol), Effect::VolSetVolume { vol: 64 });
    }
}
