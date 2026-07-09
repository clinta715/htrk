# Data Model

## Core Types

### Note

Represents a musical note or special note action within a pattern cell.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Note {
    On(u8),       // Note-on: MIDI key number 0-127 (C-0 = 0, C-4 = 60, B-9 = 127)
    Off,          // Note-off: begin release phase of envelope
    Cut,          // Note-cut: immediately silence the voice
    Fade,         // Note-fade: begin fade-out
    None,         // No note event
}

impl Note {
    fn from_tone_octave(tone: u8, octave: u8) -> Note {
        Note::On(octave * 12 + tone)
    }

    fn tone(&self) -> Option<u8> {
        match self {
            Note::On(key) => Some(key % 12),
            _ => None,
        }
    }

    fn octave(&self) -> Option<u8> {
        match self {
            Note::On(key) => Some(key / 12),
            _ => None,
        }
    }

    fn frequency(&self) -> Option<f64> {
        match self {
            Note::On(key) => Some(440.0 * 2.0_f64.powf((*key as f64 - 69.0) / 12.0)),
            _ => None,
        }
    }
}
```

#### Note Display

Notes are displayed in traditional tracker notation:

```
Display: "C-4", "C#4", "D-4", ..., "A#4", "B-4", "C-5", ...
Tone:     0      1      2    ...  10      11     0
Octave:   4      4      4    ...   4       4     5
```

Special displays: `"---"` (None), `"^^^"` (NoteOff), `"==="` (NoteCut), `"~~~"` (NoteFade)

#### Tone Names

```rust
const TONE_NAMES: [&str; 12] = [
    "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"
];
```

### Period Table (Amiga Frequency Conversion)

Used for MOD format compatibility. In classic trackers, notes are represented as
"periods" (divisors of the Amiga hardware clock) rather than direct frequencies.

```rust
// Period table: period[tone + octave * 12]
// Lower period = higher pitch
// Used internally for MOD/S3M compatibility and vibrato calculations
const PERIOD_TABLE: [u16; 108] = [
    // Octave 0 (very low, rarely used)
    1712, 1616, 1524, 1440, 1358, 1280, 1208, 1140, 1076, 1016, 960, 906,
    // Octave 1
    856, 808, 762, 720, 678, 640, 604, 570, 538, 508, 480, 453,
    // Octave 2
    428, 404, 381, 360, 339, 320, 302, 285, 269, 254, 240, 226,
    // Octave 3
    214, 202, 190, 180, 170, 160, 151, 143, 135, 127, 120, 113,
    // Octave 4 (default octave)
    107, 101, 95, 90, 85, 80, 75, 71, 67, 63, 60, 56,
    // Octave 5
    53, 50, 47, 45, 42, 40, 37, 35, 33, 31, 30, 28,
    // Octave 6
    27, 25, 24, 22, 21, 20, 19, 18, 17, 16, 15, 14,
    // Octave 7
    13, 13, 12, 12, 11, 11, 10, 10, 9, 9, 8, 8,
    // Octave 8 (very high)
    7, 7, 6, 6, 6, 6, 5, 5, 5, 5, 4, 4,
];
```

### Effect Command

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    None,

    // --- Common effects (IT, XM, S3M, MOD) ---
    Arpeggio { note1: u8, note2: u8 },         // 0xy
    PortamentoUp { speed: u8 },                 // 1xx
    PortamentoDown { speed: u8 },               // 2xx
    TonePortamento { speed: u8 },               // 3xx
    Vibrato { speed: u8, depth: u8 },           // 4xy
    TonePortamentoVolumeSlide { up: i8 },       // 5xy
    VibratoVolumeSlide { up: i8 },              // 6xy
    Tremolo { speed: u8, depth: u8 },           // 7xy
    SetPanning { pan: u8 },                     // 8xx
    SetSampleOffset { offset: u16 },            // 9xx
    VolumeSlide { up: u8, down: u8 },           // Axy
    PositionJump { order: u8 },                 // Bxx
    SetVolume { volume: u8 },                   // Cxx
    PatternBreak { row: u8 },                   // Dxx
    ExtendedEffect { param: u8 },               // Exy (sub-effects below)
    SetSpeed { speed: u8 },                     // Fxx (if xx < 32)
    SetTempo { bpm: u8 },                       // Fxx (if xx >= 32)

    // --- IT-specific ---
    SetGlobalVolume { volume: u8 },             // S1x (IT)
    GlobalVolumeSlide { up: i8, down: i8 },     // S2x (IT)
    SetEnvelopePosition { tick: u16 },          // S3x (IT)
    Panbrello { speed: u8, depth: u8 },         // S4x (IT)
    FineTune { amount: i8 },                    // S5x (IT)
    PatternDelay { ticks: u8 },                 // S6x (IT)
    InstrumentControl { param: u8 },            // S7x (IT)
    SetPanPosition { pan: u8 },                 // S8x (IT)

    // --- XM-specific ---
    ExtendedPortamento { param: u8 },           // E1x/E2x (XM)
    GlissandoControl { on: bool },              // E3x (XM)
    VibratoWaveform { waveform: u8 },           // E4x (XM)
    SetFineTune { tune: u8 },                   // E5x (XM)
    PatternLoop { count: u8 },                  // E6x (XM)
    TremoloWaveform { waveform: u8 },           // E7x (XM)
    SetPanning16 { pan: u8 },                   // E8x (XM)
    Retrigger { interval: u8 },                 // E9x (XM)
    NoteCutAfter { ticks: u8 },                 // ECx (XM)
    NoteDelay { ticks: u8 },                    // EDx (XM)

    // --- Volume Column Effects (IT/XM) ---
    VolSetVolume { vol: u8 },                   // $00-$40
    VolFineSlideUp { amount: u8 },              // $65-$74
    VolFineSlideDown { amount: u8 },            // $75-$84
    VolSlideUp { amount: u8 },                  // $85-$94
    VolSlideDown { amount: u8 },                // $95-$A4
    VolPortamento { speed: u8 },                // $C5-$D4
    VolVibrato { speed: u8 },                   // $F5-$F4 (note: overlapping ranges need care)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtendedEffect {
    NoteCut,               // E0x
    NoteDelay,             // E1x
    PatternDelay,          // E2x
    Glissando,             // E3x
    VibratoWaveform,       // E4x
    SetFineTune,           // E5x
    PatternLoop,           // E6x
    TremoloWaveform,       // E7x
    SetPanning,            // E8x
    Retrigger,             // E9x
    FineVolSlideUp,        // EAx
    FineVolSlideDown,      // EBx
    NoteCut,               // ECx
    NoteDelay,             // EDx
    PatternDelay,          // EEx
    InvertLoop,            // EFx
}
```

