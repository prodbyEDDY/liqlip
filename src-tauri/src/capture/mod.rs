//! Capture — clipboard listener, one-OpenClipboard snapshot, privacy markers, owner +
//! app icons, classification, secrets, pause / skip / collect, worker pipeline
//! (ARCHITECTURE §5). Owner: **W1-B**.
//!
//! Threads (all supervised: a panic is logged and the loop restarts with back-off):
//! - `liqlip-clip-listener` — message-only window + `AddClipboardFormatListener`
//!   ([`listener`]); forwards `WM_CLIPBOARDUPDATE` only;
//! - `liqlip-clip-capture` — debounce, dedupe, pause/skip/self-copy, owner probe, snapshot on
//!   a per-snapshot reader thread with deadlines ([`pipeline`], [`snapshot`]);
//! - `liqlip-clip-worker` — classification, secrets, toggles, PNG, hash, `Store::insert_clip`,
//!   `clips:changed`, paste-stack push, app icon ([`pipeline`], [`icons`]).
//!
//! State changes (pause, skip, collect) emit `capture:status` (`events::capture_status`)
//! and call `crate::tray::refresh(app)` so the tray icon/menu follow.

#[cfg(windows)]
mod apps;
pub mod classify;
pub mod commands;
mod formats;
#[cfg(windows)]
mod icons;
mod imaging;
#[cfg(windows)]
mod listener;
mod pipeline;
pub(crate) mod secrets; // pub(crate): the import applies the same filter (FIX-01)
mod snapshot;
mod state;
mod textread;
#[cfg(windows)]
mod win32;

use crate::model::{CaptureStatus, PerKind, RunningApp, Settings};
use crate::{events, logx, tray};
use state::CaptureState;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

/// The settings capture reads on every change (privacy + storage types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureConfig {
    pub respect_markers: bool,
    /// Exe names or full paths (matching: `snapshot::is_ignored`).
    pub ignored_apps: Vec<String>,
    pub secret_patterns: bool,
    pub types: PerKind<bool>,
}

