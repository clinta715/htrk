use eframe::egui;

use crate::sequencer::Module;

use super::theme::TrackerTheme;

pub struct OrderListResponse {
    pub selected_order: Option<usize>,
    pub insert_clicked: bool,
    pub delete_clicked: bool,
    pub duplicate_clicked: bool,
    pub pattern_changed: Option<(usize, u8)>,
    pub pattern_resized: Option<(usize, usize)>,
    pub order_reordered: Option<(usize, usize)>,
}

pub fn draw_order_list(
    ui: &mut egui::Ui,
    module: &Module,
    selected_order: usize,
    playback_order: Option<usize>,
    theme: &TrackerTheme,
) -> OrderListResponse {
    let mut resp = OrderListResponse {
        selected_order: None,
        insert_clicked: false,
        delete_clicked: false,
        duplicate_clicked: false,
        pattern_changed: None,
        pattern_resized: None,
        order_reordered: None,
    };

    ui.vertical(|ui| {
        ui.heading(
            egui::RichText::new("Song Order")
                .font(egui::FontId::proportional(12.0))
                .color(theme.order_fg),
        );

        ui.separator();

        let mut pattern_changed: Option<(usize, u8)> = None;

        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 80.0)
            .show(ui, |ui| {
                for (i, &pat_idx) in module.order_list.iter().enumerate() {
                    let is_selected = i == selected_order;
                    let is_playing = playback_order == Some(i);

                    let bg = if is_playing {
                        theme.order_playing
                    } else if is_selected {
                        theme.order_selected
                    } else {
                        theme.order_bg
                    };

                    let fg = if is_playing || is_selected {
                        theme.order_bg
                    } else {
                        theme.order_fg
                    };

                    let label_text = format!("{:03} -- {:03}", i, pat_idx);
                    let response = ui.add_sized(
                        [ui.available_width(), 16.0],
                        egui::Label::new(
                            egui::RichText::new(&label_text)
                                .font(egui::FontId::monospace(12.0))
                                .color(fg)
                                .background_color(bg),
                        )
                        .sense(egui::Sense::click() | egui::Sense::drag()),
                    );

                    if response.clicked() {
                        resp.selected_order = Some(i);
                    }

                    if response.drag_started() {
                        egui::DragAndDrop::set_payload(ui.ctx(), i as u32);
                    }

                    if egui::DragAndDrop::has_payload_of_type::<u32>(ui.ctx()) && response.hovered() {
                        ui.painter().rect(
                            response.rect,
                            egui::CornerRadius::default(),
                            egui::Color32::from_rgba_premultiplied(100, 150, 255, 80),
                            egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 150, 255)),
                            egui::StrokeKind::Outside,
                        );
                    }

                    if let Some(dragged_from) = egui::DragAndDrop::take_payload::<u32>(ui.ctx()).map(|arc| *arc) {
                        if (dragged_from as usize) != i {
                            resp.order_reordered = Some((dragged_from as usize, i));
                        }
                    }

                    if is_selected {
                        let mut edit_val = pat_idx as u32;
                        ui.horizontal(|ui| {
                            ui.add_space(32.0);
                            ui.label(egui::RichText::new("--").font(egui::FontId::monospace(12.0)).color(theme.order_fg));
                            let drag = egui::DragValue::new(&mut edit_val)
                                .range(0..=255)
                                .speed(1.0);
                            let drag_resp = ui.add(drag);
                            if drag_resp.changed() {
                                pattern_changed = Some((i, edit_val as u8));
                            }
                        });

                        if let Some(pattern) = module.patterns.get(pat_idx as usize) {
                            let mut rows = pattern.num_rows as u32;
                            ui.horizontal(|ui| {
                                ui.add_space(32.0);
                                ui.label(egui::RichText::new("Rows:").font(egui::FontId::monospace(10.0)).color(theme.order_fg));
                                let drag = egui::DragValue::new(&mut rows)
                                    .range(1..=1024)
                                    .speed(1.0);
                                let drag_resp = ui.add(drag);
                                if drag_resp.changed() {
                                    resp.pattern_resized = Some((pat_idx as usize, rows as usize));
                                }
                            });
                        }
                    }
                }
            });

        resp.pattern_changed = pattern_changed;

        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("+Ins").clicked() {
                resp.insert_clicked = true;
            }
            if ui.button("Dup").clicked() {
                resp.duplicate_clicked = true;
            }
            if ui.button("-Del").clicked() {
                resp.delete_clicked = true;
            }
        });
    });

    resp
}
