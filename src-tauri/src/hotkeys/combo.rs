//! Chord grammar + binding plan — pure logic, unit-tested (no Win32 here).
//!
//! Notation is the one Liquid Tools uses and `src/lib/hotkey.ts` emits: tokens joined by
//! `" + "`, modifiers in the order Ctrl · Alt · Shift · Super, then ONE main key
//! (`"Alt + V"`, `"Ctrl + Shift + F8"`, `"Ctrl + LWin"`). Keys are Windows virtual-key codes
//! of PHYSICAL keys, so `Alt + V` fires in the Russian layout too (CLAUDE.md hard rule 9).
//!
//! Ported from Liquid Tools `src-tauri/src/hotkeys.rs` (`parse_combo`, `token_vk`,
//! `vk_token`, `combo_string`, `binding_backend`) without what Liqlip does not bind:
//! mouse buttons and lone (bare) modifiers — a bare `LCtrl` would fire on every Ctrl+C.
//! Virtual-key codes: https://learn.microsoft.com/windows/win32/inputdev/virtual-key-codes

use crate::model::{HotkeySettings, SettingsErrorCode};

/// Required held modifiers (generic — either side satisfies, like `MOD_ALT`).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Hash)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Mods {
    pub fn any(self) -> bool {
        self.ctrl || self.alt || self.shift || self.win
    }
}

/// A parsed binding: main virtual key + required modifiers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Combo {
    pub vk: u32,
    pub mods: Mods,
}

/// Which backend can deliver a chord.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// `RegisterHotKey` → `WM_HOTKEY` (the default; no hook).
    Os,
    /// `WH_KEYBOARD_LL` — only for keys `RegisterHotKey` cannot take.
    Hook,
}

const VK_LWIN: u32 = 0x5B;
const VK_RWIN: u32 = 0x5C;
const VK_APPS: u32 = 0x5D;
const VK_F12: u32 = 0x7B;
pub const VK_ESCAPE: u32 = 0x1B;

/// Keys `RegisterHotKey` cannot (or must not) take: the Windows keys and sided modifiers as
/// the MAIN key (`"Ctrl + LWin"`), the Apps/Menu key (refused in Liquid Tools' live tests),
/// and F12 — "reserved for use by the debugger at all times, so it should not be registered
/// as a hot key" (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-registerhotkey).
pub fn backend(combo: &Combo) -> Backend {
    match combo.vk {
        VK_LWIN | VK_RWIN | VK_APPS | VK_F12 | 0xA0..=0xA5 => Backend::Hook,
        _ => Backend::Os,
    }
}

/// Sided modifier token → its VK (valid only as the main key of a chord with modifiers).
fn sided_modifier_vk(token: &str) -> Option<u32> {
    Some(match token {
        "lctrl" | "leftctrl" => 0xA2,
        "rctrl" | "rightctrl" => 0xA3,
        "lshift" | "leftshift" => 0xA0,
        "rshift" | "rightshift" => 0xA1,
        "lalt" | "leftalt" => 0xA4,
        "ralt" | "rightalt" | "altgr" => 0xA5,
        "lwin" | "leftwin" | "lsuper" => VK_LWIN,
        "rwin" | "rightwin" | "rsuper" => VK_RWIN,
        _ => return None,
    })
}

/// Modifier token in a modifier position. Sided tokens collapse to the generic modifier.
fn apply_modifier(token: &str, mods: &mut Mods) -> bool {
    match token {
        "ctrl" | "control" | "ctl" | "lctrl" | "rctrl" | "leftctrl" | "rightctrl" => mods.ctrl = true,
        "alt" | "option" | "opt" | "lalt" | "ralt" | "leftalt" | "rightalt" | "altgr" => mods.alt = true,
        "shift" | "lshift" | "rshift" | "leftshift" | "rightshift" => mods.shift = true,
        "win" | "super" | "meta" | "cmd" | "command" | "lwin" | "rwin" | "leftwin" | "rightwin" => {
            mods.win = true
        }
        _ => return false,
    }
    true
}

