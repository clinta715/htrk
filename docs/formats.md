# File Formats

## Overview

htrk supports four classic tracker module formats. Each format has a loader (parse binary → `Module`)
and most have a writer (serialize `Module` → binary). The IT format is the primary target.

## Format Handler Trait

```rust
trait FormatHandler {
    fn format_id(&self) -> &'static str;
    fn file_extension(&self) -> &'static str;
    fn detect(&self, data: &[u8]) -> bool;
    fn load(&self, data: &[u8]) -> Result<Module, FormatError>;
    fn save(&self, module: &Module) -> Result<Vec<u8>, FormatError>;
}
```

## Format Detection

```rust
fn detect_format(data: &[u8]) -> Option<ModuleFormat> {
    if data.len() < 4 { return None; }

    let magic = &data[0..4];

    match magic {
        b"IMPM" => Some(ModuleFormat::IT),      // Impulse Tracker
        b"Extended Module" if data.len() > 37 => {
            // XM: "Extended Module: " + name (20 bytes) + \x1a
            Some(ModuleFormat::XM)
        }
        _ => {
            // S3M: no magic at offset 0, check at offset 44
            if data.len() > 44 && &data[44..48] == b"SCRM" {
                return Some(ModuleFormat::S3M);
            }

            // MOD: check tracker magic at offset 1080
            if data.len() > 1084 {
                let mod_magic = &data[1080..1084];
                if is_mod_magic(mod_magic) {
                    return Some(ModuleFormat::MOD);
                }
            }

            None
        }
    }
}

fn is_mod_magic(magic: &[u8]) -> bool {
    const MOD_SIGNATURES: &[&[u8]] = &[
        b"M.K.", b"M!K!", b"FLT4", b"FLT8",
        b"4CHN", b"6CHN", b"8CHN", b"2CHN",
        b"CD81", b"OKTA", b"16CN", b"32CN",
    ];
    MOD_SIGNATURES.iter().any(|sig| magic == *sig)
}
```

---

## IT Format (Impulse Tracker) — Primary Target

### File Structure

```
┌─────────────────────────────┐ offset 0
│ Header (192 bytes)          │
├─────────────────────────────┤ offset 192
│ Order List (variable)       │
├─────────────────────────────┤ offset 192 + ord_len
│ Instrument Parapointers     │ (4 bytes each × num_instruments)
├─────────────────────────────┤
│ Sample Parapointers         │ (4 bytes each × num_samples)
├─────────────────────────────┤
│ Pattern Parapointers        │ (4 bytes each × num_patterns)
├─────────────────────────────┤
│ Instrument Data             │ (variable per instrument)
│   ┌───────────────────────┐ │
│   │ Instrument Header     │ │ (554 bytes)
│   │   + Envelope Points   │ │
│   │   + Sample Map        │ │
│   └───────────────────────┘ │
├─────────────────────────────┤
│ Sample Data                 │ (variable per sample)
│   ┌───────────────────────┐ │
│   │ Sample Header         │ │ (80 bytes)
│   │   + Sample Data       │ │ (raw PCM or compressed)
│   └───────────────────────┘ │
├─────────────────────────────┤
│ Pattern Data                │ (variable per pattern)
│   ┌───────────────────────┐ │
│   │ Row data (packed)     │ │
│   └───────────────────────┘ │
└─────────────────────────────┘
```

