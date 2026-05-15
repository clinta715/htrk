use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;

use crate::audio::commands::AudioCommand;
use crate::audio::engine::{CommandSender, create_engine_and_sender};
use crate::audio::renderer::WavRenderer;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::edit::{
    SetCellCommand, InsertRowCommand, UndoManager, SampleProperty, SetSamplePropertyCommand,
    InstrumentProperty, SetInstrumentPropertyCommand, AddEnvelopePointCommand,
    RemoveEnvelopePointCommand, SetEnvelopePointCommand, EnvelopeType, SetSampleDataCommand,
    MapNoteToSampleCommand, SetEnvelopeSustainCommand, SetEnvelopeLoopCommand,
    SetEnvelopeFlagsCommand, MapNoteToNoteCommand,
};
use crate::app_config::AppConfig;
use crate::sequencer::instrument::EnvelopePoint;
use crate::ui::sample_editor::SampleEditEvent;
use crate::ui::instrument_editor::InstrumentEditEvent;
use crate::ui::file_browser::{FileBrowser, BrowserMode};
use crate::formats;
use crate::sequencer::pattern::Cell;
use crate::sequencer::{Effect, Module, Note, MAX_CHANNELS};
use crate::ui::pattern_grid::{CursorPosition, Selection, SubColumn, VISIBLE_ROWS};
use crate::ui::TrackerTheme;
use crate::ui::theme::ThemePreset;

const NOTE_KEYS_LOWER: [(egui::Key, u8); 12] = [
    (egui::Key::Z, 0),
    (egui::Key::S, 1),
    (egui::Key::X, 2),
    (egui::Key::D, 3),
    (egui::Key::C, 4),
    (egui::Key::V, 5),
    (egui::Key::G, 6),
    (egui::Key::B, 7),
    (egui::Key::H, 8),
    (egui::Key::N, 9),
    (egui::Key::J, 10),
    (egui::Key::M, 11),
];

