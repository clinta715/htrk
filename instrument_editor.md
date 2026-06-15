# OpenMPT Instrument Editor Documentation

The Instrument Editor in OpenMPT is a comprehensive interface for designing and configuring instruments, which act as a layer between patterns and samples (or plugins). It allows for complex mapping, envelope shaping, and integration with VST/MIDI.

## Layout Overview

The Instrument Editor is divided into two primary sections: an **Upper Panel** for general settings and note mapping, and a **Lower Panel** dedicated to graphical envelope editing.

### 1. Upper Panel (General Settings & Note Map)
Managed by the `CCtrlInstruments` class and defined in `IDD_CONTROL_INSTRUMENTS`, this panel contains several logical groups:

*   **Instrument Toolbar & Header**: 
    *   Essential operations: New, Open, Save, Duplicate, and Play.
    *   Instrument selector (with spin control) and name edit box.
    *   Associated filename display.
*   **General Group**:
    *   **Global Volume**: Sets the baseline volume for the instrument.
    *   **Fade Out**: Controls how quickly a note fades out after a "Note Off" command.
    *   **Set Pan**: Optional default panning toggle and value.
*   **Pitch/Pan Separation**:
    *   **Separation (Sep)**: Controls how much the panning spreads based on the note pitch.
    *   **Centre**: Sets the root note for the separation.
*   **Sample Quality**:
    *   **Ramping**: Controls volume ramping to prevent clicks.
    *   **Resampling**: Selects the resampling algorithm (e.g., Linear, Cubic, Sinc) for this specific instrument.
*   **Filter Group**:
    *   **Cutoff & Resonance**: Default resonant filter settings with graphical sliders.
    *   **Filter Mode**: Selects the filter type (e.g., Lowpass, Highpass).
*   **Random Variation Group**:
    *   Configurable sliders for **Volume**, **Panning**, **Cutoff**, and **Resonance** variation to add "human" feel.
*   **New Note Action (NNA) Group**:
    *   **NNA**: Determines what happens when a new note starts (Note Cut, Note Off, Note Fade, Continue).
    *   **Duplicate Check Type (DCT)**: Condition for duplicate note handling (Note, Sample, Instrument).
    *   **Duplicate Note Action (DNA)**: Action taken when DCT is triggered.
*   **Note Mapping (`CNoteMapWnd`)**:
    *   A grid-based interface where each MIDI note is mapped to a specific sample and a mapped note (transposition).
    *   Visual representation of the keyboard mapping for multi-sample instruments.
*   **Plugin / MIDI Group**:
    *   **Mix Plug**: Assigns the instrument to a specific VST/DMO plugin slot.
    *   **Midi Channel/Program/Bank**: External MIDI hardware routing.
    *   **Pitch Bend Depth**: Configurable range for pitch wheel messages.

### 2. Lower Panel (Envelope Editor)
Managed by the `CViewInstrument` class, this panel provides a graphical multi-point editor for various control signals.

*   **Envelope Types**:
    *   **Volume**: Shapes the loudness over time.
    *   **Panning**: Shapes the stereo position over time.
    *   **Pitch / Filter**: Shapes either the pitch or the resonant filter cutoff over time.
*   **Graphical Interface**:
    *   **Nodes**: Users can add, remove, and drag nodes to create complex shapes.
    *   **Grid**: Optional grid for snapping nodes to specific ticks.
    *   **Zooming**: Horizontal zoom for precise node placement.
*   **Looping & Sustain**:
    *   **Envelope Loop**: A range of nodes that repeats while a note is held.
    *   **Sustain Loop**: A separate loop that occurs only while the note is held, often used for stable sustain phases.
    *   **Release Node**: Specifies where the envelope should jump when a "Note Off" is received.

## Data Design

The underlying data is represented by the `ModInstrument` struct in `soundlib/ModInstrument.h`.

*   **Multi-Point Envelopes**: Stored as a vector of `EnvelopeNode` (tick/value pairs).
*   **Compatibility**: The design supports legacy tracker formats (XM, IT) while extending them with OpenMPT-specific features like 24-bit precision, custom tunings, and VST integration.
*   **Undo System**: Integrated with `CInstrumentUndo` to allow non-destructive editing of complex mappings and envelopes.

## Functionality & Workflow

1.  **Mapping**: A user typically starts by mapping samples to notes in the Note Map.
2.  **Shaping**: They then switch to the Envelope Editor to define the volume contour (ADSFR - Attack, Decay, Sustain, Fade, Release).
3.  **Behavior**: NNAs are configured to allow for polyphonic overlapping (e.g., long-tail pads) or strict monophonic behavior (e.g., lead synths).
4.  **Integration**: If using VSTs, the instrument acts as a bridge, routing pattern notes and MIDI macros to the plugin.
