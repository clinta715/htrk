// Debug logging module
// WARNING: Using --debug flag in production causes performance issues.
// Use only when actively debugging, then remove --debug flag.
// For debug builds, compile with: cargo build --features "debug_log,audio_debug"

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static DEBUG_FILE: Mutex<Option<File>> = Mutex::new(None);
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn init(enabled: bool, config_dir: PathBuf) {
    DEBUG_ENABLED.store(enabled, Ordering::SeqCst);
    if !enabled {
        return;
    }

    let log_path = config_dir.join("debug.log");
    let _ = std::fs::create_dir_all(&config_dir);

    let file = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open debug.log: {}", e);
            return;
        }
    };

    let mut guard = DEBUG_FILE.lock().unwrap();
    *guard = Some(file);
}

pub fn is_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::SeqCst)
}

pub fn log(message: &str) {
    if !is_enabled() {
        return;
    }

    if let Ok(mut guard) = DEBUG_FILE.lock() {
        if let Some(ref mut file) = *guard {
            let _ = file.write_all(message.as_bytes());
            let _ = file.flush();
        }
    }
}

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => ({
        if $crate::debug_log::is_enabled() {
            let _msg = format!($($arg)*);
            $crate::debug_log::log(&_msg);
        }
    });
}

pub fn shutdown() {
    DEBUG_ENABLED.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = DEBUG_FILE.lock() {
        *guard = None;
    }
}

/// Install a panic hook that writes the panic message + location to
/// `<config_dir>/crash.log` and to stderr. Without this, a panic during
/// eframe shutdown is swallowed by the windowing layer and the user only
/// sees the process vanish with no clue what happened.
pub fn install_panic_hook(config_dir: std::path::PathBuf) {
    use std::io::Write;
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = std::fs::create_dir_all(&config_dir);
        let log_path = config_dir.join("crash.log");
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Always print via the default hook so stderr shows it too.
            default_hook(info);

            let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "<non-string panic payload>".to_string()
            };
            let location = info.location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "<unknown>".to_string());

            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);

            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true).append(true).open(&log_path)
            {
                let _ = writeln!(f, "[{:.3}] PANIC at {}: {}", timestamp, location, payload);
                let _ = writeln!(f, "  backtrace: {}", std::backtrace::Backtrace::capture());
                let _ = f.flush();
            }
        }));
    });
}

// ── tracing integration ──
//
// Initializes the `tracing` ecosystem alongside the legacy debug log. We use
// `tracing` for plugin-internal logging (CLAP HostLog, GUI events, etc.) and
// route everything to stderr by default; if the user has configured a log file
// path via `AppConfig.log_file_path`, we also write to that file (no ANSI).
//
// The legacy `debug_log` module above continues to work for app-level debug
// messages written via `debug_log!` — they are unaffected by tracing.
//
// Set `RUST_LOG=htrk=debug,clap=info` (etc.) to control verbosity. The
// `EnvFilter` honors per-target directives.

use std::sync::Once;
static TRACING_INIT: Once = Once::new();

/// Initialize tracing. Safe to call multiple times — only the first call
/// has any effect. Called from `HtrkApp::default`.
pub fn init_tracing(log_file_path: Option<&str>) {
    TRACING_INIT.call_once(|| {
        use tracing_subscriber::{fmt, prelude::*, EnvFilter};

        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("htrk=info,clap=info"));

        let stderr_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true);

        let registry = tracing_subscriber::registry()
            .with(env_filter)
            .with(stderr_layer);

        if let Some(path) = log_file_path {
            if let Ok(file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let file_layer = fmt::layer()
                    .with_writer(file)
                    .with_ansi(false);
                let _ = registry.with(file_layer).try_init();
                return;
            } else {
                eprintln!(
                    "[htrk] Failed to open log file '{path}', falling back to stderr only"
                );
            }
        }
        let _ = registry.try_init();
    });
}