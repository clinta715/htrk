# Scream Tracker 2 (.STM) Format

Scream Tracker 2 is an early PC tracker format developed by Future Crew. It was the predecessor to the popular S3M format.

## Identification
- Song name (20 bytes) at the start.
- Magic string `!Scream!` at offset 20.
- Followed by `0x1A` (or sometimes `0x02`).

## Versions Supported
- 1.10
- 2.00, 2.10, 2.20, 2.21

## Structure

### File Header
| Offset | Size | Description |
|--------|------|-------------|
| 0      | 20   | Song name (ASCIIZ) |
| 20     | 8    | Magic string `!Scream!` |
| 28     | 1    | EOF character (0x1A) |
| 29     | 1    | Type (1=song, 2=module) |
| 30     | 1    | Major version |
| 31     | 1    | Minor version |

### Instrument Header (31 or 32 instruments)
| Size | Description |
|------|-------------|
| 13   | Instrument name (8.3 format) |
| 1    | Instrument disk |
| 2    | Reserved |
| 2    | Sample length |
| 2    | Loop begin |
| 2    | Loop end |
| 1    | Playback volume |
| 1    | Reserved |
| 2    | C4 speed |
| 4    | Reserved |
| 2    | Length in paragraphs |

### Pattern Data
- **Channels**: Usually 4.
- **Rows**: 64 per pattern.
- **Events**: 4 bytes per event (Note, Volume/Ins, Volume, Effect/Param).

## Effects Mapping
| STM Effect | Description | XMP Internal Effect |
|------------|-------------|---------------------|
| A          | Set Tempo | FX_SPEED |
| B          | Pattern Jump | FX_JUMP |
| C          | Pattern Break | FX_BREAK |
| D          | Volume Slide | FX_VOLSLIDE |
| E          | Portamento Down | FX_PORTA_DN |
| F          | Portamento Up | FX_PORTA_UP |
| G          | Tone Portamento | FX_TONEPORTA |
| H          | Vibrato | FX_VIBRATO |
| I          | Tremor | FX_TREMOR |
| J          | Arpeggio | FX_ARPEGGIO |

## Technical Notes
- STM tempo is calculated based on a mix rate of 23863 Hz.
- BPM can be non-integer and is influenced by both speed and tempo factor.