### Header (192 bytes)

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | magic | "IMPM" |
| 4 | 26 | song_name | Null-terminated song title |
| 30 | 2 | highlight_row | Pattern highlight row spacing (minor) |
| 32 | 2 | order_count | Number of orders |
| 34 | 2 | instrument_count | Number of instruments |
| 36 | 2 | sample_count | Number of samples |
| 38 | 2 | pattern_count | Number of patterns |
| 40 | 2 | tracker_version | Version (e.g., 0x0217 = v2.17) |
| 42 | 2 | compatible_version | Minimum version for full features |
| 44 | 2 | flags | Song flags (bitfield) |
| 46 | 2 | special | Special flags (bitfield) |
| 48 | 1 | global_volume | 0-128 (default 128) |
| 49 | 1 | mix_volume | 0-128 mixing volume |
| 50 | 1 | initial_speed | Ticks per row (1-255) |
| 51 | 1 | initial_tempo | BPM (32-255) |
| 52 | 1 | panning_separation | Pitch-pan separation |
| 53 | 1 | pitch_wheel_depth | MIDI pitch wheel depth |
| 54 | 2 | message_length | Song message length |
| 56 | 4 | message_offset | Offset to song message |
| 60 | 4 | reserved | Reserved |
| 64 | 64 | channel_panning | Per-channel panning (0-64, bit 7 = muted) |
| 128 | 64 | channel_volume | Per-channel volume (0-64) |

#### Flags Bitfield (offset 44)

| Bit | Flag | Description |
|-----|------|-------------|
| 0 | stereo | Stereo mode |
| 1 | vol0_opt | Optimize for volume 0 |
| 2 | use_instruments | Use instrument mode (vs. sample mode) |
| 3 | linear_slides | Use linear slides |
| 4 | old_effects | Old effects mode |
| 5 | link_effects | Link effect G to effect value |
| 6 | use_midi | Use MIDI controller |
| 7 | request_embed | Request MIDI embed |
| 8-15 | reserved | Reserved |

#### Special Bitfield (offset 46)

| Bit | Flag | Description |
|-----|------|-------------|
| 0 | has_message | Song message attached |
| 1 | has_midi | Embedded MIDI config |
| 2 | has_embed | Embedded samples |

### Pattern Packing Format

IT patterns use a run-length + variable-length packing scheme:

```
Each row is encoded as a sequence of channel entries:
  - If channel_byte == 0: end of row
  - If (channel_byte & 0x7F) is channel (1-64):
    - If (channel_byte & 0x80) != 0: read mask_variable
    - Else: reuse last mask_variable for this channel

Mask variable (read if bit 7 of channel_byte set):
  Bit 0:   note follows
  Bit 1:   instrument follows
  Bit 2:   volume column follows
  Bit 3:   effect follows
  Bit 4:   reuse last note
  Bit 5:   reuse last instrument
  Bit 6:   reuse last volume
  Bit 7:   reuse last effect

Values:
  Note:       0-119 = note, 255 = note off, 254 = note cut, 253 = note fade
  Instrument: 1-255 = instrument
  Volume:     0-64 = set volume, 128-192 = panning
  Effect:     effect type character (A-Z)
  EffectParam: 0-255
```

#### Pattern Parsing Pseudocode

```rust
fn parse_it_pattern(data: &[u8], num_rows: usize) -> Pattern {
    let mut pattern = Pattern::new(num_rows);
    let mut pos = 0;
    let mut row = 0;

    // Channel memory for packing
    let mut last_mask = [0u8; 64];
    let mut last_note = [0u8; 64];
    let mut last_inst = [0u8; 64];
    let mut last_vol = [0u8; 64];
    let mut last_fx = [0u8; 64];
    let mut last_fx_param = [0u8; 64];

    while row < num_rows {
        let channel_byte = data[pos];
        pos += 1;

        if channel_byte == 0 {
            row += 1;
            continue;
        }

        let channel = ((channel_byte & 0x7F) as usize) - 1;

        if (channel_byte & 0x80) != 0 {
            last_mask[channel] = data[pos];
            pos += 1;
        }

        let mask = last_mask[channel];

        if (mask & 0x01) != 0 {
            last_note[channel] = data[pos];
            pos += 1;
        }
        if (mask & 0x02) != 0 {
            last_inst[channel] = data[pos];
            pos += 1;
        }
        if (mask & 0x04) != 0 {
            last_vol[channel] = data[pos];
            pos += 1;
        }
        if (mask & 0x08) != 0 {
            last_fx[channel] = data[pos];
            pos += 1;
            last_fx_param[channel] = data[pos];
            pos += 1;
        }
        
        // (Handling of reuse bits 4-7 omitted for brevity)
    }

    pattern
}
```

