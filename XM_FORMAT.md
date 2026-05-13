# XM (Extended Module) File Format Documentation

This document provides detailed documentation of the XM module format based on the FT2Play source code implementation.

## Overview

The XM format is a tracker module format originally created by FastTracker 2 (FT2). It extends the MOD format with advanced features like:
- Linear frequency tables
- Volume and panning envelopes
- Instrument vibrato
- Sample stereo panning
- More effect commands

## File Structure

```
+------------------+
|    Header        |  60 bytes + headerSize
+------------------+
| Pattern Headers  |  variable
+------------------+
| Pattern Data     |  variable
+------------------+
| Instrument Data  |  variable
+------------------+
```

## Header Format

The XM header (songHeaderTyp) is 60 bytes minimum:

| Offset | Size | Type   | Description |
|--------|------|--------|-------------|
| 0     | 17   | char[] | Signature: "Extended Module: " (17 bytes with trailing space) |
| 17    | 20   | char[] | Song name (null-terminated) |
| 37    | 20   | char[] | Tracker name / program name |
| 57    | 2    | uint16 | Version (0x0102 to 0x0104 typical) |
| 59    | 4    | int32  | Header size (size from offset 60 to end of header) |
| 63    | 2    | uint16 | Song length (number of positions) |
| 65    | 2    | uint16 | Restart position (loop start) |
| 67    | 2    | uint16 | Number of channels (must be even, 2-32) |
| 69    | 2    | uint16 | Number of patterns |
| 71    | 2    | uint16 | Number of instruments (max 128) |
| 73    | 2    | uint16 | Flags |
| 75    | 2    | uint16 | Default tempo (BPM) |
| 77    | 2    | uint16 | Default speed (ticks per row) |
| 79    | 256  | uint8[]| Pattern order table |

### Header Flags

- **bit 0 (LINEAR_FREQUENCIES)**: When set, use linear frequency table instead of Amiga-style

### Version Notes

- Version 0x0102: Old FT2 format (instruments before patterns)
- Version 0x0104 and above: Latest format (patterns before instruments)

## Pattern Data

### Pattern Header

Each pattern has a header (patternHeaderTyp):

| Offset | Size | Type   | Description |
|--------|------|--------|-------------|
| 0     | 4    | int32  | Pattern header size |
| 4     | 1    | uint8  | Packing type (usually 0) |
| 5     | 2    | uint16 | Number of rows |
| 7     | 2    | uint16 | Packed pattern data size |

### Note Packing Format

Notes are stored in a packed format where each byte can contain multiple fields. Each note consists of 5 bytes but may be compressed:

**Uncompressed note (5 bytes):**
```
[note] [instrument] [volume] [effect type] [effect param]
```

**Compressed note:**
- If bit 7 of the first byte is set (0x80), the note is compressed
- Bit 0: Note present (1 byte follows)
- Bit 1: Instrument present (1 byte follows)
- Bit 2: Volume present (1 byte follows)
- Bit 3: Effect type present (1 byte follows)
- Bit 4: Effect param present (1 byte follows)

The note field:
- 0 = no note
- 1-96 = note numbers (C-0 to B-8)
- 97 = key off (note off)

### Internal Note Structure (tonTyp)

```c
typedef struct tonTyp_t {
    uint8_t ton;      // Note number (0-97, 97 = key off)
    uint8_t instr;    // Instrument number (0-128)
    uint8_t vol;     // Volume column value (0-64 or special)
    uint8_t effTyp;  // Effect type (0-255)
    uint8_t eff;     // Effect parameter (0-255)
} tonTyp;
```

Note: The unpack function validates that note values > 97 are cleared to prevent out-of-range reads.

## Instrument Format

### Instrument Header (instrHeaderTyp)

