use std::sync::Arc;

use crate::errors::{FormatError, FormatResult};
use crate::formats::common::*;
use crate::formats::FormatHandler;
use crate::sequencer::{
    Cell, DuplicateCheckAction, DuplicateCheckType, Effect, Envelope, EnvelopeFlags, EnvelopePoint,
    Instrument, LoopType, Module, ModuleFlags, ModuleFormat, NewNoteAction, Note, Pattern, Sample,
    SampleFlags, VibratoWaveform, MAX_CHANNELS, MAX_ENVELOPE_POINTS,
};
use crate::sequencer::effect::{FormatEffect, ItEffect};

pub struct ItHandler;

impl FormatHandler for ItHandler {
    fn format_id(&self) -> &'static str {
        "IT"
    }

    fn file_extension(&self) -> &'static str {
        "it"
    }

    fn detect(&self, data: &[u8]) -> bool {
        data.len() >= 4 && &data[0..4] == b"IMPM"
    }

    fn load(&self, data: &[u8]) -> FormatResult<Module> {
        check_magic(data, 0, b"IMPM")?;

        let name = {
            let mut offset = 4;
            read_string(data, &mut offset, 26)?
        };

        let mut offset = 32; // 0x20
        let order_count = read_u16_le(data, &mut offset)? as usize;
        let instrument_count = read_u16_le(data, &mut offset)? as usize;
        let sample_count = read_u16_le(data, &mut offset)? as usize;
        let pattern_count = read_u16_le(data, &mut offset)? as usize;
        let tracker_version = read_u16_le(data, &mut offset)?;
        let _compatible_version = read_u16_le(data, &mut offset)?;
        let flags = read_u16_le(data, &mut offset)?;
        let special = read_u16_le(data, &mut offset)?;

        let global_volume = read_u8(data, &mut offset)?;
        let mix_volume = read_u8(data, &mut offset)?;
        let initial_speed = read_u8(data, &mut offset)?;
        let initial_tempo = read_u8(data, &mut offset)?;
        let _panning_separation = read_u8(data, &mut offset)?;
        let _pitch_wheel_depth = read_u8(data, &mut offset)?;
        let message_length = read_u16_le(data, &mut offset)? as usize;
        let message_offset = read_u32_le(data, &mut offset)? as usize;
        let _reserved = read_u32_le(data, &mut offset)?;

        let mut channel_panning = vec![32u8; MAX_CHANNELS];
        let mut channel_volume = vec![64u8; MAX_CHANNELS];

        offset = 64;
        for i in 0..64 {
            channel_panning[i] = read_u8(data, &mut offset)?;
        }
        for i in 0..64 {
            channel_volume[i] = read_u8(data, &mut offset)?;
        }

        let order_list = {
            let mut orders = Vec::with_capacity(order_count);
            for _ in 0..order_count {
                let o = read_u8(data, &mut offset)?;
                if o < 254 {
                    orders.push(o);
                }
            }
            orders
        };

        let instrument_paraptrs: Vec<u32> = (0..instrument_count)
            .map(|_| read_u32_le(data, &mut offset))
            .collect::<FormatResult<Vec<_>>>()?;

        let sample_paraptrs: Vec<u32> = (0..sample_count)
            .map(|_| read_u32_le(data, &mut offset))
            .collect::<FormatResult<Vec<_>>>()?;

        let pattern_paraptrs: Vec<u32> = (0..pattern_count)
            .map(|_| read_u32_le(data, &mut offset))
            .collect::<FormatResult<Vec<_>>>()?;

        let module_flags = ModuleFlags {
            stereo: (flags & 0x0001) != 0,
            use_instruments: (flags & 0x0004) != 0,
            linear_slides: (flags & 0x0008) != 0,
            old_effects: (flags & 0x0010) != 0,
            compatible_gxx: (flags & 0x0020) != 0,
            midi_enabled: (flags & 0x0040) != 0,
            request_embed: (flags & 0x0080) != 0,
            fast_volume_slides: false,
            xm_envelope_model: false,
            xm_period_model: false,
            ..ModuleFlags::default()
        };

        let message = if (special & 0x0001) != 0 && message_length > 0 && message_offset > 0 {
            if message_offset + message_length <= data.len() {
                let raw = &data[message_offset..message_offset + message_length];
                Some(
                    raw.iter()
                        .map(|&b| if b == 0x0D { '\n' } else { b as char })
                        .collect::<String>(),
                )
            } else {
                None
            }
        } else {
            None
        };

        let mut instruments = vec![Instrument::default(); instrument_count + 1];
        for (i, &paraptr) in instrument_paraptrs.iter().enumerate() {
            let abs_offset = paraptr as usize;
            if abs_offset == 0 || abs_offset + 4 > data.len() { continue; }
            if &data[abs_offset..abs_offset + 4] == b"IMPI" {
                instruments[i + 1] = parse_it_instrument(data, abs_offset)?;
            }
        }

        let mut samples = vec![Sample::default(); sample_count + 1];
        for (i, &paraptr) in sample_paraptrs.iter().enumerate() {
            let abs_offset = paraptr as usize;
            if abs_offset == 0 || abs_offset + 4 > data.len() { continue; }
            if &data[abs_offset..abs_offset + 4] == b"IMPS" {
                samples[i + 1] = parse_it_sample(data, abs_offset, tracker_version)?;
            }
        }

        let patterns: Vec<Pattern> = pattern_paraptrs
            .iter()
            .map(|&paraptr| {
                let abs_offset = paraptr as usize;
                if abs_offset == 0 || abs_offset >= data.len() {
                    return Ok(Pattern::new(64));
                }
                parse_it_pattern(data, abs_offset)
            })
            .collect::<FormatResult<Vec<_>>>()?;

        Ok(Module {
            name,
            message,
            format: ModuleFormat::IT,
            _version: tracker_version,
            tracker_name: String::new(),
            order_list,
            patterns,
            instruments,
            samples,
            initial_bpm: initial_tempo as u16,
            initial_speed,
            initial_global_volume: global_volume,
            initial_mixing_volume: mix_volume,
            channel_panning,
            channel_volume,
            flags: module_flags,
        })
    }
}