### Sample Compression (IT214 / IT215)

IT files can use two compression schemes. Both are bitstream-based, block-oriented,
variable-bit-width delta encodings. The difference is the integration depth.

#### Detecting IT214 vs IT215

The `convert` byte (offset 0x20 in sample header) bit 2 (`cvtDelta`, mask `0x04`) selects
the compression mode for compressed samples:

| `convert & 0x04` | Mode | Integration |
|---|---|---|
| 0 | IT214 | Single-delta: `mem1 += delta; output = mem1` |
| 1 | IT215 | Double-delta: `mem1 += delta; mem2 += mem1; output = mem2` |

**Do NOT use the tracker version to decide** — the convert byte is authoritative.

#### Convert Byte (`cvt`) Flags

| Bit | Mask | Meaning (uncompressed) | Meaning (compressed) |
|-----|------|----------------------|---------------------|
| 0 | 0x01 | Signed (off) / Unsigned (on) | Same — applies post-decompression |
| 1 | 0x02 | Big-endian (on) / Little-endian (off) | N/A for compressed |
| 2 | 0x04 | Delta PCM (running sum) | IT215 (double-delta) |
| 3 | 0x08 | Byte-delta (PTM loader) | N/A |

#### Block Structure

Compressed sample data is organized into blocks, each starting with a 2-byte LE length
(the size of the compressed bitstream, NOT including those 2 bytes). Each block:

- Resets the bit width to its default (9 for 8-bit, 17 for 16-bit)
- Resets both integrator buffers (`mem1 = 0`, `mem2 = 0`)
- Produces up to 0x8000 bytes of uncompressed output per block
  (0x8000 samples for 8-bit, 0x4000 samples for 16-bit)

```
CompressedData := Block*
Block          := u16le(block_size) + byte[block_size](bitstream)
```

#### Decoding Algorithm (8-bit)

```
bit_width = 9
mem1 = 0, mem2 = 0

for each sample in block (up to 0x8000):
    raw = read_bits(bit_width)

    if bit_width <= 6:
        if raw == (1 << (bit_width - 1)):
            new_width = read_bits(3) + 1
            if new_width >= bit_width: new_width += 1
            bit_width = new_width; continue
    else if bit_width < 9:
        border = (1 << (bit_width - 1)) - 4
        if raw >= border and raw <= border + 7:
            new_width = (raw - border) + 1
            if new_width >= bit_width: new_width += 1
            bit_width = new_width; continue
    else if bit_width == 9:
        if raw & 0x100:
            bit_width = (raw & 0xFF) + 1; continue

    delta = sign_extend(raw, bit_width)    // two's complement
    mem1 += delta
    mem2 += mem1
    output = is_it215 ? mem2 : mem1
```

#### Decoding Algorithm (16-bit)

Structurally identical to 8-bit, with different constants:

| Parameter | 8-bit | 16-bit |
|-----------|-------|--------|
| Default bit width | 9 | 17 |
| Mode A: bits read for width change | 3 | 4 (IT214) or 5 (IT215) |
| Mode B: border offset | -4 | -8 |
| Mode B: range | 8 values | 16 values |
| Mode B: active range | widths 7–8 | widths 7–16 |
| Mode C: top bit check | bit 8 | bit 16 |
| Block size (samples) | 0x8000 | 0x4000 |
| Block size (bytes) | 0x8000 | 0x8000 |

For bit_width <= 6 in 16-bit, Mode A may read a secondary width byte if the first is zero:
```
    new_width = read_bits(4 or 5)
    if new_width == 0:
        new_width = read_bits(8 or 4)
        if new_width == 0: break  // end of block
    else:
        if new_width >= bit_width: new_width += 1
    bit_width = new_width
```

