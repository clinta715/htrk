use super::*;

impl HtrkApp {
    pub fn refresh_output_devices(&mut self) {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        self.output_device_names = host
            .output_devices()
            .map(|iter| {
                iter.filter_map(|d| d.description().ok().map(|desc| desc.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if self.selected_device_name.is_none() {
            self.selected_device_name = host
                .default_output_device()
                .and_then(|d| d.description().ok().map(|desc| desc.to_string()));
        }
    }

    pub fn init_audio(&mut self) {
        if self.stream.is_some() {
            return;
        }
        if self.output_device_names.is_empty() {
            self.refresh_output_devices();
        }
        // Initialize selected device from persisted config if not set
        if self.selected_device_name.is_none() {
            if let Some(ref name) = self.config.output_device_name {
                if self.output_device_names.iter().any(|d| d == name) {
                    self.selected_device_name = Some(name.clone());
                }
            }
        }

        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let device = if let Some(ref name) = self.selected_device_name {
            host.output_devices().ok().and_then(|mut devs| {
                devs.find(|d| {
                    d.description().ok().map(|desc| desc.to_string()).as_deref()
                        == Some(name.as_str())
                })
            })
        } else {
            None
        };
        let device = device.or_else(|| host.default_output_device());

        let device = match device {
            Some(d) => {
                #[cfg(feature = "audio_debug")]
                debug_log!(
                    "[AUDIO] Using device: {:?}",
                    d.description().ok().map(|desc| desc.to_string())
                );
                d
            }
            None => {
                eprintln!("[AUDIO] No audio output device available");
                self.audio_init_failed = true;
                return;
            }
        };

        let supported_config = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[AUDIO] Failed to get default output config: {}", e);
                self.audio_init_failed = true;
                return;
            }
        };

        let actual_sample_rate = supported_config.sample_rate();
        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        #[cfg(feature = "audio_debug")]
        debug_log!(
            "[AUDIO] Sample rate: {}, format: {:?}, channels: {}",
            actual_sample_rate,
            sample_format,
            config.channels
        );

        self.current_sample_rate = actual_sample_rate;
        self.current_sample_format = format!("{:?}", sample_format);
        if self.selected_device_name.is_none() {
            self.selected_device_name = device.description().ok().map(|desc| desc.to_string());
        }

        let mut configs_to_try = Vec::new();

        // If user has a preferred sample rate, try it first
        if let Some(pref_sr) = self.config.preferred_sample_rate {
            configs_to_try.push(cpal::StreamConfig {
                channels: config.channels,
                sample_rate: pref_sr,
                buffer_size: cpal::BufferSize::Default,
            });
        }

        configs_to_try.push(config.clone());
        let alt1 = cpal::StreamConfig {
            channels: config.channels,
            sample_rate: config.sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };
        if alt1 != config {
            configs_to_try.push(alt1);
        }
        for &sr in &[44100u32, 48000, 22050] {
            if Some(sr) != self.config.preferred_sample_rate && sr != config.sample_rate {
                configs_to_try.push(cpal::StreamConfig {
                    channels: 2,
                    sample_rate: sr,
                    buffer_size: cpal::BufferSize::Default,
                });
            }
        }

        let mut stream_result = Err(cpal::BuildStreamError::DeviceNotAvailable);
        let mut sender = None;
        for trial_config in &configs_to_try {
            let state = self.core.playback_state.clone();
            let (mut engine, trial_sender) =
                create_engine_and_sender(state, trial_config.sample_rate, trial_config.channels);

            let trial_result = match sample_format {
                cpal::SampleFormat::F32 => device.build_output_stream(
                    trial_config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        engine.process_callback(data);
                    },
                    |err| eprintln!("Audio stream error: {}", err),
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_output_stream(
                    trial_config,
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        let mut float_buf = vec![0.0f32; data.len()];
                        engine.process_callback(&mut float_buf);
                        for (out, inp) in data.iter_mut().zip(float_buf.iter()) {
                            *out = (*inp * 32768.0).clamp(-32768.0, 32767.0) as i16;
                        }
                    },
                    |err| eprintln!("Audio stream error: {}", err),
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_output_stream(
                    trial_config,
                    move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                        let mut float_buf = vec![0.0f32; data.len()];
                        engine.process_callback(&mut float_buf);
                        for (out, inp) in data.iter_mut().zip(float_buf.iter()) {
                            let s = (*inp * 32768.0).clamp(-32768.0, 32767.0) as i16;
                            *out = (s as i32 + 32768) as u16;
                        }
                    },
                    |err| eprintln!("Audio stream error: {}", err),
                    None,
                ),
                _ => {
                    eprintln!("Unsupported sample format: {:?}", sample_format);
                    self.audio_init_failed = true;
                    return;
                }
            };

            match trial_result {
                Ok(s) => {
                    stream_result = Ok(s);
                    sender = Some(trial_sender);
                    break;
                }
                Err(e) => {
                    eprintln!(
                        "[AUDIO] Config {}ch/{}Hz failed: {}",
                        trial_config.channels, trial_config.sample_rate, e
                    );
                    stream_result = Err(e);
                }
            }
        }

        match stream_result {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    eprintln!("[AUDIO] Failed to start audio stream: {}", e);
                    self.audio_init_failed = true;
                    return;
                }
                #[cfg(feature = "audio_debug")]
                debug_log!("[AUDIO] Audio stream started successfully");
                self.audio_init_failed = false;
                self.current_sample_rate = actual_sample_rate;
                self.current_sample_format = format!("{:?}", sample_format);
                self.stream = Some(stream);
                self.core.command_sender = sender;
                if let Some(ref module) = self.core.module {
                    self.core
                        .send_command(AudioCommand::LoadModule(module.clone()));
                }
                // Apply persisted audio settings to the new engine
                self.apply_audio_settings_to_engine();
            }
            Err(e) => {
                eprintln!("[AUDIO] Failed to create audio stream: {}", e);
                self.audio_init_failed = true;
            }
        }
    }

    pub(super) fn switch_output_device(&mut self, device_name: String) {
        self.stream = None;
        self.core.command_sender = None;
        self.selected_device_name = Some(device_name);
        self.init_audio();
    }

    pub(crate) fn sync_channel_fields(&mut self) {
        self.core.sync_channel_fields();
        let count = self.core.num_channels();
        self.multichannel_channels.resize(count, false);
        if self.pattern_view.channel_names.len() < count {
            let old = self.pattern_view.channel_names.len();
            self.pattern_view
                .channel_names
                .resize_with(count, String::new);
            for i in old..count {
                self.pattern_view.channel_names[i] = format!("Ch{}", i + 1);
            }
        }
    }

    pub(crate) fn sync_send_bus_state(&mut self) {
        if let Some(ref module) = self.core.module {
            self.sendfx_panel.effect_types = module.send_bus_config;
            self.sendfx_panel.params = [
                [module.send_return_levels[0], 0.0, 0.0, 0.0, 0.0],
                [module.send_return_levels[1], 0.0, 0.0, 0.0, 0.0],
                [module.send_return_levels[2], 0.0, 0.0, 0.0, 0.0],
                [module.send_return_levels[3], 0.0, 0.0, 0.0, 0.0],
            ];
            self.sendfx_panel.pre_fader = module.send_pre_fader;
        }
    }

    /// Reload any instrument plugin slots from the current module.
    /// Called from `load_file` and `new_song` so that opening a saved
    /// `.htk` with instrument plugin assignments brings the plugins
    /// back to life automatically (with their saved state restored).
    /// Slots whose path no longer matches a discovered plugin are
    /// left in place but not loaded; the user can re-pick from the
    /// browser to fix the path.
    pub(crate) fn sync_instrument_plugin_state(&mut self) {
        // Snapshot the list of (index, slot) pairs up front so we can
        // mutate `self` inside the loop without aliasing.
        let slots: Vec<(usize, String, String, Vec<u8>)> = match self.core.module.as_ref() {
            Some(m) => m
                .instruments
                .iter()
                .enumerate()
                .filter_map(|(idx, inst)| {
                    inst.plugin.as_ref().map(|slot| {
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

        // Borrow the disjoint fields the two closures need as
        // separate locals so the borrow checker doesn't see them as
        // overlapping &mut self borrows.
        let core_command_sender = &mut self.core.command_sender;
        let instrument_plugin_handles = &mut self.instrument_plugin_handles;

        crate::ui::plugin_browser::sync_plugin_slots_from_module(
            slots,
            &discovered,
            "instrument",
            |inst_idx, descriptor, initial_state| {
                let sample_rate = if self.current_sample_rate > 0 {
                    self.current_sample_rate as f64
                } else {
                    48000.0
                };
                crate::ui::plugin_browser::load_and_install_instrument_plugin(
                    descriptor,
                    inst_idx,
                    initial_state,
                    sample_rate,
                    core_command_sender,
                )
            },
            |inst_idx, handle, _name| {
                if inst_idx < instrument_plugin_handles.len() {
                    instrument_plugin_handles[inst_idx] = Some(handle);
                }
            },
        );
    }

    pub(super) fn apply_audio_settings_to_engine(&mut self) {
        let interp = match self.config.default_interpolation.as_str() {
            "Nearest" => crate::audio::commands::InterpolationType::Nearest,
            "Cubic" => crate::audio::commands::InterpolationType::Cubic,
            _ => crate::audio::commands::InterpolationType::Linear,
        };
        self.core
            .send_command(crate::audio::commands::AudioCommand::SetInterpolation(
                interp,
            ));

        let limiter = match self.config.limiter_mode.as_str() {
            "SoftKnee" => crate::audio::commands::LimiterMode::SoftKnee,
            "SoftKneeSmooth" => crate::audio::commands::LimiterMode::SoftKneeSmooth,
            _ => crate::audio::commands::LimiterMode::HardClip,
        };
        self.core
            .send_command(crate::audio::commands::AudioCommand::SetLimiterMode(
                limiter,
            ));

        self.core
            .send_command(crate::audio::commands::AudioCommand::SetAntiClickRamping(
                self.config.anti_click_ramping,
            ));
    }

    pub(super) fn apply_config_to_live_state(&mut self) {
        self.follow_playback = self.config.follow_playback_default;
        self.sample_editor.amplify_factor = self.config.default_amplify_factor;
        if let Some(preset) = ThemePreset::from_name(&self.config.theme_preset) {
            self.theme_preset = preset;
            self.theme = TrackerTheme::from_preset(preset);
        }
        self.file_browser.restore_last_dirs(&self.config);

        self.apply_audio_settings_to_engine();

        // Apply debug logging
        let config_dir = crate::app_config::AppConfig::config_dir();
        crate::debug_log::init(self.config.debug, config_dir);
    }

    pub(crate) fn new_song(&mut self) {
        self.core.new_song();
        self.pattern_view.scroll_row = 0;
        self.pattern_view.scroll_channel = 0;
        self.sync_channel_fields();
        self.sync_send_bus_state();
        // A new song has no plugin slots, but call these anyway for
        // symmetry with load_file.
        self.sync_send_bus_plugin_state();
        self.sync_instrument_plugin_state();
    }
}
