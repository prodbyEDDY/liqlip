//! Capture state (SPEC §2.4 items 4–5, §3.11 collect mode): pause with optional deadline,
//! one-shot "skip next copy", "collect copies into the paste stack", and the self-copy
//! sequence numbers handed in by the paste writer (`mark_own_write`). Pure and
//! thread-safe; `mod.rs` owns the single app instance, emits `capture:status` and runs the
//! auto-resume timer. Times are Unix ms.

use crate::model::CaptureStatus;
use parking_lot::Mutex;
use std::collections::VecDeque;

/// `CaptureStatus.pausedUntil` value for "paused until resumed".
pub const PAUSED_INDEFINITELY: i64 = -1;
/// Longest timed pause accepted by `capture_pause` (a year; "until resumed" is `null`).
pub const MAX_PAUSE_MINUTES: i64 = 525_600;
/// Own writes remembered (a "paste all" or a burst of copies from the shelf).
const OWN_SEQS: usize = 64;

#[derive(Debug, Default)]
struct Inner {
    paused_until: Option<i64>,
    ignore_next: bool,
    collect: bool,
    /// Bumped on every pause/resume so a stale auto-resume timer does nothing.
    pause_gen: u64,
    own: VecDeque<u32>,
}

/// A scheduled auto-resume: fire `expire(gen, now)` at `deadline` (Unix ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeTimer {
    pub gen: u64,
    pub deadline: i64,
}

#[derive(Debug, Default)]
pub struct CaptureState {
    inner: Mutex<Inner>,
}

impl Inner {
    /// Lazily drop an expired timed pause (the timer may fire late after sleep/hibernate).
    /// The generation is NOT bumped: the pending timer still fires and emits the change.
    fn settle(&mut self, now: i64) {
        if let Some(until) = self.paused_until {
            if until != PAUSED_INDEFINITELY && until <= now {
                self.paused_until = None;
            }
        }
    }

    fn snapshot(&self) -> CaptureStatus {
        CaptureStatus {
            paused_until: self.paused_until,
            ignore_next: self.ignore_next,
            collect_to_stack: self.collect,
        }
    }
}