fn parse_it_instrument(data: &[u8], offset: usize) -> FormatResult<Instrument> {
    let mut pos = offset;
    check_magic(data, pos, b"IMPI")?;
    pos += 4;

    let _dos_filename = read_string(data, &mut pos, 12)?;
    let _zero = read_u8(data, &mut pos)?;
    let nna_byte = read_u8(data, &mut pos)?;
    let dup_check_type_byte = read_u8(data, &mut pos)?;
    let dup_check_action_byte = read_u8(data, &mut pos)?;
    let fade_out = read_u16_le(data, &mut pos)?;
    let pitch_pan_separation = read_u8(data, &mut pos)? as i8;
    let pitch_pan_center = read_u8(data, &mut pos)?;
    let global_volume = read_u8(data, &mut pos)?;
    let _default_pan = read_u8(data, &mut pos)?;
    let random_volume = read_u8(data, &mut pos)?;
    let random_panning = read_u8(data, &mut pos)?;
    let _tracker_version = read_u16_le(data, &mut pos)?;
    let _num_samples = read_u8(data, &mut pos)?;
    let _reserved = read_u8(data, &mut pos)?;
    let name = read_string(data, &mut pos, 26)?;
    let _ifc = read_u8(data, &mut pos)?;
    let _ifr = read_u8(data, &mut pos)?;
    let filter_cutoff_byte = _ifc;
    let filter_resonance_byte = _ifr;
    let _m_bank = read_u8(data, &mut pos)?;
    let _m_ch = read_u8(data, &mut pos)?;
    let _m_pr = read_u8(data, &mut pos)?;
    let _midi_pan = read_u8(data, &mut pos)?;

    let mut sample_map = [0u8; 120];
    let mut note_map = {
        let mut m = [0u8; 120];
        for i in 0..120 { m[i] = i as u8; }
        m
    };
    for i in 0..120 {
        let note = read_u8(data, &mut pos)?;
        let sample = read_u8(data, &mut pos)?;
        sample_map[i] = sample;
        if note < 120 {
            note_map[i] = note;
        }
    }

    let vol_env = parse_envelope(data, &mut pos)?;
    let pan_env = parse_envelope(data, &mut pos)?;
    let pitch_env = parse_envelope(data, &mut pos)?;
    let filter_env = parse_envelope(data, &mut pos)?;

    let filter_cutoff_val = if filter_cutoff_byte == 0 { 0xFFFF } else { (filter_cutoff_byte as u16) << 8 };

    Ok(Instrument {
        name,
        sample_map,
        note_map,
        volume_envelope: vol_env,
        panning_envelope: pan_env,
        pitch_envelope: pitch_env,
        filter_envelope: filter_env,
        fade_out,
        nna: match nna_byte {
            1 => NewNoteAction::Continue,
            2 => NewNoteAction::NoteOff,
            3 => NewNoteAction::NoteFade,
            _ => NewNoteAction::NoteCut,
        },
        duplicate_check_type: match dup_check_type_byte {
            1 => DuplicateCheckType::Note,
            2 => DuplicateCheckType::Sample,
            3 => DuplicateCheckType::Instrument,
            _ => DuplicateCheckType::Disabled,
        },
        duplicate_check_action: match dup_check_action_byte {
            1 => DuplicateCheckAction::NoteOff,
            2 => DuplicateCheckAction::NoteFade,
            _ => DuplicateCheckAction::NoteCut,
        },
        pitch_pan_separation,
        pitch_pan_center,
        global_volume,
        filter_cutoff: filter_cutoff_val,
        filter_resonance: filter_resonance_byte,
        filter_type: crate::sequencer::effect::FilterType::LowPass,
        random_volume,
        random_panning,
        filter_random_cutoff: 0,
        vib_type: 0,
        vib_sweep: 0,
        vib_depth: 0,
        vib_rate: 0,
    })
}

