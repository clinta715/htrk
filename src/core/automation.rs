use std::sync::Arc;

use crate::sequencer::automation::{AutomationPoint, InterpolationMode};

use super::HtrkCore;

impl HtrkCore {
    pub fn handle_automation_interaction(&mut self, interaction: crate::ui::pattern_grid::AutomationInteraction) {
        self.ensure_module_ownership();
        match interaction {
            crate::ui::pattern_grid::AutomationInteraction::PointCreated { channel, order, row, value } => {
                if let Some(ref mut module) = self.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let target = match self.automation_targets.get(channel).copied().flatten() {
                            Some(t) => t,
                            None => return,
                        };
                        let track = arc_module.automation_tracks.iter_mut()
                            .find(|tr| tr.channel == Some(channel) && tr.target == target);
                        if let Some(track) = track {
                            track.insert_point(AutomationPoint {
                                order,
                                row,
                                value,
                                interp_to_next: track.default_interp,
                            });
                        }
                    }
                }
                self.sync_to_audio();
            }
            crate::ui::pattern_grid::AutomationInteraction::PointMoved { channel, order, row, value } => {
                if let Some(ref mut module) = self.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let target = match self.automation_targets.get(channel).copied().flatten() {
                            Some(t) => t,
                            None => return,
                        };
                        let track = arc_module.automation_tracks.iter_mut()
                            .find(|tr| tr.channel == Some(channel) && tr.target == target);
                        if let Some(track) = track {
                            track.insert_point(AutomationPoint {
                                order,
                                row,
                                value,
                                interp_to_next: track.default_interp,
                            });
                        }
                    }
                }
                self.sync_to_audio();
            }
            crate::ui::pattern_grid::AutomationInteraction::FreehandDraw { channel, points } => {
                if let Some(ref mut module) = self.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let target = match self.automation_targets.get(channel).copied().flatten() {
                            Some(t) => t,
                            None => return,
                        };
                        let track = arc_module.automation_tracks.iter_mut()
                            .find(|tr| tr.channel == Some(channel) && tr.target == target);
                        if let Some(track) = track {
                            for (order, row, value) in points {
                                track.insert_point(AutomationPoint {
                                    order,
                                    row,
                                    value,
                                    interp_to_next: InterpolationMode::Hold,
                                });
                            }
                        }
                    }
                }
                self.sync_to_audio();
            }
        }
    }

    pub fn enter_automation_hex(&mut self, channel: usize, row: usize, digit: u8) {
        let selected_order = self.selected_order as u16;
        let row_u16 = row as u16;
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let target = match self.automation_targets.get(channel).copied().flatten() {
                    Some(t) => t,
                    None => return,
                };
                let track = arc_module.automation_tracks.iter_mut()
                    .find(|tr| tr.channel == Some(channel) && tr.target == target);
                if let Some(track) = track {
                    let existing = track.points.iter().find(|p| p.order == selected_order && p.row == row_u16);
                    let value = match existing {
                        Some(p) => {
                            let old_byte = (p.value * 255.0).round() as u8;
                            (old_byte & 0xF0) | digit
                        },
                        None => digit,
                    };
                    track.insert_point(AutomationPoint {
                        order: selected_order,
                        row: row_u16,
                        value: value as f32 / 255.0,
                        interp_to_next: track.default_interp,
                    });
                }
            }
        }
        self.sync_to_audio();
    }

    pub fn delete_automation_point(&mut self, channel: usize, row: usize) {
        let selected_order = self.selected_order as u16;
        let row_u16 = row as u16;
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let target = match self.automation_targets.get(channel).copied().flatten() {
                    Some(t) => t,
                    None => return,
                };
                let track = arc_module.automation_tracks.iter_mut()
                    .find(|tr| tr.channel == Some(channel) && tr.target == target);
                if let Some(track) = track {
                    track.remove_point_at(selected_order, row_u16);
                }
            }
        }
        self.sync_to_audio();
    }
}