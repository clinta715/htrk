# Module Format Support Documentation

This document describes the module formats supported by the htrk tracker and its playback engine.

## Legacy Format Support (Loading & Playback)

htrk loads and plays four legacy module formats, plus its own native HTK format. Legacy formats are converted into the universal internal representation (the `Module`, `Effect`, `Instrument`, `Sample` types) during loading.

### 1. MOD (ProTracker / Noisetracker / etc.)
*   **Identification:** Recognizes signatures like `M.K.`, `M!K!`, `FLT4`, `OCTA`, and various `xCHN`/`xxCH` (up to 32 channels).
*   **Instruments:** Supports up to 31 instruments.
*   **Samples:** 8-bit signed PCM.
*   **Effects:** Full support for standard MOD effects (0-F), including the ProTracker `E` command set.

### 2. S3M (Scream Tracker 3)
*   **Identification:** Recognizes the `SCRM` signature at offset 0x2C.
*   **Channels:** Supports up to 32 channels.
*   **Samples:** 8-bit unsigned PCM.
*   **Effects:** Maps S3M commands (A-Z) to internal C2M effects. Includes support for:
    *   Extra-fine slides (S3M commands E and F).
    *   Volume slides.
    *   S3M special commands (S-commands).
*   **GUS Specific:** Handles GUS memory management for samples.

### 3. XM (FastTracker II)
*   **Identification:** Recognizes the `Extended Module:` header.
*   **Version Support:** Requires XM version 1.04 or higher.
*   **Instruments:** Supports extended instruments with multiple samples and envelopes.
*   **Envelopes:** Supports Volume and Panning envelopes (up to 12 points each).
*   **Frequency Tables:** Supports both Amiga (logarithmic) and Linear frequency tables.
*   **Volume Column:** Full support for the XM volume column commands.

### 4. IT (Impulse Tracker)
*   **Identification:** Recognizes the `IMPM` magic at offset 0x00.
*   **Instruments:** Full support for IT instrument data (IMPI chunks), including:
    *   New Note Action (NNA): Note Cut, Continue, Note Off, Note Fade.
    *   Duplicate Check Type/Action: per-note, per-sample, or per-instrument.
    *   Four envelope types: Volume, Panning, Pitch, and Filter.
    *   Filter cutoff, resonance, and random cutoff.
    *   Instrument vibrato (type, sweep, depth, rate).
    *   Pitch pan separation/center.
*   **Samples:** Full IMPS chunk parsing, including:
    *   8-bit and 16-bit samples.
    *   Stereo interleaved samples.
    *   IT214/IT215 compressed sample data (bitstream decompression).
    *   Delta PCM, unsigned, and big-endian raw formats.
    *   Sustaining loops.
    *   Per-sample vibrato parameters.
*   **Effects:** Full IT command set (0-23), including:
    *   Extended effect column (M-commands: fine slides, glissando, vibrato/tremolo waveform, pattern loop, panning, retrigger, note delay/cut).
    *   Volume column commands (105-255): fine volume slides, portamento, vibrato, panning slides, tone portamento.
    *   Global volume, panbrello, envelope position.
*   **Channels:** Supports up to 64 channels.
*   **Message:** Embedded song message (IT special flag).

---

## Universal Effect Set

All formats are decoded into a single `Effect` enum. Every loader translates its native commands into these variants. The sequencer processes effects from this unified representation regardless of the original format.

