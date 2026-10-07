/// Debug logger for RuhI TSF
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

fn get_log_path() -> PathBuf {
    let mut path = match std::env::var("LOCALAPPDATA") {
        Ok(val) => PathBuf::from(val).join("RuhI"),
        Err(_) => std::env::temp_dir().join("RuhI"),
    };
    let _ = fs::create_dir_all(&path);
    path.push("tsf_debug.log");
    path
}

pub fn log(msg: &str) {
    let path = LOG_PATH.get_or_init(get_log_path);
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
