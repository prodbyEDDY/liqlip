//! Screenshot windows (owner: W6-B; ARCHITECTURE §18.2). W5-A put the shared Win32 helper
//! here ([`exclude_from_capture`], used by `lib.rs` for the pre-created `capture` window);
//! W6-B adds the on-demand builders for `shots`, `editor` and `pin-<n>` — from an `async`
//! command or a spawned thread only (docs.rs tauri 2.11.2, `WebviewWindowBuilder::new`, Known
//! issues: "On Windows, this function deadlocks when used in a synchronous command and event
//! handlers … You should use `async` commands and separate threads when creating windows").

use tauri::{AppHandle, WebviewWindow};

/// `setup()` step 5 (via `screenshot::init`): start-up work of the window side. No-op until W6-B.
pub(crate) fn init(_app: &AppHandle) {}

/// Keep a window out of every screen capture — ours (the GDI freeze must not see the overlay
/// or the QAO) and other apps' recordings: `SetWindowDisplayAffinity(hwnd,
/// WDA_EXCLUDEFROMCAPTURE)`
/// (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity).
/// The window stays visible on the monitor. Supported from Windows 10 2004; on older builds
/// Windows applies `WDA_MONITOR` instead (the window shows as a black box in captures). It
/// works while DWM composes the desktop (always on Windows 10/11) and only for a top-level
/// window of this process. No new `windows` feature: both live in
/// `Win32_UI_WindowsAndMessaging` (windows 0.62.2).
pub(crate) fn exclude_from_capture(win: &WebviewWindow) -> Result<(), String> {
    #[cfg(windows)]
    {
        use ::windows::Win32::Foundation::HWND;
        use ::windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE};
        let h = win.hwnd().map_err(|e| format!("failed: {e}"))?.0 as isize;
        // SAFETY: `h` is the live top-level HWND tauri just created for this process; the call
        // only stores the affinity on it.
        unsafe { SetWindowDisplayAffinity(HWND(h as *mut core::ffi::c_void), WDA_EXCLUDEFROMCAPTURE) }
            .map_err(|e| format!("failed: SetWindowDisplayAffinity: {e}"))
    }
    #[cfg(not(windows))]
    {
        let _ = win;
        Ok(())
    }
}