### Cell

A single cell in the pattern grid — the fundamental unit of tracker composition.

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Cell {
    note: Note,
    instrument: Option<u8>,    // 1-based index (0 = none)
    volume: Option<u8>,        // Volume column: 0-64, or volume command
    effect: Effect,
}

impl Cell {
    fn is_empty(&self) -> bool {
        self.note == Note::None
            && self.instrument.is_none()
            && self.volume.is_none()
            && self.effect == Effect::None
    }
}
```

### Pattern

```rust
#[derive(Clone, Debug)]
struct Pattern {
    num_rows: usize,          // Typically 32, 64, 128, 192, or 256
    data: Vec<[Cell; MAX_CHANNELS]>,  // Row-major: data[row][channel]
}

impl Pattern {
    fn new(num_rows: usize) -> Self {
        Pattern {
            num_rows,
            data: vec![[Cell::default(); MAX_CHANNELS]; num_rows],
        }
    }

    fn cell(&self, row: usize, channel: usize) -> &Cell {
        &self.data[row][channel]
    }

    fn cell_mut(&mut self, row: usize, channel: usize) -> &mut Cell {
        &mut self.data[row][channel]
    }

    fn resize_rows(&mut self, new_rows: usize) {
        self.data.resize(new_rows, [Cell::default(); MAX_CHANNELS]);
        self.num_rows = new_rows;
    }
}

const MAX_CHANNELS: usize = 64;   // IT supports up to 64 channels
const DEFAULT_ROWS: usize = 64;
```

### Sample

```rust
#[derive(Clone, Debug)]
struct Sample {
    name: String,                      // Up to 26 chars (IT)