| Variant | Description |
| :--- | :--- |
| `None` | No effect |
| `Arpeggio { note1, note2 }` | Semi-tone arpeggio (two additional notes) |
| `PortamentoUp { speed }` | Slide pitch up |
| `PortamentoDown { speed }` | Slide pitch down |
| `TonePortamento { speed }` | Glide to target note pitch |
| `Vibrato { speed, depth }` | Oscillating pitch modulation |
| `TonePortamentoVolumeSlide { up }` | Combined portamento + volume slide |
| `VibratoVolumeSlide { up }` | Combined vibrato + volume slide |
| `Tremolo { speed, depth }` | Oscillating volume modulation |
| `SetPanning { pan }` | Set channel pan position (0-255) |
| `SetSampleOffset { offset }` | Jump to sample playback offset |
| `VolumeSlide { up, down }` | Slide volume up or down |
| `PositionJump { order }` | Jump to song order |
| `SetVolume { volume }` | Set channel volume (0-64) |
| `PatternBreak { row }` | Break to next pattern at row |
| `ExtendedEffect { param }` | Raw extended effect parameter byte |
| `SetSpeed { speed }` | Set ticks per row (< 32) |
| `SetTempo { bpm }` | Set tempo in BPM (>= 32) |
| `SetGlobalVolume { volume }` | Set global module volume |
| `GlobalVolumeSlide { up, down }` | Slide global volume |
| `SetEnvelopePosition { tick }` | Jump instrument envelope to tick |
| `Panbrello { speed, depth }` | Oscillating panning modulation |
| `PatternDelay { ticks }` | Delay pattern playback by ticks |
| `SetPanPosition { pan }` | Set pan position (legacy 0-15 mapped) |
| `PanningSlide { speed }` | Slide panning position |
| `GlissandoControl { on }` | Enable/disable glissando (discrete semitones) |
| `VibratoWaveform { waveform }` | Set vibrato waveform (0=sine, 1=square, 2=ramp, 3=random) |
| `SetFineTune { tune }` | Set sample fine-tune |
| `PatternLoop { count }` | Loop pattern section; 0 = set loop start |
| `TremoloWaveform { waveform }` | Set tremolo waveform (same encoding as vibrato) |
| `SetPanning16 { pan }` | Set panning with 4-bit precision (0-255 << 4) |
| `Retrigger { interval }` | Retrigger note every N ticks |
| `NoteCutAfter { ticks }` | Cut note after N ticks |
| `NoteDelay { ticks }` | Delay note trigger by N ticks |
| `FinePortamentoUp { speed }` | Fine slide up |
| `FinePortamentoDown { speed }` | Fine slide down |
| `FineVolumeSlideUp { amount }` | Fine volume slide up |
| `FineVolumeSlideDown { amount }` | Fine volume slide down |
| `Tremor { ontime, offtime }` | Rapid volume on/off oscillation |
| `VolSetVolume { vol }` | Volume column: set volume (0-64) |
| `VolSlideUp { amount }` | Volume column: slide up |
| `VolSlideDown { amount }` | Volume column: slide down |
| `VolFineSlideUp { amount }` | Volume column: fine slide up |
| `VolFineSlideDown { amount }` | Volume column: fine slide down |
| `VolPortamento { speed }` | Volume column: tone portamento |
| `VolVibrato { speed }` | Volume column: vibrato speed |
| `SetFilterCutoff { cutoff }` | Set filter cutoff frequency (0-65535) |
| `SetFilterResonance { resonance }` | Set filter resonance (0-255) |
| `SetFilterType { filter_type }` | Set filter type (LowPass, HighPass, BandPass, Notch) |
| `FilterCutoffSlide { amount }` | Slide filter cutoff |
| `SetSendLevel { send_index, level }` | Set send bus level |
| `SetSendBusParam { bus, param, value }` | Set send FX bus parameter |
| `FormatSpecific(FormatEffect)` | Format-specific effect (see sub-enums for XM, MOD, S3M, IT) |

### Format-Specific Effects

Effects that have no equivalent in the universal set are preserved as `FormatSpecific`:

