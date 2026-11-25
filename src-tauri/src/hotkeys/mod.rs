//! Global hotkey engine — `RegisterHotKey` first, `WH_KEYBOARD_LL` only while a chord that
//! needs it is bound or the recorder is armed; native chord recording (ARCHITECTURE §8).
//! Owner: **W1-D**. Port of Liquid Tools `hotkeys.rs` without dictation, PrintScreen
//! special-casing, Escape interception, mouse buttons, bare modifiers and `Feature`.
//!
//! - `combo.rs` — grammar (`"Alt + V"` ⇄ VK + modifiers) and the binding plan (pure, tested).
//! - `native.rs` — registrar thread (owns registrations, `WM_HOTKEY`) and the hook thread.
//! - Fired actions and recorder results go through ONE dispatcher thread (`schedule`): the
//!   hook callback only enqueues (CLAUDE.md hard rule 3), the dispatcher emits events and
//!   starts each action on its own short-lived thread (a paste may block ~0.5 s).

pub mod commands;
mod combo;
#[cfg(windows)]
mod native;

use crate::model::{HotkeySettings, HudKind, SettingsError, SettingsErrorCode};
use crate::{capture, events, logx, paste, picker};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;
use tauri::AppHandle;

/// What a global hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    /// `picker::summon`
    Summon,
    /// `paste::stack::paste_next` (on a worker thread)
    StackNext,
    /// `capture::toggle_pause`
    PauseToggle,
    /// `capture::ignore_next`
    IgnoreNext,
}

/// Slot order = `combo::FIELDS` order.
pub(crate) const ACTIONS: [HotkeyAction; 4] = [
    HotkeyAction::Summon,
    HotkeyAction::StackNext,
    HotkeyAction::PauseToggle,
    HotkeyAction::IgnoreNext,
];

/// An armed recorder (between `hotkey_record_start` and a commit / cancel / stop).
#[derive(Default)]
pub(crate) struct Capture {
    /// Last sided modifier pressed with nothing after it.
    pub last_mod: Option<u32>,
    /// A chord made only of modifiers ("Ctrl + LWin"), committed when its last key lifts.
    pub modifier_chord: Option<String>,
}

/// Read by the hook callback with `try_lock` only.
pub(crate) static CAPTURE: Mutex<Option<Capture>> = Mutex::new(None);
/// Bumped on every `hotkey_record_start`, so a stale timeout cannot cancel a newer session.
static CAPTURE_GEN: AtomicU64 = AtomicU64::new(0);
/// An abandoned recorder (window closed mid-recording) must not keep the hotkeys suspended.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) enum Job {
    /// The hook swallowed a key while Alt / Win is held: tap the mask key so their release
    /// is not bare (`combo::masks_release`). Queued before the job it belongs to.
    Mask,
    Action(HotkeyAction),
    /// Recorder finished: a chord, or `""` = cancelled (Esc / timeout).
    Captured(String),
    CaptureTimeout(u64),
}

static JOBS: OnceLock<mpsc::Sender<Job>> = OnceLock::new();

/// Enqueue work for the dispatcher. Non-blocking — callable from the hook callback.
pub(crate) fn schedule(job: Job) {
    if let Some(tx) = JOBS.get() {
        let _ = tx.send(job);
    }
}

