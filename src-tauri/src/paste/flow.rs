//! The paste flow (ARCHITECTURE §6 "Поток вставки", SPEC §6) — owner **W1-C**.
//!
//! A pure planner turns (where the paste comes from, what the target is) into an ordered list
//! of [`Step`]s; the executor runs them. The order is unit-tested — it IS the guarantee:
//!
//! shelf, pasteable target:
//!   Write → WaitKeys → FocusTarget → HideShelf → Settle → Inject → After → HudIfNotPasted
//!
//! - **Write first**, while the shelf is still visible: a busy clipboard fails before anything
//!   moves, and the shelf can show the error (`failed / clipboard_busy`).
//! - **WaitKeys**: Enter / Ctrl+N / the hotkey key must be released before focus moves, or
//!   its autorepeat would land in the target ("double Enter", "a typed v").
//! - **FocusTarget before HideShelf**: `SetForegroundWindow(target)` while our process still
//!   owns the foreground is always permitted; then the (now inactive) shelf hides without
//!   triggering a system re-activation of some other window. Poll ≤ 500 ms for
//!   `GetForegroundWindow() == target`.
//! - **Settle** 30 ms, then **Inject** re-checks the foreground right before `SendInput`;
//!   focus not confirmed → no input at all, the data stays on the clipboard, HUD "copied".

use super::focus::{self, Target};
use super::input;
use super::writer::{self, Entry, WriteError};
use crate::model::{HudKind, PasteChord, PasteOutcome, PasteReason, PasteResult};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// Where the paste was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// From the shelf; the target is the window remembered at summon.
    Shelf { keep_open: bool },
    /// Without the shelf (stack hotkey): the target is the current foreground window.
    Foreground,
}

/// What the target allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// No target (desktop, own window, gone) → copy only.
    NoTarget,
    /// Higher integrity level (UIPI) → copy only, the user presses Ctrl+V.
    Elevated,
    /// Paste.
    Paste,
}

pub fn route_for(target: Option<&Target>) -> Route {
    match target {
        None => Route::NoTarget,
        Some(t) if t.info.elevated => Route::Elevated,
        Some(_) => Route::Paste,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// One clipboard transaction (+ self-copy mark). Failure aborts the whole plan.
    Write,
    /// Wait (≤ 1 s) until Enter / 1–9 / the hotkey's main key are released.
    WaitKeys,
    /// keepOpen: tell the shelf's focus-loss exit to ignore the coming deactivation.
    BeginHandoff,
    /// `SetForegroundWindow(target)` + confirm (≤ 500 ms). Failure skips Settle/Inject/After.
    FocusTarget,
    /// `picker::hide_now` — no exit animation, no focus restore.
    HideShelf,
    /// `picker::close(restore)` — the Esc close: focus goes back where the shelf was summoned
    /// from (no target → nothing was typed, the user stays where they were).
    CloseShelf,
    /// 30 ms between the clipboard write / focus change and the input.
    Settle,
    /// Re-check foreground == target; release held modifiers, chord, restore them.
    Inject,
    /// After a successful paste: caret ← × n (`{cursor}`) or Enter (`stack.newlineAfterPaste`).
    After,
    /// keepOpen: activate the shelf again.
    ReturnToShelf,
    EndHandoff,
    Hud(HudKind),
    /// HUD "copied — press Ctrl+V" if the input was not delivered (or "elevated" if UIPI
    /// blocked it).
    HudIfNotPasted,
}

/// The ordered steps for one paste.
pub fn plan(origin: Origin, route: Route, close_on_copy: bool) -> Vec<Step> {
    use Step::*;
    match (origin, route) {
        (Origin::Shelf { keep_open: false }, Route::Paste) => {
            vec![Write, WaitKeys, FocusTarget, HideShelf, Settle, Inject, After, HudIfNotPasted]
        }
        (Origin::Shelf { keep_open: true }, Route::Paste) => {
            vec![Write, WaitKeys, BeginHandoff, FocusTarget, Settle, Inject, After, ReturnToShelf, EndHandoff]
        }
        // Copy-only for an admin window, but hand focus back to it so Ctrl+V is one keystroke
        // away (SetForegroundWindow is not blocked by UIPI; input is).
        (Origin::Shelf { keep_open: false }, Route::Elevated) => vec![Write, WaitKeys, FocusTarget, HideShelf, Hud(HudKind::Elevated)],
        (Origin::Shelf { keep_open: true }, Route::Elevated) => vec![Write],
        // No target: it is a copy — the shelf closes per `picker.closeOnCopy` and gives focus
        // back, exactly like `Ctrl+Enter` (SPEC §3.3).
        (Origin::Shelf { keep_open: false }, Route::NoTarget) if close_on_copy => vec![Write, CloseShelf, Hud(HudKind::Copied)],
        (Origin::Shelf { .. }, Route::NoTarget) => vec![Write],
        (Origin::Foreground, Route::Paste) => vec![Write, WaitKeys, FocusTarget, Settle, Inject, After, HudIfNotPasted],
        (Origin::Foreground, Route::Elevated) => vec![Write, Hud(HudKind::Elevated)],
        (Origin::Foreground, Route::NoTarget) => vec![Write, Hud(HudKind::Copied)],
    }
}

/// What to type after the paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AfterPaste {
    #[default]
    Nothing,
    CaretBack(u32),
    Enter,
}

