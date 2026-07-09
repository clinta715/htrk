
use super::helpers::calculate_sample_offset;
use crate::audio::sequencer_engine::{PluginNoteEvent, SequencerEngine};

use crate::sequencer::effect::Effect;
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::sequencer::sample::Sample;

impl SequencerEngine {
    /// Release the most recent held note on `channel` if that channel's
    /// current instrument is backed by a CLAP plugin.
    ///
    /// Tracks are monophonic from the sequencer's point of view: just like a
    /// new sample note cuts off the previous sample voice (via `handle_nna`),
    /// a new plugin note must first release the previous one, an explicit
    /// `Note::Off` must reach the plugin, and `Note::Cut` / `Note::Fade` must
    /// also silence any sounding plugin voice. The CLAP note-off identifies
    /// the note to release by `(channel, key)`, so the *real* key from the
    /// channel's `last_note` is used — never a placeholder 0.
    ///
    /// `hard_cut` selects the release style: `true` emits an immediate
    /// note-off (used for `Note::Cut` and new-note interruption), `false`
    /// also emits a note-off (CLAP synths treat the release-velocity/
    /// envelope tail themselves, matching the sample path's `Note::Fade`
    /// behavior of letting the instrument's release phase run).
    ///
    /// This is a no-op when the channel has no plugin instrument or no
    /// prior `Note::On` is recorded.
    fn emit_plugin_note_off(&mut self, channel: usize, instrument_idx: u8, hard_cut: bool) {
        let _ = hard_cut; // CLAP note-off carries no cut/fade distinction; both send NoteOffEvent.
        let key = match self.state.channels.get(channel) {
            Some(ch) => match ch.last_note {
                Note::On(k) if k < 120 => k,
                _ => return, // No held note to release.
            },
            None => return,
        };
        let midi_ch = {
            let module = match self.module.as_ref() {
                Some(m) => m,
                None => return,
            };
            let idx = instrument_idx as usize;
            if idx == 0 || idx >= module.instruments.len() {
                return;
            }
            if module.instruments[idx].plugin.is_none() {
                return;
            }
            module.instruments[idx]
                .midi_base_channel
                .wrapping_add(channel as u8) % 16
        };
        self.pending_plugin_note_events.push(PluginNoteEvent {
            instrument_idx,
            midi_channel: midi_ch,
            key,
            velocity: 0,
            note_on: false,
        });
    }

    pub(crate) fn process_tick_zero_unified(&mut self) {
        let pattern_index = self.state.current_pattern as usize;
        let row = self.state.current_row as usize;

        let cells: Vec<(usize, Cell)> = {
            let module = match self.module.as_ref() {
                Some(m) => m,
                None => return,
            };
            if pattern_index >= module.patterns.len() {
                self.stop();
                return;
            }
            let pattern = &module.patterns[pattern_index];
            if row >= pattern.num_rows {
                self.advance_row();
                return;
            }
            let mut result = Vec::with_capacity(64);
            for ch in 0..64 {
                if ch >= pattern.data[row].len() {
                    break;
                }
                let cell = pattern.data[row][ch];
                if cell.is_empty() {
                    continue;
                }
                result.push((ch, cell));
            }
            result
        };

        for (ch, cell) in cells {
            self.process_cell_unified(ch, &cell);
        }
    }

    pub(crate) fn process_cell_unified(&mut self, channel: usize, cell: &Cell) {
        let module = match self.module.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };
        if channel >= self.state.channels.len() {
            return;
        }
        // Snapshot the channel's instrument BEFORE this cell updates it, so a
        // new Note::On can release the previously-held note on the PREVIOUS
        // instrument's plugin (instrument changes mid-channel).
        let prev_instrument = self.state.channels[channel].last_instrument;

        // Common: instrument
        if cell.instrument.is_some() {
            self.state.channels[channel].last_instrument = cell.instrument.unwrap();
        }

        let instrument_idx = self.state.channels[channel].last_instrument as usize;
        let has_instruments = !module.instruments.is_empty();

        let (sample_idx, remapped_key) = if has_instruments && instrument_idx > 0 && instrument_idx < module.instruments.len() {
            let inst = &module.instruments[instrument_idx];
            match cell.note {
                Note::On(key) if (key as usize) < 120 => {
                    let idx = inst.sample_map[key as usize] as usize;
                    let rk = inst.note_map[key as usize];
                    (idx, if rk < 120 { rk } else { key })
                }
                _ => (self.state.channels[channel].last_sample as usize, {
                    match cell.note { Note::On(k) => k, _ => 0 }
                }),
            }
        } else {
            (instrument_idx, match cell.note { Note::On(k) => k, _ => 0 })
        };

        if sample_idx > 0 && sample_idx < module.samples.len() {
            self.state.channels[channel].last_sample = sample_idx as u8;
        }

        let sample = if sample_idx > 0 && sample_idx < module.samples.len() {
            Some(&module.samples[sample_idx])
        } else {
            None
        };

        // Set channel defaults from sample
        self.with_processor_mut(|processor, engine| processor.init_sample_defaults(engine, channel, cell, sample));

