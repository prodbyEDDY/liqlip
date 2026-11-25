//! Thin Win32 helpers for the shelf and the HUD (thread-safe calls only; window state that
//! tao tracks is changed through Tauri, on the main thread). Each call links its doc.

use super::geometry::Rect;

pub type Hwnd = isize;

#[cfg(windows)]
mod imp {
    use super::{Hwnd, Rect};
    use std::collections::HashMap;
    use windows::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
        DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
    };
    use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, POINT};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetClassNameW, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
        GetWindowLongPtrW, GetWindowThreadProcessId, IsWindow, IsWindowVisible, SetForegroundWindow,
        SetWindowLongPtrW, SetWindowPos, ShowWindow, WindowFromPoint, GA_ROOT, GWL_EXSTYLE,
        HWND_TOPMOST, SM_CMONITORS, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SWP_SHOWWINDOW, SW_HIDE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    fn hwnd(h: Hwnd) -> HWND {
        HWND(h as *mut core::ffi::c_void)
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getforegroundwindow
    pub fn foreground() -> Option<Hwnd> {
        // SAFETY: no arguments; NULL when no window is foreground (e.g. mid-switch).
        let h = unsafe { GetForegroundWindow() };
        (!h.is_invalid()).then_some(h.0 as Hwnd)
    }

    /// `SetForegroundWindow`, then confirm. Allowed when "the calling process received the
    /// last input event" — a WM_HOTKEY / a click / a tray menu pick — or is foreground; a
    /// refusal is reported, never forced (no Alt-key injection, no lock-timeout changes).
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow
    pub fn set_foreground(h: Hwnd) -> bool {
        // SAFETY: plain window handle; failure is a FALSE return.
        let ok = unsafe { SetForegroundWindow(hwnd(h)).as_bool() };
        ok && foreground() == Some(h)
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindow
    pub fn is_window(h: Hwnd) -> bool {
        // SAFETY: any handle value is accepted.
        unsafe { IsWindow(Some(hwnd(h))).as_bool() }
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindowvisible
    pub fn is_visible(h: Hwnd) -> bool {
        // SAFETY: any handle value is accepted.
        unsafe { IsWindowVisible(hwnd(h)).as_bool() }
    }

    /// Is the window owned by this process (an own dialog, the settings window)?
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid
    pub fn is_own_process(h: Hwnd) -> bool {
        let mut pid = 0u32;
        // SAFETY: `pid` outlives the call.
        unsafe {
            GetWindowThreadProcessId(hwnd(h), Some(&mut pid));
            pid != 0 && pid == GetCurrentProcessId()
        }
    }

    /// The taskbar / its overflow: clicking the tray icon makes these foreground; they are
    /// not where focus should return to.
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getclassnamew
    pub fn is_shell_tray(h: Hwnd) -> bool {
        let mut buf = [0u16; 64];
        // SAFETY: buffer length is passed by the wrapper.
        let n = unsafe { GetClassNameW(hwnd(h), &mut buf) };
        let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        matches!(
            class.as_str(),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "NotifyIconOverflowWindow" | "TopLevelWindowForOverflowXamlIsland"
        )
    }

    /// Keep a window out of Alt+Tab: `WS_EX_TOOLWINDOW` on, `WS_EX_APPWINDOW` off ("A tool
    /// window does not appear in the taskbar or in the dialog that appears when the user
    /// presses ALT+TAB"; APPWINDOW would force it back). tao rewrites GWL_EXSTYLE from its
    /// own flags on show/hide/focusable/click-through changes (window_state.rs `apply_diff`),
    /// so this runs before and after each of them. SetWindowPos(SWP_FRAMECHANGED) flushes the
    /// cached style ("changes … will not take effect until you call SetWindowPos").
    /// https://learn.microsoft.com/windows/win32/winmsg/extended-window-styles
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowlongptrw
    pub fn apply_toolwindow(h: Hwnd) {
        // SAFETY: own window; style read-modify-write.
        unsafe {
            let ex = GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE);
            let want = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
            if want != ex {
                SetWindowLongPtrW(hwnd(h), GWL_EXSTYLE, want);
                let _ = SetWindowPos(
                    hwnd(h),
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
    }

    /// Show above everything WITHOUT activating (SWP_NOACTIVATE | SWP_SHOWWINDOW).
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowpos
    pub fn show_no_activate(h: Hwnd) {
        // SAFETY: own window.
        let _ = unsafe {
            SetWindowPos(hwnd(h), Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW)
        };
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-showwindow
    pub fn hide(h: Hwnd) {
        // SAFETY: own window.
        let _ = unsafe { ShowWindow(hwnd(h), SW_HIDE) };
    }

    /// Physical cursor position (the process is Per-Monitor-V2 aware).
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getcursorpos
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut pt = POINT::default();
        // SAFETY: `pt` outlives the call.
        unsafe { GetCursorPos(&mut pt) }.ok()?;
        Some((pt.x, pt.y))
    }

    /// Any physical mouse button down (left/right/middle). "GetAsyncKeyState … checks on the
    /// state of the physical mouse buttons", high bit = down.
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getasynckeystate
    pub fn mouse_buttons_down() -> bool {
        // SAFETY: plain key-state queries (VK_LBUTTON 0x01, VK_RBUTTON 0x02, VK_MBUTTON 0x04).
        [0x01, 0x02, 0x04].iter().any(|&vk| unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0)
    }

    /// Top-level window under a screen point (the WebView2 child resolves to its host).
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-windowfrompoint
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getancestor
    pub fn root_window_at(x: i32, y: i32) -> Option<Hwnd> {
        // SAFETY: plain point query.
        unsafe {
            let h = WindowFromPoint(POINT { x, y });
            if h.is_invalid() {
                return None;
            }
            let root = GetAncestor(h, GA_ROOT);
            Some(if root.is_invalid() { h.0 as Hwnd } else { root.0 as Hwnd })
        }
    }

    /// Everything that changes the dock for a monitor: number of monitors, the virtual
    /// screen, and rcMonitor / rcWork / effective DPI of the monitor at `screen`'s centre.
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getsystemmetrics
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getmonitorinfow
    /// https://learn.microsoft.com/windows/win32/api/shellscalingapi/nf-shellscalingapi-getdpiformonitor
    pub fn display_signature(screen: &Rect) -> [i32; 14] {
        // SAFETY: metric/monitor queries with stack out-params.
        unsafe {
            let mut sig = [0i32; 14];
            sig[0] = GetSystemMetrics(SM_CMONITORS);
            sig[1] = GetSystemMetrics(SM_XVIRTUALSCREEN);
            sig[2] = GetSystemMetrics(SM_YVIRTUALSCREEN);
            sig[3] = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            sig[4] = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            let centre = POINT { x: (screen.left + screen.right) / 2, y: (screen.top + screen.bottom) / 2 };
            let hmon = MonitorFromPoint(centre, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if GetMonitorInfoW(hmon, &mut mi).as_bool() {
                let (m, w) = (mi.rcMonitor, mi.rcWork);
                sig[5..13].copy_from_slice(&[m.left, m.top, m.right, m.bottom, w.left, w.top, w.right, w.bottom]);
            }
            let (mut dx, mut dy) = (0u32, 0u32);
            if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() {
                sig[13] = dx as i32;
            }
            sig
        }
    }

    fn wide(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    /// GDI device name (`\\.\DISPLAY1`) → monitor's EDID friendly name ("DELL U2723QE"),
    /// via the CCD API: QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS), then per path
    /// DisplayConfigGetDeviceInfo GET_SOURCE_NAME (viewGdiDeviceName) + GET_TARGET_NAME
    /// (monitorFriendlyDeviceName — empty when unknown).
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-querydisplayconfig
    /// https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-displayconfig_target_device_name
    pub fn monitor_friendly_names() -> HashMap<String, String> {
        let mut out = HashMap::new();
        // SAFETY: buffers sized by GetDisplayConfigBufferSizes; header sizes are the struct sizes.
        unsafe {
            let (mut np, mut nm) = (0u32, 0u32);
            for _ in 0..3 {
                if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) != ERROR_SUCCESS {
                    return out;
                }
                let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
                let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
                let r = QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut np, paths.as_mut_ptr(), &mut nm, modes.as_mut_ptr(), None);
                if r != ERROR_SUCCESS {
                    // ERROR_INSUFFICIENT_BUFFER: the topology changed between the calls — retry.
                    continue;
                }
                paths.truncate(np as usize);
                for p in &paths {
                    let mut src = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
                    src.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
                    src.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
                    src.header.adapterId = p.sourceInfo.adapterId;
                    src.header.id = p.sourceInfo.id;
                    if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
                        continue;
                    }
                    let mut tgt = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
                    tgt.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
                    tgt.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
                    tgt.header.adapterId = p.targetInfo.adapterId;
                    tgt.header.id = p.targetInfo.id;
                    if DisplayConfigGetDeviceInfo(&mut tgt.header) != 0 {
                        continue;
                    }
                    let name = wide(&tgt.monitorFriendlyDeviceName);
                    if !name.is_empty() {
                        // A cloned source has several targets: keep the first name.
                        out.entry(wide(&src.viewGdiDeviceName)).or_insert(name);
                    }
                }
                break;
            }
        }
        out
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Hwnd, Rect};
    use std::collections::HashMap;
    pub fn foreground() -> Option<Hwnd> {
        None
    }
    pub fn set_foreground(_: Hwnd) -> bool {
        false
    }
    pub fn is_window(_: Hwnd) -> bool {
        false
    }
    pub fn is_visible(_: Hwnd) -> bool {
        false
    }
    pub fn is_own_process(_: Hwnd) -> bool {
        false
    }
    pub fn is_shell_tray(_: Hwnd) -> bool {
        false
    }
    pub fn apply_toolwindow(_: Hwnd) {}
    pub fn show_no_activate(_: Hwnd) {}
    pub fn hide(_: Hwnd) {}
    pub fn cursor_pos() -> Option<(i32, i32)> {
        None
    }
    pub fn mouse_buttons_down() -> bool {
        false
    }
    pub fn root_window_at(_: i32, _: i32) -> Option<Hwnd> {
        None
    }
    pub fn display_signature(_: &Rect) -> [i32; 14] {
        [0; 14]
    }
    pub fn monitor_friendly_names() -> HashMap<String, String> {
        HashMap::new()
    }
}

pub use imp::*;
