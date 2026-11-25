//! Shelf window — state machine (summon / hover / exits), docking, focus, `settings_open`,
//! monitors (ARCHITECTURE §7) and the HUD (`hud.rs`). Owner: **W1-D**.
//!
//! - `machine.rs` — the pure state machine (events → effects), unit-tested.
//! - `geometry.rs` — pure docking / hover / HUD geometry, unit-tested.
//! - `poll.rs` — one background thread: hover trigger, click-outside, cursor-leave,
//!   display changes (Win32 reads only; it never touches a window).
//! - `win.rs` — thin Win32 helpers.
//!
//! Every transition runs on the MAIN thread (`dispatch` → `run_on_main_thread`): tao applies
//! window flags there synchronously, so `WS_EX_TOOLWINDOW` can be re-applied right after each
//! flag change (tao rewrites `GWL_EXSTYLE`, ARCHITECTURE §2) and effects run in order. The
//! state lock is never held while an effect runs (a window call may re-enter `dispatch`).
//! Windows are pre-created in `lib.rs`; this module only shows / hides / moves them.

pub mod commands;
mod geometry;
pub mod hud;
mod machine;
mod poll;
mod win;

use crate::model::{PickerOpen, Settings, TargetInfo};
use crate::{events, logx, paste, settings};
use geometry::{Dock, Mon, Rect};
use machine::{Config, Effect, Event, Machine, Phase};
use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent};
use win::Hwnd;

/// The webview's exit transition (`--dur-slow` 250 ms) plays before the native hide.
const HIDE_GRACE: Duration = Duration::from_millis(260);

pub(crate) struct Shared {
    pub m: Machine,
    pub dock: Option<Dock>,
    target: Option<TargetInfo>,
    /// The window that was foreground before the shelf took focus (Esc gives it back).
    restore_to: Option<Hwnd>,
    /// Bumped by `Effect::DisarmHover` (the poll's hover tracker follows it).
    pub disarm_seq: u64,
}

pub(crate) struct Picker {
    pub state: Mutex<Shared>,
    /// Wakes the poll on every transition / dock change.
    pub wake: Condvar,
}

static PICKER: OnceLock<Picker> = OnceLock::new();

/// HWNDs of the three pre-created windows (stable for the process lifetime).
#[derive(Clone, Copy)]
pub(crate) struct Hwnds {
    pub picker: Hwnd,
    pub hud: Hwnd,
    pub settings: Hwnd,
}

impl Hwnds {
    pub fn is_ours(&self, h: Hwnd) -> bool {
        h == self.picker || h == self.hud || h == self.settings
    }
}

static HWNDS: OnceLock<Hwnds> = OnceLock::new();

fn hwnd_of(app: &AppHandle, label: &str) -> Hwnd {
    app.get_webview_window(label)
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as Hwnd)
        .unwrap_or(0)
}

pub(crate) fn hwnds() -> Option<Hwnds> {
    HWNDS.get().copied()
}

fn config_of(s: &Settings) -> Config {
    Config {
        position: s.picker.position,
        center_row: s.picker.center_row,
        monitor: s.picker.monitor.clone(),
        hover_open: s.picker.hover_open,
    }
}

/// `setup()` after step 6 (windows exist): window listeners, `settings.picker.*`
/// subscription, the poll thread, the first dock (+ `picker:layout`, or the armed edge).
pub fn init(app: &AppHandle) {
    let _ = HWNDS.set(Hwnds {
        picker: hwnd_of(app, events::PICKER),
        hud: hwnd_of(app, events::HUD),
        settings: hwnd_of(app, events::SETTINGS),
    });
    let cfg = config_of(&settings::get(app));
    let _ = PICKER.set(Picker {
        state: Mutex::new(Shared { m: Machine::new(cfg), dock: None, target: None, restore_to: None, disarm_seq: 0 }),
        wake: Condvar::new(),
    });

    if let Some(win) = app.get_webview_window(events::PICKER) {
        let app2 = app.clone();
        // A second listener next to lib.rs' CloseRequested → hide (tauri keeps a list).
        win.on_window_event(move |ev| match ev {
            WindowEvent::Focused(true) => dispatch(&app2, Event::Focused),
            WindowEvent::Focused(false) => blur_guard(&app2),
            WindowEvent::CloseRequested { .. } => dispatch(&app2, Event::CloseRequested),
            _ => {}
        });
    }

    settings::subscribe(app, {
        let app = app.clone();
        move |old, new| {
            let (a, b) = (config_of(old), config_of(new));
            if a != b {
                dispatch(&app, Event::Config(b));
            }
        }
    });

    poll::spawn(app.clone());
    dispatch(app, Event::Init);
}

