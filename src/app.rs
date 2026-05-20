use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;

use crate::app_config::AppConfig;
use crate::audio::commands::AudioCommand;
use crate::audio::engine::create_engine_and_sender;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::audio::renderer::WavRenderer;
use crate::formats;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::sequencer::effect::SendEffectType;
use crate::sequencer::pattern::Cell;
use crate::sequencer::{Effect, Note, DEFAULT_CHANNELS, MAX_CHANNELS};
use crate::ui::file_browser::{BrowserMode, FileBrowser};
use crate::ui::pattern_grid::{ColumnVisibility, Selection, SubColumn, VISIBLE_ROWS};
use crate::ui::theme::ThemePreset;
use crate::ui::TrackerTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppView {
    Pattern,
    Sample,
    Instrument,
    SendFx,
    Playback,
    Automation,
}

pub struct HtrkApp {
    pub(crate) core: crate::core::HtrkCore,
    pub(crate) stream: Option<cpal::Stream>,

    pub(crate) output_device_names: Vec<String>,
    pub(crate) selected_device_name: Option<String>,
    pub(crate) current_sample_rate: u32,
    pub(crate) current_sample_format: String,
    pub(crate) pending_device_switch: Option<String>,
    pub(crate) pending_reinit: bool,

    pub(crate) file_browser: FileBrowser,

    pub(crate) current_view: AppView,

    pub(crate) scroll_row: usize,
    pub(crate) scroll_channel: usize,

    pub(crate) current_octave: u8,
    pub(crate) edit_mode: bool,
    pub(crate) follow_playback: bool,
    pub(crate) cursor_skip: u8,
    pub(crate) edit_mask_instrument: bool,
    pub(crate) edit_mask_volume: bool,
    pub(crate) multichannel_enabled: bool,
    pub(crate) multichannel_channels: Vec<bool>,

    pub(crate) channel_names: Vec<String>,
    pub(crate) channel_rename_state: crate::ui::channel_headers::ChannelRenameState,

    pub(crate) theme: TrackerTheme,
    pub(crate) theme_preset: crate::ui::theme::ThemePreset,
    pub(crate) show_shortcuts: bool,
    pub(crate) show_about: bool,
    pub(crate) settings_state: crate::ui::settings_window::SettingsState,
    pub(crate) wav_export_state: crate::ui::wav_export_window::WavExportState,
    pub(crate) sample_export_dialog: Option<crate::ui::sample_export_dialog::SampleExportDialog>,
    pub(crate) audio_init_failed: bool,
    pub(crate) sample_selection: Option<(usize, usize)>,
    pub(crate) sample_clipboard: Option<Arc<Vec<f32>>>,
    pub(crate) amplify_factor: f32,
    pub(crate) config: AppConfig,
    pub(crate) col_vis: ColumnVisibility,
    pub(crate) last_visible_rows: usize,
    pub(crate) last_visible_channels: usize,
    pub(crate) send_bus_effect_types: [SendEffectType; NUM_SEND_BUSES],
    pub(crate) send_bus_params: [[f32; 5]; NUM_SEND_BUSES],
    pub(crate) automation_dragging: Option<(usize, f32)>,
    pub(crate) automation_editor_state: crate::ui::automation_editor::AutomationEditorState,
}

impl Default for HtrkApp {
    fn default() -> Self {
        let config = AppConfig::load();
        let mut file_browser = FileBrowser::default();
        file_browser.restore_last_dirs(&config);
        file_browser.restore_favorites(&config.favorites);
        let playback_state = Arc::new(AtomicPlaybackState::default());
        HtrkApp {
            core: crate::core::HtrkCore::new(playback_state.clone()),
            stream: None,
            output_device_names: Vec::new(),
            selected_device_name: None,
            current_sample_rate: 0,
            current_sample_format: String::new(),
            pending_device_switch: None,
            pending_reinit: false,
            file_browser,
            current_view: AppView::Pattern,
            scroll_row: 0,
            scroll_channel: 0,
            current_octave: 4,
            edit_mode: true,
            follow_playback: config.follow_playback_default,
            cursor_skip: 1,
            edit_mask_instrument: true,
            edit_mask_volume: true,
            multichannel_enabled: false,
            multichannel_channels: vec![false; DEFAULT_CHANNELS],
            channel_names: (0..DEFAULT_CHANNELS).map(|i| format!("Ch{}", i + 1)).collect(),
            channel_rename_state: crate::ui::channel_headers::ChannelRenameState::default(),
            theme: TrackerTheme::from_preset(
                ThemePreset::from_name(&config.theme_preset).unwrap_or(ThemePreset::DarkModern)
            ),
            theme_preset: ThemePreset::from_name(&config.theme_preset).unwrap_or(ThemePreset::DarkModern),
            show_shortcuts: false,
            show_about: false,
            settings_state: crate::ui::settings_window::SettingsState::from_config(&config),
            wav_export_state: crate::ui::wav_export_window::WavExportState::new(44100),
            sample_export_dialog: None,
            audio_init_failed: false,
            sample_selection: None,
            sample_clipboard: None,
            amplify_factor: config.default_amplify_factor,
            col_vis: config.get_col_vis(),
            config,
            last_visible_rows: VISIBLE_ROWS,
            last_visible_channels: 16,
            send_bus_effect_types: [
                SendEffectType::Delay,
                SendEffectType::Reverb,
                SendEffectType::None,
                SendEffectType::None,
            ],
            send_bus_params: [
                [0.5, 1.0, 0.4, 0.3, 1.0],
                [0.0, 0.7, 0.5, 0.6, 0.5],
                [0.0, 0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 0.0, 0.0],
            ],
            automation_dragging: None,
            automation_editor_state: crate::ui::automation_editor::AutomationEditorState::default(),
        }
    }
}