fn parse_envelope(data: &[u8], pos: &mut usize) -> FormatResult<Option<Envelope>> {
    let flags_byte = read_u8(data, pos)?;
    let num_points = read_u8(data, pos)? as usize;
    let loop_start = read_u8(data, pos)?;
    let loop_end = read_u8(data, pos)?;
    let sustain_start = read_u8(data, pos)?;
    let sustain_end = read_u8(data, pos)?;

    let mut points = Vec::with_capacity(num_points.min(MAX_ENVELOPE_POINTS));
    for _ in 0..num_points.min(MAX_ENVELOPE_POINTS) {
        let tick = read_u16_le(data, pos)?;
        let value = read_u8(data, pos)?;
        points.push(EnvelopePoint { tick, value });
    }

    *pos += (25usize.saturating_sub(num_points.min(MAX_ENVELOPE_POINTS))) * 3;

    if flags_byte == 0 && points.is_empty() {
        return Ok(None);
    }

    Ok(Some(Envelope {
        points,
        sustain_point: if sustain_end < 25 && flags_byte & 0x02 != 0 {
            Some(sustain_start as usize)
        } else {
            None
        },
        loop_start: if flags_byte & 0x04 != 0 && (loop_start as usize) < 25 {
            Some(loop_start as usize)
        } else {
            None
        },
        loop_end: if flags_byte & 0x04 != 0 && (loop_end as usize) < 25 {
            Some(loop_end as usize)
        } else {
            None
        },
        flags: EnvelopeFlags {
            enabled: (flags_byte & 0x01) != 0,
            sustain: (flags_byte & 0x02) != 0,
            loop_: (flags_byte & 0x04) != 0,
            carry: (flags_byte & 0x08) != 0,
        },
    }))
}

