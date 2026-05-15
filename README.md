# htrk — A Modern Music Tracker

htrk is a module music tracker for composing and playing back module files in classic
tracker formats (IT, XM, S3M, MOD). Built with **Rust**, **egui/eframe**, and **cpal**.

## Quick Start

- **F1** — Keyboard shortcuts help
- **F5** — Play from start
- **F8** — Stop
- **F10** — Settings
- **Qwerty keys** — Enter notes on the Note column (also plays audio preview)

See [docs/](docs/) for full design documentation.

## Building

```bash
cargo build --release
```

## Key Features

- Pattern-based tracker workflow with mouse + keyboard editing
- Format support: IT, XM, S3M, MOD, native HTK/HTI
- Audio preview on qwerty keyboard note entry
- Themeable UI (Dark Modern, Dark Retro, Light, Amber Terminal, Matrix Green, etc.)
- Sample editor, instrument editor with envelope support
- WAV export
- Configurable interpolation (Nearest/Linear/Cubic) and limiter (HardClip/SoftKnee)
- Configurable auto-backup 