/// Main-key token → VK. Every token `src/lib/hotkey.ts::keyToken` emits maps here.
fn token_vk(t: &str) -> Option<u32> {
    if t.len() == 1 {
        let c = t.as_bytes()[0];
        if c.is_ascii_lowercase() {
            return Some(u32::from(c - b'a') + 0x41);
        }
        if c.is_ascii_digit() {
            return Some(u32::from(c - b'0') + 0x30);
        }
        // US-position OEM keys, shown as their symbol (same as hotkey.ts).
        return Some(match c {
            b'`' => 0xC0,
            b'-' => 0xBD,
            b'=' => 0xBB,
            b'[' => 0xDB,
            b']' => 0xDD,
            b'\\' => 0xDC,
            b';' => 0xBA,
            b'\'' => 0xDE,
            b',' => 0xBC,
            b'.' => 0xBE,
            b'/' => 0xBF,
            _ => return None,
        });
    }
    if let Some(n) = t.strip_prefix('f').and_then(|d| d.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x70 + n - 1);
        }
    }
    if let Some(n) = t.strip_prefix("num").and_then(|d| d.parse::<u32>().ok()) {
        if n <= 9 {
            return Some(0x60 + n);
        }
    }
    Some(match t {
        "space" => 0x20,
        // Numpad Enter shares VK_RETURN (only the extended flag differs) — same binding.
        "enter" | "return" | "numenter" => 0x0D,
        "tab" => 0x09,
        "escape" | "esc" => VK_ESCAPE,
        "backspace" | "bksp" | "back" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" | "ins" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "up" | "arrowup" => 0x26,
        "down" | "arrowdown" => 0x28,
        "left" | "arrowleft" => 0x25,
        "right" | "arrowright" => 0x27,
        "numadd" => 0x6B,
        "numsub" => 0x6D,
        "nummul" => 0x6A,
        "numdiv" => 0x6F,
        "numdec" => 0x6E,
        "capslock" => 0x14,
        "numlock" => 0x90,
        "scrolllock" => 0x91,
        "prtsc" | "printscreen" => 0x2C,
        "pause" => 0x13,
        "menu" | "apps" | "contextmenu" => VK_APPS,
        "volumeup" => 0xAF,
        "volumedown" => 0xAE,
        "volumemute" | "mute" => 0xAD,
        "medianext" => 0xB0,
        "mediaprev" | "mediaprevious" => 0xB1,
        "mediastop" => 0xB2,
        "mediaplay" | "mediaplaypause" => 0xB3,
        _ => return None,
    })
}

/// Parse `"Ctrl + Shift + V"`. `None` = not bindable (empty, unknown token, a non-modifier
/// in a modifier position, a lone modifier, a mouse button).
pub fn parse(combo: &str) -> Option<Combo> {
    let parts: Vec<String> = combo
        .split('+')
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    // "Ctrl +" / "+ V" / "" — an empty token anywhere is malformed.
    if parts.iter().any(String::is_empty) {
        return None;
    }
    let (key, mod_parts) = parts.split_last()?;
    let mut mods = Mods::default();
    for p in mod_parts {
        if !apply_modifier(p, &mut mods) {
            return None;
        }
    }
    let vk = match token_vk(key) {
        Some(vk) => vk,
        // A sided modifier as the main key needs another modifier ("Ctrl + LWin"): alone it
        // would fire on every use of that modifier.
        None => match sided_modifier_vk(key) {
            Some(vk) if mods.any() => vk,
            _ => return None,
        },
    };
    Some(Combo { vk, mods })
}

/// Sided modifier VK → its token (reverse of `sided_modifier_vk`).
pub fn sided_modifier_token(vk: u32) -> Option<&'static str> {
    Some(match vk {
        0xA2 => "LCtrl",
        0xA3 => "RCtrl",
        0xA0 => "LShift",
        0xA1 => "RShift",
        0xA4 => "LAlt",
        0xA5 => "RAlt",
        VK_LWIN => "LWin",
        VK_RWIN => "RWin",
        _ => return None,
    })
}