fn parse_it_sample(data: &[u8], offset: usize, tracker_version: u16) -> FormatResult<Sample> {
    let mut pos = offset;
    check_magic(data, pos, b"IMPS")?;
    pos += 4;

    let _dos_filename = read_string(data, &mut pos, 12)?;
    let _zero = read_u8(data, &mut pos)?;
    let global_volume = read_u8(data, &mut pos)?;
    let flags_byte = read_u8(data, &mut pos)?;
    let default_volume = read_u8(data, &mut pos)?;
    let name = read_string(data, &mut pos, 26)?;
    let convert_byte = read_u8(data, &mut pos)?;
    let default_panning = read_u8(data, &mut pos)?;

    let sample_data_length = read_u32_le(data, &mut pos)? as usize;
    let loop_start = read_u32_le(data, &mut pos)? as usize;
    let loop_end = read_u32_le(data, &mut pos)? as usize;
    let c5speed = read_u32_le(data, &mut pos)?;
    let _sustain_loop_start = read_u32_le(data, &mut pos)? as usize;
    let _sustain_loop_end = read_u32_le(data, &mut pos)? as usize;
    let sample_data_offset = read_u32_le(data, &mut pos)? as usize;

    let vibrato_speed = read_u8(data, &mut pos)?;
    let vibrato_depth = read_u8(data, &mut pos)?;
    let vibrato_rate = read_u8(data, &mut pos)?;
    let vibrato_waveform_byte = read_u8(data, &mut pos)?;

    let is_16bit = (flags_byte & 0x02) != 0;
    let is_stereo = (flags_byte & 0x04) != 0 && tracker_version >= 0x0214;
    let is_compressed = (flags_byte & 0x08) != 0;
    let has_loop = (flags_byte & 0x10) != 0;
    let is_ping_pong = (flags_byte & 0x40) != 0;

    let loop_type = if has_loop {
        if is_ping_pong { LoopType::PingPong } else { LoopType::Forward }
    } else {
        LoopType::None
    };

    let is_unsigned = (convert_byte & 0x01) != 0;
    let is_big_endian = (convert_byte & 0x02) != 0;
    let is_delta_pcm = (convert_byte & 0x04) != 0;
    let is_it215 = is_compressed && is_delta_pcm;

    let sample_data = if sample_data_offset > 0 && sample_data_length > 0 && sample_data_offset < data.len() {
        if is_compressed {
            let compressed = &data[sample_data_offset..];
            decompress_it_sample(compressed, is_16bit, is_it215, sample_data_length, is_unsigned, is_stereo)?
        } else {
            let num_channels = if is_stereo { 2 } else { 1 };
            let bytes_per_sample = if is_16bit { 2 } else { 1 };
            let size = sample_data_length * num_channels * bytes_per_sample;
            let end = (sample_data_offset + size).min(data.len());
            let raw = &data[sample_data_offset..end];
            Arc::new(load_raw_sample(raw, is_16bit, is_unsigned, is_big_endian, is_delta_pcm, is_stereo))
        }
    } else {
        Arc::new(Vec::new())
    };

    Ok(Sample {
        name,
        data: sample_data,
        sample_rate: c5speed,
        bits_per_sample: if is_16bit { 16 } else { 8 },
        loop_type,
        loop_start,
        loop_end: if loop_end > loop_start { loop_end } else { 0 },
        default_volume,
        default_panning,
        global_volume,
        relative_note: 0,
        fine_tune: 0,
        vibrato_speed,
        vibrato_depth,
        vibrato_rate,
        vibrato_waveform: match vibrato_waveform_byte & 0x03 {
            0 => VibratoWaveform::Sine,
            1 => VibratoWaveform::Square,
            2 => VibratoWaveform::Ramp,
            3 => VibratoWaveform::Random,
            _ => VibratoWaveform::Sine,
        },
        _flags: SampleFlags { is_stereo, is_16bit, is_compressed, has_trailing_byte: false },
    })
}

fn load_raw_sample(raw: &[u8], is_16bit: bool, is_unsigned: bool, is_big_endian: bool, is_delta_pcm: bool, is_stereo: bool) -> Vec<f32> {
    if raw.is_empty() { return Vec::new(); }
    let num_channels = if is_stereo { 2 } else { 1 };
    let bytes_per_sample = if is_16bit { 2 } else { 1 };
    let stride = bytes_per_sample * num_channels;
    let total_samples = if is_16bit { raw.len() / 2 } else { raw.len() };
    let samples_per_channel = total_samples / num_channels;
    let mut samples = Vec::with_capacity(samples_per_channel);

    if is_16bit {
        let mut acc: i32 = 0;
        for i in 0..samples_per_channel {
            let off = i * stride;
            if off + 1 >= raw.len() { break; }
            let raw_val = if is_big_endian {
                u16::from_be_bytes([raw[off], raw[off + 1]])
            } else {
                u16::from_le_bytes([raw[off], raw[off + 1]])
            };
            if is_delta_pcm {
                acc += raw_val as i16 as i32;
            } else {
                acc = if is_unsigned { (raw_val as i32).wrapping_sub(32768) } else { raw_val as i16 as i32 };
            }
            samples.push(acc as f32 / 32768.0);
        }
    } else {
        let mut acc: i32 = 0;
        for i in 0..samples_per_channel {
            let off = i * stride;
            if off >= raw.len() { break; }
            let raw_val = raw[off];
            if is_delta_pcm {
                acc += raw_val as i8 as i32;
            } else {
                acc = if is_unsigned { (raw_val as i32).wrapping_sub(128) } else { raw_val as i8 as i32 };
            }
            samples.push(acc as f32 / 128.0);
        }
    }
    samples
}

