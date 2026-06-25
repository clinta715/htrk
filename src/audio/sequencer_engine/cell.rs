use std::sync::Arc;

use super::helpers::calculate_sample_offset;
use crate::audio::sequencer_engine::{PluginNoteEvent, SequencerEngine};

use crate::sequencer::effect::Effect;
use crate::sequencer::instrument::{DuplicateCheckAction, DuplicateCheckType, NewNoteAction};
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::sequencer::player::ChannelState;
use crate::sequencer::sample::Sample;

impl SequencerEngine {
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
                    self.state.channels[channel].last_note = Note::On(key);

                    let has_plugin = has_instruments
                        && instrument_idx > 0
                        && instrument_idx < module.instruments.len()
                        && module.instruments[instrument_idx].plugin.is_some();

                    if has_plugin {
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
                    } else if is_tone_portamento {
                        self.with_processor_mut(|processor, engine| processor.setup_portamento(engine, channel, key, remapped_key, sample, sample_idx));
                    } else {
                        self.with_processor_mut(|processor, engine| processor.trigger_note(engine, channel, key, remapped_key, sample, sample_idx, cell, instrument_idx));
                    }
                }
                Note::Off => {
                    let has_plugin = has_instruments
                        && instrument_idx > 0
                        && instrument_idx < module.instruments.len()
                        && module.instruments[instrument_idx].plugin.is_some();
                    if has_plugin {
                        let midi_ch = module.instruments[instrument_idx]
                            .midi_base_channel
                            .wrapping_add(channel as u8) % 16;
                        self.pending_plugin_note_events.push(PluginNoteEvent {
                            instrument_idx: instrument_idx as u8,
                            midi_channel: midi_ch,
                            key: 0,
                            velocity: 0,
                            note_on: false,
                        });
                    } else {
                        self.with_processor_mut(|processor, engine| processor.handle_note_off(engine, channel));
                    }
                }
                Note::Cut => {
                    self.cut_channel_voices(channel);
                }
                Note::Fade => {
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
