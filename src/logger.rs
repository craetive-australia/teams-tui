use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

static LOG_PATH: Mutex<Option<String>> = Mutex::new(None);
static DEBUG_ENABLED: Mutex<bool> = Mutex::new(false);

pub fn init(path: &str, debug: bool) {
    if let Ok(mut p) = LOG_PATH.lock() {
        *p = Some(path.to_string());
    }
    if let Ok(mut d) = DEBUG_ENABLED.lock() {
        *d = debug;
    }

    log_info(format!(
        "=== Teams TUI Session Started (Debug: {}) ===",
        debug
    ));
}

pub fn is_debug() -> bool {
    DEBUG_ENABLED.lock().map(|d| *d).unwrap_or(false)
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
    if let Ok(path_guard) = LOG_PATH.lock() {
        if let Some(ref path) = *path_guard {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let line = format!("[{}] [{}] {}\n", timestamp, level, msg);
            let mut options = OpenOptions::new();
            options.create(true).append(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            if let Ok(mut file) = options.open(path) {
                let _ = file.write_all(line.as_bytes());
            }
        }
    }
}