fn decompress_it_sample(
    compressed: &[u8],
    is_16bit: bool,
    is_it215: bool,
    sample_data_length: usize,
    is_unsigned: bool,
    is_stereo: bool,
) -> FormatResult<Arc<Vec<f32>>> {
    let num_channels = if is_stereo { 2 } else { 1 };
    let decompressed = if is_16bit {
        let mut raw = Vec::with_capacity(sample_data_length * num_channels * 2);
        let mut compressed_offset = 0usize;
        for _ch in 0..num_channels {
            let (channel_data, bytes_read) = decompress_it214_16bit(compressed, compressed_offset, is_it215, sample_data_length, raw.len() / 2)?;
            raw.extend_from_slice(&channel_data);
            compressed_offset = bytes_read;
        }
        raw
    } else {
        let mut raw = Vec::with_capacity(sample_data_length * num_channels);
        let mut compressed_offset = 0usize;
        for _ch in 0..num_channels {
            let (channel_data, bytes_read) = decompress_it214_8bit(compressed, compressed_offset, is_it215, sample_data_length, raw.len())?;
            raw.extend_from_slice(&channel_data);
            compressed_offset = bytes_read;
        }
        raw
    };

    let samples_per_channel = sample_data_length;
    let samples: Vec<f32> = if num_channels == 2 {
        let ch0_bytes = &decompressed[..samples_per_channel * if is_16bit { 2 } else { 1 }];
        if is_16bit {
            ch0_bytes.chunks_exact(2).map(|chunk| {
                let val = u16::from_le_bytes([chunk[0], chunk[1]]);
                if is_unsigned { (val as f32 - 32768.0) / 32768.0 } else { (val as i16) as f32 / 32768.0 }
            }).collect()
        } else {
            ch0_bytes.iter().map(|&b| {
                if is_unsigned { (b as f32 - 128.0) / 128.0 } else { (b as i8) as f32 / 128.0 }
            }).collect()
        }
    } else if is_16bit {
        decompressed.chunks_exact(2).map(|chunk| {
            let val = u16::from_le_bytes([chunk[0], chunk[1]]);
            if is_unsigned { (val as f32 - 32768.0) / 32768.0 } else { (val as i16) as f32 / 32768.0 }
        }).collect()
    } else {
        decompressed.iter().map(|&b| {
            if is_unsigned { (b as f32 - 128.0) / 128.0 } else { (b as i8) as f32 / 128.0 }
        }).collect()
    };
    Ok(Arc::new(samples))
}

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, byte_pos: 0, bit_pos: 0 }
    }

    fn read_bits(&mut self, num_bits: u8) -> Option<u32> {
        let mut result: u32 = 0;
        let mut bits_out: u8 = 0;
        while bits_out < num_bits {
            if self.byte_pos >= self.data.len() { return None; }
            let bits_available = 8 - self.bit_pos;
            let bits_to_read = bits_available.min(num_bits - bits_out);
            let mask = ((1u32 << bits_to_read) - 1) << self.bit_pos;
            let val = ((self.data[self.byte_pos] as u32) & mask) >> self.bit_pos;
            result |= val << bits_out;
            bits_out += bits_to_read;
            self.bit_pos += bits_to_read;
            if self.bit_pos >= 8 { self.bit_pos = 0; self.byte_pos += 1; }
        }
        Some(result)
    }
}