Both 8-bit and 16-bit use the same double-delta integration for IT215.

#### Stereo Compressed Samples

Stereo compressed samples (flags bit 2, only valid for `cwtv >= 0x0214`) are stored as
two independent compressed streams concatenated back-to-back. Each stream:

- Is decoded with fully reset state (bit width, integrators, block position)
- Has `sample_data_length` samples per channel
- Uses the same block boundary alignment (global position, not per-channel)

The loader extracts only the left (first) channel for playback.

### Instrument Header (554 bytes)

| Offset | Size | Field |
|--------|------|-------|
| 4 | 4 | magic "IMPI" |
| 8 | 26 | instrument_name |
| 36 | 1 | new_note_action |
| 37 | 1 | duplicate_check_type |
| 38 | 1 | duplicate_check_action |
| 39 | 2 | fade_out (0-4095) |
| 41 | 1 | pitch_pan_separation |
| 42 | 1 | pitch_pan_center |
| 43 | 1 | global_volume (0-128) |
| 44 | 1 | default_pan (0-64, bit 7 = respect) |
| 45 | 1 | random_volume |
| 46 | 1 | random_panning |
| 48 | 120 | sample_map (120 × 1 byte) |
| 172 | 1 | volume_envelope_enabled |
| 173 | 1 | volume_envelope_loop_start |
| 174 | 1 | volume_envelope_loop_end |
| 175 | 1 | volume_envelope_sustain_start |
| 176 | 1 | volume_envelope_sustain_end |
| 177 | 200 | volume_envelope_points (25 × 8 bytes: tick + value each) |
| 377 | 1 | panning_envelope_enabled |
| 378 | 1 | panning_envelope_loop_start/end |
| 379-380 | | panning sustain points |
| 381 | 200 | panning_envelope_points |
| 581 | 1 | pitch_envelope_enabled |
| ... | ... | pitch_envelope_points |

### Sample Header (80 bytes)

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | magic | "IMPS" |
| 4 | 12 | dos_filename | DOS 8.3 filename |
| 16 | 1 | zero | Always 0x00 |
| 17 | 1 | global_volume | Global volume (0-64) |
| 18 | 1 | flags | Sample flags (bitfield, see below) |
| 19 | 1 | default_volume | Default volume (0-64) |
| 20 | 26 | sample_name | Null-terminated sample name |
| 46 | 1 | cvt | Convert byte (signedness, endianness, compression mode) |
| 47 | 1 | default_panning | Default panning (0-64, bit 7 = use) |
| 48 | 4 | sample_data_length | Length in samples (not bytes; per-channel for stereo) |
| 52 | 4 | loop_start | Loop start in samples |
| 56 | 4 | loop_end | Loop end in samples |
| 60 | 4 | c5speed | Sample rate for middle C |
| 64 | 4 | sustain_loop_start | Sustain loop start in samples |
| 68 | 4 | sustain_loop_end | Sustain loop end in samples |
| 72 | 4 | sample_data_offset | Absolute file offset to sample data |
| 76 | 1 | vibrato_speed | Auto-vibrato speed |
| 77 | 1 | vibrato_depth | Auto-vibrato depth |
| 78 | 1 | vibrato_rate | Auto-vibrato rate |
| 79 | 1 | vibrato_waveform | Auto-vibrato waveform (0=sine, 1=square, 2=ramp, 3=random) |

#### Sample Flags (offset 18)

| Bit | Mask | Flag |
|-----|------|------|
| 0 | 0x01 | sample_exists |
| 1 | 0x02 | 16_bit |
| 2 | 0x04 | stereo (only valid for cwtv >= 0x0214) |
| 3 | 0x08 | compressed (IT214/IT215) |
| 4 | 0x10 | loop |
| 5 | 0x20 | sustain_loop |
| 6 | 0x40 | ping_pong_loop |
| 7 | 0x80 | ping_pong_sustain |

#### Convert Byte (offset 46)

