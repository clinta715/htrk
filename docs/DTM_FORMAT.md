# DigiTrakker (.DTM) File Format

Based on the source code of Graoumf Tracker 2 (`mod_dtm.cpp`), the `.DTM` format is a chunk-based music module format used by DigiTrakker.

## File Structure

The file consists of several chunks, each with a 4-character ID and a 32-bit length (Big-Endian).

### Main Chunk (`D.T.`)
Mandatory header chunk.
- **File Type:** 16-bit word (0 = module).
- **Speed:** 16-bit word (Big-Endian).
- **Tempo:** 16-bit word (Big-Endian).
- **Song Name:** 20 characters.

### Song Chunk (`S.Q.`)
Defines the sequence of patterns.
- **Length:** 16-bit word (number of patterns in the sequence).
- **Repeat:** 16-bit word (loop point).
- **Sequence Data:** `length` bytes following the header, each representing a pattern number.

### Pattern Descriptor Chunk (`PATT`)
Global pattern information.
- **Number of Tracks:** 16-bit word.
- **Number of Patterns:** 16-bit word.
- **Version:** 4 characters (e.g., "2.04").

### Track Names Chunk (`TRKN`)
Contains names for each track as a series of null-terminated strings.

### Pattern Names Chunk (`PATN`)
Contains names for each pattern as a series of null-terminated strings.

### Data Pattern Chunk (`DAPT`)
Contains actual pattern data.
- **Track Field:** 32-bit bitfield indicating which tracks are saved.
- **Pattern Number:** 16-bit word.
- **Number of Lines:** 16-bit word.
- **Pattern Data:** Followed by raw note data. Version >= 2.04 uses `MODDTM_NOTE` structure; older versions use `MODMOD_NOTE`.

### Instrument Descriptor Chunk (`INST`)
Contains metadata for all instruments.
- **Number of Instruments:** 16-bit word.
- **Instrument Data Array:** An array of `MODDTM_INST_CHUNK_DATA` structures:
    - **Length:** 32-bit (total sample bytes).
    - **Finetune:** 8-bit (signed 4-bit nibble).
    - **Volume:** 8-bit (0-64).
    - **Repeat Position:** 32-bit (in bytes).
    - **Repeat Length:** 32-bit (in bytes).
    - **Name:** 22 characters.
    - **Flags:** Bit 0 = Stereo (1) or Mono (0).
    - **Bits per Sample:** 8-bit (typically 8 or 16).
    - **MIDI Note:** 8-bit (Center C = 48).
    - **Frequency:** 32-bit (Sampling rate).

### Data Instrument Chunk (`DAIT`)
Contains the raw sample data for a specific instrument.
- **Instrument Number:** 16-bit word.
- **Sample Data:** Followed by raw audio data (PCM).

## Pattern Data Format (Version 2.04+)

Each note in a pattern is represented by 4 bytes:
1. **Byte 1:** Octave (High 4 bits) and Note (Low 4 bits).
2. **Byte 2:** Volume (6 bits) and MSB of Instrument index.
3. **Byte 3:** LSB of Instrument index (4 bits) and Effect Command (4 bits).
4. **Byte 4:** Effect Parameter (8 bits).

## Effects
DigiTrakker supports standard ProTracker-style effects (Arpeggio, Portamento, Vibrato, etc.). Command `0x0` is Arpeggio, and most others are converted using standard ProTracker mapping.
