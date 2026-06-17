use crate::sequencer::automation::{AutomationPoint, AutomationTarget, InterpolationMode};
use crate::ui::pattern_grid::{AutomationInteraction, ContextMenuAction};
use crate::ui::sample_editor::SampleEditEvent;

#[derive(Debug, Clone)]
pub enum PanelEvent {
    // Pattern view
    AddChannel,
    RemoveChannel,
    SetAutomationTarget { channel: usize, target: AutomationTarget },
    ContextMenuAction(ContextMenuAction),
    AutomationInteraction(AutomationInteraction),
    ToggleSampleLengthBg,

    // Automation editor
    AutomationTrackAdded {
        target: AutomationTarget,
        channel: Option<usize>,
    },
    AutomationTrackRemoved { track_id: u32 },
    AutomationTrackToggled { track_id: u32 },
    AutomationPointChanged {
        track_id: u32,
        point: AutomationPoint,
    },
    AutomationPointRemoved { track_id: u32, order: u16, row: u8 },
    AutomationInterpChanged { track_id: u32, mode: InterpolationMode },

    // Sample editor
    SampleEdit(SampleEditEvent),

    // Catch-all for module sync
    SyncToAudio,
}
