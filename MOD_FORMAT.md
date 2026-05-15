# MOD File Format Documentation

This document describes the MOD file format as implemented in the ProTracker replay code (v2.3A).

## File Structure Overview

```
Offset  Size    Description
------  ----    -----------
0       20     Song title (null-padded ASCII)
20      950    Sample information (30 samples × 30 bytes)
950     1      Song length (number of positions)
951     1      Restart position
952     128    Pattern table (128 bytes, one per position)
1080    4      File identifier (e.g., "M.K.", "FLT4")
1084    -      Pattern data and sample data
```

## Sample Data Structure (30 bytes each)

Each of the 30 samples has the following structure:

```
Offset  Size    Description
------  ----    -----------
0       22     Sample name (null-padded ASCII)
22      2      Sample length (word, in 16-bit words)
24      1      Finetune value (0-15, or -8 to +7 signed)
25      1      Default volume (0-64)
26      2      Repeat start (word, offset in 16-bit words)
28      2      Repeat length (word, in 16-bit words)
```

**Notes:**
- Sample length is stored as number of 16-bit words. Multiply by 2 for byte length.
- Finetune values: 0-7 = +0 to +7, 8-15 = -8 to -1
- Repeat start of 0 with repeat length of 0 indicates no loop (one-shot sample)
- Volume is 0-64 (0x40)

## Pattern Table

The pattern table (128 bytes starting at offset 952) contains:
- One byte per song position
- Each byte specifies which pattern number to play at that position
- Values 0-127 reference pattern data

## Pattern Data Format

Patterns are stored as 4-channel data, 64 rows per pattern:

```
Row Size: 16 bytes (4 channels × 4 bytes per channel)
Pattern Size: 1024 bytes (64 rows × 16 bytes)
```

Each channel cell (4 bytes):

```
Byte 0: Note (high nibble = note, low nibble = octave)
        0 = no note, otherwise: C=1, C#=2, D=3, ... B=12
        Octave: 0-7, Note: 1-12 (C to B)

Byte 1: Instrument (1-31, 0 = no instrument)

Byte 2: Effect command (hex digit 0-F)

Byte 3: Effect parameter (hex 0x00-0xFF)
```

