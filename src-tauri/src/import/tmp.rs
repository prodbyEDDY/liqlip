//! Temporary directories (`%TEMP%\liqlip-<what>-<pid>-<ns>-<seq>`; a `replace` restore
//! stages inside the data directory so that every swap step is a same-volume rename),
//! removed by a guard on every path, the panic path included.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// `%TEMP%\<prefix><pid>-<ns>-<seq>`.
    pub fn new(prefix: &str) -> Result<TempDir, String> {
        TempDir::new_in(&std::env::temp_dir(), prefix)
    }

    pub fn new_in(parent: &Path, prefix: &str) -> Result<TempDir, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = parent.join(format!("{prefix}{}-{nanos}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&path).map_err(|e| format!("failed: create temporary folder: {e}"))?;
        Ok(TempDir { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Open SQLite connections must be closed before this runs (Windows cannot delete
        // open files): owners declare the guard before the connection so it drops last.
        if let Err(e) = std::fs::remove_dir_all(&self.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                crate::logx::warn("import", &format!("temporary folder not removed: {e}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_on_drop() {
        let p = {
            let t = TempDir::new("liqlip-tmp-test-").unwrap();
            std::fs::write(t.path().join("x.bin"), b"1").unwrap();
            t.path().to_path_buf()
        };
        assert!(!p.exists());
    }
}
