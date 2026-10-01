use crate::audio::plugins::ParamInfo;
use crate::sequencer::module::Module;
use crate::ui::automation_editor::AutomationEditorResponse;
use crate::ui::automation_editor::AutomationEditorState;
use crate::ui::theme::TrackerTheme;
use eframe::egui;

#[derive(Default)]
pub struct AutomationEditor {
    pub state: AutomationEditorState,
}

impl AutomationEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        module: &mut Module,
        theme: &TrackerTheme,
        get_send_bus_params: impl Fn(usize) -> Vec<ParamInfo>,
        get_instrument_params: impl Fn(u8) -> Vec<ParamInfo>,
    ) -> AutomationEditorResponse {
        crate::ui::automation_editor::draw_automation_editor(
            ui,
            module,
            &mut self.state,
            theme,
            get_send_bus_params,
            get_instrument_params,
        )
    }
}
