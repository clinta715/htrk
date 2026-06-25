// Plugin browser dialog — list discovered CLAP plugins, select one, and
// load + activate it. The main thread does the dlopen and clack-host work;
// the audio processor is sent to the audio engine via AudioCommand::SetSendPlugin.

use eframe::egui;

use crate::audio::commands::AudioCommand;
use crate::audio::plugins::clap_plugin::ClapPluginHandle;
use crate::audio::plugins::{HostedPluginHandle, PluginDescriptor};
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

/// User actions emitted from the plugin browser dialog (e.g. Rescan button).
/// Returned alongside the selection result.
#[derive(Default)]
pub struct PluginBrowserAction {
    /// True if the user clicked "Rescan". Caller should run a rescan and
    /// update the discovered list (and the PluginLibrary).
    pub rescan_requested: bool,
}

impl PluginBrowserAction {
    pub fn none() -> Self {
        Self { rescan_requested: false }
    }
}

/// Draw the plugin browser dialog. Returns `(result, action)` — `result` is
/// the selection state (cancelled or selected), and `action` carries any
/// side-channel requests (e.g. rescan) for the caller to process.
pub fn draw_plugin_browser(
    ctx: &egui::Context,
    open: &mut bool,
    send_index: usize,
    bus_label: &str,
    _theme: &TrackerTheme,
    discovered: &[PluginDescriptor],
    status: &PluginBrowserStatus,
) -> (PluginSelectResult, PluginBrowserAction) {
    let mut result = PluginSelectResult::Cancelled;
    let mut action = PluginBrowserAction::none();
    let mut local_open = *open;

    let title = format!("CLAP Plugin Browser — {}", bus_label);
    let window_id = egui::Id::new(format!("plugin_browser_{}_{}", bus_label, send_index));
    let _ = egui::Window::new(&title)
        .id(window_id)
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
                    // Request a rescan from the caller. The caller will
                    // call `HtrkApp::rescan_plugins()` and pass the new
                    // discovered list on the next frame.
                    action.rescan_requested = true;
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

    (result, action)
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

/// Load and activate a CLAP plugin. Returns the main-thread handle
/// (caller keeps for editor operations) and the audio-thread processor
/// (caller sends to the audio engine via the appropriate command).
/// This is a blocking operation (typically <100ms for most plugins).
///
/// If `initial_state` is `Some(non-empty)`, it's applied via the plugin's
/// state-load extension before activation, so the plugin starts with the
/// user's saved patch. Pass `None` (or an empty slice) for a fresh
/// default-state load.
///
/// Shared by send-bus and instrument plugin loading. The caller decides
/// which `AudioCommand` variant to send (`SetSendPlugin` or
/// `InstallInstrumentPlugin`).
pub fn load_and_activate_clap_plugin(
    descriptor: &PluginDescriptor,
    sample_rate: f64,
    max_block: u32,
    initial_state: Option<&[u8]>,
) -> Result<
    (
        Box<dyn HostedPluginHandle>,
        Box<dyn crate::audio::plugins::HostedPluginProcessor>,
        String,
    ),
    String,
> {
    let mut handle = ClapPluginHandle::load(&descriptor.path)
        .map_err(|e| format!("Load failed: {e}"))?;

    if let Some(state) = initial_state {
        if !state.is_empty() {
            if let Err(e) = handle.load_state(state) {
                eprintln!("[plugin] state load failed: {e}");
            }
        }
    }

    let processor = handle.activate(sample_rate, max_block)
        .map_err(|e| format!("Activate failed: {e}"))?;
    let name = processor.name().to_string();

    let handle: Box<dyn HostedPluginHandle> = Box::new(handle);
    Ok((handle, processor, name))
}

/// Load a CLAP plugin and send it to the audio engine on a send bus.
/// Returns the activated handle (main-thread side) and the plugin name.
/// The handle MUST be kept on the main thread for editor operations.
pub fn load_and_install_plugin(
    descriptor: &PluginDescriptor,
    send_index: usize,
    sample_rate: f64,
    max_block: u32,
    command_sender: &mut Option<CommandSender>,
    initial_state: Option<&[u8]>,
) -> Result<(Box<dyn HostedPluginHandle>, String), String> {
    let (handle, processor, name) =
        load_and_activate_clap_plugin(descriptor, sample_rate, max_block, initial_state)?;

    if let Some(ref mut sender) = command_sender {
        sender.send(AudioCommand::SetSendPlugin {
            send_index,
            processor: Some(processor),
        });
    } else {
        return Err("No command sender — audio engine not running?".into());
    }

    Ok((handle, name))
}

/// Iterate `handles`, capture each one's state, and deliver the
/// `(index, state)` pair to `on_state`. The handle is temporarily
/// removed from its slot to satisfy `save_state`'s `&mut self`
/// signature, then put back. Returns nothing; the caller decides
/// what to do with each state blob (typically write to a module
/// slot). Used by both send-bus and instrument save-all flows.
pub fn save_all_plugin_states(
    handles: &mut [Option<Box<dyn HostedPluginHandle>>],
    mut on_state: impl FnMut(usize, Vec<u8>),
) {
    for i in 0..handles.len() {
        if let Some(mut handle) = handles[i].take() {
            if let Ok(state) = handle.save_state() {
                on_state(i, state);
            }
            handles[i] = Some(handle);
        }
    }
}

/// Write `state` into the module's plugin slot via the caller-
/// provided `slot_for` accessor. No-op if the slot is missing. Used
/// by both send-bus and instrument write-state flows. The closure
/// receives the `&mut Module` and is responsible for navigating
/// to the right slot (instrument index vs send-bus index).
pub fn write_plugin_state_to_slot(
    module: &mut crate::sequencer::Module,
    slot_for: impl FnOnce(&mut crate::sequencer::Module) -> Option<&mut crate::sequencer::plugin::PluginSlot>,
    state: Vec<u8>,
) {
    if let Some(slot) = slot_for(module) {
        slot.state = state;
    }
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