| Format | Variant | Purpose |
| :--- | :--- | :--- |
| MOD | `ModEffect::Filter(bool)` | Toggle Amiga LED filter |
| MOD | `ModEffect::FunkIt { speed }` | ProTracker E8x FunkIt / ReTrance |
| MOD | `ModEffect::KarplusStrong { param }` | ProTracker E8x Karplus-Strong emulation |
| MOD | `ModEffect::Raw { effect, param }` | Any unmapped MOD effect code |
| XM | `XmEffect::SetSampleOffset(u16)` | XM sample offset (9xx) |
| XM | `XmEffect::KeyOff { fade_rate }` | XM instrument key-off (Gxx) |
| XM | `XmEffect::Raw { effect, param }` | Any unmapped XM effect code |
| S3M | `S3mEffect::SetSampleOffset(u16)` | S3M sample offset (Oxx) with full 24-bit address |
| S3M | `S3mEffect::Raw { effect, param }` | Any unmapped S3M effect code |
| IT | `ItEffect::SetSampleOffset(u16)` | IT sample offset (Oxx) |
| IT | `ItEffect::Raw { effect, param }` | Any unmapped IT effect code |

---

## HTK Native Format (htrk)

The HTK format (`*.htk`) is htrk's native serialization format. It uses a simple header followed by a bincode-encoded Rust structure, providing lossless round-trip for all internal data.

### File Structure

| Offset | Size | Field | Description |
| :--- | :--- | :--- | :--- |
| 0x00 | 4 | Magic | `"HTRA"` (0x41525448) |
| 0x04 | 4 | Version | Little-endian u32. Current version = 4 |
| 0x08 | 4 | Flags | Little-endian u32 (reserved, currently 0) |
| 0x0C | rest | Payload | Bincode-serialized `Module` struct |

Detection relies on the `HTRA` magic at offset 0. The version field enables forward-compatible loading: loaders reject versions higher than the known maximum.

### Module Structure

The bincode payload is a flat serialization of the `Module` struct:

| Field | Type | Description |
| :--- | :--- | :--- |
| `name` | String | Module title |
| `message` | Option\<String\> | Embedded song message (or null) |
| `format` | ModuleFormat | Always `HTK` for native files |
| `_version` | u16 | Tracker version that saved the file |
| `tracker_name` | String | Name of the tracker |
| `order_list` | Vec\<u8\> | Pattern order list (max 256 entries) |
| `patterns` | Vec\<Pattern\> | Array of pattern data |
| `instruments` | Vec\<Instrument\> | Array of instruments (index 0 is empty default) |
| `samples` | Vec\<Sample\> | Array of samples (index 0 is empty default) |
| `initial_bpm` | u16 | Default tempo in BPM |
| `initial_speed` | u8 | Default ticks per row |
| `initial_global_volume` | u8 | Default global volume (0-128) |
| `initial_mixing_volume` | u8 | Default master mix volume |
| `channel_panning` | Vec\<u8\> | Per-channel default panning (0-255) |
| `channel_volume` | Vec\<u8\> | Per-channel default volume (0-64) |
| `flags` | ModuleFlags | Module configuration flags |
| `send_bus_config` | [SendEffectType; 4] | Send FX bus routing |
| `send_return_levels` | [f32; 4] | Send FX return levels (0.0-1.0) |

#### ModuleFlags

| Field | Type | Description |
| :--- | :--- | :--- |
| `stereo` | bool | Enable stereo output |
| `use_instruments` | bool | Use instrument layers (vs. direct sample mapping) |
| `linear_slides` | bool | Use linear frequency table (XM mode) |
| `old_effects` | bool | Use old (pre-IT2.15) effect behaviour |
| `compatible_gxx` | bool | Use FT2-compatible Gxx handling |
| `midi_enabled` | bool | MIDI macro control enabled |
| `request_embed` | bool | Embed sample data request flag |
| `fast_volume_slides` | bool | Process volume slides every tick |
| `xm_envelope_model` | bool | Use XM-style envelope processing |
| `xm_period_model` | bool | Use XM-style period calculation |
| `mod_variant` | ModVariant | ProTracker / NoiseTracker / SoundTracker |
| `compatible_tracker_version` | u16 | Original tracker compatibility version (0 if not IT) |
| `panning_separation` | u8 | Stereo separation (0-128, 128 = full) |

