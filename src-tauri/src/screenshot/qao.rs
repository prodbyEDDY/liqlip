//! Quick-access overlay — the thumbnail stack after a shot, window `shots` (owner: W6-B;
//! ARCHITECTURE §18.2, §18.8; SPEC §9.4).
//!
//! - **Built on demand** on a spawned thread (`windows.rs`): at the first capture
//!   ([`hide_for_capture`], so a region session gives the page seconds to load) or at the first
//!   [`show_added`]; then it lives hidden. Transparent, frameless, always-on-top, skip-taskbar,
//!   `focusable(false)` (tao → `WS_EX_NOACTIVATE`: clicks and drag-out work without activation),
//!   `focused(false)`, no WebView2 drop handler (the OLE drag-out of cards), out of every capture
//!   (`WDA_EXCLUDEFROMCAPTURE`) and out of Alt+Tab (`WS_EX_TOOLWINDOW`).
//! - **Shown without activation** (Win32 `SetWindowPos(SWP_NOACTIVATE | SWP_SHOWWINDOW)`,
//!   hidden with `ShowWindow(SW_HIDE)`, never tao `show()`), like the HUD (§7.7). Every show /
//!   hide / move runs on the main thread (`run_on_main_thread`), so tao's posted moves and the
//!   Win32 show stay in order. Callers never block.
//! - **Geometry** (W6-E's page, INT-5): fixed width [`QAO_WIDTH`] (a 256 px card + the page's
//!   24 px shadow margin on each side), height from the page (`shots_hud_layout`) capped by the
//!   work area; the window's bottom edge is the bottom of the work area (`rcWork` — above the
//!   taskbar) of the shot's monitor, its side the configured corner (`settings.screenshot.corner`),
//!   no inset: the page's margin is the visual gap.
//! - **First event** (`shots_pending`, INT-5): `shot:added` reaches a page only after it
//!   subscribed. Until the page has asked [`pending`] (it does right after subscribing), payloads
//!   wait in [`QaoState`] and come back as that command's answer; from then on they are emitted.
//!   A reload (`PageLoadEvent::Started`) or a rebuilt window starts waiting again.
//! - **Temporary files** (§18.4, a shot not kept in history: `ShotAdded.tempPath` in
//!   `%TEMP%\Liqlip\shots\`, served as `liq://temp/<name>`): removed when the stack empties
//!   (`shots_hud_hide`), when more than [`TEMP_KEEP`] were handed out (the page keeps 4 cards),
//!   and at start-up. Only plain files directly inside that folder are ever removed
//!   (`files::is_temp_file`).

use super::files;
use super::windows::{self as w, Mon, PxRect};
use crate::events;
use crate::logx;
use crate::model::{ShotAdded, ShotCorner};
use crate::settings;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Window width in logical px: W6-E's card (256) + its shadow margin (24) on each side
/// (`src/windows/shots/stack.ts::WINDOW_W`).
pub(crate) const QAO_WIDTH: f64 = 304.0;
/// Height before the page reported one (the window is hidden until it does).
const INITIAL_HEIGHT: f64 = 216.0;
/// Temp files handed to the page that may still back a card (the stack holds up to 4).
const TEMP_KEEP: usize = 4;
/// A temp file is removed on an empty stack only once the page has had time to show it.
const TEMP_GRACE: Duration = Duration::from_millis(1500);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    None,
    Building,
    Built,
}

/// What the QAO window side remembers (`ScreenshotState.qao`).
#[derive(Default)]
pub(crate) struct QaoState {
    phase: Phase,
    /// The page has subscribed and asked `shots_pending`: events reach it.
    claimed: bool,
    /// `shot:added` payloads waiting for the page (`shots_pending` hands them over).
    pending: Vec<ShotAdded>,
    /// Device name of the last shot's monitor.
    monitor: Option<String>,
    /// Stack height from the page, logical px (0 = empty / hidden).
    height: f64,
    /// Hidden for a capture session.
    suspended: bool,
    /// Temp files handed to the page, oldest first.
    temps: VecDeque<(PathBuf, Instant)>,
}