    // Audio data
    data: Arc<Vec<f32>>,               // Shared, immutable once loaded
    sample_rate: u32,                  // Source sample rate
    bits_per_sample: u8,               // Original bit depth (8, 16, 24, 32) for save

    // Loop
    loop_type: LoopType,
    loop_start: usize,                 // In samples
    loop_end: usize,                   // In samples (exclusive)

    // Defaults
    default_volume: u8,                // 0-64 (mapped to 0.0-1.0)
    default_panning: u8,               // 0-64 (mapped to 0.0-1.0, 32 = center)
    global_volume: u8,                 // 0-64

    // Pitch
    relative_note: i8,                 // Semitone transpose (-96 to +95, XM/IT)
    fine_tune: i8,                     // -128 to +127 (1/256th of a semitone)

    // Playback flags
    vibrato_speed: u8,                 // Auto-vibrato (IT)
    vibrato_depth: u8,
    vibrato_rate: u8,
    vibrato_waveform: VibratoWaveform,

    // Flags
    flags: SampleFlags,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoopType {
    None,
    Forward,         // Standard forward loop
    PingPong,        // Alternates direction at loop boundaries
    Backward,        // Rarely used, plays in reverse
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct SampleFlags {
    is_stereo: bool,           // If true, data is interleaved L/R
    is_16bit: bool,            // Original format (for save)
    is_compressed: bool,       // IT compressed (IT214/IT215)
    has_trailing_byte: bool,   // IT quirk: sample data may have trailing byte
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VibratoWaveform {
    Sine,
    Square,
    Ramp,          // Sawtooth up
    Random,
}
```

### Instrument

```rust
#[derive(Clone, Debug)]
struct Instrument {
    name: String,                      // Up to 26 chars (IT)

    // Note-to-sample mapping
    // For each MIDI note (0-119), maps to a sample index (1-based, 0 = no sample)
    sample_map: [u8; 120],

    // Note-to-sample mapping (IT extended: 120 notes × keyboard split)
    // IT uses a 120×1 map by default; with extended mode: 120 notes × N splits
    // Simplified: just use sample_map[120]

    // Envelopes
    volume_envelope: Option<Envelope>,
    panning_envelope: Option<Envelope>,
    pitch_envelope: Option<Envelope>,

    // Envelope fade-out (applied during release)
    fade_out: u16,                     // 0-4095 (0 = instant, 4095 = very slow)

    // New Note Action
    nna: NewNoteAction,
    duplicate_check_type: DuplicateCheckType,
    duplicate_check_action: DuplicateCheckAction,

    // Pitch/Pan separation (IT)
    pitch_pan_separation: i8,          // -32 to +32
    pitch_pan_center: u8,             // MIDI note for center panning

    // Default values per instrument
    global_volume: u8,                 // 0-128

    // Filter (IT)
    cutoff: u16,                       // 0 = no filter, 128+ = frequency in Hz
    resonance: u8,                     // 0-127

    // Random variation
    random_volume: u8,                 // 0-100% variation
    random_panning: u8,
    random_cutoff: u8,
}

#[derive(Clone, Debug)]
struct Envelope {
    points: Vec<EnvelopePoint>,
    sustain_point: Option<usize>,      // Index into points
    loop_start: Option<usize>,         // Index into points
    loop_end: Option<usize>,           // Index into points

    flags: EnvelopeFlags,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnvelopePoint {
    tick: u16,                         // X-axis: time in ticks
    value: u8,                         // Y-axis: 0-64 for vol/pan, 0-63 for pitch (+/-32)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EnvelopeFlags {
    enabled: bool,
    sustain: bool,                     // Sustain loop active
    loop_: bool,                       // Envelope loop active
    carry: bool,                       // Carry envelope from previous note (IT)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NewNoteAction {
    NoteCut,       // Stop previous note immediately
    Continue,      // Continue previous note (don't trigger new)
    NoteOff,       // Begin release (fade-out) on previous note
    NoteFade,      // Begin fade-out on previous note
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DuplicateCheckType {
    Disabled,
    Note,            // Check by note value
    Sample,          // Check by sample index
    Instrument,      // Check by instrument index
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DuplicateCheckAction {
    NoteCut,        // Cut the duplicate
    NoteOff,        // Release the duplicate
    NoteFade,       // Fade the duplicate
}
```

### Module

The top-level song container.

```rust
#[derive(Clone, Debug)]
struct Module {
    name: String,                         // Song title (up to 26/32 chars)
    message: Option<String>,              // IT song message

    // Format metadata
    format: ModuleFormat,
    version: u16,                         // Format version number
    tracker_name: String,                 // Tracker that created the file

    // Song structure
    order_list: Vec<u8>,                  // Pattern indices in playback order
    patterns: Vec<Pattern>,
    instruments: Vec<Instrument>,         // Index 0 unused (1-based)
    samples: Vec<Sample>,                 // Index 0 unused (1-based)

    // Playback defaults
    initial_bpm: u16,
    initial_speed: u8,                    // Ticks per row
    initial_global_volume: u8,            // 0-128 (IT) or 0-64 (XM)
    initial_mixing_volume: u8,            // 0-128

    // Channel defaults
    channel_panning: Vec<u8>,             // Per-channel default panning (0-64)
    channel_volume: Vec<u8>,              // Per-channel default volume (0-64)

    // Flags
    flags: ModuleFlags,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModuleFormat {
    IT,       // Impulse Tracker
    XM,       // FastTracker 2
    S3M,      // ScreamTracker 3
    MOD,      // Amiga ProTracker
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ModuleFlags {
    stereo: bool,                   // Stereo mixing
    use_instruments: bool,          // IT: instruments vs. direct sample mode
    linear_slides: bool,            // IT/XM: linear vs. Amiga frequency slides
    old_effects: bool,              // IT: emulate old effect behavior
    compatible_gxx: bool,           // IT: compatible portamento
    midi_enabled: bool,             // MIDI controller
    request_embed: bool,            // IT: embed samples in file
    fast_volume_slides: bool,       // XM: XM-compatible volume slides
}
```

## Playback State Types

### Voice (Audio Thread)

Per-voice state maintained by the audio engine. One voice per active note.

```rust
struct Voice {
    // Lifetime
    active: bool,

    // Source
    sample: Option<Arc<Vec<f32>>>,
    sample_rate: f64,                    // Source sample rate
    loop_type: LoopType,

    // Playback position
    position: f64,                       // Current position in samples (fractional for interpolation)
    position_end: f64,                   // End of sample (or loop end)
    direction: f64,                      // +1.0 = forward, -1.0 = backward

    // Pitch
    base_frequency: f64,                 // Frequency of note without effects
    current_frequency: f64,              // Actual frequency with pitch effects
    sample_delta: f64,                   // Computed: current_frequency / output_sample_rate

    // Volume
    base_volume: f32,                    // Volume from note/instrument/sample
    envelope_volume: f32,                // Current envelope multiplier (0.0-1.0)
    tremolo_volume: f32,                 // Tremolo offset
    channel_volume: f32,                 // Per-channel volume
    global_volume: f32,                  // Global module volume factor
    fade_out_volume: f32,                // Current fade-out level (1.0 → 0.0)
    final_volume: f32,                   // Computed: product of all volume factors
    smoothed_volume: f32,                // Per-sample ramp position toward final_volume (mixer advances each sample; 0.0 on trigger for anti-click)

    // Panning
    base_panning: f32,                   // 0.0 = left, 0.5 = center, 1.0 = right
    envelope_panning: f32,               // Envelope panning offset (-0.5 to +0.5)
    final_panning: f32,                  // Computed: clamped base + envelope offset
    smoothed_panning: f32,               // Per-sample ramp position toward final_panning (mixer)
    ramp_enabled: bool,                  // When true, mixer ramps smoothed_* per-sample (anti-click + zipper smoothing); false = flat bit-exact gain

    // Envelope state
    vol_env: Option<EnvelopeState>,
    pan_env: Option<EnvelopeState>,
    pitch_env: Option<EnvelopeState>,

    // Effect state
    vibrato_phase: f32,
    vibrato_speed: u8,
    vibrato_depth: u8,
    vibrato_waveform: VibratoWaveform,

    tremolo_phase: f32,
    tremolo_speed: u8,
    tremolo_depth: u8,
    tremolo_waveform: VibratoWaveform,

    portamento_target: Option<f64>,      // Target frequency for tone portamento
    portamento_speed: f64,

    // Fade state
    fading: bool,
    note_off: bool,                      // True after note-off received
    cutoff_tick: Option<u16>,            // Tick at which to cut this voice (ECx)

    // Instrument data
    instrument_index: Option<u8>,
    sample_index: Option<u8>,
    note: Note,
    nna: NewNoteAction,
    fade_out_rate: u16,
}

struct EnvelopeState {
    envelope: Arc<Envelope>,
    current_point: usize,
    position: f32,                       // Interpolation position between current and next point
    released: bool,                      // True after note-off
    finished: bool,                      // True after envelope reaches end
}
```

### Sequencer State

```rust
struct SequencerState {
    // Playback position
    current_order: u16,
    current_row: u8,
    current_pattern: u8,
    current_tick: u8,

    // Timing
    bpm: u16,
    speed: u8,                           // Ticks per row
    samples_per_tick: f64,               // Computed from BPM
    sample_counter: f64,                 // Accumulator for tick timing

    // Global state
    global_volume: u8,                   // 0-128
    master_volume: f32,                  // 0.0-1.0 (config)
    playing: bool,
    paused: bool,

    // Pattern effects
    pattern_break_row: Option<u8>,       // Dxx target row
    position_jump_order: Option<u8>,     // Bxx target order
    pattern_delay_ticks: u8,             // Remaining delay ticks (S6x)
    row_delay_active: bool,

    // Per-channel state
    channels: Vec<ChannelState>,
}

struct ChannelState {
    // Current note/instrument
    last_note: Note,
    last_instrument: u8,
    last_sample: u8,

    // Volume
    channel_volume: u8,                  // 0-64
    row_volume: u8,                      // Volume set in current row

    // Panning
    channel_panning: u8,                 // 0-64 (32 = center)

    // Effect memory (many effects use "continue previous value")
    last_effect: Effect,
    last_portamento_up_speed: u8,
    last_portamento_down_speed: u8,
    last_tone_portamento_speed: u8,
    last_vibrato_speed: u8,
    last_vibrato_depth: u8,
    last_tremolo_speed: u8,
    last_tremolo_depth: u8,
    last_volume_slide_up: u8,
    last_volume_slide_down: u8,
    last_sample_offset: u16,
    last_arpeggio: (u8, u8),
    last_retrigger_interval: u8,

    // Tone portamento target
    portamento_target_period: Option<f64>,

    // Note delay (replaces per-voice delay_tick)
    delayed_cell: Option<Cell>,          // Stored cell for delayed trigger (EDx)
    note_delay_ticks: u8,                // Tick at which to trigger the delayed note

    // Effect activity flags (reset at row/pattern boundaries)
    active_effects: ActiveEffects,

    // Flags
    muted: bool,
    solo: bool,
}

struct ActiveEffects {
    volume_slide: bool,
    portamento_up: bool,
    portamento_down: bool,
    tone_portamento: bool,
    vibrato: bool,
    tremolo: bool,
    arpeggio: bool,
    panbrello: bool,
    tremor: bool,
}
```

## Edit State Types

### Edit Commands

```rust
trait EditCommand {
    fn execute(&self, module: &mut Module) -> Result<(), EditError>;
    fn undo(&self, module: &mut Module) -> Result<(), EditError>;
    fn description(&self) -> &str;
}

// Command types:
struct SetCellCommand {
    order: usize,
    row: usize,
    channel: usize,
    old_cell: Cell,
    new_cell: Cell,
}

struct InsertRowCommand {
    pattern_index: usize,
    row: usize,
    channel: Option<usize>,    // None = all channels
}

struct DeleteRowCommand {
    pattern_index: usize,
    row: usize,
    channel: Option<usize>,
    deleted_data: Vec<Cell>,
}

struct SetOrderEntryCommand {
    order_index: usize,
    old_pattern: u8,
    new_pattern: u8,
}

struct InsertOrderCommand {
    order_index: usize,
    pattern: u8,
}

struct DeleteOrderCommand {
    order_index: usize,
    deleted_pattern: u8,
}

struct SetSampleDataCommand {
    sample_index: usize,
    old_data: Arc<Vec<f32>>,
    new_data: Arc<Vec<f32>>,
}

struct SetEnvelopePointCommand {
    instrument_index: usize,
    envelope_type: EnvelopeType,
    point_index: usize,
    old_point: EnvelopePoint,
    new_point: EnvelopePoint,
}
```

### Undo Manager

```rust
struct UndoManager {
    undo_stack: Vec<Box<dyn EditCommand>>,
    redo_stack: Vec<Box<dyn EditCommand>>,
    max_depth: usize,                    // Default: 1000
    current_depth: usize,
}

impl UndoManager {
    fn execute(&mut self, cmd: Box<dyn EditCommand>, module: &mut Module) -> Result<(), EditError>;
    fn undo(&mut self, module: &mut Module) -> Result<(), EditError>;
    fn redo(&mut self, module: &mut Module) -> Result<(), EditError>;
    fn can_undo(&self) -> bool;
    fn can_redo(&self) -> bool;
    fn clear(&mut self);
}
```

## UI State Types

### Selection

```rust
#[derive(Clone, Debug, Default)]
struct Selection {
    start: Option<CursorPosition>,
    end: Option<CursorPosition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CursorPosition {
    row: usize,
    channel: usize,
    sub_column: SubColumn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubColumn {
    Note,
    InstrumentHigh,    // Tens digit of instrument
    InstrumentLow,     // Ones digit of instrument
    VolumeHigh,        // Tens digit of volume
    VolumeLow,         // Ones digit of volume
    EffectType,        // Effect command hex digit
    EffectParamHigh,   // High nibble of effect parameter
    EffectParamLow,    // Low nibble of effect parameter
}

#[derive(Clone, Debug)]
struct ClipboardData {
    cells: Vec<Vec<Cell>>,       // [row][channel]
    width: usize,                // Number of channels copied
    height: usize,               // Number of rows copied
}
```

### App State

```rust
struct HtrkApp {
    // Core data
    module: Module,
    file_path: Option<PathBuf>,
    modified: bool,

    // Playback
    playback_state: Arc<AtomicPlaybackState>,
    command_sender: CommandSender,

    // Editing
    undo_manager: UndoManager,
    cursor: CursorPosition,
    selection: Selection,
    clipboard: Option<ClipboardData>,

    // UI state
    current_octave: u8,                  // Default: 4
    edit_mode: EditMode,
    active_tab: BottomTab,
    visible_channels: usize,
    follow_playback: bool,               // Cursor follows playback position
    channel_mute_states: Vec<bool>,

    // Audio
    audio_engine: AudioEngine,
    interpolation: InterpolationType,

    // Config
    config: AppConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditMode {
    Overwrite,     // Default: new data replaces existing
    Insert,        // New data inserts, pushing rows down
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BottomTab {
    Pattern,
    Samples,
    Instruments,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InterpolationType {
    Nearest,
    Linear,
    Cubic,
}
```

## Constants

```rust
// Limits
const MAX_CHANNELS: usize = 64;
const MAX_VOICES: usize = 256;
const MAX_PATTERNS: usize = 256;
const MAX_SAMPLES: usize = 999;
const MAX_INSTRUMENTS: usize = 256;
const MAX_ORDER_LENGTH: usize = 1024;
const MAX_ENVELOPE_POINTS: usize = 25;
const MAX_PATTERN_ROWS: usize = 1024;

// Defaults
const DEFAULT_BPM: u16 = 125;
const DEFAULT_SPEED: u8 = 6;
const DEFAULT_GLOBAL_VOLUME: u8 = 128;
const DEFAULT_ROWS: usize = 64;
const DEFAULT_OCTAVE: u8 = 4;

// Ring buffer
const COMMAND_BUFFER_SIZE: usize = 256;

// Volume range
const VOLUME_MIN: u8 = 0;
const VOLUME_MAX: u8 = 64;      // IT/S3M
const VOLUME_MAX_XM: u8 = 64;   // XM
const PANNING_CENTER: u8 = 32;

// Frequency
const BASE_NOTE_RATE: f64 = 8363.0;  // IT base: middle-C sample rate
const MIDDLE_C: u8 = 60;              // MIDI note 60 = C-4
```