const NOTE_KEYS_UPPER: [(egui::Key, u8); 12] = [
    (egui::Key::Q, 0),
    (egui::Key::Num2, 1),
    (egui::Key::W, 2),
    (egui::Key::Num3, 3),
    (egui::Key::E, 4),
    (egui::Key::R, 5),
    (egui::Key::Num5, 6),
    (egui::Key::T, 7),
    (egui::Key::Num6, 8),
    (egui::Key::Y, 9),
    (egui::Key::Num7, 10),
    (egui::Key::U, 11),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppView {
    Pattern,
    Sample,
    Instrument,
    SendFx,
}

pub struct HtrkApp {
    command_sender: Option<CommandSender>,
    playback_state: Arc<AtomicPlaybackState>,
    stream: Option<cpal::Stream>,

    output_device_names: Vec<String>,
    selected_device_name: Option<String>,
    current_sample_rate: u32,
    current_sample_format: String,
    pending_device_switch: Option<String>,
    pending_reinit: bool,

    module: Option<Arc<Module>>,
    loaded_module_name: String,
    file_path: Option<String>,

    file_browser: FileBrowser,

    current_view: AppView,

    cursor: CursorPosition,
    selection: Option<Selection>,
    selection_anchor: Option<CursorPosition>,
    scroll_row: usize,
    scroll_channel: usize,

    current_octave: u8,
    edit_mode: bool,
    follow_playback: bool,
    cursor_skip: u8,
    last_entered_cell: Option<Cell>,
    edit_mask_instrument: bool,
    edit_mask_volume: bool,
    multichannel_enabled: bool,
    multichannel_channels: Vec<bool>,
    selected_order: usize,
    selected_sample: usize,
    selected_instrument: usize,

    muted_channels: Vec<bool>,
    solo_channels: Vec<bool>,
    channel_names: Vec<String>,
    channel_rename_state: crate::ui::channel_headers::ChannelRenameState,

    undo_manager: UndoManager,

    clipboard: Option<Vec<Vec<Cell>>>,
    clipboard_width: usize,

    theme: TrackerTheme,
    theme_preset: crate::ui::theme::ThemePreset,
    show_shortcuts: bool,
    show_about: bool,
    settings_state: crate::ui::settings_window::SettingsState,
    wav_export_state: crate::ui::wav_export_window::WavExportState,
    last_backup_time: std::time::Instant,
    module_dirty: bool,
    audio_init_failed: bool,
    sample_selection: Option<(usize, usize)>,
    sample_clipboard: Option<Arc<Vec<f32>>>,
    amplify_factor: f32,
    config: AppConfig,
    last_visible_rows: usize,
    last_visible_channels: usize,
    send_levels: [[f32; 2]; 64],
    send_bus_params: [[f32; 5]; 2],
}

impl Default for HtrkApp {
    fn default() -> Self {
        let config = AppConfig::load();
        let mut file_browser = FileBrowser::default();
        file_browser.restore_last_dirs(&config);
        file_browser.restore_favorites(&config.favorites);
        HtrkApp {
            command_sender: None,
            playback_state: Arc::new(AtomicPlaybackState::default()),
            stream: None,
            output_device_names: Vec::new(),
            selected_device_name: None,
            current_sample_rate: 0,
            current_sample_format: String::new(),
            pending_device_switch: None,
            pending_reinit: false,
            module: None,
            loaded_module_name: String::new(),
            file_path: None,
            file_browser,
            current_view: AppView::Pattern,
            cursor: CursorPosition {
                row: 0,
                channel: 0,
                sub_column: SubColumn::Note,
            },
            selection: None,
            selection_anchor: None,
            scroll_row: 0,
            scroll_channel: 0,
            current_octave: 4,
            edit_mode: true,
            follow_playback: config.follow_playback_default,
            cursor_skip: 1,
            last_entered_cell: None,
            edit_mask_instrument: true,
            edit_mask_volume: true,
            multichannel_enabled: false,
            multichannel_channels: vec![false; MAX_CHANNELS],
            selected_order: 0,
            selected_sample: 1,
            selected_instrument: 1,
            muted_channels: vec![false; MAX_CHANNELS],
            solo_channels: vec![false; MAX_CHANNELS],
            channel_names: (0..MAX_CHANNELS).map(|i| format!("Ch{}", i + 1)).collect(),
            channel_rename_state: crate::ui::channel_headers::ChannelRenameState::default(),
            undo_manager: UndoManager::default(),
            clipboard: None,
            clipboard_width: 0,
            theme: TrackerTheme::from_preset(
                ThemePreset::from_name(&config.theme_preset).unwrap_or(ThemePreset::DarkModern)
            ),
            theme_preset: ThemePreset::from_name(&config.theme_preset).unwrap_or(ThemePreset::DarkModern),
            show_shortcuts: false,
            show_about: false,
            settings_state: crate::ui::settings_window::SettingsState::from_config(&config),
            wav_export_state: crate::ui::wav_export_window::WavExportState::new(44100),
            last_backup_time: std::time::Instant::now(),
            module_dirty: false,
            audio_init_failed: false,
            sample_selection: None,
            sample_clipboard: None,
            amplify_factor: config.default_amplify_factor,
            config,
            last_visible_rows: VISIBLE_ROWS,
            last_visible_channels: 16,
            send_levels: [[0.0f32; 2]; 64],
            send_bus_params: [[0.5, 1.0, 1.0, 0.4, 0.3], [0.0, 0.7, 0.5, 0.6, 0.5]],
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
            let state = self.playback_state.clone();
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
                self.command_sender = sender;
                if let Some(ref module) = self.module {
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
        self.command_sender = None;
        self.selected_device_name = Some(device_name);
        self.init_audio();
    }

    fn send_command(&mut self, cmd: AudioCommand) {
        #[cfg(feature = "audio_debug")]
        debug_log!("[CMD] {:?}", cmd);
        if let Some(ref mut sender) = self.command_sender {
            sender.send(cmd);
        } else {
            #[cfg(feature = "audio_debug")]
            debug_log!("[CMD] Error: command_sender is None!");
        }
    }

    fn ensure_module_ownership(&mut self) {
        let new_module = match &self.module {
            Some(arc) if Arc::strong_count(arc) > 1 => {
                Some(Arc::new((**arc).clone()))
            }
            _ => None,
        };
        if let Some(new_arc) = new_module {
            self.module = Some(new_arc);
        }
    }

    fn sync_module_to_audio(&mut self) {
        if let Some(ref module) = self.module {
            self.send_command(AudioCommand::LoadModule(module.clone()));
            self.module_dirty = true;
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
                // Ensure minimum slots
                if module.samples.len() < 65 {
                    module.samples.resize(65, crate::sequencer::Sample::default());
                }
                if module.instruments.len() < 17 {
                    module.instruments.resize(17, crate::sequencer::Instrument::default());
                }

                self.loaded_module_name = module.name.clone();
                self.file_path = Some(path.to_string());
                let module = Arc::new(module);
                self.module = Some(module.clone());
                self.send_command(AudioCommand::Stop);
                self.send_command(AudioCommand::LoadModule(module));
                self.cursor = CursorPosition::default();
                self.selection = None;
                self.scroll_row = 0;
                self.scroll_channel = 0;
                self.selected_order = 0;
                self.selected_sample = 1;
                self.selected_instrument = 1;
                self.undo_manager.clear();
            }
            Err(e) => {
                eprintln!("Failed to load module: {}", e);
            }
        }
    }

    fn new_song(&mut self) {
        let mut module = Module::default();
        module.name = "Untitled".to_string();
        module.order_list = vec![0];
        module.patterns.push(crate::sequencer::Pattern::new(64));

        self.loaded_module_name = module.name.clone();
        self.file_path = None;
        let module = Arc::new(module);
        self.module = Some(module.clone());
        self.send_command(AudioCommand::Stop);
        self.send_command(AudioCommand::LoadModule(module));
        self.cursor = CursorPosition::default();
        self.selection = None;
        self.scroll_row = 0;
        self.scroll_channel = 0;
        self.selected_order = 0;
        self.selected_sample = 1;
        self.selected_instrument = 1;
        self.undo_manager.clear();
    }

    fn current_pattern(&self) -> Option<&crate::sequencer::Pattern> {
        let module = self.module.as_ref()?;
        let order = *module.order_list.get(self.selected_order)?;
        module.patterns.get(order as usize)
    }

    #[allow(dead_code)]
    fn current_pattern_mut(&mut self) -> Option<&mut crate::sequencer::Pattern> {
        self.ensure_module_ownership();
        let module = Arc::get_mut(self.module.as_mut()?)?;
        let order = *module.order_list.get(self.selected_order)?;
        module.patterns.get_mut(order as usize)
    }

    fn num_channels(&self) -> usize {
        self.module.as_ref().map_or(8, |m| {
            let mut max_ch = 0;
            for &pat_idx in &m.order_list {
                if let Some(pat) = m.patterns.get(pat_idx as usize) {
                    for row in &pat.data {
                        for (ch, cell) in row.iter().enumerate() {
                            if !cell.is_empty() && ch + 1 > max_ch {
                                max_ch = ch + 1;
                            }
                        }
                    }
                }
            }
            // Allow expansion up to what's visible, but at least 16 or current used.
            max_ch.max(self.last_visible_channels + self.scroll_channel).max(16).min(MAX_CHANNELS)
        })
    }

    fn num_channels_checked(&self) -> usize {
        let n = self.num_channels();
        if n == 0 { 1 } else { n }
    }

    fn get_cell_at_cursor(&self) -> Cell {
        if let Some(pattern) = self.current_pattern() {
            if self.cursor.row < pattern.num_rows && self.cursor.channel < MAX_CHANNELS {
                *pattern.cell(self.cursor.row, self.cursor.channel)
            } else {
                Cell::default()
            }
        } else {
            Cell::default()
        }
    }

    fn set_cell_at_cursor(&mut self, new_cell: Cell) {
        let cursor = self.cursor;
        let channels: Vec<usize> = if self.multichannel_enabled {
            self.multichannel_channels.iter().enumerate()
                .filter(|(_, &active)| active)
                .map(|(ch, _)| ch)
                .collect()
        } else {
            vec![cursor.channel]
        };

        let old_cells: Vec<Cell> = channels.iter().map(|&ch| {
            let saved = self.cursor;
            self.cursor.channel = ch;
            let cell = self.get_cell_at_cursor();
            self.cursor = saved;
            cell
        }).collect();

        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                for (idx, &ch) in channels.iter().enumerate() {
                    let old_cell = old_cells[idx];
                    if old_cell == new_cell {
                        continue;
                    }
                    let cmd = Box::new(SetCellCommand {
                        order: self.selected_order,
                        row: cursor.row,
                        channel: ch,
                        old_cell,
                        new_cell: new_cell.clone(),
                    });
                    let _ = self.undo_manager.execute(cmd, arc_module);
                }
            }
        }
        self.sync_module_to_audio();
    }

    fn advance_cursor_down(&mut self, step: usize) {
        if let Some(pattern) = self.current_pattern() {
            let max_row = pattern.num_rows.max(1);
            self.cursor.row = (self.cursor.row + step).min(max_row - 1);
        }
        self.ensure_cursor_visible();
    }

    fn advance_cursor_up(&mut self, step: usize) {
        self.cursor.row = self.cursor.row.saturating_sub(step);
        self.ensure_cursor_visible();
    }

    fn ensure_cursor_visible(&mut self) {
        if self.cursor.row < self.scroll_row {
            self.scroll_row = self.cursor.row;
        }
        if self.cursor.row >= self.scroll_row + self.last_visible_rows {
            self.scroll_row = self.cursor.row - self.last_visible_rows + 1;
        }

        if self.cursor.channel < self.scroll_channel {
            self.scroll_channel = self.cursor.channel;
        }
        if self.cursor.channel >= self.scroll_channel + self.last_visible_channels {
            self.scroll_channel = self.cursor.channel - self.last_visible_channels + 1;
        }
    }

    fn clear_cell_at_cursor(&mut self) {
        self.set_cell_at_cursor(Cell::default());
    }

    fn handle_keyboard_input(&mut self, ctx: &egui::Context) {
        let modifiers = ctx.input(|i| i.modifiers);

        if modifiers.ctrl && !modifiers.shift {
            let mut handled = false;
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Key { key, pressed: true, .. } = event {
                        match key {
                            egui::Key::Z if self.edit_mode => {
                                self.ensure_module_ownership();
                                if let Some(ref mut module) = self.module {
                                    if let Some(arc_module) = Arc::get_mut(module) {
                                        let _ = self.undo_manager.undo(arc_module);
                                    }
                                }
                                self.sync_module_to_audio();
                                handled = true;
                            }
                            egui::Key::Y if self.edit_mode => {
                                self.ensure_module_ownership();
                                if let Some(ref mut module) = self.module {
                                    if let Some(arc_module) = Arc::get_mut(module) {
                                        let _ = self.undo_manager.redo(arc_module);
                                    }
                                }
                                self.sync_module_to_audio();
                                handled = true;
                            }
                            egui::Key::C => {
                                self.copy_selection();
                                handled = true;
                            }
                            egui::Key::X if self.edit_mode => {
                                self.copy_selection();
                                self.delete_selection();
                                handled = true;
                            }
                            egui::Key::V if self.edit_mode => {
                                self.paste_at_cursor();
                                handled = true;
                            }
                            egui::Key::A => {
                                self.select_all();
                                handled = true;
                            }
                            egui::Key::N => {
                                self.new_song();
                                handled = true;
                            }
                            egui::Key::O => {
                                match self.current_view {
                                    AppView::Sample => self.file_browser.open(BrowserMode::Samples),
                                    AppView::Instrument => self.file_browser.open(BrowserMode::Instruments),
                                    _ => self.open_file_dialog(),
                                }
                                handled = true;
                            }
                            egui::Key::I => {
                                self.file_browser.open(BrowserMode::Samples);
                                handled = true;
                            }
                            egui::Key::ArrowRight => {
                                self.step_sub_column_forward();
                                handled = true;
                            }
                            egui::Key::ArrowLeft => {
                                self.step_sub_column_backward();
                                handled = true;
                            }
                            _ => {}
                        }
                    }
                }
            });
            if handled {
                return;
            }
        }

        if modifiers.ctrl && modifiers.shift {
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Key { key, pressed: true, .. } = event {
                        match key {
                            egui::Key::S => self.save_as_dialog(),
                            egui::Key::I => self.file_browser.open(BrowserMode::Instruments),
                            egui::Key::ArrowUp => {
                                if self.current_octave < 9 { self.current_octave += 1; }
                            }
                            egui::Key::ArrowDown => {
                                if self.current_octave > 0 { self.current_octave -= 1; }
                            }
                            _ => {}
                        }
                    }
                }
            });
            return;
        }

        if modifiers.ctrl {
            return;
        }

        ctx.input(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Key { key, pressed: true, .. } => match key {
                        egui::Key::ArrowDown => {
                            if modifiers.ctrl {
                                if self.current_octave > 0 {
                                    self.current_octave -= 1;
                                }
                            } else if modifiers.shift {
                                self.extend_selection_down();
                            } else if modifiers.alt && self.edit_mode {
                                self.transpose_selection(-1);
                            } else {
                                self.selection = None;
                                self.advance_cursor_down(1);
                            }
                        }
                        egui::Key::ArrowUp => {
                            if modifiers.ctrl {
                                if self.current_octave < 9 {
                                    self.current_octave += 1;
                                }
                            } else if modifiers.shift {
                                self.extend_selection_up();
                            } else if modifiers.alt && self.edit_mode {
                                self.transpose_selection(1);
                            } else {
                                self.selection = None;
                                self.advance_cursor_up(1);
                            }
                        }
                        egui::Key::ArrowRight => {
                            if modifiers.alt {
                                self.selection = None;
                                let num_ch = self.num_channels();
                                if self.cursor.channel < num_ch - 1 {
                                    self.cursor.channel += 1;
                                    self.cursor.sub_column = SubColumn::Note;
                                    self.ensure_cursor_visible();
                                }
                            } else if modifiers.shift {
                                self.extend_selection_right();
                            } else {
                                self.selection = None;
                                self.move_cursor_right();
                            }
                        }
                        egui::Key::ArrowLeft => {
                            if modifiers.alt {
                                self.selection = None;
                                if self.cursor.channel > 0 {
                                    self.cursor.channel -= 1;
                                    self.cursor.sub_column = SubColumn::Note;
                                    self.ensure_cursor_visible();
                                }
                            } else if modifiers.shift {
                                self.extend_selection_left();
                            } else {
                                self.selection = None;
                                self.move_cursor_left();
                            }
                        }
                        egui::Key::Tab => {
                            self.selection = None;
                            if modifiers.shift {
                                self.cursor.channel = self.cursor.channel.saturating_sub(1);
                            } else {
                                self.cursor.channel += 1;
                                self.cursor.channel = self.cursor.channel.min(self.num_channels_checked() - 1);
                            }
                            self.ensure_cursor_visible();
                        }
                        egui::Key::M if modifiers.alt => {
                            let ch = self.cursor.channel;
                            if ch < self.muted_channels.len() {
                                self.muted_channels[ch] = !self.muted_channels[ch];
                                self.send_command(AudioCommand::SetChannelMuted {
                                    channel: ch,
                                    muted: self.muted_channels[ch],
                                });
                            }
                        }
                        egui::Key::S if modifiers.alt => {
                            let ch = self.cursor.channel;
                            if ch < self.solo_channels.len() {
                                self.solo_channels[ch] = !self.solo_channels[ch];
                                self.send_command(AudioCommand::SetChannelSolo {
                                    channel: ch,
                                    solo: self.solo_channels[ch],
                                });
                            }
                        }
                        egui::Key::N if modifiers.alt => {
                            let ch = self.cursor.channel;
                            if ch < self.multichannel_channels.len() {
                                self.multichannel_channels[ch] = !self.multichannel_channels[ch];
                                self.multichannel_enabled = self.multichannel_channels.iter().any(|&v| v);
                            }
                        }
                        egui::Key::PageUp => {
                            self.selection = None;
                            self.advance_cursor_up(16);
                        }
                        egui::Key::PageDown => {
                            self.selection = None;
                            self.advance_cursor_down(16);
                        }
                        egui::Key::Home => {
                            self.selection = None;
                            self.cursor.row = 0;
                            self.ensure_cursor_visible();
                        }
                        egui::Key::End => {
                            self.selection = None;
                            if let Some(pattern) = self.current_pattern() {
                                self.cursor.row = pattern.num_rows - 1;
                                self.ensure_cursor_visible();
                            }
                        }
                        egui::Key::Backspace if self.edit_mode => {
                            self.clear_cell_at_cursor();
                        }
                        egui::Key::Delete if self.edit_mode => {
                            if modifiers.alt {
                                let selected_order = self.selected_order;
                                let row = self.cursor.row;
                                let can_delete = self.current_pattern().map_or(false, |p| p.num_rows > 1);
                                if can_delete {
                                    let deleted_data: Vec<Cell> = self.current_pattern()
                                        .map(|p| p.data[row].to_vec())
                                        .unwrap_or_default();
                                    let pat_idx = self.module.as_ref()
                                        .and_then(|m| m.order_list.get(selected_order).copied())
                                        .unwrap_or(0) as usize;
                                    self.ensure_module_ownership();
                                    if let Some(ref mut module) = self.module {
                                        if let Some(arc_module) = Arc::get_mut(module) {
                                            let cmd = Box::new(crate::edit::DeleteRowCommand {
                                                pattern_index: pat_idx,
                                                row,
                                                _channel: None,
                                                deleted_data,
                                            });
                                            let _ = self.undo_manager.execute(cmd, arc_module);
                                        }
                                    }
                                    self.sync_module_to_audio();
                                }
                            } else {
                                self.clear_cell_at_cursor();
                                self.advance_cursor_down(1);
                            }
                        }
                        egui::Key::Insert if self.edit_mode => {
                            let selected_order = self.selected_order;
                            let row = self.cursor.row;
                            self.ensure_module_ownership();
                            if let Some(ref mut module) = self.module {
                                let pat_idx = *module.order_list.get(selected_order).unwrap_or(&0) as usize;
                                if let Some(arc_module) = Arc::get_mut(module) {
                                    let cmd = Box::new(InsertRowCommand {
                                        pattern_index: pat_idx,
                                        row,
                                        _channel: None,
                                    });
                                    let _ = self.undo_manager.execute(cmd, arc_module);
                                }
                            }
                            self.sync_module_to_audio();
                        }
                        egui::Key::Space => {
                            if self.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed) {
                                self.send_command(AudioCommand::Stop);
                            } else if self.edit_mode {
                                if let Some(last_cell) = self.last_entered_cell.clone() {
                                    self.set_cell_at_cursor(last_cell);
                                    self.advance_cursor_down(self.cursor_skip as usize);
                                }
                            }
                        }
                        egui::Key::F1 => {
                            self.show_shortcuts = !self.show_shortcuts;
                        }
                        egui::Key::F2 => {
                            self.edit_mode = !self.edit_mode;
                        }
                        egui::Key::F5 => {
                            self.send_command(AudioCommand::Play);
                        }
                        egui::Key::F6 => {
                            self.send_command(AudioCommand::SetPlayMode(crate::sequencer::player::PlayMode::Pattern));
                        }
                        egui::Key::F7 => {
                            self.send_command(AudioCommand::SetPlayMode(crate::sequencer::player::PlayMode::Order));
                        }
                        egui::Key::F8 => {
                            self.send_command(AudioCommand::Stop);
                        }
                        egui::Key::F9 => {
                            let order = self.playback_state.current_order.load(std::sync::atomic::Ordering::Relaxed);
                            let row = self.playback_state.current_row.load(std::sync::atomic::Ordering::Relaxed);
                            self.send_command(AudioCommand::PlayFrom { order, row });
                        }
                        egui::Key::F10 => {
                            let should_open = !self.settings_state.open;
                            if should_open {
                                self.settings_state = crate::ui::settings_window::SettingsState::from_config(&self.config);
                                self.settings_state.open = true;
                            } else {
                                self.settings_state.open = false;
                            }
                        }
                        egui::Key::Escape => {
                            self.selection = None;
                        }
                        egui::Key::OpenBracket => {
                            self.skip_to_prev_pattern();
                        }
                        egui::Key::CloseBracket => {
                            self.skip_to_next_pattern();
                        }
                        egui::Key::Comma => {
                            self.edit_mask_instrument = !self.edit_mask_instrument;
                            self.edit_mask_volume = self.edit_mask_instrument;
                        }
                        egui::Key::Num0 if modifiers.alt => { self.cursor_skip = 0; }
                        egui::Key::Num1 if modifiers.alt => { self.cursor_skip = 1; }
                        egui::Key::Num2 if modifiers.alt => { self.cursor_skip = 2; }
                        egui::Key::Num3 if modifiers.alt => { self.cursor_skip = 3; }
                        egui::Key::Num4 if modifiers.alt => { self.cursor_skip = 4; }
                        egui::Key::Num5 if modifiers.alt => { self.cursor_skip = 5; }
                        egui::Key::Num6 if modifiers.alt => { self.cursor_skip = 6; }
                        egui::Key::Num7 if modifiers.alt => { self.cursor_skip = 7; }
                        egui::Key::Num8 if modifiers.alt => { self.cursor_skip = 8; }
                        egui::Key::Num9 if modifiers.alt => { self.cursor_skip = 9; }
                        egui::Key::Minus if !modifiers.alt => { self.skip_to_prev_pattern(); }
                        egui::Key::Equals if !modifiers.alt => { self.skip_to_next_pattern(); }
                        egui::Key::C if modifiers.alt && self.edit_mode => { self.copy_selection(); }
                        egui::Key::P if modifiers.alt && self.edit_mode => { self.paste_at_cursor(); }
                        egui::Key::Z if modifiers.alt && self.edit_mode => {
                            if self.selection.is_some() {
                                self.handle_context_menu_action(crate::ui::pattern_grid::ContextMenuAction::Reverse);
                            }
                        }
                        egui::Key::F if modifiers.alt && self.edit_mode => {
                            if self.selection.is_some() {
                                self.handle_context_menu_action(crate::ui::pattern_grid::ContextMenuAction::FillInstrument);
                            }
                        }
                        egui::Key::I if modifiers.alt && self.edit_mode => {
                            if self.selection.is_some() {
                                self.handle_context_menu_action(crate::ui::pattern_grid::ContextMenuAction::InterpolateVolume);
                            }
                        }
                        egui::Key::K if modifiers.alt && self.edit_mode => {
                            if self.selection.is_some() {
                                self.handle_context_menu_action(crate::ui::pattern_grid::ContextMenuAction::InterpolateEffect);
                            }
                        }
                        egui::Key::R if modifiers.alt && self.edit_mode => {
                            if self.selection.is_some() {
                                self.handle_context_menu_action(crate::ui::pattern_grid::ContextMenuAction::Randomize);
                            }
                        }
                        _ => {}
                    },
                    egui::Event::Text(text) => {
                        if self.module.is_none() {
                            return;
                        }
                        let ch = text.chars().next().unwrap_or('\0');
                        self.handle_text_input(ch);
                    }
                    _ => {}
                }
            }
        });
    }

    fn preview_note(&mut self, note_key: u8) {
        let vol = 0.75;
        let sample_idx = self.selected_sample;
        self.send_command(crate::audio::commands::AudioCommand::TriggerPreviewNote {
            sample_index: sample_idx,
            note_key,
            volume: vol,
            panning: 0.5,
        });
    }

    fn handle_text_input(&mut self, ch: char) {
        if self.current_pattern().is_none() {
            return;
        }

        if self.cursor.sub_column.accepts_note() {
            for (key, tone) in NOTE_KEYS_LOWER.iter() {
                let key_char = key.name();
                if key_char.len() == 1 && key_char.chars().next() == Some(ch.to_ascii_uppercase()) {
                    let note_key = self.current_octave as u8 * 12 + tone;
                    self.preview_note(note_key);
                    if self.edit_mode {
                        let note = Note::On(note_key);
                        let mut new_cell = self.get_cell_at_cursor();
                        new_cell.note = note;
                        self.set_cell_at_cursor(new_cell);
                        self.last_entered_cell = Some(new_cell);
                        self.advance_cursor_down(self.cursor_skip as usize);
                    }
                    return;
                }
            }
            for (key, tone) in NOTE_KEYS_UPPER.iter() {
                let key_char = key.name();
                if key_char.len() == 1 && key_char.chars().next() == Some(ch.to_ascii_uppercase()) {
                    let note_key = (self.current_octave as u8 + 1) * 12 + tone;
                    self.preview_note(note_key);
                    if self.edit_mode {
                        let note = Note::On(note_key);
                        let mut new_cell = self.get_cell_at_cursor();
                        new_cell.note = note;
                        self.set_cell_at_cursor(new_cell);
                        self.last_entered_cell = Some(new_cell);
                        self.advance_cursor_down(self.cursor_skip as usize);
                    }
                    return;
                }
            }
            if ch == '.' && self.edit_mode {
                let mut new_cell = self.get_cell_at_cursor();
                new_cell.note = Note::Off;
                self.set_cell_at_cursor(new_cell);
                self.last_entered_cell = Some(new_cell);
                self.advance_cursor_down(self.cursor_skip as usize);
                return;
            }
        }

        if !self.edit_mode {
            return;
        }

        if self.cursor.sub_column.accepts_decimal() {
            if let Some(d) = ch.to_digit(10) {
                let d = d as u8;
                let mut cell = self.get_cell_at_cursor();

                match self.cursor.sub_column {
                    SubColumn::InstrumentTens => {
                        let current = cell.instrument.unwrap_or(0);
                        cell.instrument = Some(d * 10 + (current % 10));
                    }
                    SubColumn::InstrumentOnes => {
                        let current = cell.instrument.unwrap_or(0);
                        cell.instrument = Some((current / 10 * 10) + d);
                    }
                    SubColumn::VolumeTens => {
                        let current = cell.volume.unwrap_or(0);
                        let val = d * 10 + (current % 10);
                        cell.volume = Some(val.min(64));
                    }
                    SubColumn::VolumeOnes => {
                        let current = cell.volume.unwrap_or(0);
                        let val = (current / 10 * 10) + d;
                        cell.volume = Some(val.min(64));
                    }
                    SubColumn::Note
                    | SubColumn::EffectType
                    | SubColumn::EffectParamHigh
                    | SubColumn::EffectParamLow => return,
                }

                self.set_cell_at_cursor(cell);

                if let Some(next) = self.cursor.sub_column.next() {
                    self.cursor.sub_column = next;
                } else {
                    self.cursor.sub_column = SubColumn::Note;
                    self.advance_cursor_down(self.cursor_skip as usize);
                }
                return;
            }
        }

        if self.cursor.sub_column.accepts_hex() {
            if let Some(d) = ch.to_ascii_uppercase().to_digit(16) {
                let d = d as u8;
                let mut cell = self.get_cell_at_cursor();

                match self.cursor.sub_column {
                    SubColumn::EffectType => {
                        cell.effect = hex_to_effect(d);
                    }
                    SubColumn::EffectParamHigh => {
                        let param = effect_param(&cell.effect);
                        let new_param = (d << 4) | (param & 0x0F);
                        cell.effect = set_effect_param(&cell.effect, new_param);
                    }
                    SubColumn::EffectParamLow => {
                        let param = effect_param(&cell.effect);
                        let new_param = (param & 0xF0) | d;
                        cell.effect = set_effect_param(&cell.effect, new_param);
                    }
                    _ => return,
                }

                self.set_cell_at_cursor(cell);

                if let Some(next) = self.cursor.sub_column.next() {
                    self.cursor.sub_column = next;
                } else {
                    self.cursor.sub_column = SubColumn::Note;
                    self.advance_cursor_down(self.cursor_skip as usize);
                }
            }
        }

        if ch == '.' && !self.cursor.sub_column.accepts_note() {
            let mut cell = self.get_cell_at_cursor();
            match self.cursor.sub_column {
                SubColumn::InstrumentTens | SubColumn::InstrumentOnes => {
                    cell.instrument = None;
                }
                SubColumn::VolumeTens | SubColumn::VolumeOnes => {
                    cell.volume = None;
                }
                SubColumn::EffectType | SubColumn::EffectParamHigh | SubColumn::EffectParamLow => {
                    cell.effect = Effect::None;
                }
                SubColumn::Note => {}
            }
            self.set_cell_at_cursor(cell);
        }
    }

    fn move_cursor_right(&mut self) {
        let num_ch = self.num_channels();
        if self.cursor.channel < num_ch - 1 {
            self.cursor.channel += 1;
            self.cursor.sub_column = SubColumn::Note;
        }
        self.ensure_cursor_visible();
    }

    fn move_cursor_left(&mut self) {
        if self.cursor.channel > 0 {
            self.cursor.channel -= 1;
            self.cursor.sub_column = SubColumn::Note;
        }
        self.ensure_cursor_visible();
    }

    fn step_sub_column_forward(&mut self) {
        if let Some(next) = self.cursor.sub_column.next() {
            self.cursor.sub_column = next;
        }
    }

    fn step_sub_column_backward(&mut self) {
        if let Some(prev) = self.cursor.sub_column.prev() {
            self.cursor.sub_column = prev;
        }
    }

    fn extend_selection_down(&mut self) {
        if self.selection.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        self.advance_cursor_down(1);
        if let Some(anchor) = self.selection_anchor {
            self.selection = Some(Selection {
                start: anchor,
                end: self.cursor,
            });
        }
    }

    fn extend_selection_up(&mut self) {
        if self.selection.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        self.advance_cursor_up(1);
        if let Some(anchor) = self.selection_anchor {
            self.selection = Some(Selection {
                start: anchor,
                end: self.cursor,
            });
        }
    }

    fn extend_selection_right(&mut self) {
        if self.selection.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        self.move_cursor_right();
        if let Some(anchor) = self.selection_anchor {
            self.selection = Some(Selection {
                start: anchor,
                end: self.cursor,
            });
        }
    }

    fn extend_selection_left(&mut self) {
        if self.selection.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        self.move_cursor_left();
        if let Some(anchor) = self.selection_anchor {
            self.selection = Some(Selection {
                start: anchor,
                end: self.cursor,
            });
        }
    }

    fn select_all(&mut self) {
        if let Some(pattern) = self.current_pattern() {
            let num_ch = self.num_channels();
            let sel = Selection {
                start: CursorPosition {
                    row: 0,
                    channel: 0,
                    sub_column: SubColumn::Note,
                },
                end: CursorPosition {
                    row: pattern.num_rows - 1,
                    channel: num_ch - 1,
                    sub_column: SubColumn::EffectParamLow,
                },
            };
            self.selection = Some(sel);
        }
    }

    fn transpose_selection(&mut self, delta: i8) {
        let sel = match &self.selection {
            Some(s) => s.clone(),
            None => {
                let cursor = self.cursor;
                Selection {
                    start: cursor,
                    end: cursor,
                }
            }
        };
        let (min, max) = sel.normalized();
        let selected_order = self.selected_order;

        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let pat_idx = *arc_module.order_list.get(selected_order).unwrap_or(&0) as usize;
                if pat_idx < arc_module.patterns.len() {
                    let mut old_notes = Vec::new();
                    for row in min.row..=max.row {
                        for ch in min.channel..=max.channel {
                            let note = arc_module.patterns[pat_idx].data[row][ch].note;
                            if let Note::On(_) = note {
                                old_notes.push((row, ch, note));
                            }
                        }
                    }
                    let cmd = crate::edit::TransposeCommand {
                        order: selected_order,
                        delta,
                        old_notes,
                    };
                    let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                }
            }
        }
        self.sync_module_to_audio();
    }

    fn handle_context_menu_action(&mut self, action: crate::ui::pattern_grid::ContextMenuAction) {
        if !self.edit_mode {
            return;
        }
        let sel = match &self.selection {
            Some(s) => s.clone(),
            None => return,
        };
        let (min, max) = sel.normalized();
        let selected_order = self.selected_order;

        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let pat_idx = *arc_module.order_list.get(selected_order).unwrap_or(&0) as usize;
                if pat_idx >= arc_module.patterns.len() {
                    return;
                }

                match action {
                    crate::ui::pattern_grid::ContextMenuAction::FillInstrument => {
                        let mut old_cells = Vec::new();
                        for row in min.row..=max.row {
                            for ch in min.channel..=max.channel {
                                let cell = arc_module.patterns[pat_idx].data[row][ch];
                                if cell.note != Note::None {
                                    old_cells.push((row, ch, cell));
                                }
                            }
                        }
                        let cmd = crate::edit::FillInstrumentCommand {
                            order: selected_order,
                            old_cells,
                            instrument: self.selected_instrument as u8,
                        };
                        let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                    }
                    crate::ui::pattern_grid::ContextMenuAction::InterpolateVolume => {
                        let mut old_cells = Vec::new();
                        let mut new_cells = Vec::new();
                        for ch in min.channel..=max.channel {
                            let first_vol = arc_module.patterns[pat_idx].data[min.row][ch].volume;
                            let last_vol = arc_module.patterns[pat_idx].data[max.row][ch].volume;
                            if let (Some(fv), Some(lv)) = (first_vol, last_vol) {
                                let total = max.row - min.row;
                                for (step, row) in (min.row..=max.row).enumerate() {
                                    let old_cell = arc_module.patterns[pat_idx].data[row][ch];
                                    old_cells.push((row, ch, old_cell));
                                    let mut new_cell = old_cell;
                                    new_cell.volume = Some(crate::edit::interpolate_u8(fv, lv, step, total));
                                    new_cells.push((row, ch, new_cell));
                                }
                            }
                        }
                        if !new_cells.is_empty() {
                            let cmd = crate::edit::InterpolateCommand {
                                order: selected_order,
                                old_cells,
                                new_cells,
                            };
                            let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                        }
                    }
                    crate::ui::pattern_grid::ContextMenuAction::InterpolateEffect => {
                        let mut old_cells = Vec::new();
                        let mut new_cells = Vec::new();
                        for ch in min.channel..=max.channel {
                            let first_param = crate::sequencer::effect::effect_param_value(&arc_module.patterns[pat_idx].data[min.row][ch].effect);
                            let last_param = crate::sequencer::effect::effect_param_value(&arc_module.patterns[pat_idx].data[max.row][ch].effect);
                            if let (Some(fp), Some(lp)) = (first_param, last_param) {
                                let total = max.row - min.row;
                                for (step, row) in (min.row..=max.row).enumerate() {
                                    let old_cell = arc_module.patterns[pat_idx].data[row][ch];
                                    old_cells.push((row, ch, old_cell));
                                    let new_val = crate::edit::interpolate_u8(fp, lp, step, total);
                                    let new_cell = crate::sequencer::effect::set_effect_param_value(old_cell, new_val);
                                    new_cells.push((row, ch, new_cell));
                                }
                            }
                        }
                        if !new_cells.is_empty() {
                            let cmd = crate::edit::InterpolateCommand {
                                order: selected_order,
                                old_cells,
                                new_cells,
                            };
                            let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                        }
                    }
                    crate::ui::pattern_grid::ContextMenuAction::Reverse => {
                        for ch in min.channel..=max.channel {
                            let old_cells: Vec<Cell> = (min.row..=max.row)
                                .map(|r| arc_module.patterns[pat_idx].data[r][ch])
                                .collect();
                            let cmd = crate::edit::ReverseCommand {
                                order: selected_order,
                                channel: ch,
                                start_row: min.row,
                                end_row: max.row,
                                old_cells,
                            };
                            let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                        }
                    }
                    crate::ui::pattern_grid::ContextMenuAction::Randomize => {
                        let mut old_cells = Vec::new();
                        let mut new_cells = Vec::new();
                        for row in min.row..=max.row {
                            for ch in min.channel..=max.channel {
                                let old_cell = arc_module.patterns[pat_idx].data[row][ch];
                                old_cells.push((row, ch, old_cell));
                                let mut new_cell = old_cell;
                                if let Note::On(key) = old_cell.note {
                                    let new_key = crate::edit::random_u8(key.saturating_sub(12).max(0), (key as u16 + 12).min(119) as u8);
                                    new_cell.note = Note::On(new_key);
                                }
                                if let Some(v) = old_cell.volume {
                                    let min_v = v.saturating_sub(16);
                                    let max_v = (v as u16 + 16).min(255) as u8;
                                    new_cell.volume = Some(crate::edit::random_u8(min_v, max_v));
                                }
                                new_cells.push((row, ch, new_cell));
                            }
                        }
                        if !new_cells.is_empty() {
                            let cmd = crate::edit::RandomizeCommand {
                                order: selected_order,
                                old_cells,
                                new_cells,
                            };
                            let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                        }
                    }
                }
            }
        }
        self.sync_module_to_audio();
    }

    fn skip_to_prev_pattern(&mut self) {
        let order_len = self.module.as_ref().map_or(0, |m| m.order_list.len());
        if order_len == 0 {
            return;
        }
        self.selected_order = if self.selected_order == 0 {
            order_len - 1
        } else {
            self.selected_order - 1
        };
        self.cursor.row = 0;
        self.ensure_cursor_visible();
        self.selection = None;
        if self.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed) {
            self.send_command(AudioCommand::PlayFrom { order: self.selected_order as u16, row: 0 });
        }
    }

    fn skip_to_next_pattern(&mut self) {
        let order_len = self.module.as_ref().map_or(0, |m| m.order_list.len());
        if order_len == 0 {
            return;
        }
        self.selected_order = if self.selected_order >= order_len - 1 {
            0
        } else {
            self.selected_order + 1
        };
        self.cursor.row = 0;
        self.ensure_cursor_visible();
        self.selection = None;
        if self.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed) {
            self.send_command(AudioCommand::PlayFrom { order: self.selected_order as u16, row: 0 });
        }
    }

    fn copy_selection(&mut self) {
        if let Some(sel) = &self.selection {
            let (min, max) = sel.normalized();
            if let Some(pattern) = self.current_pattern() {
                let mut data = Vec::new();
                for row in min.row..=max.row {
                    let mut row_data = Vec::new();
                    for ch in min.channel..=max.channel {
                        row_data.push(*pattern.cell(row, ch));
                    }
                    data.push(row_data);
                }
                self.clipboard = Some(data);
                self.clipboard_width = max.channel - min.channel + 1;
            }
        }
    }

    fn delete_selection(&mut self) {
        let sel = match &self.selection {
            Some(s) => s.clone(),
            None => return,
        };
        let (min, max) = sel.normalized();
        let selected_order = self.selected_order;

        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let pat_idx = *arc_module.order_list.get(selected_order).unwrap_or(&0) as usize;
                if pat_idx >= arc_module.patterns.len() {
                    return;
                }
                let pattern = &arc_module.patterns[pat_idx];
                let mut old_cells = Vec::new();
                let mut new_cells = Vec::new();
                for row in min.row..=max.row {
                    for ch in min.channel..=max.channel {
                        if row < pattern.num_rows && ch < crate::sequencer::pattern::MAX_CHANNELS {
                            let old = pattern.data[row][ch];
                            if !old.is_empty() {
                                old_cells.push((row, ch, old));
                                new_cells.push((row, ch, Cell::default()));
                            }
                        }
                    }
                }
                if !old_cells.is_empty() {
                    let cmd = Box::new(crate::edit::BulkSetCellsCommand {
                        order: selected_order,
                        old_cells,
                        new_cells,
                    });
                    let _ = self.undo_manager.execute(cmd, arc_module);
                }
            }
        }
        self.sync_module_to_audio();
    }

    fn paste_at_cursor(&mut self) {
        let clipboard = self.clipboard.clone();
        let clipboard_data = match &clipboard {
            Some(d) => d.clone(),
            None => return,
        };
        let selected_order = self.selected_order;
        let cursor_row = self.cursor.row;
        let cursor_ch = self.cursor.channel;

        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let pat_idx = *arc_module.order_list.get(selected_order).unwrap_or(&0) as usize;
                if pat_idx >= arc_module.patterns.len() {
                    return;
                }
                let pattern = &arc_module.patterns[pat_idx];
                let mut old_cells = Vec::new();
                let mut new_cells = Vec::new();
                for (row_offset, row_data) in clipboard_data.iter().enumerate() {
                    let target_row = cursor_row + row_offset;
                    if target_row >= pattern.num_rows {
                        continue;
                    }
                    for (ch_offset, cell) in row_data.iter().enumerate() {
                        let target_ch = cursor_ch + ch_offset;
                        if target_ch >= crate::sequencer::pattern::MAX_CHANNELS {
                            continue;
                        }
                        if cell.is_empty() {
                            continue;
                        }
                        let old = pattern.data[target_row][target_ch];
                        old_cells.push((target_row, target_ch, old));
                        new_cells.push((target_row, target_ch, *cell));
                    }
                }
                if !old_cells.is_empty() {
                    let cmd = Box::new(crate::edit::BulkSetCellsCommand {
                        order: selected_order,
                        old_cells,
                        new_cells,
                    });
                    let _ = self.undo_manager.execute(cmd, arc_module);
                }
            }
        }
        self.sync_module_to_audio();
    }

    fn open_file_dialog(&mut self) {
        self.file_browser.open(BrowserMode::Modules);
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
                // Set name from filename if empty
                if sample.name.is_empty() {
                    if let Some(name) = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()) {
                        sample.name = name.to_string();
                    }
                }

                // Create module if it doesn't exist
                if self.module.is_none() {
                    self.new_song();
                }

                let sample_idx = self.selected_sample;
                self.ensure_module_ownership();
                if let Some(ref mut module_arc) = self.module {
                    if let Some(m) = Arc::get_mut(module_arc) {
                        // Ensure the sample vector is large enough
                        if sample_idx >= m.samples.len() {
                            m.samples.resize(sample_idx + 1, crate::sequencer::Sample::default());
                        }
                        m.samples[sample_idx] = sample;
                    }
                }
                self.sync_module_to_audio();
            }
            Err(e) => {
                eprintln!("Failed to import WAV: {}", e);
            }
        }
    }

    fn save_current_file(&mut self) {
        let path = match &self.file_path {
            Some(p) => p.clone(),
            None => {
                self.save_as_dialog();
                return;
            }
        };
        self.save_file(&path);
    }

    fn save_file(&mut self, path: &str) {
        let module = match &self.module {
            Some(m) => m,
            None => return,
        };
        let data = formats::save_module(module);
        match std::fs::write(path, &data) {
            Ok(()) => {
                self.file_path = Some(path.to_string());
                self.module_dirty = false;
                self.last_backup_time = std::time::Instant::now();
            }
            Err(e) => {
                eprintln!("Failed to save file: {}", e);
            }
        }
    }

    fn save_as_dialog(&mut self) {
        self.file_browser.open(BrowserMode::Projects);
    }

    fn open_wav_export_dialog(&mut self) {
        let module_loaded = self.module.is_some();
        let total_orders = self.module.as_ref().map(|m| m.order_list.len()).unwrap_or(0) as u64;
        let sample_rate = if self.current_sample_rate > 0 {
            self.current_sample_rate
        } else {
            44100
        };

        let default_name = if !self.loaded_module_name.is_empty() {
            self.loaded_module_name.clone()
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

        let module = match &self.module {
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
        if let Some(ref path) = self.file_path {
            self.config.last_file_path = Some(path.clone());
        }
        self.config.favorites = self.file_browser.save_favorites();
        self.config.save();
    }

    fn check_auto_backup(&mut self) {
        let interval = self.config.auto_backup_interval_secs;
        if interval == 0 || !self.module_dirty || self.module.is_none() {
            return;
        }
        if self.last_backup_time.elapsed().as_secs() < interval {
            return;
        }

        let backup_dir = self.config.get_backup_dir();
        let _ = std::fs::create_dir_all(&backup_dir);

        let name = if self.loaded_module_name.is_empty() {
            "untitled".to_string()
        } else {
            self.loaded_module_name.trim_end_matches(".htk")
                .trim_end_matches(".it")
                .trim_end_matches(".xm")
                .trim_end_matches(".s3m")
                .trim_end_matches(".mod")
                .to_string()
        };
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let backup_path = backup_dir.join(format!("{}_backup_{}.htk", name, timestamp));

        if let Some(ref module) = self.module {
            let data = crate::formats::save_module(module);
            let _ = std::fs::write(&backup_path, &data);
        }

        self.module_dirty = false;
        self.last_backup_time = std::time::Instant::now();
    }

    fn handle_sample_edit(&mut self, event: SampleEditEvent) {
        let sample_idx = self.selected_sample;
        let module = match &self.module {
            Some(m) => m,
            None => return,
        };
        let sample = match module.samples.get(sample_idx) {
            Some(s) => s,
            None => return,
        };

        let cmd: Box<dyn crate::edit::EditCommand> = match event {
            SampleEditEvent::NameChanged(n) => Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::Name(n),
                old_property: SampleProperty::Name(sample.name.clone()),
            }),
            SampleEditEvent::VolumeChanged(v) => Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::DefaultVolume(v),
                old_property: SampleProperty::DefaultVolume(sample.default_volume),
            }),
            SampleEditEvent::PanningChanged(p) => Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::DefaultPanning(p),
                old_property: SampleProperty::DefaultPanning(sample.default_panning),
            }),
            SampleEditEvent::GlobalVolumeChanged(v) => Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::GlobalVolume(v),
                old_property: SampleProperty::GlobalVolume(sample.global_volume),
            }),
            SampleEditEvent::LoopTypeChanged(t) => Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::LoopType(t),
                old_property: SampleProperty::LoopType(sample.loop_type),
            }),
            SampleEditEvent::LoopStartChanged(s) => Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::LoopStart(s),
                old_property: SampleProperty::LoopStart(sample.loop_start),
            }),
            SampleEditEvent::LoopEndChanged(e) => Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::LoopEnd(e),
                old_property: SampleProperty::LoopEnd(sample.loop_end),
            }),
            SampleEditEvent::RelativeNoteChanged(n) => Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::RelativeNote(n),
                old_property: SampleProperty::RelativeNote(sample.relative_note),
            }),
            SampleEditEvent::FineTuneChanged(t) => Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::FineTune(t),
                old_property: SampleProperty::FineTune(sample.fine_tune),
            }),
            SampleEditEvent::Normalize => {
                let mut data = (*sample.data).clone();
                let max = data.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
                if max > 0.0 {
                    let factor = 1.0 / max;
                    for x in data.iter_mut() {
                        *x *= factor;
                    }
                }
                Box::new(SetSampleDataCommand {
                    sample_index: sample_idx,
                    old_data: sample.data.clone(),
                    new_data: Arc::new(data),
                })
            }
            SampleEditEvent::Reverse => {
                let mut data = (*sample.data).clone();
                data.reverse();
                Box::new(SetSampleDataCommand {
                    sample_index: sample_idx,
                    old_data: sample.data.clone(),
                    new_data: Arc::new(data),
                })
            }
            SampleEditEvent::CutRegion(s, e) => {
                let s = s.min(e);
                let e = s.max(e);
                let mut data = (*sample.data).clone();
                self.sample_clipboard = Some(Arc::new(data[s..e].to_vec()));
                data.drain(s..e);
                Box::new(SetSampleDataCommand {
                    sample_index: sample_idx,
                    old_data: sample.data.clone(),
                    new_data: Arc::new(data),
                })
            }
            SampleEditEvent::CopyRegion(s, e) => {
                let s = s.min(e);
                let e = s.max(e);
                self.sample_clipboard = Some(Arc::new(sample.data[s..e].to_vec()));
                return;
            }
            SampleEditEvent::PasteRegion(pos) => {
                let clip = match self.sample_clipboard.as_ref() {
                    Some(c) => c.clone(),
                    None => return,
                };
                let data = (*sample.data).clone();
                let pos = pos.min(data.len());
                let mut new_data = Vec::with_capacity(data.len() + clip.len());
                new_data.extend_from_slice(&data[..pos]);
                new_data.extend_from_slice(&clip);
                new_data.extend_from_slice(&data[pos..]);
                Box::new(SetSampleDataCommand {
                    sample_index: sample_idx,
                    old_data: sample.data.clone(),
                    new_data: Arc::new(new_data),
                })
            }
            SampleEditEvent::CropRegion(s, e) => {
                let s = s.min(e);
                let e = s.max(e);
                let data = sample.data[s..e].to_vec();
                Box::new(SetSampleDataCommand {
                    sample_index: sample_idx,
                    old_data: sample.data.clone(),
                    new_data: Arc::new(data),
                })
            }
            SampleEditEvent::Amplify(factor) => {
                let mut data = (*sample.data).clone();
                for x in data.iter_mut() {
                    *x *= factor;
                }
                Box::new(SetSampleDataCommand {
                    sample_index: sample_idx,
                    old_data: sample.data.clone(),
                    new_data: Arc::new(data),
                })
            }
            SampleEditEvent::SilenceRegion(s, e) => {
                let s = s.min(e);
                let e = s.max(e);
                let mut data = (*sample.data).clone();
                for x in data[s..e].iter_mut() {
                    *x = 0.0;
                }
                Box::new(SetSampleDataCommand {
                    sample_index: sample_idx,
                    old_data: sample.data.clone(),
                    new_data: Arc::new(data),
                })
            }
            SampleEditEvent::TrimSilence => {
                let data = &sample.data;
                let threshold = 0.001;
                let start = data.iter().position(|&x| x.abs() > threshold).unwrap_or(0);
                let end = data.iter().rposition(|&x| x.abs() > threshold).map(|p| p + 1).unwrap_or(data.len());
                let trimmed = if start < end { data[start..end].to_vec() } else { Vec::new() };
                Box::new(SetSampleDataCommand {
                    sample_index: sample_idx,
                    old_data: sample.data.clone(),
                    new_data: Arc::new(trimmed),
                })
            }
            SampleEditEvent::SetLoopFromSelection(start, end) => {
                let start_cmd: Box<dyn crate::edit::EditCommand> = Box::new(SetSamplePropertyCommand {
                    sample_index: sample_idx,
                    property: SampleProperty::LoopStart(start),
                    old_property: SampleProperty::LoopStart(sample.loop_start),
                });
                let end_cmd: Box<dyn crate::edit::EditCommand> = Box::new(SetSamplePropertyCommand {
                    sample_index: sample_idx,
                    property: SampleProperty::LoopEnd(end),
                    old_property: SampleProperty::LoopEnd(sample.loop_end),
                });
                let type_cmd: Box<dyn crate::edit::EditCommand> = Box::new(SetSamplePropertyCommand {
                    sample_index: sample_idx,
                    property: SampleProperty::LoopType(crate::sequencer::sample::LoopType::Forward),
                    old_property: SampleProperty::LoopType(sample.loop_type),
                });
                self.ensure_module_ownership();
                if let Some(ref mut module_arc) = self.module {
                    if let Some(m) = Arc::get_mut(module_arc) {
                        let _ = self.undo_manager.execute(start_cmd, m);
                        let _ = self.undo_manager.execute(end_cmd, m);
                        let _ = self.undo_manager.execute(type_cmd, m);
                    }
                }
                self.sync_module_to_audio();
                return;
            }
        };

        self.ensure_module_ownership();
        if let Some(ref mut module_arc) = self.module {
            if let Some(m) = Arc::get_mut(module_arc) {
                let _ = self.undo_manager.execute(cmd, m);
            }
        }
        self.sync_module_to_audio();
    }

    fn handle_instrument_edit(&mut self, event: InstrumentEditEvent) {
        let inst_idx = self.selected_instrument;
        let module = match &self.module {
            Some(m) => m,
            None => return,
        };
        let inst = match module.instruments.get(inst_idx) {
            Some(i) => i,
            None => return,
        };

        let cmd: Box<dyn crate::edit::EditCommand> = match event {
            InstrumentEditEvent::NameChanged(n) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::Name(n),
                old_property: InstrumentProperty::Name(inst.name.clone()),
            }),
            InstrumentEditEvent::NnaChanged(n) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::Nna(n),
                old_property: InstrumentProperty::Nna(inst.nna),
            }),
            InstrumentEditEvent::DuplicateCheckTypeChanged(t) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::DuplicateCheckType(t),
                old_property: InstrumentProperty::DuplicateCheckType(inst.duplicate_check_type),
            }),
            InstrumentEditEvent::DuplicateCheckActionChanged(a) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::DuplicateCheckAction(a),
                old_property: InstrumentProperty::DuplicateCheckAction(inst.duplicate_check_action),
            }),
            InstrumentEditEvent::FadeoutChanged(f) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::Fadeout(f),
                old_property: InstrumentProperty::Fadeout(inst.fade_out),
            }),
            InstrumentEditEvent::GlobalVolumeChanged(v) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::GlobalVolume(v),
                old_property: InstrumentProperty::GlobalVolume(inst.global_volume),
            }),
            InstrumentEditEvent::PitchPanSeparationChanged(s) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::PitchPanSeparation(s),
                old_property: InstrumentProperty::PitchPanSeparation(inst.pitch_pan_separation),
            }),
            InstrumentEditEvent::PitchPanCenterChanged(c) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::PitchPanCenter(c),
                old_property: InstrumentProperty::PitchPanCenter(inst.pitch_pan_center),
            }),
            InstrumentEditEvent::RandomVolumeChanged(v) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::RandomVolume(v),
                old_property: InstrumentProperty::RandomVolume(inst.random_volume),
            }),
            InstrumentEditEvent::RandomPanningChanged(p) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::RandomPanning(p),
                old_property: InstrumentProperty::RandomPanning(inst.random_panning),
            }),
            InstrumentEditEvent::FilterCutoffChanged(c) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::FilterCutoff(c),
                old_property: InstrumentProperty::FilterCutoff(inst.filter_cutoff),
            }),
            InstrumentEditEvent::FilterResonanceChanged(r) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::FilterResonance(r),
                old_property: InstrumentProperty::FilterResonance(inst.filter_resonance),
            }),
            InstrumentEditEvent::FilterTypeChanged(t) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::FilterType(t),
                old_property: InstrumentProperty::FilterType(inst.filter_type),
            }),
            InstrumentEditEvent::FilterRandomCutoffChanged(c) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::FilterRandomCutoff(c),
                old_property: InstrumentProperty::FilterRandomCutoff(inst.filter_random_cutoff),
            }),
            InstrumentEditEvent::EnvelopePointMoved(env_type, idx, t, v) => {
                let env = match env_type {
                    EnvelopeType::Volume => &inst.volume_envelope,
                    EnvelopeType::Panning => &inst.panning_envelope,
                    EnvelopeType::Pitch => &inst.pitch_envelope,
                    EnvelopeType::Filter => &inst.filter_envelope,
                };
                let old_pt = env.as_ref().map(|e| e.points[idx]).unwrap_or_default();
                Box::new(SetEnvelopePointCommand {
                    instrument_index: inst_idx,
                    envelope_type: env_type,
                    point_index: idx,
                    old_point: old_pt,
                    new_point: EnvelopePoint { tick: t, value: v },
                })
            }
            InstrumentEditEvent::EnvelopePointAdded(env_type, t, v) => Box::new(AddEnvelopePointCommand {
                instrument_index: inst_idx,
                envelope_type: env_type,
                point: EnvelopePoint { tick: t, value: v },
            }),
            InstrumentEditEvent::EnvelopePointRemoved(env_type, idx) => {
                let env = match env_type {
                    EnvelopeType::Volume => &inst.volume_envelope,
                    EnvelopeType::Panning => &inst.panning_envelope,
                    EnvelopeType::Pitch => &inst.pitch_envelope,
                    EnvelopeType::Filter => &inst.filter_envelope,
                };
                let old_pt = env.as_ref().map(|e| e.points[idx]).unwrap_or_default();
                Box::new(RemoveEnvelopePointCommand {
                    instrument_index: inst_idx,
                    envelope_type: env_type,
                    point_index: idx,
                    old_point: old_pt,
                })
            }
            InstrumentEditEvent::EnvelopeSustainChanged(env_type, new_sustain) => {
                let env = match env_type {
                    EnvelopeType::Volume => &inst.volume_envelope,
                    EnvelopeType::Panning => &inst.panning_envelope,
                    EnvelopeType::Pitch => &inst.pitch_envelope,
                    EnvelopeType::Filter => &inst.filter_envelope,
                };
                Box::new(SetEnvelopeSustainCommand {
                    instrument_index: inst_idx,
                    envelope_type: env_type,
                    old_sustain: env.as_ref().and_then(|e| e.sustain_point),
                    new_sustain,
                })
            }
            InstrumentEditEvent::EnvelopeLoopChanged(env_type, new_enabled, new_start, new_end) => {
                let env = match env_type {
                    EnvelopeType::Volume => &inst.volume_envelope,
                    EnvelopeType::Panning => &inst.panning_envelope,
                    EnvelopeType::Pitch => &inst.pitch_envelope,
                    EnvelopeType::Filter => &inst.filter_envelope,
                };
                Box::new(SetEnvelopeLoopCommand {
                    instrument_index: inst_idx,
                    envelope_type: env_type,
                    old_loop_enabled: env.as_ref().map_or(false, |e| e.flags.loop_),
                    new_loop_enabled: new_enabled,
                    old_loop_start: env.as_ref().and_then(|e| e.loop_start),
                    new_loop_start: new_start,
                    old_loop_end: env.as_ref().and_then(|e| e.loop_end),
                    new_loop_end: new_end,
                })
            }
            InstrumentEditEvent::EnvelopeFlagsChanged(env_type, new_flags) => {
                let env = match env_type {
                    EnvelopeType::Volume => &inst.volume_envelope,
                    EnvelopeType::Panning => &inst.panning_envelope,
                    EnvelopeType::Pitch => &inst.pitch_envelope,
                    EnvelopeType::Filter => &inst.filter_envelope,
                };
                Box::new(SetEnvelopeFlagsCommand {
                    instrument_index: inst_idx,
                    envelope_type: env_type,
                    old_flags: env.as_ref().map(|e| e.flags).unwrap_or_default(),
                    new_flags,
                })
            }
            InstrumentEditEvent::SampleMapChanged(note, new_idx) => Box::new(MapNoteToSampleCommand {
                instrument_index: inst_idx,
                note,
                old_sample: inst.sample_map[note as usize],
                new_sample: new_idx,
            }),
            InstrumentEditEvent::NoteMapChanged(note, new_dest) => Box::new(MapNoteToNoteCommand {
                instrument_index: inst_idx,
                note,
                old_dest: inst.note_map[note as usize],
                new_dest,
            }),
            InstrumentEditEvent::VibTypeChanged(v) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::VibType(v),
                old_property: InstrumentProperty::VibType(inst.vib_type),
            }),
            InstrumentEditEvent::VibSweepChanged(v) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::VibSweep(v),
                old_property: InstrumentProperty::VibSweep(inst.vib_sweep),
            }),
            InstrumentEditEvent::VibDepthChanged(v) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::VibDepth(v),
                old_property: InstrumentProperty::VibDepth(inst.vib_depth),
            }),
            InstrumentEditEvent::VibRateChanged(v) => Box::new(SetInstrumentPropertyCommand {
                instrument_index: inst_idx,
                property: InstrumentProperty::VibRate(v),
                old_property: InstrumentProperty::VibRate(inst.vib_rate),
            }),
            InstrumentEditEvent::SampleMapFillAll(sample_idx) => Box::new(
                crate::edit::SetSampleMapCommand {
                    instrument_index: inst_idx,
                    new_sample_index: sample_idx,
                    old_map: inst.sample_map,
                },
            ),
            InstrumentEditEvent::SaveInstrument => {
                return;
            }
            InstrumentEditEvent::LoadInstrument => {
                return;
            }
        };

        self.ensure_module_ownership();
        if let Some(ref mut module_arc) = self.module {
            if let Some(m) = Arc::get_mut(module_arc) {
                let _ = self.undo_manager.execute(cmd, m);
            }
        }
        self.sync_module_to_audio();
    }

    fn save_instrument_dialog(&mut self) {
        let module = match &self.module {
            Some(m) => m,
            None => {
                eprintln!("No module loaded");
                return;
            }
        };
        let inst_idx = self.selected_instrument;
        let inst = match module.instruments.get(inst_idx) {
            Some(i) => i,
            None => {
                eprintln!("No instrument selected");
                return;
            }
        };
        let inst_name = if inst.name.is_empty() {
            format!("Instrument_{:02X}", inst_idx)
        } else {
            inst.name.clone()
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Save Instrument")
            .set_file_name(format!("{}.hti", inst_name))
            .add_filter("HTRK Instruments", &["hti"])
            .save_file()
        {
            self.save_instrument_to_file(inst_idx, path.to_string_lossy().as_ref());
        }
    }

    fn save_instrument_to_file(&mut self, inst_idx: usize, path: &str) {
        let module = match &self.module {
            Some(m) => m,
            None => return,
        };
        let inst = match module.instruments.get(inst_idx) {
            Some(i) => i,
            None => return,
        };
        let sample_indices: Vec<u8> = inst.sample_map.iter().cloned().collect();
        let samples: Vec<_> = sample_indices.iter()
            .filter_map(|&idx| {
                if idx > 0 && idx as usize - 1 < module.samples.len() {
                    Some(module.samples[idx as usize - 1].clone())
                } else {
                    None
                }
            })
            .collect();
        let data = match crate::formats::hti::save_instrument(inst, &samples) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to save instrument: {:?}", e);
                return;
            }
        };
        if let Err(e) = std::fs::write(path, &data) {
            eprintln!("Failed to write instrument file: {}", e);
        }
    }

    fn load_instrument_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Load Instrument")
            .add_filter("HTRK Instruments", &["hti"])
            .pick_file()
        {
            self.load_instrument_from_file(path.to_string_lossy().as_ref());
        }
    }

    fn load_instrument_from_file(&mut self, path: &str) {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read instrument file: {}", e);
                return;
            }
        };
        let (loaded_inst, loaded_samples) = match crate::formats::hti::load_instrument(&data) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Failed to load instrument: {:?}", e);
                return;
            }
        };
        let inst_idx = self.selected_instrument;
        if self.module.is_none() {
            self.new_song();
        }
        self.ensure_module_ownership();
        if let Some(ref mut module_arc) = self.module {
            if let Some(m) = Arc::get_mut(module_arc) {
                if inst_idx >= m.instruments.len() {
                    m.instruments.resize(inst_idx + 1, crate::sequencer::Instrument::default());
                }
                let sample_map = loaded_inst.sample_map.clone();
                m.instruments[inst_idx] = loaded_inst;
                let mut available_slots: Vec<usize> = (1..m.samples.len())
                    .filter(|&i| m.samples[i].data.is_empty())
                    .collect();
                let mut sample_mapping: HashMap<usize, usize> = HashMap::new();
                for (new_idx, sample) in loaded_samples.iter().enumerate() {
                    if let Some(&existing_idx) = available_slots.first() {
                        m.samples[existing_idx] = sample.clone();
                        sample_mapping.insert(new_idx + 1, existing_idx);
                        available_slots.remove(0);
                    } else {
                        let new_sample_idx = m.samples.len();
                        m.samples.push(sample.clone());
                        sample_mapping.insert(new_idx + 1, new_sample_idx);
                    }
                }
                let mut remapped_map = [0u8; 120];
                for (note, &old_idx) in sample_map.iter().enumerate().take(120) {
                    if let Some(&new_idx) = sample_mapping.get(&(old_idx as usize)) {
                        remapped_map[note] = new_idx as u8;
                    } else {
                        remapped_map[note] = old_idx;
                    }
                }
                m.instruments[inst_idx].sample_map = remapped_map;
            }
        }
        self.sync_module_to_audio();
    }
}