| Offset | Size | Type     | Description |
|--------|------|----------|-------------|
| 0     | 4    | int32    | Instrument size |
| 4     | 22   | char[]   | Instrument name |
| 26    | 1    | uint8    | Instrument type (0 = sample) |
| 27    | 2    | uint16   | Number of samples |
| 29    | 4    | int32    | Sample size (total size of all samples) |
| 33    | 96   | uint8[]  | Sample number for each note (96 = 8 octaves) |
| 129   | 48   | int16[]  | Volume envelope points (12 points, x+y pairs) |
| 177   | 48   | int16[]  | Panning envelope points (12 points, x+y pairs) |
| 225   | 1    | uint8    | Volume envelope number of points |
| 226   | 1    | uint8    | Panning envelope number of points |
| 227   | 1    | uint8    | Volume envelope sustain point |
| 228   | 1    | uint8    | Volume envelope loop start |
| 229   | 1    | uint8    | Volume envelope loop end |
| 230   | 1    | uint8    | Panning envelope sustain point |
| 231   | 1    | uint8    | Panning envelope loop start |
| 232   | 1    | uint8    | Panning envelope loop end |
| 233   | 1    | uint8    | Volume envelope type (bit 0: enabled, bit 1: sustain, bit 2: loop) |
| 234   | 1    | uint8    | Panning envelope type (bit 0: enabled, bit 1: sustain, bit 2: loop) |
| 235   | 1    | uint8    | Vibrato type (0=sine, 1=square, 2=saw, 3=inverted saw) |
| 236   | 1    | uint8    | Vibrato sweep (0-255, how fast vibrato deepens) |
| 237   | 1    | uint8    | Vibrato depth (0-255) |
| 238   | 1    | uint8    | Vibrato rate (0-255) |
| 239   | 2    | uint16   | Fadeout volume (0-32768) |
| 241   | 1    | uint8    | MIDI on flag |
| 242   | 1    | uint8    | MIDI channel |
| 243   | 2    | int16    | MIDI program |
| 245   | 2    | int16    | MIDI bend |
| 247   | 1    | uint8    | Mute |
| 248   | 15   | uint8[]  | Reserved |
| 263   | varies| sample[] | Sample headers (up to 32) |

### Sample Header (sampleHeaderTyp)

| Offset | Size | Type     | Description |
|--------|------|----------|-------------|
| 0     | 4    | int32    | Sample length |
| 4     | 4    | int32    | Loop start |
| 8     | 4    | int32    | Loop length |
| 12    | 1    | uint8    | Default volume (0-64) |
| 13    | 1    | int8     | Fine tune (-128 to +127) |
| 14    | 1    | uint8    | Type (bit 0-1: loop type, bit 4: 16-bit) |
| 15    | 1    | uint8    | Panning (0-255, 128 = center) |
| 16    | 1    | int8     | Relative note number |
| 17    | 1    | uint8    | Reserved |
| 18    | 22   | char[]   | Sample name |

### Sample Type Flags

- **LOOP_OFF (0)**: No loop
- **LOOP_FORWARD (1)**: Forward loop
- **LOOP_PINGPONG (2)**: Bidirectional/pingpong loop
- **SAMPLE_16BIT (16)**: 16-bit sample

### Envelope Flags

- **ENV_ENABLED (1)**: Envelope enabled
- **ENV_SUSTAIN (2)**: Envelope has sustain point
- **ENV_LOOP (4)**: Envelope loops

## Sample Data Format

Sample data is stored as **delta-encoded** 8-bit or 16-bit PCM data:
- 8-bit samples: unsigned (0-255), converted to signed (-128 to +127) during load
- 16-bit samples: signed little-endian
- Data is delta-encoded: each byte represents the difference from the previous sample

During loading, the function `delta2Samp()` converts delta-encoded data to linear PCM.

## Effect Commands

The following effect commands are supported (based on the implementation):

### Volume Column Effects

| Code | Effect |
|------|--------|
| 0-64 | Set volume (0-64) |
| 65-75 | Volume slide up |
| 76-86 | Volume slide down |
| 87 | Fine volume slide down |
| 88 | Fine volume slide up |
| 89 | Set vibrato speed |
| 90-96 | Vibrato |
| 97-127 | Reserved |

### Main Effect Commands

