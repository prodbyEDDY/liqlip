//! Screenshot windows (owner: W6-B; ARCHITECTURE §18.2): the shared helpers of the three
//! on-demand windows `shots` (QAO, `qao.rs`), `editor` (`editor.rs`) and `pin-<n>` (`pin.rs`),
//! plus [`exclude_from_capture`] (W5-A), which `lib.rs` also uses for the pre-created `capture`.
//!
//! **Building on demand.** docs.rs tauri 2.11.2, `WebviewWindowBuilder::new`, Known issues:
//! "On Windows, this function deadlocks when used in a synchronous command and event handlers,
//! see [the Webview2 issue]. You should use `async` commands and separate threads when creating
//! windows." (the same text in the crate source we build against, `webview/webview_window.rs`).
//! So these windows are built only from an `async` command (`shot_edit`, `shot_pin`) or from a
//! spawned thread (the QAO), never on the main thread, in a window event handler, a hotkey or a
//! tray callback. [`may_build`] enforces it at run time: `setup()` runs on the main thread and
//! records its id in [`init`]; a build requested there is refused instead of hanging the app.
//! No lock of ours is held while a window is being built (the build waits for the main thread).
//!
//! **Geometry** is physical px (the process is Per-Monitor-V2, ARCHITECTURE §7.4) and is
//! applied position → size → position: moving onto a monitor with another scale makes tao
//! apply `WM_DPICHANGED`'s suggested rect, which would rescale a size set before the move.

use crate::model::ThemePref;
use crate::settings;
use std::sync::OnceLock;
use std::thread::ThreadId;
use tauri::{AppHandle, PhysicalPosition, PhysicalSize, Theme, Url, WebviewWindow};

static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// WebView2 browser arguments of EVERY window of the process (`lib.rs` and the three on-demand
/// windows here). All windows share one WebView2 environment (one data directory), and wry
/// 0.55.1 warns: "Webview instances with different browser arguments must also have different
/// data directories" — so the string is identical everywhere. Setting it replaces wry's default
/// (`webview2/mod.rs::create_environment`: `--disable-features=msWebOOUI,msPdfOOUI,
/// msSmartScreenProtection`, plus `--autoplay-policy=no-user-gesture-required` only when wry's
/// `autoplay` is on, which Tauri 2.11.2 never sets), so the default is kept verbatim and the
/// autoplay switch is added: the QAO's shutter sound (Web Audio, `shots/shutter.ts`) plays in a
/// window that is never activated, so it never gets a user gesture (Chromium autoplay policy).
/// Switches are space-separated (learn.microsoft.com `ICoreWebView2EnvironmentOptions::
/// put_AdditionalBrowserArguments`); tauri docs: `WebviewWindowBuilder::additional_browser_args`.
pub(crate) const BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

/// Navigation guard of EVERY window of the process (`lib.rs` and the three on-demand windows):
/// `WebviewWindowBuilder::on_navigation` (docs.rs tauri 2.11.2: "Returning `false` cancels the
/// navigation"). Only the app's own pages may load — `http(s)://tauri.localhost/…` (the bundled
/// frontend on Windows: tauri 2.11.2 `manager/mod.rs::tauri_protocol_url`) and, in a dev build,
/// the `devUrl` origin. Why: windows built with `disable_drag_drop_handler()` (shelf, settings,
/// capture overlay, QAO) leave WebView2's own external drop on (`AllowExternalDrop`, default
/// TRUE — learn.microsoft.com `ICoreWebView2Controller4`; wry turns it off only for its own
/// handler, `webview2/mod.rs`), and a file or link dropped on a page that does not handle the drop
/// navigates the webview to it — a QAO card dragged out and released back over the stack would
/// replace the QAO page with the PNG, and the page would never claim another shot. Our pages never
/// navigate anywhere else (no links, no `location` changes), so nothing legitimate is refused.
/// A refusal is logged by scheme only (a dropped path may name user content).
pub(crate) fn navigation_guard(app: &AppHandle) -> impl Fn(&Url) -> bool + Send + 'static {
    let dev = app.config().build.dev_url.clone();
    move |url| {
        let ok = own_page(url, dev.as_ref());
        if !ok {
            crate::logx::warn("windows", &format!("navigation refused (scheme {})", url.scheme()));
        }
        ok
    }
}

