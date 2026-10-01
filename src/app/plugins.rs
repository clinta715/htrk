use super::*;

impl HtrkApp {
    pub(super) fn load_and_install_instrument_plugin(
        &mut self,
        descriptor: &crate::audio::plugins::PluginDescriptor,
        instrument_idx: usize,
        initial_state: Option<&[u8]>,
    ) -> Result<(Box<dyn HostedPluginHandle>, String), String> {
        let sample_rate = if self.current_sample_rate > 0 {
            self.current_sample_rate as f64
        } else {
            48000.0
        };
        crate::ui::plugin_browser::load_and_install_instrument_plugin(
            descriptor,
            instrument_idx,
            initial_state,
            sample_rate,
            &mut self.core.command_sender,
        )
    }

    pub(super) fn unload_instrument_plugin(&mut self, instrument_idx: usize) {
        if instrument_idx >= self.instrument_plugin_handles.len() {
            return;
        }
        // Save state before dropping the handle. The handle's save_state()
        // is &mut self so we need a temporary move out + back in.
        if let Some(mut handle) = self.instrument_plugin_handles[instrument_idx].take() {
            if let Ok(state) = handle.save_state() {
                self.write_instrument_plugin_state(instrument_idx, state);
            }
            // Release any previewed note-off for this instrument so a
            // reloaded plugin doesn't get a stale "note on" without off.
            self.send_all_preview_note_offs_for_instrument(instrument_idx);
            // Drop the handle (plugin instance deallocated); send None to
            // audio engine so the processor is dropped there too. A proper
            // deactivation sequence would need a way to retrieve the
            // stopped processor from the audio thread — deferred to Phase 4.
            drop(handle);
        }
        if let Some(ref mut sender) = self.core.command_sender {
            sender.send(AudioCommand::InstallInstrumentPlugin {
                instrument_idx,
                processor: None,
            });
        }
    }

    /// Copy the given opaque state blob into the module's instrument slot
    /// so the next save persists it. Does nothing if the slot is missing.
    pub(super) fn write_instrument_plugin_state(&mut self, instrument_idx: usize, state: Vec<u8>) {
        self.core.ensure_module_ownership();
        if let Some(ref mut module) = self.core.module {
            if let Some(arc_module) = std::sync::Arc::get_mut(module) {
                crate::ui::plugin_browser::write_plugin_state_to_slot(
                    arc_module,
                    |m| {
                        if instrument_idx < m.instruments.len() {
                            m.instruments[instrument_idx].plugin.as_mut()
                        } else {
                            None
                        }
                    },
                    state,
                );
            }
        }
    }

    /// Iterate all loaded instrument plugins, capture their state, and
    /// write it into the module's `instrument.plugin.state` field. Call
    /// this on project save so plugins reload with the same patches.
    pub(crate) fn save_all_instrument_plugin_states(&mut self) {
        // Borrow the two pieces we need as separate locals so the
        // closure passed to save_all_plugin_states can borrow them
        // independently (avoids &mut self / &mut self conflict).
        let core_module = &mut self.core.module;
        let handles = std::mem::take(&mut self.instrument_plugin_handles);
        let mut handles = handles;
        crate::ui::plugin_browser::save_all_plugin_states(&mut handles, |idx, state| {
            // Write the state into the module slot (if it still exists).
            if let Some(arc_module) = core_module
                .as_mut()
                .and_then(|m| std::sync::Arc::get_mut(m))
            {
                crate::ui::plugin_browser::write_plugin_state_to_slot(
                    arc_module,
                    |m| {
                        if idx < m.instruments.len() {
                            m.instruments[idx].plugin.as_mut()
                        } else {
                            None
                        }
                    },
                    state,
                );
            }
        });
        self.instrument_plugin_handles = handles;
    }

    /// Send note-off for all currently-held preview notes for the given
    /// instrument. Called when the instrument is unloaded or the module
    /// is replaced.
    pub(super) fn send_all_preview_note_offs_for_instrument(&mut self, instrument_idx: usize) {
        let mut remaining = Vec::new();
        for entry in self.preview_held_notes.drain(..) {
            let (key, inst, midi_ch, note_key) = entry;
            if inst as usize == instrument_idx {
                self.core
                    .send_command(AudioCommand::PreviewInstrumentPluginNoteOff {
                        instrument_idx: inst as usize,
                        midi_channel: midi_ch,
                        note_key,
                    });
            } else {
                remaining.push((key, inst, midi_ch, note_key));
            }
        }
        self.preview_held_notes = remaining;
    }

