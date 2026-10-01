use super::*;

impl HtrkApp {
    /// Shared exit-time shutdown. eframe's `App::on_exit` trait signature
    /// differs by the `glow` feature (which `devtools` enables), so there
    /// are two `on_exit` impls in the trait impl below; both delegate here
    /// so the shutdown runs identically in every build configuration.
    /// Stops the MCP server, stops the audio thread, drops the cpal stream,
    /// persists the preset + sample library caches, and saves the app config.
    pub(super) fn shutdown_on_exit(&mut self) {
        eprintln!("[EXIT] on_exit start");
        if let Some(ref mut server) = self.mcp_server {
            eprintln!("[EXIT] stopping MCP server");
            server.stop();
            eprintln!("[EXIT] MCP server stopped");
        }
        eprintln!("[EXIT] sending audio Stop command");
        self.core
            .send_command(crate::audio::commands::AudioCommand::Stop);
        // Cut the audio thread's command source so it can no longer try to
        // read commands after we drop the stream.
        self.core.command_sender = None;
        // Give the audio thread a moment to observe the Stop command and
        // finish any in-flight callback before we drop the cpal Stream.
        // cpal's Stream::drop on Windows WASAPI blocks until the callback
        // returns, and a callback that just dequeued our Stop command
        // returns immediately. The sleep is paranoia for the case where
        // the callback is stuck in a long mix (e.g. very large send FX).
        std::thread::sleep(std::time::Duration::from_millis(50));
        eprintln!("[EXIT] dropping audio stream");
        self.stream = None;
        eprintln!("[EXIT] audio shutdown complete");

        // Persist the preset cache atomically so the next launch can skip
        // the (potentially ~30s) rescan.
        let cache_path = crate::app_config::AppConfig::config_dir().join("preset_cache.json");
        if let Ok(lib) = self.preset_library.read() {
            if lib.preset_count() > 0 {
                let _ = lib.save_to_file(&cache_path);
            }
        }
        // Persist the sample library cache so the next launch has instant
        // directory browsing. Skipped if the library is empty (no roots
        // configured) to avoid creating empty cache files on first run.
        let sample_cache_path =
            crate::app_config::AppConfig::config_dir().join("sample_library_cache.json");
        if let Ok(lib) = self.sample_library.read() {
            if !lib.roots.is_empty() && lib.cache_len() > 0 {
                let _ = lib.save_to_file(&sample_cache_path);
            }
        }
        eprintln!("[EXIT] saving config");
        crate::actions::save_config(self);
        eprintln!("[EXIT] on_exit done");
    }
}