fn own_page(url: &Url, dev: Option<&Url>) -> bool {
    match url.scheme() {
        "http" | "https" if url.host_str() == Some("tauri.localhost") => true,
        "about" => url.path() == "blank",
        _ => cfg!(dev) && dev.is_some_and(|d| d.origin() == url.origin()),
    }
}

/// `setup()` step 5 (via `screenshot::init`, on the main thread): remember the main thread,
/// clear stale temp shots (`qao.rs`), follow the language in the editor title.
pub(crate) fn init(app: &AppHandle) {
    let _ = MAIN_THREAD.set(std::thread::current().id());
    super::qao::init(app);
    super::editor::init(app);
}

/// Is this the event-loop thread (where `setup()` ran)?
pub(crate) fn on_main_thread() -> bool {
    MAIN_THREAD.get().is_some_and(|id| *id == std::thread::current().id())
}

/// `Err` on the main thread: building a webview window there deadlocks on Windows (Known issues
/// above). Callers on another thread may build.
pub(crate) fn may_build() -> Result<(), String> {
    if on_main_thread() {
        Err("failed: a window cannot be built on the main thread".into())
    } else {
        Ok(())
    }
}

/// The native theme of a new window (title bar, scrollbars, `prefers-color-scheme`) — the same
/// mapping `theme::apply` pushes to the windows that existed at start-up: light / dark force it,
/// system = `None` (follow Windows). Content theme and language come from the page itself
/// (`initAppearance()`: `settings_get` + `settings:changed`), as in every other window.
pub(crate) fn native_theme(app: &AppHandle) -> Option<Theme> {
    match settings::get(app).general.theme {
        ThemePref::Light => Some(Theme::Light),
        ThemePref::Dark => Some(Theme::Dark),
        ThemePref::System => None,
    }
}

// ── Monitors and rectangles (physical px) ───────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PxRect {
    pub(crate) fn contains(&self, (px, py): (i32, i32)) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

/// A monitor as the windows need it: GDI device name (`\\.\DISPLAY1` — `MonitorFrame.id`,
/// `ShotAdded.monitor`), bounds, work area (`rcWork`: without the taskbar), scale factor.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mon {
    pub name: Option<String>,
    pub rect: PxRect,
    pub work: PxRect,
    pub scale: f64,
}

/// Every monitor (Tauri `available_monitors`: tao `szDevice` as the name, `rcMonitor`,
/// `rcWork`, the effective scale). Empty when the query fails.
pub(crate) fn monitors(app: &AppHandle) -> Vec<Mon> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let (p, s, wa) = (m.position(), m.size(), m.work_area());
            Mon {
                name: m.name().cloned(),
                rect: PxRect { x: p.x, y: p.y, w: s.width as i32, h: s.height as i32 },
                work: PxRect {
                    x: wa.position.x,
                    y: wa.position.y,
                    w: wa.size.width as i32,
                    h: wa.size.height as i32,
                },
                scale: if m.scale_factor() > 0.0 { m.scale_factor() } else { 1.0 },
            }
        })
        .collect()
}

/// The monitor a window goes to: the one named `name` (a shot's monitor), else the one under
/// `point` (the cursor; the nearest one in a gap between monitors), else the first (Tauri lists
/// the primary monitor first only by chance — callers pass the cursor whenever they can).
pub(crate) fn pick_monitor<'a>(mons: &'a [Mon], name: Option<&str>, point: Option<(i32, i32)>) -> Option<&'a Mon> {
    if let Some(n) = name {
        if let Some(m) = mons.iter().find(|m| m.name.as_deref() == Some(n)) {
            return Some(m);
        }
    }
    if let Some(p) = point {
        if let Some(m) = mons.iter().find(|m| m.rect.contains(p)) {
            return Some(m);
        }
        let dist = |m: &Mon| {
            let dx = (m.rect.x - p.0).max(0).max(p.0 - (m.rect.x + m.rect.w - 1));
            let dy = (m.rect.y - p.1).max(0).max(p.1 - (m.rect.y + m.rect.h - 1));
            i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy)
        };
        return mons.iter().min_by_key(|m| dist(m));
    }
    mons.first()
}

/// Logical px → physical px of a monitor.
pub(crate) fn px(logical: f64, scale: f64) -> i32 {
    (logical * scale).round() as i32
}

