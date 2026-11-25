//! Minimal persistent logger for a tray app whose stderr nobody sees.
//! One file under `%APPDATA%\com.liqlip.app\logs\liqlip.log`, rotated at 1 MB to
//! `.old`. Every call also mirrors to stderr so `pnpm tauri dev` keeps its console.
//! Ported from Liquid Tools `logx.rs`.
//!
//! PRIVACY RULE (CLAUDE.md hard rule 5): never log clipboard content — not text, not
//! file names, not window titles. Log ids, sizes, kinds, error codes, durations.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const MAX_BYTES: u64 = 1024 * 1024;
const FILE: &str = "liqlip.log";
const FILE_OLD: &str = "liqlip.log.old";

static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Point the logger at `dir` (created if absent) and install a panic hook that
/// records the panic before the default hook runs. Call once, first thing in `setup()`.
pub fn init_at(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    if let Ok(mut d) = DIR.lock() {
        *d = Some(dir.to_path_buf());
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        warn("panic", &format!("{info}"));
        default(info);
    }));
}

/// Append one timestamped line; rotates at MAX_BYTES. Best-effort, never panics.
pub fn warn(scope: &str, msg: &str) {
    eprintln!("[{scope}] {msg}");
    let Some(dir) = DIR.lock().ok().and_then(|d| d.clone()) else {
        return;
    };
    let path = dir.join(FILE);
    if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join(FILE_OLD));
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{secs} [{scope}] {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_rotates() {
        let dir = std::env::temp_dir().join(format!("liqlip-logx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        init_at(&dir);
        warn("test", "hello");
        let log = dir.join(FILE);
        let body = std::fs::read_to_string(&log).unwrap();
        assert!(body.contains("[test] hello"));
        // Force rotation: write > MAX_BYTES.
        let big = "x".repeat(4096);
        for _ in 0..((MAX_BYTES as usize / 4096) + 2) {
            warn("test", &big);
        }
        assert!(dir.join(FILE_OLD).exists(), "rotated file must exist");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