/// Open the shelf from the hotkey, the tray or a second instance (toggle: closes it when
/// it is open from a summon). Never blocks; the work runs on the main thread.
pub fn summon(app: &AppHandle) {
    let visible = hwnds().map(|h| win::is_visible(h.picker)).unwrap_or(false);
    dispatch(app, Event::Summon { visible });
}

/// Hide immediately, no exit animation, WITHOUT restoring focus (the paste flow gives focus
/// to the target itself). Blocks until the window is hidden (≤ 1 s); callable from any
/// thread, including the main one.
#[allow(dead_code)] // W1-C calls it (paste flow)
pub fn hide_now(app: &AppHandle) {
    if !dispatch_wait(app, Event::HideNow, Duration::from_secs(1)) {
        logx::warn("picker", "hide_now: main thread did not answer in 1 s");
    }
}

/// Close the shelf the way Esc does — exit animation, then the native hide — giving focus back
/// to the window it was summoned from (`restore_focus`, only while the shelf still holds it).
/// The paste flow uses it when there is no target and the record was only copied (W3-A:
/// SPEC §3.3 "focus always returns where it was", like `Ctrl+Enter`). Never blocks.
pub fn close(app: &AppHandle, restore_focus: bool) {
    dispatch(app, Event::Hide { restore: restore_focus });
}

/// Show + activate the settings window, optionally on a section (`settings:navigate`).
/// Closes an open shelf first (always-on-top, it would cover settings). Used by
/// `settings_open`, the tray and the first-run onboarding (`setup()` step 10).
pub fn show_settings(app: &AppHandle, section: Option<&str>) {
    let section = section.map(str::to_string);
    let app2 = app.clone();
    let r = app.run_on_main_thread(move || {
        run_event(&app2, Event::ShowSettings);
        let Some(w) = app2.get_webview_window(events::SETTINGS) else { return };
        if w.is_minimized().unwrap_or(false) {
            let _ = w.unminimize();
        }
        if let Err(e) = w.show() {
            logx::warn("picker", &format!("settings show: {e}"));
        }
        // Tray pick / onboarding at launch / Ctrl+, in the shelf all hold the foreground
        // right; when refused Windows flashes the taskbar button instead (SetForegroundWindow
        // docs) — no forcing.
        let h = hwnd_of(&app2, events::SETTINGS);
        if h != 0 && !win::set_foreground(h) {
            logx::warn("picker", "settings: foreground refused (taskbar button flashes)");
        }
        if let Some(s) = section {
            events::settings_navigate(&app2, &s);
        }
    });
    if let Err(e) = r {
        logx::warn("picker", &format!("show_settings: {e}"));
    }
}

/// Re-send the shelf's layout/open state to a webview that (re)loaded — e.g. a summon in the
/// first second after launch, before the page subscribed. Wire from `Builder::on_page_load`
/// (`PageLoadEvent::Finished`), see the W1-D report snippet.
#[allow(dead_code)] // wired by the coordinator in lib.rs
pub fn on_page_load(app: &AppHandle, label: &str) {
    if label != events::PICKER {
        return;
    }
    let Some(p) = PICKER.get() else { return };
    let (dock, phase, target) = {
        let s = p.state.lock();
        (s.dock.clone(), s.m.phase, s.target.clone())
    };
    if let Some(d) = dock {
        events::picker_layout(app, &geometry::layout(&d));
    }
    if let Phase::Open { mode, interactive } = phase {
        events::picker_open(app, &PickerOpen { mode, target, interactive });
    }
}

// ── Dispatch / executor ─────────────────────────────────────────────────────

/// Queue an event for the main thread (runs inline when already on it).
pub(crate) fn dispatch(app: &AppHandle, ev: Event) {
    let app2 = app.clone();
    if let Err(e) = app.run_on_main_thread(move || run_event(&app2, ev)) {
        logx::warn("picker", &format!("dispatch: {e}"));
    }
}

/// `dispatch` and wait until it has run. False on timeout.
fn dispatch_wait(app: &AppHandle, ev: Event, timeout: Duration) -> bool {
    let (tx, rx) = mpsc::channel();
    let app2 = app.clone();
    let posted = app.run_on_main_thread(move || {
        run_event(&app2, ev);
        let _ = tx.send(());
    });
    posted.is_ok() && rx.recv_timeout(timeout).is_ok()
}

fn run_event(app: &AppHandle, first: Event) {
    let Some(p) = PICKER.get() else { return };
    let mut queue = VecDeque::from([first]);
    while let Some(ev) = queue.pop_front() {
        let effects = p.state.lock().m.on(ev);
        p.wake.notify_all();
        for fx in effects {
            if let Some(next) = execute(app, p, fx) {
                queue.push_back(next);
            }
        }
    }
}

