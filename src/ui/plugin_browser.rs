// Plugin browser dialog — list discovered CLAP plugins, select one, and
// load + activate it. The main thread does the dlopen and clack-host work;
// the audio processor is sent to the audio engine via AudioCommand::SetSendPlugin.

use eframe::egui;

use crate::audio::commands::AudioCommand;
use crate::audio::plugins::clap_plugin::ClapPluginHandle;
use crate::audio::plugins::{discovery, HostedPluginHandle, PluginDescriptor};
use crate::audio::CommandSender;
use crate::ui::style::FONT_BODY;
use crate::ui::theme::TrackerTheme;

/// Result of a plugin selection by the user. The dialog is closed after
/// returning. The caller is responsible for loading the plugin and sending
/// the processor to the audio engine.
pub enum PluginSelectResult {
    /// User picked a plugin descriptor. Caller should:
    /// 1. `ClapPluginHandle::load(descriptor.path)` — main thread, blocking
    /// 2. `handle.activate(sample_rate, max_block)` — main thread
    /// 3. `SetSendPlugin { send_index, processor }` — send to audio thread
    Selected { descriptor: PluginDescriptor, send_index: usize },
    Cancelled,
}

/// Draw the plugin browser dialog. Returns a result when the user closes
/// the dialog (either by selecting a plugin or cancelling).
pub fn draw_plugin_browser(
    ctx: &egui::Context,
    open: &mut bool,
    send_index: usize,
    bus_label: &str,
    _theme: &TrackerTheme,
    discovered: &[PluginDescriptor],
    status: &PluginBrowserStatus,
) -> PluginSelectResult {
    let mut result = PluginSelectResult::Cancelled;
    let mut local_open = *open;

    let title = format!("CLAP Plugin Browser — Send Bus {}", bus_label);
    let _ = egui::Window::new(&title)
        .id(egui::Id::new("plugin_browser"))
        .open(&mut local_open)
        .resizable(true)
        .default_size([480.0, 420.0])
        .min_width(360.0)
        .min_height(280.0)
        .show(ctx, |ui| {
            // Status / error display
            match status {
                PluginBrowserStatus::Idle => {
                    ui.label(egui::RichText::new(format!(
                        "{} plugin(s) discovered.", discovered.len()
                    )).size(FONT_BODY).weak());
                }
                PluginBrowserStatus::Loading(name) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(egui::RichText::new(format!("Loading {}...", name))
                            .size(FONT_BODY).strong());
                    });
                }
                PluginBrowserStatus::Error(msg) => {
                    ui.colored_label(egui::Color32::from_rgb(255, 100, 100), msg);
                }
                PluginBrowserStatus::Loaded(name) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 255, 100),
                        format!("Loaded: {}", name)
                    );
                }
            }

            ui.separator();

            if discovered.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(egui::RichText::new(
                        "No CLAP plugins found.\nAdd scan paths in Settings > Paths."
                    ).size(FONT_BODY).weak());
                });
            } else {
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for (i, d) in discovered.iter().enumerate() {
                            let is_busy = matches!(status, PluginBrowserStatus::Loading(_));
                            let label = if d.vendor.is_empty() || d.vendor == "Unknown" {
                                format!("{}. {} ({})", i + 1, d.name, d.plugin_id)
                            } else {
                                format!("{}. {} — {} ({})", i + 1, d.name, d.vendor, d.plugin_id)
                            };
                            let plugin_type = match d.plugin_type {
                                crate::audio::plugins::PluginType::Instrument => "Inst",
                                crate::audio::plugins::PluginType::Effect => "FX",
                                crate::audio::plugins::PluginType::Both => "Inst+FX",
                                crate::audio::plugins::PluginType::Analyzer => "Analyzer",
                            };
                            let resp = ui.add_enabled(!is_busy, egui::Button::new(
                                egui::RichText::new(&label).size(FONT_BODY)
                            ));
                            if resp.clicked() {
                                result = PluginSelectResult::Selected {
                                    descriptor: d.clone(),
                                    send_index,
                                };
                            }
                            ui.label(egui::RichText::new(format!(
                                "    type: {} | {} in / {} out | state: {}",
                                plugin_type, d.audio_inputs, d.audio_outputs, d.supports_state
                            )).size(FONT_BODY - 1.0).weak());
                        }
                    });
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Rescan").clicked() {
                    // Trigger a filesystem rescan (blocking but quick)
                    let _ = discovery::scan_default_paths();
                }
                if ui.button("Close").clicked() {
                    *open = false;
                }
            });
        });

    // Sync back the X-button state: if the user closed via the X, is_open is false
    if !local_open {
        *open = false;
    }

    // If the user picked a plugin, close the dialog too
    if matches!(result, PluginSelectResult::Selected { .. }) {
        *open = false;
    }

    result
}

/// Status of the plugin browser, displayed in the dialog.
#[derive(Clone, Debug, Default)]
pub enum PluginBrowserStatus {
    #[default]
    Idle,
    Loading(String),
    Error(String),
    Loaded(String),
}

// ── Plugin loading (main thread) ──

/// Load a CLAP plugin and send it to the audio engine on a send bus.
/// Returns the activated handle (main-thread side) and the plugin name.
/// The handle MUST be kept on the main thread for editor operations.
/// This is a blocking operation (typically <100ms for most plugins).
pub fn load_and_install_plugin(
    descriptor: &PluginDescriptor,
    send_index: usize,
    sample_rate: f64,
    max_block: u32,
    command_sender: &mut Option<CommandSender>,
) -> Result<(Box<dyn HostedPluginHandle>, String), String> {
    // Load and activate on the main thread
    let mut handle = ClapPluginHandle::load(&descriptor.path)
        .map_err(|e| format!("Load failed: {e}"))?;
    let processor = handle.activate(sample_rate, max_block)
        .map_err(|e| format!("Activate failed: {e}"))?;
    let name = processor.name().to_string();

    // Send the audio processor to the audio engine
    if let Some(ref mut sender) = command_sender {
        sender.send(AudioCommand::SetSendPlugin {
            send_index,
            processor: Some(processor),
        });
    } else {
        return Err("No command sender — audio engine not running?".into());
    }

    // Erase the concrete type to a trait object so the caller can store it
    // alongside other potential plugin formats in the future.
    let handle: Box<dyn HostedPluginHandle> = Box::new(handle);
    Ok((handle, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_browser_status_default() {
        let status = PluginBrowserStatus::default();
        assert!(matches!(status, PluginBrowserStatus::Idle));
    }
}