| Code | Name | Description |
|------|------|-------------|
| 0 | Arpeggio | Quick note sequence |
| 1 | Portamento Up | Slide pitch up |
| 2 | Portamento Down | Slide pitch down |
| 3 | Tone Portamento | Slide to note |
| 4 | Vibrato | Pitch oscillation |
| 5 | Tone + Vol Slide | Combined effect |
| 6 | Vib + Vol Slide | Combined effect |
| 7 | Tremolo | Volume oscillation |
| 8 | Set Panning | Set channel pan |
| 9 | Sample Offset | Set sample position |
| A | Volume Slide | Slide volume |
| B | Position Jump | Jump to position |
| C | Set Volume | Set volume |
| D | Pattern Break | Break to next pattern |
| E | Extended Effects | Sub-commands |
| F | Set Speed | Set tick speed |
| G | Set Global Volume | Global volume |
| H | Global Volume Slide | Slide global vol |
| K | Key Off | Trigger release |
| L | Set Envelope Position | Position in envelope |
| M | Set Volume | (same as C) |
| N | Set Panning | (same as 8) |
| P | Panning Slide | Slide panning |
| R | Retrig | Retrigger note |
| S | Special | Extended commands |
| T | Set Tempo | Set BPM |
| X | Set Panning | (same as 8) |
| Y | Vibrato + Pan Slide | Combined effect |
| Z | Filter | Not supported |

### Extended Effects (Ex)

| Sub | Effect |
|-----|--------|
| 0x0 | Retrig |
| 0x1 | Note Cut |
| 0x2 | Note Delay |
| 0x3 | Pattern Delay |
| 0x4 | Envelope Control |
| 0x8 | Fine Portamento Up |
| 0x9 | Fine Portamento Down |
| 0xA | Fine Volume Slide Up |
| 0xB | Fine Volume Slide Down |
| 0xC | Set Vibrato Waveform |
| 0xD | Set Tremolo Waveform |
| 0xE | Set Panbrello Waveform |

## Loading Order

### Format v1.02 (Old):
1. Read header
2. Read instrument headers
3. Read pattern data
4. Read sample data

### Format v1.04+ (Latest):
1. Read header
2. Read pattern data
3. Read instrument headers
4. Read sample data

## Validation Rules

From the source code, these validation checks are performed:

- Version must be 0x0102 to 0x0104
- Channels must be 2-32 (even number)
- Patterns max 256
- Instruments max 128
- Samples per instrument max 16
- Header size must be at least 4 bytes
- Sample data is delta-decoded during loading

## Default Values

- Default speed: 125 ticks per row (if header value is 0)
- Default tempo: 6 (if header value is 0)
- Default sample volume: 64
- Default sample panning: 128 (center)
- Default channel panning: 128 (center)
- Default global volume: 64

## Implementation Notes

From the FT2Play implementation:

1. **Sample Interpolation Tap**: Sample buffers are allocated with +2 extra bytes for interpolation tap fix
2. **Loop Fix**: When both forward and pingpong loop bits are set, pingpong takes precedence but interpolation uses forward calculation
3. **Fadeout Range**: FT2 uses 0-32768 for fadeout (not 0-65536 as sometimes documented)
4. **Vibrato Sweep**: If sweep > 0, amplitude starts at 0 and increases; otherwise starts at full depth
5. **Empty Instruments**: Instrument 0 is reserved as placeholder for empty instruments

## HTRK Implementation Status

### Module Flags

The XM loader (`src/formats/xm.rs`) sets these flags:

```rust
ModuleFlags {
    stereo: true,           // XM is always stereo-capable
    use_instruments: true,  // XM uses instrument mode
    linear_slides: true,    // Linear frequency slides
    old_effects: false,     // Not old-style MOD effects
    xm_envelope_model: true,  // Use XM envelope behavior
    xm_period_model: true,   // Use XM period calculations
    ..ModuleFlags::default()
}
```

### Constants