fn start_dispatcher(app: &AppHandle) {
    let (tx, rx) = mpsc::channel::<Job>();
    if JOBS.set(tx).is_err() {
        return; // already running
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("liqlip-hotkey-jobs".into())
        .spawn(move || {
            for job in rx {
                match job {
                    // Within a millisecond of the swallowed press — long before the user lets
                    // go of the modifier.
                    Job::Mask => paste::mask_modifier_release(),
                    Job::Action(action) => {
                        let app = app.clone();
                        let _ = std::thread::Builder::new()
                            .name("liqlip-hotkey-action".into())
                            .spawn(move || run_action(&app, action));
                    }
                    Job::Captured(combo) => {
                        events::hotkeys_captured(&app, &combo);
                        end_recording();
                    }
                    Job::CaptureTimeout(gen) => {
                        if CAPTURE_GEN.load(Ordering::SeqCst) == gen && CAPTURE.lock().take().is_some() {
                            logx::warn("hotkeys", "recorder timed out");
                            events::hotkeys_captured(&app, "");
                            end_recording();
                        }
                    }
                }
            }
        });
    if let Err(e) = spawned {
        logx::warn("hotkeys", &format!("dispatcher: {e}"));
    }
}

/// Run a fired action (its own thread; may block).
fn run_action(app: &AppHandle, action: HotkeyAction) {
    match action {
        HotkeyAction::Summon => picker::summon(app),
        // Blocking (≤ ~1.6 s) — this is the action's own thread. On the hotkey path
        // paste_next itself shows the HUD for an empty stack / focus_lost / elevated (W1-C).
        HotkeyAction::StackNext => {
            if let Err(e) = paste::stack::paste_next(app) {
                logx::warn("hotkeys", &format!("stack next: {e}"));
            }
        }
        HotkeyAction::PauseToggle => match capture::toggle_pause(app) {
            Ok(st) => {
                let kind = if st.paused_until.is_some() { HudKind::Paused } else { HudKind::Resumed };
                picker::hud::show(app, kind, None);
            }
            Err(e) => logx::warn("hotkeys", &format!("pause toggle: {e}")),
        },
        HotkeyAction::IgnoreNext => match capture::ignore_next(app) {
            Ok(st) if st.ignore_next => picker::hud::show(app, HudKind::SkipNext, None),
            Ok(_) => {}
            Err(e) => logx::warn("hotkeys", &format!("ignore next: {e}")),
        },
    }
}

/// Main (non-modifier) virtual key of a chord in the settings notation (`"Alt + Shift + V"` →
/// `0x56`, `"Ctrl + Num5"` → `0x65`), parsed by the same grammar that binds it. The stack-paste
/// flow waits for this key to be released before it types (W3-A): an autorepeat of a key it
/// did not know — `Num5`, arrows, `-` — would otherwise land in the target once the paste has
/// lifted the modifiers. `None` for an empty or unbindable chord.
pub fn main_vk(combo: &str) -> Option<u16> {
    combo::parse(combo.trim()).and_then(|c| u16::try_from(c.vk).ok())
}

fn to_errors(errs: &[Option<SettingsErrorCode>]) -> Vec<SettingsError> {
    errs.iter()
        .zip(combo::FIELDS)
        .filter_map(|(e, field)| e.map(|code| SettingsError { field: field.into(), code }))
        .collect()
}

fn merge(a: [Option<SettingsErrorCode>; 4], b: [Option<SettingsErrorCode>; 4]) -> [Option<SettingsErrorCode>; 4] {
    std::array::from_fn(|i| a[i].or(b[i]))
}

/// `setup()` step 7: start the dispatcher + registrar and bind the saved chords. A chord
/// taken by another app (Liquid Tools on `Alt + V`) stays wanted and is retried every 30 s;
/// it is reported here (logged by `lib.rs`) and by every `apply_settings`.
pub fn start(app: &AppHandle, hotkeys: &HotkeySettings) -> Vec<SettingsError> {
    start_dispatcher(app);
    let (want, parse_errs) = combo::desired(hotkeys, hotkeys);
    // `prev = want`: a refused chord stays wanted (pending) instead of falling back to "".
    let os_errs = native_apply(want, want);
    sync_hook();
    to_errors(&merge(parse_errs, os_errs))
}

/// Synchronous hook from `settings_set` (ARCHITECTURE §15): bind every changed action.
/// One error per rejected field (`hotkeys.<name>`): `invalid` — unparsable or the same chord
/// as another action; `taken` — `RegisterHotKey` refused (another app owns it). A rejected
/// field keeps its PREVIOUS binding, matching the revert `settings_set` applies. When nothing
/// changed it still reports fields whose chord is not active (startup conflict), after one
/// retry — see the `settings.rs` snippet in the W1-D report.
pub fn apply_settings(app: &AppHandle, prev: &HotkeySettings, next: &HotkeySettings) -> Vec<SettingsError> {
    let _ = app;
    let (want, parse_errs) = combo::desired(prev, next);
    let os_errs = if prev == next {
        native_status()
    } else {
        // Saving a chord ends any recording session (the recorder has committed).
        CAPTURE.lock().take();
        let prev_c = combo::strings(prev).map(|s| combo::parse(s.trim()));
        native_apply(prev_c, want)
    };
    sync_hook();
    to_errors(&merge(parse_errs, os_errs))
}

/// `hotkey_record_start`: suspend every global hotkey (so pressing the current chord records
/// it instead of firing), arm the recorder and make sure the hook is in. The result arrives
/// as `hotkeys:captured { combo }` (`""` = cancelled). Errors only when the native hook
/// cannot be installed — the webview recorder still works for ordinary chords then.
pub fn record_start() -> Result<(), String> {
    *CAPTURE.lock() = Some(Capture::default());
    let gen = CAPTURE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    native_suspend(true);
    std::thread::spawn(move || {
        std::thread::sleep(CAPTURE_TIMEOUT);
        schedule(Job::CaptureTimeout(gen));
    });
    sync_hook_result().map_err(|e| format!("failed: hook: {e}"))
}

/// `hotkey_record_stop` (idempotent): disarm and restore the global hotkeys.
pub fn record_stop() {
    CAPTURE.lock().take();
    end_recording();
}

fn end_recording() {
    native_suspend(false);
    sync_hook();
}

#[cfg(windows)]
fn native_apply(
    prev: [Option<combo::Combo>; 4],
    want: [Option<combo::Combo>; 4],
) -> [Option<SettingsErrorCode>; 4] {
    native::request(|reply| native::Request::Apply { prev, want, reply }).unwrap_or_else(|e| {
        logx::warn("hotkeys", &format!("apply: {e}"));
        // The engine is unreachable: every requested chord is unusable for now.
        want.map(|w| w.map(|_| SettingsErrorCode::Invalid))
    })
}

#[cfg(windows)]
fn native_status() -> [Option<SettingsErrorCode>; 4] {
    native::request(|reply| native::Request::Status { reply }).unwrap_or_else(|e| {
        logx::warn("hotkeys", &format!("status: {e}"));
        [None; 4]
    })
}

#[cfg(windows)]
fn native_suspend(suspend: bool) {
    let r = if suspend {
        native::request(|reply| native::Request::Suspend { reply })
    } else {
        native::request(|reply| native::Request::Resume { reply })
    };
    if let Err(e) = r {
        logx::warn("hotkeys", &format!("suspend({suspend}): {e}"));
    }
}

#[cfg(windows)]
fn sync_hook_result() -> Result<(), String> {
    let needed = native::has_hook_bindings() || CAPTURE.lock().is_some();
    native::sync_hook(needed)
}

#[cfg(not(windows))]
fn native_apply(_: [Option<combo::Combo>; 4], _: [Option<combo::Combo>; 4]) -> [Option<SettingsErrorCode>; 4] {
    [None; 4]
}
#[cfg(not(windows))]
fn native_status() -> [Option<SettingsErrorCode>; 4] {
    [None; 4]
}
#[cfg(not(windows))]
fn native_suspend(_: bool) {}
#[cfg(not(windows))]
fn sync_hook_result() -> Result<(), String> {
    Ok(())
}

fn sync_hook() {
    if let Err(e) = sync_hook_result() {
        logx::warn("hotkeys", &format!("hook: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_follow_field_order() {
        let errs = [None, Some(SettingsErrorCode::Taken), None, Some(SettingsErrorCode::Invalid)];
        let out = to_errors(&errs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].field, "hotkeys.stackNext");
        assert_eq!(out[0].code, SettingsErrorCode::Taken);
        assert_eq!(out[1].field, "hotkeys.ignoreNext");
    }

    #[test]
    fn parse_errors_win_over_os_errors() {
        let a = [Some(SettingsErrorCode::Invalid), None, None, None];
        let b = [Some(SettingsErrorCode::Taken), Some(SettingsErrorCode::Taken), None, None];
        assert_eq!(merge(a, b), [Some(SettingsErrorCode::Invalid), Some(SettingsErrorCode::Taken), None, None]);
    }

    /// W3-A: every main key the grammar binds is the key the paste waits for — not only the
    /// letters/F-keys the old ad-hoc parser knew.
    #[test]
    fn main_vk_follows_the_binding_grammar() {
        assert_eq!(main_vk("Alt + Shift + V"), Some(0x56));
        assert_eq!(main_vk("alt+shift+v"), Some(0x56));
        assert_eq!(main_vk("Ctrl + Num5"), Some(0x65));
        assert_eq!(main_vk("Ctrl + Down"), Some(0x28));
        assert_eq!(main_vk("Ctrl + Alt + -"), Some(0xBD));
        assert_eq!(main_vk("Ctrl + PgDn"), Some(0x22));
        assert_eq!(main_vk("Ctrl + Delete"), Some(0x2E));
        assert_eq!(main_vk("Alt + F12"), Some(0x7B));
        assert_eq!(main_vk("Ctrl + LWin"), Some(0x5B));
        assert_eq!(main_vk(" Super + Space "), Some(0x20));
        for unbound in ["", "  ", "LCtrl", "Ctrl +", "Alt + F25", "MMB"] {
            assert_eq!(main_vk(unbound), None, "{unbound:?}");
        }
        // Every key the native recorder can report is waited for.
        for vk in 1u32..=0xFF {
            if let Some(token) = combo::vk_token(vk) {
                let chord = combo::combo_string(combo::Mods { alt: true, shift: true, ..Default::default() }, &token);
                assert_eq!(main_vk(&chord), Some(vk as u16), "{chord}");
            }
        }
    }

    #[test]
    fn slots_match_fields() {
        assert_eq!(ACTIONS.len(), combo::FIELDS.len());
        assert_eq!(ACTIONS[0], HotkeyAction::Summon);
        assert_eq!(combo::FIELDS[1], "hotkeys.stackNext");
    }
}