        // Volume column
        if let Some(vol) = cell.volume {
            // Pxy effect — volume column carries the param value
            if let Effect::SetSendBusParam { bus, param, .. } = cell.effect {
                let idx = (bus as usize) * 4 + (param as usize) % 4;
                let mapped = ((vol as u16 * 255 + 49) / 99).min(255) as u8; // 0-99 → 0-255, rounding
                self.state.channels[channel].last_send_param_value[idx] = mapped;
            } else {
                self.with_processor_mut(|processor, engine| processor.process_volume_column(engine, channel, vol));
            }
        }
        // Set volume effects
        if let Effect::SetVolume { volume } = &cell.effect {
            let v = (*volume).min(64);
            self.state.channels[channel].channel_volume = v;
            self.state.channels[channel].row_volume = v;
        }
        if let Effect::VolSetVolume { vol } = &cell.effect {
            let v = (*vol).min(64);
            self.state.channels[channel].channel_volume = v;
            self.state.channels[channel].row_volume = v;
        }

        let is_tone_portamento = matches!(
            cell.effect,
            Effect::TonePortamento { .. } | Effect::TonePortamentoVolumeSlide { .. }
                | Effect::VolPortamento { .. }
        );

        let is_note_delay = matches!(cell.effect, Effect::NoteDelay { ticks } if ticks > 0);

        let has_volume_effect = cell.volume_effect.is_some();

        if is_note_delay {
            self.state.channels[channel].delayed_cell = Some(*cell);
            if let Effect::NoteDelay { ticks } = cell.effect {
                self.state.channels[channel].note_delay_ticks = ticks;
            }
            if let Note::On(key) = cell.note {
                self.state.channels[channel].last_note = Note::On(key);
            }
                } else {
                    match cell.note {
                        Note::On(key) => {
                            let has_plugin = has_instruments
                                && instrument_idx > 0
                                && instrument_idx < module.instruments.len()
                                && module.instruments[instrument_idx].plugin.is_some();

                            if has_plugin {
                                // Monophonic interruption: a track plays one
                                // note at a time, so a new Note::On must first
                                // release the previously-held plugin note on
                                // this channel (mirrors the sample path calling
                                // `handle_nna(NoteCut)` before `allocate_voice`).
                                // Must run BEFORE updating `last_note` below, so
                                // the helper reads the OLD key from channel state.
                                self.emit_plugin_note_off(channel, prev_instrument, true);

                                let midi_ch = module.instruments[instrument_idx]
                                    .midi_base_channel
                                    .wrapping_add(channel as u8) % 16;
                                self.pending_plugin_note_events.push(PluginNoteEvent {
                                    instrument_idx: instrument_idx as u8,
                                    midi_channel: midi_ch,
                                    key,
                                    velocity: 100,
                                    note_on: true,
                                });
                                // Apply instrument parameter macros.
                                // For each macro defined on this instrument,
                                // read the source value (currently just the
                                // cell's volume column), normalize to
                                // 0.0–1.0, remap to the macro's range, and
                                // queue a SetInstrumentPluginParam value.
                                // The audio engine routes queued values to
                                // the matching instrument_plugin_processors
                                // slot on the next tick.
                                if let Some(vol) = cell.volume {
                                    let inst = &module.instruments[instrument_idx];
                                    for m in &inst.macros {
                                        let normalized = (vol as f32 / 64.0).clamp(0.0, 1.0);
                                        let value = m.range_min
                                            + normalized * (m.range_max - m.range_min);
                                        self.pending_instrument_plugin_param_changes
                                            .push((instrument_idx as u8, m.param_id, value));
                                    }
                                }
                            } else if is_tone_portamento {
                                self.with_processor_mut(|processor, engine| processor.setup_portamento(engine, channel, key, remapped_key, sample, sample_idx));
                            } else {
                                self.with_processor_mut(|processor, engine| processor.trigger_note(engine, channel, key, remapped_key, sample, sample_idx, cell, instrument_idx));
                            }
                            // Record the new note as the channel's held note
                            // (after the plugin release above so it doesn't
                            // shadow the previous note being interrupted).
                            self.state.channels[channel].last_note = Note::On(key);
                        }
                Note::Off => {
                    let has_plugin = has_instruments
                        && instrument_idx > 0
                        && instrument_idx < module.instruments.len()
                        && module.instruments[instrument_idx].plugin.is_some();
                    if has_plugin {
                        // Release the held plugin note by its real key. A CLAP
                        // note-off matches the sounding voice by (channel, key),
                        // so we must use the previously-played key, not a
                        // placeholder, or the synth never sees the release.
                        self.emit_plugin_note_off(channel, instrument_idx as u8, false);
                    } else {
                        self.with_processor_mut(|processor, engine| processor.handle_note_off(engine, channel));
                    }
                }
                Note::Cut => {
                    // Cut must also silence any sounding plugin voice on this
                    // channel, not just sample voices.
                    self.emit_plugin_note_off(channel, instrument_idx as u8, true);
                    self.cut_channel_voices(channel);
                }
                Note::Fade => {
                    // Fade releases plugin voices (the synth's own release tail
                    // runs) in addition to starting sample-voice fades.
                    self.emit_plugin_note_off(channel, instrument_idx as u8, false);
                    self.fade_channel_voices(channel);
                }
                Note::None => {}
            }

            self.apply_effect_unified(channel, &cell.effect, true);
        }

        // Apply volume_effect on tick 0
        if has_volume_effect {
            if let Some(vol_eff) = cell.volume_effect {
                self.apply_effect_unified(channel, &vol_eff, true);
            }
        }
    }

    pub(crate) fn calculate_sample_offset(&self, channel: usize, cell: &Cell, sample: &Sample) -> usize {
        calculate_sample_offset(&self.state, channel, cell, sample)
    }
}
