use eframe::egui;

use super::style::{FONT_BODY, FONT_CAPTION, FONT_SECTION};
use super::theme::TrackerTheme;

/// One channel strip's UI state. The mixer view's `ui()` function is
/// the only place that touches this. Channel volume / panning are
/// stored in the module; this struct just keeps per-channel UI scratch
/// state like the in-flight panning value while the user is dragging
/// a slider (so the audio engine doesn't see a value change on every
/// micro-frame during the drag).
pub struct MixerState {
    /// The first channel index currently scrolled into view.
    pub scroll_channel: usize,
    /// Last number of channels the layout fit on screen. Used to keep
    /// the visible-count in sync with the current window width so the
    /// user doesn't get bumped around on resize.
    pub last_visible_channels: usize,
    /// Whether the user has the "show all channels" toggle on. When
    /// off, the mixer only shows the channels that have a non-empty
    /// cell anywhere in the current pattern (a quick way to focus on
    /// what's actually in use).
    pub show_all: bool,
}

impl Default for MixerState {
    fn default() -> Self {
        MixerState {
            scroll_channel: 0,
            last_visible_channels: 8,
            show_all: true,
        }
    }
}

impl MixerState {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        core: &mut crate::core::HtrkCore,
        theme: &TrackerTheme,
        send_bus_plugins: &[Option<crate::sequencer::plugin::PluginSlot>; 4],
        send_bus_return_levels: &[f32; 4],
    ) {
        // Top toolbar
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("MIXER").size(FONT_SECTION).strong().color(theme.fg_instrument));
            ui.separator();
            ui.checkbox(&mut self.show_all, "Show all channels")
                .on_hover_text("When off, only channels that contain at least one cell in the current pattern are shown.");
            ui.separator();
            if ui.small_button("Reset").clicked() {
                core.with_module_mut(|arc_module, core| {
                    for v in arc_module.channel_volume.iter_mut() { *v = 64; }
                    for p in arc_module.channel_panning.iter_mut() { *p = 128; }
                    for sl in core.send_levels.iter_mut() { *sl = [0.0; 4]; }
                });
            }
            ui.label(egui::RichText::new("(vol=64, pan=128, sends=0)").color(theme.fg_dim).size(FONT_CAPTION));
        });
        ui.add_space(4.0);

        let num_channels = core.num_channels();
        if num_channels == 0 {
            ui.label(egui::RichText::new("No module loaded. Press Ctrl+N to start a new song.").color(theme.fg_dim));
            return;
        }

            egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    // 1. Channel strips
                    for ch in 0..num_channels {
                        if !self.show_all && !channel_in_use(core, ch) {
                            continue;
                        }
                        draw_channel_strip(ui, core, ch, theme);
                    }
                    // 2. Send bus strips
                    for bus in 0..4 {
                        let plugin_name = send_bus_plugins[bus].as_ref().map(slot_display_name);
                        draw_send_bus_strip(ui, core, bus, plugin_name.as_deref(), send_bus_return_levels[bus], theme);
                    }
                    // 3. Master strip
                    draw_master_strip(ui, core, theme);
                });
            });
    }
}

fn channel_in_use(core: &crate::core::HtrkCore, ch: usize) -> bool {
    let Some(module) = core.module.as_ref() else { return false; };
    let pat_idx = module.order_list.get(core.selected_order).copied().unwrap_or(0) as usize;
    let Some(pattern) = module.patterns.get(pat_idx) else { return false; };
    pattern.data[..pattern.num_rows].iter().any(|row| {
        let cell = &row[ch];
        cell.note != crate::sequencer::Note::None
            || cell.instrument.is_some()
            || cell.volume.is_some()
            || cell.effect != crate::sequencer::effect::Effect::None
    })
}

