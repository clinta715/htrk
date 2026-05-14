# HTRK Keyboard Reference

## Note Entry

The QWERTY keyboard maps to a piano layout for entering notes.

```
Lower octave (octave N):
  Z=C  S=C#  X=D  D=D#  C=E  V=F  G=F#  B=G  H=G#  N=A  J=A#  M=B

Upper octave (octave N+1):
  Q=C  2=C#  W=D  3=D#  E=F  R=F  5=F#  T=G  6=G#  Y=A  7=A#  U=B
```

| Key | Action |
|-----|--------|
| `Z..M` / `Q..U` | Enter note (lower / upper octave) |
| `.` on Note col | Note Off (`===`) |
| `.` on other cols | Clear field value |
| `0-9` on Instr/Vol | Decimal entry (instrument 0-99, volume 0-64) |
| `0-9 A-F` on Fx cols | Hex entry for effect type and parameter |

## Navigation

| Key | Action |
|-----|--------|
| Arrow keys | Move cursor |
| Shift + Arrow | Extend selection |
| Alt + Up/Down | Transpose selection ±1 semitone |
| Tab / Shift+Tab | Next / previous channel |
| Alt+Left/Alt+Right | Skip to previous/next channel |
| `[` / `]` or `-` / `=` | Previous / next pattern |
| PgUp / PgDn | Scroll 16 rows |
| Home / End | Jump to first / last row |

## Transport

| Key | Action |
|-----|--------|
| Space | Repeat last entry (stopped) / Stop (playing) |
| F5 | Play from start |
| F6 | Play pattern |
| F7 | Play through order list |
| F8 | Stop all |
| F9 | Play from current playback position |

## Pattern Editing

| Key | Action |
|-----|--------|
| Backspace | Clear cell at cursor |
| Delete | Clear cell + advance down |
| Insert | Insert empty row |
| Alt+Delete | Delete row |
| Ctrl+Z / Ctrl+Y | Undo / Redo |
| Ctrl+A | Select all |
| Escape | Clear selection |

## Block Operations

| Key | Action |
|-----|--------|
| Ctrl+C | Copy block (system clipboard) |
| Ctrl+X | Cut block |
| Ctrl+V | Paste block |
| Alt+C | Copy block (IT-style internal clipboard) |
| Alt+P | Paste from IT clipboard |
| Alt+Z | Reverse block |
| Alt+F | Fill instrument in block |
| Alt+I | Interpolate volume across block |
| Alt+K | Interpolate effect params across block |
| Alt+R | Randomize notes/volume in block |

## IT-Style Features

| Key | Action |
|-----|--------|
| Alt+0..9 | Set cursor skip value (rows advanced after note entry) |
| `,` (comma) | Toggle edit mask (instrument + volume auto-fill) |
| Space (stopped) | Repeat last entered cell |
| Alt+N | Toggle multichannel editing for current channel |
| Ctrl+Shift+Up/Down | Increase / Decrease current octave |
| Ctrl+Up/Down | Decrease / Increase current octave |

## Channel

| Key | Action |
|-----|--------|
| F2 | Toggle record mode |
| Alt+M | Toggle mute channel |
| Alt+S | Toggle solo channel |

## File

| Key | Action |
|-----|--------|
| Ctrl+N | New song |
| Ctrl+O | Open module |
| Ctrl+I | Import sample |
| Ctrl+Shift+I | Import instrument |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As... |

## Display

| Key | Action |
|-----|--------|
| F1 | Toggle help/shortcuts window |
| F10 | Toggle settings window |