/// Generic modifiers minus the family of `vk` (a sided modifier pressed as the main key is
/// also reported as held by the hook's modifier tracking).
pub fn mods_without_vk(mut mods: Mods, vk: u32) -> Mods {
    match vk {
        0x11 | 0xA2 | 0xA3 => mods.ctrl = false,
        0x12 | 0xA4 | 0xA5 => mods.alt = false,
        0x10 | 0xA0 | 0xA1 => mods.shift = false,
        VK_LWIN | VK_RWIN => mods.win = false,
        _ => {}
    }
    mods
}

/// The hook swallowed a key while these modifiers are held: will their release be "bare"?
/// A held Alt whose press and release reach the app with nothing between them puts it in
/// menu mode; a bare Win release opens Start. Then the dispatcher taps the unassigned mask
/// key (AutoHotkey's "menu mask key", the same trick the paste uses — `paste::input`).
/// Ctrl and Shift releases do nothing on their own.
pub fn masks_release(held: Mods) -> bool {
    held.alt || held.win
}

/// Canonical display token of a main VK (reverse of `token_vk`). `None` = not bindable.
pub fn vk_token(vk: u32) -> Option<String> {
    Some(match vk {
        0x41..=0x5A => char::from(b'A' + (vk - 0x41) as u8).to_string(),
        0x30..=0x39 => char::from(b'0' + (vk - 0x30) as u8).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x20 => "Space".into(),
        0x0D => "Enter".into(),
        0x09 => "Tab".into(),
        VK_ESCAPE => "Escape".into(),
        0x08 => "Backspace".into(),
        0x2E => "Delete".into(),
        0x2D => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x26 => "Up".into(),
        0x28 => "Down".into(),
        0x25 => "Left".into(),
        0x27 => "Right".into(),
        0x6B => "NumAdd".into(),
        0x6D => "NumSub".into(),
        0x6A => "NumMul".into(),
        0x6F => "NumDiv".into(),
        0x6E => "NumDec".into(),
        0x14 => "CapsLock".into(),
        0x90 => "NumLock".into(),
        0x91 => "ScrollLock".into(),
        0x2C => "PrtSc".into(),
        0x13 => "Pause".into(),
        VK_APPS => "Menu".into(),
        0xAF => "VolumeUp".into(),
        0xAE => "VolumeDown".into(),
        0xAD => "Mute".into(),
        0xB0 => "MediaNext".into(),
        0xB1 => "MediaPrev".into(),
        0xB2 => "MediaStop".into(),
        0xB3 => "MediaPlay".into(),
        0xC0 => "`".into(),
        0xBD => "-".into(),
        0xBB => "=".into(),
        0xDB => "[".into(),
        0xDD => "]".into(),
        0xDC => "\\".into(),
        0xBA => ";".into(),
        0xDE => "'".into(),
        0xBC => ",".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        _ => return None,
    })
}

/// `"Ctrl + Alt + Shift + Super + <token>"` — the order the webview recorder produces.
pub fn combo_string(mods: Mods, token: &str) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(5);
    if mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    if mods.win {
        parts.push("Super");
    }
    parts.push(token);
    parts.join(" + ")
}

// ── Settings → desired bindings ─────────────────────────────────────────────

/// The four actions, in the fixed order used for slots, fields and error reporting.
pub const FIELDS: [&str; 4] = [
    "hotkeys.summon",
    "hotkeys.stackNext",
    "hotkeys.pauseToggle",
    "hotkeys.ignoreNext",
];

pub type Slots<T> = [T; 4];

pub fn strings(h: &HotkeySettings) -> Slots<&str> {
    [
        h.summon.as_str(),
        h.stack_next.as_str(),
        h.pause_toggle.as_str(),
        h.ignore_next.as_str(),
    ]
}

/// Parse a settings string: `""` = unbound (`Ok(None)`), unparsable = `Err`.
fn parse_field(s: &str) -> Result<Option<Combo>, ()> {
    let s = s.trim();
    if s.is_empty() {
        Ok(None)
    } else {
        parse(s).map(Some).ok_or(())
    }
}

