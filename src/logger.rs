use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);
static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn init(path: &str, debug: bool) {
    DEBUG_ENABLED.store(debug, Ordering::Relaxed);

    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    if let Ok(file) = options.open(path) {
        if let Ok(mut lock) = LOG_FILE.lock() {
            *lock = Some(file);
        }
    }

    log_info(format!(
        "=== Teams TUI Session Started (Debug: {debug}) ==="
    ));
}

#[must_use]
pub fn is_debug() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

pub fn log_info(msg: impl AsRef<str>) {
    write_entry("INFO", msg.as_ref());
}

pub fn log_debug(msg: impl AsRef<str>) {
    if is_debug() {
        write_entry("DEBUG", msg.as_ref());
    }
}

pub fn log_warn(msg: impl AsRef<str>) {
    write_entry("WARN", msg.as_ref());
}

pub fn log_error(msg: impl AsRef<str>) {
    write_entry("ERROR", msg.as_ref());
}

fn write_entry(level: &str, msg: &str) {
    if let Ok(mut lock) = LOG_FILE.lock() {
        if let Some(ref mut file) = *lock {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "[{timestamp}] [{level}] {msg}");
        }
    }
}
