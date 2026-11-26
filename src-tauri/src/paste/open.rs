//! Open / Open with… / Show in Explorer (SPEC §3.5 `Ctrl+O`, `Ctrl+Shift+O`, §3.7) — owner **W1-C**.
//!
//! - **Open**: links in the default browser, files and stored images with their default app —
//!   through `tauri-plugin-opener`'s Rust API (`open_url` / `open_path`; the Rust side is not
//!   subject to the JS permission scope). Only `http`, `https`, `ftp`, `mailto` URLs go to the
//!   shell; a `www.` link gets `https://`; a `file:` link is opened as the local path it names.
//! - **Open with…**: `SHOpenWithDialog` with `OAIF_EXEC` on a dedicated STA thread ("You can
//!   only use SHOpenWithDialog to open a single file"; without OAIF_EXEC the user gets a
//!   settings hint instead — learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shopenwithdialog).
//! - **Show in Explorer**: the plugin's `reveal_items_in_dir`, which is
//!   `SHOpenFolderAndSelectItems` (+ a ShellExecute fallback) —
//!   learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shopenfolderandselectitems.

use super::compose::file_paths;
use crate::model::Kind;
use crate::store::PastePayload;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// What "Open" resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenTarget {
    Url(String),
    Path(PathBuf),
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// `%XX` decoding (UTF-8), as in `file:` URLs.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A link record's text → something safe to open. `None` for anything that is not a plain
/// web / mail link or a local `file:` path (no `javascript:`, no custom protocol handlers).
pub fn normalize_link(raw: &str) -> Option<OpenTarget> {
    let s = raw.trim();
    if s.is_empty() || s.contains(['\r', '\n']) {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("file:") {
        let rest = s["file:".len()..].trim_start_matches('/');
        let path = percent_decode(rest).replace('/', "\\");
        // `file://server/share/x` → UNC path.
        let unc = s["file:".len()..].starts_with("//") && !s["file:".len()..].starts_with("///");
        let path = if unc { format!("\\\\{path}") } else { path };
        return (!path.is_empty()).then(|| OpenTarget::Path(PathBuf::from(path)));
    }
    for scheme in ["http://", "https://", "ftp://", "mailto:"] {
        if lower.starts_with(scheme) {
            return Some(OpenTarget::Url(s.to_string()));
        }
    }
    if lower.starts_with("www.") && !s.contains(char::is_whitespace) {
        return Some(OpenTarget::Url(format!("https://{s}")));
    }
    None
}

/// The stored PNG path, resolved against the data directory if the store returned a
/// relative one.
pub fn resolve_media(app: &AppHandle, p: &Path) -> PathBuf {
    if p.is_absolute() {
        return p.to_path_buf();
    }
    app.path().app_data_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf())
}

/// "Open" for a record (pure, existence checked by the caller).
pub fn open_target(p: &PastePayload) -> Result<OpenTarget, String> {
    match p.kind {
        Kind::Link => normalize_link(p.text.as_deref().unwrap_or("")).ok_or_else(|| "invalid: not an openable link".to_string()),
        Kind::File => file_paths(p).into_iter().next().map(|s| OpenTarget::Path(PathBuf::from(s))).ok_or_else(|| "not_found: no file".to_string()),
        Kind::Image | Kind::Screenshot => p.png_path.clone().map(OpenTarget::Path).ok_or_else(|| "not_found: image file".to_string()),
        _ => Err(format!("invalid: cannot open a {} record", p.kind.as_str())),
    }
}

/// Existing local files of a file / image record (for Open with… / Reveal).
pub fn local_files(app: &AppHandle, p: &PastePayload) -> Result<Vec<PathBuf>, String> {
    let paths: Vec<PathBuf> = match p.kind {
        Kind::File => file_paths(p).into_iter().map(PathBuf::from).collect(),
        Kind::Image | Kind::Screenshot => p.png_path.iter().map(|x| resolve_media(app, x)).collect(),
        _ => return Err(format!("invalid: a {} record has no file", p.kind.as_str())),
    };
    let existing: Vec<PathBuf> = paths.into_iter().filter(|x| x.exists()).collect();
    if existing.is_empty() {
        return Err("not_found: the file no longer exists".into());
    }
    Ok(existing)
}

pub fn open(app: &AppHandle, p: &PastePayload) -> Result<(), String> {
    match open_target(p)? {
        OpenTarget::Url(url) => app.opener().open_url(url, None::<&str>).map_err(|e| format!("failed: {e}")),
        OpenTarget::Path(path) => {
            let path = if p.kind.is_image() { resolve_media(app, &path) } else { path };
            if !path.exists() {
                return Err("not_found: the file no longer exists".into());
            }
            app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| format!("failed: {e}"))
        }
    }
}

