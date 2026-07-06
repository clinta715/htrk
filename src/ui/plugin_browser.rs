// Plugin browser dialog — list discovered CLAP plugins, select one, and
// load + activate it. The main thread does the dlopen and clack-host work;
// the audio processor is sent to the audio engine via AudioCommand::SetSendPlugin.

use eframe::egui;

use crate::audio::commands::AudioCommand;
use crate::audio::plugins::clap_plugin::ClapPluginHandle;
use crate::audio::plugins::{HostedPluginHandle, PluginDescriptor};
use crate::audio::CommandSender;
use crate::ui::style::{FONT_BODY, FONT_CAPTION};
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

/// Short type label shown in the browser (and folded into the filter
/// haystack). Kept as the single source of truth so the display and the
/// search never drift apart.
fn plugin_type_label(d: &PluginDescriptor) -> &'static str {
    match d.plugin_type {
        crate::audio::plugins::PluginType::Instrument => "Inst",
        crate::audio::plugins::PluginType::Effect => "FX",
        crate::audio::plugins::PluginType::Both => "Inst+FX",
        crate::audio::plugins::PluginType::Analyzer => "Analyzer",
    }
}

/// Does `descriptor` match the quicksearch filter? An empty filter matches
/// every plugin. Otherwise the filter is matched case-insensitively against
/// the plugin's name, vendor, plugin_id, and type label (e.g. "fx",
/// "inst+fx"). The filter is lowercased internally so callers can pass the
/// raw text field contents.
fn plugin_matches_filter(descriptor: &PluginDescriptor, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let filter_lower = filter.to_lowercase();
    let haystack = format!(
        "{} {} {} {}",
        descriptor.name,
        descriptor.vendor,
        descriptor.plugin_id,
        plugin_type_label(descriptor),
    )
    .to_lowercase();
    haystack.contains(&filter_lower)
}