fn draw_channel_strip(ui: &mut egui::Ui, core: &mut crate::core::HtrkCore, ch: usize, theme: &TrackerTheme) {
    let (vol, pan, muted, soloed) = {
        let module = core.module.as_ref().unwrap();
        let vol = module.channel_volume.get(ch).copied().unwrap_or(64);
        let pan = module.channel_panning.get(ch).copied().unwrap_or(128);
        (vol, pan, core.muted_channels[ch], core.solo_channels[ch])
    };
    let send_levels = core.send_levels.get(ch).copied().unwrap_or([0.0; 4]);

    egui::Frame::NONE
        .fill(theme.status_bg)
        .stroke(egui::Stroke::new(1.0, theme.channel_header_bg))
        .corner_radius(egui::CornerRadius::same(2))
        .inner_margin(egui::Margin::symmetric(4, 4))
        .show(ui, |ui| {
            ui.set_width(72.0);
            ui.vertical(|ui| {
                // Channel name + index
                ui.horizontal(|ui| {
                    let muted_color = if muted { theme.channel_muted } else { theme.fg_dim };
                    ui.label(egui::RichText::new(format!("Ch{:02}", ch + 1)).strong().color(theme.fg_text));
                    if muted { ui.label(egui::RichText::new("(M)").color(muted_color)); }
                    if soloed { ui.label(egui::RichText::new("(S)").color(theme.channel_solo)); }
                });

                // Mute / Solo
                ui.horizontal(|ui| {
                    let mute_label = if muted { "Muted" } else { "Mute" };
                    let solo_label = if soloed { "Soloed" } else { "Solo" };
                    if ui.small_button(mute_label).clicked() {
                        core.toggle_mute(ch);
                    }
                    if ui.small_button(solo_label).clicked() {
                        core.toggle_solo(ch);
                    }
                });
                ui.add_space(2.0);

                // Volume
                ui.label(egui::RichText::new("Vol").size(FONT_CAPTION).color(theme.fg_volume));
                let mut new_vol = vol as f32;
                if ui.add(egui::Slider::new(&mut new_vol, 0.0..=64.0)
                    .step_by(1.0)
                    .show_value(true)
                    .clamping(egui::SliderClamping::Always))
                    .on_hover_text("Channel volume (0-64). 0 = silent, 64 = full.")
                    .changed()
                {
                    let new_vol = new_vol.round() as u8;
                    core.with_module_mut(|arc_module, _core| {
                        if let Some(v) = arc_module.channel_volume.get_mut(ch) {
                            *v = new_vol;
                        }
                    });
                }
                ui.add_space(2.0);

                // Pan
                ui.label(egui::RichText::new("Pan").size(FONT_CAPTION).color(theme.fg_instrument));
                let mut new_pan = pan as f32;
                if ui.add(egui::Slider::new(&mut new_pan, 0.0..=255.0)
                    .step_by(1.0)
                    .show_value(true)
                    .clamping(egui::SliderClamping::Always))
                    .on_hover_text("Channel panning (0-255). 0 = full left, 128 = center, 255 = full right.")
                    .changed()
                {
                    let new_pan = new_pan.round() as u8;
                    core.with_module_mut(|arc_module, _core| {
                        if let Some(p) = arc_module.channel_panning.get_mut(ch) {
                            *p = new_pan;
                        }
                    });
                }
                ui.add_space(2.0);

                ui.separator();
                ui.label(egui::RichText::new("Sends").size(FONT_CAPTION).color(theme.fg_dim));
                for bus in 0..4 {
                    let mut lvl = send_levels[bus];
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&"ABCD".chars().nth(bus).unwrap().to_string()).size(FONT_CAPTION).color(theme.fg_dim));
                        if ui.add(egui::Slider::new(&mut lvl, 0.0..=1.0)
                            .step_by(0.01)
                            .show_value(false)
                            .clamping(egui::SliderClamping::Always))
                            .on_hover_text(format!("Send to bus {} (0.0 = dry, 1.0 = full wet)", "ABCD".chars().nth(bus).unwrap()))
                            .changed()
                        {
                            core.set_send_level(ch, bus, lvl);
                        }
                    });
                }
            });
        });
}