| Bit | Mask | Meaning (uncompressed) | Meaning (compressed) |
|-----|------|----------------------|---------------------|
| 0 | 0x01 | Unsigned (on) / Signed (off) | Same — applies post-decompression |
| 1 | 0x02 | Big-endian (on) / Little-endian (off) | N/A |
| 2 | 0x04 | Delta PCM (running sum) | IT215 double-delta (on) / IT214 single-delta (off) |
| 3 | 0x08 | Byte-delta (PTM loader) | N/A |

---

## XM Format (FastTracker 2)

### File Structure

```
┌─────────────────────────────┐
│ Header (336 bytes)          │
│   magic: "Extended Module:" │
│   name: 20 bytes            │
│   tracker: 20 bytes         │
│   version: 2 bytes          │
│   header_size: 4 bytes      │
│   song_length, restart, etc. │
│   patterns: 256 bytes       │
│   instruments: 256 bytes    │
│   flags, tempo, bpm         │
├─────────────────────────────┤
│ Pattern Headers + Data      │
│   (variable per pattern)    │
├─────────────────────────────┤
│ Instrument Headers          │
│   + Sample Data             │
│   (variable per instrument) │
└─────────────────────────────┘
```

### Key Differences from IT

| Feature | IT | XM |
|---------|----|----|
| Max channels | 64 | 32 |
| Max instruments | 255 | 128 |
| Pattern packing | RLE with channel mask | RLE with note/instrument/vol/effect |
| Sample mapping | Per-note sample map | Per-note range (key regions) |
| Envelopes | Volume, panning, pitch | Volume, panning |
| NNAs | Cut/Continue/Off/Fade | Cut/Continue/Off/Fade (parsed from extended header; default NoteCut) |
| Sample compression | IT214/IT215 | Delta-packed (optional ADPCM in some) |
| Frequency mode | Linear or Amiga | Always linear |
| Effect codes | Letters (A-Z, S1x-SFx) | Hex (0x0-0xF, Exx, Fxx) |

### XM Pattern Packing

```
Read header:
  packing_type: u8 (always 0)
  num_rows: u16 (1-256)
  data_size: u16

For each row × channel:
  Read byte. If bit 7 set, this is a packed row:
    Bit 6: note follows
    Bit 5: instrument follows
    Bit 4: volume column follows
    Bit 3: effect follows
    Bit 2: effect parameter follows
  If bit 7 NOT set, it's the note value directly (all 5 fields present)

Note values: 0 = none, 97 = note off
```

### XM Instrument Format

XM instruments use "key assignments" — a 96-byte table mapping each note (relative to
first note) to a sample number:

```
Instrument header (varies):
  - header_size: u32           (offset +0, total header size including this field)
  - name: 22 bytes             (+4)
  - type: u8                   (+26, always 0)
  - num_samples: u16           (+27)
  - If num_samples > 0:
    - sample_key_map: 96 bytes          (+29, note → sample index, 0 = no sample)
    - volume envelope: 12 × 4 = 48 bytes (+125, tick:u16, value:u16 each)
    - panning envelope: 12 × 4 = 48 bytes (+173, tick:u16, value:u16 each)
    - num_volume_points: u8             (+221)
    - num_panning_points: u8            (+222)
    - volume_sustain_point: u8          (+223)
    - volume_loop_start: u8             (+224)
    - volume_loop_end: u8               (+225)
    - panning_sustain_point: u8         (+226)
    - panning_loop_start: u8            (+227)
    - panning_loop_end: u8              (+228)
    - volume_type: u8                   (+229, bitfield)
    - panning_type: u8                  (+230, bitfield)
    - vibrato_type: u8                  (+231)
    - vibrato_sweep: u8                 (+232)
    - vibrato_depth: u8                 (+233)
    - vibrato_rate: u8                  (+234)
    - volume_fadeout: u16               (+235)
    - reserved: u16                     (+237)
    --- Extended fields (OpenMPT) ---
    - nna: u16                          (+241, 0=Cut, 1=Continue, 2=NoteOff, 3=NoteFade)
    - dct: u16                          (+243, 0=Disabled, 1=Note, 2=Sample, 3=Instrument)
    - dca: u16                          (+245, 0=NoteCut, 1=NoteOff, 2=NoteFade)
    - (more extended fields continue...)
    --- End extended fields ---
    Then: sample headers (40 bytes each × num_samples, starting at offset header_size)
    Then: sample data (raw delta-packed PCM)
```