/// What the settings ask for, before talking to the OS. A field is `invalid` when its
/// chord does not parse or duplicates another action's chord; an invalid field keeps its
/// PREVIOUS chord (`settings_set` reverts the string). Fields that did not change claim
/// their chord first, so a new chord can never steal one that is already in use.
pub fn desired(prev: &HotkeySettings, next: &HotkeySettings) -> (Slots<Option<Combo>>, Slots<Option<SettingsErrorCode>>) {
    let prev_s = strings(prev);
    let next_s = strings(next);
    // A previous string that no longer parses (hand-edited file) counts as unbound.
    let prev_c: Slots<Option<Combo>> = prev_s.map(|s| parse_field(s).unwrap_or(None));
    let mut want = prev_c;
    let mut errs: Slots<Option<SettingsErrorCode>> = [None; 4];
    let mut changed = [false; 4];
    for i in 0..4 {
        match parse_field(next_s[i]) {
            Ok(c) => {
                changed[i] = c != prev_c[i];
                want[i] = c;
            }
            Err(()) => errs[i] = Some(SettingsErrorCode::Invalid),
        }
    }
    // Duplicates. Claim order: unchanged / reverted fields first, then changed ones, each
    // group by slot. A changed loser reverts to its previous chord (which can clash again —
    // iterate); an unchanged loser (a hand-edited file with one chord twice) is unbound.
    let rank = |i: usize, errs: &Slots<Option<SettingsErrorCode>>| {
        if changed[i] && errs[i].is_none() { 10 + i } else { i }
    };
    for _ in 0..5 {
        let mut clash = false;
        for i in 0..4 {
            let Some(c) = want[i] else { continue };
            let beaten = (0..4).any(|j| j != i && want[j] == Some(c) && rank(j, &errs) < rank(i, &errs));
            if !beaten {
                continue;
            }
            clash = true;
            if changed[i] && errs[i].is_none() {
                want[i] = prev_c[i];
            } else {
                want[i] = None;
            }
            errs[i] = Some(SettingsErrorCode::Invalid);
        }
        if !clash {
            break;
        }
    }
    (want, errs)
}

