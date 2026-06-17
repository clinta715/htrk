mod editing;
mod automation;
mod channels;

use std::sync::Arc;

use crate::audio::commands::AudioCommand;
use crate::audio::engine::CommandSender;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::edit::UndoManager;
use crate::sequencer::automation::AutomationTarget;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::sequencer::module::{Module, DEFAULT_CHANNELS};
use crate::sequencer::pattern::Cell;
use crate::ui::pattern_grid::{CursorPosition, Selection};

pub struct HtrkCore {
    pub(crate) module: Option<Arc<Module>>,
    pub(crate) loaded_module_name: String,
    pub(crate) file_path: Option<String>,

    pub(crate) undo_manager: UndoManager,
    pub(crate) clipboard: Option<Vec<Vec<Cell>>>,
    pub(crate) clipboard_width: usize,
    pub(crate) last_entered_cell: Option<Cell>,

    pub(crate) cursor: CursorPosition,
    pub(crate) selection: Option<Selection>,
    pub(crate) selection_anchor: Option<CursorPosition>,

    pub(crate) muted_channels: Vec<bool>,
    pub(crate) solo_channels: Vec<bool>,
    pub(crate) send_levels: Vec<[f32; NUM_SEND_BUSES]>,

    pub(crate) selected_order: usize,
    pub(crate) selected_sample: usize,
    pub(crate) selected_instrument: usize,

    pub(crate) automation_targets: Vec<Option<AutomationTarget>>,

    pub(crate) command_sender: Option<CommandSender>,
    pub(crate) playback_state: Arc<AtomicPlaybackState>,

    pub(crate) module_dirty: bool,
    pub(crate) last_backup_time: std::time::Instant,
}

impl HtrkCore {
    pub fn new(playback_state: Arc<AtomicPlaybackState>) -> Self {
        HtrkCore {
            module: None,
            loaded_module_name: String::new(),
            file_path: None,

            undo_manager: UndoManager::default(),
            clipboard: None,
            clipboard_width: 0,
            last_entered_cell: None,

            cursor: CursorPosition::default(),
            selection: None,
            selection_anchor: None,

            muted_channels: vec![false; DEFAULT_CHANNELS],
            solo_channels: vec![false; DEFAULT_CHANNELS],
            send_levels: vec![[0.0f32; NUM_SEND_BUSES]; DEFAULT_CHANNELS],

            selected_order: 0,
            selected_sample: 1,
            selected_instrument: 1,

            automation_targets: vec![None; DEFAULT_CHANNELS],

            command_sender: None,
            playback_state,

            module_dirty: false,
            last_backup_time: std::time::Instant::now(),
        }
    }

    pub fn set_command_sender(&mut self, sender: Option<CommandSender>) {
        self.command_sender = sender;
    }

    pub fn module(&self) -> Option<&Module> {
        self.module.as_deref()
    }

    pub fn module_arc(&self) -> Option<&Arc<Module>> {
        self.module.as_ref()
    }

    pub fn loaded_module_name(&self) -> &str {
        &self.loaded_module_name
    }

    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    pub fn cursor(&self) -> CursorPosition {
        self.cursor
    }

    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    pub fn selection_anchor(&self) -> Option<CursorPosition> {
        self.selection_anchor
    }

    pub fn selected_order(&self) -> usize {
        self.selected_order
    }

    pub fn selected_sample(&self) -> usize {
        self.selected_sample
    }

    pub fn selected_instrument(&self) -> usize {
        self.selected_instrument
    }

    pub fn muted_channels(&self) -> &[bool] {
        &self.muted_channels
    }

    pub fn solo_channels(&self) -> &[bool] {
        &self.solo_channels
    }

    pub fn send_levels(&self) -> &[[f32; NUM_SEND_BUSES]] {
        &self.send_levels
    }

    pub fn undo_manager(&self) -> &UndoManager {
        &self.undo_manager
    }

    pub fn can_undo(&self) -> bool {
        self.undo_manager.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo_manager.can_redo()
    }

