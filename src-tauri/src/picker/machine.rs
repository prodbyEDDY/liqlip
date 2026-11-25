//! The shelf state machine (ARCHITECTURE §7) — pure: events in, effects out, unit-tested.
//! `mod.rs` executes the effects on the main thread and feeds results back as events.
//!
//! ```text
//!            Config(hover on, edge)                 HoverTrigger (cursor rests on the edge)
//!   Hidden ───────────────────────────▶ Armed ────────────────────────────▶ Open{Hover, passive}
//!     │  ◀── Config(hover off) ──────────┘ │                                   │ MakeInteractive /
//!     │ Summon                     Summon  │                                   │ Summon (upgrade)
//!     ▼                                    ▼                                   ▼
//!   Open{Summon, interactive = activation succeeded}  ◀────────────  Open{_, interactive}
//!     │ Hide(Esc) · Summon(repeat) · ClickOutside · Blur(interactive) · CursorLeft(passive hover)
//!     │ · CloseRequested(Alt+F4) · ShowSettings                       HideNow (paste: native hide now)
//!     ▼                                                                      │
//!   Closing ── 260 ms (HideTimer, same generation) ──▶ Armed if hover is on, else Hidden
//!     └── Summon inside the 260 ms cancels the hide (generation bump) and re-opens
//! ```

use crate::model::{PickerMode, Position};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Window hidden.
    Hidden,
    /// Hover armed: window shown, click-through, not focusable, panel parked off-screen by
    /// the frontend's closed state; the poll watches the docked edge.
    Armed,
    Open { mode: PickerMode, interactive: bool },
    /// `picker:close` sent, click-through; the native hide (or re-arm) follows in 260 ms.
    Closing,
}

/// The `settings.picker` fields this module applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub position: Position,
    pub center_row: bool,
    pub monitor: String,
    pub hover_open: bool,
}

impl Config {
    /// Hover-open only exists for edge positions (SPEC §3.1).
    pub fn armable(&self) -> bool {
        self.hover_open && self.position != Position::Center
    }

    fn same_layout(&self, other: &Config) -> bool {
        self.position == other.position && self.center_row == other.center_row && self.monitor == other.monitor
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// After the windows exist (`picker::init`).
    Init,
    /// Hotkey / tray / second instance. `visible` = the native window is actually shown
    /// (a window hidden behind our back — Alt+F4 — must not turn a summon into a close).
    Summon { visible: bool },
    HoverTrigger,
    CursorLeft,
    ClickOutside,
    /// Keyboard focus went to another process (the caller filtered drags/own dialogs).
    Blur,
    /// The window became foreground (a click on a summoned-but-passive shelf).
    Focused,
    /// `picker_hide(restoreFocus)` — Esc on the frontend.
    Hide { restore: bool },
    /// Paste: hide at once, no animation wait, no focus restore.
    HideNow,
    HideTimer(u64),
    MakeInteractive,
    /// Alt+F4 (`lib.rs` already hid the window).
    CloseRequested,
    ShowSettings,
    Config(Config),
    /// Display layout changed / the cursor moved to another monitor while armed.
    Redock,
    /// Result of `ShowActive` / `Activate`.
    Activated(bool),
    /// Focus left during a keep-open paste hand-off / a drag-out and did not come back: stay
    /// open, passive (a click activates it again) — no close, no focus restore.
    Deactivated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Remember the foreground window (focus restore) + `paste::focus::capture_target` —
    /// BEFORE the shelf shows.
    CaptureTarget,
    /// Dock to the configured position/monitor, emit `picker:layout`.
    Dock,
    /// Not click-through, focusable, show, `SetForegroundWindow` → `Activated(ok)`.
    ShowActive,
    /// Show without activation (armed state).
    ShowPassive,
    ClickThrough(bool),
    Focusable(bool),
    /// `SetForegroundWindow` on the shelf → `Activated(ok)`.
    Activate,
    EmitOpen { mode: PickerMode, interactive: bool },
    EmitClose,
    /// Give focus back to the remembered window if the shelf still holds it.
    RestoreFocus,
    ScheduleHide(u64),
    HideWindow,
    /// The hover trigger must see the cursor leave the edge before it fires again.
    DisarmHover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    Esc,
    Repeat,
    ClickOutside,
    Blur,
    CursorLeft,
    AltF4,
    Settings,
}

#[derive(Debug)]
pub struct Machine {
    pub phase: Phase,
    pub cfg: Config,
    /// Bumped by every close and every re-open; a hide timer only acts on its own value.
    pub gen: u64,
    /// Bumped on every entry into `Open` (the poll resets its click tracker).
    pub open_seq: u64,
    /// `Open` waiting for `Activated` to announce itself.
    awaiting_open: Option<PickerMode>,
    pub last_close: Option<CloseReason>,
}

impl Machine {
    pub fn new(cfg: Config) -> Machine {
        Machine { phase: Phase::Hidden, cfg, gen: 0, open_seq: 0, awaiting_open: None, last_close: None }
    }