fn picker_window(app: &AppHandle) -> Option<(WebviewWindow, Hwnd)> {
    let w = app.get_webview_window(events::PICKER)?;
    let h = hwnds().map(|h| h.picker).unwrap_or(0);
    Some((w, h))
}

/// A tao flag change wrapped in `WS_EX_TOOLWINDOW` re-application: before (the shell sees a
/// tool window when it is shown) and after (tao has just rewritten GWL_EXSTYLE).
fn tao_op(w: &WebviewWindow, h: Hwnd, what: &str, op: impl FnOnce(&WebviewWindow) -> tauri::Result<()>) {
    win::apply_toolwindow(h);
    if let Err(e) = op(w) {
        logx::warn("picker", &format!("{what}: {e}"));
    }
    win::apply_toolwindow(h);
}

fn execute(app: &AppHandle, p: &Picker, fx: Effect) -> Option<Event> {
    let (w, h) = picker_window(app)?;
    match fx {
        Effect::CaptureTarget => {
            // BEFORE the shelf shows: the foreground window is where focus returns, and the
            // paste target (W1-C classifies it: accepted / kept previous / none).
            // The settings window counts (Esc returns there); the shelf / HUD / taskbar do not.
            let ours = hwnds();
            let fg = win::foreground().filter(|&f| {
                !ours.is_some_and(|o| f == o.picker || f == o.hud) && !win::is_shell_tray(f)
            });
            let target = paste::focus::capture_target(app);
            let mut s = p.state.lock();
            s.restore_to = fg;
            s.target = target;
        }
        Effect::Dock => dock_now(app, p, &w),
        Effect::ShowActive => {
            tao_op(&w, h, "show", |w| {
                w.set_ignore_cursor_events(false)?;
                w.set_focusable(true)?;
                w.show()
            });
            reassert_geometry(p, &w);
            // The process may take the foreground right after WM_HOTKEY / a tray click
            // ("received the last input event"); if not, the shelf opens passive
            // (`interactive:false`) and a click activates it.
            return Some(Event::Activated(win::set_foreground(h)));
        }
        Effect::ShowPassive => {
            // Show without activation: SWP_NOACTIVATE first, then tao's show() only syncs
            // its VISIBLE flag (ShowWindow(SW_SHOW) on an already visible window does not
            // activate it).
            win::apply_toolwindow(h);
            win::show_no_activate(h);
            tao_op(&w, h, "show passive", |w| w.show());
            reassert_geometry(p, &w);
        }
        Effect::ClickThrough(on) => tao_op(&w, h, "click-through", |w| w.set_ignore_cursor_events(on)),
        Effect::Focusable(on) => tao_op(&w, h, "focusable", |w| w.set_focusable(on)),
        Effect::Activate => return Some(Event::Activated(win::set_foreground(h))),
        Effect::EmitOpen { mode, interactive } => {
            let target = p.state.lock().target.clone();
            events::picker_open(app, &PickerOpen { mode, target, interactive });
        }
        Effect::EmitClose => events::picker_close(app),
        Effect::RestoreFocus => {
            // Only while the shelf still holds focus: a click on another window already put
            // focus where the user wants it.
            let prev = p.state.lock().restore_to;
            let fg = win::foreground();
            if let Some(prev) = prev {
                if (fg == Some(h) || fg.is_none()) && win::is_window(prev) && !win::set_foreground(prev) {
                    logx::warn("picker", "restore focus refused");
                }
            }
        }
        Effect::ScheduleHide(gen) => {
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(HIDE_GRACE);
                dispatch(&app, Event::HideTimer(gen));
            });
        }
        Effect::HideWindow => tao_op(&w, h, "hide", |w| w.hide()),
        Effect::DisarmHover => {
            p.state.lock().disarm_seq += 1;
            p.wake.notify_all();
        }
    }
    None
}

/// All monitors as Tauri reports them (physical rcMonitor / rcWork, effective scale).
fn monitors(w: &WebviewWindow) -> Vec<Mon> {
    let primary = w.primary_monitor().ok().flatten();
    w.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let (pos, size, wa) = (m.position(), m.size(), m.work_area());
            let id = m.name().cloned().unwrap_or_default();
            let is_primary = primary.as_ref().is_some_and(|p| {
                p.name() == m.name() && p.position() == m.position()
            });
            Mon {
                id,
                primary: is_primary,
                screen: Rect::new(pos.x, pos.y, size.width as i32, size.height as i32),
                work: Rect::new(wa.position.x, wa.position.y, wa.size.width as i32, wa.size.height as i32),
                scale: m.scale_factor(),
            }
        })
        .collect()
}