/// Position → size → position (physical px). Each call is posted to the main thread by tao;
/// call it from the main thread when the order relative to a Win32 show matters.
pub(crate) fn place(win: &WebviewWindow, r: PxRect) {
    let _ = win.set_position(PhysicalPosition::new(r.x, r.y));
    let _ = win.set_size(PhysicalSize::new(r.w.max(1) as u32, r.h.max(1) as u32));
    let _ = win.set_position(PhysicalPosition::new(r.x, r.y));
}

/// The window's HWND as a plain integer (0 never returned).
pub(crate) fn hwnd(win: &WebviewWindow) -> Option<isize> {
    win.hwnd().ok().map(|h| h.0 as isize).filter(|&h| h != 0)
}

// ── Win32 (pattern of `picker/win.rs`, which is private to the shelf) ───────

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

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
        ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    fn hwnd(h: isize) -> HWND {
        HWND(h as *mut core::ffi::c_void)
    }

    /// Out of Alt+Tab: `WS_EX_TOOLWINDOW` on, `WS_EX_APPWINDOW` off ("A tool window does not
    /// appear in the taskbar or in the dialog that appears when the user presses ALT+TAB"),
    /// then `SetWindowPos(SWP_FRAMECHANGED)` ("changes … will not take effect until you call
    /// SetWindowPos"). tao rewrites GWL_EXSTYLE on its own show/hide/focusable calls
    /// (`window_state.rs::apply_diff`), so these windows are shown and hidden through Win32 only.
    /// https://learn.microsoft.com/windows/win32/winmsg/extended-window-styles
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowlongptrw
    pub fn apply_toolwindow(h: isize) {
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

    /// Show on top WITHOUT activating (`SWP_NOACTIVATE | SWP_SHOWWINDOW`) — tao's `show()` is
    /// `ShowWindow(SW_SHOW)`, which "Activates the window and displays it".
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowpos
    pub fn show_no_activate(h: isize) {
        // SAFETY: own window.
        let _ = unsafe {
            SetWindowPos(hwnd(h), Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW)
        };
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-showwindow
    pub fn hide(h: isize) {
        // SAFETY: own window.
        let _ = unsafe { ShowWindow(hwnd(h), SW_HIDE) };
    }

    /// `SetForegroundWindow`, then confirm with `GetForegroundWindow`. Allowed when "the calling
    /// process received the last input event" (a click in the QAO or the shelf, a hotkey) —
    /// a refusal is reported, never forced (Windows flashes the taskbar button instead).
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow
    pub fn set_foreground(h: isize) -> bool {
        // SAFETY: plain window handle; failure is a FALSE return.
        unsafe { SetForegroundWindow(hwnd(h)).as_bool() && GetForegroundWindow() == hwnd(h) }
    }

    /// Physical cursor position (Per-Monitor-V2 process).
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getcursorpos
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut pt = POINT::default();
        // SAFETY: `pt` outlives the call.
        unsafe { GetCursorPos(&mut pt) }.ok()?;
        Some((pt.x, pt.y))
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn apply_toolwindow(_: isize) {}
    pub fn show_no_activate(_: isize) {}
    pub fn hide(_: isize) {}
    pub fn set_foreground(_: isize) -> bool {
        false
    }
    pub fn cursor_pos() -> Option<(i32, i32)> {
        None
    }
}

pub(crate) use imp::{apply_toolwindow, cursor_pos, hide, set_foreground, show_no_activate};

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(name: &str, x: i32, w: i32, scale: f64) -> Mon {
        Mon {
            name: Some(name.into()),
            rect: PxRect { x, y: 0, w, h: 1080 },
            work: PxRect { x, y: 0, w, h: 1040 },
            scale,
        }
    }

    #[test]
    fn a_monitor_is_picked_by_name_then_by_point() {
        let mons = [mon(r"\\.\DISPLAY1", 0, 1920, 1.0), mon(r"\\.\DISPLAY2", 1920, 2560, 1.5)];
        let name = |m: Option<&Mon>| m.and_then(|m| m.name.clone());
        assert_eq!(name(pick_monitor(&mons, Some(r"\\.\DISPLAY2"), Some((10, 10)))).as_deref(), Some(r"\\.\DISPLAY2"));
        // A monitor that is gone falls back to the cursor's.
        assert_eq!(name(pick_monitor(&mons, Some(r"\\.\DISPLAY9"), Some((2000, 10)))).as_deref(), Some(r"\\.\DISPLAY2"));
        // In a gap (below both), the nearest one.
        assert_eq!(name(pick_monitor(&mons, None, Some((100, 5000)))).as_deref(), Some(r"\\.\DISPLAY1"));
        assert_eq!(name(pick_monitor(&mons, None, None)).as_deref(), Some(r"\\.\DISPLAY1"));
        assert!(pick_monitor(&[], Some("x"), Some((0, 0))).is_none());
    }

    #[test]
    fn logical_to_physical_rounds() {
        assert_eq!(px(16.0, 1.0), 16);
        assert_eq!(px(16.0, 1.25), 20);
        assert_eq!(px(16.0, 1.75), 28);
        assert!(PxRect { x: -1920, y: 0, w: 1920, h: 1080 }.contains((-1, 1079)));
        assert!(!PxRect { x: -1920, y: 0, w: 1920, h: 1080 }.contains((0, 0)));
    }

    /// Every window of the process shares one WebView2 environment, so every builder must pass
    /// the same browser arguments: a window built without them would ask for an environment with
    /// other options in the same data folder, which WebView2 refuses — only at run time, natively.
    #[test]
    fn every_window_builder_passes_the_shared_browser_args() {
        fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    rust_files(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        rust_files(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        // Needles assembled at run time, so this file's own text never matches them.
        let (builder, call) = (["WebviewWindowBuilder", "::new("].concat(), [".additional_", "browser_args("].concat());
        let (hub, here) = ([&call[1..], "screenshot::windows::BROWSER_ARGS)"].concat(), [&call[1..], "w::BROWSER_ARGS)"].concat());
        let guard = [".on_navigation(", "screenshot::windows::navigation_guard(app))"].concat();
        let guard_here = [".on_navigation(", "w::navigation_guard(app))"].concat();
        let mut builders = 0;
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            let built = src.matches(builder.as_str()).count();
            let args = src.matches(call.as_str()).count();
            let shared = src.matches(hub.as_str()).count() + src.matches(here.as_str()).count();
            let guarded = src.matches(guard.as_str()).count() + src.matches(guard_here.as_str()).count();
            assert_eq!((args, shared, guarded), (built, built, built), "{}: {built} builders, {args} with browser args, {shared} with BROWSER_ARGS, {guarded} with the navigation guard", f.display());
            builders += built;
        }
        // picker, hud, settings, capture (lib.rs) + shots, editor, pin-<n> (on demand).
        assert_eq!(builders, 7);
        // wry's default (kept verbatim, it is replaced by any explicit string) + autoplay.
        assert!(BROWSER_ARGS.starts_with("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection "));
        assert!(BROWSER_ARGS.ends_with("--autoplay-policy=no-user-gesture-required"));
    }

    #[test]
    fn only_our_own_pages_may_load() {
        let u = |s: &str| Url::parse(s).unwrap();
        let dev = u("http://localhost:1430");
        // The bundled frontend (Windows: http, or https with `useHttpsScheme`) and about:blank.
        for ok in ["http://tauri.localhost/shots.html", "https://tauri.localhost/editor.html?clip=3", "about:blank"] {
            assert!(own_page(&u(ok), Some(&dev)), "{ok}");
            assert!(own_page(&u(ok), None), "{ok}");
        }
        // A dropped file or link, another app scheme, a lookalike host, another local port.
        for bad in [
            "file:///C:/Users/me/AppData/Roaming/com.liqlip.app/media/12.png",
            "https://example.com/",
            "http://liq.localhost/media/12",
            "http://tauri.localhost.example.com/",
            "http://localhost:8080/",
            "about:srcdoc",
            "data:text/html,hi",
        ] {
            assert!(!own_page(&u(bad), Some(&dev)), "{bad}");
        }
        // The dev server — only in a dev build (`cfg(dev)`, set by tauri-build).
        assert_eq!(own_page(&u("http://localhost:1430/capture.html"), Some(&dev)), cfg!(dev));
        assert!(!own_page(&u("http://localhost:1430/capture.html"), None));
    }

    #[test]
    fn no_main_thread_recorded_means_building_is_allowed() {
        // `init` never ran in the test binary: nothing is the main thread.
        assert!(!on_main_thread());
        assert!(may_build().is_ok());
    }
}