    fn resting(&self) -> Phase {
        if self.cfg.armable() { Phase::Armed } else { Phase::Hidden }
    }

    fn arm_effects() -> Vec<Effect> {
        vec![Effect::Focusable(false), Effect::ClickThrough(true), Effect::Dock, Effect::ShowPassive]
    }

    fn open_summon(&mut self, dock: bool) -> Vec<Effect> {
        self.gen += 1;
        self.open_seq += 1;
        self.phase = Phase::Open { mode: PickerMode::Summon, interactive: false };
        self.awaiting_open = Some(PickerMode::Summon);
        let mut fx = vec![Effect::CaptureTarget];
        if dock {
            fx.push(Effect::Dock);
        }
        fx.push(Effect::ShowActive);
        fx
    }

    fn close(&mut self, reason: CloseReason, restore: bool) -> Vec<Effect> {
        self.gen += 1;
        self.phase = Phase::Closing;
        self.awaiting_open = None;
        self.last_close = Some(reason);
        let mut fx = Vec::with_capacity(5);
        if restore {
            fx.push(Effect::RestoreFocus);
        }
        fx.extend([Effect::EmitClose, Effect::ClickThrough(true), Effect::DisarmHover, Effect::ScheduleHide(self.gen)]);
        fx
    }

    pub fn on(&mut self, ev: Event) -> Vec<Effect> {
        use Phase::*;
        match (self.phase, ev) {
            (_, Event::Init) => {
                if self.phase == Hidden && self.cfg.armable() {
                    self.phase = Armed;
                    Self::arm_effects()
                } else {
                    vec![Effect::Dock]
                }
            }

            (Open { mode: PickerMode::Summon, .. }, Event::Summon { visible: true }) => {
                self.close(CloseReason::Repeat, true)
            }
            // A hover-opened shelf + the hotkey: the user wants the keyboard — upgrade in place.
            (Open { mode: PickerMode::Hover, .. }, Event::Summon { visible: true }) => self.open_summon(false),
            (Open { .. } | Hidden | Armed | Closing, Event::Summon { .. }) => self.open_summon(true),

            (Armed, Event::HoverTrigger) => {
                self.open_seq += 1;
                self.phase = Open { mode: PickerMode::Hover, interactive: false };
                vec![
                    Effect::CaptureTarget,
                    Effect::ClickThrough(false),
                    Effect::EmitOpen { mode: PickerMode::Hover, interactive: false },
                ]
            }
            (Open { mode: PickerMode::Hover, interactive: false }, Event::CursorLeft) => {
                self.close(CloseReason::CursorLeft, false)
            }
            (Open { .. }, Event::ClickOutside) => self.close(CloseReason::ClickOutside, true),
            (Open { interactive: true, .. }, Event::Blur) => self.close(CloseReason::Blur, false),
            (Open { mode, interactive: true }, Event::Deactivated) => {
                self.phase = Open { mode, interactive: false };
                vec![]
            }
            (Open { mode, interactive: false }, Event::Focused) if self.awaiting_open.is_none() => {
                self.phase = Open { mode, interactive: true };
                vec![]
            }
            (Open { .. }, Event::Hide { restore }) => self.close(CloseReason::Esc, restore),
            (Open { .. }, Event::CloseRequested) => self.close(CloseReason::AltF4, true),
            (Open { .. }, Event::ShowSettings) => self.close(CloseReason::Settings, false),

            (Open { .. } | Closing, Event::HideNow) => {
                // Native hide right now; the timer only re-arms (or re-hides — idempotent).
                let mut fx = self.close(CloseReason::Esc, false);
                fx.insert(2, Effect::HideWindow);
                self.last_close = None;
                fx
            }
            (Closing, Event::CloseRequested) => vec![],

            (Closing, Event::HideTimer(g)) if g == self.gen => {
                self.phase = self.resting();
                if self.phase == Armed {
                    Self::arm_effects()
                } else {
                    vec![Effect::HideWindow]
                }
            }

            (Open { mode, interactive: false }, Event::MakeInteractive) => {
                self.awaiting_open = None;
                self.phase = Open { mode, interactive: false };
                vec![Effect::Focusable(true), Effect::Activate]
            }

            (Open { mode, .. }, Event::Activated(ok)) => {
                self.phase = Open { mode, interactive: ok };
                match self.awaiting_open.take() {
                    Some(m) => vec![Effect::EmitOpen { mode: m, interactive: ok }],
                    None => vec![],
                }
            }

            (phase, Event::Config(cfg)) => {
                let relayout = !self.cfg.same_layout(&cfg);
                self.cfg = cfg;
                match phase {
                    Hidden if self.cfg.armable() => {
                        self.phase = Armed;
                        Self::arm_effects()
                    }
                    Hidden => {
                        if relayout { vec![Effect::Dock] } else { vec![] }
                    }
                    Armed if !self.cfg.armable() => {
                        self.phase = Hidden;
                        vec![Effect::HideWindow]
                    }
                    Armed | Open { .. } if relayout => vec![Effect::Dock],
                    _ => vec![],
                }
            }

            (Armed | Open { .. }, Event::Redock) => vec![Effect::Dock],

            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Effect::*;

    fn cfg(position: Position, hover: bool) -> Config {
        Config { position, center_row: false, monitor: "cursor".into(), hover_open: hover }
    }

    fn open_summoned(m: &mut Machine) {
        let fx = m.on(Event::Summon { visible: false });
        assert_eq!(fx, vec![CaptureTarget, Dock, ShowActive]);
        assert_eq!(m.on(Event::Activated(true)), vec![EmitOpen { mode: PickerMode::Summon, interactive: true }]);
    }

    #[test]
    fn summon_captures_target_before_showing_and_announces_after_activation() {
        let mut m = Machine::new(cfg(Position::Center, false));
        open_summoned(&mut m);
        assert_eq!(m.phase, Phase::Open { mode: PickerMode::Summon, interactive: true });
        assert_eq!(m.open_seq, 1);
    }

    #[test]
    fn refused_activation_opens_passive_and_a_click_makes_it_interactive() {
        let mut m = Machine::new(cfg(Position::Center, false));
        m.on(Event::Summon { visible: false });
        assert_eq!(m.on(Event::Activated(false)), vec![EmitOpen { mode: PickerMode::Summon, interactive: false }]);
        assert_eq!(m.on(Event::Blur), vec![], "never had focus — no blur close");
        assert_eq!(m.on(Event::Focused), vec![]);
        assert_eq!(m.phase, Phase::Open { mode: PickerMode::Summon, interactive: true });
    }

    #[test]
    fn repeat_hotkey_closes_with_focus_restore_then_hides() {
        let mut m = Machine::new(cfg(Position::Center, false));
        open_summoned(&mut m);
        let fx = m.on(Event::Summon { visible: true });
        let g = m.gen;
        assert_eq!(fx, vec![RestoreFocus, EmitClose, ClickThrough(true), DisarmHover, ScheduleHide(g)]);
        assert_eq!(m.phase, Phase::Closing);
        assert_eq!(m.on(Event::HideTimer(g - 1)), vec![], "stale timer");
        assert_eq!(m.on(Event::HideTimer(g)), vec![HideWindow]);
        assert_eq!(m.phase, Phase::Hidden);
    }

    #[test]
    fn summon_during_closing_cancels_the_hide() {
        let mut m = Machine::new(cfg(Position::Center, false));
        open_summoned(&mut m);
        m.on(Event::Hide { restore: true });
        let closing_gen = m.gen;
        let fx = m.on(Event::Summon { visible: true });
        assert_eq!(fx, vec![CaptureTarget, Dock, ShowActive]);
        assert_eq!(m.on(Event::HideTimer(closing_gen)), vec![], "superseded");
        m.on(Event::Activated(true));
        assert_eq!(m.phase, Phase::Open { mode: PickerMode::Summon, interactive: true });
    }

    #[test]
    fn hidden_behind_our_back_summon_reopens() {
        let mut m = Machine::new(cfg(Position::Center, false));
        open_summoned(&mut m);
        // Alt+F4 hid the window without CloseRequested reaching us.
        assert_eq!(m.on(Event::Summon { visible: false }), vec![CaptureTarget, Dock, ShowActive]);
    }

    #[test]
    fn alt_f4_closes_and_next_summon_works() {
        let mut m = Machine::new(cfg(Position::Center, false));
        open_summoned(&mut m);
        let fx = m.on(Event::CloseRequested);
        assert!(fx.contains(&RestoreFocus) && fx.contains(&EmitClose));
        let g = m.gen;
        m.on(Event::HideTimer(g));
        assert_eq!(m.phase, Phase::Hidden);
        assert_eq!(m.on(Event::Summon { visible: false }), vec![CaptureTarget, Dock, ShowActive]);
    }

    #[test]
    fn blur_closes_interactive_without_restore_but_a_handoff_only_deactivates() {
        let mut m = Machine::new(cfg(Position::Center, false));
        open_summoned(&mut m);
        // keepOpen paste / drag-out took focus away and it did not come back.
        assert_eq!(m.on(Event::Deactivated), vec![]);
        assert_eq!(m.phase, Phase::Open { mode: PickerMode::Summon, interactive: false });
        assert_eq!(m.on(Event::Blur), vec![], "passive now");
        assert_eq!(m.on(Event::Focused), vec![], "a click re-activates");
        let fx = m.on(Event::Blur);
        assert!(!fx.contains(&RestoreFocus));
        assert_eq!(m.last_close, Some(CloseReason::Blur));
    }

    #[test]
    fn click_outside_closes_with_conditional_restore() {
        let mut m = Machine::new(cfg(Position::Center, false));
        open_summoned(&mut m);
        let fx = m.on(Event::ClickOutside);
        assert_eq!(fx[0], RestoreFocus);
        assert_eq!(m.last_close, Some(CloseReason::ClickOutside));
    }

    #[test]
    fn hide_now_hides_immediately_and_never_restores() {
        let mut m = Machine::new(cfg(Position::Center, false));
        open_summoned(&mut m);
        let fx = m.on(Event::HideNow);
        let g = m.gen;
        assert_eq!(fx, vec![EmitClose, ClickThrough(true), HideWindow, DisarmHover, ScheduleHide(g)]);
        assert_eq!(m.on(Event::HideTimer(g)), vec![HideWindow]);
        // Already hidden: a paste from the tray/hotkey path is a no-op.
        assert_eq!(m.on(Event::HideNow), vec![]);
    }

    #[test]
    fn hover_arms_triggers_passive_and_leaves() {
        let mut m = Machine::new(cfg(Position::Left, true));
        assert_eq!(m.on(Event::Init), vec![Focusable(false), ClickThrough(true), Dock, ShowPassive]);
        assert_eq!(m.phase, Phase::Armed);
        let fx = m.on(Event::HoverTrigger);
        assert_eq!(fx, vec![CaptureTarget, ClickThrough(false), EmitOpen { mode: PickerMode::Hover, interactive: false }]);
        assert_eq!(m.on(Event::Blur), vec![], "passive shelf has no focus");
        let fx = m.on(Event::CursorLeft);
        assert!(!fx.contains(&RestoreFocus));
        let g = m.gen;
        assert_eq!(m.on(Event::HideTimer(g)), vec![Focusable(false), ClickThrough(true), Dock, ShowPassive]);
        assert_eq!(m.phase, Phase::Armed);
    }

    #[test]
    fn hover_click_into_search_makes_it_interactive_and_cursor_leave_no_longer_closes() {
        let mut m = Machine::new(cfg(Position::Right, true));
        m.on(Event::Init);
        m.on(Event::HoverTrigger);
        assert_eq!(m.on(Event::MakeInteractive), vec![Focusable(true), Activate]);
        assert_eq!(m.on(Event::Activated(true)), vec![], "no second picker:open");
        assert_eq!(m.phase, Phase::Open { mode: PickerMode::Hover, interactive: true });
        assert_eq!(m.on(Event::CursorLeft), vec![]);
        assert!(m.on(Event::Hide { restore: true }).contains(&RestoreFocus));
    }

    #[test]
    fn hotkey_on_hover_open_upgrades_to_summon() {
        let mut m = Machine::new(cfg(Position::Bottom, true));
        m.on(Event::Init);
        m.on(Event::HoverTrigger);
        assert_eq!(m.on(Event::Summon { visible: true }), vec![CaptureTarget, ShowActive]);
        assert_eq!(m.on(Event::Activated(true)), vec![EmitOpen { mode: PickerMode::Summon, interactive: true }]);
    }

    #[test]
    fn center_never_arms() {
        let mut m = Machine::new(cfg(Position::Center, true));
        assert_eq!(m.on(Event::Init), vec![Dock]);
        assert_eq!(m.phase, Phase::Hidden);
        assert_eq!(m.on(Event::HoverTrigger), vec![]);
    }

    #[test]
    fn settings_change_applies_live() {
        let mut m = Machine::new(cfg(Position::Center, false));
        m.on(Event::Init);
        // Hover on for an edge → armed now.
        assert_eq!(m.on(Event::Config(cfg(Position::Left, true))), vec![Focusable(false), ClickThrough(true), Dock, ShowPassive]);
        // Edge changes while armed → re-dock.
        assert_eq!(m.on(Event::Config(cfg(Position::Top, true))), vec![Dock]);
        // Hover off → hidden.
        assert_eq!(m.on(Event::Config(cfg(Position::Top, false))), vec![HideWindow]);
        // Hidden: a layout change still re-docks (fresh picker:layout); identical config is a no-op.
        assert_eq!(m.on(Event::Config(cfg(Position::Right, false))), vec![Dock]);
        assert_eq!(m.on(Event::Config(cfg(Position::Right, false))), vec![]);
        // Open: layout change re-docks; the resting state follows the new config on close.
        open_summoned(&mut m);
        assert_eq!(m.on(Event::Config(cfg(Position::Bottom, true))), vec![Dock]);
        m.on(Event::Hide { restore: true });
        let g = m.gen;
        m.on(Event::HideTimer(g));
        assert_eq!(m.phase, Phase::Armed);
    }

    #[test]
    fn show_settings_closes_an_open_shelf_without_restore() {
        let mut m = Machine::new(cfg(Position::Center, false));
        assert_eq!(m.on(Event::ShowSettings), vec![]);
        open_summoned(&mut m);
        let fx = m.on(Event::ShowSettings);
        assert!(!fx.contains(&RestoreFocus) && fx.contains(&EmitClose));
    }

    #[test]
    fn redock_only_when_visible() {
        let mut m = Machine::new(cfg(Position::Left, false));
        assert_eq!(m.on(Event::Redock), vec![]);
        open_summoned(&mut m);
        assert_eq!(m.on(Event::Redock), vec![Dock]);
    }
}
