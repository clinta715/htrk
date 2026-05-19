# Impulse Tracker (IT) Format in Open Cubic Player

This document describes how Open Cubic Player (OCP) handles the Impulse Tracker (IT) module format.

## Overview

OCP supports IT files through a dedicated player engine (`playit`). Unlike other module formats that are converted to a generic GMD format, IT files have their own specialized loader and player to handle the complex features inherent to the format, such as New Note Actions (NNA), resonant filters, and advanced envelopes.

## File Structure

The IT format uses a header-based structure with offsets (32-bit pointers) to instruments, samples, and patterns.

### Main Header (192 bytes)

| Offset | Size | Name | Description |
|--------|------|------|-------------|
| 0x00 | 4 | sig | Signature "IMPM" (0x4D504D49) |
| 0x04 | 26 | name | Module name |
| 0x1E | 2 | philite | Pattern Highlight (rows) |
| 0x20 | 2 | nords | Number of orders |
| 0x22 | 2 | nins | Number of instruments |
| 0x24 | 2 | nsmps | Number of samples |
| 0x26 | 2 | npats | Number of patterns |
| 0x28 | 2 | cwtv | Created With Tracker version |
| 0x2A | 2 | cmwt | Compatible With Tracker version |
| 0x2C | 2 | flags | Module flags (Stereo, Vol/Pan/Pitch Envelopes, etc.) |
| 0x2E | 2 | special | Special flags (Message, MIDI, etc.) |
| 0x30 | 1 | gvol | Global Volume (0-128) |
| 0x31 | 1 | mvol | Master Volume (0-128) |
| 0x32 | 1 | ispd | Initial Speed |
| 0x33 | 1 | itmp | Initial Tempo |
| 0x34 | 1 | chsep | Channel Separation |
| 0x35 | 1 | pwd | Pitch Wheel Depth |
| 0x36 | 2 | msglen | Message length |
| 0x38 | 4 | msgoff | Message offset |
| 0x3C | 4 | reserved | Reserved |
| 0x40 | 64 | pan | Channel Panning (0-64, 100=Surround, 255=Mute) |
| 0x80 | 64 | vol | Channel Volume (0-64) |

## Instruments (ITI)

IT instruments are 554 bytes long and support complex mapping and envelopes.

- **NNA (New Note Action)**: Cut, Continue, Note Off, Note Fade.
- **DCT (Duplicate Check Type)**: Off, Note, Sample, Instrument.
- **DCA (Duplicate Check Action)**: Cut, Note Off, Note Fade.
- **Envelopes**: Volume, Pan, and Pitch/Filter envelopes. Each has up to 25 nodes, loop, and sustain points.

## Samples (ITS)

IT samples are 80 bytes long (header only).

- **Compression**: Supports IT 2.14 and 2.15 8-bit/16-bit compression (handled in `playit/itsex.c`).
- **Sample Types**: Signed/Unsigned, PCM/Compressed, Mono/Stereo.
- **Looping**: Normal and Ping-pong (Bi-directional) loops.

## Patterns

Patterns are stored in a packed format. Each channel in a row can have:
- **Note**: 0-119 (C-0 to B-9), 254 (Note Cut), 255 (Note Off).
- **Instrument**: 1-255.
- **Volume Column**: 0-64 (Volume), 65-127 (Effect).
- **Command Column**: Command A-Z + Parameter.

### Pattern Decoding
The loader (`playit/itload.c`) decodes the IT packed data into a row-major internal format for the player.

## Playback Features (`playit/itplay.c`)

- **NNA Handling**: IT allows more physical channels than logical channels (up to 256 physical channels).
- **Filters**: Resonant low-pass filters (cutoff and resonance) can be controlled via envelopes or commands.
- **Randomization**: Supports random volume and panning variations per note.
- **Pitch/Frequency**: Supports both Amiga (logarithmic) and Linear frequency slides.

## Implementation Details

- **Loader**: `playit/itload.c`
- **Player**: `playit/itplay.c`
- **Compression**: `playit/itsex.c` (IT Sample EXpansion)
- **Channel Logic**: `playit/itchan.c`
- **Envelope/Instrument Logic**: `playit/itpinst.c`
