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