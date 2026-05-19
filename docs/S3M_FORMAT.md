# Scream Tracker 3 (S3M) Format in Open Cubic Player

This document describes how Open Cubic Player (OCP) handles the Scream Tracker 3 (S3M) module format.

## Overview

OCP supports S3M files through its Generic Module Player (`playgmd`). The S3M file is loaded and converted into an internal GMD (Generic Module Data) format, which is then played by the GMD engine.

## File Structure

S3M files use a paragraph-based addressing system where offsets are stored as 16-bit "ParaPointers". The actual file offset is calculated as `ParaPointer * 16`.

### Main Header (96 bytes)

| Offset | Size | Name | Description |
|--------|------|------|-------------|
| 0x00 | 28 | name | Module name (space padded) |
| 0x1C | 1 | sig | Signature (0x1A) |
| 0x1D | 1 | type | File type (16 = S3M) |
| 0x1E | 2 | reserved | Reserved (0) |
| 0x20 | 2 | orders | Number of orders in the song |
| 0x22 | 2 | ins | Number of instruments |
| 0x24 | 2 | pats | Number of patterns |
| 0x26 | 2 | flags | Module flags (see below) |
| 0x28 | 2 | cwt | Created With Tracker version |
| 0x2A | 2 | ffv | File Format Version (1=old/signed, 2=new/unsigned) |
| 0x2C | 4 | magic | Magic string "SCRM" |
| 0x30 | 1 | gv | Global Volume (0-64) |
| 0x31 | 1 | is | Initial Speed (3-255) |
| 0x32 | 1 | it | Initial Tempo (32-255) |
| 0x33 | 1 | mv | Master Volume (0-127, bit 7 = stereo/mono) |
| 0x34 | 1 | uc | Ultra-Click removal channels |
| 0x35 | 1 | dp | Default Panning flag (0xFC = use header) |
| 0x36 | 8 | reserved | Reserved |
| 0x3E | 2 | special | Special ParaPointer |
| 0x40 | 32 | channels | Channel settings (0-7: Left, 8-15: Right, 255: Unused) |

### Module Flags

- **Bit 0**: ST2 Vibrato (deprecated)
- **Bit 3**: 0-volume optimizations
- **Bit 4**: Amiga limits
- **Bit 6**: ST3.00 volume slides
- **Bit 7**: Special custom data in file

## Instruments

Instruments are accessed via the Instrument ParaPointer Map. Each instrument header is 80 bytes long.

### PCM Sample Header (Type 1)

| Offset | Size | Description |
|--------|------|-------------|
| 0x00 | 1 | Type (1 = PCM) |
| 0x01 | 12 | DOS filename |
| 0x0D | 3 | MemSeg (ParaPointer to sample data) |
| 0x10 | 4 | Length (bytes) |
| 0x14 | 4 | Loop Start (bytes) |
| 0x18 | 4 | Loop End (bytes) |
| 0x1C | 1 | Volume (0-64) |
| 0x1E | 1 | Packing (0 = PCM, 1 = DP30ADPCM) |
| 0x1F | 1 | Flags (Bit 0: Loop, Bit 1: Stereo, Bit 2: 16-bit) |
| 0x20 | 4 | C2Spd (Frequency for C-4) |
| 0x30 | 28 | Instrument Name |
| 0x4C | 4 | Magic "SCRS" |

### AdLib Instrument (Type 2-7)

S3M supports OPL2/OPL3 FM synthesis instruments. OCP detects these and requires the OPL playback plugin for proper output.

## Patterns

Patterns are stored as packed data. Each pattern begins with a 16-bit length field.

### Data Packing

Rows consist of channel entries. Each entry starts with a `what` byte:
- **Bits 0-4**: Channel number (0-31).
- **Bit 5**: If set, follow with Note (byte) and Instrument (byte).
- **Bit 6**: If set, follow with Volume (byte).
- **Bit 7**: If set, follow with Command (byte) and Info (byte).
- **0x00**: End of row.

#### Note Encoding
- 0xFE: Note Cut
- 0xFF: No note
- Otherwise: `(octave << 4) | note_index`

## Effects Mapping

S3M effects are mapped to GMD internal commands in `playgmd/gmdls3m.c`:

| S3M | Description | GMD Command |
|-----|-------------|-------------|
| A | Set Tempo | `cmdTempo` |
| B | Order Jump | `cmdGoto` |
| C | Pattern Break | `cmdBreak` |
| D | Volume Slide | `cmdVolSlideUp` / `cmdVolSlideDown` |
| E | Pitch Slide Down | `cmdPitchSlideDown` |
| F | Pitch Slide Up | `cmdPitchSlideUp` |
| G | Tone Portamento | `cmdPitchSlideToNote` |
| H | Vibrato | `cmdPitchVibrato` |
| I | Tremor | `cmdTremor` |
| J | Arpeggio | `cmdArpeggio` |
| K | Vib + Vol Slide | `cmdPitchVibrato` + `cmdVolSlide` |
| L | Port + Vol Slide| `cmdPitchSlideToNote` + `cmdVolSlide` |
| O | Sample Offset | `cmdOffset` |
| Q | Retrig | `cmdRetrig` |
| R | Volume Vibrato | `cmdVolVibrato` |
| S | Special | Multiple (see below) |
| T | Set Speed | `cmdSpeed` |
| U | Fine Vibrato | `cmdPitchVibratoFine` |
| V | Global Volume | `cmdGlobVol` |
| X | Panning | `cmdPlayPan` |

### Special Commands (S)

- **S1**: Glissando Control
- **S3**: Vibrato Waveform
- **S4**: Tremolo Waveform
- **S8**: Panning
- **S9**: Surround
- **SB**: Pattern Loop
- **SC**: Note Cut
- **SD**: Note Delay
- **SE**: Pattern Delay

## Implementation Details

- **Loader**: `playgmd/gmdls3m.c` - Handles parsing and conversion to GMD.
- **Playback Engine**: `playgmd/gmdplay.c` - Generic GMD engine that executes the commands.
- **Diagnostics**: `playgmd/dumps3m.c` - A standalone utility for dumping S3M file contents.