fn hex_to_effect(d: u8) -> crate::sequencer::Effect {
    match d {
        0 => crate::sequencer::Effect::Arpeggio { note1: 0, note2: 0 },
        1 => crate::sequencer::Effect::PortamentoUp { speed: 0 },
        2 => crate::sequencer::Effect::PortamentoDown { speed: 0 },
        3 => crate::sequencer::Effect::TonePortamento { speed: 0 },
        4 => crate::sequencer::Effect::Vibrato { speed: 0, depth: 0 },
        5 => crate::sequencer::Effect::TonePortamentoVolumeSlide { up: 0 },
        6 => crate::sequencer::Effect::VibratoVolumeSlide { up: 0 },
        7 => crate::sequencer::Effect::Tremolo { speed: 0, depth: 0 },
        8 => crate::sequencer::Effect::SetPanning { pan: 0 },
        9 => crate::sequencer::Effect::SetSampleOffset { offset: 0 },
        0xA => crate::sequencer::Effect::VolumeSlide { up: 0, down: 0 },
        0xB => crate::sequencer::Effect::PositionJump { order: 0 },
        0xC => crate::sequencer::Effect::SetVolume { volume: 0 },
        0xD => crate::sequencer::Effect::PatternBreak { row: 0 },
        0xE => crate::sequencer::Effect::ExtendedEffect { param: 0 },
        0xF => crate::sequencer::Effect::SetSpeed { speed: 0 },
        _ => crate::sequencer::Effect::None,
    }
}

