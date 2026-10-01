use super::*;

// ── Host Handler ──
//
// `HtrkHostShared` is the shared (thread-safe) callback handler that CLAP
// plugins use to log messages and request GUI changes. We register the
// `HostLog` and `HostGui` host-side extensions so plugins can talk back to us.

pub struct HtrkHost;
pub struct HtrkHostShared {
    pub(crate) state_ext: std::sync::OnceLock<Option<clack_extensions::state::PluginState>>,
}

impl Default for HtrkHostShared {
    fn default() -> Self {
        Self::new()
    }
}

impl HtrkHostShared {
    pub fn new() -> Self {
        HtrkHostShared {
            state_ext: std::sync::OnceLock::new(),
        }
    }
}

impl<'a> SharedHandler<'a> for HtrkHostShared {
    fn request_restart(&self) {
        tracing::debug!("CLAP plugin requested restart");
    }
    fn request_process(&self) {
        tracing::debug!("CLAP plugin requested process");
    }
    fn request_callback(&self) {
        tracing::debug!("CLAP plugin requested callback");
    }
    fn initializing(&self, instance: InitializingPluginHandle<'a>) {
        let _ = self.state_ext.set(instance.get_extension());
    }
}

impl HostLogImpl for HtrkHostShared {
    fn log(&self, severity: LogSeverity, message: &str) {
        match severity {
            LogSeverity::Debug => tracing::debug!("CLAP: {message}"),
            LogSeverity::Info => tracing::info!("CLAP: {message}"),
            LogSeverity::Warning => tracing::warn!("CLAP: {message}"),
            LogSeverity::Error => tracing::error!("CLAP: {message}"),
            LogSeverity::Fatal => tracing::error!("CLAP FATAL: {message}"),
            LogSeverity::HostMisbehaving => tracing::error!("CLAP HOST MISBEHAVING: {message}"),
            LogSeverity::PluginMisbehaving => tracing::warn!("CLAP PLUGIN MISBEHAVING: {message}"),
        }
    }
}

impl HostGuiImpl for HtrkHostShared {
    fn resize_hints_changed(&self) {
        tracing::debug!("CLAP plugin resize hints changed");
    }
    fn request_resize(&self, new_size: GuiSize) -> Result<(), HostError> {
        tracing::debug!(
            "CLAP plugin requested resize to {}x{}",
            new_size.width,
            new_size.height
        );
        Ok(())
    }
    fn request_show(&self) -> Result<(), HostError> {
        tracing::debug!("CLAP plugin requested show");
        Ok(())
    }
    fn request_hide(&self) -> Result<(), HostError> {
        tracing::debug!("CLAP plugin requested hide");
        Ok(())
    }
    fn closed(&self, was_destroyed: bool) {
        tracing::debug!("CLAP plugin GUI closed (destroyed: {was_destroyed})");
    }
}

/// Unit struct used as the main-thread handler type. Required so we can
/// implement external traits (like `HostStateImpl`) without violating the
/// orphan rule.
pub struct HtrkMainThread;

impl<'a> MainThreadHandler<'a> for HtrkMainThread {}

impl clack_extensions::state::HostStateImpl for HtrkMainThread {
    fn mark_dirty(&mut self) {
        tracing::debug!("CLAP plugin marked state as dirty");
    }
}

impl HostHandlers for HtrkHost {
    type Shared<'a> = HtrkHostShared;
    type MainThread<'a> = HtrkMainThread;
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder
            .register::<HostLog>()
            .register::<HostGui>()
            .register::<clack_extensions::state::HostState>();
    }
}