#### SendEffectType

| Variant | Description |
| :--- | :--- |
| `None` | No effect |
| `Delay` | Tempo-synced stereo delay with feedback filter |
| `Reverb` | Schroeder/Moorer reverb |
| `Chorus` | Stereo chorus |
| `Flanger` | Stereo flanger |
| `Phaser` | Stereo phaser |

### Pattern Encoding

Each pattern serializes as:

| Field | Type | Description |
| :--- | :--- | :--- |
| `num_rows` | usize | Number of rows in this pattern |
| `data` | Vec\<Vec\<Cell\>\> | Row-major array [num_rows][64 channels] |

#### Cell Structure

| Field | Type | Description |
| :--- | :--- | :--- |
| `note` | Note | Note event |
| `instrument` | Option\<u8\> | Instrument number (1-based, None = no change) |
| `volume` | Option\<u8\> | Volume value (0-64, None = no change) |
| `volume_effect` | Option\<Effect\> | Volume column effect (decoded from legacy vol byte) |
| `effect` | Effect | Effect column command |

#### Note Enum

| Variant | Description |
| :--- | :--- |
| `None` | No note |
| `On(key)` | Note-on with MIDI key (0-119, where 60 = C-4) |
| `Off` | Note-off |
| `Cut` | Note-cut (immediate silence) |
| `Fade` | Note-fade (begin fade-out) |

### Instrument Structure

| Field | Type | Description |
| :--- | :--- | :--- |
| `name` | String | Instrument name |
| `sample_map` | [u8; 120] | Key-to-sample mapping (note 0-119 -> sample index) |
| `note_map` | [u8; 120] | Key-to-note mapping (note transposition) |
| `volume_envelope` | Option\<Envelope\> | Volume envelope (0-64) |
| `panning_envelope` | Option\<Envelope\> | Panning envelope (0-64) |
| `pitch_envelope` | Option\<Envelope\> | Pitch envelope (cents) |
| `filter_envelope` | Option\<Envelope\> | Filter cutoff envelope |
| `fade_out` | u16 | Fade-out rate (0-65535) |
| `nna` | NewNoteAction | Note cut / continue / note off / note fade |
| `duplicate_check_type` | DuplicateCheckType | Disabled / Note / Sample / Instrument |
| `duplicate_check_action` | DuplicateCheckAction | Note cut / note off / note fade |
| `pitch_pan_separation` | i8 | Pitch-to-pan separation (-128 to 127) |
| `pitch_pan_center` | u8 | Center note for pitch-pan |
| `global_volume` | u8 | Instrument global volume (0-128) |
| `filter_cutoff` | u16 | Default filter cutoff (0-65535) |
| `filter_resonance` | u8 | Default filter resonance (0-255) |
| `filter_type` | FilterType | LowPass / HighPass / BandPass / Notch |
| `random_volume` | u8 | Random volume variation (0-255) |
| `random_panning` | u8 | Random panning variation (0-255) |
| `filter_random_cutoff` | u8 | Random cutoff variation (0-255) |
| `vib_type` | u8 | Instrument auto-vibrato type |
| `vib_sweep` | u8 | Vibrato sweep (ramp-up rate) |
| `vib_depth` | u8 | Vibrato depth |
| `vib_rate` | u8 | Vibrato rate |

#### Envelope Structure

| Field | Type | Description |
| :--- | :--- | :--- |
| `points` | Vec\<EnvelopePoint\> | Envelope nodes (max 25) |
| `sustain_point` | Option\<usize\> | Sustain loop start index |
| `loop_start` | Option\<usize\> | Envelope loop start index |
| `loop_end` | Option\<usize\> | Envelope loop end index |
| `flags` | EnvelopeFlags | Enabled / Sustain / Loop / Carry |