fn dock_now(app: &AppHandle, p: &Picker, w: &WebviewWindow) {
    let cfg = p.state.lock().m.cfg.clone();
    let mons = monitors(w);
    let Some(mon) = geometry::pick_monitor(&mons, &cfg.monitor, win::cursor_pos()) else {
        logx::warn("picker", "dock: no monitors");
        return;
    };
    let d = geometry::dock(cfg.position, cfg.center_row, mon, &mons);
    apply_geometry(w, &d.window);
    events::picker_layout(app, &geometry::layout(&d));
    p.state.lock().dock = Some(d);
    p.wake.notify_all();
}

/// Position → size → position (physical). Moving onto a monitor with another scale makes
/// tao handle WM_DPICHANGED during the first move (it rescales and may shift the window to
/// Windows' suggested rect); the size and the second move then set the exact geometry.
fn apply_geometry(w: &WebviewWindow, r: &Rect) {
    let _ = w.set_position(PhysicalPosition::new(r.left, r.top));
    let _ = w.set_size(PhysicalSize::new(r.width().max(1) as u32, r.height().max(1) as u32));
    let _ = w.set_position(PhysicalPosition::new(r.left, r.top));
}

/// After a show: a DPI change deferred to the first show of a hidden window is corrected.
fn reassert_geometry(p: &Picker, w: &WebviewWindow) {
    let dock = p.state.lock().dock.clone();
    if let Some(d) = dock {
        if let (Ok(pos), Ok(size)) = (w.outer_position(), w.inner_size()) {
            let r = d.window;
            if pos.x != r.left || pos.y != r.top || size.width as i32 != r.width() || size.height as i32 != r.height() {
                apply_geometry(w, &r);
            }
        }
    }
}

/// Last time (ms on `clock_ms`) a keepOpen hand-off or a drag-out was seen in progress. A
/// drag-out runs a modal loop on the main thread, so the `Focused(false)` it causes can be
/// delivered only after `dragout::active()` is false again — the poll records the window.
static GUARD_SEEN_MS: AtomicU64 = AtomicU64::new(0);
const GUARD_TAIL_MS: u64 = 800;

fn clock_ms() -> u64 {
    static T0: OnceLock<Instant> = OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// W1-C: `handoff_active` (keepOpen paste moved focus to the target and back) or
/// `dragout::active` (modal OLE drag). Records the time for `guard_recent`.
pub(crate) fn guard_now() -> bool {
    let on = paste::focus::handoff_active() || paste::dragout::active();
    if on {
        GUARD_SEEN_MS.store(clock_ms(), Ordering::Relaxed);
    }
    on
}

fn guard_recent() -> bool {
    let t = GUARD_SEEN_MS.load(Ordering::Relaxed);
    t != 0 && clock_ms().saturating_sub(t) < GUARD_TAIL_MS
}

/// `Focused(false)` of an interactive shelf: wait out a hand-off / drag-out / a plain drag
/// (button held), give the activation a moment, then close — unless focus came back or went
/// to one of our own windows (a dialog of this process, the settings window). After a
/// hand-off or a drag-out the shelf is not closed but becomes passive (W1-C contract).
fn blur_guard(app: &AppHandle) {
    let Some(p) = PICKER.get() else { return };
    {
        let s = p.state.lock();
        if !matches!(s.m.phase, Phase::Open { interactive: true, .. }) {
            return;
        }
    }
    let mut handed_off = guard_now() || guard_recent();
    let app = app.clone();
    std::thread::spawn(move || {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(15) {
            let guarded = guard_now();
            handed_off |= guarded;
            if !guarded && !win::mouse_buttons_down() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        // Let a tray-icon click (its own toggle, delivered on the release) land first.
        std::thread::sleep(Duration::from_millis(100));
        // Mid-switch there may be no foreground window for a moment.
        let mut fg = None;
        for _ in 0..8 {
            std::thread::sleep(Duration::from_millis(40));
            fg = win::foreground();
            if fg.is_some() {
                break;
            }
        }
        let picker = hwnds().map(|h| h.picker).unwrap_or(0);
        match fg {
            Some(f) if f == picker => {}
            Some(f) if win::is_own_process(f) => {}
            // After a hand-off/drag the shelf stays, passive (W1-C: "a click activates it").
            _ if handed_off => dispatch(&app, Event::Deactivated),
            _ => dispatch(&app, Event::Blur),
        }
    });
}