impl HtrkApp {
    pub fn refresh_output_devices(&mut self) {
        use cpal::traits::{HostTrait, DeviceTrait};
        let host = cpal::default_host();
        self.output_device_names = host
            .output_devices()
            .map(|iter| iter.filter_map(|d| d.description().ok().map(|desc| desc.to_string())).collect())
            .unwrap_or_default();
        if self.selected_device_name.is_none() {
            self.selected_device_name = host.default_output_device().and_then(|d| d.description().ok().map(|desc| desc.to_string()));
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

        use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};

        let host = cpal::default_host();
        let device = if let Some(ref name) = self.selected_device_name {
            host.output_devices()
                .ok()
                .and_then(|mut devs| devs.find(|d| d.description().ok().map(|desc| desc.to_string()).as_deref() == Some(name.as_str())))
        } else {
            None
        };
        let device = device.or_else(|| host.default_output_device());

        let device = match device {
            Some(d) => {
                #[cfg(feature = "audio_debug")]
                debug_log!("[AUDIO] Using device: {:?}", d.description().ok().map(|desc| desc.to_string()));
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
        debug_log!("[AUDIO] Sample rate: {}, format: {:?}, channels: {}", actual_sample_rate, sample_format, config.channels);

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
            let (mut engine, trial_sender) = create_engine_and_sender(state, trial_config.sample_rate, trial_config.channels);

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
                    eprintln!("[AUDIO] Config {}ch/{}Hz failed: {}", trial_config.channels, trial_config.sample_rate, e);
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
                    self.send_command(AudioCommand::LoadModule(module.clone()));
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

    fn switch_output_device(&mut self, device_name: String) {
        self.stream = None;
        self.core.command_sender = None;
        self.selected_device_name = Some(device_name);
        self.init_audio();
    }

    pub(crate) fn send_command(&mut self, cmd: AudioCommand) {
        self.core.send_command(cmd);
    }

    pub(crate) fn ensure_module_ownership(&mut self) {
        self.core.ensure_module_ownership();
    }

    pub(crate) fn sync_module_to_audio(&mut self) {
        self.core.sync_to_audio();
    }

    fn sync_channel_fields(&mut self) {
        self.core.sync_channel_fields();
        let count = self.core.num_channels();
        self.multichannel_channels.resize(count, false);
        if self.channel_names.len() < count {
            let old = self.channel_names.len();
            self.channel_names.resize_with(count, || String::new());
            for i in old..count {
                self.channel_names[i] = format!("Ch{}", i + 1);
            }
        } else {
            self.channel_names.truncate(count);
        }
    }

    fn apply_audio_settings_to_engine(&mut self) {
        let interp = match self.config.default_interpolation.as_str() {
            "Nearest" => crate::audio::commands::InterpolationType::Nearest,
            "Cubic" => crate::audio::commands::InterpolationType::Cubic,
            _ => crate::audio::commands::InterpolationType::Linear,
        };
        self.send_command(crate::audio::commands::AudioCommand::SetInterpolation(interp));

        let limiter = match self.config.limiter_mode.as_str() {
            "SoftKnee" => crate::audio::commands::LimiterMode::SoftKnee,
            "SoftKneeSmooth" => crate::audio::commands::LimiterMode::SoftKneeSmooth,
            _ => crate::audio::commands::LimiterMode::HardClip,
        };
        self.send_command(crate::audio::commands::AudioCommand::SetLimiterMode(limiter));
    }

    fn apply_config_to_live_state(&mut self) {
        self.follow_playback = self.config.follow_playback_default;
        self.amplify_factor = self.config.default_amplify_factor;
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

    fn load_file(&mut self, path: &str) {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read file: {}", e);
                return;
            }
        };

        if data.len() < 4 {
            eprintln!("File too small to be a valid module");
            return;
        }

        match formats::load_module(&data) {
            Ok(mut module) => {
                if module.samples.len() < 65 {
                    module.samples.resize(65, crate::sequencer::Sample::default());
                }
                if module.instruments.len() < 17 {
                    module.instruments.resize(17, crate::sequencer::Instrument::default());
                }

                let name = module.name.clone();
                self.core.load_module(module, name, Some(path.to_string()));
                self.scroll_row = 0;
                self.scroll_channel = 0;
                self.sync_channel_fields();
            }
            Err(e) => {
                eprintln!("Failed to load module: {}", e);
            }
        }
    }

    pub(crate) fn new_song(&mut self) {
        self.core.new_song();
        self.scroll_row = 0;
        self.scroll_channel = 0;
        self.sync_channel_fields();
    }

    pub(crate) fn current_pattern(&self) -> Option<&crate::sequencer::Pattern> {
        self.core.current_pattern()
    }

    #[allow(dead_code)]
    fn current_pattern_mut(&mut self) -> Option<&mut crate::sequencer::Pattern> {
        self.core.current_pattern_mut()
    }

    pub(crate) fn num_channels(&self) -> usize {
        self.core.num_channels()
    }

    pub(crate) fn num_channels_checked(&self) -> usize {
        self.core.num_channels_checked()
    }

    pub(crate) fn get_cell_at_cursor(&self) -> Cell {
        self.core.get_cell_at_cursor()
    }

    pub(crate) fn set_cell_at_cursor(&mut self, new_cell: Cell) {
        self.core.set_cell_at_cursor(new_cell, &self.multichannel_channels, self.multichannel_enabled);
    }

    pub(crate) fn advance_cursor_down(&mut self, step: usize) {
        if let Some(pattern) = self.current_pattern() {
            let max_row = pattern.num_rows.max(1);
            self.core.cursor.row = (self.core.cursor.row + step).min(max_row - 1);
        }
        self.ensure_cursor_visible();
    }

    pub(crate) fn advance_cursor_up(&mut self, step: usize) {
        self.core.cursor.row = self.core.cursor.row.saturating_sub(step);
        self.ensure_cursor_visible();
    }

    pub(crate) fn ensure_cursor_visible(&mut self) {
        if self.core.cursor.row < self.scroll_row {
            self.scroll_row = self.core.cursor.row;
        }
        if self.core.cursor.row >= self.scroll_row + self.last_visible_rows {
            self.scroll_row = self.core.cursor.row - self.last_visible_rows + 1;
        }

        if self.core.cursor.channel < self.scroll_channel {
            self.scroll_channel = self.core.cursor.channel;
        }
        if self.core.cursor.channel >= self.scroll_channel + self.last_visible_channels {
            self.scroll_channel = self.core.cursor.channel - self.last_visible_channels + 1;
        }
    }

    pub(crate) fn clear_cell_at_cursor(&mut self) {
        self.core.clear_cell_at_cursor();
    }

    pub(crate) fn move_cursor_right(&mut self) {
        let num_ch = self.num_channels();
        if self.core.cursor.channel < num_ch - 1 {
            self.core.cursor.channel += 1;
            self.core.cursor.sub_column = SubColumn::Note;
        }
        self.ensure_cursor_visible();
    }

    pub(crate) fn move_cursor_left(&mut self) {
        if self.core.cursor.channel > 0 {
            self.core.cursor.channel -= 1;
            self.core.cursor.sub_column = SubColumn::Note;
        }
        self.ensure_cursor_visible();
    }

    pub(crate) fn first_visible_sub_column(col_vis: crate::ui::pattern_grid::ColumnVisibility) -> crate::ui::pattern_grid::SubColumn {
        use crate::ui::pattern_grid::SubColumn;
        if col_vis.note { return SubColumn::Note; }
        if col_vis.instrument { return SubColumn::InstrumentTens; }
        if col_vis.volume { return SubColumn::VolumeTens; }
        if col_vis.effect { return SubColumn::EffectType; }
        SubColumn::Note
    }

    pub(crate) fn step_sub_column_forward(&mut self) {
        let col_vis = self.config.get_col_vis();
        if let Some(next) = self.core.cursor.sub_column.next_visible(col_vis) {
            self.core.cursor.sub_column = next;
        }
    }

    pub(crate) fn step_sub_column_backward(&mut self) {
        let col_vis = self.config.get_col_vis();
        if let Some(prev) = self.core.cursor.sub_column.prev_visible(col_vis) {
            self.core.cursor.sub_column = prev;
        }
    }

    pub(crate) fn cycle_spacing_mode(&mut self) {
        use crate::app_config::SpacingMode;
        let modes = [SpacingMode::Compact, SpacingMode::Normal, SpacingMode::Wide, SpacingMode::ExtraWide];
        let current = self.config.get_spacing_mode();
        let idx = modes.iter().position(|&m| m == current).unwrap_or(1);
        let next = modes[(idx + 1) % modes.len()];
        self.config.set_spacing_mode(next);
    }

    pub(crate) fn extend_selection_down(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.advance_cursor_down(1);
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn extend_selection_up(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.advance_cursor_up(1);
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn extend_selection_right(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.move_cursor_right();
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn extend_selection_left(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.move_cursor_left();
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn select_all(&mut self) {
        self.core.select_all();
    }

    pub(crate) fn transpose_selection(&mut self, delta: i8) {
        self.core.transpose_selection(delta);
    }

    pub(crate) fn handle_context_menu_action(&mut self, action: crate::ui::pattern_grid::ContextMenuAction) {
        if !self.edit_mode {
            return;
        }
        self.core.handle_context_menu_action(action);
    }

    fn handle_automation_interaction(&mut self, interaction: crate::ui::pattern_grid::AutomationInteraction) {
        self.core.handle_automation_interaction(interaction);
    }

    pub(crate) fn enter_automation_hex(&mut self, channel: usize, row: usize, digit: u8) {
        self.core.enter_automation_hex(channel, row, digit);
    }

    pub(crate) fn delete_automation_point(&mut self, channel: usize, row: usize) {
        self.core.delete_automation_point(channel, row);
    }

    pub(crate) fn skip_to_prev_pattern(&mut self) {
        self.core.skip_to_prev_pattern();
        self.ensure_cursor_visible();
    }

    pub(crate) fn skip_to_next_pattern(&mut self) {
        self.core.skip_to_next_pattern();
        self.ensure_cursor_visible();
    }

    pub(crate) fn copy_selection(&mut self) {
        self.core.copy_selection();
    }

    pub(crate) fn delete_selection(&mut self) {
        self.core.delete_selection();
    }

    pub(crate) fn paste_at_cursor(&mut self) {
        self.core.paste_at_cursor();
    }

    pub(crate) fn open_file_dialog(&mut self) {
        self.file_browser.open(BrowserMode::Modules, &mut self.config);
    }

    fn import_wav(&mut self, path: &str) {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read WAV file: {}", e);
                return;
            }
        };
        match crate::formats::wav::import_wav(&data) {
            Ok(mut sample) => {
                if sample.name.is_empty() {
                    if let Some(name) = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()) {
                        sample.name = name.to_string();
                    }
                }

                if self.core.module.is_none() {
                    self.new_song();
                }

                let sample_idx = self.core.selected_sample;
                self.core.import_wav_to_sample(sample_idx, sample);
            }
            Err(e) => {
                eprintln!("Failed to import WAV: {}", e);
            }
        }
    }

    fn save_current_file(&mut self) {
        let path = match &self.core.file_path {
            Some(p) => p.clone(),
            None => {
                self.save_as_dialog();
                return;
            }
        };
        self.save_file(&path);
    }

    fn save_file(&mut self, path: &str) {
        self.core.save_file(path);
    }

    pub(crate) fn save_as_dialog(&mut self) {
        self.file_browser.open(BrowserMode::Projects, &mut self.config);
    }

    fn open_wav_export_dialog(&mut self) {
        let module_loaded = self.core.module.is_some();
        let total_orders = self.core.module.as_ref().map(|m| m.order_list.len()).unwrap_or(0) as u64;
        let sample_rate = if self.current_sample_rate > 0 {
            self.current_sample_rate
        } else {
            44100
        };

        let default_name = if !self.core.loaded_module_name.is_empty() {
            self.core.loaded_module_name.clone()
        } else {
            "untitled".to_string()
        };

        self.wav_export_state.default_directory = self.config.default_wav_path.as_ref().map(|p| {
            let pb = std::path::PathBuf::from(p);
            if pb.is_dir() { pb } else { std::path::PathBuf::new() }
        }).filter(|p| p.as_os_str().is_empty().then(|| false).unwrap_or(true));

        self.wav_export_state.open(&default_name, module_loaded, Some(total_orders), sample_rate);
        self.wav_export_state.update_estimates(Some(total_orders));
    }

    fn export_wav_with_settings(&mut self) {
        let settings = self.wav_export_state.settings().clone();

        let module = match &self.core.module {
            Some(m) => m.clone(),
            None => {
                self.wav_export_state.finish_export(false, Some("No module loaded to export".to_string()));
                return;
            }
        };

        let file_path = match settings.file_path.clone() {
            Some(path) => path,
            None => {
                self.wav_export_state.finish_export(false, Some("No file path selected".to_string()));
                return;
            }
        };

        let sample_rate = if settings.sample_rate > 0 {
            settings.sample_rate
        } else {
            self.current_sample_rate
        };

        if sample_rate == 0 {
            self.wav_export_state.finish_export(false, Some("No valid sample rate available".to_string()));
            return;
        }

        let progress_arc = self.wav_export_state.progress_arc();
        let state_arc = self.wav_export_state.state_arc();
        let cancel_arc = self.wav_export_state.cancel_arc();

        // Extract config values before the move closure
        let interp_str = self.config.default_interpolation.clone();
        let limiter_str = self.config.limiter_mode.clone();

        self.wav_export_state.start_export();

        std::thread::spawn(move || {
            let spec = hound::WavSpec {
                channels: settings.channel_mode.channels(),
                sample_rate,
                bits_per_sample: settings.bit_depth.bits(),
                sample_format: if settings.bit_depth.is_float() {
                    hound::SampleFormat::Float
                } else {
                    hound::SampleFormat::Int
                },
            };

            let result = std::fs::File::create(&file_path)
                .and_then(|file| {
                    let writer = hound::WavWriter::new(file, spec)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                    Ok(writer)
                });

            match result {
                Ok(mut writer) => {
                    let mut renderer = WavRenderer::new(module, sample_rate);
                    let interp = match interp_str.as_str() {
                        "Nearest" => crate::audio::commands::InterpolationType::Nearest,
                        "Cubic" => crate::audio::commands::InterpolationType::Cubic,
                        _ => crate::audio::commands::InterpolationType::Linear,
                    };
                    renderer.set_interpolation(interp);
                    let limiter = match limiter_str.as_str() {
                        "SoftKnee" => crate::audio::commands::LimiterMode::SoftKnee,
                        "SoftKneeSmooth" => crate::audio::commands::LimiterMode::SoftKneeSmooth,
                        _ => crate::audio::commands::LimiterMode::HardClip,
                    };
                    renderer.set_limiter_mode(limiter);
                    renderer.set_channels(settings.channel_mode == crate::ui::wav_export_window::ChannelMode::Stereo);

                    let render_result = renderer.render_with_settings(
                        &mut writer,
                        &settings,
                        |p| {
                            if cancel_arc.load(Ordering::SeqCst) {
                                return false;
                            }
                            let pct = (p * 100.0) as u32;
                            progress_arc.store(pct, Ordering::SeqCst);
                            state_arc.store(1, Ordering::SeqCst);
                            true
                        },
                    );

                    match render_result {
                        Ok(()) => {
                            if let Err(e) = writer.finalize() {
                                state_arc.store(3, Ordering::SeqCst);
                                eprintln!("Failed to finalize audio file: {}", e);
                            } else {
                                state_arc.store(2, Ordering::SeqCst);
                                println!("Exported to: {}", file_path.display());
                            }
                        }
                        Err(e) => {
                            state_arc.store(3, Ordering::SeqCst);
                            eprintln!("Failed to render audio: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    state_arc.store(3, Ordering::SeqCst);
                    eprintln!("Failed to create file: {}", e);
                }
            }
        });
    }

    fn update_wav_export_progress(&mut self) {
        if !self.wav_export_state.is_exporting {
            return;
        }

        let progress = self.wav_export_state.export_progress_atomic.load(Ordering::SeqCst);
        let state = self.wav_export_state.export_state_atomic.load(Ordering::SeqCst);

        self.wav_export_state.export_progress = (progress as f32) / 100.0;
        self.wav_export_state.export_status = if progress > 0 {
            format!("Rendering... {}%", progress)
        } else {
            "Preparing...".to_string()
        };

        if state == 2 {
            self.wav_export_state.is_exporting = false;
            self.wav_export_state.export_complete = true;
            self.wav_export_state.export_status = "Export complete!".to_string();
        } else if state == 3 {
            self.wav_export_state.is_exporting = false;
            self.wav_export_state.export_complete = true;
            self.wav_export_state.export_status = "Export failed (check console)".to_string();
        }
    }

    #[allow(dead_code)]
    fn export_wav_file(&mut self) {
    }

    fn save_config(&mut self) {
        self.config.last_dirs.clear();
        for (mode, path) in &self.file_browser.last_dirs {
            let key = match mode {
                BrowserMode::Modules => "modules",
                BrowserMode::Samples => "samples",
                BrowserMode::Instruments => "instruments",
                BrowserMode::Projects => "projects",
            };
            self.config.last_dirs.insert(key.to_string(), path.to_string_lossy().into_owned());
        }
        if let Some(ref path) = self.core.file_path {
            self.config.last_file_path = Some(path.clone());
        }
        self.config.favorites = self.file_browser.save_favorites();
        self.config.save();
    }

    fn check_auto_backup(&mut self) {
        let interval = self.config.auto_backup_interval_secs;
        if interval == 0 || !self.core.module_dirty || self.core.module.is_none() {
            return;
        }
        if self.core.last_backup_time.elapsed().as_secs() < interval {
            return;
        }

        let backup_dir = self.config.get_backup_dir();
        let _ = std::fs::create_dir_all(&backup_dir);

        let name = if self.core.loaded_module_name.is_empty() {
            "untitled".to_string()
        } else {
            self.core.loaded_module_name.trim_end_matches(".htk")
                .trim_end_matches(".it")
                .trim_end_matches(".xm")
                .trim_end_matches(".s3m")
                .trim_end_matches(".mod")
                .to_string()
        };
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let backup_path = backup_dir.join(format!("{}_backup_{}.htk", name, timestamp));

        if let Some(ref module) = self.core.module {
            let data = crate::formats::save_module(module);
            let _ = std::fs::write(&backup_path, &data);
        }

        self.core.module_dirty = false;
        self.core.last_backup_time = std::time::Instant::now();
    }
}

impl eframe::App for HtrkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_zoom_factor(self.config.zoom_factor);

        if self.stream.is_none() && !self.audio_init_failed {
            self.init_audio();
        }

        crate::actions::handle_keyboard_input(self, ctx);

        // Clamp cursor and scroll to active channel count
        let nch = self.num_channels();
        self.core.cursor.channel = self.core.cursor.channel.min(nch.saturating_sub(1));
        self.scroll_channel = self.scroll_channel.min(nch.saturating_sub(1));

        self.update_wav_export_progress();

        let (playback_row, playback_order, playback_pattern) = {
            let playing = self.core.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed);
            if playing {
                let order = self.core.playback_state.current_order.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let row = self.core.playback_state.current_row.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let pat = self.core.playback_state.current_pattern.load(std::sync::atomic::Ordering::Relaxed) as usize;
                (Some(row), Some(order), Some(pat))
            } else {
                (None, None, None)
            }
        };

        if let Some(order) = playback_order {
            if self.follow_playback && order != self.core.selected_order {
                if let Some(ref module) = self.core.module {
                    if order < module.order_list.len() {
                        self.core.selected_order = order;
                        self.core.cursor.row = 0;
                        self.ensure_cursor_visible();
                    }
                }
            }
        }

        if let Some(row) = playback_row {
            if self.follow_playback {
                if let (Some(active_pat), Some(ref module)) = (playback_pattern, self.core.module.as_ref()) {
                    if !module.order_list.is_empty() {
                        let order_idx = self.core.selected_order.min(module.order_list.len().saturating_sub(1));
                        let displayed_pat = module.order_list[order_idx] as usize;
                        if displayed_pat == active_pat {
                            if row < self.scroll_row {
                                self.scroll_row = row;
                            }
                            if row >= self.scroll_row + self.last_visible_rows {
                                self.scroll_row = row - self.last_visible_rows + 1;
                            }
                        }
                    }
                }
            }
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            let menu_resp = crate::ui::menu_bar::draw_menu_bar(
                ui,
                self.core.undo_manager.can_undo(),
                self.core.undo_manager.can_redo(),
                self.core.selection.is_some(),
                self.follow_playback,
                self.theme_preset,
                self.config.get_spacing_mode(),
                &self.theme,
                self.current_sample_rate,
                &self.current_sample_format,
                &mut self.col_vis,
            );
            if menu_resp.new_song {
                self.new_song();
            }
            if menu_resp.open_file {
                self.open_file_dialog();
            }
            if menu_resp.import_sample {
                self.file_browser.open(BrowserMode::Samples, &mut self.config);
            }
            if menu_resp.import_instrument {
                self.file_browser.open(BrowserMode::Instruments, &mut self.config);
            }
            if menu_resp.save_file {
                self.save_current_file();
            }
            if menu_resp.save_as {
                self.save_as_dialog();
            }
            if menu_resp.export_wav {
                self.open_wav_export_dialog();
            }
            if menu_resp.undo {
                self.ensure_module_ownership();
                if let Some(ref mut module) = self.core.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let _ = self.core.undo_manager.undo(arc_module);
                    }
                }
                self.sync_module_to_audio();
            }
            if menu_resp.redo {
                self.ensure_module_ownership();
                if let Some(ref mut module) = self.core.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let _ = self.core.undo_manager.redo(arc_module);
                    }
                }
                self.sync_module_to_audio();
            }
            if menu_resp.cut {
                self.copy_selection();
                self.delete_selection();
            }
            if menu_resp.copy {
                self.copy_selection();
            }
            if menu_resp.paste {
                self.paste_at_cursor();
            }
            if menu_resp.select_all {
                self.select_all();
            }
            if menu_resp.follow_playback {
                self.follow_playback = !self.follow_playback;
            }
            if let Some(preset) = menu_resp.theme_changed {
                self.theme_preset = preset;
                self.theme = TrackerTheme::from_preset(preset);
                self.config.theme_preset = preset.config_key().to_string();
                self.config.save();
            }
            if let Some(mode) = menu_resp.spacing_mode_changed {
                self.config.set_spacing_mode(mode);
                self.config.save();
            }
            if let Some(col_vis) = menu_resp.col_vis {
                self.col_vis = col_vis;
                self.config.set_col_vis(col_vis);
                self.config.save();
            }
            if menu_resp.show_shortcuts {
                self.show_shortcuts = true;
            }
            if menu_resp.show_about {
                self.show_about = true;
            }
            if menu_resp.show_settings {
                self.settings_state = crate::ui::settings_window::SettingsState::from_config(&self.config);
                self.settings_state.open = true;
            }
        });

        if let Some(device_name) = self.pending_device_switch.take() {
            self.switch_output_device(device_name);
        }
        if self.pending_reinit {
            self.pending_reinit = false;
            if self.stream.is_some() {
                self.stream = None;
                self.core.command_sender = None;
                self.init_audio();
            }
        }

        egui::TopBottomPanel::top("transport_bar").show(ctx, |ui| {
            let transport_resp = crate::ui::transport::draw_transport(
                ui,
                &self.core.playback_state,
                &mut self.core.command_sender,
                &self.theme,
            );
            if transport_resp.prev_pattern_clicked {
                self.skip_to_prev_pattern();
            }
            if transport_resp.next_pattern_clicked {
                self.skip_to_next_pattern();
            }
        });

        let num_ch = self.num_channels();
        let panel_w = ctx.available_rect().width() - 12.0;
        let scope_height = crate::ui::oscilloscope::compute_scope_height(panel_w, num_ch);
        egui::TopBottomPanel::top("oscilloscope")
            .exact_height(scope_height)
            .show(ctx, |ui| {
                crate::ui::oscilloscope::draw_oscilloscope(
                    ui,
                    &self.core.playback_state,
                    &self.theme,
                    num_ch,
                );
            });

        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(22.0)
            .show(ctx, |ui| {
                let cpu = self.core.playback_state.cpu_usage_pct.load(std::sync::atomic::Ordering::Relaxed);
                let total_rows = self.current_pattern().map_or(64, |p| p.num_rows);
                let hint = format!("Ins: {} | Smp: {}", self.core.selected_instrument, self.core.selected_sample);
                crate::ui::status_bar::draw_status_bar(
                    ui,
                    self.core.module.as_ref().map(|m| m.as_ref()),
                    self.core.selected_order,
                    self.core.cursor.row,
                    total_rows,
                    self.num_channels(),
                    cpu,
                    self.current_octave,
                    self.cursor_skip,
                    self.core.selected_instrument,
                    self.core.selected_sample,
                    self.edit_mode,
                    &hint,
                    &self.theme,
                );
            });

        egui::SidePanel::left("order_list")
            .min_width(120.0)
            .default_width(150.0)
            .show(ctx, |ui| {
                if let Some(ref module) = self.core.module {
                    let order_resp = crate::ui::order_list::draw_order_list(
                        ui,
                        module,
                        self.core.selected_order,
                        playback_order,
                        &self.theme,
                    );
                    let should_insert = order_resp.insert_clicked;
                    let should_delete = order_resp.delete_clicked;
                    let should_duplicate = order_resp.duplicate_clicked;
                    let pattern_changed = order_resp.pattern_changed;
                    let pattern_resized = order_resp.pattern_resized;
                    let order_reordered = order_resp.order_reordered;
                    if let Some(idx) = order_resp.selected_order {
                        self.core.selected_order = idx;
                        self.core.cursor.row = 0;
                        self.ensure_cursor_visible();
                    }
                    let mut changed = false;
                    self.ensure_module_ownership();
                    if let Some(ref mut m) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(m) {
                            if let Some((order_idx, new_pat)) = pattern_changed {
                                if order_idx < arc_module.order_list.len() {
                                    arc_module.order_list[order_idx] = new_pat;
                                    changed = true;
                                }
                            }
                            if should_insert || should_delete {
                                if should_insert {
                                    let new_pat = arc_module.patterns.len() as u8;
                                    arc_module.patterns.push(crate::sequencer::Pattern::new(64));
                                    arc_module.order_list.insert(self.core.selected_order + 1, new_pat);
                                    changed = true;
                                }
                                if should_delete && arc_module.order_list.len() > 1 {
                                    if self.core.selected_order < arc_module.order_list.len() {
                                        arc_module.order_list.remove(self.core.selected_order);
                                        if self.core.selected_order >= arc_module.order_list.len() {
                                            self.core.selected_order = arc_module.order_list.len().saturating_sub(1);
                                        }
                                        changed = true;
                                    }
                                }
                            }
                            if let Some((from, to)) = order_reordered {
                                if from < arc_module.order_list.len() {
                                    let item = arc_module.order_list.remove(from);
                                    let insert_at = if to > from { to - 1 } else { to };
                                    let insert_at = insert_at.min(arc_module.order_list.len());
                                    arc_module.order_list.insert(insert_at, item);
                                    self.core.selected_order = insert_at;
                                    changed = true;
                                }
                            }
                            if should_duplicate {
                                let cur_pat_idx = *arc_module.order_list.get(self.core.selected_order).unwrap_or(&0) as usize;
                                if cur_pat_idx < arc_module.patterns.len() {
                                    let cloned = arc_module.patterns[cur_pat_idx].clone();
                                    let new_idx = arc_module.patterns.len() as u8;
                                    arc_module.patterns.push(cloned);
                                    let insert_at = (self.core.selected_order + 1).min(arc_module.order_list.len());
                                    arc_module.order_list.insert(insert_at, new_idx);
                                    self.core.selected_order = insert_at;
                                    changed = true;
                                }
                            }
                            if let Some((order_idx, new_rows)) = pattern_resized {
                                let pat_idx = *arc_module.order_list.get(order_idx).unwrap_or(&0) as usize;
                                if pat_idx < arc_module.patterns.len() {
                                    arc_module.patterns[pat_idx].resize_rows(new_rows);
                                    changed = true;
                                }
                            }
                        }
                    }
                    if changed {
                        self.sync_module_to_audio();
                    }
                } else {
                    ui.label("No module loaded");
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.core.module.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("htrk - tracker");
                    ui.add_space(20.0);
                    ui.label("No module loaded. Press Ctrl+O to open a file, or Ctrl+N for a new song.");
                });
                return;
            }

            ui.horizontal(|ui| {
            ui.selectable_value(&mut self.current_view, AppView::Pattern, "Pattern");
            ui.selectable_value(&mut self.current_view, AppView::Sample, "Sample");
            ui.selectable_value(&mut self.current_view, AppView::Instrument, "Instrument");
            ui.selectable_value(&mut self.current_view, AppView::SendFx, "Send FX");
            ui.selectable_value(&mut self.current_view, AppView::Playback, "Playback");
            ui.selectable_value(&mut self.current_view, AppView::Automation, "Automation");
            });
            ui.separator();

            match self.current_view {
                AppView::Pattern => {
                    let num_channels = self.num_channels();

                    let metrics = crate::ui::pattern_grid::GridMetrics::new(self.config.editor_font_size as f32, self.config.get_spacing_mode(), self.config.get_col_vis());
                    let visible_channels = crate::ui::pattern_grid::GridMetrics::calculate_visible_channels(ui, metrics);
                    let visible_channels = visible_channels.min(num_channels - self.scroll_channel).max(1);

                    // Channel add/remove buttons
                    ui.horizontal(|ui| {
                        ui.set_min_height(0.0);
                        if ui.button("+").clicked() {
                            self.ensure_module_ownership();
                            if let Some(ref mut module) = self.core.module {
                                if let Some(arc_module) = Arc::get_mut(module) {
                                    if arc_module.channel_panning.len() < MAX_CHANNELS {
                                        arc_module.channel_panning.push(crate::sequencer::module::PANNING_CENTER);
                                        arc_module.channel_volume.push(crate::sequencer::module::VOLUME_MAX);
                                        self.sync_module_to_audio();
                                        self.sync_channel_fields();
                                    }
                                }
                            }
                        }
                        let can_remove = self.core.module.as_ref()
                            .map(|m| m.channel_panning.len() > 1).unwrap_or(false);
                        if ui.button("−").clicked() && can_remove {
                            self.ensure_module_ownership();
                            if let Some(ref mut module) = self.core.module {
                                if let Some(arc_module) = Arc::get_mut(module) {
                                    arc_module.channel_panning.pop();
                                    arc_module.channel_volume.pop();
                                    self.sync_module_to_audio();
                                    self.sync_channel_fields();
                                    if self.core.cursor.channel >= self.num_channels() {
                                        self.core.cursor.channel = self.num_channels().saturating_sub(1);
                                    }
                                    if self.scroll_channel >= self.num_channels() {
                                        self.scroll_channel = self.num_channels().saturating_sub(1);
                                    }
                                }
                            }
                        }
                    });

                    let ch_resp = crate::ui::channel_headers::draw_channel_headers(
                        ui,
                        num_channels,
                        self.scroll_channel,
                        visible_channels,
                        &self.core.muted_channels,
                        &self.core.solo_channels,
                        &self.channel_names,
                        &self.core.module.as_ref().map(|m| m.channel_panning.clone()).unwrap_or_default(),
                        &self.core.send_levels,
                        &mut self.channel_rename_state,
                        &self.theme,
                        &self.core.playback_state,
                        metrics,
                        &self.core.automation_targets,
                    );

                    if let Some(ch) = ch_resp.toggle_mute {
                        self.core.toggle_mute(ch);
                    }
                    if let Some(ch) = ch_resp.toggle_solo {
                        self.core.toggle_solo(ch);
                    }
                    if let Some((ch, si, level)) = ch_resp.send_changed {
                        self.core.set_send_level(ch, si, level);
                    }
                    if let Some((ch, name)) = ch_resp.rename_channel {
                        if ch < self.channel_names.len() {
                            self.channel_names[ch] = name;
                        }
                    }
                    if let Some((ch, target)) = ch_resp.automation_target_changed {
                        if ch < self.core.automation_targets.len() {
                            self.core.automation_targets[ch] = target;
                            if let Some(ref t) = target {
                                self.ensure_module_ownership();
                                if let Some(ref mut module) = self.core.module {
                                    if let Some(arc_module) = Arc::get_mut(module) {
                                        let exists = arc_module.automation_tracks.iter().any(
                                            |tr| tr.channel == Some(ch) && tr.target == *t
                                        );
                                        if !exists {
                                            let id = arc_module.next_automation_id;
                                            arc_module.next_automation_id += 1;
                                            arc_module.automation_tracks.push(
                                                crate::sequencer::AutomationTrack::new(id, *t, Some(ch))
                                            );
                                        }
                                    }
                                }
                                self.sync_module_to_audio();
                            }
                        }
                    }

                    if let Some(module) = &self.core.module {
                        if !module.order_list.is_empty() {
                            let order_idx = self.core.selected_order.min(module.order_list.len().saturating_sub(1));
                            let pat_idx = module.order_list[order_idx] as usize;
                            let grid_playback_row = if playback_pattern == Some(pat_idx) { playback_row } else { None };
                            if let Some(pattern) = module.patterns.get(pat_idx) {
                                let auto_overlays: Vec<Option<crate::ui::pattern_grid::AutomationOverlayInfo>> = (0..num_channels).map(|ch| {
                                    self.core.automation_targets.get(ch).and_then(|t| t.as_ref()).map(|target| {
                                        let track = module.automation_tracks.iter()
                                            .find(|tr| tr.channel == Some(ch) && tr.target == *target)
                                            .map(|tr| std::sync::Arc::new(tr.clone()));
                                        crate::ui::pattern_grid::AutomationOverlayInfo {
                                            target: *target,
                                            track,
                                            current_order: self.core.selected_order as u16,
                                            speed: module.initial_speed,
                                        }
                                    })
                                }).collect();

                                let grid_resp = crate::ui::pattern_grid::draw_pattern_grid(
                                    ui,
                                    pattern,
                                    &self.core.cursor,
                                    self.core.selection.as_ref(),
                                    grid_playback_row,
                                    self.scroll_row,
                                    self.scroll_channel,
                                    num_channels,
                                    metrics,
                                    &self.theme,
                                    self.config.row_highlight_minor,
                                    self.config.row_highlight_major,
                                    self.config.get_sample_length_bg(),
                                    self.config.get_col_vis(),
                                    self.core.module.as_ref().map(|v| &**v),
                                    &auto_overlays,
                                );

                                self.last_visible_rows = grid_resp.visible_rows;
                                self.last_visible_channels = grid_resp.visible_channels;

                                if let Some(pos) = grid_resp.clicked_position {
                                    self.core.cursor = pos;
                                    self.core.selection = None;
                                    self.core.selection_anchor = None;
                                    self.ensure_cursor_visible();
                                }
                                if let Some(pos) = grid_resp.drag_position {
                                    if self.core.selection_anchor.is_none() {
                                        self.core.selection_anchor = Some(self.core.cursor);
                                    }
                                    self.core.cursor = pos;
                                    if let Some(anchor) = self.core.selection_anchor {
                                        self.core.selection = Some(Selection {
                                            start: anchor,
                                            end: self.core.cursor,
                                        });
                                    }
                                    self.ensure_cursor_visible();
                                }
                                if let Some(action) = grid_resp.context_menu_action {
                                    self.handle_context_menu_action(action);
                                }
                                if let Some(interaction) = grid_resp.automation_interaction {
                                    self.handle_automation_interaction(interaction);
                                }
                                if grid_resp.toggle_sample_length_bg {
                                    self.config.toggle_sample_length_bg();
                                }
                                if let Some(tooltip) = grid_resp.effect_tooltip {
                                    ui.label(egui::RichText::new(&tooltip).size(10.0).color(egui::Color32::GRAY));
                                }
                            }
                        }
                    }
                }
                AppView::Sample => {
                    if let Some(module) = &self.core.module {
                        if let Some(event) = crate::ui::sample_editor::draw_sample_editor(
                            ui,
                            module,
                            &mut self.core.selected_sample,
                            &self.theme,
                            &mut self.sample_selection,
                            &mut self.sample_clipboard,
                            &mut self.amplify_factor,
                        ) {
                            crate::actions::handle_sample_edit(self, event);
                        }
                    }
                }
                AppView::Instrument => {
                    if let Some(module) = &self.core.module {
                        if let Some(event) = crate::ui::instrument_editor::draw_instrument_editor(
                            ui,
                            module,
                            &mut self.core.selected_instrument,
                            &self.theme,
                        ) {
                            match event {
                                crate::ui::instrument_editor::InstrumentEditEvent::SaveInstrument => {
                                    crate::actions::save_instrument_dialog(self);
                                }
                                crate::ui::instrument_editor::InstrumentEditEvent::LoadInstrument => {
                                    crate::actions::load_instrument_dialog(self);
                                }
                                crate::ui::instrument_editor::InstrumentEditEvent::ExportInstrument(idx) => {
                                    crate::actions::export_instrument_dialog(self, idx);
                                }
                                crate::ui::instrument_editor::InstrumentEditEvent::ImportInstrument => {
                                    crate::actions::load_instrument_dialog(self);
                                }
                                other => crate::actions::handle_instrument_edit(self, other),
                            }
                        }
                    }
                }
                AppView::SendFx => {
                    crate::ui::sendfx_editor::draw_sendfx_view(
                        ui,
                        &mut self.core.command_sender,
                        &mut self.send_bus_effect_types,
                        &mut self.send_bus_params,
                    );
                }
                AppView::Playback => {
                    let num_channels = self.num_channels();
                    crate::ui::playback_view::draw_playback_view(
                        ui,
                        &self.core.playback_state,
                        &mut self.core.command_sender,
                        &self.theme,
                        num_channels,
                    );
                }
                AppView::Automation => {
                    self.automation_editor_state.selected_order = self.core.selected_order as u16;
                    self.ensure_module_ownership();
                    if let Some(ref mut module) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(module) {
                            let auto_resp = crate::ui::automation_editor::draw_automation_editor(
                                ui,
                                arc_module,
                                &mut self.automation_editor_state,
                                &self.theme,
                            );
                            if let Some((target, channel)) = auto_resp.track_added {
                                let id = arc_module.next_automation_id;
                                arc_module.next_automation_id += 1;
                                arc_module.automation_tracks.push(
                                    crate::sequencer::AutomationTrack::new(id, target, channel)
                                );
                                self.automation_editor_state.selected_track_id = Some(id);
                            }
                            if let Some(tid) = auto_resp.track_removed {
                                arc_module.automation_tracks.retain(|t| t.id != tid);
                                if self.automation_editor_state.selected_track_id == Some(tid) {
                                    self.automation_editor_state.selected_track_id = None;
                                }
                            }
                            if let Some(tid) = auto_resp.track_toggled {
                                if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == tid) {
                                    t.enabled = !t.enabled;
                                }
                            }
                            if let Some((track_id, point)) = auto_resp.point_changed {
                                if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                                    t.insert_point(point);
                                }
                            }
                            if let Some((track_id, order, row)) = auto_resp.point_removed {
                                if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                                    t.remove_point_at(order, row);
                                }
                            }
                            if let Some((track_id, mode)) = auto_resp.interp_changed {
                                if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                                    t.default_interp = mode;
                                }
                            }
                            self.sync_module_to_audio();
                        }
                    }
                }
            }
        });

        if self.show_shortcuts {
            crate::ui::help_screen::draw_shortcuts_window(ctx, &mut self.show_shortcuts);
        }

        if self.settings_state.open {
            let action = crate::ui::settings_window::draw_settings_window(
                ctx,
                &mut self.settings_state,
                &self.output_device_names,
                self.selected_device_name.as_deref(),
            );
            match action {
                crate::ui::settings_window::SettingsAction::Save | crate::ui::settings_window::SettingsAction::Apply => {
                    self.settings_state.apply_to_config(&mut self.config);
                    self.config.save();
                    self.apply_config_to_live_state();
                    self.pending_reinit = true;
                }
                crate::ui::settings_window::SettingsAction::Cancel => {
                    self.settings_state = crate::ui::settings_window::SettingsState::from_config(&self.config);
                }
                crate::ui::settings_window::SettingsAction::RefreshDevices => {
                    self.refresh_output_devices();
                }
                crate::ui::settings_window::SettingsAction::SelectDevice(name) => {
                    self.pending_device_switch = Some(name);
                }
                crate::ui::settings_window::SettingsAction::None => {}
            }
        }

        if crate::ui::wav_export_window::draw_wav_export(ctx, &mut self.wav_export_state) {
            self.export_wav_with_settings();
        }

        if self.show_about {
            egui::Window::new("About htrk")
                .open(&mut self.show_about)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(false)
                .default_width(350.0)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("htrk v0.6.0");
                        ui.add_space(8.0);
                        ui.label("A modern tracker / music sequencer");
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Built with Rust + egui + cpal")
                                .size(11.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(12.0);
                        ui.label("Supports .htk, .it, .xm, .s3m, .mod formats");
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("Development guided by Clint Anderson")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(180, 180, 200)),
                        );
                        ui.label(
                            egui::RichText::new("clinta@gmail.com")
                                .size(10.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("AI-assisted code from GLM, DeepSeek, and MiniMax")
                                .size(10.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(8.0);
                    });
                });
        }

        if let Some(ref mut dialog) = self.sample_export_dialog {
            if let Some((path, bit_depth)) = dialog.show(ctx) {
                let sample_idx = dialog.sample_index;
                if let Some(ref module) = self.core.module {
                    if let Some(sample) = module.samples.get(sample_idx) {
                        let wav_data = crate::formats::wav::export_wav(sample, bit_depth);
                        if let Err(e) = std::fs::write(&path, wav_data) {
                            eprintln!("Failed to write sample: {}", e);
                        } else {
                            if let Some(parent) = path.parent() {
                                self.config.default_wav_path = Some(parent.to_string_lossy().into_owned());
                            }
                            self.config.set_sample_export_bit_depth(bit_depth);
                        }
                    }
                }
                self.sample_export_dialog = None;
            }
        }

        if self.file_browser.show {
            let mut file_browser_open = true;
            egui::Window::new("File Browser")
                .open(&mut file_browser_open)
                .default_size([600.0, 400.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(true)
                .show(ctx, |ui| {
                    if let Some(path) = self.file_browser.render(ui, Some(&mut self.config)) {
                        let path_str = path.to_string_lossy().to_string();
                        let ext = path.extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        if ext == "wav" {
                            self.import_wav(&path_str);
                        } else {
                            self.load_file(&path_str);
                        }
                    }
                });
            if !file_browser_open {
                self.file_browser.close();
            }
        }

        self.check_auto_backup();

        ctx.request_repaint();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config();
    }
}