pub fn reveal(app: &AppHandle, p: &PastePayload) -> Result<(), String> {
    let files = local_files(app, p)?;
    app.opener().reveal_items_in_dir(files).map_err(|e| format!("failed: {e}"))
}

/// "Open with…" for the first existing file. Blocks until the dialog closes (call from a
/// worker): the dialog runs on its own thread with COM initialized as STA.
///
/// The shelf is hidden first (W3-A): the dialog is a window of THIS process, so it keeps the
/// foreground and the shelf's focus-loss exit stays put (focus went to an own window), while
/// the always-on-top shelf covered the centred, non-topmost dialog — the user saw nothing.
/// After the dialog the chosen app takes the foreground, as after "Open".
pub fn open_with(app: &AppHandle, p: &PastePayload) -> Result<(), String> {
    let file = local_files(app, p)?.remove(0);
    crate::picker::hide_now(app);
    #[cfg(windows)]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("liqlip-open-with".into())
            .spawn(move || {
                let _ = tx.send(win::open_with_dialog(&file));
            })
            .map_err(|e| format!("failed: {e}"))?;
        rx.recv().map_err(|e| format!("failed: {e}"))?
    }
    #[cfg(not(windows))]
    {
        let _ = file;
        Err("failed: Windows only".into())
    }
}

#[cfg(windows)]
mod win {
    use std::path::Path;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
    use windows::Win32::UI::Shell::{SHOpenWithDialog, OAIF_EXEC, OPENASINFO};

    pub fn open_with_dialog(file: &Path) -> Result<(), String> {
        let wide: Vec<u16> = file.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: COM initialized (and balanced) on this dedicated thread; OPENASINFO points
        // at a NUL-terminated path that outlives the modal call.
        unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let info = OPENASINFO { pcszFile: PCWSTR(wide.as_ptr()), pcszClass: PCWSTR::null(), oaifInFlags: OAIF_EXEC };
            let r = SHOpenWithDialog(None, &info);
            if init.is_ok() {
                CoUninitialize();
            }
            match r {
                Ok(()) => Ok(()),
                // The user closed the dialog.
                Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => Ok(()),
                Err(e) => Err(format!("failed: SHOpenWithDialog {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paste::compose::tests::{files, image, text};

    #[test]
    fn links_are_whitelisted() {
        assert_eq!(normalize_link(" https://example.com/a?b=1 "), Some(OpenTarget::Url("https://example.com/a?b=1".into())));
        assert_eq!(normalize_link("HTTP://X.ORG"), Some(OpenTarget::Url("HTTP://X.ORG".into())));
        assert_eq!(normalize_link("www.example.com"), Some(OpenTarget::Url("https://www.example.com".into())));
        assert_eq!(normalize_link("mailto:a@b.c"), Some(OpenTarget::Url("mailto:a@b.c".into())));
        assert_eq!(normalize_link("javascript:alert(1)"), None);
        assert_eq!(normalize_link("ms-settings:privacy"), None);
        assert_eq!(normalize_link("https://a\nhttps://b"), None);
        assert_eq!(normalize_link(""), None);
    }

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(normalize_link("file:///C:/Users/Me/My%20Doc.txt"), Some(OpenTarget::Path("C:\\Users\\Me\\My Doc.txt".into())));
        assert_eq!(normalize_link("file:///C:/%D0%9F%D0%B0%D0%BF%D0%BA%D0%B0"), Some(OpenTarget::Path("C:\\Папка".into())));
        assert_eq!(normalize_link("file://server/share/x.pdf"), Some(OpenTarget::Path("\\\\server\\share\\x.pdf".into())));
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%4"), "%4");
    }

    #[test]
    fn open_target_by_kind() {
        let link = PastePayload { kind: Kind::Link, ..text(1, "https://e.com") };
        assert_eq!(open_target(&link), Ok(OpenTarget::Url("https://e.com".into())));
        assert_eq!(open_target(&files(2, &["C:\\a.pdf", "C:\\b.pdf"])), Ok(OpenTarget::Path("C:\\a.pdf".into())));
        assert_eq!(open_target(&image(3)), Ok(OpenTarget::Path("C:\\data\\media\\3.png".into())));
        assert!(open_target(&text(4, "plain")).unwrap_err().starts_with("invalid"));
    }
}
