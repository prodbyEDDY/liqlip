//! `apps_running`: applications with a visible top-level window, for the "ignore an app"
//! picker in settings. One entry per exe (lower-cased full path), Liqlip itself excluded,
//! name = FileDescription, icon = the shared capture icon cache (data URL).
//!
//! Filter (what Alt+Tab would roughly show): visible, unowned, not a tool window unless
//! `WS_EX_APPWINDOW`, has a title, not DWM-cloaked (suspended UWP / other virtual desktop
//! shells). UWP apps are hosted by `ApplicationFrameHost.exe`; the real app is the child
//! window of another process.
//! - https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumwindows
//! - https://learn.microsoft.com/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute (DWMWA_CLOAKED)

use super::{icons, win32};
use crate::model::RunningApp;
use std::collections::HashSet;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetWindow, GetWindowLongPtrW, GetWindowTextLengthW, IsWindowVisible, GWL_EXSTYLE,
    GW_OWNER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Vec<isize>` passed by `top_level_windows`, alive for the
    // whole (synchronous) enumeration.
    let v = unsafe { &mut *(lparam.0 as *mut Vec<isize>) };
    v.push(hwnd.0 as isize);
    BOOL(1)
}

fn top_level_windows() -> Vec<isize> {
    let mut v: Vec<isize> = Vec::new();
    // SAFETY: the callback only pushes into `v`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut v as *mut Vec<isize> as isize)) };
    v
}

fn child_windows(parent: HWND) -> Vec<isize> {
    let mut v: Vec<isize> = Vec::new();
    // SAFETY: as above.
    let _ = unsafe { EnumChildWindows(Some(parent), Some(collect), LPARAM(&mut v as *mut Vec<isize> as isize)) };
    v
}

fn is_app_window(hwnd: HWND) -> bool {
    // SAFETY: plain queries on a window handle from EnumWindows (may be gone: then false/0).
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0 {
            return false;
        }
        if GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        let mut cloaked = 0u32;
        let r = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        !(r.is_ok() && cloaked != 0)
    }
}

/// The process that really owns an app window (UWP frame → hosted app).
fn app_pid(hwnd: HWND, exe: &str) -> u32 {
    let pid = win32::window_pid(hwnd);
    if exe.ends_with("\\applicationframehost.exe") {
        for child in child_windows(hwnd) {
            let cpid = win32::window_pid(HWND(child as *mut _));
            if cpid != 0 && cpid != pid {
                return cpid;
            }
        }
    }
    pid
}

/// Running apps, sorted by name. Blocking (icon extraction): call off the UI thread.
pub fn running() -> Vec<RunningApp> {
    // SAFETY: no preconditions.
    let own = unsafe { GetCurrentProcessId() };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in top_level_windows() {
        let hwnd = HWND(raw as *mut _);
        if !is_app_window(hwnd) {
            continue;
        }
        let Some(frame_exe) = win32::process_exe(win32::window_pid(hwnd)) else { continue };
        let pid = app_pid(hwnd, &frame_exe);
        if pid == own {
            continue;
        }
        let Some(exe) = (if pid == win32::window_pid(hwnd) { Some(frame_exe) } else { win32::process_exe(pid) }) else {
            continue;
        };
        if !seen.insert(exe.clone()) {
            continue;
        }
        out.push(RunningApp { name: win32::app_name(&exe), icon_url: icons::data_url(&exe), exe });
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_apps_are_unique_and_exclude_self() {
        let apps = running();
        let own = win32::process_exe(unsafe { GetCurrentProcessId() }).unwrap_or_default();
        let mut exes: Vec<&str> = apps.iter().map(|a| a.exe.as_str()).collect();
        let n = exes.len();
        exes.sort();
        exes.dedup();
        assert_eq!(exes.len(), n, "one entry per exe");
        assert!(apps.iter().all(|a| a.exe != own && !a.name.is_empty() && a.exe == a.exe.to_lowercase()));
    }
}