fn decompress_it214_8bit(compressed: &[u8], start_offset: usize, is_it215: bool, sample_data_length: usize, samples_already_decompressed: usize) -> FormatResult<(Vec<u8>, usize)> {
    let mut pos = start_offset;
    let channel_offset = samples_already_decompressed;
    let channel_end = channel_offset + sample_data_length;
    let mut output = Vec::with_capacity(sample_data_length);
    while output.len() < sample_data_length {
        if pos + 2 > compressed.len() { break; }
        let block_size = u16::from_le_bytes([compressed[pos], compressed[pos+1]]) as usize;
        pos += 2;
        if block_size == 0 { break; }
        if pos + block_size > compressed.len() { break; }
        let block_data = &compressed[pos..pos + block_size];
        pos += block_size;
        let mut reader = BitReader::new(block_data);
        let mut bit_width: u8 = 9;
        let mut value1: i16 = 0;
        let mut value2: i16 = 0;
        let global_pos = channel_offset + output.len();
        let block_target_len = ((global_pos / 0x8000 + 1) * 0x8000).min(channel_end) - channel_offset;
        while output.len() < block_target_len {
            let raw = reader.read_bits(bit_width).ok_or_else(|| FormatError::DecompressionFailed("IT214: EOF".into()))?;
            if bit_width <= 6 {
                if raw == (1 << (bit_width - 1)) {
                    let mut nw = (reader.read_bits(3).ok_or_else(|| FormatError::DecompressionFailed("IT214: EOF".into()))? as u8).wrapping_add(1);
                    if nw >= bit_width { nw = nw.wrapping_add(1); }
                    bit_width = nw;
                    continue;
                }
            } else if bit_width < 9 {
                let border = (1 << (bit_width - 1)) - 4;
                if raw >= border && raw <= border + 7 {
                    let mut nw = (raw - border) as u8 + 1;
                    if nw >= bit_width { nw += 1; }
                    bit_width = nw;
                    continue;
                }
            } else if bit_width == 9 {
                if (raw & 0x100) != 0 {
                    bit_width = (raw & 0xFF) as u8 + 1;
                    continue;
                }
            }
            let delta = if (raw & (1 << (bit_width - 1))) != 0 { (raw as i32) - (1 << bit_width) } else { raw as i32 };
            value1 = value1.wrapping_add(delta as i16);
            value2 = value2.wrapping_add(value1);
            output.push(if is_it215 { value2 as u8 } else { value1 as u8 });
        }
    }
    Ok((output, pos))
}

fn decompress_it214_16bit(compressed: &[u8], start_offset: usize, is_it215: bool, sample_data_length: usize, samples_already_decompressed: usize) -> FormatResult<(Vec<u8>, usize)> {
    let mut pos = start_offset;
    let channel_offset = samples_already_decompressed;
    let channel_end = channel_offset + sample_data_length;
    let mut output = Vec::with_capacity(sample_data_length * 2);
    while output.len() < sample_data_length * 2 {
        if pos + 2 > compressed.len() { break; }
        let block_size = u16::from_le_bytes([compressed[pos], compressed[pos+1]]) as usize;
        pos += 2;
        if block_size == 0 { break; }
        if pos + block_size > compressed.len() { break; }
        let block_data = &compressed[pos..pos + block_size];
        pos += block_size;
        let mut reader = BitReader::new(block_data);
        let mut bit_width: u8 = 17;
        let mut value1: i32 = 0;
        let mut value2: i32 = 0;
        let global_sample_pos = channel_offset + output.len() / 2;
        let block_target_samples = ((global_sample_pos / 0x4000 + 1) * 0x4000).min(channel_end) - channel_offset;
        while output.len() < block_target_samples * 2 {
            let raw = reader.read_bits(bit_width).ok_or_else(|| FormatError::DecompressionFailed("IT214: EOF".into()))?;
            if bit_width <= 6 {
                if raw == (1 << (bit_width - 1)) {
                    let mut nw = reader.read_bits(if is_it215 { 5 } else { 4 }).ok_or_else(|| FormatError::DecompressionFailed("IT214: EOF".into()))? as u8;
                    if nw == 0 {
                        nw = reader.read_bits(if is_it215 { 4 } else { 8 }).ok_or_else(|| FormatError::DecompressionFailed("IT214: EOF".into()))? as u8;
                        if nw == 0 { break; }
                        bit_width = nw;
                    } else {
                        if nw >= bit_width { nw += 1; }
                        bit_width = nw;
                    }
                    continue;
                }
            } else if bit_width < 17 {
                let border = (1 << (bit_width - 1)) - 8;
                if raw >= border && raw <= border + 15 {
                    let mut nw = (raw - border) as u8 + 1;
                    if nw >= bit_width { nw += 1; }
                    bit_width = nw;
                    continue;
                }
            } else if bit_width == 17 {
                if (raw & 0x10000) != 0 {
                    bit_width = (raw & 0xFF) as u8 + 1;
                    continue;
                }
            }
            let delta = if (raw & (1 << (bit_width - 1))) != 0 { (raw as i32) - (1 << bit_width) } else { raw as i32 };
            value1 = value1.wrapping_add(delta);
            value2 = value2.wrapping_add(value1);
            let out_val = if is_it215 { value2 } else { value1 };
            let b = (out_val as i16).to_le_bytes();
            output.push(b[0]); output.push(b[1]);
        }
    }
    Ok((output, pos))}

