# htrk v0.11.0 — Design Documentation

A modern Impulse Tracker / ScreamTracker clone built in Rust with egui.

## Overview

htrk is a module music tracker that supports composing and playing back music in
classic tracker formats (IT, XM, S3M, MOD). It combines the authentic pattern-based
editing workflow of DOS-era trackers with a modern, themeable UI.

## Design Documents

| Document | Description |
|----------|-------------|
| [Architecture](architecture.md) | System architecture, thread model, data flow, and crate structure |
| [Data Model](data-model.md) | All Rust data structures, enums, type aliases, and invariants |
| [Audio Engine](audio-engine.md) | Mixing pipeline, resampling, voice management, DSP effects |
| [Sequencer](sequencer.md) | Playback state machine, tick timing, effect command processing |
| [File Formats](formats.md) | IT, XM, S3M, MOD binary format specs and parsing strategy |
| [UI Design](ui-design.md) | Layout, widgets, keyboard shortcuts, themes, interaction model |
| [Implementation Plan](implementation-plan.md) | Phased task breakdown with dependencies and milestones |
| [Testing Strategy](testing-strategy.md) | Test approach, coverage targets, and reference test files |

## Quick Reference

- **Language**: Rust (edition 2021+)
- **UI Framework**: egui + eframe
- **Audio Output**: cpal (cross-platform)
- **Thread Model**: UI thread + real-time audio thread, lock-free communication
- **Internal Sample Format**: f32 normalized [-1.0, 1.0]
- **Primary Format**: Impulse Tracker (.it)
- **Default Octave**: 4 (Z key = C-4)