impl QaoState {
    /// A new payload: `true` → emit it now (the page listens); `false` → it waits for
    /// `shots_pending`. Only the claim counts: a page that claimed exists and listens (`claimed`
    /// is cleared when a build starts, a page (re)loads and the window is destroyed), even in the
    /// instant before its builder thread marks the window built.
    fn offer(&mut self, payload: &ShotAdded) -> bool {
        if self.claimed {
            return true;
        }
        self.pending.push(payload.clone());
        false
    }

    /// `shots_pending`: from now on payloads are emitted; the waiting ones are the answer.
    fn claim(&mut self) -> Vec<ShotAdded> {
        self.claimed = true;
        std::mem::take(&mut self.pending)
    }
}

fn with<T>(app: &AppHandle, f: impl FnOnce(&mut QaoState) -> T) -> Option<T> {
    let st = app.try_state::<super::ScreenshotState>()?;
    let mut guard = st.qao.lock();
    Some(f(&mut guard))
}

/// Start-up (`windows::init`): clear temp files a previous run left behind, off the main thread.
pub(crate) fn init(_app: &AppHandle) {
    let spawned = std::thread::Builder::new().name("liqlip-shots-sweep".into()).spawn(|| {
        let n = sweep(&files::temp_dir());
        if n > 0 {
            logx::warn("screenshot", &format!("removed {n} stale QAO temp files"));
        }
    });
    if let Err(e) = spawned {
        logx::warn("screenshot", &format!("temp sweep: {e}"));
    }
}

// ── API for the capture side (session / commit) ─────────────────────────────

/// A shot was taken and `afterCapture` is `overlay` (or the save failed): show its card. Builds
/// the window on first use (a spawned thread); the payload waits until the page claims it. The
/// window appears once the page reports its height (`shots_hud_layout`), not before. Any
/// thread; never blocks.
pub(crate) fn show_added(app: &AppHandle, payload: ShotAdded) {
    let Some((now, visible)) = with(app, |st| {
        st.monitor = Some(payload.monitor.clone());
        if let Some(t) = payload.temp_path.as_deref().map(PathBuf::from) {
            st.temps.push_back((t, Instant::now()));
        }
        (st.offer(&payload), st.height > 0.0)
    }) else {
        return;
    };
    if now {
        events::shot_added(app, &payload);
        if visible {
            // A new monitor: move the stack there now; the page's layout call follows.
            refresh(app);
        }
    } else {
        ensure_built(app);
    }
    trim_temps(app);
}

/// A capture session starts: hide the stack (it keeps its cards) and build the window if it
/// does not exist yet, so it has loaded by the time the shot is committed. Never blocks.
pub(crate) fn hide_for_capture(app: &AppHandle) {
    if with(app, |st| st.suspended = true).is_none() {
        return;
    }
    ensure_built(app);
    refresh(app);
}

/// The session ended (committed or cancelled): show the stack again if it has cards.
pub(crate) fn restore_after_capture(app: &AppHandle) {
    if with(app, |st| st.suspended = false).is_some() {
        refresh(app);
    }
}

// ── Commands ────────────────────────────────────────────────────────────────

/// `shots_pending` — the page subscribed to `shot:added`: hand over the payloads that arrived
/// before (oldest first); later ones are emitted.
pub(crate) fn pending(app: &AppHandle) -> Result<Vec<ShotAdded>, String> {
    with(app, QaoState::claim).ok_or_else(|| "failed: screenshot state".to_string())
}

/// `shots_hud_layout { height }` — the stack's height in logical px (the window follows; it is
/// shown here, without activation, unless a session is open).
pub(crate) fn layout(app: &AppHandle, height: f64) -> Result<(), String> {
    if !height.is_finite() || height < 0.0 {
        return Err(format!("invalid: height {height}"));
    }
    with(app, |st| st.height = height).ok_or("failed: screenshot state")?;
    refresh(app);
    if height == 0.0 {
        clear_temps(app);
    }
    Ok(())
}

