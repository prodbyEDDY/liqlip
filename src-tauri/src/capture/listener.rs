//! Clipboard listener: a hidden message-only window (`HWND_MESSAGE`) on its own thread with
//! a message loop, registered with `AddClipboardFormatListener`; every `WM_CLIPBOARDUPDATE`
//! becomes a [`LoopMsg::Notify`] for the capture loop. The window procedure does nothing
//! else (no clipboard access, no locks that can block, no panics across the FFI boundary).
//!
//! - Message-only windows: https://learn.microsoft.com/windows/win32/winmsg/window-features#message-only-windows
//! - https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-addclipboardformatlistener
//!   ("it is posted a WM_CLIPBOARDUPDATE message whenever the contents of the clipboard have
//!   changed")
//! - https://learn.microsoft.com/windows/win32/dataxchg/wm-clipboardupdate

use super::pipeline::LoopMsg;
use super::win32::wide;
use std::cell::RefCell;
use std::sync::mpsc::Sender;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostMessageW, PostQuitMessage,
    RegisterClassExW, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WNDCLASSEXW,
};

const CLASS: &str = "LiqlipClipboardListener";
/// Private message: leave the message loop (posted by [`Listener::stop`]).
const WM_LIQLIP_STOP: u32 = WM_APP + 0x51;

thread_local! {
    /// The capture loop's sender for the window(s) of THIS thread (the wndproc runs on the
    /// thread that created the window).
    static TX: RefCell<Option<Sender<LoopMsg>>> = const { RefCell::new(None) };
}

/// A running listener: its window handle (as an integer, `Send`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listener {
    pub hwnd: isize,
    pub thread_id: u32,
}

impl Listener {
    /// Ask the listener thread to leave its loop; it then removes itself from the listener
    /// list and destroys the window.
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-postmessagew
    pub fn stop(&self) {
        // SAFETY: posting to a window of ours; fails harmlessly if it is already gone.
        let _ = unsafe { PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_LIQLIP_STOP, WPARAM(0), LPARAM(0)) };
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLIPBOARDUPDATE => {
            // `try_with` / `try_borrow`: never panic inside an extern "system" callback.
            let _ = TX.try_with(|tx| {
                if let Ok(tx) = tx.try_borrow() {
                    if let Some(tx) = tx.as_ref() {
                        let _ = tx.send(LoopMsg::Notify);
                    }
                }
            });
            LRESULT(0)
        }
        WM_LIQLIP_STOP => {
            // SAFETY: called on the window's own thread.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Removes the listener registration and destroys the window on every exit path.
struct WindowGuard {
    hwnd: HWND,
    listening: bool,
}

impl Drop for WindowGuard {
    fn drop(&mut self) {
        // SAFETY: `hwnd` was created by this thread; both calls are idempotent-safe.
        unsafe {
            if self.listening {
                let _ = RemoveClipboardFormatListener(self.hwnd);
            }
            let _ = DestroyWindow(self.hwnd);
        }
        let _ = TX.try_with(|tx| tx.try_borrow_mut().map(|mut t| *t = None));
    }
}

/// Run the listener on the CURRENT thread until [`Listener::stop`] (→ `Ok`) or a Win32
/// failure (→ `Err`, the caller's supervisor retries). `ready` is called once the window is
/// listening.
pub fn run(tx: Sender<LoopMsg>, ready: impl FnOnce(Listener)) -> Result<(), String> {
    TX.with(|t| *t.borrow_mut() = Some(tx));
    let class = wide(CLASS);
    // SAFETY: `None` = the module of the current process image.
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|e| format!("GetModuleHandleW: {e}"))?;
    let hinstance = HINSTANCE::from(module);
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-registerclassexw —
    // a restart after a panic finds the class already registered: fine.
    // SAFETY: `wc` and `class` outlive the call.
    if unsafe { RegisterClassExW(&wc) } == 0 {
        // SAFETY: no preconditions.
        let err = unsafe { GetLastError() };
        if err != ERROR_CLASS_ALREADY_EXISTS {
            return Err(format!("RegisterClassExW: {err:?}"));
        }
    }
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-createwindowexw with
    // `HWND_MESSAGE` as parent → message-only window (invisible, not enumerable).
    // SAFETY: class registered above; all pointers outlive the call.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            None,
        )
    }
    .map_err(|e| format!("CreateWindowExW: {e}"))?;
    let mut guard = WindowGuard { hwnd, listening: false };
    // SAFETY: `hwnd` is our live window.
    unsafe { AddClipboardFormatListener(hwnd) }.map_err(|e| format!("AddClipboardFormatListener: {e}"))?;
    guard.listening = true;
    // SAFETY: no preconditions.
    ready(Listener { hwnd: hwnd.0 as isize, thread_id: unsafe { GetCurrentThreadId() } });

    let mut msg = MSG::default();
    loop {
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getmessagew —
        // 0 = WM_QUIT, -1 = error.
        // SAFETY: `msg` outlives the call.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
        if r == 0 {
            break;
        }
        if r == -1 {
            return Err("GetMessageW failed".into());
        }
        // SAFETY: `msg` was filled by GetMessageW.
        unsafe { DispatchMessageW(&msg) };
    }
    drop(guard);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    /// The listener plumbing without touching clipboard content: a WM_CLIPBOARDUPDATE posted
    /// to our own window reaches the capture loop's channel; stop leaves the loop cleanly.
    #[test]
    fn notifies_and_stops() {
        let (tx, rx) = mpsc::channel();
        let (rtx, rrx) = mpsc::channel();
        let t = std::thread::spawn(move || run(tx, |l| rtx.send(l).unwrap()));
        let l = rrx.recv_timeout(Duration::from_secs(5)).expect("listener ready");
        assert_ne!(l.hwnd, 0);
        // SAFETY: posting to the test's own window.
        unsafe { PostMessageW(Some(HWND(l.hwnd as *mut _)), WM_CLIPBOARDUPDATE, WPARAM(0), LPARAM(0)) }.unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(LoopMsg::Notify));
        l.stop();
        assert_eq!(t.join().unwrap(), Ok(()));
        // A second listener on another thread can register the same class again.
        let (tx2, _rx2) = mpsc::channel();
        let (rtx2, rrx2) = mpsc::channel();
        let t2 = std::thread::spawn(move || run(tx2, |l| rtx2.send(l).unwrap()));
        rrx2.recv_timeout(Duration::from_secs(5)).unwrap().stop();
        assert_eq!(t2.join().unwrap(), Ok(()));
    }
}
