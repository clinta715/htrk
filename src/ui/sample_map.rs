use eframe::egui;

pub enum SampleMapEvent {
    NoteClicked(u8),
    NoteDragged(u8),
}

pub fn draw_sample_map(
    ui: &mut egui::Ui,
    sample_map: &[u8; 120],
    selected_sample: u8,
) -> Option<SampleMapEvent> {
    let mut event = None;
    
    ui.vertical(|ui| {
        ui.heading("Sample Map");
        ui.label(format!("Mapping to sample: {:02X}", selected_sample));

        let available_width = ui.available_width();
        let cell_size = (available_width / 12.0).floor().min(30.0);
        let grid_width = cell_size * 12.0;
        let grid_height = cell_size * 10.0;

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(grid_width, grid_height),
            egui::Sense::click_and_drag()
        );

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

        let notes = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];

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
                
                // Draw cell background
                let bg_color: egui::Color32 = if sample_idx == 0 {
                    ui.visuals().faint_bg_color
                } else {
                    let hue = (sample_idx as f32 * 0.1) % 1.0;
                    egui::ecolor::Hsva::new(hue, 0.5, 0.3, 1.0).into()
                };
                
                painter.rect_filled(cell_rect.shrink(1.0), 1.0, bg_color);
                
                // Draw note name
                let text = format!("{}{}", notes[note_in_octave as usize], octave);
                painter.text(
                    cell_rect.center() - egui::vec2(0.0, cell_size * 0.2),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::monospace(cell_size * 0.3),
                    ui.visuals().text_color()
                );

                // Draw sample index
                if sample_idx > 0 {
                    painter.text(
                        cell_rect.center() + egui::vec2(0.0, cell_size * 0.2),
                        egui::Align2::CENTER_CENTER,
                        format!("{:02X}", sample_idx),
                        egui::FontId::monospace(cell_size * 0.4),
                        egui::Color32::WHITE
                    );
                }

                // Handle interaction
                if response.clicked() && cell_rect.contains(response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO)) {
                    event = Some(SampleMapEvent::NoteClicked(note_idx as u8));
                }
                if response.dragged() && cell_rect.contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or(egui::Pos2::ZERO)) {
                    event = Some(SampleMapEvent::NoteDragged(note_idx as u8));
                }
            }
        }

        // Grid lines
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