impl CaptureState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self, now: i64) -> CaptureStatus {
        let mut i = self.inner.lock();
        i.settle(now);
        i.snapshot()
    }

    pub fn is_paused(&self, now: i64) -> bool {
        let mut i = self.inner.lock();
        i.settle(now);
        i.paused_until.is_some()
    }

    /// Pause for `minutes` (`None` = until resumed). `minutes` must be 1..=[MAX_PAUSE_MINUTES].
    pub fn pause(&self, minutes: Option<i64>, now: i64) -> Result<(CaptureStatus, Option<ResumeTimer>), String> {
        if let Some(m) = minutes {
            if !(1..=MAX_PAUSE_MINUTES).contains(&m) {
                return Err(format!("invalid: pause minutes must be 1..={MAX_PAUSE_MINUTES}"));
            }
        }
        let mut i = self.inner.lock();
        i.pause_gen += 1;
        let timer = match minutes {
            Some(m) => {
                let deadline = now.saturating_add(m * 60_000);
                i.paused_until = Some(deadline);
                Some(ResumeTimer { gen: i.pause_gen, deadline })
            }
            None => {
                i.paused_until = Some(PAUSED_INDEFINITELY);
                None
            }
        };
        Ok((i.snapshot(), timer))
    }

    pub fn resume(&self) -> CaptureStatus {
        let mut i = self.inner.lock();
        if i.paused_until.is_some() {
            i.paused_until = None;
            i.pause_gen += 1;
        }
        i.snapshot()
    }

    /// Recording → paused until resumed; paused → recording (hotkey `PauseToggle`).
    pub fn toggle_pause(&self, now: i64) -> CaptureStatus {
        if self.is_paused(now) {
            self.resume()
        } else {
            // `None` minutes never fails.
            self.pause(None, now).map(|(s, _)| s).unwrap_or_else(|_| self.status(now))
        }
    }

    /// Auto-resume timer callback: resumes only if the pause it was scheduled for is still the
    /// current one and its deadline has passed. `Some(status)` = state changed (emit).
    /// (A lazily settled pause — `status()` read after the deadline — still counts: the
    /// generation proves no other pause/resume happened since, so the change is emitted.)
    pub fn expire(&self, timer: ResumeTimer, now: i64) -> Option<CaptureStatus> {
        let mut i = self.inner.lock();
        if i.pause_gen != timer.gen || now < timer.deadline {
            return None;
        }
        i.paused_until = None;
        i.pause_gen += 1;
        Some(i.snapshot())
    }

    /// Whether a timer is still the current one (no pause/resume since it was scheduled).
    pub fn timer_alive(&self, timer: ResumeTimer) -> bool {
        self.inner.lock().pause_gen == timer.gen
    }

    pub fn set_ignore_next(&self) -> CaptureStatus {
        let mut i = self.inner.lock();
        i.ignore_next = true;
        i.snapshot()
    }

    /// Disarm the one-shot skip without consuming a change (the tray's checked item clicked
    /// again, FIX-05).
    pub fn clear_ignore_next(&self) -> CaptureStatus {
        let mut i = self.inner.lock();
        i.ignore_next = false;
        i.snapshot()
    }

    /// Consume the one-shot skip. `true` = this change must be skipped.
    pub fn take_ignore_next(&self) -> bool {
        std::mem::take(&mut self.inner.lock().ignore_next)
    }

    pub fn set_collect(&self, value: bool) -> CaptureStatus {
        let mut i = self.inner.lock();
        i.collect = value;
        i.snapshot()
    }

    pub fn collect(&self) -> bool {
        self.inner.lock().collect
    }

    /// Self-copy guard: remember the sequence number of a clipboard write made by Liqlip.
    pub fn mark_own(&self, seq: u32) {
        if seq == 0 {
            return; // no WINSTA_ACCESSCLIPBOARD — never matches a real change
        }
        let mut i = self.inner.lock();
        if !i.own.contains(&seq) {
            if i.own.len() == OWN_SEQS {
                i.own.pop_front();
            }
            i.own.push_back(seq);
        }
    }

    pub fn is_own(&self, seq: u32) -> bool {
        seq != 0 && self.inner.lock().own.contains(&seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_pause_and_auto_resume() {
        let s = CaptureState::new();
        assert_eq!(s.status(0), CaptureStatus::default());
        let (st, timer) = s.pause(Some(5), 1_000).unwrap();
        let timer = timer.unwrap();
        assert_eq!(st.paused_until, Some(1_000 + 300_000));
        assert_eq!(timer.deadline, 301_000);
        assert!(s.is_paused(300_999));
        assert!(s.timer_alive(timer));
        // Too early: nothing happens.
        assert_eq!(s.expire(timer, 300_000), None);
        // On time: resumes and reports it once.
        assert_eq!(s.expire(timer, 301_000).unwrap().paused_until, None);
        assert_eq!(s.expire(timer, 301_001), None);
        assert!(!s.is_paused(301_001));
    }

    #[test]
    fn stale_timer_is_ignored_and_expiry_is_lazy() {
        let s = CaptureState::new();
        let (_, t1) = s.pause(Some(1), 0).unwrap();
        let (_, t2) = s.pause(Some(30), 10).unwrap();
        let (t1, t2) = (t1.unwrap(), t2.unwrap());
        assert!(!s.timer_alive(t1));
        assert_eq!(s.expire(t1, 70_000), None, "an older pause's timer must not resume");
        assert!(s.is_paused(70_000));
        // A late timer (machine slept): status already reports "recording", and the timer
        // still emits the change exactly once.
        assert!(!s.is_paused(t2.deadline + 1));
        assert!(s.timer_alive(t2));
        assert_eq!(s.expire(t2, t2.deadline + 1).unwrap().paused_until, None);
        assert_eq!(s.expire(t2, t2.deadline + 2), None);
    }

    #[test]
    fn indefinite_pause_toggle_and_validation() {
        let s = CaptureState::new();
        let (st, timer) = s.pause(None, 5).unwrap();
        assert_eq!(st.paused_until, Some(PAUSED_INDEFINITELY));
        assert!(timer.is_none());
        assert!(s.is_paused(i64::MAX));
        assert_eq!(s.toggle_pause(10).paused_until, None);
        assert_eq!(s.toggle_pause(10).paused_until, Some(PAUSED_INDEFINITELY));
        assert_eq!(s.resume().paused_until, None);
        assert!(s.pause(Some(0), 0).unwrap_err().starts_with("invalid:"));
        assert!(s.pause(Some(-5), 0).is_err());
        assert!(s.pause(Some(MAX_PAUSE_MINUTES + 1), 0).is_err());
    }

    #[test]
    fn ignore_next_is_one_shot_and_collect_toggles() {
        let s = CaptureState::new();
        assert!(!s.take_ignore_next());
        assert!(s.set_ignore_next().ignore_next);
        assert!(s.status(0).ignore_next);
        assert!(s.take_ignore_next());
        assert!(!s.take_ignore_next());
        // Disarmed before any change (tray, FIX-05): nothing left to consume.
        s.set_ignore_next();
        assert!(!s.clear_ignore_next().ignore_next);
        assert!(!s.status(0).ignore_next);
        assert!(!s.take_ignore_next());
        assert!(s.set_collect(true).collect_to_stack);
        assert!(s.collect());
        assert!(!s.set_collect(false).collect_to_stack);
    }

    #[test]
    fn own_sequence_numbers() {
        let s = CaptureState::new();
        s.mark_own(0);
        assert!(!s.is_own(0));
        s.mark_own(41);
        assert!(s.is_own(41));
        assert!(!s.is_own(42));
        for seq in 100..100 + OWN_SEQS as u32 {
            s.mark_own(seq);
        }
        assert!(!s.is_own(41), "the ring keeps only the latest writes");
        assert!(s.is_own(100 + OWN_SEQS as u32 - 1));
    }
}