    /// Release any previewed instrument-plugin notes whose trigger key
    /// is no longer held. Called every frame from `ui()`.
    pub(super) fn release_unheld_preview_notes(&mut self, ctx: &egui::Context) {
        if self.preview_held_notes.is_empty() {
            return;
        }
        let mut remaining = Vec::with_capacity(self.preview_held_notes.len());
        for (key, inst, midi_ch, note_key) in self.preview_held_notes.drain(..) {
            let still_held = ctx.input(|i| i.key_down(key));
            if still_held {
                remaining.push((key, inst, midi_ch, note_key));
            } else {
                self.core
                    .send_command(AudioCommand::PreviewInstrumentPluginNoteOff {
                        instrument_idx: inst as usize,
                        midi_channel: midi_ch,
                        note_key,
                    });
            }
        }
        self.preview_held_notes = remaining;
    }

    // ─── Send-bus plugin state persistence (mirrors instrument side) ─────

    /// Iterate all loaded send-bus plugin handles, capture their state, and
    /// write it into the module's `send_bus_plugins[i].state` field. Call
    /// this on project save so plugins reload with the same patches.
    pub(crate) fn save_all_send_bus_plugin_states(&mut self) {
        // Borrow the two pieces we need as separate locals so the
        // closure passed to save_all_plugin_states can borrow them
        // independently (avoids &mut self / &mut self conflict).
        let core_module = &mut self.core.module;
        let handles = std::mem::take(&mut self.send_bus_handles);
        let mut handles = handles;
        crate::ui::plugin_browser::save_all_plugin_states(&mut handles, |idx, state| {
            if let Some(arc_module) = core_module
                .as_mut()
                .and_then(|m| std::sync::Arc::get_mut(m))
            {
                crate::ui::plugin_browser::write_plugin_state_to_slot(
                    arc_module,
                    |m| {
                        if idx < m.send_bus_plugins.len() {
                            m.send_bus_plugins[idx].as_mut()
                        } else {
                            None
                        }
                    },
                    state,
                );
            }
        });
        self.send_bus_handles = handles;
    }

    /// Reload any send-bus plugin slots from the current module. Called
    /// from `load_file` and `new_song` so opening a saved `.htk` with
    /// send-bus plugin assignments brings the plugins back to life
    /// automatically (with their saved state restored). Slots whose path
    /// is no longer in the discovered library are left in place but not
    /// loaded; the user can re-pick from the browser to fix the path.
    pub(crate) fn sync_send_bus_plugin_state(&mut self) {
        let slots: Vec<(usize, String, String, Vec<u8>)> = match self.core.module.as_ref() {
            Some(m) => m
                .send_bus_plugins
                .iter()
                .enumerate()
                .filter_map(|(idx, opt)| {
                    opt.as_ref().map(|slot| {
                        (
                            idx,
                            slot.format.clone(),
                            slot.path.clone(),
                            slot.state.clone(),
                        )
                    })
                })
                .collect(),
            None => return,
        };
        let discovered = self.discovered_plugins();

        // Borrow the disjoint fields the two closures need.
        let core_command_sender = &mut self.core.command_sender;
        let send_bus_handles = &mut self.send_bus_handles;
        let sendfx_panel_plugin_names = &mut self.sendfx_panel.plugin_names;
        let sample_rate = if self.current_sample_rate > 0 {
            self.current_sample_rate as f64
        } else {
            48000.0
        };

        crate::ui::plugin_browser::sync_plugin_slots_from_module(
            slots,
            &discovered,
            "send bus",
            |send_index, descriptor, initial_state| {
                let (handle, processor, name) =
                    crate::ui::plugin_browser::load_and_activate_clap_plugin(
                        descriptor,
                        sample_rate,
                        512,
                        initial_state,
                    )?;
                if let Some(ref mut sender) = core_command_sender {
                    sender.send(AudioCommand::SetSendPlugin {
                        send_index,
                        processor: Some(processor),
                    });
                } else {
                    return Err("No command sender — audio engine not running?".into());
                }
                Ok((handle, name))
            },
            |send_index, handle, name| {
                send_bus_handles[send_index] = Some(handle);
                sendfx_panel_plugin_names[send_index] = Some(name);
            },
        );
    }
}
