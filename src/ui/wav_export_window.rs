use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitDepth {
    Bits8,
    Bits16,
    Bits24,
    Bits32,
    Bits32Float,
}

impl BitDepth {
    pub fn bits(&self) -> u16 {
        match self {
            BitDepth::Bits8 => 8,
            BitDepth::Bits16 => 16,
            BitDepth::Bits24 => 24,
            BitDepth::Bits32 => 32,
            BitDepth::Bits32Float => 32,
        }
    }

    pub fn is_float(&self) -> bool {
        matches!(self, BitDepth::Bits32Float)
    }

    pub fn label(&self) -> &'static str {
        match self {
            BitDepth::Bits8 => "8-bit",
            BitDepth::Bits16 => "16-bit",
            BitDepth::Bits24 => "24-bit",
            BitDepth::Bits32 => "32-bit (integer)",
            BitDepth::Bits32Float => "32-bit (float)",
        }
    }

    pub fn all() -> Vec<BitDepth> {
        vec![
            BitDepth::Bits8,
            BitDepth::Bits16,
            BitDepth::Bits24,
            BitDepth::Bits32,
            BitDepth::Bits32Float,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelMode {
    Mono,
    Stereo,
}

impl ChannelMode {
    pub fn channels(&self) -> u16 {
        match self {
            ChannelMode::Mono => 1,
            ChannelMode::Stereo => 2,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ChannelMode::Mono => "Mono",
            ChannelMode::Stereo => "Stereo",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioFormat {
    Wav,
    Aiff,
    Flac,
}

impl AudioFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            AudioFormat::Wav => "wav",
            AudioFormat::Aiff => "aiff",
            AudioFormat::Flac => "flac",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AudioFormat::Wav => "WAV",
            AudioFormat::Aiff => "AIFF",
            AudioFormat::Flac => "FLAC",
        }
    }

    pub fn all() -> Vec<AudioFormat> {
        vec![AudioFormat::Wav, AudioFormat::Aiff, AudioFormat::Flac]
    }
}

#[derive(Clone, Debug)]
pub struct WavExportSettings {
    pub file_path: Option<PathBuf>,
    pub bit_depth: BitDepth,
    pub sample_rate: u32,
    pub channel_mode: ChannelMode,
    pub format: AudioFormat,
    pub normalize: bool,
    pub dither: bool,
}

impl Default for WavExportSettings {
    fn default() -> Self {
        WavExportSettings {
            file_path: None,
            bit_depth: BitDepth::Bits16,
            sample_rate: 44100,
            channel_mode: ChannelMode::Stereo,
            format: AudioFormat::Wav,
            normalize: false,
            dither: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WavExportState {
    pub open: bool,
    settings: WavExportSettings,
    sample_rate_options: Vec<u32>,
    estimated_duration_secs: f64,
    estimated_file_size_mb: f64,
    module_loaded: bool,
    pub is_exporting: bool,
    pub export_progress: f32,
    pub export_status: String,
    pub export_cancelled: Arc<AtomicBool>,
    pub export_error: Option<String>,
    pub export_complete: bool,
    pub export_progress_atomic: Arc<AtomicU32>,
    pub export_state_atomic: Arc<AtomicU32>,
    pub default_directory: Option<PathBuf>,
}

impl WavExportState {
    pub fn new(sample_rate: u32) -> Self {
        WavExportState {
            open: false,
            settings: WavExportSettings {
                file_path: None,
                bit_depth: BitDepth::Bits16,
                sample_rate,
                channel_mode: ChannelMode::Stereo,
                format: AudioFormat::Wav,
                normalize: false,
                dither: false,
            },
            sample_rate_options: vec![22050, 44100, 48000, 88200, 96000, 192000],
            estimated_duration_secs: 0.0,
            estimated_file_size_mb: 0.0,
            module_loaded: false,
            is_exporting: false,
            export_progress: 0.0,
            export_status: String::new(),
            export_cancelled: Arc::new(AtomicBool::new(false)),
            export_error: None,
            export_complete: false,
            export_progress_atomic: Arc::new(AtomicU32::new(0)),
            export_state_atomic: Arc::new(AtomicU32::new(0)),
            default_directory: None,
        }
    }

    pub fn open(&mut self, default_name: &str, module_loaded: bool, total_samples: Option<u64>, sample_rate: u32) {
        self.open = true;
        self.module_loaded = module_loaded;
        
        let filename = format!("{}.{}", default_name, self.settings.format.extension());
        self.settings.file_path = Some(PathBuf::from(filename));
        self.settings.sample_rate = sample_rate;

        if let Some(total) = total_samples {
            let bytes_per_sample = (self.settings.bit_depth.bits() / 8) as f64;
            let channels = self.settings.channel_mode.channels() as f64;
            let duration = total as f64 / sample_rate as f64;
            let bytes = duration * sample_rate as f64 * channels * bytes_per_sample;
            self.estimated_duration_secs = duration;
            self.estimated_file_size_mb = bytes / (1024.0 * 1024.0);
        } else {
            self.estimated_duration_secs = 0.0;
            self.estimated_file_size_mb = 0.0;
        }
    }

    pub fn settings(&self) -> &WavExportSettings {
        &self.settings
    }

    pub fn update_estimates(&mut self, total_samples: Option<u64>) {
        if let Some(total) = total_samples {
            let bytes_per_sample = (self.settings.bit_depth.bits() / 8) as f64;
            let channels = self.settings.channel_mode.channels() as f64;
            let duration = total as f64 / self.settings.sample_rate as f64;
            let bytes = duration * self.settings.sample_rate as f64 * channels * bytes_per_sample;
            self.estimated_duration_secs = duration;
            self.estimated_file_size_mb = bytes / (1024.0 * 1024.0);
        }
    }

    pub fn start_export(&mut self) {
        self.is_exporting = true;
        self.export_progress = 0.0;
        self.export_status = "Starting export...".to_string();
        self.export_cancelled.store(false, Ordering::SeqCst);
        self.export_error = None;
        self.export_complete = false;
        self.export_progress_atomic.store(0, Ordering::SeqCst);
        self.export_state_atomic.store(0, Ordering::SeqCst);
    }

    pub fn update_progress(&mut self, progress: f32, status: &str) {
        self.export_progress = progress;
        self.export_status = status.to_string();
    }

    pub fn finish_export(&mut self, success: bool, error: Option<String>) {
        self.is_exporting = false;
        self.export_progress = if success { 1.0 } else { self.export_progress };
        self.export_complete = true;
        if let Some(e) = error {
            self.export_error = Some(e);
        }
    }

    pub fn progress_arc(&self) -> Arc<AtomicU32> {
        self.export_progress_atomic.clone()
    }

    pub fn state_arc(&self) -> Arc<AtomicU32> {
        self.export_state_atomic.clone()
    }

    pub fn cancel_arc(&self) -> Arc<AtomicBool> {
        self.export_cancelled.clone()
    }
}

pub fn draw_wav_export(
    ctx: &egui::Context,
    state: &mut WavExportState,
) -> bool {
    if !state.open {
        return false;
    }

    let mut exported = false;

    egui::Window::new("Export Audio")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.heading("Export Audio");
            ui.separator();

            if !state.module_loaded {
                ui.colored_label(egui::Color32::RED, "No module loaded to export");
                ui.end_row();
                if ui.button("Close").clicked() {
                    state.open = false;
                }
                return;
            }

            egui::Grid::new("export_grid")
                .num_columns(2)
                .spacing([10.0, 5.0])
                .show(ui, |ui| {
                    ui.label("File:");
                    if let Some(ref path) = state.settings.file_path {
                        ui.label(path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                    } else {
                        ui.label("(no file selected)");
                    }
                    ui.end_row();

                    ui.label("");
                    if ui.button("Browse...").clicked() {
                        let mut dialog = rfd::FileDialog::new()
                            .set_title("Save Audio File")
                            .set_file_name(state.settings.file_path.as_ref().and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string())).unwrap_or_default())
                            .add_filter("WAV Files", &["wav"])
                            .add_filter("AIFF Files", &["aiff", "aif"])
                            .add_filter("FLAC Files", &["flac"]);
                        if let Some(ref dir) = state.default_directory {
                            dialog = dialog.set_directory(dir);
                        }
                        if let Some(path) = dialog.save_file() {
                            state.settings.file_path = Some(path);
                        }
                    }
                    ui.end_row();

                    ui.label("Format:");
                    egui::ComboBox::new("format", "")
                        .selected_text(state.settings.format.label())
                        .show_ui(ui, |ui| {
                            for fmt in AudioFormat::all() {
                                ui.selectable_value(&mut state.settings.format, fmt, fmt.label());
                            }
                        });
                    ui.end_row();

                    ui.label("Sample Rate:");
                    egui::ComboBox::new("sample_rate", "")
                        .selected_text(format!("{} Hz", state.settings.sample_rate))
                        .show_ui(ui, |ui| {
                            for rate in &state.sample_rate_options {
                                ui.selectable_value(&mut state.settings.sample_rate, *rate, format!("{} Hz", rate));
                            }
                            ui.selectable_value(&mut state.settings.sample_rate, 0, "Use device rate");
                        });
                    ui.end_row();

                    ui.label("Bit Depth:");
                    egui::ComboBox::new("bit_depth", "")
                        .selected_text(state.settings.bit_depth.label())
                        .show_ui(ui, |ui| {
                            for depth in BitDepth::all() {
                                ui.selectable_value(&mut state.settings.bit_depth, depth, depth.label());
                            }
                        });
                    ui.end_row();

                    ui.label("Channels:");
                    egui::ComboBox::new("channels", "")
                        .selected_text(state.settings.channel_mode.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.settings.channel_mode, ChannelMode::Mono, "Mono");
                            ui.selectable_value(&mut state.settings.channel_mode, ChannelMode::Stereo, "Stereo");
                        });
                    ui.end_row();

                    ui.label("");
                    ui.checkbox(&mut state.settings.normalize, "Normalize output");
                    ui.end_row();

                    ui.label("");
                    ui.checkbox(&mut state.settings.dither, "Apply dithering");
                    ui.end_row();
                });

            ui.separator();

            if state.estimated_duration_secs > 0.0 && !state.is_exporting {
                let minutes = (state.estimated_duration_secs / 60.0).floor() as u32;
                let seconds = (state.estimated_duration_secs % 60.0).floor() as u32;
                ui.label(format!(
                    "Estimated: {:02}:{:02}, {:.1} MB",
                    minutes, seconds, state.estimated_file_size_mb
                ));
            }

            if state.is_exporting || state.export_complete {
                ui.separator();
                ui.label(&state.export_status);
                if state.is_exporting {
                    let progress_bar = egui::ProgressBar::new(state.export_progress)
                        .show_percentage();
                    ui.add(progress_bar);
                }
                if let Some(ref error) = state.export_error {
                    ui.colored_label(egui::Color32::RED, error);
                }
            }

            ui.separator();

            ui.horizontal(|ui| {
                let spacer = ui.available_width() - 160.0;
                ui.add_space(spacer);

                if state.is_exporting {
                    if ui.button("Cancel Export").clicked() {
                        state.export_cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    if ui.button("Close").clicked() {
                        state.open = false;
                    }
                } else {
                    if ui.button("Cancel").clicked() {
                        state.open = false;
                    }

                    if ui
                        .add_enabled(state.settings.file_path.is_some() && !state.is_exporting, egui::Button::new("Export"))
                        .clicked()
                    {
                        exported = true;
                    }
                }
            });
        });

    exported
}