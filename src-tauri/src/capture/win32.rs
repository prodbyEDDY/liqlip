//! Win32 side of capture (`windows` 0.62): the [`ClipboardApi`] implementation, the owner
//! process (`GetClipboardOwner` → `GetWindowThreadProcessId` → `QueryFullProcessImageNameW`,
//! foreground fallback) and the app display name (`FileDescription` from the version
//! resource). Every `unsafe` block wraps one documented call; links next to each.

use super::snapshot::{ClipboardApi, FormatIds, OwnerInfo};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_NULL,
};

/// NUL-terminated UTF-16 for `PCWSTR` parameters.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-registerclipboardformatw
/// — returns the same id for the same name in every process; 0 on failure (then the format
/// is simply never "present").
fn register(name: &str) -> u32 {
    let w = wide(name);
    // SAFETY: `w` is a valid NUL-terminated wide string alive for the call.
    unsafe { RegisterClipboardFormatW(PCWSTR(w.as_ptr())) }
}

pub struct Win32Clipboard {
    ids: FormatIds,
    own_pid: u32,
}

impl Win32Clipboard {
    pub fn new() -> Self {
        Win32Clipboard {
            ids: FormatIds {
                // https://learn.microsoft.com/windows/win32/dataxchg/html-clipboard-format
                html: register("HTML Format"),
                rtf: register("Rich Text Format"),
                png: register("PNG"),
                // Shell clipboard format CFSTR_PREFERREDDROPEFFECT.
                drop_effect: register("Preferred DropEffect"),
                // https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats
                // ("Cloud Clipboard and Clipboard History Formats").
                exclude_monitor: register("ExcludeClipboardContentFromMonitorProcessing"),
                can_include_history: register("CanIncludeInClipboardHistory"),
                can_upload_cloud: register("CanUploadToCloudClipboard"),
                // Convention used by password managers (KeePass, …) for clipboard viewers.
                viewer_ignore: register("Clipboard Viewer Ignore"),
            },
            // SAFETY: no preconditions.
            own_pid: unsafe { GetCurrentProcessId() },
        }
    }
}

/// `GetClipboardSequenceNumber` — usable without opening the clipboard (the listener's
/// dedupe and the paste writer's `mark_own_write`).
pub fn sequence_number() -> u32 {
    // SAFETY: no preconditions; 0 = no WINSTA_ACCESSCLIPBOARD.
    unsafe { GetClipboardSequenceNumber() }
}

/// Process id of a window's thread (0 if the window is gone).
/// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid
pub fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: `pid` outlives the call; an invalid hwnd yields 0.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Full image path of a process, lower-cased. `PROCESS_QUERY_LIMITED_INFORMATION` is enough
/// for `QueryFullProcessImageNameW` and is granted for elevated processes of the same user.
/// https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew
pub fn process_exe(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    // SAFETY: plain handle-returning call; the handle is closed below on every path.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut result = None;
    for cap in [1024usize, 32_768] {
        let mut buf = vec![0u16; cap];
        let mut len = cap as u32;
        // SAFETY: `buf` holds `len` u16s; `len` receives the written count (no NUL).
        let ok = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) };
        if ok.is_ok() {
            result = Some(String::from_utf16_lossy(&buf[..len as usize]).to_lowercase());
            break;
        }
    }
    // SAFETY: `process` came from OpenProcess and is closed exactly once.
    let _ = unsafe { CloseHandle(process) };
    result.filter(|s| !s.is_empty())
}

/// Owner of the clipboard content, else the foreground window (an app that empties the
/// clipboard after `OpenClipboard(NULL)` leaves no owner).
/// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getclipboardowner
pub fn clipboard_owner() -> Option<OwnerInfo> {
    // SAFETY: no preconditions; Err = no owner window.
    if let Ok(hwnd) = unsafe { GetClipboardOwner() } {
        let pid = window_pid(hwnd);
        if pid != 0 {
            return Some(OwnerInfo { pid, exe: process_exe(pid), from_foreground: false });
        }
    }
    // SAFETY: no preconditions; may be null.
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_invalid() {
        return None;
    }
    let pid = window_pid(fg);
    (pid != 0).then(|| OwnerInfo { pid, exe: process_exe(pid), from_foreground: true })
}

impl ClipboardApi for Win32Clipboard {
    fn ids(&self) -> FormatIds {
        self.ids
    }

    fn sequence(&self) -> u32 {
        sequence_number()
    }

    fn own_pid(&self) -> u32 {
        self.own_pid
    }

    fn owner(&self) -> Option<OwnerInfo> {
        clipboard_owner()
    }

