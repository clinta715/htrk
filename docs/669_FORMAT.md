# Composer 669 (.669) Format

Composer 669 (and its successor UNIS 669) is a PC tracker format known for its 8-channel limitation and specific effect set.

## Identification
Magic bytes at the beginning:
- `if`: Standard Composer 669
- `JN`: Extended UNIS 669

## Structure

### File Header
| Offset | Size | Description |
|--------|------|-------------|
| 0      | 2    | Marker (`if` or `JN`) |
| 2      | 108  | Song message / title |
| 110    | 1    | Number of samples (0-64) |
| 111    | 1    | Number of patterns (0-128) |
| 112    | 1    | Loop order number |
| 113    | 128  | Order list |
| 241    | 128  | Tempo list for patterns |
| 369    | 128  | Break list for patterns |

### Instrument Header
| Size | Description |
|------|-------------|
| 13   | Instrument name (ASCIIZ) |
| 4    | Instrument length |
| 4    | Loop start |
| 4    | Loop end |

### Pattern Data
- **Channels**: Fixed at 8.
- **Rows**: 64 per pattern.
- **Events**: 3 bytes per event (Note/Ins, Ins/Vol, Effect).

## Effects Mapping
| 669 Effect | Description | XMP Internal Effect |
|------------|-------------|---------------------|
| 0          | Portamento Up | FX_669_PORTA_UP |
| 1          | Portamento Down | FX_669_PORTA_DN |
| 2          | Tone Portamento | FX_669_TPORTA |
| 3          | Finetune | FX_669_FINETUNE |
| 4          | Vibrato | FX_669_VIBRATO |
| 5          | Set Speed | FX_SPEED_CP |

## Technical Notes
- The format uses a linear frequency table (CSPD).
- Default BPM is 78.
- Panning is fixed (alternating Left/Right).