```rust
const XM_HEADER_MAGIC: &[u8; 17] = b"Extended Module: ";
const XM_FILE_TYPE_MARKER: u8 = 0x1A;
const XM_VERSION: u16 = 0x0104;
const XM_MAX_CHANNELS: usize = 32;
const XM_MAX_ENVELOPE_POINTS: usize = 12;
```

### Sample Rate

All XM samples are loaded with a default sample rate of **8363 Hz** (the classic FT2 sample rate).

### Delta Decoding

The HTRK implementation uses delta decoding:
- 8-bit: `acc = acc.wrapping_add(byte)` then cast to i8
- 16-bit: `acc = acc.wrapping_add(delta)` then cast to i16

### Key Mapping

The sample key map is extended to 120 entries (beyond the standard 96) for compatibility, but only indices 0-95 are populated from the file.

### Effect Architecture

XM effects use `Effect::FormatSpecific(FormatEffect::Xm(XmEffect::...))` for XM-specific variants that differ from the universal `Effect` enum:

| XmEffect Variant | Used For |
|-----------------|----------|
| `SetSampleOffset(u16)` | XM 9xx — offset in bytes (same as universal but format-tagged) |
| `KeyOff { delay }` | XM Kxx — key off with optional delay |

All other XM effects map directly to the universal `Effect` enum (Arpeggio, PortamentoUp, TonePortamento, VolumeSlide, etc.).

### Volume Column

The XM volume column is decoded via `decode_xm_volume_column()` into standard effects:

| Volume Range | Effect |
|-------------|--------|
| 0x10-0x50 | `SetVol` (0-64) |
| 0x60-0x6F | `VolSlideDown` |
| 0x70-0x7F | `VolSlideUp` |
| 0x80-0x8F | `VolFineSlideDown` |
| 0x90-0x9F | `VolFineSlideUp` |
| 0xA0-0xAF | Vibrato speed |
| 0xB0-0xBF | Vibrato depth |
| 0xC0-0xCF | `SetPanPosition` |
| 0xD0-0xDF | Pan slide left |
| 0xE0-0xEF | Pan slide right |
| 0xF0-0xFF | `TonePortamento` |

### Save/Export

Full XM save support via `save_module()` (`xm.rs:872`):
- Writes v1.04 format with proper header, order list, patterns, instruments, and samples
- Re-encodes universal effects back to XM effect codes via `encode_xm_effect()`
- Re-encodes volume column effects via `encode_xm_volume_column()`
- Delta-encodes sample data for output (8-bit and 16-bit)
- Writes complete instrument headers with envelopes, key maps, and sample headers

### Sequencer Integration

XM playback uses the `use_xm_model` flag in `SequencerEngine` to gate XM-specific behavior:

| Gate | Location | Purpose |
|------|----------|---------|
| `self.use_xm_model` | `sequencer_engine.rs` throughout | XM period model, envelope processing, NNA handling |
| `module.flags.linear_slides` | `sequencer_engine.rs` | Linear frequency slides vs Amiga period |
| `module.flags.xm_envelope_model` | `sequencer_engine.rs` | XM envelope sustain/release behavior |
| `FormatEffect::Xm(...)` | `sequencer_engine.rs` ~line 790 | XM-specific effect dispatch |
| `FormatEffect::Xm(XmEffect::KeyOff)` | `sequencer_engine.rs` | Key off with delay |

### Note Table

| Value | Note | Octave |
|-------|------|--------|
| 1 | C | 0 |
| ... | ... | ... |
| 12 | B | 0 |
| 13 | C | 1 |
| ... | ... | ... |
| 96 | B | 7 |
| 97 | Note Off | - |

### Common Tracker Names

| Tracker | Name String |
|---------|-------------|
| Fast Tracker II v2.x | `"FastTracker v2.00"` |
| Fast Tracker II v1.x | `"FastTracker 2.00"` |
| Modplug Tracker | `"ModPlug Tracker"` |
| OpenMPT | `"OpenMPT x.xx"` |

### References

- **Fast Tracker II:** Original tracker that created the format
- **OpenMPT:** Extensive XM format documentation
- **XM format specs:** Various community documentation
- **FT2Play source:** Reference C implementation used for spec verification