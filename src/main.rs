fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("htrk"),
        ..Default::default()
    };

    eframe::run_native(
        "htrk",
        options,
        Box::new(|_cc| Ok(Box::new(htrk::app::HtrkApp::default()))),
    )
}