impl From<&Settings> for CaptureConfig {
    fn from(s: &Settings) -> Self {
        CaptureConfig {
            respect_markers: s.privacy.respect_markers,
            ignored_apps: s.privacy.ignored_apps.clone(),
            secret_patterns: s.privacy.secret_patterns,
            types: s.storage.types,
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The app-wide capture state (module-internal static; `mark_own_write` has no `AppHandle`
/// by contract, ARCHITECTURE §15 rule 1).
fn state() -> &'static Arc<CaptureState> {
    static STATE: OnceLock<Arc<CaptureState>> = OnceLock::new();
    STATE.get_or_init(|| Arc::new(CaptureState::new()))
}

/// `capture:status` to the shelf + settings, and the tray (pause icon/menu).
fn publish(app: &AppHandle, status: &CaptureStatus) {
    events::capture_status(app, status);
    tray::refresh(app);
}

// ── Runtime (threads) ───────────────────────────────────────────────────────

#[cfg(windows)]
mod runtime {
    use super::pipeline::{self, LoopDeps, LoopMsg, Sink, WorkItem};
    use super::snapshot::Timing;
    use super::{icons, listener, now_ms, publish, state, win32, CaptureConfig};
    use crate::model::{ChangeOrigin, ClipsChange, ClipsChanged};
    use crate::store::{InsertOutcome, NewClip, Store};
    use crate::{events, logx, paste, settings};
    use parking_lot::Mutex;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;
    use tauri::{AppHandle, Manager};

    struct Runtime {
        loop_tx: Sender<LoopMsg>,
        listener: Mutex<Option<listener::Listener>>,
        stopping: AtomicBool,
    }

    fn runtime() -> &'static OnceLock<Runtime> {
        static RT: OnceLock<Runtime> = OnceLock::new();
        &RT
    }

    /// Run `body` until it returns normally; a panic or an `Err` is logged and the body is
    /// restarted after a growing back-off (1 s → 30 s), unless the app is stopping.
    fn supervise(name: &'static str, mut body: impl FnMut() -> Result<(), String> + Send + 'static) {
        let spawned = std::thread::Builder::new().name(format!("liqlip-clip-{name}")).spawn(move || {
            let mut backoff = Duration::from_secs(1);
            loop {
                match catch_unwind(AssertUnwindSafe(&mut body)) {
                    Ok(Ok(())) => return,
                    Ok(Err(e)) => logx::warn("capture", &format!("{name} failed: {e}; restarting")),
                    Err(_) => logx::warn("capture", &format!("{name} panicked; restarting")),
                }
                if runtime().get().is_some_and(|r| r.stopping.load(Ordering::SeqCst)) {
                    return;
                }
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        });
        if let Err(e) = spawned {
            logx::warn("capture", &format!("cannot start {name} thread: {e}"));
        }
    }

    /// Store + events + paste stack + icons for the worker.
    struct AppSink {
        app: AppHandle,
    }

    impl Sink for AppSink {
        fn insert(&mut self, clip: NewClip) -> Result<InsertOutcome, String> {
            let store = self.app.try_state::<Store>().ok_or_else(|| "failed: store not ready".to_string())?;
            store.insert_clip(clip)
        }

        fn inserted(&mut self, outcome: &InsertOutcome, exe: Option<&str>, collect: bool) {
            events::clips_changed(
                &self.app,
                &ClipsChanged {
                    origin: ChangeOrigin::Capture,
                    change: if outcome.created { ClipsChange::Added } else { ClipsChange::Updated },
                    ids: Some(vec![outcome.id]),
                },
            );
            if collect {
                if let Err(e) = paste::stack::push(&self.app, outcome.id) {
                    logx::warn("capture", &format!("paste stack push: {e}"));
                }
            }
            match (outcome.app_icon_missing, outcome.app_id, exe) {
                (true, Some(app_id), Some(exe)) => {
                    // Blocking is fine here (worker thread, once per app).
                    if let Some(icon) = icons::png_for(exe) {
                        if let Some(store) = self.app.try_state::<Store>() {
                            if let Err(e) = store.set_app_icon(app_id, &icon.png) {
                                logx::warn("capture", &format!("set_app_icon({app_id}): {e}"));
                            }
                        }
                    }
                }
                // Warm the in-memory cache (background) so `app_icon_data_url` on the shelf
                // summon path is a hit for apps the user copies from.
                (_, _, Some(exe)) => {
                    let _ = icons::data_url_cached(exe);
                }
                _ => {}
            }
        }

        fn app_name(&mut self, exe: &str) -> String {
            win32::app_name(exe)
        }
    }

    pub fn start(app: &AppHandle) {
        let (loop_tx, loop_rx) = mpsc::channel::<LoopMsg>();
        let (work_tx, work_rx) = mpsc::channel::<WorkItem>();
        let rt = Runtime { loop_tx: loop_tx.clone(), listener: Mutex::new(None), stopping: AtomicBool::new(false) };
        if runtime().set(rt).is_err() {
            logx::warn("capture", "start called twice; ignored");
            return;
        }
        let api = Arc::new(win32::Win32Clipboard::new());

        // Worker: survives panics with its channel intact.
        {
            let app = app.clone();
            let rx = Arc::new(Mutex::new(work_rx));
            supervise("worker", move || {
                let rx = rx.lock();
                let mut sink = AppSink { app: app.clone() };
                pipeline::run_worker(&rx, &mut sink, state());
                Ok(())
            });
        }

        // Capture loop.
        {
            let app = app.clone();
            let rx: Arc<Mutex<Receiver<LoopMsg>>> = Arc::new(Mutex::new(loop_rx));
            supervise("capture", move || {
                let rx = rx.lock();
                let deps = LoopDeps {
                    api: api.clone(),
                    state: state().clone(),
                    config: Box::new({
                        let app = app.clone();
                        move || CaptureConfig::from(&settings::get(&app))
                    }),
                    on_status: Box::new({
                        let app = app.clone();
                        move |s| publish(&app, &s)
                    }),
                    worker: work_tx.clone(),
                    timing: Timing::default(),
                    now_ms,
                };
                pipeline::run_capture_loop(&rx, deps);
                Ok(())
            });
        }

        // Listener: (re)creates its window on every restart.
        supervise("listener", move || {
            let r = listener::run(loop_tx.clone(), |l| {
                if let Some(rt) = runtime().get() {
                    *rt.listener.lock() = Some(l);
                }
            });
            if let Some(rt) = runtime().get() {
                *rt.listener.lock() = None;
                if rt.stopping.load(Ordering::SeqCst) {
                    return Ok(());
                }
            }
            // A clean exit we did not ask for is unexpected: restart.
            r.and(Err("message loop ended".into()))
        });
    }

    pub fn stop() {
        let Some(rt) = runtime().get() else { return };
        rt.stopping.store(true, Ordering::SeqCst);
        if let Some(l) = rt.listener.lock().take() {
            l.stop();
        }
        let _ = rt.loop_tx.send(LoopMsg::Stop);
    }
}

/// `setup()` step 5: start the listener thread + capture loop + worker (needs `Store` and
/// settings managed). Returns immediately.
pub fn start(app: &AppHandle) {
    #[cfg(windows)]
    runtime::start(app);
    #[cfg(not(windows))]
    let _ = app;
}

/// `RunEvent::Exit`: remove the clipboard listener (`RemoveClipboardFormatListener`) and stop
/// the loops. Safe to call more than once or without `start`.
#[allow(dead_code)] // coordinator wires it into lib.rs (snippet in the W1-B report)
pub fn stop() {
    #[cfg(windows)]
    runtime::stop();
}

/// Self-copy guard (§5): the paste writer calls this with `GetClipboardSequenceNumber()`
/// taken right after its own `CloseClipboard`; the listener ignores that update.
/// Callable from any thread without an `AppHandle`.
#[allow(dead_code)] // W1-C calls it
pub fn mark_own_write(seq: u32) {
    state().mark_own(seq);
}

/// Current clipboard sequence number (`GetClipboardSequenceNumber`), for writers that want
/// to call [`mark_own_write`] without their own Win32 binding. 0 = unavailable.
#[allow(dead_code)] // W1-C may use it
pub fn clipboard_sequence() -> u32 {
    #[cfg(windows)]
    return win32::sequence_number();
    #[cfg(not(windows))]
    0
}

/// The clipboard's current `CF_UNICODETEXT` (`None` = no text on it) for `{clipboard}` in
/// snippets — command `clipboard_text` (FIX-09, W3-A; additive §15 API). One short, retried
/// `OpenClipboard` behind an owner probe, the read on its own thread with a deadline
/// ([`textread`]); privacy markers are not applied (an explicit user action, nothing is
/// stored or logged). `Err("busy: …")` when the clipboard is held, the owner hangs or the
/// read times out. Blocking (≤ ≈ 1.5 s) — call from the blocking pool.
pub fn clipboard_text() -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        static API: OnceLock<Arc<win32::Win32Clipboard>> = OnceLock::new();
        let api = API.get_or_init(|| Arc::new(win32::Win32Clipboard::new()));
        textread::current_text(api, &snapshot::Timing::default()).map_err(textread::TextError::to_command_error)
    }
    #[cfg(not(windows))]
    Ok(None)
}

/// Current recording state.
pub fn status(app: &AppHandle) -> CaptureStatus {
    let _ = app;
    state().status(now_ms())
}

/// Auto-resume: wakes at the deadline (in ≤ 30 s steps, so a suspended machine resumes on
/// time after wake-up) and emits the change unless another pause/resume superseded it.
fn arm_resume_timer(app: &AppHandle, timer: state::ResumeTimer) {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("liqlip-clip-resume".into()).spawn(move || loop {
        if !state().timer_alive(timer) {
            return;
        }
        let now = now_ms();
        if now >= timer.deadline {
            if let Some(s) = state().expire(timer, now) {
                publish(&app, &s);
            }
            return;
        }
        let left = (timer.deadline - now).clamp(1, 30_000) as u64;
        std::thread::sleep(Duration::from_millis(left));
    });
    if let Err(e) = spawned {
        logx::warn("capture", &format!("cannot start the resume timer: {e}"));
    }
}

/// Pause for `minutes` (`None` = until resumed). Tray, hotkey and `capture_pause` use it.
pub fn pause(app: &AppHandle, minutes: Option<i64>) -> Result<CaptureStatus, String> {
    let (status, timer) = state().pause(minutes, now_ms())?;
    if let Some(t) = timer {
        arm_resume_timer(app, t);
    }
    publish(app, &status);
    Ok(status)
}

/// Resume recording.
pub fn resume(app: &AppHandle) -> Result<CaptureStatus, String> {
    let status = state().resume();
    publish(app, &status);
    Ok(status)
}

/// Pause until resumed if recording, resume if paused (hotkey `PauseToggle`).
#[allow(dead_code)] // W1-D calls it from the hotkey engine
pub fn toggle_pause(app: &AppHandle) -> Result<CaptureStatus, String> {
    let status = state().toggle_pause(now_ms());
    publish(app, &status);
    Ok(status)
}

/// Skip the next clipboard change (one-shot). Tray, hotkey and `capture_ignore_next`.
pub fn ignore_next(app: &AppHandle) -> Result<CaptureStatus, String> {
    let status = state().set_ignore_next();
    publish(app, &status);
    Ok(status)
}

/// Disarm a pending "skip the next change" (W2-F, additive — FIX-05: the tray's checked
/// "Skip next copy" item clicked again). Emits `capture:status` + refreshes the tray.
pub fn cancel_ignore_next(app: &AppHandle) -> Result<CaptureStatus, String> {
    let status = state().clear_ignore_next();
    publish(app, &status);
    Ok(status)
}

/// "Collect copies into the paste stack" mode (`capture_set_collect`).
pub fn set_collect(app: &AppHandle, value: bool) -> Result<CaptureStatus, String> {
    let status = state().set_collect(value);
    publish(app, &status);
    Ok(status)
}

/// `data:image/png;base64,…` icon of an executable from the capture icon cache
/// (extracted once per exe, failures remembered). Used by paste on the shelf summon path
/// (`TargetInfo.iconUrl`), so it **never blocks**: a hit is a map lookup + a copy of the
/// pre-encoded URL; a miss returns `None` at once and extracts in the background (the next
/// call — the next summon — gets the icon). The capture worker pre-warms the cache for every
/// app the user copies from.
#[allow(dead_code)] // W1-C calls it
pub fn app_icon_data_url(exe: &str) -> Option<String> {
    #[cfg(windows)]
    return icons::data_url_cached(exe);
    #[cfg(not(windows))]
    {
        let _ = exe;
        None
    }
}

/// Visible top-level apps (ignored-apps picker). Blocking.
pub fn running_apps() -> Vec<RunningApp> {
    #[cfg(windows)]
    return apps::running();
    #[cfg(not(windows))]
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_follows_settings() {
        let mut s = Settings::default();
        let c = CaptureConfig::from(&s);
        assert!(c.respect_markers && c.secret_patterns);
        assert_eq!(c.ignored_apps.len(), 4);
        assert!(Kind::ALL.iter().all(|k| *c.types.get(*k)));
        s.privacy.respect_markers = false;
        s.storage.types.image = false;
        let c = CaptureConfig::from(&s);
        assert!(!c.respect_markers && !c.types.image);
    }

    use crate::model::Kind;
}
