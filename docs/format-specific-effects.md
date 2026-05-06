# Format-Specific Effects Architecture

## Overview

This document describes the architecture for handling format-specific tracker effects in HTRK. The goal is to provide proper support for unique commands per format while maintaining a common "HTRK-native" effect subset for user editing.

## Problem Statement

Currently, HTRK uses a unified `Effect` enum that attempts to handle all formats generically. This leads to:
- Scattered state fields (`last_sample_offset`, `fine_tune_offset`) in channel state
- Bugs where format-specific behavior isn't properly handled (e.g., sample offset not persisting in retrigger/note delay scenarios)
- Confusion about which effects behave differently per format

## Solution

Implement format-specific effect types with a conversion layer:

```
Format-Specific Parse → FormatEffect → HtrkEffect (where possible)
                              ↓
                      Store in .htk
                              ↓
                    Sequencer processes
                    with format context
```

## Type System Design

### Effect Enum Structure

```rust
// src/sequencer/effect.rs

/// Main effect enum - all effects flow through this
pub enum Effect {
    /// HTRK-native/Universal effects (common subset for user editing)
    /// These convert cleanly between formats
    Htrk(HtrkEffect),
    
    /// Format-specific effects (preserve exact behavior)
    /// These cannot be converted to universal without loss
    Format(FormatEffect),
}

/// Universal/HTRK-native effects - common subset
pub enum HtrkEffect {
    VolumeSlide { fine: bool, up: u8, down: u8 },
    Vibrato { waveform: u8, depth: u8, speed: u8 },
    Tremolo { waveform: u8, depth: u8, speed: u8 },
    Arpeggio { add1: u8, add2: u8 },
    TonePortamento { speed: u8 },
    PanSlide { left: u8, right: u8 },
    PositionJump { order: u8 },
    PatternBreak { row: u8 },
    SetSpeed { speed: u8 },
    SetTempo { tempo: u8 },
    SetVolume { volume: u8 },
    SetGlobalVolume { volume: u8 },
    SetChannelVolume { volume: u8 },
    Retrigger { interval: u8 },
    NoteCut { tick: u8 },
    NoteDelay { tick: u8 },
    PatternDelay { rows: u8 },
    // Future: more as needed
}

/// Format-specific effects
pub enum FormatEffect {
    Xm(XmEffect),
    Mod(ModEffect),
    S3m(S3mEffect),
    It(ItEffect),
    Htk(HtkEffect),  // Future: native format
}

/// XM-unique effects
pub enum XmEffect {
    SetSampleOffset(u16),     // Effect 9 - per-channel memory
    Panbrello(u8),            // Effect T
    VolumeColumn(u8),         // Volume column commands (unique to XM)
    // Overlapping effects stored as HtrkEffect
}

/// MOD-unique effects  
pub enum ModEffect {
    // TODO: Document unique MOD effects
}

/// S3M-unique effects
pub enum S3mEffect {
    // TODO: Document unique S3M effects
}

/// IT-unique effects
pub enum ItEffect {
    // TODO: Document unique IT effects
}

/// HTRK-native format (future)
pub enum HtkEffect {
    // Effects unique to HTRK format
}
```

### Format Context

```rust
pub enum FormatType {
    Mod,
    Xm,
    S3m,
    It,
    Htk,
}

impl FormatType {
    pub fn supports_volume_column(&self) -> bool {
        matches!(self, FormatType::Xm)
    }
    
    pub fn supports_sample_offset(&self) -> bool {
        matches!(self, FormatType::Xm | FormatType::Mod | FormatType::S3m)
    }
    
    // ... more format capability queries
}
```

## Effect Conversion

### At Load Time

```
Raw bytes → Format parser → FormatEffect → (try) HtrkEffect
                                    ↓ (if convertible)
                               Store as Effect::Htrk
                                    ↓ (if not)
                               Store as Effect::Format
```

### At Save Time

```
Effect → Convert to FormatEffect → Serialize to .htk
```

## Sequencer Integration

### Current Problems to Solve

1. **Scattered State**: Remove `last_sample_offset`, `fine_tune_offset` from channel state
2. **Missing Paths**: Fix 3 code paths that don't handle sample offset:
   - `retrig_channel_note_period` (retrigger effect Qxx)
   - `trigger_delayed_note_period` (note delay EDx)
   - `retrigger_channel_note` (another retrigger path)

### New Architecture

```rust
impl SequencerEngine {
    pub fn process_effect(&mut self, effect: &Effect, channel: usize) {
        match effect {
            Effect::Htrk(e) => self.process_htrk_effect(e, channel),
            Effect::Format(fe) => self.process_format_effect(fe, channel),
        }
    }
    
    fn process_format_effect(&mut self, effect: &FormatEffect, channel: usize) {
        match effect {
            FormatEffect::Xm(xm) => self.process_xm_effect(xm, channel),
            FormatEffect::Mod(mod_eff) => self.process_mod_effect(mod_eff, channel),
            FormatEffect::S3m(s3m) => self.process_s3m_effect(s3m, channel),
            FormatEffect::It(it) => self.process_it_effect(it, channel),
            FormatEffect::Htk(htk) => self.process_htk_effect(htk, channel),
        }
    }
    
    fn process_xm_effect(&mut self, effect: &XmEffect, channel: usize) {
        match effect {
            XmEffect::SetSampleOffset(offset) => {
                // Direct handling - effect carries data, no scattered state
                self.state.channels[channel].voice_sample_offset = *offset;
                // Apply to any active voice immediately
                for voice in &mut self.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.position = *offset as f64;
                    }
                }
            }
            XmEffect::Panbrello(val) => { /* ... */ }
            XmEffect::VolumeColumn(val) => { /* ... */ }
        }
    }
}
```

## Implementation Phases

### Phase 1: Type System (Current)
- Create `FormatEffect` enum with variants for each format
- Create `HtrkEffect` enum for universal effects
- Modify `Effect` enum to contain either

### Phase 2: Parsers
- Update MOD, XM, S3M, IT parsers to output `FormatEffect`
- Add conversion functions

### Phase 3: Sequencer
- Remove scattered state fields from channel
- Implement format-aware effect processing
- Fix broken code paths

### Phase 4: Storage
- Update .htk format to store `FormatEffect`
- Add version number for future migration

## Migration Priority

Based on "difference from current standard" and user impact:

| Priority | Effect | Status |
|----------|--------|--------|
| 1 | Sample Offset (9) | Broken - priority fix needed |
| 2 | Volume Column | XM unique - significant |
| 3 | Period/Pitch | MOD vs XM - fundamental |
| 4 | Retrigger/Note Delay | We found bugs here |
| 5 | Panbrello | XM unique |
| 6 | Arpeggio | MOD 3-note vs others |
| 7 | Tremor | Different per format |

## Testing Strategy

1. Load known files from each format
2. Compare audio output with reference players
3. Test .htk save/load roundtrip
4. Test format conversion warnings

## References

- ft2play (FastTracker 2) for XM behavior
- OpenMPT for MOD/S3M/IT behavior
- Original player source code where available

---

*Document Version: 1.0*
*Last Updated: 2026-05-05*