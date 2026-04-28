use eframe::egui;
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
) -> Option<NoteMapEvent> {
    let mut event = None;

    ui.vertical(|ui| {
        ui.heading("Note Map (key -> remap)");
        ui.label(
            egui::RichText::new("Click a cell to cycle destination note. Cells that differ from identity are highlighted.")
                .size(10.0)
                .color(egui::Color32::GRAY),
        );

        let cell_size = 20.0;
        let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.vertical(|ui| {
                // Header row
                ui.horizontal(|ui| {
                    ui.add_sized([30.0, cell_size], egui::Label::new(""));
                    for name in &note_names {
                        ui.add_sized([cell_size, cell_size], egui::Label::new(
                            egui::RichText::new(*name).size(10.0).monospace()
                        ));
                    }
                });

                for octave in 0..10 {
                    ui.horizontal(|ui| {
                        ui.add_sized([30.0, cell_size], egui::Label::new(
                            egui::RichText::new(format!("{}:", octave)).size(10.0).monospace()
                        ));
                        for semitone in 0..12 {
                            let key = (octave * 12 + semitone) as u8;
                            let dest = note_map[key as usize];
                            let is_identity = dest == key;

                            let label = note_name(dest);
                            let bg = if !is_identity {
                                egui::Color32::from_rgb(40, 60, 100)
                            } else if key % 2 == 0 {
                                egui::Color32::from_rgb(24, 24, 32)
                            } else {
                                egui::Color32::from_rgb(20, 20, 28)
                            };

                            let resp = ui.add_sized(
                                [cell_size, cell_size],
                                egui::Label::new(
                                    egui::RichText::new(label)
                                        .size(9.0)
                                        .monospace()
                                        .color(egui::Color32::from_rgb(200, 200, 220))
                                        .background_color(bg),
                                ).sense(egui::Sense::click()),
                            );

                            if resp.clicked() {
                                // Cycle destination note upward by 1 semitone
                                let new_dest = if dest >= 119 { 0 } else { dest + 1 };
                                event = Some(NoteMapEvent { note: key, new_dest });
                            }
                            if resp.secondary_clicked() {
                                // Reset to identity
                                if dest != key {
                                    event = Some(NoteMapEvent { note: key, new_dest: key });
                                }
                            }
                        }
                    });
                }
            });
        });
    });

    event
}
