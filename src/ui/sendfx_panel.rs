use eframe::egui;
use crate::audio::CommandSender;
use crate::sequencer::effect::{NUM_SEND_BUSES, SendEffectType};

pub struct SendFxPanel {
    pub effect_types: [SendEffectType; NUM_SEND_BUSES],
    pub params: [[f32; 5]; NUM_SEND_BUSES],
    pub pre_fader: [bool; NUM_SEND_BUSES],
    /// Names of currently loaded CLAP plugins per send bus (None = no plugin).
    /// Updated by the UI when a plugin is loaded or removed.
    pub plugin_names: [Option<String>; NUM_SEND_BUSES],
    /// Index of the send bus currently showing the plugin browser dialog
    /// (None = no dialog open).
    pub plugin_browser_open_for: Option<usize>,
}

impl Default for SendFxPanel {
    fn default() -> Self {
        SendFxPanel {
            effect_types: [
                SendEffectType::Delay,
                SendEffectType::Reverb,
                SendEffectType::None,
                SendEffectType::None,
            ],
            params: [
                [0.5, 1.0, 0.4, 0.3, 1.0],
                [0.0, 0.7, 0.5, 0.6, 0.5],
                [0.0, 0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 0.0, 0.0],
            ],
            pre_fader: [false; NUM_SEND_BUSES],
            plugin_names: [None, None, None, None],
            plugin_browser_open_for: None,
        }
    }
}

impl SendFxPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui, command_sender: &mut Option<CommandSender>) {
        crate::ui::sendfx_editor::draw_sendfx_view(
            ui,
            command_sender,
            &mut self.effect_types,
            &mut self.params,
            &mut self.pre_fader,
            &mut self.plugin_names,
            &mut self.plugin_browser_open_for,
        );
    }
}
