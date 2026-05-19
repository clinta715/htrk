# Ultra Tracker (.ULT) Format

Ultra Tracker is a PC tracker format developed by FreeJack of The Elven Nation.

## Identification
Magic string at the beginning of the file: `MAS_UTrack_V00x`
- `V001`: Version < 1.4
- `V002`: Version 1.4
- `V003`: Version 1.5
- `V004`: Version 1.6

## Structure

### File Header
| Offset | Size | Description |
|--------|------|-------------|
| 0      | 15   | Magic string `MAS_UTrack_V00x` |
| 15     | 32   | Song title (ASCII) |
| 47     | 1    | Message size (number of 32-byte lines) |

If message size > 0, it is followed by `msgsize * 32` bytes of text.

### Module Data
- **Number of Samples**: 1 byte.
- **Instruments**: Followed by instrument headers.

### Instrument Header (repeated for each sample)
| Size | Description |
|------|-------------|
| 32   | Instrument name |
| 12   | DOS filename |
| 4    | Loop start (little endian) |
| 4    | Loop end (little endian) |
| 4    | Sample size start (little endian) |
| 4    | Sample size end (little endian) |
| 1    | Volume |
| 1    | Bidi-loop flags |
| 2    | C2SPD (Version >= 1.6) |
| 2    | Finetune |

### Song Structure
- **Orders**: 256 bytes (0xFF terminated).
- **Channels**: 1 byte (number of channels - 1).
- **Patterns**: 1 byte (number of patterns - 1).
- **Panning**: If version >= 1.5, `channels` bytes of panning positions (0-15).

### Patterns
Patterns are stored by channel. Each channel contains 64 rows of events.
Events can be RLE compressed using the `0xFC` repeat code.

## Effects Mapping
| ULT Effect | Description | XMP Internal Effect |
|------------|-------------|---------------------|
| 0x03       | Tone Portamento | FX_ULT_TPORTA |
| 0x09       | Sample Offset | FX_SAMPLE_OFFSET |
| 0x0B       | Panning | FX_SETPAN |
| 0x0F       | Speed/BPM | FX_ULT_TEMPO |