Each `EnvelopePoint` has `tick: u16` (position in ticks) and `value: u8` (amplitude 0-64 or -32..+32 for pitch).

### Sample Structure

| Field | Type | Description |
| :--- | :--- | :--- |
| `name` | String | Sample name |
| `data` | Vec\<f32\> | Normalized float samples (-1.0 to 1.0) |
| `sample_rate` | u32 | Base sample rate in Hz |
| `bits_per_sample` | u8 | Original bit depth (8 or 16) |
| `loop_type` | LoopType | None / Forward / PingPong / Backward |
| `loop_start` | usize | Loop start offset (in samples) |
| `loop_end` | usize | Loop end offset (in samples) |
| `default_volume` | u8 | Default volume (0-64) |
| `default_panning` | u8 | Default panning (0-255, 32 = center legacy) |
| `global_volume` | u8 | Sample global volume (0-64) |
| `relative_note` | i8 | Relative transpose in semitones |
| `fine_tune` | i8 | Fine pitch adjustment |
| `vibrato_speed` | u8 | Auto-vibrato speed |
| `vibrato_depth` | u8 | Auto-vibrato depth |
| `vibrato_rate` | u8 | Auto-vibrato rate |
| `vibrato_waveform` | VibratoWaveform | Sine / Square / Ramp / Random |
| `_flags` | SampleFlags | Metadata flags (stereo, 16-bit, compressed, trailing byte) |

---

## Technical Details

### Frequency Calculation
*   **Amiga (period) model:** Uses the classic Amiga period table (`PERIOD_TABLE` in `note.rs`) with 108 entries spanning C-1 to B-3. Periods are converted to frequency via `period_to_frequency()`. Used for MOD and non-linear XM.
*   **Linear frequency model (XM/IT):** Uses FT2-compatible linear period tables (`AMIGA_PERIODS`, `LINEAR_PERIODS` in `period.rs`) with 1936 entries. Frequency scales linearly with note value for consistent pitch ratios across octaves.
*   **Flag control:** The `linear_slides` flag in `ModuleFlags` selects the model; `xm_period_model` enables XM-specific period calculation in the sequencer.

### Frequency Tables
*   `PERIOD_TABLE`: 108-entry Amiga period table (C-1 to B-3), used for MOD playback and legacy compatibility.
*   `AMIGA_PERIODS` / `LINEAR_PERIODS`: Lazy-initialized FT2-compatible tables of 1936 entries each, used for XM and IT playback.

### Software Mixer
*   All mixing is performed in software (no hardware acceleration).
*   Supports 8-bit and 16-bit sample depths.
*   Supports stereo interleaved samples (IT format).
*   Resampling: Per-sample interpolation via the resampler module.
*   Volume ramping between notes to prevent clicks.

### Filter Engine
*   State-variable filter (SVF) per voice.
*   Filter types: Low-pass, High-pass, Band-pass, Notch.
*   Per-instrument filter cutoff and resonance defaults.
*   Filter cutoff slides (continuous modulation).
*   Filter envelope modulation (IT format).
*   Legacy Amiga LED filter emulation for MOD format (hardware low-pass toggle).

### Send Effects
*   Four independent send/return buses.
*   Per-channel send level routing.
*   Available effect types:
    *   **Delay:** Tempo-synced stereo delay with feedback damping and SVF filter in feedback path.
    *   **Reverb:** Schroeder-Moorer reverb with adjustable decay and diffusion.
    *   **Chorus:** Stereo chorus with modulated delay lines.
    *   **Flanger:** Stereo flanger with feedback.
    *   **Phaser:** Stereo phaser with adjustable stages.
*   Each bus type exposes parameters that can be automated via the sequencer.

### Channel Model
*   Up to 64 logical channels.
*   Up to 256 simultaneous voices (polyphony).
*   Per-channel: separate audio filters, send levels, panning, and volume.
*   Voice stealing with configurable priority.