/// `shots_hud_hide` — the stack is empty; the window keeps living, hidden.
pub(crate) fn hide(app: &AppHandle) -> Result<(), String> {
    layout(app, 0.0)
}

// ── Window ──────────────────────────────────────────────────────────────────

/// Build `shots` on a spawned thread unless it exists or is being built.
fn ensure_built(app: &AppHandle) {
    let start = with(app, |st| {
        if st.phase != Phase::None {
            return false;
        }
        st.phase = Phase::Building;
        st.claimed = false;
        true
    });
    if start != Some(true) {
        return;
    }
    let app2 = app.clone();
    let spawned = std::thread::Builder::new().name("liqlip-shots-window".into()).spawn(move || {
        let result = build(&app2);
        let failed = with(&app2, |st| match &result {
            Ok(()) => {
                st.phase = Phase::Built;
                0
            }
            Err(_) => {
                st.phase = Phase::None;
                std::mem::take(&mut st.pending).len()
            }
        });
        match result {
            // A layout the page reported before the window counted as built was not shown
            // (`refresh` waits for `Built`: affinity and tool-window style first): apply it now.
            Ok(()) => refresh(&app2),
            Err(e) => logx::warn("screenshot", &format!("shots window: {e} ({} cards dropped)", failed.unwrap_or(0))),
        }
    });
    if let Err(e) = spawned {
        with(app, |st| st.phase = Phase::None);
        logx::warn("screenshot", &format!("shots window thread: {e}"));
    }
}

fn build(app: &AppHandle) -> Result<(), String> {
    w::may_build()?;
    if app.get_webview_window(events::SHOTS).is_some() {
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(app, events::SHOTS, WebviewUrl::App("shots.html".into()))
        .title("Liqlip Shots")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focusable(false)
        .focused(false)
        .visible(false)
        .inner_size(QAO_WIDTH, INITIAL_HEIGHT)
        .theme(w::native_theme(app))
        // Same WebView2 environment arguments as every window of the process (§18.2).
        .additional_browser_args(w::BROWSER_ARGS)
        // Only our own page: a card released back over the stack must not navigate it (§18.2).
        .on_navigation(w::navigation_guard(app))
        // WebView2's own drop handler would swallow the OLE drag-out of a card (as the shelf).
        .disable_drag_drop_handler()
        .on_page_load(|win, payload| on_page_load(win.app_handle(), payload.event()))
        .build()
        .map_err(|e| format!("failed: {e}"))?;
    if let Err(e) = w::exclude_from_capture(&win) {
        logx::warn("screenshot", &format!("shots window affinity: {e}"));
    }
    if let Some(h) = w::hwnd(&win) {
        w::apply_toolwindow(h);
    }
    let (app2, win2) = (app.clone(), win.clone());
    win.on_window_event(move |ev| match ev {
        // Never destroyed by a close request (it cannot be activated, but Alt+F4 could reach
        // it through a focused child): hide, keep the window.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            if let Some(h) = w::hwnd(&win2) {
                w::hide(h);
            }
        }
        // Destroyed anyway (WebView2 crash, exit): the next shot builds it again.
        WindowEvent::Destroyed => {
            with(&app2, |st| {
                st.phase = Phase::None;
                st.claimed = false;
                st.height = 0.0;
            });
        }
        _ => {}
    });
    Ok(())
}

/// Main thread (WebView2 event): a (re)load starts — the new page has not subscribed yet, so
/// payloads wait for its `shots_pending` again.
fn on_page_load(app: &AppHandle, event: PageLoadEvent) {
    if event == PageLoadEvent::Started {
        with(app, |st| st.claimed = false);
    }
}

