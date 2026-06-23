use eframe::egui;
use crate::ui::style::{FONT_BODY, FONT_CAPTION};
use crate::sequencer::note::Note;

fn note_name(key: u8) -> String {
    Note::On(key).to_string()
}

pub struct NoteMapEvent {
    pub note: u8,
    pub new_dest: u8,
}

pub fn draw_note_map(
    ui: &mut egui::Ui,
    note_map: &[u8; 120],
    _selected_sample: u8,
    theme: &crate::ui::TrackerTheme,
    cell_size: f32,
) -> Option<NoteMapEvent> {
    let mut event = None;

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Note Transpose Map:").size(FONT_BODY).color(ui.visuals().text_color().gamma_multiply(0.7)));
            ui.label(egui::RichText::new("Left-click drag to transpose, Right-click to reset").size(FONT_CAPTION).color(theme.fg_dim));
        });
        ui.add_space(2.0);

        let grid_width = cell_size * 12.0;
        let grid_height = cell_size * 10.0;

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(grid_width, grid_height),
            egui::Sense::click_and_drag()
        );

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

        let notes = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];

        let mut current_hovered_note: Option<usize> = None;
        if let Some(pos) = response.hover_pos() {
            let col = ((pos.x - rect.left()) / cell_size).floor() as usize;
            let row = ((pos.y - rect.top()) / cell_size).floor() as usize;
            if col < 12 && row < 10 {
                current_hovered_note = Some(row * 12 + col);
            }
        }

        for octave in 0..10 {
            for note_in_octave in 0..12 {
                let note_idx = octave * 12 + note_in_octave;
                let x = rect.left() + note_in_octave as f32 * cell_size;
                let y = rect.top() + octave as f32 * cell_size;
                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(cell_size, cell_size)
                );

                let dest_note = note_map[note_idx as usize];
                let is_identity = dest_note == note_idx as u8;

                let is_hovered = current_hovered_note == Some(note_idx);
                let mut bg_color: egui::Color32 = if is_identity {
                    ui.visuals().faint_bg_color
                } else {
                    theme.bg_selected.gamma_multiply(0.8)
                };

                if is_hovered {
                    let mut hsva = egui::ecolor::Hsva::from(bg_color);
                    hsva.v += 0.1;
                    bg_color = hsva.into();
                }

                painter.rect_filled(cell_rect.shrink(1.0), 1.0, bg_color);
                if is_hovered {
                    painter.rect_stroke(cell_rect.shrink(1.0), 1.0, egui::Stroke::new(1.0, ui.visuals().selection.stroke.color), egui::StrokeKind::Inside);
                }

                // Source note label (small)
                let src_text = format!("{}{}", notes[note_in_octave as usize], octave);
                painter.text(
                    cell_rect.left_top() + egui::vec2(2.0, 2.0),
                    egui::Align2::LEFT_TOP,
                    src_text,
                    egui::FontId::monospace(cell_size * 0.22),
                    ui.visuals().text_color().gamma_multiply(0.5)
                );

                // Dest note label (large)
                let dest_text = note_name(dest_note);
                painter.text(
                    cell_rect.center() + egui::vec2(0.0, 2.0),
                    egui::Align2::CENTER_CENTER,
                    dest_text,
                    egui::FontId::monospace(cell_size * 0.4),
                    if is_identity { ui.visuals().text_color() } else { theme.fg_note }
                );
            }
        }

        if let Some(note_idx) = current_hovered_note {
            let dest_note = note_map[note_idx];
            let tooltip_text = if dest_note == note_idx as u8 {
                format!("Note {}: (no transpose)", note_name(note_idx as u8))
            } else {
                format!("Note {}: Transposed to {}", note_name(note_idx as u8), note_name(dest_note))
            };
            response.clone().on_hover_text(tooltip_text);

            if response.clicked_by(egui::PointerButton::Primary) || response.dragged_by(egui::PointerButton::Primary) {
                // For transpose, let's keep it simple: primary click cycles +1 semitone
                // unless we want a more sophisticated UI. 
                // Actually, let's make it so you can drag vertically to change transpose value?
                // For now, let's just cycle or use a fixed change to prove interaction works.
                let delta = if ui.input(|i| i.modifiers.shift) { 12 } else { 1 };
                let new_dest = (dest_note as i16 + delta as i16).rem_euclid(120) as u8;
                if new_dest != dest_note {
                    event = Some(NoteMapEvent { note: note_idx as u8, new_dest });
                }
            } else if response.clicked_by(egui::PointerButton::Secondary) || response.dragged_by(egui::PointerButton::Secondary) {
                if dest_note != note_idx as u8 {
                    event = Some(NoteMapEvent { note: note_idx as u8, new_dest: note_idx as u8 });
                }
            }
        }

        for i in 0..=12 {
            let x = rect.left() + i as f32 * cell_size;
            painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], ui.visuals().widgets.noninteractive.bg_stroke);
        }
        for i in 0..=10 {
            let y = rect.top() + i as f32 * cell_size;
            painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], ui.visuals().widgets.noninteractive.bg_stroke);
        }
    });

    event
}