#### Type Byte Bitfields

The volume_type and panning_type bytes encode envelope behavior:

| Bit | Mask | Flag | Description |
|-----|------|------|-------------|
| 0 | 0x01 | enabled | Envelope is active |
| 1 | 0x02 | sustain | Sustain point enabled (holds at sustain until note-off) |
| 2 | 0x04 | loop | Loop region enabled |
| 3-4 | 0x18 | reserved | Reserved |
| 5 | 0x20 | carry | Carry envelope position across note triggers |

#### Notes on XM Instrument Parsing

**Envelopes**: Both volume and panning envelopes are parsed **in a single pass** from the
24-point pool (12 vol + 12 pan). The volume points occupy the first 48 bytes, panning
points the next 48 bytes. The control bytes (num_points, sustain, loop, type) are read
once and split per-envelope. Earlier versions of htrk called `parse_xm_envelope` twice,
which caused the second call to read garbage from beyond the envelope sub-block.

**Carry flag**: Bit 5 of the type byte (`0x20`) enables envelope carry — without it,
every new note on the same channel resets the envelope to its initial state. This was
previously hardcoded to `false`.

**NNA/DCT/DCA**: These extended fields are only read when the instrument header is
large enough (≥247 bytes). If absent, defaults are `NoteCut`, `Disabled`, `NoteCut`
respectively. Previously all were hardcoded regardless of the file. This was the
primary cause of premature sample cutoff in XM playback — files authored with
`NoteOff` or `NoteFade` NNA would abruptly kill voices on re-trigger.

### XM Sample Data

Samples are stored as delta-packed signed values:

```rust
fn decode_xm_sample(data: &[u8], bits: u8) -> Vec<f32> {
    let bytes_per_sample = if bits == 16 { 2 } else { 1 };
    let num_samples = data.len() / bytes_per_sample;

    let mut samples = Vec::with_capacity(num_samples);
    let mut accumulator: i32 = 0;

    for i in 0..num_samples {
        let delta = if bits == 16 {
            let raw = i16::from_le_bytes([data[i*2], data[i*2+1]]);
            raw as i32
        } else {
            data[i] as i8 as i32
        };

        accumulator += delta;

        if bits == 16 {
            samples.push(accumulator as f32 / 32768.0);
        } else {
            samples.push(accumulator as f32 / 128.0);
        }
    }

    samples
}
```

---

## S3M Format (ScreamTracker 3)

### File Structure

```
┌─────────────────────────────┐
│ Header (96 bytes)           │
│   magic at 44: "SCRM"      │
│   name: 28 bytes            │
│   type: 1 byte              │
│   order_count, etc.         │
├─────────────────────────────┤
│ Order List (variable)       │
├─────────────────────────────┤
│ Instrument Pointers         │ (2 bytes each × num_instruments)
├─────────────────────────────┤
│ Pattern Pointers            │ (2 bytes each × num_patterns)
├─────────────────────────────┤
│ Instrument Data             │
│   Parapointers to samples   │
│   (sample headers are 16-bit│
│    offsets × 16 = byte addr)│
├─────────────────────────────┤
│ Pattern Data                │ (packed, similar to IT but simpler)
└─────────────────────────────┘
```

### Key Differences from IT