/// Re-apply visibility and geometry on the main thread from the current state.
fn refresh(app: &AppHandle) {
    let corner = settings::get(app).screenshot.corner;
    let app2 = app.clone();
    let posted = app.run_on_main_thread(move || {
        let Some((show, monitor, height)) =
            with(&app2, |st| (st.phase == Phase::Built && st.height > 0.0 && !st.suspended, st.monitor.clone(), st.height))
        else {
            return;
        };
        let Some(win) = app2.get_webview_window(events::SHOTS) else { return };
        let Some(h) = w::hwnd(&win) else { return };
        if !show {
            w::hide(h);
            return;
        }
        let mons = w::monitors(&app2);
        let Some(mon) = w::pick_monitor(&mons, monitor.as_deref(), w::cursor_pos()) else { return };
        let r = qao_rect(mon, height, corner);
        w::place(&win, r);
        w::apply_toolwindow(h);
        w::show_no_activate(h);
        // A DPI change deferred to the first show of a window moved while hidden: re-assert.
        w::place(&win, r);
    });
    if let Err(e) = posted {
        logx::warn("screenshot", &format!("shots refresh: {e}"));
    }
}

/// The stack's window: [`QAO_WIDTH`] × `height` logical px (at most the work area), its bottom
/// edge on the bottom of the monitor's work area, in the `corner` side — no inset (the page's
/// 24 px margin is the gap).
fn qao_rect(mon: &Mon, height: f64, corner: ShotCorner) -> PxRect {
    let (s, wa) = (mon.scale, mon.work);
    let width = w::px(QAO_WIDTH, s).clamp(1, wa.w.max(1));
    let h = w::px(height, s).clamp(1, wa.h.max(1));
    let x = match corner {
        ShotCorner::Left => wa.x,
        ShotCorner::Right => wa.x + wa.w - width,
    };
    PxRect { x, y: wa.y + wa.h - h, w: width, h }
}

// ── Temp files ──────────────────────────────────────────────────────────────

/// Remove the temp files the page can no longer show: beyond the newest [`TEMP_KEEP`].
fn trim_temps(app: &AppHandle) {
    let old: Vec<PathBuf> = with(app, |st| {
        let extra = st.temps.len().saturating_sub(TEMP_KEEP);
        st.temps.drain(..extra).map(|(p, _)| p).collect()
    })
    .unwrap_or_default();
    remove_temps(&files::temp_dir(), &old);
}

/// The stack is empty: every temp file handed out more than [`TEMP_GRACE`] ago is unused.
fn clear_temps(app: &AppHandle) {
    let old: Vec<PathBuf> = with(app, |st| {
        let now = Instant::now();
        let (gone, keep): (Vec<_>, Vec<_>) = st.temps.drain(..).partition(|(_, at)| now.duration_since(*at) >= TEMP_GRACE);
        st.temps = keep.into_iter().collect();
        gone.into_iter().map(|(p, _)| p).collect()
    })
    .unwrap_or_default();
    remove_temps(&files::temp_dir(), &old);
}

fn remove_temps(root: &Path, paths: &[PathBuf]) {
    for p in paths {
        if files::is_temp_file(root, p) {
            if let Err(e) = std::fs::remove_file(p) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    logx::warn("screenshot", &format!("temp file not removed: {e}"));
                }
            }
        }
    }
}

