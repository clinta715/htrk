fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let debug_enabled = args.iter().any(|a| a == "--debug");

    if debug_enabled || htrk::app_config::AppConfig::load().debug {
        htrk::debug_log::init(true, htrk::app_config::AppConfig::config_dir());
    }

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