/// Everything the executor needs.
pub struct Job<'a> {
    pub origin: Origin,
    pub target: Option<Target>,
    pub entries: &'a [Entry],
    pub chord: PasteChord,
    pub after: AfterPaste,
    /// Extra keys to wait for (the hotkey's main key).
    pub wait_keys: Vec<u16>,
    pub close_on_copy: bool,
}

pub const FOCUS_TIMEOUT: Duration = Duration::from_millis(500);
pub const SETTLE: Duration = Duration::from_millis(30);
pub const KEYS_TIMEOUT: Duration = Duration::from_millis(1000);
/// Between the chord and the caret / Enter keys: the target processes its paste first.
pub const AFTER_DELAY: Duration = Duration::from_millis(40);

/// Result of running a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunResult {
    Done(PasteOutcome),
    /// The clipboard write failed — nothing else happened.
    WriteFailed(WriteError),
}

fn outcome(result: PasteResult, reason: Option<PasteReason>) -> PasteOutcome {
    PasteOutcome { result, reason }
}

/// Why the input step did not deliver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Miss {
    FocusLost,
    Blocked,
}

/// After `WaitKeys`: a trigger key (Enter, 1–9, the hotkey's key) still held after 1 s means
/// focus must NOT move — its autorepeat would land in whatever gets the focus. That holds for
/// an administrator window too (W3-A: the elevated route used to hand it focus anyway, and a
/// held Enter then reached, say, an admin console): `FocusTarget` and `Inject` are skipped,
/// the data stays on the clipboard.
fn miss_after_wait(released: bool) -> Option<Miss> {
    (!released).then_some(Miss::FocusLost)
}

/// Runs its closure when dropped — on every exit path of a run, a panic in a step included.
struct OnDrop<F: FnMut()>(F);
impl<F: FnMut()> Drop for OnDrop<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

