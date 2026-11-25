//! Atomic file writes: write to `<path>.tmp`, then rename over the target.
//! std::fs::rename on Windows maps to MoveFileExW(MOVEFILE_REPLACE_EXISTING),
//! so the destination is replaced in one step — a crash mid-write leaves the
//! old file intact instead of a truncated half. (Ported 1:1 from Liquid Tools.)

use std::path::Path;

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension(match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{ext}.tmp"),
        None => "tmp".to_string(),
    });
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_existing() {
        let dir = std::env::temp_dir().join(format!("liqlip-fsx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert!(!dir.join("data.json.tmp").exists(), "tmp file must not linger");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