/// Draw the plugin browser dialog. Returns `(result, action)` — `result` is
/// the selection state (cancelled or selected), and `action` carries any
/// side-channel requests (e.g. rescan) for the caller to process.
pub fn draw_plugin_browser(
    ctx: &egui::Context,
    open: &mut bool,
    send_index: usize,
    bus_label: &str,
    theme: &TrackerTheme,
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

            // Quicksearch filter. Persists per-window via egui temp storage:
            // each browser instance (send bus A/B/C/D, each instrument) gets
            // its own filter because `make_persistent_id` is scoped to the
            // window Id. Matches name, vendor, plugin_id, and type label,
            // case-insensitively.
            let filter_id = ui.make_persistent_id("plugin_browser_filter");
            let mut filter = ui.data(|d| d.get_temp::<String>(filter_id).unwrap_or_default());

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Filter:").size(FONT_BODY).color(theme.fg_dim));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text("name, vendor, type...")
                        .desired_width(ui.available_width()),
                );
                if resp.changed() {
                    ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
                }
            });
            ui.add_space(2.0);

            let mut shown = 0usize;

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
                            let plugin_type = plugin_type_label(d);
                            if !plugin_matches_filter(d, &filter) {
                                continue;
                            }
                            shown += 1;

                            let is_busy = matches!(status, PluginBrowserStatus::Loading(_));
                            let label = if d.vendor.is_empty() || d.vendor == "Unknown" {
                                format!("{}. {} ({})", i + 1, d.name, d.plugin_id)
                            } else {
                                format!("{}. {} — {} ({})", i + 1, d.name, d.vendor, d.plugin_id)
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

                if shown == 0 && !filter.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new(format!(
                            "No plugins match \"{}\".", filter
                        )).size(FONT_BODY).weak());
                    });
                }
            }

            ui.separator();
            ui.horizontal(|ui| {
                if !discovered.is_empty() {
                    ui.label(egui::RichText::new(format!(
                        "{} of {} shown", shown, discovered.len()
                    )).size(FONT_CAPTION).color(theme.fg_dim));
                }
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

/// Lower-level form of the instrument plugin install: takes the
/// sample rate and command sender as parameters so the caller (the
/// auto-reload path) can borrow disjoint fields of `self` without
/// needing the whole `&mut self`. Pairs with `load_and_install_plugin`
/// (send-bus) to provide a uniform install surface for both
/// plugin locations.
pub fn load_and_install_instrument_plugin(
    descriptor: &PluginDescriptor,
    instrument_idx: usize,
    initial_state: Option<&[u8]>,
    sample_rate: f64,
    command_sender: &mut Option<CommandSender>,
) -> Result<(Box<dyn HostedPluginHandle>, String), String> {
    let (handle, processor, name) =
        load_and_activate_clap_plugin(descriptor, sample_rate, 512, initial_state)?;

    if let Some(ref mut sender) = command_sender {
        sender.send(AudioCommand::InstallInstrumentPlugin {
            instrument_idx,
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

/// Re-load every plugin slot listed in `slots` from the discovered
/// library. For each (index, format, path, state) entry, look up
/// the matching descriptor; if found, call `load_and_install` to
/// activate the plugin and apply its saved state; on success, hand
/// the result to `store_handle`. Slots whose path is no longer
/// discoverable are logged and skipped. Used by both the send-bus
/// and instrument auto-reload paths (`sync_send_bus_plugin_state`,
/// `sync_instrument_plugin_state`) so they share the slot
/// collection, descriptor lookup, error logging, and iteration
/// shape. The per-target differences (which command to send to
/// the audio engine, where the handle + UI label get stored) live
/// in the two closures.
pub fn sync_plugin_slots_from_module(
    slots: Vec<(usize, String, String, Vec<u8>)>,
    discovered: &[crate::audio::plugins::PluginDescriptor],
    location_label: &'static str,
    mut load_and_install: impl FnMut(
        usize,
        &crate::audio::plugins::PluginDescriptor,
        Option<&[u8]>,
    ) -> Result<(Box<dyn crate::audio::plugins::HostedPluginHandle>, String), String>,
    mut store_handle: impl FnMut(usize, Box<dyn crate::audio::plugins::HostedPluginHandle>, String),
) {
    if slots.is_empty() {
        return;
    }
    for (idx, format, path, state) in slots {
        let descriptor = discovered
            .iter()
            .find(|d| {
                d.format.as_str().eq_ignore_ascii_case(&format)
                    && d.path.to_string_lossy() == path.as_str()
            })
            .cloned();
        let descriptor = match descriptor {
            Some(d) => d,
            None => {
                eprintln!(
                    "[plugin] {location_label} {idx}: no discovered plugin at {path} (id={format})",
                );
                continue;
            }
        };
        let initial_state = if state.is_empty() { None } else { Some(&state[..]) };
        match load_and_install(idx, &descriptor, initial_state) {
            Ok((handle, name)) => store_handle(idx, handle, name),
            Err(e) => {
                eprintln!(
                    "[plugin] failed to auto-load {location_label} {idx} ({}): {e}",
                    descriptor.name,
                );
            }
        }
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

    use crate::audio::plugins::{PluginDescriptor, PluginFormat, PluginType};
    use std::path::PathBuf;

    fn desc(name: &str, vendor: &str, id: &str, ty: PluginType) -> PluginDescriptor {
        PluginDescriptor {
            format: PluginFormat::Clap,
            path: PathBuf::new(),
            plugin_id: id.into(),
            name: name.into(),
            vendor: vendor.into(),
            version: String::new(),
            description: String::new(),
            plugin_type: ty,
            audio_inputs: 2,
            audio_outputs: 2,
            has_editor: false,
            supports_state: false,
        }
    }

    #[test]
    fn filter_empty_matches_all() {
        let d = desc("Vital", "Matt Tytel", "vital", PluginType::Instrument);
        assert!(plugin_matches_filter(&d, ""));
    }

    #[test]
    fn filter_matches_name_case_insensitive() {
        let d = desc("TAL-Reverb-4", "TAL-Toge", "tal-reverb-4", PluginType::Effect);
        assert!(plugin_matches_filter(&d, "reverb"));
        assert!(plugin_matches_filter(&d, "TAL"));
        assert!(plugin_matches_filter(&d, "tal-reverb"));
        assert!(!plugin_matches_filter(&d, "chorus"));
    }

    #[test]
    fn filter_matches_vendor() {
        let d = desc("Dexed", "Digital Suburban", "dexed", PluginType::Instrument);
        assert!(plugin_matches_filter(&d, "suburban"));
        assert!(plugin_matches_filter(&d, "digital"));
    }

    #[test]
    fn filter_matches_plugin_id() {
        let d = desc("Surge XT", "Surge", "surge-xt", PluginType::Both);
        assert!(plugin_matches_filter(&d, "surge-xt"));
    }

    #[test]
    fn filter_matches_type_label() {
        let fx = desc("XeniaFX", "Airwindows", "xeniafx", PluginType::Effect);
        let inst = desc("Dexed", "Digital Suburban", "dexed", PluginType::Instrument);
        let both = desc("Surge XT", "Surge", "surge-xt", PluginType::Both);
        assert!(plugin_matches_filter(&fx, "fx"));
        assert!(!plugin_matches_filter(&fx, "inst"));
        assert!(plugin_matches_filter(&inst, "inst"));
        assert!(!plugin_matches_filter(&inst, "fx"));
        assert!(plugin_matches_filter(&both, "inst+fx"));
        assert!(plugin_matches_filter(&both, "fx"));
    }

    #[test]
    fn filter_no_match_returns_false() {
        let d = desc("Vital", "Matt Tytel", "vital", PluginType::Instrument);
        assert!(!plugin_matches_filter(&d, "reverb"));
        assert!(!plugin_matches_filter(&d, "xyzzy"));
    }
}