/// Run a paste. Serialized process-wide (a second paste waits for the first). Blocking:
/// call from a worker / `spawn_blocking`.
pub fn run(app: &AppHandle, job: Job<'_>) -> RunResult {
    static LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    let _serial = LOCK.lock();
    let route = route_for(job.target.as_ref());
    let steps = plan(job.origin, route, job.close_on_copy);
    let started = Instant::now();
    let mut miss: Option<Miss> = None;
    let mut injected = false;
    // A hand-off flag left set (a step panicked between Begin/EndHandoff) would turn every
    // later focus loss of the shelf into "stay open, passive" — it could no longer close by a
    // click elsewhere (W3-A). Cleared on every exit path.
    let _handoff = steps.contains(&Step::BeginHandoff).then(|| OnDrop(|| focus::set_handoff(false)));

    for step in steps {
        match step {
            Step::Write => {
                if let Err(e) = writer::write(job.entries) {
                    crate::logx::warn("paste", &format!("clipboard write failed: {e:?}"));
                    return RunResult::WriteFailed(e);
                }
            }
            Step::WaitKeys => {
                let keys = input::trigger_keys(&job.wait_keys);
                #[cfg(windows)]
                if let Some(m) = miss_after_wait(input::wait_released(&keys, KEYS_TIMEOUT)) {
                    crate::logx::warn("paste", "trigger key still held; focus stays, input skipped");
                    miss = Some(m);
                }
                #[cfg(not(windows))]
                let _ = keys;
            }
            Step::BeginHandoff => focus::set_handoff(true),
            Step::EndHandoff => focus::set_handoff(false),
            Step::FocusTarget => {
                if miss.is_some() {
                    continue;
                }
                let ok = job.target.as_ref().is_some_and(|t| bring_to_front(t.hwnd));
                if !ok && route == Route::Paste {
                    crate::logx::warn("paste", "target did not become foreground within 500 ms");
                    miss = Some(Miss::FocusLost);
                }
            }
            Step::HideShelf => crate::picker::hide_now(app),
            Step::CloseShelf => crate::picker::close(app, true),
            Step::Settle => {
                if miss.is_none() {
                    std::thread::sleep(SETTLE);
                }
            }
            Step::Inject => {
                if miss.is_some() {
                    continue;
                }
                match job.target.as_ref().map(|t| inject(t.hwnd, job.chord)) {
                    Some(Ok(())) => injected = true,
                    Some(Err(m)) => miss = Some(m),
                    None => miss = Some(Miss::FocusLost),
                }
            }
            Step::After => {
                if injected {
                    if let Some(t) = job.target.as_ref() {
                        after_paste(t.hwnd, job.after);
                    }
                }
            }
            Step::ReturnToShelf => {
                if let Some(h) = focus::picker_hwnd(app) {
                    if !bring_to_front(h) {
                        crate::logx::warn("paste", "keepOpen: the shelf could not be re-activated");
                    }
                }
            }
            Step::Hud(kind) => hud(app, kind, &job),
            Step::HudIfNotPasted => match miss {
                Some(Miss::FocusLost) => hud(app, HudKind::Copied, &job),
                Some(Miss::Blocked) => hud(app, HudKind::Elevated, &job),
                None => {}
            },
        }
    }
    let result = match (route, miss) {
        (Route::NoTarget, _) => outcome(PasteResult::Copied, Some(PasteReason::NoTarget)),
        (Route::Elevated, _) | (Route::Paste, Some(Miss::Blocked)) => outcome(PasteResult::Copied, Some(PasteReason::Elevated)),
        (Route::Paste, Some(Miss::FocusLost)) => outcome(PasteResult::Copied, Some(PasteReason::FocusLost)),
        (Route::Paste, None) => outcome(PasteResult::Pasted, None),
    };
    crate::logx::warn(
        "paste",
        &format!("{:?} {:?} → {:?}/{:?} in {} ms", job.origin, route, result.result, result.reason, started.elapsed().as_millis()),
    );
    RunResult::Done(result)
}

/// `HudShow.text` of a toast that asks the user to paste ("copied / administrator window —
/// press {keys}"): the chord that pastes in the target app (`input::resolve_chord`: the
/// per-app setting, then the built-ins, then Ctrl+V), FIX-07. `None` — no target, or a toast
/// without keys — and the HUD says Ctrl+V.
fn hud_keys(kind: HudKind, has_target: bool, chord: PasteChord) -> Option<String> {
    (has_target && matches!(kind, HudKind::Copied | HudKind::Elevated)).then(|| input::chord_label(chord).to_string())
}

fn hud(app: &AppHandle, kind: HudKind, job: &Job<'_>) {
    crate::picker::hud::show(app, kind, hud_keys(kind, job.target.is_some(), job.chord));
}

fn bring_to_front(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        focus::bring_to_front(hwnd, FOCUS_TIMEOUT)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

/// Foreground re-check → release held modifiers → chord → restore them, one SendInput batch.
fn inject(hwnd: isize, chord: PasteChord) -> Result<(), Miss> {
    #[cfg(windows)]
    {
        if focus::foreground() != hwnd {
            return Err(Miss::FocusLost);
        }
        let layout = input::foreground_layout(hwnd);
        let seq = input::paste_sequence(&input::held_modifiers(), chord);
        let sent = input::send(&seq, layout);
        if sent == seq.len() {
            return Ok(());
        }
        if sent > 0 {
            // Partly inserted: make sure no synthetic Ctrl/Shift stays down (a user modifier
            // left released is corrected by its own physical release).
            input::send(&input::chord_cleanup(chord), layout);
            crate::logx::warn("paste", &format!("SendInput inserted {sent}/{}", seq.len()));
            return Err(Miss::FocusLost);
        }
        // Nothing inserted and no error reported: UIPI (SendInput docs) or BlockInput.
        crate::logx::warn("paste", "SendInput inserted nothing (UIPI?)");
        Err(Miss::Blocked)
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, chord);
        Err(Miss::FocusLost)
    }
}