    pub fn module_dirty(&self) -> bool {
        self.module_dirty
    }

    pub fn set_module_dirty(&mut self, dirty: bool) {
        self.module_dirty = dirty;
    }

    pub fn last_backup_time(&self) -> std::time::Instant {
        self.last_backup_time
    }

    pub fn set_last_backup_time(&mut self, time: std::time::Instant) {
        self.last_backup_time = time;
    }

    pub fn clipboard(&self) -> Option<&Vec<Vec<Cell>>> {
        self.clipboard.as_ref()
    }

    pub fn command_sender(&self) -> &Option<CommandSender> {
        &self.command_sender
    }

    pub fn command_sender_mut(&mut self) -> &mut Option<CommandSender> {
        &mut self.command_sender
    }

    pub fn playback_state(&self) -> &Arc<AtomicPlaybackState> {
        &self.playback_state
    }

    pub(crate) fn send_command(&mut self, cmd: AudioCommand) {
        #[cfg(feature = "audio_debug")]
        crate::debug_log!("[CMD] {:?}", cmd);
        if let Some(ref mut sender) = self.command_sender {
            if !sender.send(cmd.clone()) {
                // Buffer full — retry with yield to let audio callback drain.
                // This prevents silent drops that cause UI/audio engine desync.
                for _ in 0..100 {
                    std::thread::yield_now();
                    if sender.send(cmd.clone()) {
                        return;
                    }
                }
                eprintln!("[WARN] Audio command buffer full, command dropped: {:?}", cmd);
            }
        }
    }

