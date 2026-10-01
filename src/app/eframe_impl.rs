use super::*;

impl eframe::App for HtrkApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let devmcp = self.devmcp.clone();
        let _guard = FrameGuard::new(devmcp.as_ref(), &ctx);

        let vp_rect = ctx.viewport_rect();
        self.config.window_width = Some(vp_rect.width());
        self.config.window_height = Some(vp_rect.height());

        // Initial plugin scan. Done on the first frame so the struct
        // constructor doesn't need to mutate `self`. The scan blocks the
        // main thread for ~1s on a typical install (50-100 plugins);
        // for very large collections we can make this incremental later.
        if !self.plugin_scan_done {
            let _ = self.rescan_plugins();
            self.plugin_scan_done = true;
        }

        if !self.preset_scan_done && self.plugin_scan_done {
            let cache_path = crate::app_config::AppConfig::config_dir().join("preset_cache.json");
            if let Ok(cached) = crate::audio::plugins::PresetLibrary::load_from_file(&cache_path) {
                eprintln!(
                    "[presets] Loaded {} preset(s) from cache",
                    cached.preset_count()
                );
                if let Ok(mut lib) = self.preset_library.write() {
                    *lib = cached;
                }
            } else {
                let _ = self.rescan_presets();
                if let Ok(lib) = self.preset_library.read() {
                    if lib.preset_count() > 0 {
                        let _ = lib.save_to_file(&cache_path);
                    }
                }
            }
            self.preset_scan_done = true;
        }

        if !self.sample_library_loaded {
            let cache_path =
                crate::app_config::AppConfig::config_dir().join("sample_library_cache.json");
            if let Ok(cached) = crate::mcp::library::SampleLibrary::load_from_file(&cache_path) {
                let cached_count = cached.cache_len();
                eprintln!(
                    "[sample_library] Loaded {} entry/entries from cache",
                    cached_count
                );
                if let Ok(mut lib) = self.sample_library.write() {
                    // Preserve the configured roots (from AppConfig) — the
                    // cache's `roots` field reflects the state at save time
                    // and may not match the current configuration.
                    let configured_roots = lib.roots.clone();
                    *lib = cached;
                    if !configured_roots.is_empty() {
                        lib.roots = configured_roots;
                    }
                }
            }
            self.sample_library_loaded = true;
        }

        // Release any previewed instrument-plugin notes whose trigger
        // key is no longer held.
        self.release_unheld_preview_notes(&ctx);

        let (playback_row, playback_order, playback_pattern, playback_tick, playback_speed) =
            self.draw_preamble(&ctx);

        if ctx.input(|i| i.viewport().close_requested()) {
            if self.config.confirm_on_exit
                && self.core.module_dirty()
                && !self.show_exit_confirm
                && !self.exit_confirmed
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_exit_confirm = true;
            }
        }

        self.handle_menu_bar(ui, &ctx);

        self.handle_transport_bar(ui);
        self.handle_oscilloscope(ui, &ctx);
        self.handle_status_bar(ui);

        self.handle_order_list(
            ui,
            playback_order,
            playback_row,
            playback_tick,
            playback_speed,
        );

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.core.module.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("htrk - tracker");
                    ui.add_space(20.0);
                    ui.label(
                        "No module loaded. Press Ctrl+O to open a file, or Ctrl+N for a new song.",
                    );
                });
                return;
            }

            ui.horizontal(|ui| {
                ui.dev_selectable_value(
                    "view.pattern",
                    &mut self.current_view,
                    AppView::Pattern,
                    "Pattern",
                );
                ui.dev_selectable_value(
                    "view.sample",
                    &mut self.current_view,
                    AppView::Sample,
                    "Sample",
                );
                ui.dev_selectable_value(
                    "view.instrument",
                    &mut self.current_view,
                    AppView::Instrument,
                    "Instrument",
                );
                ui.dev_selectable_value(
                    "view.sendfx",
                    &mut self.current_view,
                    AppView::SendFx,
                    "Send FX",
                );
                ui.dev_selectable_value(
                    "view.playback",
                    &mut self.current_view,
                    AppView::Playback,
                    "Playback",
                );
                ui.dev_selectable_value(
                    "view.automation",
                    &mut self.current_view,
                    AppView::Automation,
                    "Automation",
                );
                ui.dev_selectable_value(
                    "view.mixer",
                    &mut self.current_view,
                    AppView::Mixer,
                    "Mixer",
                );
            });
            ui.dev_separator("view.separator");

            match self.current_view {
                AppView::Pattern => self.handle_pattern_tab(
                    ui,
                    playback_pattern,
                    playback_row,
                    playback_tick,
                    playback_speed,
                ),
                AppView::Sample => self.handle_sample_tab(ui),
                AppView::Instrument => self.handle_instrument_tab(ui, frame),
                AppView::SendFx => self.handle_sendfx_tab(ui, frame, &ctx),
                AppView::Playback => self.handle_playback_tab(
                    ui,
                    playback_pattern,
                    playback_row,
                    playback_tick,
                    playback_speed,
                ),
                AppView::Automation => self.handle_automation_tab(ui),
                AppView::Mixer => self.handle_mixer_tab(ui),
            }
        });

        let size = ctx.viewport_rect().size();
        self.config.window_width = Some(size.x);
        self.config.window_height = Some(size.y);

        self.draw_dialogs(&ctx);
    }

    #[cfg(not(feature = "devtools"))]
    fn on_exit(&mut self) {
        self.shutdown_on_exit();
    }

    #[cfg(feature = "devtools")]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown_on_exit();
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        eguidev::raw_input_hook(self.devmcp.as_ref(), ctx, raw_input);
    }
}