| Feature | IT | S3M |
|---------|----|----|
| Channels | 64 | 32 (16 digital + 16 AdLib) |
| Envelopes | Yes | No |
| Instruments | Sample maps + envelopes | Samples only (no instrument layer) |
| NNAs | Yes | No (always cut) |
| Sample format | 8/16-bit, compressed | 8/16-bit, uncompressed |
| Default panning | Per-channel | Stereo interleaved (odd=right, even=left) |
| Effect codes | Letters | Hex |
| Volume range | 0-64 | 0-64 |
| Frequency | Linear or Amiga | Amiga (period-based) |

### S3M Pattern Packing

```
For each row:
  Read byte:
    If 0: end of row, advance to next row
    If non-zero:
      Bits 7-6: 00
      Bits 5-0: channel
      Next bytes based on what follows:
        Bit 7 of channel byte is always 0 for S3M
        Byte with bit 7 set = command byte

  Actually, S3M uses this format:
    Read byte b:
    If b == 0: row done
    Channel = b & 0x1F
    What = b >> 5
      bit 0 (0x20): note follows
      bit 1 (0x40): volume follows
      bit 2 (0x80): command + param follow

    Note byte: 0xFF = none, 0xFE = note off, 0-239 = note
    Volume byte: 0-64
    Command byte: effect letter
    Param byte: effect parameter
```

### S3M Note Encoding

S3M uses a note value (not period) directly:

```
note_value = octave * 16 + tone
  tone: 0=C, 1=C#, 2=D, ..., 11=B
  octave: 0-14

So C-4 = 4*16 + 0 = 64 (0x40)
    A-4 = 4*16 + 9 = 73 (0x49)
```

---

## MOD Format (Amiga ProTracker)

### File Structure

```
┌─────────────────────────────┐
│ Header (1084 bytes)         │
│   name: 20 bytes            │
│   sample_info: 31 × 30 bytes│ (sample headers)
│   song_length: 1 byte       │
│   restart: 1 byte           │
│   order_list: 128 bytes     │
│   magic: 4 bytes            │ (e.g., "M.K.")
├─────────────────────────────┤
│ Pattern Data                │
│   Fixed 1024 bytes/pattern  │
│   (4 channels × 256 rows)   │
├─────────────────────────────┤
│ Sample Data                 │
│   Raw 8-bit signed PCM      │
└─────────────────────────────┘
```

### Key Differences from IT

| Feature | IT | MOD |
|---------|----|----|
| Channels | 64 | 4 (or 6/8 with variants) |
| Patterns | Variable rows | Fixed 64 rows |
| Notes | MIDI key | Amiga periods |
| Samples | 8/16-bit | 8-bit only |
| Envelopes | Yes | No |
| Volume per note | Volume column | No (effect Cxx only) |
| Panning | Per-channel | Hard-wired L/R/L/R |
| Effects | Extended | Limited (0-F hex) |

### MOD Pattern Data

Each row is 4 channels × 4 bytes = 16 bytes. Each 4-byte word encodes:

```
Byte 0: [8 bits of sample_number (high)] [4 high bits of period]
Byte 1: [4 low bits of period] [4 bits of sample_number (low)]
Byte 2: Effect type (hex digit)
Byte 3: Effect parameter (hex)

Decoded:
  sample_hi = (byte0 & 0xF0) | ((byte2 & 0xF0) >> 4)  -- Wait, this is wrong

Actually, the standard ProTracker encoding is:
  byte0: [sssspppp]  s=sample high 4 bits, p=period high 4 bits
  byte1: [ppppssss]  p=period low 8 bits total... no

Correct decoding:
  Word = big-endian 32-bit value
  sample = ((word >> 24) & 0xF0) | ((word >> 12) & 0x0F)  -- wait

Let me be precise:
  byte0 = (sample_hi << 4) | (period_hi >> 4)
  byte1 = ((period_hi & 0xF) << 4) | sample_lo
  byte2 = effect
  byte3 = effect_param

Actually, the correct MOD format per note is:
  byte0 = sample_number_high_4 << 4 | period_high_4
  byte1 = period_low_8  (lower 8 bits of 12-bit period)
  Wait, no. Let me look at this more carefully.

Standard ProTracker 4-byte note:
  Bits 31-28: sample number high nibble
  Bits 27-16: 12-bit period
  Bits 15-12: sample number low nibble
  Bits 11-8:  effect number
  Bits 7-0:   effect parameter

  byte0 = (sample_hi << 4) | (period >> 8)
  byte1 = period & 0xFF
  byte2 = (sample_lo << 4) | effect
  byte3 = effect_param

So:
  sample = ((byte0 & 0xF0) << 4) | (byte2 & 0xF0) >> 4  -- hmm
```

