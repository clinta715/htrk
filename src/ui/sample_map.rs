use eframe::egui;
use crate::sequencer::Module;

pub enum SampleMapEvent {
    NoteClicked(u8),
    NoteDragged(u8),
    NoteCleared(u8),
}

pub fn draw_sample_map(
    ui: &mut egui::Ui,
    sample_map: &[u8; 120],
    selected_sample: u8,
    module: &Module,
) -> Option<SampleMapEvent> {
    let mut event = None;

    ui.vertical(|ui| {
        ui.heading("Sample Map");
        ui.label(format!("Mapping to sample: {:02X}", selected_sample));

        let available_width = ui.available_width();
        let cell_size = (available_width / 12.0).floor().min(30.0);
        let grid_width = cell_size * 12.0;
        let grid_height = cell_size * 10.0;

        let (rect, mut response) = ui.allocate_exact_size(
            egui::vec2(grid_width, grid_height),
            egui::Sense::click_and_drag()
        );

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

        let notes = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];

        let mut hovered_note: Option<usize> = None;
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            if rect.contains(pos) {
                let col = ((pos.x - rect.left()) / cell_size).floor() as usize;
                let row = ((pos.y - rect.top()) / cell_size).floor() as usize;
                if col < 12 && row < 10 {
                    hovered_note = Some(row * 12 + col);
                }
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

                let sample_idx = sample_map[note_idx as usize];

                let bg_color: egui::Color32 = if sample_idx == 0 {
                    ui.visuals().faint_bg_color
                } else {
                    let hue = (sample_idx as f32 * 0.1) % 1.0;
                    egui::ecolor::Hsva::new(hue, 0.5, 0.3, 1.0).into()
                };

                painter.rect_filled(cell_rect.shrink(1.0), 1.0, bg_color);

                let text = format!("{}{}", notes[note_in_octave as usize], octave);
                painter.text(
                    cell_rect.center() - egui::vec2(0.0, cell_size * 0.2),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::monospace(cell_size * 0.3),
                    ui.visuals().text_color()
                );

                if sample_idx > 0 {
                    painter.text(
                        cell_rect.center() + egui::vec2(0.0, cell_size * 0.2),
                        egui::Align2::CENTER_CENTER,
                        format!("{:02X}", sample_idx),
                        egui::FontId::monospace(cell_size * 0.4),
                        egui::Color32::WHITE
                    );
                }

                if response.clicked() && cell_rect.contains(response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO)) {
                    if ui.input(|i| i.pointer.secondary_clicked()) {
                        event = Some(SampleMapEvent::NoteCleared(note_idx as u8));
                    } else {
                        event = Some(SampleMapEvent::NoteClicked(note_idx as u8));
                    }
                }
                if response.dragged() && cell_rect.contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or(egui::Pos2::ZERO)) {
                    event = Some(SampleMapEvent::NoteDragged(note_idx as u8));
                }
            }
        }

        if let Some(note_idx) = hovered_note {
            let sample_idx = sample_map[note_idx];
            let tooltip_text = if sample_idx == 0 {
                let notes_name = format!("{}{}", notes[note_idx % 12], note_idx / 12);
                format!("{}: (no sample)", notes_name)
            } else {
                let sname = module.samples.get(sample_idx as usize)
                    .map(|s| if s.name.is_empty() { "(unnamed)".to_string() } else { s.name.clone() })
                    .unwrap_or_else(|| "(invalid)".to_string());
                let notes_name = format!("{}{}", notes[note_idx % 12], note_idx / 12);
                format!("{}: Sample {:02X} - {}", notes_name, sample_idx, sname)
            };
            response = response.on_hover_text(tooltip_text);
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