    pub(crate) fn ensure_module_ownership(&mut self) {
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

    pub fn sync_module_to_audio(&mut self) {
        if let Some(ref module) = self.module {
            self.send_command(AudioCommand::LoadModule(module.clone()));
            self.module_dirty = true;
        }
    }

    pub(crate) fn sync_channel_fields(&mut self) {
        let count = self.module.as_ref()
            .map(|m| m.channel_panning.len())
            .unwrap_or(DEFAULT_CHANNELS);
        self.send_levels.resize(count, [0.0; NUM_SEND_BUSES]);
        self.muted_channels.resize(count, false);
        self.solo_channels.resize(count, false);
        self.automation_targets.resize(count, None);
    }

    pub(crate) fn current_pattern(&self) -> Option<&crate::sequencer::Pattern> {
        let module = self.module.as_ref()?;
        let order = *module.order_list.get(self.selected_order)?;
        module.patterns.get(order as usize)
    }

    pub(crate) fn current_pattern_mut(&mut self) -> Option<&mut crate::sequencer::Pattern> {
        self.ensure_module_ownership();
        let module = Arc::get_mut(self.module.as_mut()?)?;
        let order = *module.order_list.get(self.selected_order)?;
        module.patterns.get_mut(order as usize)
    }

    pub(crate) fn num_channels(&self) -> usize {
        self.module.as_ref()
            .map(|m| m.channel_panning.len())
            .unwrap_or(DEFAULT_CHANNELS)
    }

    pub(crate) fn num_channels_checked(&self) -> usize {
        let n = self.num_channels();
        if n == 0 { 1 } else { n }
    }

    pub fn load_module(&mut self, module: Module, name: String, path: Option<String>) {
        let module = Arc::new(module);
        self.module = Some(module.clone());
        self.loaded_module_name = name;
        self.file_path = path;
        self.send_command(AudioCommand::Stop);
        self.send_command(AudioCommand::LoadModule(module));
        self.cursor = CursorPosition::default();
        self.selection = None;
        self.selected_order = 0;
        self.selected_sample = 1;
        self.selected_instrument = 1;
        self.sync_channel_fields();
        self.undo_manager.clear();
    }

    pub fn new_song(&mut self) {
        let mut module = Module::default();
        module.name = "Untitled".to_string();
        module.order_list = vec![0];
        module.patterns.push(crate::sequencer::Pattern::new(64));

        let name = module.name.clone();
        let module = Arc::new(module);
        self.module = Some(module.clone());
        self.loaded_module_name = name;
        self.file_path = None;
        self.send_command(AudioCommand::Stop);
        self.send_command(AudioCommand::LoadModule(module));
        self.cursor = CursorPosition::default();
        self.selection = None;
        self.selected_order = 0;
        self.selected_sample = 1;
        self.selected_instrument = 1;
        self.sync_channel_fields();
        self.undo_manager.clear();
    }

    pub fn save_file(&mut self, path: &str) -> bool {
        let module = match &self.module {
            Some(m) => m,
            None => return false,
        };
        let data = crate::formats::save_module(module);
        match std::fs::write(path, &data) {
            Ok(()) => {
                self.file_path = Some(path.to_string());
                self.module_dirty = false;
                self.last_backup_time = std::time::Instant::now();
                true
            }
            Err(e) => {
                eprintln!("Failed to save file: {}", e);
                false
            }
        }
    }

    pub fn import_wav_to_sample(&mut self, sample_idx: usize, sample: crate::sequencer::Sample) {
        self.ensure_module_ownership();
        if let Some(ref mut module_arc) = self.module {
            if let Some(m) = Arc::get_mut(module_arc) {
                if sample_idx >= m.samples.len() {
                    m.samples.resize(sample_idx + 1, crate::sequencer::Sample::default());
                }
                m.samples[sample_idx] = sample;
            }
        }
        self.sync_module_to_audio();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_headless_core() -> HtrkCore {
        let playback_state = Arc::new(AtomicPlaybackState::default());
        HtrkCore::new(playback_state)
    }

    #[test]
    fn headless_new_song() {
        let mut core = make_headless_core();
        assert!(core.module.is_none());

        core.new_song();
        assert!(core.module.is_some());
        assert_eq!(core.loaded_module_name, "Untitled");
        assert!(core.file_path.is_none());
        assert_eq!(core.selected_order, 0);
    }

    #[test]
    fn headless_edit_and_undo() {
        let mut core = make_headless_core();
        core.new_song();

        let cell = Cell {
            note: crate::sequencer::note::Note::On(60),
            instrument: Some(1),
            volume: Some(64),
            volume_effect: None,
            effect: crate::sequencer::effect::Effect::None,
        };
        core.set_cell_at_cursor(cell.clone(), &[], false);
        let retrieved = core.get_cell_at_cursor();
        assert_eq!(retrieved.note, crate::sequencer::note::Note::On(60));
        assert_eq!(retrieved.instrument, Some(1));

        core.undo();
        let after_undo = core.get_cell_at_cursor();
        assert_eq!(after_undo.note, crate::sequencer::note::Note::None);
    }

    #[test]
    fn headless_channel_mute_solo() {
        let mut core = make_headless_core();
        core.new_song();

        assert!(!core.muted_channels[0]);
        core.toggle_mute(0);
        assert!(core.muted_channels[0]);
        core.toggle_mute(0);
        assert!(!core.muted_channels[0]);

        assert!(!core.solo_channels[0]);
        core.toggle_solo(0);
        assert!(core.solo_channels[0]);
    }

    #[test]
    fn headless_selection_and_copy() {
        let mut core = make_headless_core();
        core.new_song();

        let cell = Cell {
            note: crate::sequencer::note::Note::On(48),
            instrument: Some(2),
            volume: None,
            volume_effect: None,
            effect: crate::sequencer::effect::Effect::None,
        };
        core.set_cell_at_cursor(cell, &[], false);

        let anchor = CursorPosition { row: 0, channel: 0, sub_column: crate::ui::pattern_grid::SubColumn::Note };
        let end = CursorPosition { row: 0, channel: 0, sub_column: crate::ui::pattern_grid::SubColumn::Note };
        core.selection = Some(Selection { start: anchor, end });
        core.copy_selection();
        assert!(core.clipboard.is_some());
    }
}