fn effect_param(effect: &crate::sequencer::Effect) -> u8 {
    crate::sequencer::effect::effect_param_value(effect).unwrap_or(0)
}

fn set_effect_param(effect: &crate::sequencer::Effect, param: u8) -> crate::sequencer::Effect {
    let mut fake_cell = crate::sequencer::pattern::Cell::default();
    fake_cell.effect = *effect;
    crate::sequencer::effect::set_effect_param_value(fake_cell, param).effect
}

impl eframe::App for HtrkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_zoom_factor(self.config.zoom_factor);

        if self.stream.is_none() && !self.audio_init_failed {
            self.init_audio();
        }

        self.handle_keyboard_input(ctx);

        self.update_wav_export_progress();

        let (playback_row, playback_order, playback_pattern) = {
            let playing = self.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed);
            if playing {
                let order = self.playback_state.current_order.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let row = self.playback_state.current_row.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let pat = self.playback_state.current_pattern.load(std::sync::atomic::Ordering::Relaxed) as usize;
                (Some(row), Some(order), Some(pat))
            } else {
                (None, None, None)
            }
        };

        if let Some(order) = playback_order {
            if self.follow_playback && order != self.selected_order {
                if let Some(ref module) = self.module {
                    if order < module.order_list.len() {
                        self.selected_order = order;
                        self.cursor.row = 0;
                        self.ensure_cursor_visible();
                    }
                }
            }
        }

        if let Some(row) = playback_row {
            if self.follow_playback {
                if let (Some(active_pat), Some(ref module)) = (playback_pattern, self.module.as_ref()) {
                    if !module.order_list.is_empty() {
                        let order_idx = self.selected_order.min(module.order_list.len().saturating_sub(1));
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
                self.undo_manager.can_undo(),
                self.undo_manager.can_redo(),
                self.selection.is_some(),
                self.follow_playback,
                self.theme_preset,
                &self.theme,
                self.current_sample_rate,
                &self.current_sample_format,
            );
            if menu_resp.new_song {
                self.new_song();
            }
            if menu_resp.open_file {
                self.open_file_dialog();
            }
            if menu_resp.import_sample {
                self.file_browser.open(BrowserMode::Samples);
            }
            if menu_resp.import_instrument {
                self.file_browser.open(BrowserMode::Instruments);
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
                if let Some(ref mut module) = self.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let _ = self.undo_manager.undo(arc_module);
                    }
                }
                self.sync_module_to_audio();
            }
            if menu_resp.redo {
                self.ensure_module_ownership();
                if let Some(ref mut module) = self.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let _ = self.undo_manager.redo(arc_module);
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
                self.command_sender = None;
                self.init_audio();
            }
        }

        egui::TopBottomPanel::top("transport_bar").show(ctx, |ui| {
            let transport_resp = crate::ui::transport::draw_transport(
                ui,
                &self.playback_state,
                &mut self.command_sender,
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
                    &self.playback_state,
                    &self.theme,
                    num_ch,
                );
            });

        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(22.0)
            .show(ctx, |ui| {
                let cpu = self.playback_state.cpu_usage_pct.load(std::sync::atomic::Ordering::Relaxed);
                let total_rows = self.current_pattern().map_or(64, |p| p.num_rows);
                let hint = format!("Ins: {} | Smp: {}", self.selected_instrument, self.selected_sample);
                crate::ui::status_bar::draw_status_bar(
                    ui,
                    self.module.as_ref().map(|m| m.as_ref()),
                    self.selected_order,
                    self.cursor.row,
                    total_rows,
                    self.num_channels(),
                    cpu,
                    self.current_octave,
                    self.cursor_skip,
                    self.selected_instrument,
                    self.selected_sample,
                    self.edit_mode,
                    &hint,
                    &self.theme,
                );
            });

        egui::SidePanel::left("order_list")
            .min_width(120.0)
            .default_width(150.0)
            .show(ctx, |ui| {
                if let Some(ref module) = self.module {
                    let order_resp = crate::ui::order_list::draw_order_list(
                        ui,
                        module,
                        self.selected_order,
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
                        self.selected_order = idx;
                        self.cursor.row = 0;
                        self.ensure_cursor_visible();
                    }
                    let mut changed = false;
                    self.ensure_module_ownership();
                    if let Some(ref mut m) = self.module {
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
                                    arc_module.order_list.insert(self.selected_order + 1, new_pat);
                                    changed = true;
                                }
                                if should_delete && arc_module.order_list.len() > 1 {
                                    if self.selected_order < arc_module.order_list.len() {
                                        arc_module.order_list.remove(self.selected_order);
                                        if self.selected_order >= arc_module.order_list.len() {
                                            self.selected_order = arc_module.order_list.len().saturating_sub(1);
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
                                    self.selected_order = insert_at;
                                    changed = true;
                                }
                            }
                            if should_duplicate {
                                let cur_pat_idx = *arc_module.order_list.get(self.selected_order).unwrap_or(&0) as usize;
                                if cur_pat_idx < arc_module.patterns.len() {
                                    let cloned = arc_module.patterns[cur_pat_idx].clone();
                                    let new_idx = arc_module.patterns.len() as u8;
                                    arc_module.patterns.push(cloned);
                                    let insert_at = (self.selected_order + 1).min(arc_module.order_list.len());
                                    arc_module.order_list.insert(insert_at, new_idx);
                                    self.selected_order = insert_at;
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
            if self.module.is_none() {
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
            });
            ui.separator();

            match self.current_view {
                AppView::Pattern => {
                    let num_channels = self.num_channels();

                    let metrics = crate::ui::pattern_grid::GridMetrics::new(self.config.editor_font_size as f32);
                    let visible_channels = crate::ui::pattern_grid::GridMetrics::calculate_visible_channels(ui, metrics);
                    let visible_channels = visible_channels.min(num_channels - self.scroll_channel).max(1);

                    let ch_resp = crate::ui::channel_headers::draw_channel_headers(
                        ui,
                        num_channels,
                        self.scroll_channel,
                        visible_channels,
                        &self.muted_channels,
                        &self.solo_channels,
                        &self.channel_names,
                        &self.module.as_ref().map(|m| m.channel_panning.clone()).unwrap_or_default(),
                        &self.send_levels,
                        &mut self.channel_rename_state,
                        &self.theme,
                        &self.playback_state,
                        metrics,
                    );

                    if let Some(ch) = ch_resp.toggle_mute {
                        self.muted_channels[ch] = !self.muted_channels[ch];
                        self.send_command(AudioCommand::SetChannelMuted {
                            channel: ch,
                            muted: self.muted_channels[ch],
                        });
                    }
                    if let Some(ch) = ch_resp.toggle_solo {
                        self.solo_channels[ch] = !self.solo_channels[ch];
                        self.send_command(AudioCommand::SetChannelSolo {
                            channel: ch,
                            solo: self.solo_channels[ch],
                        });
                    }
                    if let Some((ch, si, level)) = ch_resp.send_changed {
                        if ch < self.send_levels.len() && si < 2 {
                            self.send_levels[ch][si] = level;
                            self.send_command(crate::audio::commands::AudioCommand::SetSendLevel {
                                channel: ch,
                                send_index: si,
                                level,
                            });
                        }
                    }
                    if let Some((ch, name)) = ch_resp.rename_channel {
                        if ch < self.channel_names.len() {
                            self.channel_names[ch] = name;
                        }
                    }

                    if let Some(module) = &self.module {
                        if !module.order_list.is_empty() {
                            let order_idx = self.selected_order.min(module.order_list.len().saturating_sub(1));
                            let pat_idx = module.order_list[order_idx] as usize;
                            let grid_playback_row = if playback_pattern == Some(pat_idx) { playback_row } else { None };
                            if let Some(pattern) = module.patterns.get(pat_idx) {
                                let grid_resp = crate::ui::pattern_grid::draw_pattern_grid(
                                    ui,
                                    pattern,
                                    &self.cursor,
                                    self.selection.as_ref(),
                                    grid_playback_row,
                                    self.scroll_row,
                                    self.scroll_channel,
                                    num_channels,
                                    metrics,
                                    &self.theme,
                                    self.config.row_highlight_minor,
                                    self.config.row_highlight_major,
                                );

                                self.last_visible_rows = grid_resp.visible_rows;
                                self.last_visible_channels = grid_resp.visible_channels;

                                if let Some(pos) = grid_resp.clicked_position {
                                    self.cursor = pos;
                                    self.selection = None;
                                    self.selection_anchor = None;
                                    self.ensure_cursor_visible();
                                }
                                if let Some(pos) = grid_resp.drag_position {
                                    if self.selection_anchor.is_none() {
                                        self.selection_anchor = Some(self.cursor);
                                    }
                                    self.cursor = pos;
                                    if let Some(anchor) = self.selection_anchor {
                                        self.selection = Some(Selection {
                                            start: anchor,
                                            end: self.cursor,
                                        });
                                    }
                                    self.ensure_cursor_visible();
                                }
                                if let Some(action) = grid_resp.context_menu_action {
                                    self.handle_context_menu_action(action);
                                }
                                if let Some(tooltip) = grid_resp.effect_tooltip {
                                    ui.label(egui::RichText::new(&tooltip).size(10.0).color(egui::Color32::GRAY));
                                }
                            }
                        }
                    }
                }
                AppView::Sample => {
                    if let Some(module) = &self.module {
                        if let Some(event) = crate::ui::sample_editor::draw_sample_editor(
                            ui,
                            module,
                            &mut self.selected_sample,
                            &self.theme,
                            &mut self.sample_selection,
                            &mut self.sample_clipboard,
                            &mut self.amplify_factor,
                        ) {
                            self.handle_sample_edit(event);
                        }
                    }
                }
                AppView::Instrument => {
                    if let Some(module) = &self.module {
                        if let Some(event) = crate::ui::instrument_editor::draw_instrument_editor(
                            ui,
                            module,
                            &mut self.selected_instrument,
                            &self.theme,
                        ) {
                            match event {
                                crate::ui::instrument_editor::InstrumentEditEvent::SaveInstrument => {
                                    self.save_instrument_dialog();
                                }
                                crate::ui::instrument_editor::InstrumentEditEvent::LoadInstrument => {
                                    self.load_instrument_dialog();
                                }
                                other => self.handle_instrument_edit(other),
                            }
                        }
                    }
                }
                AppView::SendFx => {
                    crate::ui::sendfx_editor::draw_sendfx_view(
                        ui,
                        &mut self.command_sender,
                        &mut self.send_bus_params,
                    );
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

        if self.file_browser.show {
            let mut file_browser_open = true;
            egui::Window::new("File Browser")
                .open(&mut file_browser_open)
                .default_size([600.0, 400.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(true)
                .show(ctx, |ui| {
                    if let Some(path) = self.file_browser.render(ui) {
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