/// Bring the live bindings to `want` through `register`/`unregister` (the OS registrar or
/// the hook table). A chord the OS refuses keeps the action on its `prev` chord and is
/// reported (`taken` / `invalid` from `register`); if reverting makes it clash with another
/// action's new chord, that action is reverted too (reported `taken`), so the final set
/// never has duplicates and always matches what `settings_set` will save.
/// Returns the final live bindings and per-slot errors.
pub fn reconcile(
    live: Slots<Option<Combo>>,
    prev: Slots<Option<Combo>>,
    mut want: Slots<Option<Combo>>,
    register: &mut dyn FnMut(usize, Combo) -> Result<(), SettingsErrorCode>,
    unregister: &mut dyn FnMut(usize, Combo),
) -> (Slots<Option<Combo>>, Slots<Option<SettingsErrorCode>>) {
    let mut live = live;
    let mut errs: Slots<Option<SettingsErrorCode>> = [None; 4];
    for _ in 0..5 {
        // A reverted slot reclaims its previous chord; whoever took it in the meantime
        // goes back to its own previous chord.
        for i in 0..4 {
            if errs[i].is_none() {
                continue;
            }
            for j in 0..4 {
                if j != i && errs[j].is_none() && want[j].is_some() && want[j] == want[i] {
                    errs[j] = Some(SettingsErrorCode::Taken);
                    want[j] = prev[j];
                }
            }
        }
        // Free everything that changes first (a swap needs both chords released).
        for i in 0..4 {
            if let Some(c) = live[i] {
                if want[i] != Some(c) {
                    unregister(i, c);
                    live[i] = None;
                }
            }
        }
        let mut reverted = false;
        for i in 0..4 {
            let Some(c) = want[i] else { continue };
            if live[i] == Some(c) {
                continue;
            }
            match register(i, c) {
                Ok(()) => live[i] = Some(c),
                Err(code) => {
                    errs[i].get_or_insert(code);
                    if want[i] != prev[i] {
                        want[i] = prev[i];
                        reverted = true;
                    }
                    // else: the previous chord itself is unavailable — the slot stays unbound
                    // (pending) and the error stands.
                }
            }
        }
        if !reverted {
            break;
        }
    }
    (live, errs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn c(s: &str) -> Combo {
        parse(s).unwrap_or_else(|| panic!("parse {s}"))
    }

    fn hk(a: &str, b: &str, cc: &str, d: &str) -> HotkeySettings {
        HotkeySettings {
            summon: a.into(),
            stack_next: b.into(),
            pause_toggle: cc.into(),
            ignore_next: d.into(),
            // W5: the screenshot slots exist in the settings but are not engine slots yet (W6-A).
            shot_region: String::new(),
            shot_screen: String::new(),
            shot_window: String::new(),
            shot_repeat: String::new(),
            shot_text: String::new(),
        }
    }

    #[test]
    fn parses_plain_chords() {
        let v = c("Alt + V");
        assert_eq!(v.vk, 0x56);
        assert_eq!(v.mods, Mods { alt: true, ..Default::default() });
        let s = c("ctrl+shift+v");
        assert_eq!(s.mods, Mods { ctrl: true, shift: true, ..Default::default() });
        assert_eq!(c("Super + Num5").vk, 0x65);
        assert_eq!(c("F8").vk, 0x77);
        assert_eq!(c("Ctrl + `").vk, 0xC0);
        assert_eq!(c("Ctrl + NumEnter"), c("Ctrl + Enter"));
        // Sided modifiers in modifier position collapse to the generic modifier.
        assert_eq!(c("RAlt + S"), c("Alt + S"));
    }

    #[test]
    fn rejects_unbindable() {
        for s in ["", "   ", "Ctrl +", "+ V", "Ctrl + + V", "NotAKey", "A + B", "Ctrl + Alt", "LCtrl", "RAlt", "MMB", "Ctrl + M4"] {
            assert!(parse(s).is_none(), "must reject {s:?}");
        }
    }

    #[test]
    fn sided_modifier_main_key_needs_a_modifier_and_the_hook() {
        let w = c("Ctrl + LWin");
        assert_eq!(w.vk, 0x5B);
        assert_eq!(w.mods, Mods { ctrl: true, ..Default::default() });
        assert_eq!(backend(&w), Backend::Hook);
    }

    #[test]
    fn backend_is_os_except_hook_only_keys() {
        for s in ["Alt + V", "Alt + Shift + V", "PrtSc", "Ctrl + F11", "Pause"] {
            assert_eq!(backend(&c(s)), Backend::Os, "{s}");
        }
        for s in ["F12", "Ctrl + F12", "Menu", "Ctrl + LWin", "Shift + RAlt"] {
            assert_eq!(backend(&c(s)), Backend::Hook, "{s}");
        }
    }

    /// Every token `src/lib/hotkey.ts::keyToken` can emit must bind (alone and with Ctrl).
    #[test]
    fn every_frontend_token_binds() {
        let mut tokens: Vec<String> = (b'A'..=b'Z').map(|b| (b as char).to_string()).collect();
        for d in 0..=9 {
            tokens.push(d.to_string());
            tokens.push(format!("Num{d}"));
        }
        for f in 1..=24 {
            tokens.push(format!("F{f}"));
        }
        for t in [
            "Up", "Down", "Left", "Right", "Home", "End", "PageUp", "PageDown", "Insert", "Delete",
            "Backspace", "Space", "Enter", "Tab", "Escape", "CapsLock", "NumLock", "ScrollLock",
            "PrtSc", "Pause", "`", "-", "=", "[", "]", "\\", ";", "'", ",", ".", "/", "NumEnter",
            "NumAdd", "NumSub", "NumMul", "NumDiv", "NumDec",
        ] {
            tokens.push(t.into());
        }
        for t in &tokens {
            assert!(parse(t).is_some(), "token {t}");
            assert!(parse(&format!("Ctrl + {t}")).is_some(), "Ctrl + {t}");
        }
    }

    /// What the native recorder reports (`vk_token` + `combo_string`) parses back to the
    /// same key — `hotkeys:captured` is exactly what `apply_settings` reads.
    #[test]
    fn recorded_chords_round_trip() {
        for vk in 1u32..=0xFF {
            let Some(token) = vk_token(vk) else { continue };
            let mods = Mods { ctrl: true, alt: true, shift: true, win: true };
            let s = combo_string(mods, &token);
            let parsed = parse(&s).unwrap_or_else(|| panic!("{s}"));
            assert_eq!(parsed.vk, vk, "{s}");
            assert_eq!(parsed.mods, mods);
        }
        assert_eq!(combo_string(Mods { alt: true, ..Default::default() }, "V"), "Alt + V");
        let lwin = combo_string(Mods { ctrl: true, ..Default::default() }, sided_modifier_token(0x5B).unwrap());
        assert_eq!(parse(&lwin), Some(c("Ctrl + LWin")));
    }

    /// W3-A: a swallowed `Alt + F12` / `Super + Menu` would leave a bare Alt / Win release.
    #[test]
    fn swallowed_alt_or_win_chords_need_the_mask() {
        assert!(masks_release(c("Alt + F12").mods));
        assert!(masks_release(c("Ctrl + Alt + Menu").mods));
        assert!(masks_release(c("Super + F12").mods));
        assert!(!masks_release(c("Ctrl + F12").mods));
        assert!(!masks_release(c("Ctrl + Shift + Menu").mods));
        assert!(!masks_release(c("F12").mods));
        // The main key's own family is not "held": swallowing LWin itself leaves no bare Win.
        assert!(!masks_release(c("Ctrl + LWin").mods));
        assert!(masks_release(c("Alt + RCtrl").mods), "Alt stays held while RCtrl is swallowed");
    }

    #[test]
    fn mods_without_vk_drops_the_main_modifier_family() {
        let m = Mods { ctrl: true, win: true, ..Default::default() };
        assert_eq!(mods_without_vk(m, 0x5B), Mods { ctrl: true, ..Default::default() });
        assert_eq!(mods_without_vk(m, 0x56), m);
    }

    #[test]
    fn desired_accepts_valid_changes_and_empty() {
        let prev = hk("Alt + V", "Alt + Shift + V", "", "");
        let next = hk("Ctrl + Space", "", "Ctrl + Alt + P", "");
        let (want, errs) = desired(&prev, &next);
        assert_eq!(errs, [None; 4]);
        assert_eq!(want, [Some(c("Ctrl + Space")), None, Some(c("Ctrl + Alt + P")), None]);
    }

    #[test]
    fn desired_reverts_unparsable_and_duplicate_fields() {
        let prev = hk("Alt + V", "Alt + Shift + V", "", "");
        // stackNext copies summon (unchanged → summon keeps it); pauseToggle is garbage.
        let next = hk("Alt + V", "alt+v", "Ctrl + ??", "");
        let (want, errs) = desired(&prev, &next);
        assert_eq!(errs[1], Some(SettingsErrorCode::Invalid));
        assert_eq!(errs[2], Some(SettingsErrorCode::Invalid));
        assert_eq!(want[0], Some(c("Alt + V")));
        assert_eq!(want[1], Some(c("Alt + Shift + V")), "keeps its previous chord");
        assert_eq!(want[2], None);
    }

    #[test]
    fn desired_two_new_equal_chords_keep_the_first() {
        let prev = hk("", "", "", "");
        let next = hk("Alt + V", "Alt + V", "", "");
        let (want, errs) = desired(&prev, &next);
        assert_eq!(errs, [None, Some(SettingsErrorCode::Invalid), None, None]);
        assert_eq!(want, [Some(c("Alt + V")), None, None, None]);
    }

    #[test]
    fn desired_revert_that_clashes_cascades() {
        // summon takes stackNext's chord; stackNext's new value is garbage → it reverts to
        // that same chord, so summon must give it back.
        let prev = hk("Alt + V", "Alt + Shift + V", "", "");
        let next = hk("Alt + Shift + V", "%%", "", "");
        let (want, errs) = desired(&prev, &next);
        assert_eq!(errs[0], Some(SettingsErrorCode::Invalid));
        assert_eq!(errs[1], Some(SettingsErrorCode::Invalid));
        assert_eq!(want, [Some(c("Alt + V")), Some(c("Alt + Shift + V")), None, None]);
    }

    #[test]
    fn desired_swap_is_allowed() {
        let prev = hk("Alt + V", "Alt + Shift + V", "", "");
        let next = hk("Alt + Shift + V", "Alt + V", "", "");
        let (want, errs) = desired(&prev, &next);
        assert_eq!(errs, [None; 4]);
        assert_eq!(want, [Some(c("Alt + Shift + V")), Some(c("Alt + V")), None, None]);
    }

    /// A fake OS: `external` chords belong to other apps; our own registrations are unique
    /// system-wide like real hotkeys.
    struct FakeOs {
        external: HashSet<Combo>,
        ours: Vec<(usize, Combo)>,
    }

    impl FakeOs {
        fn run(&mut self, live: Slots<Option<Combo>>, prev: Slots<Option<Combo>>, want: Slots<Option<Combo>>) -> (Slots<Option<Combo>>, Slots<Option<SettingsErrorCode>>) {
            for (i, c) in live.iter().enumerate() {
                if let Some(c) = c {
                    self.ours.push((i, *c));
                }
            }
            let ours = std::cell::RefCell::new(std::mem::take(&mut self.ours));
            let external = self.external.clone();
            let out = reconcile(
                live,
                prev,
                want,
                &mut |i, c| {
                    let mut o = ours.borrow_mut();
                    if external.contains(&c) || o.iter().any(|(_, x)| *x == c) {
                        return Err(SettingsErrorCode::Taken);
                    }
                    o.push((i, c));
                    Ok(())
                },
                &mut |i, c| ours.borrow_mut().retain(|(j, x)| !(*j == i && *x == c)),
            );
            self.ours = ours.into_inner();
            out
        }
    }

    #[test]
    fn reconcile_registers_and_unregisters() {
        let mut os = FakeOs { external: HashSet::new(), ours: Vec::new() };
        let live = [Some(c("Alt + V")), Some(c("Alt + Shift + V")), None, None];
        let want = [Some(c("Ctrl + Space")), None, Some(c("Ctrl + P")), None];
        let (out, errs) = os.run(live, live, want);
        assert_eq!(errs, [None; 4]);
        assert_eq!(out, want);
        assert_eq!(os.ours.len(), 2);
    }

    #[test]
    fn reconcile_taken_keeps_previous_binding() {
        let mut os = FakeOs { external: [c("Ctrl + Space")].into_iter().collect(), ours: Vec::new() };
        let live = [Some(c("Alt + V")), None, None, None];
        let want = [Some(c("Ctrl + Space")), None, None, None];
        let (out, errs) = os.run(live, live, want);
        assert_eq!(errs[0], Some(SettingsErrorCode::Taken));
        assert_eq!(out[0], Some(c("Alt + V")), "previous chord re-registered");
    }

    #[test]
    fn reconcile_swap_works_and_failed_swap_rolls_back_both() {
        let live = [Some(c("Alt + V")), Some(c("Alt + Shift + V")), None, None];
        let swap = [live[1], live[0], None, None];
        let mut os = FakeOs { external: HashSet::new(), ours: Vec::new() };
        let (out, errs) = os.run(live, live, swap);
        assert_eq!(errs, [None; 4]);
        assert_eq!(out, swap);

        // summon → new external-taken chord, stackNext → summon's old chord: summon fails
        // and reverts to Alt+V, so stackNext must revert too.
        let mut os = FakeOs { external: [c("Ctrl + Space")].into_iter().collect(), ours: Vec::new() };
        let want = [Some(c("Ctrl + Space")), Some(c("Alt + V")), None, None];
        let (out, errs) = os.run(live, live, want);
        assert_eq!(errs[0], Some(SettingsErrorCode::Taken));
        assert_eq!(errs[1], Some(SettingsErrorCode::Taken));
        assert_eq!(out, live);
    }

    #[test]
    fn reconcile_retries_a_pending_slot() {
        // Startup failure: nothing live, the chord is still wanted. Once free, it binds.
        let want = [Some(c("Alt + V")), None, None, None];
        let mut os = FakeOs { external: [c("Alt + V")].into_iter().collect(), ours: Vec::new() };
        let (out, errs) = os.run([None; 4], want, want);
        assert_eq!(errs[0], Some(SettingsErrorCode::Taken));
        assert_eq!(out[0], None);
        os.external.clear();
        let (out, errs) = os.run(out, want, want);
        assert_eq!(errs, [None; 4]);
        assert_eq!(out[0], Some(c("Alt + V")));
    }
}
