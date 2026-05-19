# OctaMED (.MMD) File Format

Based on the source code of Graoumf Tracker 2 (`mod_mmd.cpp`), the `.MMD` format is used by OctaMED on the Amiga. GT2 supports MMD0 and MMD1 versions.

## Versions
- **MMD0:** Classic OctaMED format (ID: 'MMD0').
- **MMD1:** OctaMED Professional format with extended features (ID: 'MMD1').

## File Structure
Unlike chunk-based formats, MMD is a pointer-based format where a main header contains offsets (Big-Endian) to various structures within the file.

### Main Header (`MODMMD_Header`)
- **ID:** 32-bit ('MMD0', 'MMD1', etc.).
- **ModLen:** Total length of the module.
- **Song Offset:** Pointer to `MODMMD_Song`.
- **Block Array Offset:** Pointer to an array of pointers to `MODMMD_Block`.
- **Sample Array Offset:** Pointer to an array of pointers to `MODMMD_InstrHdr`.
- **Expansion Data Offset:** Pointer to `MODMMD_Exp` structure.

### Song Data (`MODMMD_Song`)
- **Sample Metadata:** 63 `MODMMD_Sample` structures (repeat, length, volume, etc.).
- **NumBlocks:** Number of blocks (patterns) in the module.
- **SongLen:** Number of entries in the play sequence.
- **PlaySeq:** Sequence of block numbers to play.
- **DefTempo:** Default tempo.
- **Flags:**
    - `0x01`: Filter On
    - `0x10`: Volume Hex
    - `0x20`: ST/NT/PT Compatibility
    - `0x40`: 8-Channel Mode
- **Flags2:**
    - `0x20`: BPM Mode On

### Expansion Data (`MODMMD_Exp`)
This structure adds support for modern features:
- **Song Name Offset:** Pointer to the song name string.
- **Instrument Info Offset:** Pointer to `MODMMD_InstrInfo` (long instrument names).
- **Instrument Extensions:** Pointers to release/decay and finetune settings.
- **Annotation Text:** Pointer to a module comment/info block.

### Blocks (Patterns)
- **MMD0 Blocks:** Max 64 tracks, max 256 lines (8-bit counts).
- **MMD1 Blocks:** Extended track and line counts (16-bit counts).
- **Block Info:** Optional pointer to block names and masks.

### Instruments
- **Samples:** Ordinary 1-octave or multi-octave (IFF) samples.
- **Synthetic/Hybrid:** Instruments with volume and waveform envelopes.

## Effect Commands (Mapping to GT2)
MMD uses its own set of effect commands, mapped during loading:
- **0x00:** Arpeggio (if parameter != 0)
- **0x01:** Portamento Up
- **0x02:** Portamento Down
- **0x03:** Tone Portamento
- **0x04:** Vibrato
- **0x08:** Panning
- **0x09:** Set Sample Offset
- **0x0A:** Vibrato (mapped to internal vibrato)
- **0x0D:** Volume Slide
- **0x0F:** Set Tempo/BPM
- **0x11/0x12:** Note Cut/Delay
- **0x14/0x15:** Vibrato depth/speed
- **0x18/0x19:** Sample offset (high/low)
- **0x1C/0x1D:** Retrigger
- **0x90:** Set Panning
- **0x70+x:** Retrigger