/// Caret / Enter after the paste, only into the still-confirmed target.
fn after_paste(hwnd: isize, after: AfterPaste) {
    let keys = match after {
        AfterPaste::Nothing => return,
        AfterPaste::CaretBack(0) => return,
        AfterPaste::CaretBack(n) => input::taps(input::VK_LEFT, n.min(input::MAX_CARET_BACK)),
        AfterPaste::Enter => input::taps(input::VK_RETURN, 1),
    };
    std::thread::sleep(AFTER_DELAY);
    #[cfg(windows)]
    {
        if focus::foreground() != hwnd {
            crate::logx::warn("paste", "target lost focus before the after-paste keys");
            return;
        }
        let layout = input::foreground_layout(hwnd);
        input::send(&input::wrapped(&input::held_modifiers(), &keys), layout);
    }
    #[cfg(not(windows))]
    let _ = (hwnd, keys);
}

#[cfg(test)]
mod tests {
    use super::Step::*;
    use super::*;
    use crate::model::TargetInfo;

    const SHELF: Origin = Origin::Shelf { keep_open: false };
    const KEEP: Origin = Origin::Shelf { keep_open: true };

    fn target(elevated: bool) -> Target {
        Target {
            hwnd: 1,
            pid: 2,
            info: TargetInfo { exe: "c:\\x.exe".into(), name: "X".into(), title: "t".into(), icon_url: None, elevated },
        }
    }

    #[test]
    fn route_from_target() {
        assert_eq!(route_for(None), Route::NoTarget);
        assert_eq!(route_for(Some(&target(true))), Route::Elevated);
        assert_eq!(route_for(Some(&target(false))), Route::Paste);
    }

    #[test]
    fn shelf_paste_order_is_the_guarantee() {
        let p = plan(SHELF, Route::Paste, true);
        assert_eq!(p, vec![Write, WaitKeys, FocusTarget, HideShelf, Settle, Inject, After, HudIfNotPasted]);
        let pos = |s: Step| p.iter().position(|x| *x == s).unwrap();
        assert!(pos(Write) < pos(FocusTarget), "a busy clipboard fails before focus moves");
        assert!(pos(WaitKeys) < pos(FocusTarget), "trigger keys released before focus moves");
        assert!(pos(FocusTarget) < pos(Settle) && pos(Settle) < pos(Inject), "focus → 30 ms → input");
        assert!(pos(HideShelf) < pos(Inject), "the shelf is gone before typing");
    }

    #[test]
    fn keep_open_never_hides_and_returns_to_the_shelf() {
        let p = plan(KEEP, Route::Paste, true);
        assert!(!p.contains(&HideShelf));
        assert_eq!(p.first(), Some(&Write));
        assert!(p.iter().position(|s| *s == BeginHandoff) < p.iter().position(|s| *s == FocusTarget));
        assert_eq!(&p[p.len() - 2..], &[ReturnToShelf, EndHandoff]);
        assert_eq!(plan(KEEP, Route::Elevated, true), vec![Write]);
        assert_eq!(plan(KEEP, Route::NoTarget, true), vec![Write]);
    }

    #[test]
    fn elevated_and_missing_targets_never_inject() {
        for origin in [SHELF, KEEP, Origin::Foreground] {
            for route in [Route::Elevated, Route::NoTarget] {
                for close in [true, false] {
                    let p = plan(origin, route, close);
                    assert!(!p.contains(&Inject) && !p.contains(&After), "{origin:?} {route:?}");
                    assert_eq!(p[0], Write);
                }
            }
        }
        assert_eq!(plan(SHELF, Route::Elevated, true), vec![Write, WaitKeys, FocusTarget, HideShelf, Hud(HudKind::Elevated)]);
        assert_eq!(plan(Origin::Foreground, Route::Elevated, true), vec![Write, Hud(HudKind::Elevated)]);
    }

