/// Debug logger for RuhI TSF with automatic log size rotation (max 2 MB)
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static LOG_WRITE_COUNT: AtomicU64 = AtomicU64::new(0);
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024; // 2 MB

fn get_log_path() -> PathBuf {
    let mut path = match std::env::var("LOCALAPPDATA") {
        Ok(val) => PathBuf::from(val).join("RuhI"),
        Err(_) => std::env::temp_dir().join("RuhI"),
    };
    let _ = fs::create_dir_all(&path);
    path.push("tsf_debug.log");
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > MAX_LOG_BYTES {
            let _ = fs::write(&path, b"");
        }
    }
    path
}

pub fn log(msg: &str) {
    let path = LOG_PATH.get_or_init(get_log_path);
    let count = LOG_WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
    if count % 256 == 0 {
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > MAX_LOG_BYTES {
                let _ = fs::write(path, b"");
            }
        }
    }
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let time_str = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(f, "[{}] (PID {}) {}", time_str, std::process::id(), msg);
    }
}