/// Remove every plain file directly inside `dir`; the folder stays. Returns how many.
fn sweep(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| std::fs::remove_file(e.path()).is_ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(x: i32, y: i32, w: i32, h: i32, scale: f64) -> Mon {
        Mon { name: None, rect: PxRect { x, y, w, h: h + 40 }, work: PxRect { x, y, w, h }, scale }
    }

    #[test]
    fn the_stack_sits_on_the_bottom_corner_of_the_work_area() {
        // 1920×1080 at 100 %, taskbar 40 px at the bottom: work area 1920×1040. One card = 216.
        let m = mon(0, 0, 1920, 1040, 1.0);
        assert_eq!(qao_rect(&m, 216.0, ShotCorner::Left), PxRect { x: 0, y: 1040 - 216, w: 304, h: 216 });
        assert_eq!(qao_rect(&m, 396.0, ShotCorner::Right), PxRect { x: 1920 - 304, y: 1040 - 396, w: 304, h: 396 });
        // A second monitor left of the primary at 150 %: logical sizes scale, coordinates stay physical.
        let m2 = mon(-2560, 0, 2560, 1380, 1.5);
        let r = qao_rect(&m2, 216.0, ShotCorner::Left);
        assert_eq!(r, PxRect { x: -2560, y: 1380 - 324, w: 456, h: 324 });
        // A taskbar on top: the work area starts lower, the stack still ends at its bottom.
        let top = Mon { name: None, rect: PxRect { x: 0, y: 0, w: 1920, h: 1080 }, work: PxRect { x: 0, y: 48, w: 1920, h: 1032 }, scale: 1.0 };
        assert_eq!(qao_rect(&top, 216.0, ShotCorner::Right).y + 216, 1080);
    }

    #[test]
    fn the_stack_never_outgrows_the_work_area() {
        let m = mon(0, 0, 1366, 728, 1.0);
        let r = qao_rect(&m, 756.0, ShotCorner::Left);
        assert_eq!((r.y, r.h), (0, 728), "four cards on a short screen: the page squeezes them");
        // A tiny height still makes a valid window.
        assert_eq!(qao_rect(&m, 0.2, ShotCorner::Left).h, 1);
    }

    fn added(n: i64) -> ShotAdded {
        ShotAdded { clip: None, saved_path: None, temp_path: Some(format!("C:\\t\\{n}.png")), monitor: "M".into(), error: None }
    }

    #[test]
    fn payloads_wait_until_the_page_claims_them() {
        let mut st = QaoState::default();
        // Before the window exists, while it builds, and after it is built but not yet claimed.
        assert!(!st.offer(&added(1)));
        st.phase = Phase::Building;
        assert!(!st.offer(&added(2)));
        st.phase = Phase::Built;
        assert!(!st.offer(&added(3)));
        let got = st.claim();
        assert_eq!(got.iter().map(|p| p.temp_path.clone().unwrap()).collect::<Vec<_>>(), ["C:\\t\\1.png", "C:\\t\\2.png", "C:\\t\\3.png"]);
        // Claimed: emitted at once, nothing waits, a second claim is empty.
        assert!(st.offer(&added(4)));
        assert!(st.claim().is_empty());
        // A reload: waiting again until the new page claims.
        st.claimed = false;
        assert!(!st.offer(&added(5)));
        assert_eq!(st.claim().len(), 1);
        // A page that claims in the instant before its builder thread marks the window built
        // gets the next shot as an event — it never waits for a claim that already happened.
        let mut st = QaoState { phase: Phase::Building, ..QaoState::default() };
        assert!(st.claim().is_empty());
        assert!(st.offer(&added(6)));
        assert!(st.pending.is_empty());
    }

    #[test]
    fn only_plain_files_inside_the_temp_folder_are_removed() {
        let base = std::env::temp_dir().join(format!("liqlip-qao-test-{}", std::process::id()));
        let root = base.join("shots");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let inside = root.join("1.png");
        let nested = root.join("sub").join("2.png");
        let outside = base.join("keep.png");
        for f in [&inside, &nested, &outside] {
            std::fs::write(f, b"x").unwrap();
        }
        remove_temps(&root, &[outside.clone(), nested.clone()]);
        assert!(outside.exists() && nested.exists(), "never outside the folder or in a sub-folder");
        assert_eq!(sweep(&root), 1);
        assert!(!inside.exists() && nested.exists(), "the sweep keeps sub-folders");
        assert_eq!(sweep(&base.join("nope")), 0);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