    #[test]
    fn no_target_follows_close_on_copy() {
        // W3-A: a copy from the shelf gives focus back where it was summoned from (SPEC §3.3),
        // like Ctrl+Enter — the Esc close, not the paste's silent hide.
        assert_eq!(plan(SHELF, Route::NoTarget, true), vec![Write, CloseShelf, Hud(HudKind::Copied)]);
        assert_eq!(plan(SHELF, Route::NoTarget, false), vec![Write]);
        assert_eq!(plan(Origin::Foreground, Route::NoTarget, true), vec![Write, Hud(HudKind::Copied)]);
        // Only the no-target copy closes that way; a paste never restores focus over its target.
        for (origin, route) in [(SHELF, Route::Paste), (SHELF, Route::Elevated), (KEEP, Route::Paste), (Origin::Foreground, Route::Paste)] {
            assert!(!plan(origin, route, true).contains(&CloseShelf), "{origin:?} {route:?}");
        }
    }

    /// W3-A: a trigger key still held after the wait keeps focus where it is — on every route
    /// that would move it (the elevated one included), and every such plan waits first.
    #[test]
    fn held_trigger_key_blocks_the_focus_move_on_every_route() {
        assert_eq!(miss_after_wait(true), None);
        assert_eq!(miss_after_wait(false), Some(Miss::FocusLost));
        for origin in [SHELF, KEEP, Origin::Foreground] {
            for route in [Route::Paste, Route::Elevated, Route::NoTarget] {
                for close in [true, false] {
                    let p = plan(origin, route, close);
                    if let Some(focus) = p.iter().position(|s| *s == FocusTarget) {
                        let wait = p.iter().position(|s| *s == WaitKeys);
                        assert!(wait.is_some_and(|w| w < focus), "{origin:?} {route:?}: {p:?}");
                    }
                }
            }
        }
    }

    /// W3-A: the run's clean-up (the hand-off flag) also runs when a step panics. A local flag
    /// stands in for the process-wide one (other tests toggle that in parallel).
    #[test]
    fn cleanup_runs_when_a_step_unwinds() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let flag = AtomicBool::new(true);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _reset = OnDrop(|| flag.store(false, Ordering::SeqCst));
            panic!("a step failed");
        }));
        assert!(r.is_err());
        assert!(!flag.load(Ordering::SeqCst));
        let flag = AtomicBool::new(true);
        drop(OnDrop(|| flag.store(false, Ordering::SeqCst)));
        assert!(!flag.load(Ordering::SeqCst), "and on the normal path");
    }

    /// FIX-07: the HUD names the chord of the target app, not always Ctrl+V.
    #[test]
    fn hud_names_the_chord_of_the_target() {
        assert_eq!(hud_keys(HudKind::Copied, true, PasteChord::ShiftInsert).as_deref(), Some("Shift+Insert"));
        assert_eq!(hud_keys(HudKind::Elevated, true, PasteChord::CtrlShiftV).as_deref(), Some("Ctrl+Shift+V"));
        assert_eq!(hud_keys(HudKind::Copied, true, PasteChord::CtrlV).as_deref(), Some("Ctrl+V"));
        assert_eq!(hud_keys(HudKind::Copied, false, PasteChord::ShiftInsert), None, "no target → the HUD's Ctrl+V");
        assert_eq!(hud_keys(HudKind::StackEmpty, true, PasteChord::ShiftInsert), None, "a toast without keys");
        // The chord comes from the same resolver as the input (per-app setting → built-in).
        let mintty = input::resolve_chord("c:\\program files\\git\\usr\\bin\\mintty.exe", &[]);
        assert_eq!(hud_keys(HudKind::Copied, true, mintty).as_deref(), Some("Shift+Insert"));
    }

    #[test]
    fn foreground_paste_has_no_shelf_steps() {
        let p = plan(Origin::Foreground, Route::Paste, true);
        assert_eq!(p, vec![Write, WaitKeys, FocusTarget, Settle, Inject, After, HudIfNotPasted]);
    }
}