    /// `WM_NULL` with `SMTO_ABORTIFHUNG`: returns at once if the system already considers the
    /// owner's thread hung, else after at most `timeout`. Delayed rendering needs an owner
    /// window, so "no owner" means nothing can block.
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendmessagetimeoutw
    fn owner_responsive(&self, timeout: Duration) -> bool {
        // SAFETY: no preconditions.
        let Ok(hwnd) = (unsafe { GetClipboardOwner() }) else { return true };
        let pid = window_pid(hwnd);
        if pid == 0 || pid == self.own_pid {
            return true; // gone, or our own window (never probe ourselves)
        }
        let mut result = 0usize;
        // SAFETY: `result` outlives the call; WM_NULL has no parameters.
        let r = unsafe {
            SendMessageTimeoutW(
                hwnd,
                WM_NULL,
                WPARAM(0),
                LPARAM(0),
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                timeout.as_millis().min(u32::MAX as u128) as u32,
                Some(&mut result),
            )
        };
        r.0 != 0
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-openclipboard —
    /// `NULL` owner: we only read.
    fn try_open(&self) -> bool {
        // SAFETY: no preconditions; paired with `close` by `snapshot::OpenGuard`.
        unsafe { OpenClipboard(None) }.is_ok()
    }

    fn close(&self) {
        // SAFETY: called once per successful `try_open`, on the same thread.
        let _ = unsafe { CloseClipboard() };
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumclipboardformats
    fn formats(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut f = 0u32;
        loop {
            // SAFETY: clipboard is open (caller); 0 = end of list or failure.
            f = unsafe { EnumClipboardFormats(f) };
            if f == 0 || out.len() >= 1024 {
                break;
            }
            out.push(f);
        }
        out
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-isclipboardformatavailable
    fn available(&self, fmt: u32) -> bool {
        // SAFETY: no preconditions.
        unsafe { IsClipboardFormatAvailable(fmt) }.is_ok()
    }

    /// `GetClipboardData` → `GlobalSize` + `GlobalLock` → visit → `GlobalUnlock`. The handle
    /// belongs to the clipboard: never freed, never kept locked, never used after close
    /// (GetClipboardData remarks).
    fn read_with(&self, fmt: u32, visit: &mut dyn FnMut(&[u8])) -> bool {
        // SAFETY: clipboard is open on this thread (caller). May block while the owner renders
        // a delayed format — that is why the snapshot runs on its own thread.
        let Ok(handle) = (unsafe { GetClipboardData(fmt) }) else { return false };
        let hglobal = HGLOBAL(handle.0);
        // SAFETY: `hglobal` is the clipboard's memory object for `fmt`.
        let size = unsafe { GlobalSize(hglobal) };
        if size == 0 {
            return false;
        }
        // SAFETY: as above; null = lock failed (e.g. not a global memory object).
        let ptr = unsafe { GlobalLock(hglobal) } as *const u8;
        if ptr.is_null() {
            return false;
        }
        // SAFETY: GlobalLock returned a pointer to at least `size` readable bytes, valid until
        // GlobalUnlock; the slice does not escape `visit`.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, size) };
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| visit(bytes)));
        // SAFETY: balanced with the GlobalLock above (its BOOL is "still locked", not failure).
        let _ = unsafe { GlobalUnlock(hglobal) };
        if let Err(p) = r {
            std::panic::resume_unwind(p);
        }
        true
    }

    /// https://learn.microsoft.com/windows/win32/api/shellapi/nf-shellapi-dragqueryfilew —
    /// `0xFFFFFFFF` returns the count; a null buffer returns the length without the NUL.
    fn read_files(&self, max: usize) -> Option<(Vec<String>, usize)> {
        // SAFETY: clipboard open; the CF_HDROP handle is an HDROP owned by the clipboard.
        let handle = unsafe { GetClipboardData(super::snapshot::CF_HDROP) }.ok()?;
        let hdrop = HDROP(handle.0);
        // SAFETY: valid HDROP for the duration of the open clipboard.
        let total = unsafe { DragQueryFileW(hdrop, 0xFFFF_FFFF, None) } as usize;
        let mut paths = Vec::with_capacity(total.min(max));
        for i in 0..total.min(max) as u32 {
            // SAFETY: as above.
            let len = unsafe { DragQueryFileW(hdrop, i, None) } as usize;
            if len == 0 {
                continue;
            }
            let mut buf = vec![0u16; len + 1];
            // SAFETY: `buf` has room for `len` chars + NUL.
            let n = unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) } as usize;
            if n > 0 {
                paths.push(String::from_utf16_lossy(&buf[..n.min(len)]));
            }
        }
        Some((paths, total))
    }
}

