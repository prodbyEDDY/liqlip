//! Paste target (ARCHITECTURE §6 "Цель") — owner **W1-C**.
//!
//! `capture_target` runs when the shelf is summoned, BEFORE it shows: the foreground window
//! becomes the target unless it is ours, the desktop or the taskbar. The rule for rejected
//! windows (SPEC §3.1, §6.1):
//! - the **shelf itself** is in front (a re-summon during its closing animation, a re-show
//!   after `keepOpen`) → the remembered target is kept — the user's context did not change,
//!   and it is never overwritten with nothing (the Liquid Tools bug);
//! - any other own window (settings, tray menu), the desktop (`Progman`/`WorkerW`), the taskbar
//!   (`Shell_TrayWnd`/`Shell_SecondaryTrayWnd`, tray overflow), an invisible or minimized window
//!   → no target (`None`): Enter copies.
//!
//! Focus is handed back with `SetForegroundWindow` only — no `AttachThreadInput`, no
//! `SPI_SETFOREGROUNDLOCKTIMEOUT` (see [`bring_to_front`]).

use crate::model::TargetInfo;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

/// The remembered target: window, owning process, description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub hwnd: isize,
    pub pid: u32,
    pub info: TargetInfo,
}

static TARGET: Mutex<Option<Target>> = Mutex::new(None);
static HANDOFF: AtomicBool = AtomicBool::new(false);

/// What to do with the foreground window at summon time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// A real application window → new target.
    Accept,
    /// The shelf itself is in front → keep the remembered target.
    KeepPrevious,
    /// Desktop / taskbar / other own window / hidden / minimized → no target.
    NoTarget,
}

/// Shell surfaces that are never a paste target: the desktop and the taskbars (+ the tray
/// overflow flyouts of Windows 10 and 11).
pub fn is_shell_class(class: &str) -> bool {
    matches!(
        class,
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "TopLevelWindowForOverflowXamlIsland"
    )
}

pub fn classify(is_picker: bool, own_process: bool, class: &str, visible: bool, minimized: bool) -> Verdict {
    if is_picker {
        Verdict::KeepPrevious
    } else if own_process || is_shell_class(class) || !visible || minimized {
        Verdict::NoTarget
    } else {
        Verdict::Accept
    }
}