fn parse_it_pattern(data: &[u8], offset: usize) -> FormatResult<Pattern> {
    let mut pos = offset;
    let _len = read_u16_le(data, &mut pos)?;
    let rows = read_u16_le(data, &mut pos)? as usize;
    pos += 4;
    let rows = if rows == 0 { 64 } else { rows };
    let mut pattern = Pattern::new(rows);
    let mut last_mask = [0u8; 64];
    let mut last_note = [0u8; 64];
    let mut last_inst = [0u8; 64];
    let mut last_vol = [0u8; 64];
    let mut last_fx = [0u8; 64];
    let mut last_fxp = [0u8; 64];
    let mut row = 0usize;
    while row < rows {
        if pos >= data.len() { break; }
        let mask_byte = data[pos]; pos += 1;
        if mask_byte == 0 { row += 1; continue; }
        let ch = ((mask_byte & 0x7F) as usize).saturating_sub(1);
        if ch >= 64 {
             if (mask_byte & 0x80) != 0 {
                 let m = data[pos]; pos += 1;
                 if (m & 0x01) != 0 { pos += 1; }
                 if (m & 0x02) != 0 { pos += 1; }
                 if (m & 0x04) != 0 { pos += 1; }
                 if (m & 0x08) != 0 { pos += 1; }
                 if (m & 0x10) != 0 { pos += 1; }
             }
             continue;
        }
        if (mask_byte & 0x80) != 0 {
            let m = data[pos]; pos += 1;
            last_mask[ch] = m;
            if (m & 0x01) != 0 { last_note[ch] = data[pos]; pos += 1; }
            if (m & 0x02) != 0 { last_inst[ch] = data[pos]; pos += 1; }
            if (m & 0x04) != 0 { last_vol[ch] = data[pos]; pos += 1; }
            if (m & 0x08) != 0 { last_fx[ch] = data[pos]; pos += 1; last_fxp[ch] = data[pos]; pos += 1; }
        }
        let m = last_mask[ch];
        let note = if (m & 0x01) != 0 { decode_it_note(last_note[ch]) } else { Note::None };
        let inst = if (m & 0x02) != 0 && last_inst[ch] > 0 { Some(last_inst[ch]) } else { None };
        let vol = if (m & 0x04) != 0 { Some(last_vol[ch]) } else { None };
        let fx = if (m & 0x08) != 0 { decode_it_effect(last_fx[ch], last_fxp[ch]) } else { Effect::None };
        pattern.data[row][ch] = Cell { note, instrument: inst, volume: vol, volume_effect: None, effect: fx };
    }
    Ok(pattern)
}

fn decode_it_note(raw: u8) -> Note {
    match raw {
        253 => Note::Fade,
        254 => Note::Cut,
        255 => Note::Off,
        n if n < 120 => Note::On(n),
        _ => Note::None,
    }
}