**Note encoding:** Upper nibble = octave (0-7), Lower nibble = note (1=C, 2=C#, ..., 12=B)
Example: 0x31 = octave 3, note 1 = C3

## File Identifier

The 4-byte identifier at offset 1080 indicates the MOD variant:

```
"M.K."  = ProTracker 4-channel (standard)
"FLT4"  = Startrekker 4-channel
"FLT8"  = Startrekker 8-channel
"6CHN"  = 6-channel MOD
"8CHN"  = 8-channel MOD
"CD81"  = Falcon030 format
```

## Period Table

ProTracker uses a period table to convert note+octave to DMA period values.
The base periods for tuning 0 are:

```
C-0 to B-0:  856, 808, 762, 720, 678, 640, 604, 570, 538, 508, 480, 453
C-1 to B-1:  428, 404, 381, 360, 339, 320, 302, 285, 269, 254, 240, 226
C-2 to B-2:  214, 202, 190, 180, 170, 160, 151, 143, 135, 127, 120, 113
```

Finetune adjusts these values. There are 16 finetune settings (tuning 0-7 and -1 to -8).

## Effect Commands

### Basic Commands (0x0-0xF)

| Cmd | Name              | Description                                    |
|-----|-------------------|------------------------------------------------|
| 0   | Arpeggio          | Quick alternate between base, +x, +y semitones|
| 1   | Pitch Slide Up    | Slide period up by parameter                  |
| 2   | Pitch Slide Down  | Slide period down by parameter                |
| 3   | Tone Portamento   | Slide to target period                        |
| 4   | Vibrato           | Period oscillation                             |
| 5   | Tone+Vol Slide    | Combined tone portamento + volume slide       |
| 6   | Vibrato+Vol Slide | Combined vibrato + volume slide               |
| 7   | Tremolo           | Volume oscillation                            |
| 8   | (unused)          | -                                              |
| 9   | Sample Offset     | Start sample at offset × 256 bytes            |
| A   | Volume Slide      | Adjust volume up/down                         |
| B   | Position Jump     | Jump to song position                          |
| C   | Set Volume        | Set channel volume (0-64)                     |
| D   | Pattern Break     | Break to next pattern at row (BCD: D13 → row 13) |
| E   | Extended          | Sub-commands E0-EF                            |
| F   | Set Speed         | Set speed (1-31) or BPM (32-255)              |

### Extended Commands (E)

| Sub | Name              | Description                                    |
|-----|-------------------|------------------------------------------------|
| E0  | Filter Toggle     | Enable/disable audio filter                    |
| E1  | Fine Pitch Up     | Precise pitch slide up                         |
| E2  | Fine Pitch Down   | Precise pitch slide down                       |
| E3  | Glissando         | Toggle glissando mode                          |
| E4  | Vibrato Control   | Set vibrato waveform                           |
| E5  | Set Finetune      | Set sample finetune                            |
| E6  | Loop Set/Jump     | Set loop point or loop sample                 |
| E7  | Tremolo Control   | Set tremolo waveform                           |
| E8  | (unused)          | -                                              |
| E9  | Retrig Note       | Retrigger sample at interval                   |
| EA  | Volume Fine Up    | Precise volume increase                       |
| EB  | Volume Fine Down  | Precise volume decrease                       |
| EC  | Note Cut          | Cut note after x ticks                        |
| ED  | Note Delay        | Delay note trigger                            |
| EE  | Pattern Delay     | Delay entire pattern                          |
| EF  | Funk It           | Increment funk counter                        |

## Playback Speed

- Speed values 1-6: Set ticks per row (default is 6)
- Speed values 32-255: Set BPM (ticks per minute)
- At 125 BPM with speed 6: 50 rows/second, 8000 rows/minute

## Amiga Audio Hardware

The replay uses four DMA audio channels (0-3):
- Each channel has: AUDxPER (period), AUDxLEN (length), AUDxVOL (volume)
- AUDxDAT register receives sample data
- Loop address stored in AUDxLOOP

## Implementation Notes

From the source code (`protracker.asm`):

1. Sample data starts after: 1084 + (numPatterns × 1024) bytes
2. Pattern count is determined by scanning for highest pattern number in pattern table
3. Sample start addresses are calculated during initialization based on pattern count
4. The period range is 113 (highest) to 856 (lowest note)
5. Minimum period 113, maximum 856 (or 113 with upper bits masked)

## Source Code Reference

See `protracker.asm` for the complete implementation:
- `mt_init` (line 210): Initialization and sample address calculation
- `mt_music` (line 328): Main playback routine
- `mt_PlayVoice` (line 390): Per-channel note processing
- Effect handlers: Lines 590-1292
- Period table: Lines 1303-1367

---

## ProTrackR Implementation Notes

This section documents differences between the generic MOD format specification above and the
ProTrackR R package implementation (`R/06PTModule.r`, `R/09playing_routines.r`).

### File Identifier Support

**Generic spec supports:** "M.K.", "FLT4", "FLT8", "6CHN", "8CHN", "CD81"

**ProTrackR supports only:**
- `"M.K."` - Standard ProTracker (max 64 patterns)
- `"M!K!"` - Extended format (max 100 patterns)

The validity check in `06PTModule.r:19-20` explicitly rejects all other identifiers.
See `R/00constants.r` where `maximumTrackCount <- 4` - no support for 6 or 8-channel MODs.

### Sample Count

- Generic spec: 30 samples
- ProTrackR: **31 samples** (ProTracker 2.3A standard)

This is documented in `06PTModule.r:11` - "We're only being compatible with ProTracker by supporting 31 samples"

### Effect Command Implementation Status

| Command | Generic Spec | ProTrackR Status |
|---------|--------------|------------------|
| 0 (Arpeggio) | Listed | Partially implemented |
| E3 (Glissando) | Listed | **Not implemented** |
| E5 (Set Finetune) | Listed | **Partially implemented** |
| E8 | Listed as unused | Not implemented |
| EF (Funk It) | Listed | **Not implemented** |

All other basic commands (1-7, 9-F) and extended commands (E0, E1, E2, E4, E6, E7, E9, EA, EB, EC, ED, EE)
are implemented.

### Known Test Case Failures

ProTrackR documentation (`R/ProTrackR-package.r:130-141`) notes these tests fail vs. OpenMPT reference:
- AmigaLimitsFinetune.mod
- ArpWraparound.mod
- finetune.mod
- PortaSmpChange.mod
- PTInstrSwap.mod
- PTSwapEmpty.mod

These edge cases indicate some implementation nuances differ from the reference playback.

### Pattern Table Calculation

- Generic spec: Pattern count determined by scanning pattern table for highest pattern number
- ProTrackR: Uses `max(as.numeric(mod@pattern.order)) + 1` (same approach, matches spec)

---

## Propulse Implementation Details

This section documents the specific MOD format support and implementation details in Propulse Tracker, based on its source code (`protracker.player.pas`).

### Supported Variants & Identifiers

Propulse supports several MOD variants, mapping them to its internal ProTracker-compatible engine:

| Identifier | Format Description | Notes |
|------------|--------------------|-------|
| `M.K.` | ProTracker 1.x / 2.x | Standard 4-channel, up to 64 patterns. |
| `M!K!` | ProTracker 2.x | 4-channel, extended to 100 patterns. |
| `FLT4` | StarTrekker | 4-channel. |
| `4CHN` | FastTracker II | 4-channel MODs only. |
| `N.T.` | NoiseTracker 1.0 | Handled as ProTracker. |
| `FEST` | NoiseTracker (alt) | Special finetune and effect handling (see below). |
| `M&K!` | NoiseTracker (alt) | Handled same as `FEST`. |
| (None) | Ultimate SoundTracker | 15 samples (STK format). |

### Legacy & Compression Support

- **PowerPacked (`PP20`):** Propulse includes a built-in decruncher for PowerPacked compressed modules.
- **The Ultimate SoundTracker (STK):**
    - Detected if no standard ID is found and file structure matches 15 samples.
    - **Effect Conversion:**
        - `1xx` (Arpeggio) -> `0xx`
        - `2xx` (Pitch Slide) -> `1xx` (Up) or `2xx` (Down) based on parameter.
        - `Dxx` -> `D00` (Pattern Break) if parameter is 0, else `Axx` (Volume Slide).
    - **Tempo:** STK tempo is calculated from the restart position if it's not 120 (which maps to 125 BPM).

### Special Effect Handling

- **`FEST` (NoiseTracker):**
    - Finetune is 5-bit and inverted: `(0 - (val & 0x1F)) / 2`.
    - `Dxx` (Pattern Break) is always treated as `D00`.
- **`4CHN` (FastTracker II):**
    - Removes `8xx` and `E8x` (panning) commands as they conflict with standard MOD.
    - Removes `F00` speed command (FT2 didn't use `F00` as a stop command in MODs).
- **`E8x` Karplus-Strong (Propulse Extension):** 
    - Implements Karplus-Strong string synthesis by averaging adjacent sample bytes in real-time. This is a non-standard extension.
- **`EFx` Funk It:** 
    - Fully implemented ProTracker "funk" effect that modifies sample data rhythmically.

### Playback Accuracy & Hardware Emulation

- **Engine:** Based on `pt2play` (8bitbubsy), derived from original ProTracker disassembly.
- **Amiga Filters:**
    - 5.2Hz High-pass filter.
    - 4.4kHz Low-pass filter.
    - 3.1kHz "LED" Sallen-Key filter (toggled via `E0x`).
- **Anti-Aliasing:** Uses BLEP (Band-Limited Step) synthesis to reduce aliasing, mimicking Amiga's output characteristics.
- **"Beep Fix":** Automatically zeroes the first two bytes of non-looping samples to prevent the common ProTracker "loop beep".
- **Illegal Loops:** Automatically detects and fixes loops that exceed sample length, adjusting either the loop or the sample length to ensure stability.

### Import Logic

Propulse can import other formats by converting them to its internal ProTracker structure:
- **Impulse Tracker (`.it`)**: Via `ProTracker.Format.IT`.
- **Scream Tracker 3 (`.s3m`)**: Via `ProTracker.Format.S3M`.
- **The Player 6.1a (`.p61`)**: Via `ProTracker.Format.P61`.

---

## HTRK Implementation Status

This section documents which Propulse/ProTracker features HTRK currently implements, how they are
architected, and what remains outstanding.

### Supported File Identifiers

| Identifier | Format | HTRK `ModVariant` | Status |
|------------|--------|-------------------|--------|
| `M.K.` | ProTracker 1.x/2.x | `ProTracker` | Supported |
| `M!K!` | ProTracker 2.x extended | `ProTracker` | Supported |
| `FLT4` | StarTrekker 4ch | `ProTracker` | Supported |
| `FLT8` | StarTrekker 8ch | `ProTracker` | Supported |
| `4CHN` | FastTracker II 4ch | `ProTracker` | Supported |
| `6CHN` | 6-channel | `ProTracker` | Supported |
| `8CHN` | 8-channel | `ProTracker` | Supported |
| `2CHN` | 2-channel | `ProTracker` | Supported |
| `CD81` | Falcon030 | `ProTracker` | Supported |
| `OKTA` | Oktalyzer | `ProTracker` | Supported |
| `16CN` | 16-channel | `ProTracker` | Supported |
| `32CN` | 32-channel | `ProTracker` | Supported |
| `N.T.` | NoiseTracker 1.0 | `NoiseTracker` | Supported |
| `FEST` | NoiseTracker (alt) | `NoiseTracker` | Supported |
| `M&K!` | NoiseTracker (alt) | `NoiseTracker` | Supported |
| *(none)* | Ultimate SoundTracker | `SoundTracker` | Supported (fallback detection) |

Detection logic lives in `src/formats/modfile.rs`:

- `detect_magic()` — matches the 4-byte identifier at offset 1080 and returns channel count +
  `ModMagic` variant (`Standard`, `NoiseTracker`, or `SoundTracker`).
- `detect_stk()` — heuristic fallback for 15-sample STK files with no magic bytes; checks for
  nonzero sample lengths in the first 15 sample descriptors (offset 600 header).

### ModVariant Tracking

The `ModVariant` enum (`src/sequencer/module.rs`) is stored in `ModuleFlags.mod_variant` and set
by the MOD loader:

```
ProTracker    — default; all standard MOD identifiers
NoiseTracker  — N.T., FEST, M&K!
SoundTracker  — 15-sample STK files (no magic)
```

The sequencer engine and loader can branch on this to apply format-specific quirks.

### Effect Commands

#### Universal Effects (all variants)

All basic commands `0`–`F` and extended commands `E0`–`EE` are converted by the loader into the
universal `Effect` enum.  See `convert_effect_pt()` in `src/formats/modfile.rs`.

#### NoiseTracker-Specific Effects

Handled by `convert_effect_nt()`:
- **`Dxx`** is always converted to `PatternBreak { row: 0 }` (ignoring the parameter), matching
  NoiseTracker behaviour.

#### SoundTracker-Specific Effects

Handled by `convert_effect_stk()`:
- **`1xx`** is remapped to `Arpeggio` (STK used `1xx` for arpeggio instead of `0xx`).
- **`2xx`** is remapped to `PortamentoUp` or `PortamentoDown` depending on the parameter sign
  (STK used `2xx` for bidirectional pitch slide instead of `1xx`/`2xx`).
- **`D00`** → `PatternBreak { row: 0 }`.
- **`Dxx` (nonzero)** → `VolumeSlide` (STK used `Dxx` for volume slide instead of pattern break).
- All other effects fall through to the ProTracker converter.

#### MOD-Specific Effects (`ModEffect`)

These live in `src/sequencer/effect.rs` and are dispatched only when the sequencer encounters
`FormatEffect::Mod(...)`:

| ModEffect | Source Command | Implementation |
|-----------|---------------|----------------|
| `Filter(bool)` | `E0x` | Sets `amiga_led_filter` flag on all active voices for the channel. |
| `FunkIt { speed }` | `EFx` | Stores `funk_speed` and resets `funk_pos` in `ChannelState`. |
| `KarplusStrong { param }` | `E8x` | Stores `karplus_param` in `ChannelState` for per-tick processing. |
| `Raw { effect, param }` | (reserved) | Passthrough for unrecognised MOD-specific commands. |

Sequencer dispatch is in `src/audio/sequencer_engine.rs` around line 801, inside the
`FormatEffect::Mod(...)` match branch — this code only runs for MOD files.

### Finetune Decoding

Two decoding functions exist in `src/formats/modfile.rs`:

- `decode_finetune(raw)` — standard ProTracker: 4-bit value `0–7 = +0…+7`, `8–15 = −8…−1`.
- `decode_finetune_noisetracker(raw)` — NoiseTracker/FEST: 5-bit inverted, `(0 − (val & 0x1F)) / 2`.

The loader selects the appropriate decoder based on `ModMagic` variant.

### SoundTracker Tempo Derivation

STK files have no BPM/speed metadata.  The loader derives initial speed from the restart position
byte (offset 951) when it is nonzero and not 120.  When the restart position is 120 or 0, the
default speed of 6 and BPM of 125 are used.  This matches the ProTrackR/Propulse approach.

### Beep Fix

When loading MOD files, the first two bytes of **non-looping** sample data are zeroed to prevent
the common ProTracker "loop beep" artefact.  This happens in the MOD loader
(`src/formats/modfile.rs`, inside the sample construction loop) and only affects MOD playback —
no other format's sample data is touched.

### Illegal Loop Correction

When a loop region extends beyond the sample length, the loader now **corrects** the loop rather
than discarding it:

1. If `loop_start < sample_length`: clamp `loop_length` to `sample_length − loop_start`.
2. If `loop_start >= sample_length`: reset loop entirely (`LoopType::None`).
3. If the corrected `loop_length ≤ 2`: treat as no loop (`LoopType::None`).

This replaces the previous behaviour of silently discarding any loop that didn't fit.

### Amiga LED Filter Emulation

HTRK emulates the Amiga A500's 3.1 kHz Sallen-Key "LED" low-pass filter, toggled by the `E0x`
effect command:

- **`Voice.amiga_led_svf`** (`StateVariableFilter`) — a dedicated SVF instance per voice for the
  LED filter, separate from the existing instrument filter (`Voice.svf`).
- **`Voice.amiga_led_filter`** — per-voice flag, set when `E0x` enables the filter.
- **`SequencerEngine.amiga_led_filter`** — engine-level flag, set to `true` when a MOD module is
  loaded (`module.format == ModuleFormat::MOD`).  The `E0x` effect only toggles the LED filter
  when this flag is true, so XM/S3M/IT playback is unaffected.
- **Mixer** (`src/audio/mixer.rs`) — both render paths apply `voice.amiga_led_svf.process(...)` at
  3100 Hz with Q=0.707 when `amiga_led_filter` is true.

### Playback Architecture (MOD-specific paths)

All MOD-specific behaviour is gated behind one of these mechanisms:

| Gate | Location | Purpose |
|------|----------|---------|
| `ModuleFormat::MOD` | `sequencer_engine.rs` `load_module()` | Sets `amiga_led_filter` flag |
| `FormatEffect::Mod(...)` | `sequencer_engine.rs` line ~801 | Dispatches MOD-only effects |
| `!module.flags.linear_slides` | `sequencer_engine.rs` throughout | Period-domain math (Amiga) vs linear (XM) |
| `!self.use_xm_model` | `sequencer_engine.rs` | MOD-specific volume slide memory, vibrato quirks |
| `ModVariant` in `ModuleFlags` | `modfile.rs` loader | NoiseTracker/STK-specific parsing |
| `voice.amiga_led_filter` | `mixer.rs` both render paths | LED filter audio processing |

None of these code paths are reachable during XM, S3M, or IT playback.

### Not Yet Implemented

| Feature | Priority | Notes |
|---------|----------|-------|
| PowerPacked (`PP20`) decrunching | Low | Would be a pre-loading step in `ModHandler.detect()` / `load()`. |
| BLEP (Band-Limited Step) anti-aliasing | Low | Would require changes to the resampler path in `voice.rs` / `mixer.rs`, gated behind a format flag. |
| Amiga 5.2 Hz high-pass / 4.4 kHz low-pass filters | Low | Static hardware filters (always-on, not togglable via effects). Would need per-voice SVF instances similar to `amiga_led_svf`. |
| Funk It sample-data processing | Medium | `ModEffect::FunkIt` stores speed/position in `ChannelState` but does not yet rhythmically modify sample data. |
| Karplus-Strong synthesis | Medium | `ModEffect::KarplusStrong` stores param in `ChannelState` but does not yet implement the averaging algorithm. |

### FUTURE.MOD Sequencer Audit (2026-05-13)

**File**: `FUTURE.MOD` — "the future" by twilight, M.K. format, 4-channel, 125 BPM, speed 6, 62 orders, 31 patterns, 32 samples.

#### Complete Effect Usage

| Effect | Values Used | Sequencer Location | Status |
|--------|------------|-------------------|--------|
| `TonePortamento` (3xx) | 0, 1, 4 | `seq_engine.rs:837-864` (row), `1509-1518` (tick) | Verified correct |
| `TPorta+Vol` (5xx) | various | `seq_engine.rs:1000-1010` | Verified correct |
| `PortaDown` (2xx) | 1, 4, 15 | `seq_engine.rs:866-880` (row), `apply_portamento_down` `2494-2515` | Verified correct |
| `FinePortaDown` (E2x) | 1 | `seq_engine.rs:938-959` | Verified correct (calls `apply_portamento_down`) |
| `FineVolUp` (EAx) | 1, 2, 3 | `seq_engine.rs:1166-1184` | Verified correct |
| `FineVolDown` (EBx) | 1, 2 | `seq_engine.rs:1187-1204` | Verified correct |
| `SetVol` (Cxx) | 0–64 | `seq_engine.rs` volume column | Verified correct |
| `Offs` (9xx) | 5120 (= param 0x14 × 256) | `seq_engine.rs:648-674` | Verified correct (clamped to `sample.data.len()-1`) |
| `PatLoop` (E6x) | 0, 3, 5, 10 | `seq_engine.rs:1259-1278` (row), `3159-3172` (advance_row) | Verified correct |
| `PatBreak` (Dxx) | 0 | `seq_engine.rs:830-834` | Verified correct |
| `Speed` (Fxx <32) | 3, 6, 12 | `seq_engine.rs` speed/tempo | Verified correct |
| `Tempo` (Fxx ≥32) | 141 | `seq_engine.rs` speed/tempo | Verified correct |

#### Sample Loop Analysis

Many samples are non-looping (`LoopType::None`); 8 samples use forward loops:

| Sample # | Loop Range | Notes |
|----------|-----------|-------|
| 1 | 21576–42912 | Full-range loop |
| 10 | 22004–39024 | |
| 11 | 360–13500 | Short loop body |
| 12 | 3820–22752 | |
| 14 | 7358–21862 | |
| 17 | 30406–37670 | |
| 30 | 4528–9174 | Short loop body |
| 31 | 4542–13454 | |

#### Audit Findings — No Obvious Bugs

All effects used by FUTURE.MOD were traced through the sequencer engine and found to be correctly implemented:

1. **SetSampleOffset**: `calculate_sample_offset()` at line 674 clamps offset to `sample.data.len()-1`, preventing overshoot for short samples.
2. **PatLoop**: `pattern_loop_final_pass` persists across loops within a pattern (correct), cleared on order transitions via `reset_pattern_loop_state()`.
3. **TonePortamento (non-XM)**: Stores `portamento_target_period`/`portamento_target_frequency` without triggering new note (slides from current pitch). `TonePorta(0)` reuses `last_tone_portamento_speed`.
4. **FineVolUp/FineVolDown**: Directly modify `channel_volume` and immediately update voice `base_volume`.
5. **Loop handling**: MOD loader (`modfile.rs:370-418`) correctly parses loop words → bytes, handles illegal loops (clamp), and sets `LoopType::Forward` only when `loop_length > 2`. Mixer (`mixer.rs:110-163`) correctly wraps forward loops and deactivates voices for `LoopType::None`.

#### Remaining Investigation Candidates

If playback still sounds incorrect, the following areas deserve further investigation:

- **Volume reset on new note** (AGENTS.md rule #3): Verify that MOD playback resets channel volume to `sample.default_volume` when a new instrument/sample is triggered, and that volume slides from the previous row don't bleed into the new note.
- **TonePortamento target persistence**: Confirm `portamento_target_period` survives across rows when `TonePorta(0)` is used (speed=0 means "continue previous").
- **PatLoop edge case**: Test with FUTURE.MOD's specific loop values (E63, E65, E6A) to confirm loop counts decrement correctly and `pattern_loop_final_pass` doesn't block legitimate re-loops.
- **Compare against reference player**: Play FUTURE.MOD in OpenMPT/MilkyTracker and compare note-by-note to identify exactly which rows/channels diverge.