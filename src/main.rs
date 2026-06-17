fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let debug_enabled = args.iter().any(|a| a == "--debug");

    let config = htrk::app_config::AppConfig::load();
    let w = config.window_width.unwrap_or(1200.0);
    let h = config.window_height.unwrap_or(800.0);

    if debug_enabled || config.debug {
        htrk::debug_log::init(true, htrk::app_config::AppConfig::config_dir());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([w, h])
            .with_title("htrk"),
        ..Default::default()
    };

    eframe::run_native(
        "htrk",
        options,
        Box::new(|_cc| Ok(Box::new(htrk::app::HtrkApp::default()))),
    )
}
