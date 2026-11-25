//! The shelf's background poll (one thread for the process lifetime). It only READS global
//! state (`GetCursorPos`, `GetAsyncKeyState`, monitor info) and turns it into events; every
//! window change happens on the main thread through `dispatch`.
//!
//! Asleep (condvar) while the shelf is hidden or closing. Cadence:
//! - armed, cursor far from the docked edge — 250 ms; within `NEAR_BAND` — 60 ms (Liquid
//!   Tools' two-speed poll: a mouse cannot cross the band in one slow tick);
//! - open — 30 ms (a quick click lasts longer than that).
//!
//! Why polling for "click outside": a passive (hover / refused-activation) shelf never has
//! focus, so `Focused(false)` cannot report clicks elsewhere, and clicks on the shelf's own
//! transparent bleed land in the webview. A low-level mouse hook would do it too, but hooks
//! are reserved for the hotkey engine (ARCHITECTURE §8) and must stay minimal.

use super::geometry::{self, ClickTracker, HoverTracker, Rect};
use super::machine::{Event, Phase};
use super::{dispatch, hwnds, win, PICKER};
use crate::model::PickerMode;
use std::time::{Duration, Instant};
use tauri::AppHandle;

const TICK_FAR: u64 = 250;
const TICK_NEAR: u64 = 60;
const TICK_OPEN: u64 = 30;
/// A click outside closes shortly after its release — the clicked app has activated by then
/// (focus is not handed back over it), and a tray-icon click (its own toggle, delivered on
/// the release) is handled first — or after CLICK_HOLD if the button stays down.
const CLICK_AFTER_RELEASE: Duration = Duration::from_millis(100);
const CLICK_HOLD: Duration = Duration::from_millis(400);

pub(super) fn spawn(app: AppHandle) {
    let r = std::thread::Builder::new().name("liqlip-picker-poll".into()).spawn(move || run(&app));
    if let Err(e) = r {
        crate::logx::warn("picker", &format!("poll thread: {e}"));
    }
}

fn run(app: &AppHandle) {
    let Some(p) = PICKER.get() else { return };
    let t0 = Instant::now();
    let mut hover = HoverTracker::default();
    let mut clicks = ClickTracker::default();
    let (mut seen_open, mut seen_disarm) = (u64::MAX, 0u64);
    // Display signature + the dock window it was taken for.
    let mut sig: Option<([i32; 14], Rect)> = None;
    // (pressed at, released at) of a click outside that has not closed the shelf yet.
    let mut pending_click: Option<(Instant, Option<Instant>)> = None;
    let mut redock_sent_for: Option<Rect> = None;
    loop {
        let (phase, dock, monitor, open_seq, disarm_seq) = {
            let mut s = p.state.lock();
            while !matches!(s.m.phase, Phase::Armed | Phase::Open { .. }) || s.dock.is_none() {
                p.wake.wait(&mut s);
            }
            let Some(dock) = s.dock.clone() else { continue };
            (s.m.phase, dock, s.m.cfg.monitor.clone(), s.m.open_seq, s.disarm_seq)
        };
        let buttons = win::mouse_buttons_down();
        if open_seq != seen_open {
            seen_open = open_seq;
            clicks.reset(buttons);
            pending_click = None;
        }
        if disarm_seq != seen_disarm {
            seen_disarm = disarm_seq;
            hover.disarm();
        }

        // Monitor plugged / unplugged, resolution, scale or taskbar changed → re-dock live.
        let now_sig = win::display_signature(&dock.screen);
        match sig {
            Some((old, w)) if w == dock.window && old != now_sig => {
                sig = Some((now_sig, dock.window));
                dispatch(app, Event::Redock);
            }
            _ => sig = Some((now_sig, dock.window)),
        }

        let mut tick = TICK_FAR;
        if let Some((x, y)) = win::cursor_pos() {
            match phase {
                Phase::Armed => {
                    // "Under the cursor": the armed edge follows the cursor's monitor.
                    if monitor == "cursor" && !dock.screen.contains(x, y) {
                        if redock_sent_for != Some(dock.window) {
                            redock_sent_for = Some(dock.window);
                            dispatch(app, Event::Redock);
                        }
                    } else if !dock.seam {
                        let on_edge = geometry::on_trigger_line(&dock, x, y);
                        let ms = t0.elapsed().as_millis() as u64;
                        if hover.tick(on_edge, buttons, ms) {
                            dispatch(app, Event::HoverTrigger);
                        }
                    }
                    if geometry::near_edge(&dock, x, y) {
                        tick = TICK_NEAR;
                    }
                }
                Phase::Open { mode, interactive } => {
                    tick = TICK_OPEN;
                    if clicks.tick(buttons, dock.panel.contains(x, y)) && !on_own_dialog(x, y) {
                        pending_click = Some((Instant::now(), None));
                    }
                    if let Some((pressed, released)) = pending_click.as_mut() {
                        if !buttons && released.is_none() {
                            *released = Some(Instant::now());
                        }
                        let due = released.is_some_and(|r| r.elapsed() >= CLICK_AFTER_RELEASE)
                            || pressed.elapsed() >= CLICK_HOLD;
                        if due {
                            pending_click = None;
                            dispatch(app, Event::ClickOutside);
                        }
                    }
                    if mode == PickerMode::Hover && !interactive && !geometry::keep_open_rect(&dock).contains(x, y) {
                        dispatch(app, Event::CursorLeft);
                    }
                }
                _ => {}
            }
        }
        let mut s = p.state.lock();
        let _ = p.wake.wait_for(&mut s, Duration::from_millis(tick));
    }
}

/// A click on a dialog of this process (not one of the three Tauri windows) — e.g. an
/// "Open with" dialog started from the shelf — is not a click outside.
fn on_own_dialog(x: i32, y: i32) -> bool {
    let Some(root) = win::root_window_at(x, y) else { return false };
    let ours = hwnds().is_some_and(|h| h.is_ours(root));
    !ours && win::is_own_process(root)
}