/// Called by the shelf (`picker::summon`, W1-D) BEFORE it shows itself: remember the
/// foreground window as the paste target and describe it. Own windows, the desktop and
/// the shell tray are rejected → `None`, and the previously remembered target is NOT
/// overwritten with nothing.
#[allow(dead_code)] // W1-D calls it from `picker::summon`
pub fn capture_target(app: &AppHandle) -> Option<TargetInfo> {
    #[cfg(windows)]
    {
        let fg = win::foreground();
        let verdict = if fg == 0 {
            Verdict::NoTarget
        } else {
            let probe = win::probe(fg);
            classify(is_picker(app, fg), probe.own_process, &probe.class, probe.visible, probe.minimized)
        };
        match verdict {
            Verdict::Accept => {
                let target = win::describe(fg);
                let info = target.as_ref().map(|t| t.info.clone());
                *TARGET.lock() = target;
                info
            }
            Verdict::KeepPrevious => current_target(app),
            Verdict::NoTarget => {
                *TARGET.lock() = None;
                None
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        None
    }
}

/// The remembered target, re-validated (window still alive). `picker_target` and the
/// footer hint use it.
pub fn current_target(app: &AppHandle) -> Option<TargetInfo> {
    let _ = app;
    current().map(|t| t.info)
}

/// The remembered target if its window is still alive, visible, not minimized and owned by
/// the same process (a reused HWND is not the same target).
pub fn current() -> Option<Target> {
    let t = TARGET.lock().clone()?;
    if is_usable(&t) {
        Some(t)
    } else {
        None
    }
}

/// Describe an arbitrary window as a target (the stack hotkey pastes into the CURRENT
/// foreground window). `None` for own / shell / hidden / minimized windows.
pub fn target_for(app: &AppHandle, hwnd: isize) -> Option<Target> {
    #[cfg(windows)]
    {
        if hwnd == 0 {
            return None;
        }
        let probe = win::probe(hwnd);
        match classify(is_picker(app, hwnd), probe.own_process, &probe.class, probe.visible, probe.minimized) {
            Verdict::Accept => win::describe(hwnd),
            _ => None,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (app, hwnd);
        None
    }
}

/// `true` while a `keepOpen` paste has moved focus from the shelf to the target and back.
/// The shelf's focus-loss exit (W1-D, `WindowEvent::Focused(false)`) must ignore that
/// deactivation while this is set.
#[allow(dead_code)] // W1-D reads it in its Focused(false) handler
pub fn handoff_active() -> bool {
    HANDOFF.load(Ordering::SeqCst)
}

pub(crate) fn set_handoff(on: bool) {
    HANDOFF.store(on, Ordering::SeqCst);
}

fn is_usable(t: &Target) -> bool {
    #[cfg(windows)]
    {
        win::is_alive(t.hwnd, t.pid)
    }
    #[cfg(not(windows))]
    {
        let _ = t;
        false
    }
}

/// The shelf window's HWND (cached: pre-created windows are never destroyed).
pub fn picker_hwnd(app: &AppHandle) -> Option<isize> {
    use std::sync::OnceLock;
    use tauri::Manager;
    static PICKER: OnceLock<isize> = OnceLock::new();
    if let Some(h) = PICKER.get() {
        return Some(*h);
    }
    let win = app.get_webview_window(crate::events::PICKER)?;
    #[cfg(windows)]
    {
        let h = win.hwnd().ok()?.0 as isize;
        if h != 0 {
            let _ = PICKER.set(h);
            return Some(h);
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = win;
        None
    }
}

fn is_picker(app: &AppHandle, hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        picker_hwnd(app).is_some_and(|p| p == hwnd || win::root_owner(hwnd) == p)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, hwnd);
        false
    }
}

#[cfg(windows)]
pub use win::{bring_to_front, foreground};

#[cfg(windows)]
mod win {
    use super::Target;
    use crate::model::TargetInfo;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    use windows::core::{BOOL, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
        IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow, GA_ROOTOWNER,
    };

    fn h(hwnd: isize) -> HWND {
        HWND(hwnd as *mut _)
    }

    pub fn foreground() -> isize {
        // SAFETY: no preconditions.
        unsafe { GetForegroundWindow().0 as isize }
    }

    fn pid_of(hwnd: isize) -> u32 {
        let mut pid = 0u32;
        // SAFETY: out-pointer to a local.
        unsafe { GetWindowThreadProcessId(h(hwnd), Some(&mut pid)) };
        pid
    }

    pub fn root_owner(hwnd: isize) -> isize {
        // SAFETY: plain query; NULL for an invalid handle.
        unsafe { GetAncestor(h(hwnd), GA_ROOTOWNER).0 as isize }
    }

    fn class_name(hwnd: isize) -> String {
        let mut buf = [0u16; 256];
        // SAFETY: buffer length is passed by the wrapper.
        let n = unsafe { GetClassNameW(h(hwnd), &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    /// Cheap facts for [`super::classify`].
    pub struct Probe {
        pub own_process: bool,
        pub class: String,
        pub visible: bool,
        pub minimized: bool,
    }

    pub fn probe(hwnd: isize) -> Probe {
        // SAFETY: plain window queries.
        let me = unsafe { GetCurrentProcessId() };
        let root = root_owner(hwnd);
        // Own process: the window itself or its root owner (WebView2 popups owned by our
        // windows, our tray menu's hidden window).
        let own_process = pid_of(hwnd) == me || (root != 0 && pid_of(root) == me);
        unsafe {
            Probe {
                own_process,
                class: class_name(hwnd),
                visible: IsWindowVisible(h(hwnd)).as_bool(),
                minimized: IsIconic(h(hwnd)).as_bool(),
            }
        }
    }

    pub fn is_alive(hwnd: isize, pid: u32) -> bool {
        // SAFETY: plain window queries; IsWindow first so the others see a live handle.
        unsafe {
            IsWindow(Some(h(hwnd))).as_bool() && pid_of(hwnd) == pid && IsWindowVisible(h(hwnd)).as_bool() && !IsIconic(h(hwnd)).as_bool()
        }
    }

    /// Window title via GetWindowTextW, which for another process's window reads the stored
    /// caption without sending WM_GETTEXT — a hung target cannot block us
    /// (learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowtextw).
    fn title(hwnd: isize) -> String {
        let mut buf = vec![0u16; 512];
        // SAFETY: buffer length is passed by the wrapper.
        let n = unsafe { GetWindowTextW(h(hwnd), &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    /// Full image path of a process (`QueryFullProcessImageNameW`, Win32 format).
    fn exe_path(pid: u32) -> Option<String> {
        // SAFETY: OpenProcess/QueryFullProcessImageNameW with a sized buffer; handle closed.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = vec![0u16; 32768];
            let mut len = buf.len() as u32;
            let r = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(process);
            r.ok()?;
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    struct ChildSearch {
        frame_pid: u32,
        found: u32,
    }

    unsafe extern "system" fn find_other_pid(child: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam is the &mut ChildSearch passed below, alive for the enumeration.
        let search = unsafe { &mut *(lparam.0 as *mut ChildSearch) };
        let pid = pid_of(child.0 as isize);
        if pid != 0 && pid != search.frame_pid {
            search.found = pid;
            return BOOL(0); // stop
        }
        BOOL(1)
    }

    /// UWP apps live in an `ApplicationFrameHost.exe` frame whose child CoreWindow belongs to
    /// the real app process — describe that one (name, icon, integrity level).
    fn uwp_app_pid(hwnd: isize, frame_pid: u32) -> Option<u32> {
        let mut search = ChildSearch { frame_pid, found: 0 };
        // SAFETY: callback + pointer to a local that outlives the synchronous enumeration.
        unsafe {
            let _ = EnumChildWindows(Some(h(hwnd)), Some(find_other_pid), LPARAM(&mut search as *mut _ as isize));
        }
        (search.found != 0).then_some(search.found)
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// `FileDescription` from the version resource (first translation, then en-US) —
    /// learn.microsoft.com/windows/win32/api/winver/nf-winver-verqueryvaluew.
    fn file_description(exe: &str) -> Option<String> {
        let path = wide(exe);
        // SAFETY: size query + buffer of that size; VerQueryValueW returns pointers into it,
        // read only while `data` lives.
        unsafe {
            let size = GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None);
            if size == 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            GetFileVersionInfoW(PCWSTR(path.as_ptr()), None, size, data.as_mut_ptr().cast()).ok()?;
            let mut langs: Vec<String> = Vec::new();
            let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut len = 0u32;
            let sub = wide("\\VarFileInfo\\Translation");
            if VerQueryValueW(data.as_ptr().cast(), PCWSTR(sub.as_ptr()), &mut ptr, &mut len).as_bool() && len >= 4 {
                let pairs = std::slice::from_raw_parts(ptr as *const u16, (len / 2) as usize);
                for p in pairs.chunks_exact(2) {
                    langs.push(format!("{:04x}{:04x}", p[0], p[1]));
                }
            }
            langs.extend(["040904b0".to_string(), "040904e4".to_string()]);
            for lang in langs {
                let sub = wide(&format!("\\StringFileInfo\\{lang}\\FileDescription"));
                let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
                let mut len = 0u32;
                if VerQueryValueW(data.as_ptr().cast(), PCWSTR(sub.as_ptr()), &mut ptr, &mut len).as_bool() && len > 0 {
                    let s = std::slice::from_raw_parts(ptr as *const u16, len as usize);
                    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
                    let text = String::from_utf16_lossy(&s[..end]).trim().to_string();
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
            None
        }
    }

    /// Display name for an exe, cached per exe (the version resource is read once).
    fn app_name(exe: &str) -> String {
        static NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
        let key = exe.to_lowercase();
        if let Some(n) = NAMES.lock().as_ref().and_then(|m| m.get(&key)) {
            return n.clone();
        }
        let name = file_description(exe).unwrap_or_else(|| {
            let base = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
            base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base).to_string()
        });
        NAMES.lock().get_or_insert_with(HashMap::new).insert(key, name.clone());
        name
    }

    /// Full description of an accepted window.
    pub fn describe(hwnd: isize) -> Option<Target> {
        let frame_pid = pid_of(hwnd);
        if frame_pid == 0 {
            return None;
        }
        let mut app_pid = frame_pid;
        let mut exe = exe_path(frame_pid).unwrap_or_default();
        if super::super::input::exe_basename(&exe) == "applicationframehost.exe" {
            if let Some(pid) = uwp_app_pid(hwnd, frame_pid) {
                if let Some(path) = exe_path(pid) {
                    app_pid = pid;
                    exe = path;
                }
            }
        }
        let title = title(hwnd);
        let name = if exe.is_empty() { title.clone() } else { app_name(&exe) };
        let exe_lower = exe.to_lowercase();
        let icon_url = if exe.is_empty() { None } else { crate::capture::app_icon_data_url(&exe_lower) };
        let elevated = super::super::uipi::target_is_elevated(app_pid);
        Some(Target {
            hwnd,
            pid: frame_pid,
            info: TargetInfo { exe: exe_lower, name, title, icon_url, elevated },
        })
    }

    fn wait_foreground(hwnd: isize, deadline: Instant) -> bool {
        loop {
            if foreground() == hwnd {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Make `hwnd` the foreground window and wait (10 ms steps, ≤ `timeout`) until
    /// `GetForegroundWindow` confirms it.
    ///
    /// `SetForegroundWindow` is allowed here because, per its documented conditions, "the
    /// calling process is the foreground process" (the shelf was active) or "the calling
    /// process received the last input event" (the Enter / click that triggered the paste)
    /// (learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow).
    /// If the first call loses a race with the system's own activation of the next window
    /// after the shelf hides, ONE more `SetForegroundWindow` follows halfway through the
    /// budget. Deliberately not used: `AttachThreadInput` to a foreign thread (merges input
    /// state with another process) and `SPI_SETFOREGROUNDLOCKTIMEOUT` (a system-wide setting —
    /// CLAUDE.md hard rule 6). A minimized target is not restored behind the user's back.
    pub fn bring_to_front(hwnd: isize, timeout: Duration) -> bool {
        // SAFETY: plain window calls on a handle validated by the caller.
        unsafe {
            if !IsWindow(Some(h(hwnd))).as_bool() || IsIconic(h(hwnd)).as_bool() {
                return false;
            }
            if foreground() == hwnd {
                return true;
            }
            let start = Instant::now();
            let _ = SetForegroundWindow(h(hwnd));
            if wait_foreground(hwnd, start + timeout / 2) {
                return true;
            }
            let _ = SetForegroundWindow(h(hwnd));
            wait_foreground(hwnd, start + timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_surfaces_are_rejected() {
        for class in ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd", "NotifyIconOverflowWindow"] {
            assert!(is_shell_class(class), "{class}");
            assert_eq!(classify(false, false, class, true, false), Verdict::NoTarget);
        }
        assert!(!is_shell_class("Notepad"));
        assert!(!is_shell_class("Chrome_WidgetWin_1"));
    }

    #[test]
    fn own_windows_desktop_hidden_minimized() {
        // The shelf itself in front → keep the remembered target (never zero it).
        assert_eq!(classify(true, true, "Tauri Window", true, false), Verdict::KeepPrevious);
        // Settings window / tray menu of our process → no target.
        assert_eq!(classify(false, true, "Tauri Window", true, false), Verdict::NoTarget);
        assert_eq!(classify(false, true, "#32768", true, false), Verdict::NoTarget);
        assert_eq!(classify(false, false, "Notepad", false, false), Verdict::NoTarget, "invisible");
        assert_eq!(classify(false, false, "Notepad", true, true), Verdict::NoTarget, "minimized");
        assert_eq!(classify(false, false, "Notepad", true, false), Verdict::Accept);
        assert_eq!(classify(false, false, "CASCADIA_HOSTING_WINDOW_CLASS", true, false), Verdict::Accept);
    }

    #[test]
    fn handoff_flag_round_trip() {
        set_handoff(true);
        assert!(handoff_active());
        set_handoff(false);
        assert!(!handoff_active());
    }
}