// ── App display name ────────────────────────────────────────────────────────

fn names() -> &'static Mutex<HashMap<String, String>> {
    static NAMES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    NAMES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `FileDescription` of an exe (cached per path), else the file name without `.exe`.
pub fn app_name(exe: &str) -> String {
    if let Some(n) = names().lock().get(exe) {
        return n.clone();
    }
    let name = file_description(exe).unwrap_or_else(|| super::pipeline::default_app_name(exe));
    let mut map = names().lock();
    if map.len() > 512 {
        map.clear();
    }
    map.insert(exe.to_string(), name.clone());
    name
}

/// https://learn.microsoft.com/windows/win32/api/winver/nf-winver-getfileversioninfow and
/// …/nf-winver-verqueryvaluew: read `\VarFileInfo\Translation`, then
/// `\StringFileInfo\<lang><codepage>\FileDescription` for each listed language (falling back
/// to US English / Unicode and Latin-1 code pages).
fn file_description(exe: &str) -> Option<String> {
    let path = wide(exe);
    // SAFETY: valid NUL-terminated path.
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None) };
    if size == 0 {
        return None;
    }
    let mut block = vec![0u8; size as usize];
    // SAFETY: `block` is `size` bytes.
    unsafe { GetFileVersionInfoW(PCWSTR(path.as_ptr()), None, size, block.as_mut_ptr().cast()) }.ok()?;

    let query = |sub: &str| -> Option<(*const u8, u32)> {
        let sub = wide(sub);
        let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        // SAFETY: `block` is a version-info block from GetFileVersionInfoW; `ptr` points into
        // it (valid while `block` lives); `len` is in chars for strings, bytes otherwise.
        let ok = unsafe { VerQueryValueW(block.as_ptr().cast(), PCWSTR(sub.as_ptr()), &mut ptr, &mut len) };
        (ok.as_bool() && !ptr.is_null() && len > 0).then_some((ptr as *const u8, len))
    };

    let mut langs: Vec<(u16, u16)> = Vec::new();
    if let Some((p, len)) = query("\\VarFileInfo\\Translation") {
        // SAFETY: `len` bytes of (WORD lang, WORD codepage) pairs inside `block`.
        let bytes = unsafe { std::slice::from_raw_parts(p, len as usize) };
        for pair in bytes.chunks_exact(4) {
            langs.push((u16::from_le_bytes([pair[0], pair[1]]), u16::from_le_bytes([pair[2], pair[3]])));
        }
    }
    langs.extend([(0x0409, 0x04B0), (0x0409, 0x04E4), (0x0000, 0x04B0)]);
    for (lang, cp) in langs {
        if let Some((p, len)) = query(&format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription")) {
            // SAFETY: `len` UTF-16 chars (incl. the NUL) inside `block`.
            let chars = unsafe { std::slice::from_raw_parts(p as *const u16, len as usize) };
            let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
            let s = String::from_utf16_lossy(&chars[..end]).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_formats_are_stable_and_distinct() {
        let a = Win32Clipboard::new();
        let b = Win32Clipboard::new();
        assert_eq!(a.ids, b.ids);
        let ids = [
            a.ids.html,
            a.ids.rtf,
            a.ids.png,
            a.ids.drop_effect,
            a.ids.exclude_monitor,
            a.ids.can_include_history,
            a.ids.can_upload_cloud,
            a.ids.viewer_ignore,
        ];
        // Registered formats live in 0xC000..=0xFFFF.
        assert!(ids.iter().all(|&f| (0xC000..=0xFFFF).contains(&f)), "{ids:?}");
        let mut sorted = ids.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
    }

    #[test]
    fn own_process_exe_and_name() {
        let pid = unsafe { GetCurrentProcessId() };
        let exe = process_exe(pid).expect("own exe path");
        assert!(exe.ends_with(".exe"), "{exe}");
        assert_eq!(exe, exe.to_lowercase());
        assert!(!app_name(&exe).is_empty());
        assert_eq!(process_exe(0), None);
    }

    #[test]
    fn file_description_of_a_system_exe() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let notepad = format!("{windir}\\System32\\notepad.exe");
        if std::path::Path::new(&notepad).exists() {
            // Localized ("Notepad" / "Блокнот"), but present.
            assert!(file_description(&notepad).is_some_and(|d| !d.is_empty()));
        }
        assert_eq!(file_description("c:\\definitely\\missing.exe"), None);
    }
}