fn decode_it_effect(fx: u8, p: u8) -> Effect {
    match fx {
        0 => Effect::Arpeggio { note1: p >> 4, note2: p & 0x0F },
        1 => Effect::PortamentoUp { speed: p },
        2 => Effect::PortamentoDown { speed: p },
        3 => Effect::TonePortamento { speed: p },
        4 => Effect::Vibrato { speed: p >> 4, depth: p & 0x0F },
        5 => Effect::TonePortamentoVolumeSlide { up: p as i8 },
        6 => Effect::VibratoVolumeSlide { up: p as i8 },
        7 => Effect::Tremolo { speed: p >> 4, depth: p & 0x0F },
        8 => Effect::SetPanning { pan: p },
        9 => Effect::SetSampleOffset { offset: (p as u16) << 8 },
        10 => Effect::VolumeSlide { up: p >> 4, down: p & 0x0F },
        11 => Effect::PositionJump { order: p },
        12 => Effect::SetVolume { volume: p },
        13 => Effect::PatternBreak { row: p },
        14 => {
            let sub = p >> 4;
            let val = p & 0x0F;
            match sub {
                0x1 => Effect::FinePortamentoUp { speed: val << 4 },
                0x2 => Effect::FinePortamentoDown { speed: val << 4 },
                0x3 => Effect::GlissandoControl { on: val != 0 },
                0x4 => Effect::VibratoWaveform { waveform: val & 0x03 },
                0x5 => Effect::SetFineTune { tune: val },
                0x6 => Effect::PatternLoop { count: val },
                0x7 => Effect::TremoloWaveform { waveform: val & 0x03 },
                0x8 => Effect::SetPanning16 { pan: val << 4 },
                0x9 => Effect::Retrigger { interval: val },
                0xA => Effect::FineVolumeSlideUp { amount: val },
                0xB => Effect::FineVolumeSlideDown { amount: val },
                0xC => Effect::NoteCutAfter { ticks: val },
                0xD => Effect::NoteDelay { ticks: val },
                0xE => Effect::PatternDelay { ticks: val },
                _ if sub > 0 => Effect::FormatSpecific(FormatEffect::It(ItEffect::Raw { effect: 0xE0 | sub, param: val })),
                _ => Effect::None,
            }
        }
        15 => if p < 32 { Effect::SetSpeed { speed: p } } else { Effect::SetTempo { bpm: p } },
        16 => Effect::SetGlobalVolume { volume: p },
        17 => Effect::GlobalVolumeSlide { up: (p >> 4) as i8, down: -((p & 0x0F) as i8) },
        18 => Effect::SetEnvelopePosition { tick: p as u16 },
        19 => Effect::Panbrello { speed: p >> 4, depth: p & 0x0F },
        20 => {
            let hi = p >> 4;
            let lo = p & 0x0F;
            match hi {
                0 => Effect::FineVolumeSlideUp { amount: lo },
                1 => Effect::FineVolumeSlideDown { amount: lo },
                2 => Effect::FinePortamentoUp { speed: lo << 4 },
                3 => Effect::FinePortamentoDown { speed: lo << 4 },
                4 => Effect::FinePortamentoUp { speed: lo },
                5 => Effect::FinePortamentoDown { speed: lo },
                6 => Effect::PortamentoUp { speed: lo },
                7 => Effect::PortamentoDown { speed: lo },
                _ => Effect::FormatSpecific(FormatEffect::It(ItEffect::Raw { effect: fx, param: p })),
            }
        }
        21 => Effect::Vibrato { speed: p, depth: 0 },
        22 => Effect::Vibrato { speed: 0, depth: p },
        23 => {
            let hi = p >> 4;
            let _lo = p & 0x0F;
            match hi {
                1 => Effect::SetSampleOffset { offset: (p as u16) << 8 },
                _ => Effect::SetPanning { pan: p },
            }
        }
        _ if fx > 0 => Effect::FormatSpecific(FormatEffect::It(ItEffect::Raw { effect: fx, param: p })),
        _ => Effect::None,
    }
}

pub fn save_module(_module: &Module) -> Vec<u8> {
    // Placeholder implementation for now
    Vec::new()
}

