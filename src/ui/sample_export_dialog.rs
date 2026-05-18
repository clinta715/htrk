use eframe::egui;
use std::path::PathBuf;

pub struct SampleExportDialog {
    pub sample_index: usize,
    pub sample_name: String,
    pub file_path: Option<PathBuf>,
    pub bit_depth: u8,
    pub sample_rate: u32,
    pub error_message: Option<String>,
}

impl SampleExportDialog {
    pub fn new(sample_index: usize, sample_name: String, sample_rate: u32, default_dir: Option<&str>, default_bit_depth: u8) -> Self {
        let default_filename = crate::formats::wav::sanitize_filename(&sample_name, "sample");
        let mut default_path = PathBuf::new();
        if let Some(dir) = default_dir {
            let dir_path = PathBuf::from(dir);
            if dir_path.is_dir() {
                default_path = dir_path.join(&default_filename);
            }
        }
        if default_path.file_name().is_none() {
            default_path = PathBuf::from(format!("{}.wav", default_filename));
        }

        Self {
            sample_index,
            sample_name,
            file_path: Some(default_path),
            bit_depth: default_bit_depth,
            sample_rate: if sample_rate == 0 { 44100 } else { sample_rate },
            error_message: None,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<(PathBuf, u8)> {
        let mut result = None;
        let mut open = true;

        egui::Window::new(format!("Export Sample {:02X}", self.sample_index))
            .default_size([450.0, 180.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(420.0);

                if let Some(ref err) = self.error_message.take() {
                    ui.colored_label(egui::Color32::RED, err);
                    ui.add_space(5.0);
                }

                ui.label(format!("Sample: {} ({} Hz)", self.sample_name, self.sample_rate));
                ui.add_space(10.0);

                ui.label("File:");
                ui.horizontal(|ui| {
                    let path_str = self.file_path.as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| String::new());
                    let mut path_display = path_str.clone();
                    if ui.text_edit_singleline(&mut path_display).changed() {
                        if !path_display.is_empty() {
                            self.file_path = Some(PathBuf::from(path_display));
                        }
                    }
                    if ui.button("Browse...").clicked() {
                        let mut dialog = rfd::FileDialog::new()
                            .set_title("Save Sample")
                            .set_file_name(path_str.split(['/', '\\']).last().unwrap_or("sample.wav"));
                        if let Some(ref p) = self.file_path {
                            if let Some(parent) = p.parent() {
                                if parent.is_dir() {
                                    dialog = dialog.set_directory(parent);
                                }
                            }
                        }
                        dialog = dialog.add_filter("WAV Files", &["wav"]);
                        if let Some(path) = dialog.save_file() {
                            let final_path = if path.extension().map(|e| e != "wav").unwrap_or(true) {
                                path.with_extension("wav")
                            } else {
                                path
                            };
                            self.file_path = Some(final_path);
                        }
                    }
                });
                ui.add_space(10.0);

                ui.label("Bit Depth:");
                ui.horizontal(|ui| {
                    for depth in &[8u8, 16, 24, 32] {
                        let label = match depth {
                            8 => "8-bit (unsigned)",
                            16 => "16-bit",
                            24 => "24-bit",
                            32 => "32-bit float",
                            _ => unreachable!(),
                        };
                        if ui.selectable_label(self.bit_depth == *depth, label).clicked() {
                            self.bit_depth = *depth;
                        }
                    }
                });
                ui.add_space(15.0);

                ui.horizontal(|ui| {
                    if ui.button("Export").clicked() {
                        if let Some(ref path) = self.file_path {
                            result = Some((path.clone(), self.bit_depth));
                        } else {
                            self.error_message = Some("Please select a file path.".to_string());
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        result = None;
                    }
                });
            });

        result
    }
}