### Correct MOD Note Decoding

```rust
fn decode_mod_note(bytes: [u8; 4]) -> (u8, u16, u8, u8) {
    let sample = ((bytes[0] & 0xF0) >> 4) | ((bytes[2] & 0xF0));
    let period = ((bytes[0] & 0x0F) as u16) << 8 | bytes[1] as u16;
    let effect = bytes[2] & 0x0F;
    let param = bytes[3];

    (sample, period, effect, param)
}

fn period_to_note(period: u16) -> Note {
    if period == 0 { return Note::None; }

    // Find closest period in table
    for (key, &p) in PERIOD_TABLE.iter().enumerate() {
        if period >= p {
            return Note::On(key as u8);
        }
    }

    Note::None
}
```

### MOD Sample Header (30 bytes × 31 samples)

| Offset | Size | Field |
|--------|------|-------|
| 0 | 22 | sample_name |
| 22 | 2 | sample_length (words, so × 2 for bytes) |
| 24 | 1 | fine_tune (4-bit signed: 0-7 = 0 to +7, 8-15 = -8 to -1) |
| 25 | 1 | volume (0-64) |
| 26 | 2 | loop_start (words) |
| 28 | 2 | loop_length (words, 0 = no loop) |

---

## Format Conversion Strategy

When loading a non-IT format, we convert to the internal IT-style `Module` structure:

| Source | Conversion |
|--------|-----------|
| MOD → IT | Period → note, 4 ch → expand to 64, no instruments, create instrument per sample |
| S3M → IT | S3M notes → IT notes, samples become instruments with 1:1 mapping, stereo hard-pan → channel panning |
| XM → IT | XM delta samples → raw, XM envelopes → IT envelopes, XM key map → IT sample map, XM NNAs → mapped 1:1 |

When saving in a non-IT format, we apply constraints:

| Target | Constraint |
|--------|-----------|
| IT ← Module | Direct mapping, lossless if module was loaded from IT |
| XM ← Module | No NNAs (must resolve), max 32 channels, max 128 instruments, delta-encode samples |
| S3M ← Module | No envelopes (bake into volumes), max 32 channels, period-based notes |
| MOD ← Module | 4 channels only, 8-bit samples, 31 samples max, no volume column, 64-row patterns |

## Common Parsing Utilities

```rust
// common.rs

fn read_u8(data: &[u8], offset: &mut usize) -> u8 {
    let val = data[*offset];
    *offset += 1;
    val
}

fn read_u16_le(data: &[u8], offset: &mut usize) -> u16 {
    let val = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    val
}

fn read_u16_be(data: &[u8], offset: &mut usize) -> u16 {
    let val = u16::from_be_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    val
}

fn read_u32_le(data: &[u8], offset: &mut usize) -> u32 {
    let val = u32::from_le_bytes([
        data[*offset], data[*offset + 1],
        data[*offset + 2], data[*offset + 3]
    ]);
    *offset += 4;
    val
}

fn read_string(data: &[u8], offset: &mut usize, len: usize) -> String {
    let s = &data[*offset..*offset + len];
    *offset += len;
    String::from_utf8_lossy(s).trim_end_matches('\0').trim_end().to_string()
}

fn read_bool(data: &[u8], offset: &mut usize) -> bool {
    let val = data[*offset] != 0;
    *offset += 1;
    val
}
```