fn draw_send_bus_strip(ui: &mut egui::Ui, core: &mut crate::core::HtrkCore, bus: usize, plugin_name: Option<&str>, return_level: f32, theme: &TrackerTheme) {
    egui::Frame::NONE
        .fill(theme.status_bg)
        .stroke(egui::Stroke::new(1.0, theme.channel_header_bg))
        .corner_radius(egui::CornerRadius::same(2))
        .inner_margin(egui::Margin::symmetric(4, 4))
        .show(ui, |ui| {
            ui.set_width(110.0);
            ui.vertical(|ui| {
                let label = match bus { 0 => "FX A", 1 => "FX B", 2 => "FX C", _ => "FX D" };
                ui.label(egui::RichText::new(label).strong().color(theme.fg_instrument));
                ui.add_space(2.0);

                ui.label(egui::RichText::new("Plugin").size(FONT_CAPTION).color(theme.fg_dim));
                match plugin_name {
                    Some(name) => {
                        ui.label(egui::RichText::new(name).size(FONT_BODY).color(theme.fg_note));
                    }
                    None => {
                        ui.label(egui::RichText::new("(none)").size(FONT_BODY).color(theme.fg_dim));
                    }
                }
                ui.label(egui::RichText::new("Open F12 to load / edit").size(FONT_CAPTION).color(theme.fg_dim));
                ui.add_space(2.0);

                ui.separator();
                ui.label(egui::RichText::new("Return").size(FONT_CAPTION).color(theme.fg_dim));
                let mut lvl = return_level;
                if ui.add(egui::Slider::new(&mut lvl, 0.0..=1.0)
                    .step_by(0.01)
                    .show_value(true)
                    .clamping(egui::SliderClamping::Always))
                    .on_hover_text("Bus return level (0.0 = silent, 1.0 = full).")
                    .changed()
                {
                    core.with_module_mut(|arc_module, _core| {
                        arc_module.send_return_levels[bus] = lvl;
                    });
                }
            });
        });
}

fn draw_master_strip(ui: &mut egui::Ui, core: &mut crate::core::HtrkCore, theme: &TrackerTheme) {
    let master_vol = core
        .module
        .as_ref()
        .map(|m| m.initial_global_volume)
        .unwrap_or(64);
    egui::Frame::NONE
        .fill(theme.status_bg)
        .stroke(egui::Stroke::new(2.0, theme.fg_instrument))
        .corner_radius(egui::CornerRadius::same(2))
        .inner_margin(egui::Margin::symmetric(4, 4))
        .show(ui, |ui| {
            ui.set_width(90.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("MASTER").strong().color(theme.fg_instrument));
                ui.add_space(2.0);
                ui.label(egui::RichText::new("Volume").size(FONT_CAPTION).color(theme.fg_dim));
                let mut new_vol = master_vol as f32;
                if ui.add(egui::Slider::new(&mut new_vol, 0.0..=64.0)
                    .step_by(1.0)
                    .show_value(true)
                    .clamping(egui::SliderClamping::Always))
                    .on_hover_text("Master volume (0-64).")
                    .changed()
                {
                    let v = new_vol.round() as u8;
                    core.with_module_mut(|arc_module, _core| {
                        arc_module.initial_global_volume = v;
                    });
                }
                ui.add_space(2.0);
                ui.separator();
                ui.label(egui::RichText::new("Channels: ").size(FONT_CAPTION).color(theme.fg_dim));
                ui.label(egui::RichText::new(format!("{}", core.num_channels())).size(FONT_BODY).color(theme.fg_text));
            });
        });
}

/// Format a PluginSlot as a short, human-readable display name. Uses
/// the file stem of the .clap path (e.g. `C:\CLAP\Charlatan.clap` ->
/// `Charlatan`). Falls back to the plugin_id if the path is empty.
fn slot_display_name(slot: &crate::sequencer::plugin::PluginSlot) -> String {
    let path = std::path::Path::new(&slot.path);
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        return stem.to_string();
    }
    if !slot.plugin_id.is_empty() {
        return slot.plugin_id.clone();
    }
    "?".to_string()
}
